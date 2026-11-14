//! The changelog line that names the loader a mod's release was built for,
//! in its Unreleased section.

use cargo_enshwick::changelog::{UNRELEASED, span};

/// The changelog line of a release with no other change.
const BUILT_FOR: &str = "- Built for Enshwick ";

/// The `changelog` with its Unreleased section naming the loader `version`,
/// or without that line for `None`: an existing line is updated or dropped,
/// an empty section gets one under `### Changed`, and a section with other
/// changes stays as it is, since those changes bump the mod already.
pub(crate) fn built_for(changelog: &str, version: Option<&str>) -> Result<String, String> {
    let (start, end) = span(changelog, UNRELEASED).ok_or("no `## [Unreleased]` section")?;
    let body = &changelog[start..end];
    let has_line = body.lines().any(|l| l.starts_with(BUILT_FOR));
    let body = match version {
        Some(version) if has_line => {
            let line = format!("{BUILT_FOR}{version}.");
            join_lines(body.lines().map(|l| {
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
                join_lines(rest)
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

/// The loader version a changelog `section`'s loader line names, if it has
/// one.
pub(crate) fn named(section: &str) -> Option<&str> {
    section
        .lines()
        .find_map(|line| line.strip_prefix(BUILT_FOR))
        .map(|rest| rest.trim_end().trim_end_matches('.'))
}

/// The lines, each ended by a newline.
fn join_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> String {
    lines.into_iter().fold(String::new(), |mut text, line| {
        text.push_str(line);
        text.push('\n');
        text
    })
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
    fn the_loader_line_names_its_version_without_the_final_period() {
        assert_eq!(
            named("### Changed\n\n- Other.\n- Built for Enshwick 1.0.0-alpha.15.\n"),
            Some("1.0.0-alpha.15")
        );
        assert_eq!(named("### Fixed\n\n- A bug.\n"), None);
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
}
