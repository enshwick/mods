//! `cargo xtask notes`: the release notes of a mod, written before it is
//! released: the warning that anything may change before 1.0, the loader
//! alpha the mod builds on, how to install it, and the hand-written section
//! of its version in the mod's `CHANGELOG.md`. A mod is about to be released
//! when its tag `<package>-v<version>` is not on the remote yet; with
//! `--unfinished`, also when its tag has no GitHub release with every file
//! yet, so a release run that stopped partway is finished by the next.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Package, packages, say};

const USAGE: &str =
    "usage: cargo xtask notes (--unreleased | --unfinished | --package <name>...) [--out <dir>]";
/// Where the platform comes from; its tags name the loader's releases.
pub(crate) const PLATFORM: &str = "https://github.com/enshwick/enshwick";
/// The workspace dependency whose commit names the loader a mod builds on.
const API: &str = "enshwick";
pub(crate) const CHANGELOG: &str = "CHANGELOG.md";
/// Which packages release-plz releases.
const RELEASE_CONFIG: &str = "release-plz.toml";
/// Only a mod with a player page ships to players.
pub(crate) const PLAYER_PAGE: &str = "docs/player.md";
/// The list of the mods a run takes, written beside their notes: the
/// `release` workflow's jobs run once per entry.
const RELEASES: &str = "releases.json";
/// GitHub refuses a release body longer than this, in characters.
const MAX_BODY: usize = 125_000;
const WARNING: &str = "\
> [!WARNING]
> This is an alpha. Before 1.0 anything may change, including the API, the
> ABI, file layouts, settings and mod ids. Mods built for one alpha may need a
> rebuild for the next. Back up your saves.
>
> Це альфа-версія. До 1.0 може змінитися будь-що: API, ABI, розташування
> файлів, налаштування та ідентифікатори модів. Моди, зібрані для однієї
> альфа-версії, можуть потребувати перезбирання для наступної. Зробіть
> резервні копії збережень.
";

/// Writes `<package>.md` into `--out` (`target/notes` by default) for each
/// chosen mod, or for every mod not released yet with `--unreleased` or
/// not finished with `--unfinished`, and `releases.json` listing them; the
/// files there are the mods a release takes, so a run that refuses one
/// writes none. With either flag, a released mod built for another loader
/// than the pinned one is refused too: a loader alpha releases every mod.
pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let root = root();
    let mut out = root.join("target").join("notes");
    let mut unreleased = false;
    let mut unfinished = false;
    let mut chosen: Vec<&str> = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--unreleased" => unreleased = true,
            "--unfinished" => unfinished = true,
            "--package" => chosen.push(rest.next().ok_or(USAGE)?),
            "--out" => out = PathBuf::from(rest.next().ok_or(USAGE)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    // Exactly one of the three ways to choose.
    if [unreleased, unfinished, !chosen.is_empty()]
        .iter()
        .filter(|way| **way)
        .count()
        != 1
    {
        return Err(USAGE.to_owned());
    }
    let config = release_config(&root)?;
    let shipped = shipped(&config)?;
    if let Some(missing) = chosen
        .iter()
        .find(|name| !shipped.iter().any(|p| p.name == **name))
    {
        return Err(format!("{missing} is not a mod players get"));
    }
    let rev =
        pinned_rev(&std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string())?)
            .ok_or_else(|| format!("Cargo.toml: no `rev` of {API}"))?;
    let way = if unfinished {
        Way::Unfinished
    } else if unreleased {
        Way::Unreleased
    } else {
        Way::Chosen(&chosen)
    };
    let Some(mods) = choose(shipped, way, &rev)? else {
        return Ok(false);
    };
    if mods.is_empty() {
        say("every mod's version is released already");
        return Ok(true);
    }
    let loader = loader_tag(&rev)?;
    let mut ok = true;
    let mut written = Vec::new();
    for package in &mods {
        let path = package.dir.join(CHANGELOG);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(|changelog| {
                closed(&changelog, &package.version)
                    .map_err(|e| format!("{}: {e}", path.display()))
                    .and_then(|section| notes(&package.name, &loader, section))
            });
        match text {
            Ok(text) => written.push((package, text)),
            Err(error) => {
                say(&error);
                ok = false;
            }
        }
    }
    if !ok {
        return Ok(false);
    }
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (package, text) in &written {
        let file = out.join(format!("{}.md", package.name));
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        say(&format!("wrote {}", file.display()));
    }
    let releases: Vec<serde_json::Value> = written
        .iter()
        .map(|(package, _)| {
            serde_json::json!({
                "tag": tag_of(package),
                "package_name": package.name,
                "version": package.version,
                "title": title(&config, package),
            })
        })
        .collect();
    let file = out.join(RELEASES);
    std::fs::write(&file, serde_json::Value::from(releases).to_string())
        .map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(true)
}

/// How a run chooses the mods it takes.
#[derive(Debug, Clone, Copy)]
enum Way<'a> {
    /// Those whose tag is not on the remote.
    Unreleased,
    /// Those whose tag is not on the remote or has no finished release.
    Unfinished,
    /// The named ones.
    Chosen(&'a [&'a str]),
}

/// The shipped mods `way` takes; `None` when a released mod that is not
/// taken was built for another loader than `rev`, said on stderr.
fn choose(shipped: Vec<Package>, way: Way<'_>, rev: &str) -> Result<Option<Vec<Package>>, String> {
    let mut mods = Vec::new();
    let mut ok = true;
    for package in shipped {
        let tag = tag_of(&package);
        let take = match way {
            Way::Unfinished => !finished(&tag, &package.name)?,
            Way::Unreleased => !released(&tag)?,
            Way::Chosen(chosen) => chosen.contains(&package.name.as_str()),
        };
        if take {
            mods.push(package);
        } else if !matches!(way, Way::Chosen(_)) && built_on(&tag)?.as_deref() != Some(rev) {
            say(&format!(
                "{tag} was built for another loader than the pinned one; run `cargo xtask pin <loader tag>` so release-pr bumps it"
            ));
            ok = false;
        }
    }
    Ok(ok.then_some(mods))
}

/// The workspace root.
pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The tag release-plz gives the package's version.
pub(crate) fn tag_of(package: &Package) -> String {
    format!("{}-v{}", package.name, package.version)
}

/// The packages `release-plz.toml` names, as tables.
pub(crate) fn release_config(root: &Path) -> Result<Vec<toml::Table>, String> {
    let path = root.join(RELEASE_CONFIG);
    std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| entries(&text))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The `[[package]]` tables of a release-plz configuration.
pub(crate) fn entries(config: &str) -> Result<Vec<toml::Table>, String> {
    let config: toml::Table = toml::from_str(config).map_err(|e| e.to_string())?;
    Ok(config
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
        .cloned()
        .collect())
}

/// The mods players get, by [`ships`].
pub(crate) fn shipped(config: &[toml::Table]) -> Result<Vec<Package>, String> {
    let held = held_back(config);
    Ok(packages()?
        .into_iter()
        .filter(|p| ships(p, &held))
        .collect())
}

/// The release's title: the package's `git_release_name` with its version,
/// else its tag, as release-plz names it.
fn title(config: &[toml::Table], package: &Package) -> String {
    config
        .iter()
        .find(|entry| entry.get("name").and_then(toml::Value::as_str) == Some(&package.name))
        .and_then(|entry| entry.get("git_release_name"))
        .and_then(toml::Value::as_str)
        .map_or_else(
            || tag_of(package),
            |name| name.replace("{{ version }}", &package.version),
        )
}

/// The files the `release` workflow attaches to a mod's release.
fn release_files(package: &str) -> [String; 4] {
    [
        format!("{package}.zip"),
        format!("{package}.pdb"),
        format!("{package}.cdx.json"),
        "SHA256SUMS".to_owned(),
    ]
}

/// Whether the tag is on the remote and its GitHub release has every file,
/// read with `gh`.
fn finished(tag: &str, package: &str) -> Result<bool, String> {
    if !released(tag)? {
        return Ok(false);
    }
    let output = Command::new("gh")
        .args([
            "release",
            "view",
            tag,
            "--json",
            "assets",
            "--jq",
            ".assets[].name",
        ])
        .output()
        .map_err(|error| format!("running gh: {error}"))?;
    let error = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return if error.contains("release not found") {
            Ok(false)
        } else {
            Err(format!("gh release view {tag}: {}", error.trim()))
        };
    }
    let names = String::from_utf8_lossy(&output.stdout);
    Ok(has_files(&names, package))
}

/// Whether `gh`'s list of a release's files, one a line, holds every file
/// a release of `package` attaches.
fn has_files(names: &str, package: &str) -> bool {
    release_files(package)
        .iter()
        .all(|file| names.lines().any(|name| name == file))
}

/// The platform commit the root manifest at `tag` pins, fetching the tag
/// first when this clone lacks it; `None` when that manifest pins none.
pub(crate) fn built_on(tag: &str) -> Result<Option<String>, String> {
    let local = format!("refs/tags/{tag}");
    if git(&["rev-parse", "--verify", "--quiet", &local]).is_err() {
        let refspec = format!("{local}:{local}");
        let mut fetch = vec!["fetch", "--no-tags", "origin", refspec.as_str()];
        // A shallow clone stays shallow; a full one must not become so.
        if git(&["rev-parse", "--is-shallow-repository"])?.trim() == "true" {
            fetch.push("--depth=1");
        }
        git(&fetch)?;
    }
    Ok(pinned_rev(&git(&["show", &format!("{local}:Cargo.toml")])?))
}

/// Whether the repository's remote has the tag already.
pub(crate) fn released(tag: &str) -> Result<bool, String> {
    Ok(
        !git(&["ls-remote", "--tags", "origin", &format!("refs/tags/{tag}")])?
            .trim()
            .is_empty(),
    )
}

/// Whether players get the package: a mod with a player page that
/// `release-plz.toml` does not hold back.
fn ships(package: &Package, held: &[String]) -> bool {
    package.cdylib && !held.contains(&package.name) && package.dir.join(PLAYER_PAGE).is_file()
}

/// The packages `release-plz.toml` never releases (`release = false`), so a
/// mod held back there gets no notes and does not stop the others.
pub(crate) fn held_back(config: &[toml::Table]) -> Vec<String> {
    config
        .iter()
        .filter(|package| package.get("release").and_then(toml::Value::as_bool) == Some(false))
        .filter_map(|package| package.get("name").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// The commit a root manifest pins the API to.
pub(crate) fn pinned_rev(manifest: &str) -> Option<String> {
    let manifest: toml::Table = toml::from_str(manifest).ok()?;
    manifest
        .get("workspace")?
        .get("dependencies")?
        .get(API)?
        .get("rev")?
        .as_str()
        .map(str::to_owned)
}

/// The loader release whose commit the workspace pins the API to.
fn loader_tag(rev: &str) -> Result<String, String> {
    let listing = git(&["ls-remote", "--tags", PLATFORM, "refs/tags/v*"])?;
    release_at(&listing, rev).map(str::to_owned).ok_or_else(|| {
        format!(
            "{API} is pinned to {rev}, which no loader release tags; run `cargo xtask pin <loader tag>` with the loader alpha the mods are built for"
        )
    })
}

pub(crate) fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| format!("running git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("git {}: {error}", args.join(" ")))
}

/// The version tag `git ls-remote --tags` lists at commit `rev`, an
/// annotated tag by the commit it points to; the newest when several do.
fn release_at<'a>(listing: &'a str, rev: &str) -> Option<&'a str> {
    listing
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(sha, _)| sha.eq_ignore_ascii_case(rev))
        .filter_map(|(_, name)| name.strip_prefix("refs/tags/"))
        .map(|name| name.strip_suffix("^{}").unwrap_or(name))
        .filter(|tag| version_of(tag).is_some())
        .max_by_key(|tag| version_of(tag))
}

/// Major, minor, patch, whether it is a release, and each prerelease
/// identifier as (alphanumeric, number, text).
type VersionKey<'a> = (u64, u64, u64, bool, Vec<(bool, u64, &'a str)>);

/// A tag `v<major>.<minor>.<patch>[-<pre>]` as a key that orders as SemVer
/// does: a release after its prereleases, numeric identifiers by number and
/// before alphanumeric ones; `None` for any other tag.
fn version_of(tag: &str) -> Option<VersionKey<'_>> {
    let version = tag.strip_prefix('v')?;
    let (core, pre) = version
        .split_once('-')
        .map_or((version, None), |(core, pre)| (core, Some(pre)));
    let mut numbers = core.split('.').map(|n| n.parse::<u64>().ok());
    let (major, minor, patch) = (numbers.next()??, numbers.next()??, numbers.next()??);
    if numbers.next().is_some() || pre.is_some_and(str::is_empty) {
        return None;
    }
    let identifiers = pre
        .unwrap_or_default()
        .split('.')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u64>()
                .map_or((true, 0, part), |number| (false, number, ""))
        })
        .collect();
    Some((major, minor, patch, pre.is_none(), identifiers))
}

/// The section of `version` in a changelog closed as it: one with changes
/// still under Unreleased, or without the version's heading or changes, is
/// refused, so a release never ships changes its notes leave out.
fn closed<'a>(changelog: &'a str, version: &str) -> Result<&'a str, String> {
    if section(changelog, "Unreleased").is_some() {
        return Err(format!(
            "changes under Unreleased; close them as `## [{version}]` first"
        ));
    }
    section(changelog, version).ok_or_else(|| format!("no changes under `## [{version}]`"))
}

/// The body of the `## [version]` section of a Keep a Changelog file, up to
/// the next section or the link definitions at the file's foot, trimmed;
/// `None` without one or when it is empty.
pub(crate) fn section<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
    let heading = format!("## [{version}]");
    let mut offset = 0;
    let mut start = None;
    for line in changelog.split_inclusive('\n') {
        if let Some(start) = start {
            let definition = line.starts_with('[') && line.contains("]: ");
            if line.starts_with("## ") || definition {
                return Some(changelog[start..offset].trim()).filter(|s| !s.is_empty());
            }
        } else if line.starts_with(&heading) {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start
        .map(|start| changelog[start..].trim())
        .filter(|s| !s.is_empty())
}

fn notes(package: &str, loader: &str, changes: &str) -> Result<String, String> {
    let version = loader.strip_prefix('v').unwrap_or(loader);
    let text = format!(
        "{WARNING}\nBuilt for Enshwick {version}. Extract [enshwick.zip]({PLATFORM}/releases/tag/{loader}) of that release and then `{package}.zip` into the folder that holds `enshrouded.exe` or `enshrouded_server.exe`.\n\n## What's new\n\n{changes}\n"
    );
    let length = text.chars().count();
    if length > MAX_BODY {
        return Err(format!(
            "the notes of {package} have {length} characters; GitHub takes {MAX_BODY} at most"
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REV: &str = "30693a6f24efd923047903f0245336e31136cd35";

    #[test]
    fn the_loader_release_is_the_version_tag_at_the_pinned_commit() {
        let listing = format!(
            "1111111111111111111111111111111111111111\trefs/tags/v1.0.0-alpha.11\n\
             2222222222222222222222222222222222222222\trefs/tags/v1.0.0-alpha.12\n\
             {REV}\trefs/tags/v1.0.0-alpha.12^{{}}\n\
             {REV}\trefs/tags/vnext\n"
        );
        assert_eq!(release_at(&listing, REV), Some("v1.0.0-alpha.12"));
        assert_eq!(
            release_at(&listing, &REV.to_uppercase()),
            Some("v1.0.0-alpha.12")
        );
    }

    #[test]
    fn a_commit_no_release_tags_has_no_loader_release() {
        let listing = "1111111111111111111111111111111111111111\trefs/tags/v1.0.0-alpha.11\n";
        assert_eq!(release_at(listing, REV), None);
        assert_eq!(release_at(&format!("{REV}\trefs/tags/dev\n"), REV), None);
    }

    #[test]
    fn the_newest_of_several_tags_at_one_commit_names_the_release() {
        let listing = format!(
            "{REV}\trefs/tags/v1.0.0-alpha.9\n{REV}\trefs/tags/v1.0.0\n{REV}\trefs/tags/v1.0.0-alpha.10\n"
        );
        assert_eq!(release_at(&listing, REV), Some("v1.0.0"));
        let alphas = format!("{REV}\trefs/tags/v1.0.0-alpha.10\n{REV}\trefs/tags/v1.0.0-alpha.9\n");
        assert_eq!(release_at(&alphas, REV), Some("v1.0.0-alpha.10"));
    }

    #[test]
    fn only_packages_release_plz_never_releases_are_held_back() {
        let config = "[workspace]\nrelease_always = true\n\n\
            [[package]]\nname = \"a\"\ngit_release_name = \"A {{ version }}\"\n\n\
            [[package]]\nname = \"b\"\nrelease = false\n\n\
            [[package]]\nname = \"c\"\nrelease = true\n";
        assert_eq!(held_back(&entries(config).unwrap()), ["b"]);
        assert_eq!(entries("[workspace]\n"), Ok(Vec::new()));
        assert!(entries("[[package]\n").is_err());
    }

    #[test]
    fn the_pinned_commit_is_the_rev_of_the_api_in_the_workspace() {
        let manifest = format!(
            "[workspace.dependencies]\nenshwick = {{ git = \"{PLATFORM}\", rev = \"{REV}\" }}\n"
        );
        assert_eq!(pinned_rev(&manifest).as_deref(), Some(REV));
        assert_eq!(pinned_rev("[workspace]\n"), None);
        assert_eq!(pinned_rev("[workspace"), None);
    }

    #[test]
    fn a_release_is_finished_only_with_every_file_attached() {
        let all = "m.cdx.json\nm.pdb\nm.zip\nSHA256SUMS\n";
        assert!(has_files(all, "m"));
        assert!(!has_files("m.pdb\nm.zip\nSHA256SUMS\n", "m"));
        assert!(!has_files("", "m"));
        assert!(!has_files("n.cdx.json\nn.pdb\nn.zip\nSHA256SUMS\n", "m"));
    }

    #[test]
    fn a_release_is_titled_by_its_release_name_or_else_its_tag() {
        let config = entries(
            "[[package]]\nname = \"m\"\ngit_release_name = \"Mod {{ version }}\"\n\
             [[package]]\nname = \"n\"\n",
        )
        .unwrap();
        let package = |name: &str| Package {
            name: name.to_owned(),
            version: "0.2.0".to_owned(),
            dir: PathBuf::new(),
            id: name.to_owned(),
            windows_only: false,
            cdylib: true,
            lib: true,
        };
        assert_eq!(title(&config, &package("m")), "Mod 0.2.0");
        assert_eq!(title(&config, &package("n")), "n-v0.2.0");
    }

    #[test]
    fn only_a_mod_with_a_player_page_that_is_not_held_back_ships() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let package = |dir: &str, cdylib: bool| Package {
            name: "m".to_owned(),
            version: "0.1.0".to_owned(),
            dir: root.join(dir),
            id: "m".to_owned(),
            windows_only: false,
            cdylib,
            lib: true,
        };
        assert!(ships(&package("area-loot", true), &[]));
        assert!(!ships(&package("area-loot", true), &["m".to_owned()]));
        assert!(!ships(&package("area-loot", false), &[]));
        assert!(!ships(&package("tools/xtask", true), &[]));
    }

    #[test]
    fn only_tags_shaped_as_versions_count() {
        assert!(version_of("v1.0.0-alpha.12").is_some());
        assert!(version_of("v0.2.0").is_some());
        for tag in ["dev", "1.0.0", "v1.0", "v1.0.0.0", "v1.0.0-", "vx.0.0"] {
            assert_eq!(version_of(tag), None, "{tag}");
        }
    }

    #[test]
    fn notes_take_the_closed_section_of_the_version() {
        let changelog = "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-11-05\n\n### Added\n\n- More loot.\n\n## [0.1.0]\n\n- First.\n";
        assert_eq!(closed(changelog, "0.2.0"), Ok("### Added\n\n- More loot."));
        assert_eq!(closed(changelog, "0.1.0"), Ok("- First."));
    }

    #[test]
    fn the_last_section_stops_at_the_link_definitions() {
        let changelog = "## [Unreleased]\n\n## [0.1.0]\n\n- First, see [docs].\n\n[docs]: https://example.com/docs\n[0.1.0]: https://example.com/tag\n";
        assert_eq!(closed(changelog, "0.1.0"), Ok("- First, see [docs]."));
    }

    #[test]
    fn notes_refuse_open_changes_and_a_missing_or_empty_section() {
        let open = "## [Unreleased]\n\n- Next.\n\n## [0.2.0]\n\n- A.\n";
        assert!(closed(open, "0.2.0").unwrap_err().contains("Unreleased"));
        let empty = "## [Unreleased]\n\n## [0.2.0]\n\n## [0.1.0]\n\n- A.\n";
        assert!(closed(empty, "0.2.0").unwrap_err().contains("0.2.0"));
        assert!(closed(empty, "0.3.0").unwrap_err().contains("0.3.0"));
    }

    #[test]
    fn notes_open_with_both_warnings_and_name_the_loader_release() {
        let text = notes("enshwick-game-rates", "v1.0.0-alpha.12", "- More loot.").unwrap();
        assert!(text.starts_with("> [!WARNING]\n> This is an alpha."));
        assert!(text.contains("> Це альфа-версія."));
        assert!(text.contains(
            "Built for Enshwick 1.0.0-alpha.12. Extract [enshwick.zip](https://github.com/enshwick/enshwick/releases/tag/v1.0.0-alpha.12)"
        ));
        assert!(text.contains("`enshwick-game-rates.zip`"));
        assert!(text.ends_with("## What's new\n\n- More loot.\n"));
    }

    #[test]
    fn notes_over_the_length_github_takes_are_refused() {
        let at_limit = notes("m", "v1.0.0-alpha.12", "").unwrap();
        let room = MAX_BODY - at_limit.chars().count();
        assert!(notes("m", "v1.0.0-alpha.12", &"x".repeat(room)).is_ok());
        assert!(notes("m", "v1.0.0-alpha.12", &"x".repeat(room + 1)).is_err());
    }
}
