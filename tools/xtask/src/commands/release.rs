//! The steps of a release that release-plz cannot take.
//!
//! `cargo xtask built-for <loader tag>` names the loader in the root
//! manifest's `[workspace.metadata.release]`, as the loader the next release
//! builds on, and says in the changelog of each mod in the release set
//! (`release-plz.toml`) whose released version was built for another
//! loader, and that has nothing else under Unreleased, which loader it is
//! built for now, so its folder changes and release-plz bumps it: a mod
//! built for one loader alpha needs a rebuild for the next. A mod outside
//! the set keeps its version and changelog.
//!
//! `cargo xtask pin <loader tag>` pins the platform to a loader release for
//! every crate, so every mod builds on it. The `release` workflow pins in
//! its own job, for the commit it tags; `main` follows the platform's `main`.
//!
//! `cargo xtask close --date <YYYY-MM-DD>` runs on the release pull request's
//! branch: it closes each bumped mod's Unreleased section as its new version,
//! sets a bumped mod back to its released version when it has nothing for
//! players or that version's release has no files yet, and writes the pull
//! request's body.

use std::path::PathBuf;

pub(crate) mod built_for;

use cargo_enshwick::changelog::{self, CHANGELOG, is_date};

use self::built_for::built_for;
use crate::commands::check::Step;
use crate::commands::notes::{self, MANIFEST, PLATFORM, Remote};
use crate::{Package, say};

/// The command line `close` takes.
#[derive(Debug, clap::Args)]
pub(crate) struct CloseArgs {
    /// The release date the sections carry, `YYYY-MM-DD`.
    #[arg(long, value_parser = date)]
    date: String,
    /// The git ref whose manifests hold the versions released last.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// The file to write the release pull request's body into.
    #[arg(long)]
    body: Option<PathBuf>,
}

fn date(text: &str) -> Result<String, String> {
    if is_date(text) {
        Ok(text.to_owned())
    } else {
        Err(format!("{text:?} is no date as YYYY-MM-DD"))
    }
}

/// A loader release: its tag and the platform commit the tag names.
pub(crate) struct Loader {
    pub(crate) tag: String,
    pub(crate) commit: String,
}

/// The table of the root manifest that names the loader of the next release.
const RELEASE_TABLE: &str = "[workspace.metadata.release]";

impl Loader {
    /// The loader the release set is built for, as the root `manifest`'s
    /// `[workspace.metadata.release]` names it.
    pub(crate) fn of_release(manifest: &str) -> Result<Self, String> {
        Self::read(&release_loader(manifest)?)
    }

    /// The loader release tagged `tag`, with or without its `v`, read from
    /// the platform's remote.
    fn read(tag: &str) -> Result<Self, String> {
        let tag = Self::tag(tag)?;
        let reference = format!("refs/tags/{tag}");
        let listing = notes::git(&[
            "ls-remote",
            PLATFORM,
            &reference,
            &format!("{reference}^{{}}"),
        ])?;
        let commit = commit_of(&listing, &tag)
            .ok_or_else(|| format!("{PLATFORM} has no tag {tag}"))?
            .to_owned();
        Ok(Self { tag, commit })
    }

    /// `text` as a loader tag `v<version>`; refused unless it is a version,
    /// since a workflow input reaches it.
    fn tag(text: &str) -> Result<String, String> {
        let tag = if text.starts_with('v') {
            text.to_owned()
        } else {
            format!("v{text}")
        };
        if is_version(&tag[1..]) {
            Ok(tag)
        } else {
            Err(format!("{text:?} is no loader tag such as v1.0.0-alpha.15"))
        }
    }

    /// The loader's version, its tag without the `v`.
    fn version(&self) -> &str {
        &self.tag[1..]
    }
}

/// Whether `text` is a SemVer version without build metadata: three numbers
/// without leading zeros, then optional dot-separated identifiers of ASCII
/// letters, digits and hyphens.
fn is_version(text: &str) -> bool {
    let (core, pre) = text
        .split_once('-')
        .map_or((text, None), |(core, pre)| (core, Some(pre)));
    let number = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    let identifier = |part: &str| {
        !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    core.split('.').count() == 3
        && core.split('.').all(number)
        && pre.is_none_or(|pre| pre.split('.').all(identifier))
}

/// The loader tag the root `manifest`'s `[workspace.metadata.release]` names.
fn release_loader(manifest: &str) -> Result<String, String> {
    let table: toml::Table = toml::from_str(manifest).map_err(|e| e.to_string())?;
    ["workspace", "metadata", "release", "loader"]
        .iter()
        .try_fold(&toml::Value::Table(table), |value, key| value.get(key))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("{MANIFEST}: no `loader` in {RELEASE_TABLE}; run release-pr for the loader the mods are built for")
        })
}

/// The root `manifest` with `[workspace.metadata.release]` naming `tag`, its
/// layout kept.
fn with_release_loader(manifest: &str, tag: &str) -> Result<String, String> {
    let old = release_loader(manifest)?;
    let named = manifest.replacen(
        &format!("loader = \"{old}\""),
        &format!("loader = \"{tag}\""),
        1,
    );
    if release_loader(&named)? != tag {
        return Err(format!(
            "{MANIFEST}: the `loader` of {RELEASE_TABLE} is not written as `loader = \"<tag>\"`; set it by hand"
        ));
    }
    Ok(named)
}

/// Pins every platform crate to the commit of `<loader tag>`, in the root
/// manifest and the lock.
pub(crate) fn pin(tag: &str) -> Result<bool, String> {
    let loader = Loader::read(tag)?;
    let path = notes::root().join(MANIFEST);
    let manifest =
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pinned =
        repin(&manifest, &loader.commit).map_err(|e| format!("{}: {e}", path.display()))?;
    if pinned != manifest {
        std::fs::write(&path, pinned).map_err(|e| format!("{}: {e}", path.display()))?;
        say(&format!(
            "pinned the platform to {} ({})",
            loader.commit, loader.tag
        ));
    }
    // Also when the manifest is pinned already: a run whose update failed
    // left the lock on the old commit.
    Ok(Step::new(&["update", "--workspace"]).run(false))
}

/// Names `<loader tag>` in the root manifest's `[workspace.metadata.release]`
/// and adds or updates the line naming it in the changelog of each shipped
/// mod whose released version was built for another commit, and drops it
/// where the release was built on that loader; refused while a closed
/// version without its tag names another loader, which `release` builds
/// first.
pub(crate) fn built_for_loader(tag: &str) -> Result<bool, String> {
    let loader = Loader::read(tag)?;
    let (commit, version) = (loader.commit.as_str(), loader.version());
    let root = notes::root();
    let manifest_path = root.join(MANIFEST);
    let manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let remote = notes::GitHub::read()?;
    let mut ok = true;
    for package in notes::shipped(&notes::release_config(&root)?)? {
        let tag = notes::tag_of(&package);
        let released = remote.released(&tag);
        let built = if released {
            notes::built_on(&tag)?
        } else {
            None
        };
        let path = package.dir.join(CHANGELOG);
        let changelog =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let closed = changelog::closed(&changelog, &package.version)
            .ok()
            .flatten()
            .and_then(built_for::named);
        let line = match loader_line(released, built.as_deref(), closed, commit, version) {
            LoaderLine::Keep => continue,
            LoaderLine::Drop => None,
            LoaderLine::Name(version) => Some(version),
            LoaderLine::Refuse(named) => {
                say(&format!(
                    "{} {} is closed for Enshwick {named} and not released yet; run release with v{named} first",
                    package.name, package.version
                ));
                ok = false;
                continue;
            }
        };
        let updated =
            built_for(&changelog, line).map_err(|e| format!("{}: {e}", path.display()))?;
        if updated != changelog {
            std::fs::write(&path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
            say(&format!(
                "{}: {}",
                package.name,
                line.map_or("dropped the loader line".to_owned(), |v| format!(
                    "built for Enshwick {v}"
                ))
            ));
        }
    }
    if ok {
        let named = with_release_loader(&manifest, &loader.tag)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        std::fs::write(&manifest_path, named)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    }
    Ok(ok)
}

/// The commit `git ls-remote` lists for `tag`: the one an annotated tag
/// points to, else the tag's own.
fn commit_of<'a>(listing: &'a str, tag: &str) -> Option<&'a str> {
    let at = |name: &str| {
        notes::refs(listing)
            .find(|(_, reference)| *reference == name)
            .map(|(sha, _)| sha)
    };
    at(&format!("refs/tags/{tag}^{{}}")).or_else(|| at(&format!("refs/tags/{tag}")))
}

/// The root `manifest` with every platform crate of
/// `[workspace.dependencies]` pinned to `commit`, its layout kept.
fn repin(manifest: &str, commit: &str) -> Result<String, String> {
    let revs = platform_revs(manifest)?;
    if revs.is_empty() {
        return Err("no platform crate pinned by `rev` in [workspace.dependencies]".to_owned());
    }
    let pinned = revs.iter().fold(manifest.to_owned(), |text, rev| {
        text.replace(&format!("rev = \"{rev}\""), &format!("rev = \"{commit}\""))
    });
    if platform_revs(&pinned)?.iter().any(|rev| rev != commit) {
        return Err(
            "a platform `rev` is not written as `rev = \"<commit>\"`; pin it by hand".to_owned(),
        );
    }
    Ok(pinned)
}

/// The `rev` of each platform crate in the `manifest`'s
/// `[workspace.dependencies]`.
fn platform_revs(manifest: &str) -> Result<Vec<String>, String> {
    let table: toml::Table = toml::from_str(manifest).map_err(|e| e.to_string())?;
    Ok(table
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter(|(_, spec)| crate::check::mod_rules::from_the_platform(spec))
        .filter_map(|(_, spec)| spec.get("rev").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .collect())
}

/// What `built-for` does to a shipped mod's loader line.
#[derive(Debug, PartialEq, Eq)]
enum LoaderLine<'a> {
    /// Leave the changelog alone.
    Keep,
    /// Drop the line: the release was built on the loader's commit.
    Drop,
    /// Name this loader version.
    Name(&'a str),
    /// Refuse the loader: the version, closed but untagged, names this other
    /// one.
    Refuse(&'a str),
}

/// The [`LoaderLine`] of a mod whose version is `released` or not, built on
/// the commit `built`, its closed section naming the loader `closed`: an
/// untagged version builds on the new loader anyway, unless its section
/// names another one.
fn loader_line<'a>(
    released: bool,
    built: Option<&str>,
    closed: Option<&'a str>,
    commit: &str,
    version: &'a str,
) -> LoaderLine<'a> {
    if !released {
        match closed {
            Some(named) if named != version => LoaderLine::Refuse(named),
            _ => LoaderLine::Keep,
        }
    } else if built == Some(commit) {
        LoaderLine::Drop
    } else {
        LoaderLine::Name(version)
    }
}

/// The manifest with its `[package]` version `from` set to `to`.
fn set_version(manifest: &str, from: &str, to: &str) -> Option<String> {
    let line = format!("version = \"{from}\"\n");
    let at = manifest.find(&line)?;
    Some(format!(
        "{}version = \"{to}\"\n{}",
        &manifest[..at],
        &manifest[at + line.len()..]
    ))
}

/// Closes the changelog of each shipped mod whose version has no tag yet,
/// sets a bumped mod back to its version on `--base` (`origin/main`) when
/// that one is released and has nothing new under Unreleased or no finished
/// release yet, and writes the release pull request's body to `--body`.
pub(crate) fn close(args: &CloseArgs) -> Result<bool, String> {
    let (date, base, body_file) = (args.date.as_str(), &args.base, args.body.as_ref());
    let shipped = notes::shipped(&notes::release_config(&notes::root())?)?;
    let base_manifest = |package: &Package| {
        let dir = package.dir.to_string_lossy();
        notes::git(&["-C", &dir, "show", &format!("{base}:./{MANIFEST}")])
    };
    let closing = close_all(&shipped, date, &notes::GitHub::read()?, &base_manifest);
    // A failed mod fails the run: nothing of it may reach the pull request.
    if !closing.ok {
        return Ok(false);
    }
    if closing.kept && !Step::new(&["update", "--workspace"]).run(false) {
        return Ok(false);
    }
    if let Some(file) = body_file {
        std::fs::write(file, closing.body).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(true)
}

/// What `close` did to the shipped mods.
#[derive(Debug)]
struct Closing {
    /// The release pull request's body.
    body: String,
    /// Whether a mod was set back, so the lock needs updating.
    kept: bool,
    /// Whether every bumped mod was closed or kept; a mod that was not is
    /// named on stderr and the others are still planned.
    ok: bool,
}

/// Closes or keeps each mod of `shipped` whose version `remote` has no
/// tag for, reading its manifest on the base with `base_manifest`. Files
/// are written only once every mod has its plan, so a failed run leaves
/// the tree as it was and a rerun writes the same body as one clean run.
fn close_all(
    shipped: &[Package],
    date: &str,
    remote: &impl Remote,
    base_manifest: &dyn Fn(&Package) -> Result<String, String>,
) -> Closing {
    let mut closing = Closing {
        body: String::from("## Chores\n\n"),
        kept: false,
        ok: true,
    };
    let mut writes = Vec::new();
    for package in shipped {
        if remote.released(&notes::tag_of(package)) {
            continue;
        }
        let line = base_manifest(package)
            .and_then(|manifest| previous(package, &manifest, remote))
            .and_then(|previous| {
                let (plan, write) = close_one(package, date, previous.as_ref())?;
                writes.extend(write);
                Ok(match plan {
                    Plan::Close => format!(
                        "- Released `{}` {}, its changelog closed as that version.\n",
                        package.name, package.version
                    ),
                    Plan::Kept(released, why) => {
                        closing.kept = true;
                        format!("- Kept `{}` at {released}: {why}.\n", package.name)
                    }
                })
            });
        match line {
            Ok(line) => closing.body.push_str(&line),
            Err(error) => {
                say(&format!("{} {}: {error}", package.name, package.version));
                closing.ok = false;
            }
        }
    }
    if closing.ok {
        for (path, text) in writes {
            if let Err(error) = std::fs::write(&path, text) {
                say(&format!("{}: {error}", path.display()));
                closing.ok = false;
                break;
            }
        }
    }
    closing
}

/// The [`plan`] for the bumped package and the file it changes: its
/// changelog closed, or its manifest set back to `previous`.
fn close_one<'a>(
    package: &Package,
    date: &str,
    previous: Option<&'a Released>,
) -> Result<(Plan<'a>, Option<(PathBuf, String)>), String> {
    let path = package.dir.join(CHANGELOG);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let plan = plan(previous, &text, &package.version)?;
    let write = match plan {
        Plan::Close => changelog::close(&text, &package.version, date)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .map(|closed| (path, closed)),
        Plan::Kept(released, _) => Some(set_back(package, released)?),
    };
    Ok((plan, write))
}

/// A package's released version on the base.
#[derive(Debug, PartialEq, Eq)]
struct Released {
    version: String,
    /// Whether its GitHub release has every file.
    finished: bool,
}

/// The package's version in its `base_manifest`, when that is another
/// version and `remote` has its tag.
fn previous(
    package: &Package,
    base_manifest: &str,
    remote: &impl Remote,
) -> Result<Option<Released>, String> {
    let Some(version) = version_in(base_manifest) else {
        return Ok(None);
    };
    let tag = notes::tag(&package.name, &version);
    if version == package.version || !remote.released(&tag) {
        return Ok(None);
    }
    let finished = remote.finished(&tag, &package.name)?;
    Ok(Some(Released { version, finished }))
}

/// The `[package]` version in `manifest`.
fn version_in(manifest: &str) -> Option<String> {
    manifest
        .parse::<toml::Table>()
        .ok()?
        .get("package")?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// What `close` does with a bumped mod.
#[derive(Debug, PartialEq, Eq)]
enum Plan<'a> {
    /// Close its changelog as the new version.
    Close,
    /// Set it back to this released version, for this reason.
    Kept(&'a str, &'static str),
}

/// The [`Plan`] for a mod bumped to `version` past its `previous` release,
/// from its `changelog`. A version tagged without its files is kept until
/// `release` finishes it: past the bump no run would take that tag again.
///
/// # Errors
///
/// Why the mod can be neither closed nor kept; it stays bumped and the run
/// fails.
fn plan<'a>(
    previous: Option<&'a Released>,
    changelog: &str,
    version: &str,
) -> Result<Plan<'a>, String> {
    let changes = changelog::section(changelog, changelog::UNRELEASED).is_some();
    let closed = changelog::section(changelog, version).is_some();
    Ok(match previous {
        Some(Released {
            version: released,
            finished: false,
        }) if closed => {
            return Err(format!(
                "closed already, but {released} has no finished release; move its section back under Unreleased so the mod is kept at {released}, which the next `release` finishes"
            ));
        }
        Some(Released {
            version: released,
            finished: false,
        }) => Plan::Kept(
            released,
            "its release has no files yet; `release` finishes it first",
        ),
        _ if changes || closed => Plan::Close,
        Some(Released {
            version: released,
            finished: true,
        }) => Plan::Kept(released, "nothing under Unreleased for players"),
        None => return Err("nothing under Unreleased and no released version to keep".to_owned()),
    })
}

/// The package's manifest and its text set back to the `released` version.
fn set_back(package: &Package, released: &str) -> Result<(PathBuf, String), String> {
    let path = package.dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let kept = set_version(&text, &package.version, released)
        .ok_or_else(|| format!("{}: no `version = \"{}\"`", path.display(), package.version))?;
    Ok((path, kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::commands::notes::Fake;

    const OLD: &str = "1111111111111111111111111111111111111111";
    const NEW: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn an_annotated_tag_names_the_commit_it_points_to() {
        let listing =
            format!("{OLD}\trefs/tags/v1.0.0-alpha.14\n{NEW}\trefs/tags/v1.0.0-alpha.14^{{}}\n");
        assert_eq!(commit_of(&listing, "v1.0.0-alpha.14"), Some(NEW));
        let light = format!("{OLD}\trefs/tags/v1.0.0-alpha.14\n");
        assert_eq!(commit_of(&light, "v1.0.0-alpha.14"), Some(OLD));
        assert_eq!(commit_of(&light, "v1.0.0-alpha.1"), None);
    }

    #[test]
    fn a_loader_tag_is_a_version_with_or_without_its_v() {
        assert_eq!(Loader::tag("1.0.0-alpha.15").unwrap(), "v1.0.0-alpha.15");
        assert_eq!(Loader::tag("v1.0.0").unwrap(), "v1.0.0");
        assert_eq!(Loader::tag("v0.10.0-rc-1.2").unwrap(), "v0.10.0-rc-1.2");
        for text in [
            "",
            "main",
            "v1.0",
            "--upload-pack=x",
            "v1.0.0-",
            "v1.0.0-a b/../x",
            "v1.0.0-alpha..15",
            "v01.0.0",
            "v1.0.0+build",
        ] {
            assert!(Loader::tag(text).is_err(), "{text}");
        }
    }

    #[test]
    fn repinning_moves_every_platform_crate_and_nothing_else() {
        let manifest = format!(
            "[workspace.dependencies]\n\
             # The platform.\n\
             enshwick = {{ git = \"{PLATFORM}\", rev = \"{OLD}\" }}\n\
             enshwick-luau = {{ git = \"{PLATFORM}\", rev = \"{OLD}\" }}\n\
             other = {{ git = \"https://example.com/other\", rev = \"{NEW}\" }}\n"
        );
        let pinned = repin(&manifest, "3".repeat(40).as_str()).unwrap();
        assert_eq!(pinned, manifest.replace(OLD, &"3".repeat(40)));
        assert!(repin("[workspace]\n", NEW).is_err());
    }

    #[test]
    fn a_rev_spelt_another_way_is_refused_rather_than_left_on_the_old_commit() {
        let manifest = format!(
            "[workspace.dependencies]\nenshwick = {{ git = \"{PLATFORM}\", rev=\"{OLD}\" }}\n"
        );
        assert!(repin(&manifest, NEW).is_err());
    }

    #[test]
    fn built_for_leaves_an_untagged_version_alone_and_drops_the_line_of_a_release_on_the_loader() {
        let line = |released, built| loader_line(released, built, None, NEW, "1.0.0-alpha.14");
        assert_eq!(line(false, None), LoaderLine::Keep);
        assert_eq!(line(true, Some(NEW)), LoaderLine::Drop);
        assert_eq!(line(true, Some(OLD)), LoaderLine::Name("1.0.0-alpha.14"));
        assert_eq!(line(true, None), LoaderLine::Name("1.0.0-alpha.14"));
    }

    #[test]
    fn built_for_refuses_another_loader_while_an_untagged_version_is_closed_for_one() {
        let line = |closed| loader_line(false, None, closed, NEW, "1.0.0-alpha.16");
        assert_eq!(
            line(Some("1.0.0-alpha.15")),
            LoaderLine::Refuse("1.0.0-alpha.15")
        );
        assert_eq!(line(Some("1.0.0-alpha.16")), LoaderLine::Keep);
    }

    #[test]
    fn the_release_loader_is_read_and_renamed_in_place() {
        let manifest = "[workspace]\n\n# Why.\n[workspace.metadata.release]\nloader = \"v1.0.0-alpha.15\"\n\n[workspace.dependencies]\nx = \"1\"\n";
        assert_eq!(release_loader(manifest).unwrap(), "v1.0.0-alpha.15");
        let named = with_release_loader(manifest, "v1.0.0-alpha.16").unwrap();
        assert_eq!(named, manifest.replace("alpha.15", "alpha.16"));
        assert!(release_loader("[workspace]\n").is_err());
        let spelt = manifest.replace("loader = ", "loader= ");
        assert!(with_release_loader(&spelt, "v1.0.0-alpha.16").is_err());
    }

    /// A changelog with a change under Unreleased.
    const CHANGES: &str = "## [Unreleased]\n\n- A.\n\n## [0.1.1]\n\n- Old.\n";
    /// A changelog with nothing under Unreleased.
    const NOTHING: &str = "## [Unreleased]\n\n## [0.1.1]\n\n- Old.\n";
    /// A changelog closed as 0.1.2 already.
    const CLOSED: &str = "## [Unreleased]\n\n## [0.1.2]\n\n- A.\n\n## [0.1.1]\n\n- Old.\n";

    fn released(finished: bool) -> Released {
        Released {
            version: "0.1.1".to_owned(),
            finished,
        }
    }

    #[test]
    fn a_bump_past_a_release_without_its_files_is_kept_until_the_release_is_finished() {
        let unfinished = released(false);
        assert!(matches!(
            plan(Some(&unfinished), CHANGES, "0.1.2"),
            Ok(Plan::Kept("0.1.1", why)) if why.contains("no files")
        ));
        assert!(matches!(
            plan(Some(&unfinished), NOTHING, "0.1.2"),
            Ok(Plan::Kept("0.1.1", _))
        ));
        assert!(matches!(
            plan(Some(&unfinished), CLOSED, "0.1.2"),
            Err(why) if why.contains("0.1.1")
        ));
    }

    #[test]
    fn a_bump_past_a_finished_release_closes_unless_nothing_is_new() {
        let finished = released(true);
        assert_eq!(plan(Some(&finished), CHANGES, "0.1.2"), Ok(Plan::Close));
        assert_eq!(plan(Some(&finished), CLOSED, "0.1.2"), Ok(Plan::Close));
        assert!(matches!(
            plan(Some(&finished), NOTHING, "0.1.2"),
            Ok(Plan::Kept("0.1.1", why)) if why.contains("nothing")
        ));
        assert_eq!(plan(None, CHANGES, "0.1.2"), Ok(Plan::Close));
        assert!(plan(None, NOTHING, "0.1.2").is_err());
    }

    #[test]
    fn an_empty_heading_of_the_new_version_does_not_count_as_closed() {
        let stray = "## [Unreleased]\n\n- A.\n\n## [0.1.2]\n\n## [0.1.1]\n\n- Old.\n";
        assert!(matches!(
            plan(Some(&released(false)), stray, "0.1.2"),
            Ok(Plan::Kept("0.1.1", _))
        ));
    }

    /// A mod `name` at `version` in a fresh folder under the temp folder,
    /// with this changelog.
    fn mod_at(test: &str, name: &str, version: &str, changelog: &str) -> Package {
        let dir = std::env::temp_dir()
            .join(format!("xtask-{test}-{}", std::process::id()))
            .join(name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST), manifest(name, version)).unwrap();
        std::fs::write(dir.join(CHANGELOG), changelog).unwrap();
        package(name, version, dir)
    }

    fn package(name: &str, version: &str, dir: PathBuf) -> Package {
        Package {
            name: name.to_owned(),
            version: version.to_owned(),
            dir,
            id: name.to_owned(),
            windows_only: false,
            cdylib: true,
            lib: true,
        }
    }

    fn manifest(name: &str, version: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n")
    }

    fn file(package: &Package, name: &str) -> String {
        std::fs::read_to_string(package.dir.join(name)).unwrap()
    }

    #[test]
    fn the_previous_version_is_the_base_one_only_when_it_differs_and_is_tagged() {
        let package = package("m", "0.1.2", PathBuf::new());
        let remote = Fake {
            tags: &["m-v0.1.1", "m-v0.1.0"],
            finished: &["m-v0.1.0"],
            failing: &[],
        };
        assert_eq!(
            previous(&package, &manifest("m", "0.1.1"), &remote),
            Ok(Some(released(false)))
        );
        assert_eq!(
            previous(&package, &manifest("m", "0.1.0"), &remote),
            Ok(Some(Released {
                version: "0.1.0".to_owned(),
                finished: true
            }))
        );
        assert_eq!(
            previous(&package, &manifest("m", "0.1.2"), &remote),
            Ok(None)
        );
        assert_eq!(
            previous(&package, &manifest("m", "0.0.9"), &remote),
            Ok(None)
        );
        assert_eq!(previous(&package, "[package]\n", &remote), Ok(None));
        let failing = Fake {
            failing: &["m-v0.1.1"],
            ..remote
        };
        assert!(previous(&package, &manifest("m", "0.1.1"), &failing).is_err());
    }

    /// A manifest reader giving each package `version`, as on one revision.
    fn at(version: &str) -> impl Fn(&Package) -> Result<String, String> {
        move |package| Ok(manifest(&package.name, version))
    }

    #[test]
    fn close_closes_keeps_and_skips_each_mod() {
        let test = "close";
        let closes = mod_at(test, "closes", "0.1.2", CHANGES);
        let kept = mod_at(test, "kept", "0.1.2", CHANGES);
        let idle = mod_at(test, "idle", "0.1.2", NOTHING);
        let tagged = mod_at(test, "tagged", "0.1.2", CHANGES);
        let remote = Fake {
            tags: &[
                "closes-v0.1.1",
                "kept-v0.1.1",
                "idle-v0.1.1",
                "tagged-v0.1.2",
            ],
            finished: &["closes-v0.1.1", "idle-v0.1.1"],
            failing: &[],
        };
        let shipped = [closes, kept, idle, tagged];
        let closing = close_all(&shipped, "2026-11-09", &remote, &at("0.1.1"));
        let [closes, kept, idle, tagged] = &shipped;
        assert!(closing.ok);
        assert!(closing.kept);
        assert_eq!(
            closing.body,
            "## Chores\n\n\
             - Released `closes` 0.1.2, its changelog closed as that version.\n\
             - Kept `kept` at 0.1.1: its release has no files yet; `release` finishes it first.\n\
             - Kept `idle` at 0.1.1: nothing under Unreleased for players.\n"
        );
        assert!(file(closes, CHANGELOG).contains("## [0.1.2] - 2026-11-09\n\n- A."));
        assert_eq!(file(closes, MANIFEST), manifest("closes", "0.1.2"));
        assert_eq!(file(kept, CHANGELOG), CHANGES);
        assert_eq!(file(kept, MANIFEST), manifest("kept", "0.1.1"));
        assert_eq!(file(idle, MANIFEST), manifest("idle", "0.1.1"));
        assert_eq!(file(tagged, CHANGELOG), CHANGES);
        assert_eq!(file(tagged, MANIFEST), manifest("tagged", "0.1.2"));
        std::fs::remove_dir_all(closes.dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_failed_mod_leaves_every_mod_as_it_was_so_a_rerun_gets_the_same_body() {
        let test = "rerun";
        let shipped = [
            mod_at(test, "kept", "0.1.2", CHANGES),
            mod_at(test, "idle", "0.1.2", NOTHING),
            mod_at(test, "closes", "0.1.2", CHANGES),
            mod_at(test, "failing", "0.1.2", CHANGES),
        ];
        let tags = &[
            "kept-v0.1.1",
            "idle-v0.1.1",
            "closes-v0.1.1",
            "failing-v0.1.1",
        ];
        let finished = &["idle-v0.1.1", "closes-v0.1.1", "failing-v0.1.1"];
        let broken = Fake {
            tags,
            finished,
            failing: &["failing-v0.1.1"],
        };
        let first = close_all(&shipped, "2026-11-09", &broken, &at("0.1.1"));
        assert!(!first.ok);
        for package in &shipped {
            assert_eq!(file(package, MANIFEST), manifest(&package.name, "0.1.2"));
        }
        let [kept, idle, closes, failing] = &shipped;
        assert_eq!(file(kept, CHANGELOG), CHANGES);
        assert_eq!(file(idle, CHANGELOG), NOTHING);
        assert_eq!(file(closes, CHANGELOG), CHANGES);
        assert_eq!(file(failing, CHANGELOG), CHANGES);
        let fixed = Fake {
            tags,
            finished,
            failing: &[],
        };
        let second = close_all(&shipped, "2026-11-09", &fixed, &at("0.1.1"));
        assert!(second.ok);
        assert!(second.kept);
        assert!(second.body.contains("- Kept `kept` at 0.1.1"));
        assert!(second.body.contains("- Kept `idle` at 0.1.1"));
        assert!(second.body.contains("- Released `closes` 0.1.2"));
        assert!(second.body.contains("- Released `failing` 0.1.2"));
        assert_eq!(file(kept, MANIFEST), manifest("kept", "0.1.1"));
        assert_eq!(file(idle, MANIFEST), manifest("idle", "0.1.1"));
        assert_eq!(file(closes, CHANGELOG).matches("## [0.1.2]").count(), 1);
        std::fs::remove_dir_all(kept.dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn close_fails_without_touching_a_mod_it_must_refuse() {
        let refused = mod_at("refuse", "refused", "0.1.2", CLOSED);
        let remote = Fake {
            tags: &["refused-v0.1.1"],
            finished: &[],
            failing: &[],
        };
        let shipped = [refused];
        let closing = close_all(&shipped, "2026-11-09", &remote, &at("0.1.1"));
        assert!(!closing.ok);
        assert!(!closing.kept);
        assert_eq!(closing.body, "## Chores\n\n");
        assert_eq!(file(&shipped[0], CHANGELOG), CLOSED);
        std::fs::remove_dir_all(shipped[0].dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn setting_a_mod_back_rewrites_its_version_and_refuses_another_one() {
        let package = mod_at("set-back", "m", "0.1.2", CHANGES);
        assert_eq!(
            set_back(&package, "0.1.1").unwrap(),
            (package.dir.join(MANIFEST), manifest("m", "0.1.1"))
        );
        assert_eq!(file(&package, MANIFEST), manifest("m", "0.1.2"));
        let other = Package {
            version: "0.1.3".to_owned(),
            ..package
        };
        assert!(set_back(&other, "0.1.1").unwrap_err().contains("0.1.3"));
        std::fs::remove_dir_all(other.dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn keeping_a_version_changes_only_the_package_line() {
        let manifest = "[package]\nname = \"m\"\nversion = \"0.1.1\"\n\n[dependencies]\nx = { version = \"0.1.1\" }\n";
        assert_eq!(
            set_version(manifest, "0.1.1", "0.1.0").unwrap(),
            manifest.replacen("version = \"0.1.1\"\n", "version = \"0.1.0\"\n", 1)
        );
        assert_eq!(set_version(manifest, "0.2.0", "0.1.0"), None);
    }

    #[test]
    fn only_a_calendar_shaped_date_closes_a_section() {
        assert!(is_date("2026-11-08"));
        for date in ["2026-11-8", "20261108", "2026/11/08", "2026-11-08T", ""] {
            assert!(!is_date(date), "{date}");
        }
    }
}
