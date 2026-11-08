//! A mod's hand-written `CHANGELOG.md`, in the Keep a Changelog form: its
//! `## [version]` sections read for the release notes and edited by
//! `cargo xtask pin` and `cargo xtask close`.

pub(crate) const CHANGELOG: &str = "CHANGELOG.md";
const UNRELEASED: &str = "Unreleased";
/// The changelog line of a release with no other change.
const BUILT_FOR: &str = "- Built for Enshwick ";

/// Where the body of the `## [version]` section starts and ends: after its
/// heading's line, up to the next section or the link definitions at the
/// file's foot.
fn span(changelog: &str, version: &str) -> Option<(usize, usize)> {
    let heading = format!("## [{version}]");
    let mut offset = 0;
    let mut start = None;
    for line in changelog.split_inclusive('\n') {
        if let Some(start) = start {
            let definition = line.starts_with('[') && line.contains("]: ");
            if line.starts_with("## ") || definition {
                return Some((start, offset));
            }
        } else if line.starts_with(&heading) {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start.map(|start| (start, changelog.len()))
}

/// The body of the `## [version]` section, trimmed; `None` without one or
/// when it is empty.
pub(crate) fn section<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
    span(changelog, version)
        .map(|(start, end)| changelog[start..end].trim())
        .filter(|s| !s.is_empty())
}

/// The section of `version` in a changelog closed as it: one with changes
/// still under Unreleased, or without the version's heading or changes, is
/// refused, so a release never ships changes its notes leave out.
pub(crate) fn closed<'a>(changelog: &'a str, version: &str) -> Result<&'a str, String> {
    if section(changelog, UNRELEASED).is_some() {
        return Err(format!(
            "changes under Unreleased; close them as `## [{version}]` first"
        ));
    }
    section(changelog, version).ok_or_else(|| format!("no changes under `## [{version}]`"))
}

/// The `changelog` with what is under Unreleased moved under
/// `## [version] - date`, Unreleased left empty above it; `None` when
/// nothing is under Unreleased. A version closed already but not released
/// is refused: a second heading would hide the first from its notes.
pub(crate) fn close(changelog: &str, version: &str, date: &str) -> Result<Option<String>, String> {
    let Some((start, end)) = span(changelog, UNRELEASED) else {
        return Ok(None);
    };
    let body = changelog[start..end].trim();
    if body.is_empty() {
        return Ok(None);
    }
    if span(changelog, version).is_some() {
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

/// The `changelog` with its Unreleased section naming the loader `version`,
/// or without that line for `None`: an existing line is updated or dropped,
/// an empty section gets one under `### Changed`, and a section with other
/// changes stays as it is, since those changes bump the mod already.
pub(crate) fn built_for(changelog: &str, version: Option<&str>) -> Result<String, String> {
    let (start, end) = span(changelog, UNRELEASED).ok_or("no `## [Unreleased]` section")?;
    let body = &changelog[start..end];
    let has_line = body.lines().any(|l| l.starts_with(BUILT_FOR));
    let lines = |lines: &mut dyn Iterator<Item = &str>| {
        lines.fold(String::new(), |mut text, l| {
            text.push_str(l);
            text.push('\n');
            text
        })
    };
    let body = match version {
        Some(version) if has_line => {
            let line = format!("{BUILT_FOR}{version}.");
            lines(&mut body.lines().map(|l| {
                if l.starts_with(BUILT_FOR) {
                    line.as_str()
                } else {
                    l
                }
            }))
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
                lines(&mut rest.into_iter())
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

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "# Changelog\n\nIntro.\n\n## [Unreleased]\n";
    const RELEASED: &str = "## [0.1.0] - 2026-11-05\n\n### Added\n\n- First.\n";

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
            close(&changelog, "0.1.1", "2026-11-08").unwrap().unwrap(),
            format!("{HEAD}\n## [0.1.1] - 2026-11-08\n\n### Fixed\n\n- A bug.\n\n{RELEASED}")
        );
        let foot = "## [Unreleased]\n\n- A.\n[0.1.0]: https://example.com\n";
        assert_eq!(
            close(foot, "0.1.1", "2026-11-08").unwrap().unwrap(),
            "## [Unreleased]\n\n## [0.1.1] - 2026-11-08\n\n- A.\n\n[0.1.0]: https://example.com\n"
        );
        assert_eq!(
            close(&format!("{HEAD}\n{RELEASED}"), "0.1.1", "2026-11-08"),
            Ok(None)
        );
    }

    #[test]
    fn a_version_closed_already_is_never_closed_twice() {
        let changelog = format!("{HEAD}\n### Fixed\n\n- Late.\n\n{RELEASED}");
        assert!(close(&changelog, "0.1.0", "2026-11-08").is_err());
    }

    #[test]
    fn only_a_link_definition_at_a_line_start_ends_the_section() {
        let changelog = "## [Unreleased]\n\n- Rates: [x]: y.\n[ref] at the start.\n";
        assert_eq!(
            close(changelog, "0.1.1", "2026-11-08").unwrap().unwrap(),
            "## [Unreleased]\n\n## [0.1.1] - 2026-11-08\n\n- Rates: [x]: y.\n[ref] at the start.\n\n"
        );
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
}
