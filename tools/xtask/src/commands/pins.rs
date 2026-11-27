//! `cargo xtask pins`: every platform pin of the root manifest is a commit
//! of the platform's `main`, and none moves back from the pin of the branch
//! the change merges into. A mods pull request that waits for a platform
//! one pins its branch head, and a stale Renovate run an older commit; both
//! fail here instead of merging. It reads the platform's commit history, so
//! CI runs it as a step of its own rather than inside `cargo xtask check`.

use std::path::PathBuf;
use std::process::Command;

use crate::check::mod_rules::platform_revs;
use crate::commands::notes::{MANIFEST, PLATFORM, git, pinned_rev};
use crate::{metadata, say};

/// The platform branch every pin is a commit of.
const MAIN: &str = "main";

/// The command line `pins` takes.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The commit or branch the change merges into, such as `origin/main`:
    /// no pin may move back from the platform commit its manifest pins.
    #[arg(long)]
    base: Option<String>,
}

/// Checks the pins against the platform's history; whether they hold.
///
/// # Errors
///
/// When a manifest does not read, or git cannot fetch the platform's
/// `main` or a pinned commit.
pub(crate) fn run(args: &Args) -> Result<bool, String> {
    let root = PathBuf::from(metadata()?["workspace_root"].as_str().unwrap_or("."));
    let path = root.join(MANIFEST);
    let manifest: toml::Table = std::fs::read_to_string(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let revs = platform_revs(&manifest);
    let base = match &args.base {
        Some(base) => pinned_rev(&git(&["show", &format!("{base}:{MANIFEST}")])?),
        None => None,
    };
    let history = History::fetch(revs.iter().chain(&base))?;
    let problems = problems(&revs, base.as_deref(), |a, b| history.is_ancestor(a, b))?;
    for problem in &problems {
        say(problem);
    }
    if problems.is_empty() {
        say(&format!(
            "the platform pins are on its {MAIN} and move only forward"
        ));
    }
    Ok(problems.is_empty())
}

/// What is wrong with the pinned `revs`, given the `base` branch's pin and
/// whether one commit is an ancestor of another (a commit of itself).
fn problems(
    revs: &[String],
    base: Option<&str>,
    is_ancestor: impl Fn(&str, &str) -> Result<bool, String>,
) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for rev in revs {
        if !is_ancestor(rev, MAIN)? {
            problems.push(format!(
                "the platform is pinned to {rev}, which is not on its {MAIN}; once its pull request merges, pin the commit it merged as"
            ));
        }
        if let Some(base) = base
            && !is_ancestor(base, rev)?
        {
            problems.push(format!(
                "the platform pin moves back from {base} to {rev}, which does not descend from it: the change read a stale {MAIN}; pin its newest commit"
            ));
        }
    }
    Ok(problems)
}

/// The platform's commits, without trees or files, in a bare repository of
/// its own that goes when this does.
#[derive(Debug)]
struct History {
    dir: PathBuf,
}

impl History {
    /// Fetches the platform's `main` and the `revs`, which may lie on no
    /// branch: GitHub serves the head of every pull request by its hash.
    fn fetch<'a>(revs: impl Iterator<Item = &'a String>) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("xtask-pins-{}", std::process::id()));
        let history = Self { dir };
        let git_dir = history.git_dir();
        git(&["init", "--quiet", "--bare", &history.dir.to_string_lossy()])?;
        let main = format!("+refs/heads/{MAIN}:refs/heads/{MAIN}");
        let mut fetch = vec![
            git_dir.as_str(),
            "fetch",
            "--quiet",
            "--no-tags",
            "--filter=tree:0",
            PLATFORM,
            main.as_str(),
        ];
        for rev in revs {
            fetch.push(rev);
        }
        git(&fetch)?;
        Ok(history)
    }

    fn git_dir(&self) -> String {
        format!("--git-dir={}", self.dir.display())
    }

    /// Whether `ancestor` is `descendant` or one of its ancestors.
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, String> {
        let status = Command::new("git")
            .args([
                &self.git_dir(),
                "merge-base",
                "--is-ancestor",
                ancestor,
                descendant,
            ])
            .status()
            .map_err(|error| format!("running git: {error}"))?;
        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(format!(
                "git merge-base --is-ancestor {ancestor} {descendant}: {status}"
            )),
        }
    }
}

impl Drop for History {
    fn drop(&mut self) {
        let _gone = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A platform whose `main` is `a`, then `b`, with `side` branching off
    /// `a`: each commit with its ancestors.
    fn is_ancestor(ancestor: &str, descendant: &str) -> Result<bool, String> {
        if !["a", "b", "side"].contains(&ancestor) {
            return Err(format!("no commit {ancestor}"));
        }
        let line: &[&str] = match descendant {
            MAIN | "b" => &["a", "b"],
            "a" => &["a"],
            "side" => &["a", "side"],
            _ => return Err(format!("no commit {descendant}")),
        };
        Ok(line.contains(&ancestor))
    }

    fn revs(revs: &[&str]) -> Vec<String> {
        revs.iter().map(|rev| (*rev).to_owned()).collect()
    }

    #[test]
    fn a_pin_on_main_that_stays_or_moves_forward_passes() {
        assert_eq!(
            problems(&revs(&["b"]), Some("a"), is_ancestor).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            problems(&revs(&["b"]), Some("b"), is_ancestor).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            problems(&revs(&["a"]), None, is_ancestor).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_pin_off_main_is_refused_naming_the_commit() {
        assert_eq!(
            problems(&revs(&["side"]), Some("a"), is_ancestor).unwrap(),
            [
                "the platform is pinned to side, which is not on its main; once its pull request merges, pin the commit it merged as"
            ]
        );
    }

    #[test]
    fn a_pin_moving_back_to_an_older_main_commit_is_refused() {
        assert_eq!(
            problems(&revs(&["a"]), Some("b"), is_ancestor).unwrap(),
            [
                "the platform pin moves back from b to a, which does not descend from it: the change read a stale main; pin its newest commit"
            ]
        );
    }

    #[test]
    fn a_pin_beside_the_base_s_is_refused_both_ways() {
        assert_eq!(
            problems(&revs(&["side"]), Some("b"), is_ancestor)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn an_unknown_commit_is_an_error_not_a_pass() {
        assert!(problems(&revs(&["gone"]), None, is_ancestor).is_err());
    }
}
