//! `cargo xtask check`: the compiling checks a change needs before it
//! merges; the git hooks run only the checks that compile nothing.
//!
//! First the tree's own rules ([`crate::check`]), the code rules clippy
//! cannot express (ast-grep, from `sgconfig.yml`) and the unused
//! dependencies (cargo-machete). Clippy, the tests and the doctests run on
//! the host, where the packages that build only for Windows
//! (`[package.metadata.xtask] windows-only = true`) stay out; off Windows
//! clippy also runs for Windows through `cargo xwin`, beside the host
//! checks. `--windows` runs the Windows-only packages' tests under Wine, and
//! `--all` adds formatting, rustdoc, cargo-deny and those tests; `--no-wine`
//! leaves them out, for CI, which runs them natively on Windows.

use std::io::Write as _;
use std::process::Command;
use std::thread;
use std::time::Instant;

use crate::commands::wine;
use crate::{Package, cargo, packages, say};

pub(crate) const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

/// The command line `check` takes.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Adds formatting, rustdoc, cargo-deny and the Windows-only packages'
    /// tests.
    #[arg(long)]
    all: bool,
    /// Runs the Windows-only packages' tests under Wine.
    #[arg(long, conflicts_with = "no_wine")]
    windows: bool,
    /// Leaves the Windows-only packages' tests out, for CI, which runs them
    /// natively on Windows.
    #[arg(long)]
    no_wine: bool,
    /// Taken for the platform's command line; a mod repository is small, so
    /// every change checks all of it.
    #[arg(long, hide = true)]
    base: Option<String>,
}

/// Runs the check `args` asks for; whether it passed.
///
/// # Errors
///
/// When `cargo metadata` or a rule's input does not read.
pub(crate) fn run(args: &Args) -> Result<bool, String> {
    let packages = packages()?;
    let mut ok = check(&packages, args.all)?;
    if ok && !args.no_wine && (args.all || args.windows) {
        let windows_only: Vec<&str> = packages
            .iter()
            .filter(|p| p.windows_only)
            .map(|p| p.name.as_str())
            .collect();
        ok = windows_only.is_empty() || wine::run(&windows_only, args.windows);
    }
    Ok(ok)
}

/// One command line: cargo, or another tool of the checks.
#[derive(Debug)]
pub(crate) struct Step {
    program: Option<&'static str>,
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
}

impl Step {
    pub(crate) fn new(words: &[&str]) -> Self {
        Self {
            program: None,
            args: words.iter().map(|w| (*w).to_owned()).collect(),
            env: Vec::new(),
        }
    }

    /// A step that runs `program` instead of cargo.
    fn tool(program: &'static str, words: &[&str]) -> Self {
        Self {
            program: Some(program),
            ..Self::new(words)
        }
    }

    fn then_owned(mut self, words: &[String]) -> Self {
        self.args.extend_from_slice(words);
        self
    }

    pub(crate) fn then(mut self, words: &[&str]) -> Self {
        self.args.extend(words.iter().map(|w| (*w).to_owned()));
        self
    }

    pub(crate) fn env(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.env.push((name, value.into()));
        self
    }

    fn line(&self) -> String {
        format!(
            "{} {}",
            self.program.unwrap_or("cargo"),
            self.args.join(" ")
        )
    }

    /// Runs the step with its output shown, or captured when `quiet` (for a
    /// step running beside others) and shown only when it fails.
    pub(crate) fn run(&self, quiet: bool) -> bool {
        let started = Instant::now();
        let mut command = Command::new(self.program.map_or_else(cargo, str::to_owned));
        command
            .args(&self.args)
            .envs(self.env.iter().map(|(name, value)| (name, value)));
        let (ok, output) = if quiet {
            match command.output() {
                Ok(output) => (
                    output.status.success(),
                    [output.stdout, output.stderr].concat(),
                ),
                Err(error) => (false, error.to_string().into_bytes()),
            }
        } else {
            say(&format!("> {}", self.line()));
            match command.status() {
                Ok(status) => (status.success(), Vec::new()),
                Err(error) => (false, error.to_string().into_bytes()),
            }
        };
        let seconds = started.elapsed().as_secs_f32();
        if ok {
            say(&format!("ok in {seconds:.1} s: {}", self.line()));
        } else {
            let _shown = std::io::stderr().write_all(&output);
            say(&format!("FAILED in {seconds:.1} s: {}", self.line()));
        }
        ok
    }
}

/// The ast-grep rules' own tests, the rules over the repository, then the
/// dependencies no crate uses.
fn source_checks() -> [Step; 3] {
    [
        Step::tool("ast-grep", &["test", "--skip-snapshot-tests"]),
        Step::tool("ast-grep", &["scan"]),
        Step::tool("cargo-machete", &[]),
    ]
}

/// Runs the tree's rules and the source checks, then the host steps one
/// after another (they share the build lock) and the Windows lint beside
/// them; the first failing host step ends the host checks.
fn check(packages: &[Package], all: bool) -> Result<bool, String> {
    let problems = crate::check::problems()?;
    for problem in &problems {
        say(problem);
    }
    if !problems.is_empty() || !source_checks().iter().all(|step| step.run(false)) {
        return Ok(false);
    }
    let mut workspace = vec!["--workspace".to_owned()];
    if !cfg!(windows) {
        for package in packages.iter().filter(|p| p.windows_only) {
            workspace.extend(["--exclude".to_owned(), package.name.clone()]);
        }
    }
    let host = |words: &[&str]| Step::new(words).then_owned(&workspace);
    let mut steps = Vec::new();
    if all {
        steps.push(Step::new(&["fmt", "--all", "--check"]));
    }
    steps.push(host(&["clippy", "--all-targets"]).then(&["--", "-D", "warnings"]));
    steps.push(host(&["nextest", "run", "--no-tests=pass"]));
    // Doctests need a library; off Windows a Windows-only one runs them
    // under Wine.
    if packages
        .iter()
        .any(|p| p.lib && (cfg!(windows) || !p.windows_only))
    {
        steps.push(host(&["test", "--doc"]));
    }
    if all {
        steps.push(host(&["doc", "--no-deps"]).env("RUSTDOCFLAGS", "-D warnings"));
        steps.push(Step::new(&["deny", "check"]));
    }
    let windows = (!cfg!(windows)).then(|| {
        Step::new(&[
            "xwin",
            "clippy",
            "--target",
            WINDOWS_TARGET,
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ])
    });
    if let Some(step) = &windows {
        say(&format!("> {} (beside the host checks)", step.line()));
    }
    Ok(thread::scope(|scope| {
        let windows = windows.as_ref().map(|step| scope.spawn(|| step.run(true)));
        let host_passed = steps.iter().all(|step| step.run(false));
        let windows_passed = windows.is_none_or(|handle| handle.join().unwrap_or(false));
        host_passed && windows_passed
    }))
}
