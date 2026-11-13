//! The rules `cargo xtask check` holds the tree to without compiling, run
//! in the order of [`RULES`]: those of any mod workspace
//! ([`cargo_enshwick::rules`]) and this repository's own ([`mod_rules`]).

pub(crate) mod mod_rules;

use crate::metadata;

/// A rule over the workspace: its problems, one line each.
type Rule = fn() -> Result<Vec<String>, String>;

/// Every rule, in the order they run.
const RULES: [Rule; 2] = [crate_rules, mod_rules::check];

/// The problems of every package by the rules any mod workspace meets.
///
/// # Errors
///
/// When `cargo metadata` does not run or a package's files do not read.
fn crate_rules() -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for package in metadata()?["packages"].as_array().into_iter().flatten() {
        problems.extend(cargo_enshwick::rules::check(package).map_err(|e| e.to_string())?);
    }
    Ok(problems)
}

/// The problems every rule finds.
///
/// # Errors
///
/// When a rule's input does not read.
pub(crate) fn problems() -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for rule in RULES {
        problems.extend(rule()?);
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workspace_meets_every_rule() {
        assert_eq!(problems().unwrap(), Vec::<String>::new());
    }
}
