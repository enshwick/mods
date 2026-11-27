//! `cargo xtask pins`: every platform pin of the root manifest is a commit
//! of the platform's `main`, and none moves back from the pin of the branch
//! the change merges into. A mods pull request that waits for a platform
//! one pins its branch head, and a stale Renovate run an older commit; both
//! fail here instead of merging. It reads the platform's commit history, so
//! CI runs it as a step of its own rather than inside `cargo xtask check`.

use std::path::PathBuf;

use crate::check::mod_rules::{full_commit, platform_revs};
use crate::commands::notes::{MANIFEST, PLATFORM, git, git_answer};
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
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let revs = revs_of(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let bases = match &args.base {
        Some(base) => revs_of(&git(&["show", &format!("{base}:{MANIFEST}")])?)
            .map_err(|e| format!("{base}:{MANIFEST}: {e}"))?,
        None => Vec::new(),
    };
    let history = History::fetch(revs.iter().chain(&bases).filter(|rev| full_commit(rev)))?;
    let problems = problems(&revs, &bases, |a, b| history.is_ancestor(a, b))?;
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

/// Each platform commit the manifest `text` pins, once.
fn revs_of(text: &str) -> Result<Vec<String>, String> {
    let manifest: toml::Table = text.parse().map_err(|e| format!("{e}"))?;
    Ok(platform_revs(&manifest))
}

/// What is wrong with the pinned `revs`, given the `bases` the branch the
/// change merges into pins and whether one commit is an ancestor of another
/// (a commit of itself). A base pin that is itself off `main` is the fault
/// the change may be fixing, so a pin need not descend from it.
fn problems(
    revs: &[String],
    bases: &[String],
    is_ancestor: impl Fn(&str, &str) -> Result<bool, String>,
) -> Result<Vec<String>, String> {
    let mut on_main = Vec::new();
    for base in bases.iter().filter(|base| full_commit(base)) {
        if is_ancestor(base, MAIN)? {
            on_main.push(base);
        }
    }
    let mut problems = Vec::new();
    for rev in revs {
        if !full_commit(rev) {
            problems.push(format!(
                "the platform is pinned to {rev}, which is not a full commit hash; pin a commit of its {MAIN} by its 40-character hash"
            ));
            continue;
        }
        if !is_ancestor(rev, MAIN)? {
            problems.push(format!(
                "the platform is pinned to {rev}, which is not on its {MAIN}; once its pull request merges, pin the commit it merged as"
            ));
        }
        for base in &on_main {
            if !is_ancestor(base, rev)? {
                problems.push(format!(
                    "the platform pin moves back from {base} to {rev}, which does not descend from it: the change read a stale {MAIN}; pin its newest commit"
                ));
            }
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
        git_answer(&[
            &self.git_dir(),
            "merge-base",
            "--is-ancestor",
            ancestor,
            descendant,
        ])
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

    /// The platform's `main` is `A`, then `B`; `SIDE` branches off `A`.
    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SIDE: &str = "cccccccccccccccccccccccccccccccccccccccc";

    /// Each commit of that platform with its ancestors.
    fn is_ancestor(ancestor: &str, descendant: &str) -> Result<bool, String> {
        if ![A, B, SIDE].contains(&ancestor) {
            return Err(format!("no commit {ancestor}"));
        }
        let line: &[&str] = match descendant {
            MAIN | B => &[A, B],
            A => &[A],
            SIDE => &[A, SIDE],
            _ => return Err(format!("no commit {descendant}")),
        };
        Ok(line.contains(&ancestor))
    }

    fn revs(revs: &[&str]) -> Vec<String> {
        revs.iter().map(|rev| (*rev).to_owned()).collect()
    }

    #[test]
    fn a_pin_on_main_that_stays_or_moves_forward_passes() {
        for (rev, bases) in [(B, &[A][..]), (B, &[B]), (A, &[])] {
            assert_eq!(
                problems(&revs(&[rev]), &revs(bases), is_ancestor).unwrap(),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn a_pin_off_main_is_refused_naming_the_commit() {
        assert_eq!(
            problems(&revs(&[SIDE]), &revs(&[A]), is_ancestor).unwrap(),
            [format!(
                "the platform is pinned to {SIDE}, which is not on its main; once its pull request merges, pin the commit it merged as"
            )]
        );
    }

    #[test]
    fn a_pin_moving_back_to_an_older_main_commit_is_refused() {
        assert_eq!(
            problems(&revs(&[A]), &revs(&[B]), is_ancestor).unwrap(),
            [format!(
                "the platform pin moves back from {B} to {A}, which does not descend from it: the change read a stale main; pin its newest commit"
            )]
        );
    }

    #[test]
    fn a_pin_on_a_side_branch_of_the_base_pin_is_refused_both_ways() {
        assert_eq!(
            problems(&revs(&[SIDE]), &revs(&[B]), is_ancestor)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn moving_from_a_base_pin_off_main_to_a_main_commit_passes() {
        assert_eq!(
            problems(&revs(&[B]), &revs(&[SIDE]), is_ancestor).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_short_hash_or_branch_name_is_refused_without_asking_git() {
        let never = |a: &str, b: &str| Err(format!("asked git about {a} and {b}"));
        let problems = problems(&revs(&["21fed9fb", MAIN]), &revs(&[MAIN]), never).unwrap();
        assert_eq!(problems.len(), 2);
        assert!(
            problems
                .iter()
                .all(|problem| problem.contains("not a full commit hash"))
        );
    }

    #[test]
    fn every_platform_pin_of_the_base_manifest_counts() {
        let base = format!(
            "[workspace.dependencies]\n\
             enshwick-macros = {{ git = \"{PLATFORM}\", rev = \"{A}\" }}\n\
             [patch.'{PLATFORM}']\n\
             enshwick = {{ git = \"{PLATFORM}\", rev = \"{B}\" }}\n"
        );
        assert_eq!(revs_of(&base).unwrap(), [A, B]);
    }

    #[test]
    fn a_base_manifest_that_does_not_parse_is_an_error() {
        assert!(revs_of("[workspace\n").is_err());
    }

    #[test]
    fn an_unknown_commit_is_an_error_not_a_pass() {
        let gone = "dddddddddddddddddddddddddddddddddddddddd";
        assert!(problems(&revs(&[gone]), &[], is_ancestor).is_err());
    }
}
