//! The two steps of a release that release-plz cannot take.
//!
//! `cargo xtask pin <loader tag>` pins the platform to a loader release and
//! says so in the changelog of every mod players get that has nothing else
//! under Unreleased, so each mod's folder changes and release-plz bumps
//! every mod: a mod built for one loader alpha needs a rebuild for the next.
//!
//! `cargo xtask close --date <YYYY-MM-DD>` runs on the release pull request's
//! branch: it closes each bumped mod's Unreleased section as its new version,
//! sets a bumped mod with nothing for players back to its released version,
//! and writes the pull request's body.

use std::fmt::Write as _;

use crate::notes::{self, CHANGELOG, PLATFORM};
use crate::{Package, Step, say};

const PIN_USAGE: &str = "usage: cargo xtask pin <loader tag>";
const CLOSE_USAGE: &str =
    "usage: cargo xtask close --date <YYYY-MM-DD> [--base <git ref>] [--body <file>]";
/// The changelog line of a release with no other change.
const BUILT_FOR: &str = "- Built for Enshwick ";

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
    for package in notes::shipped(&notes::release_config(&root)?)? {
        let tag = notes::tag_of(&package);
        let released = notes::released(&tag)?;
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

/// The `changelog` with its Unreleased section naming the loader `version`,
/// or without that line for `None`: an existing line is updated or dropped,
/// an empty section gets one under `### Changed`, and a section with other
/// changes stays as it is, since those changes bump the mod already.
fn built_for(changelog: &str, version: Option<&str>) -> Result<String, String> {
    let (start, end) = unreleased(changelog).ok_or("no `## [Unreleased]` section")?;
    let body = &changelog[start..end];
    let has_line = body.lines().any(|l| l.starts_with(BUILT_FOR));
    let body = match version {
        Some(version) if has_line => {
            let line = format!("{BUILT_FOR}{version}.");
            body.lines()
                .map(|l| {
                    if l.starts_with(BUILT_FOR) {
                        line.as_str()
                    } else {
                        l
                    }
                })
                .fold(String::new(), |mut text, l| {
                    text.push_str(l);
                    text.push('\n');
                    text
                })
        }
        Some(version) if body.trim().is_empty() => {
            format!("\n### Changed\n\n{BUILT_FOR}{version}.\n\n")
        }
        None if has_line => {
            let rest: Vec<&str> = body.lines().filter(|l| !l.starts_with(BUILT_FOR)).collect();
            // Only headings left: the line was the section's one change.
            if rest
                .iter()
                .all(|l| l.trim().is_empty() || l.starts_with("### "))
            {
                "\n".to_owned()
            } else {
                rest.iter().fold(String::new(), |mut text, l| {
                    text.push_str(l);
                    text.push('\n');
                    text
                })
            }
        }
        _ => return Ok(changelog.to_owned()),
    };
    Ok(format!(
        "{}{body}{}",
        &changelog[..start],
        &changelog[end..]
    ))
}

/// Where the body of `## [Unreleased]` starts and ends: after its heading's
/// line, up to the next section or the link definitions at the file's foot.
fn unreleased(changelog: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut start = None;
    for line in changelog.split_inclusive('\n') {
        if let Some(start) = start {
            let definition = line.starts_with('[') && line.contains("]: ");
            if line.starts_with("## ") || definition {
                return Some((start, offset));
            }
        } else if line.starts_with("## [Unreleased]") {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start.map(|start| (start, changelog.len()))
}

/// The `changelog` with what is under Unreleased moved under
/// `## [version] - date`, Unreleased left empty above it; `None` when
/// nothing is under Unreleased. A version closed already but not released
/// is refused: a second heading would hide the first from its notes.
fn close_section(changelog: &str, version: &str, date: &str) -> Result<Option<String>, String> {
    let Some((start, end)) = unreleased(changelog) else {
        return Ok(None);
    };
    let body = changelog[start..end].trim();
    if body.is_empty() {
        return Ok(None);
    }
    let heading = format!("## [{version}]");
    if changelog.lines().any(|line| line.starts_with(&heading)) {
        return Err(format!(
            "{version} is closed already and has changes under Unreleased; move them into its section by hand"
        ));
    }
    Ok(Some(format!(
        "{}\n## [{version}] - {date}\n\n{body}\n\n{}",
        &changelog[..start],
        &changelog[end..]
    )))
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
/// sets a mod bumped with nothing under Unreleased back to its version on
/// `--base` (`origin/main`) when that one is released, and writes the
/// release pull request's body to `--body`.
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
    let root = notes::root();
    let config = notes::release_config(&root)?;
    let mut body = String::from("## Chores\n\n");
    let mut kept = false;
    let mut ok = true;
    for package in notes::shipped(&config)? {
        if notes::released(&notes::tag_of(&package))? {
            continue;
        }
        let path = package.dir.join(CHANGELOG);
        let changelog =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let closed = match close_section(&changelog, &package.version, date) {
            Ok(closed) => closed,
            Err(error) => {
                say(&format!("{}: {error}", package.name));
                ok = false;
                continue;
            }
        };
        if let Some(closed) = closed {
            std::fs::write(&path, closed).map_err(|e| format!("{}: {e}", path.display()))?;
        } else if notes::section(&changelog, &package.version).is_none() {
            if let Some(released) = keep(&package, &base)? {
                let _ = writeln!(
                    body,
                    "- Kept `{}` at {released}: nothing under Unreleased for players.",
                    package.name
                );
                kept = true;
            } else {
                say(&format!(
                    "{}: {} has nothing under Unreleased and no released version to keep",
                    package.name, package.version
                ));
                ok = false;
            }
            continue;
        }
        let _ = writeln!(
            body,
            "- Released `{}` {}, its changelog closed as that version.",
            package.name, package.version
        );
    }
    if kept && !Step::new(&["update", "--workspace"]).run(false) {
        return Ok(false);
    }
    if let Some(file) = body_file {
        std::fs::write(file, body).map_err(|e| format!("{file}: {e}"))?;
    }
    Ok(ok)
}

/// Sets the package's manifest back to its version on `base` when that
/// version is released, and returns it.
fn keep(package: &Package, base: &str) -> Result<Option<String>, String> {
    let dir = package.dir.to_string_lossy();
    let manifest = notes::git(&["-C", &dir, "show", &format!("{base}:./Cargo.toml")])?;
    let Some(released) = manifest.parse::<toml::Table>().ok().and_then(|m| {
        m.get("package")?
            .get("version")?
            .as_str()
            .map(str::to_owned)
    }) else {
        return Ok(None);
    };
    let tag = format!("{}-v{released}", package.name);
    if released == package.version || !notes::released(&tag)? {
        return Ok(None);
    }
    let path = package.dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let kept = set_version(&text, &package.version, &released)
        .ok_or_else(|| format!("{}: no `version = \"{}\"`", path.display(), package.version))?;
    std::fs::write(&path, kept).map_err(|e| format!("{}: {e}", path.display()))?;
    say(&format!("{}: kept at {released}", package.name));
    Ok(Some(released))
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
    const HEAD: &str = "# Changelog\n\nIntro.\n\n## [Unreleased]\n";
    const RELEASED: &str = "## [0.1.0] - 2026-11-05\n\n### Added\n\n- First.\n";

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

    #[test]
    fn pinning_back_drops_a_loader_line_and_keeps_other_changes() {
        let changelog = format!("{HEAD}\n{RELEASED}");
        let built = built_for(&changelog, Some("1.0.0-alpha.14")).unwrap();
        assert_eq!(built_for(&built, None).unwrap(), changelog);
        let mixed = format!(
            "{HEAD}\n### Changed\n\n- Built for Enshwick 1.0.0-alpha.14.\n- Other.\n\n{RELEASED}"
        );
        assert_eq!(
            built_for(&mixed, None).unwrap(),
            format!("{HEAD}\n### Changed\n\n- Other.\n\n{RELEASED}")
        );
        assert_eq!(built_for(&changelog, None).unwrap(), changelog);
    }

    #[test]
    fn an_empty_unreleased_section_names_the_loader() {
        let changelog = format!("{HEAD}\n{RELEASED}");
        let built = built_for(&changelog, Some("1.0.0-alpha.14")).unwrap();
        assert_eq!(
            built,
            format!("{HEAD}\n### Changed\n\n- Built for Enshwick 1.0.0-alpha.14.\n\n{RELEASED}")
        );
        let again = built_for(&built, Some("1.0.0-alpha.15")).unwrap();
        assert_eq!(again, built.replace("alpha.14", "alpha.15"));
        assert_eq!(
            built_for(&format!("{HEAD}\n"), Some("1.0.0")).unwrap(),
            format!("{HEAD}\n### Changed\n\n- Built for Enshwick 1.0.0.\n\n")
        );
    }

    #[test]
    fn unreleased_changes_keep_the_section_as_it_is() {
        let changelog = format!("{HEAD}\n### Fixed\n\n- A bug.\n\n{RELEASED}");
        assert_eq!(
            built_for(&changelog, Some("1.0.0-alpha.14")).unwrap(),
            changelog
        );
        assert!(built_for(RELEASED, Some("1.0.0-alpha.14")).is_err());
    }

    #[test]
    fn closing_moves_the_unreleased_changes_under_the_version() {
        let changelog = format!("{HEAD}\n### Fixed\n\n- A bug.\n\n{RELEASED}");
        assert_eq!(
            close_section(&changelog, "0.1.1", "2026-11-08")
                .unwrap()
                .unwrap(),
            format!("{HEAD}\n## [0.1.1] - 2026-11-08\n\n### Fixed\n\n- A bug.\n\n{RELEASED}")
        );
        let foot = "## [Unreleased]\n\n- A.\n[0.1.0]: https://example.com\n";
        assert_eq!(
            close_section(foot, "0.1.1", "2026-11-08").unwrap().unwrap(),
            "## [Unreleased]\n\n## [0.1.1] - 2026-11-08\n\n- A.\n\n[0.1.0]: https://example.com\n"
        );
        assert_eq!(
            close_section(&format!("{HEAD}\n{RELEASED}"), "0.1.1", "2026-11-08"),
            Ok(None)
        );
    }

    #[test]
    fn a_version_closed_already_is_never_closed_twice() {
        let changelog = format!("{HEAD}\n### Fixed\n\n- Late.\n\n{RELEASED}");
        assert!(close_section(&changelog, "0.1.0", "2026-11-08").is_err());
    }

    #[test]
    fn only_a_link_definition_at_a_line_start_ends_the_section() {
        let changelog = "## [Unreleased]\n\n- Rates: [x]: y.\n[ref] at the start.\n";
        assert_eq!(
            close_section(changelog, "0.1.1", "2026-11-08")
                .unwrap()
                .unwrap(),
            "## [Unreleased]\n\n## [0.1.1] - 2026-11-08\n\n- Rates: [x]: y.\n[ref] at the start.\n\n"
        );
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
