//! `cargo xtask notes`: the release notes of a mod, written before it is
//! released: the warning that anything may change before 1.0, the loader
//! alpha the mod builds on, how to install it, and the hand-written section
//! of its version in the mod's `CHANGELOG.md`. A mod is about to be released
//! when its tag `<package>-v<version>` is not on the remote yet.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Package, packages, say};

const USAGE: &str = "usage: cargo xtask notes (--unreleased | --package <name>...) [--out <dir>]";
/// Where the platform comes from; its tags name the loader's releases.
const PLATFORM: &str = "https://github.com/enshwick/enshwick";
/// The workspace dependency whose commit names the loader a mod builds on.
const API: &str = "enshwick";
const CHANGELOG: &str = "CHANGELOG.md";
/// Which packages release-plz releases.
const RELEASE_CONFIG: &str = "release-plz.toml";
/// Only a mod with a player page ships to players.
const PLAYER_PAGE: &str = "docs/player.md";
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
/// chosen mod, or for every mod not released yet with `--unreleased`; the
/// files there are the mods a release takes, so a run that refuses one
/// writes none.
pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = root.join("target").join("notes");
    let mut unreleased = false;
    let mut chosen: Vec<&str> = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--unreleased" => unreleased = true,
            "--package" => chosen.push(rest.next().ok_or(USAGE)?),
            "--out" => out = PathBuf::from(rest.next().ok_or(USAGE)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    // Exactly one of the two ways to choose.
    if unreleased != chosen.is_empty() {
        return Err(USAGE.to_owned());
    }
    let config = root.join(RELEASE_CONFIG);
    let held = std::fs::read_to_string(&config)
        .map_err(|e| e.to_string())
        .and_then(|text| held_back(&text))
        .map_err(|e| format!("{}: {e}", config.display()))?;
    let shipped: Vec<Package> = packages()?
        .into_iter()
        .filter(|p| ships(p, &held))
        .collect();
    if let Some(missing) = chosen
        .iter()
        .find(|name| !shipped.iter().any(|p| p.name == **name))
    {
        return Err(format!("{missing} is not a mod players get"));
    }
    let mut mods = Vec::new();
    for package in shipped {
        let take = if unreleased {
            !released(&format!("{}-v{}", package.name, package.version))?
        } else {
            chosen.contains(&package.name.as_str())
        };
        if take {
            mods.push(package);
        }
    }
    if mods.is_empty() {
        say("every mod's version is released already");
        return Ok(true);
    }
    let loader = loader_tag(&root)?;
    let mut written = Vec::new();
    let mut ok = true;
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
            Ok(text) => written.push((package.name.as_str(), text)),
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
    for (name, text) in written {
        let file = out.join(format!("{name}.md"));
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        say(&format!("wrote {}", file.display()));
    }
    Ok(true)
}

/// Whether the repository's remote has the tag already.
fn released(tag: &str) -> Result<bool, String> {
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
fn held_back(config: &str) -> Result<Vec<String>, String> {
    let config: toml::Table = toml::from_str(config).map_err(|e| e.to_string())?;
    Ok(config
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|package| package.get("release").and_then(toml::Value::as_bool) == Some(false))
        .filter_map(|package| package.get("name").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .collect())
}

/// The loader release whose commit the workspace pins the API to.
fn loader_tag(root: &Path) -> Result<String, String> {
    let path = root.join("Cargo.toml");
    let manifest: toml::Table = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| toml::from_str(&text).map_err(|e| e.to_string()))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let rev = manifest
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.get(API))
        .and_then(|spec| spec.get("rev"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{}: no `rev` of {API}", path.display()))?;
    let listing = git(&["ls-remote", "--tags", PLATFORM, "refs/tags/v*"])?;
    release_at(&listing, rev).map(str::to_owned).ok_or_else(|| {
        format!(
            "{API} is pinned to {rev}, which no loader release tags; pin it to the commit of the loader alpha the mods are built for"
        )
    })
}

fn git(args: &[&str]) -> Result<String, String> {
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
fn section<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
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
        assert_eq!(held_back(config), Ok(vec!["b".to_owned()]));
        assert_eq!(held_back("[workspace]\n"), Ok(Vec::new()));
        assert!(held_back("[[package]\n").is_err());
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
