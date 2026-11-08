//! The two steps of a release that release-plz cannot take.
//!
//! `cargo xtask pin <loader tag>` pins the platform to a loader release and
//! says so in the changelog of every mod players get that has nothing else
//! under Unreleased, so each mod's folder changes and release-plz bumps
//! every mod: a mod built for one loader alpha needs a rebuild for the next.
//!
//! `cargo xtask close --date <YYYY-MM-DD>` runs on the release pull request's
//! branch: it closes each bumped mod's Unreleased section as its new version,
//! sets a bumped mod back to its released version when it has nothing for
//! players or that version's release has no files yet, and writes the pull
//! request's body.

use std::fmt::Write as _;

use crate::changelog::{self, CHANGELOG};
use crate::notes::{self, PLATFORM, Remote};
use crate::{Package, Step, say};

const PIN_USAGE: &str = "usage: cargo xtask pin <loader tag>";
const CLOSE_USAGE: &str =
    "usage: cargo xtask close --date <YYYY-MM-DD> [--base <git ref>] [--body <file>]";

/// Pins every platform crate to the commit of `<loader tag>` and adds or
/// updates the line naming the loader in the changelog of each shipped mod
/// whose released version was built for another commit.
pub(crate) fn pin(args: &[String]) -> Result<bool, String> {
    let [tag] = args else {
        return Err(PIN_USAGE.to_owned());
    };
    let tag = if tag.starts_with('v') {
        tag.clone()
    } else {
        format!("v{tag}")
    };
    let version = &tag[1..];
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
    let root = notes::root();
    let path = root.join("Cargo.toml");
    let manifest =
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pinned = repin(&manifest, &commit).map_err(|e| format!("{}: {e}", path.display()))?;
    if pinned != manifest {
        std::fs::write(&path, pinned).map_err(|e| format!("{}: {e}", path.display()))?;
        say(&format!("pinned the platform to {commit} ({tag})"));
    }
    // Also when the manifest is pinned already: a run whose update failed
    // left the lock on the old commit.
    if !Step::new(&["update", "--workspace"]).run(false) {
        return Ok(false);
    }
    let remote = notes::GitHub::read()?;
    for package in notes::shipped(&notes::release_config(&root)?)? {
        let tag = notes::tag_of(&package);
        let released = remote.released(&tag);
        let built = if released {
            notes::built_on(&tag)?
        } else {
            None
        };
        let line = match loader_line(released, built.as_deref(), &commit, version) {
            LoaderLine::Keep => continue,
            LoaderLine::Drop => None,
            LoaderLine::Name(version) => Some(version),
        };
        let path = package.dir.join(CHANGELOG);
        let changelog =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let updated = changelog::built_for(&changelog, line)
            .map_err(|e| format!("{}: {e}", path.display()))?;
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
    Ok(true)
}

/// The commit `git ls-remote` lists for `tag`: the one an annotated tag
/// points to, else the tag's own.
fn commit_of<'a>(listing: &'a str, tag: &str) -> Option<&'a str> {
    let at = |name: &str| {
        listing
            .lines()
            .filter_map(|line| line.split_once('\t'))
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
        .filter(|(_, spec)| crate::mod_rules::from_the_platform(spec))
        .filter_map(|(_, spec)| spec.get("rev").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .collect())
}

/// What `pin` does to a shipped mod's loader line.
#[derive(Debug, PartialEq, Eq)]
enum LoaderLine<'a> {
    /// Leave the changelog alone.
    Keep,
    /// Drop the line: the release was built on the pinned commit.
    Drop,
    /// Name this loader version.
    Name(&'a str),
}

/// The [`LoaderLine`] of a mod whose version is `released` or not, built on
/// the commit `built`: an untagged version is built on the new pin anyway.
fn loader_line<'a>(
    released: bool,
    built: Option<&str>,
    commit: &str,
    version: &'a str,
) -> LoaderLine<'a> {
    if !released {
        LoaderLine::Keep
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
pub(crate) fn close(args: &[String]) -> Result<bool, String> {
    let mut date = None;
    let mut base = "origin/main".to_owned();
    let mut body_file = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let value = rest.next().ok_or(CLOSE_USAGE)?;
        match arg.as_str() {
            "--date" => date = Some(value.as_str()),
            "--base" => base.clone_from(value),
            "--body" => body_file = Some(value),
            _ => return Err(CLOSE_USAGE.to_owned()),
        }
    }
    let date = date.filter(|d| is_date(d)).ok_or(CLOSE_USAGE)?;
    let shipped = notes::shipped(&notes::release_config(&notes::root())?)?;
    let base_manifest = |package: &Package| {
        let dir = package.dir.to_string_lossy();
        notes::git(&["-C", &dir, "show", &format!("{base}:./Cargo.toml")])
    };
    let closing = close_all(&shipped, date, &notes::GitHub::read()?, &base_manifest);
    if closing.kept && !Step::new(&["update", "--workspace"]).run(false) {
        return Ok(false);
    }
    if let Some(file) = body_file {
        std::fs::write(file, closing.body).map_err(|e| format!("{file}: {e}"))?;
    }
    Ok(closing.ok)
}

/// What `close` did to the shipped mods.
#[derive(Debug)]
struct Closing {
    /// The release pull request's body.
    body: String,
    /// Whether a mod was set back, so the lock needs updating.
    kept: bool,
    /// Whether every bumped mod was closed or kept; a mod that was not is
    /// named on stderr and the others go on.
    ok: bool,
}

/// Closes or keeps each mod of `shipped` whose version `remote` has no
/// tag for, reading its manifest on the base with `base_manifest`.
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
    for package in shipped {
        if remote.released(&notes::tag_of(package)) {
            continue;
        }
        let previous =
            base_manifest(package).and_then(|manifest| previous(package, &manifest, remote));
        let done = previous
            .as_ref()
            .map_err(String::clone)
            .and_then(|previous| close_one(package, date, previous.as_ref()));
        match done {
            Ok(Done::Closed) => {
                let _ = writeln!(
                    closing.body,
                    "- Released `{}` {}, its changelog closed as that version.",
                    package.name, package.version
                );
            }
            Ok(Done::Kept(released, why)) => {
                let _ = writeln!(
                    closing.body,
                    "- Kept `{}` at {released}: {why}.",
                    package.name
                );
                closing.kept = true;
            }
            Err(error) => {
                say(&format!("{} {}: {error}", package.name, package.version));
                closing.ok = false;
            }
        }
    }
    closing
}

/// What `close` did with one bumped mod.
#[derive(Debug, PartialEq, Eq)]
enum Done<'a> {
    /// Closed its changelog as the new version.
    Closed,
    /// Set it back to this released version, for this reason.
    Kept(&'a str, &'static str),
}

/// Closes the bumped package's changelog or sets it back to `previous`,
/// as [`plan`] says.
fn close_one<'a>(
    package: &Package,
    date: &str,
    previous: Option<&'a Released>,
) -> Result<Done<'a>, String> {
    let path = package.dir.join(CHANGELOG);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    match plan(previous, &text, &package.version) {
        Plan::Close => {
            if let Some(closed) = changelog::close(&text, &package.version, date)? {
                std::fs::write(&path, closed).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            Ok(Done::Closed)
        }
        Plan::Kept(released, why) => {
            set_back(package, released)?;
            Ok(Done::Kept(released, why))
        }
        Plan::Refuse(why) => Err(why),
    }
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
    let Some(version) = base_manifest.parse::<toml::Table>().ok().and_then(|m| {
        m.get("package")?
            .get("version")?
            .as_str()
            .map(str::to_owned)
    }) else {
        return Ok(None);
    };
    let tag = format!("{}-v{version}", package.name);
    if version == package.version || !remote.released(&tag) {
        return Ok(None);
    }
    let finished = remote.finished(&tag, &package.name)?;
    Ok(Some(Released { version, finished }))
}

/// What `close` does with a bumped mod.
#[derive(Debug, PartialEq, Eq)]
enum Plan<'a> {
    /// Close its changelog as the new version.
    Close,
    /// Set it back to this released version, for this reason.
    Kept(&'a str, &'static str),
    /// Leave it bumped and fail the run, for this reason.
    Refuse(String),
}

/// The [`Plan`] for a mod bumped to `version` past its `previous` release,
/// from its `changelog`. A version tagged without its files is kept until
/// `release` finishes it: past the bump no run would take that tag again.
fn plan<'a>(previous: Option<&'a Released>, changelog: &str, version: &str) -> Plan<'a> {
    let changes = changelog::section(changelog, changelog::UNRELEASED).is_some();
    let closed = changelog::section(changelog, version).is_some();
    match previous {
        Some(Released {
            version: released,
            finished: false,
        }) if closed => Plan::Refuse(format!(
            "closed already, but {released} has no finished release; move its section back under Unreleased so the mod is kept at {released}, which the next `release` finishes"
        )),
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
        None => Plan::Refuse("nothing under Unreleased and no released version to keep".to_owned()),
    }
}

/// Sets the package's manifest back to the `released` version.
fn set_back(package: &Package, released: &str) -> Result<(), String> {
    let path = package.dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let kept = set_version(&text, &package.version, released)
        .ok_or_else(|| format!("{}: no `version = \"{}\"`", path.display(), package.version))?;
    std::fs::write(&path, kept).map_err(|e| format!("{}: {e}", path.display()))?;
    say(&format!("{}: kept at {released}", package.name));
    Ok(())
}

/// Whether `text` is a date as `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn pin_leaves_an_untagged_version_alone_and_drops_the_line_of_a_release_on_the_pin() {
        let line = |released, built| loader_line(released, built, NEW, "1.0.0-alpha.14");
        assert_eq!(line(false, None), LoaderLine::Keep);
        assert_eq!(line(true, Some(NEW)), LoaderLine::Drop);
        assert_eq!(line(true, Some(OLD)), LoaderLine::Name("1.0.0-alpha.14"));
        assert_eq!(line(true, None), LoaderLine::Name("1.0.0-alpha.14"));
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
            Plan::Kept("0.1.1", why) if why.contains("no files")
        ));
        assert!(matches!(
            plan(Some(&unfinished), NOTHING, "0.1.2"),
            Plan::Kept("0.1.1", _)
        ));
        assert!(matches!(
            plan(Some(&unfinished), CLOSED, "0.1.2"),
            Plan::Refuse(why) if why.contains("0.1.1")
        ));
    }

    #[test]
    fn a_bump_past_a_finished_release_closes_unless_nothing_is_new() {
        let finished = released(true);
        assert_eq!(plan(Some(&finished), CHANGES, "0.1.2"), Plan::Close);
        assert_eq!(plan(Some(&finished), CLOSED, "0.1.2"), Plan::Close);
        assert!(matches!(
            plan(Some(&finished), NOTHING, "0.1.2"),
            Plan::Kept("0.1.1", why) if why.contains("nothing")
        ));
        assert_eq!(plan(None, CHANGES, "0.1.2"), Plan::Close);
        assert!(matches!(plan(None, NOTHING, "0.1.2"), Plan::Refuse(_)));
    }

    #[test]
    fn an_empty_heading_of_the_new_version_does_not_count_as_closed() {
        let stray = "## [Unreleased]\n\n- A.\n\n## [0.1.2]\n\n## [0.1.1]\n\n- Old.\n";
        assert!(matches!(
            plan(Some(&released(false)), stray, "0.1.2"),
            Plan::Kept("0.1.1", _)
        ));
    }

    /// A remote with these tags, these finished, and a `gh` failing on
    /// these.
    struct Fake {
        tags: &'static [&'static str],
        finished: &'static [&'static str],
        failing: &'static [&'static str],
    }

    impl Remote for Fake {
        fn released(&self, tag: &str) -> bool {
            self.tags.contains(&tag)
        }

        fn finished(&self, tag: &str, _: &str) -> Result<bool, String> {
            if self.failing.contains(&tag) {
                return Err(format!("gh release view {tag}: HTTP 502"));
            }
            Ok(self.finished.contains(&tag))
        }
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
        std::fs::write(dir.join("Cargo.toml"), manifest(name, version)).unwrap();
        std::fs::write(dir.join(CHANGELOG), changelog).unwrap();
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
        let package = mod_at("previous", "m", "0.1.2", CHANGES);
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

    #[test]
    fn close_closes_keeps_and_skips_each_mod_and_one_failure_stops_none_of_the_others() {
        let test = "close";
        let closes = mod_at(test, "closes", "0.1.2", CHANGES);
        let kept = mod_at(test, "kept", "0.1.2", CHANGES);
        let idle = mod_at(test, "idle", "0.1.2", NOTHING);
        let failing = mod_at(test, "failing", "0.1.2", CHANGES);
        let tagged = mod_at(test, "tagged", "0.1.2", CHANGES);
        let remote = Fake {
            tags: &[
                "closes-v0.1.1",
                "kept-v0.1.1",
                "idle-v0.1.1",
                "failing-v0.1.1",
                "tagged-v0.1.2",
            ],
            finished: &["closes-v0.1.1", "idle-v0.1.1"],
            failing: &["failing-v0.1.1"],
        };
        let shipped = [failing, closes, kept, idle, tagged];
        let closing = close_all(&shipped, "2026-11-09", &remote, &|package| {
            Ok(manifest(&package.name, "0.1.1"))
        });
        let [failing, closes, kept, idle, tagged] = &shipped;
        assert!(!closing.ok);
        assert!(closing.kept);
        assert_eq!(
            closing.body,
            "## Chores\n\n\
             - Released `closes` 0.1.2, its changelog closed as that version.\n\
             - Kept `kept` at 0.1.1: its release has no files yet; `release` finishes it first.\n\
             - Kept `idle` at 0.1.1: nothing under Unreleased for players.\n"
        );
        assert!(file(closes, CHANGELOG).contains("## [0.1.2] - 2026-11-09\n\n- A."));
        assert_eq!(file(closes, "Cargo.toml"), manifest("closes", "0.1.2"));
        assert_eq!(file(kept, CHANGELOG), CHANGES);
        assert_eq!(file(kept, "Cargo.toml"), manifest("kept", "0.1.1"));
        assert_eq!(file(idle, "Cargo.toml"), manifest("idle", "0.1.1"));
        for untouched in [failing, tagged] {
            assert_eq!(file(untouched, CHANGELOG), CHANGES);
            assert_eq!(
                file(untouched, "Cargo.toml"),
                manifest(&untouched.name, "0.1.2")
            );
        }
        std::fs::remove_dir_all(closes.dir.parent().unwrap()).unwrap();
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
        let closing = close_all(&shipped, "2026-11-09", &remote, &|package| {
            Ok(manifest(&package.name, "0.1.1"))
        });
        assert!(!closing.ok);
        assert!(!closing.kept);
        assert_eq!(closing.body, "## Chores\n\n");
        assert_eq!(file(&shipped[0], CHANGELOG), CLOSED);
        std::fs::remove_dir_all(shipped[0].dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn setting_a_mod_back_rewrites_its_version_and_refuses_another_one() {
        let package = mod_at("set-back", "m", "0.1.2", CHANGES);
        set_back(&package, "0.1.1").unwrap();
        assert_eq!(file(&package, "Cargo.toml"), manifest("m", "0.1.1"));
        assert!(set_back(&package, "0.1.0").unwrap_err().contains("0.1.2"));
        std::fs::remove_dir_all(package.dir.parent().unwrap()).unwrap();
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
