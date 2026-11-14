//! Repository tasks, run as `cargo xtask <task>`: one module per task in
//! [`commands`], and the rules `cargo xtask check` holds the tree to
//! without compiling in [`check`].

#![forbid(unsafe_code)]

mod check;
mod commands;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Parser as _;

/// The tasks of the repository.
#[derive(Debug, clap::Parser)]
#[command(name = "cargo xtask", about = "The repository's own tasks")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Debug, clap::Subcommand)]
enum Task {
    /// Runs the compiling checks a change needs before it merges.
    Check(commands::check::Args),
    /// Writes each mod's archive and PDB, as a player downloads them.
    Dist(commands::dist::Args),
    /// Writes the release notes of each mod about to be released, and
    /// refuses a mod whose notes are not ready.
    Notes(commands::notes::Args),
    /// Pins every platform crate to the commit of a loader tag.
    Pin {
        /// The loader's tag, such as `v1.0.0-alpha.14`.
        tag: String,
    },
    /// Names a loader tag in the changelog of each mod of the release set
    /// whose released version was built for another loader.
    BuiltFor {
        /// The loader's tag, such as `v1.0.0-alpha.14`.
        tag: String,
    },
    /// Closes the changelog of each mod about to be released.
    Close(commands::release::CloseArgs),
    /// Runs one Windows executable under Wine; cargo calls this as the
    /// target runner.
    #[command(hide = true)]
    WineRun {
        /// The executable and its arguments.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match &cli.task {
        Task::Check(args) => commands::check::run(args),
        Task::Dist(args) => commands::dist::run(args),
        Task::Notes(args) => commands::notes::run(args),
        Task::Pin { tag } => commands::release::pin(tag),
        Task::BuiltFor { tag } => commands::release::built_for_loader(tag),
        Task::Close(args) => commands::release::close(args),
        Task::WineRun { args } => commands::wine::run_one(args),
    };
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            say(&error);
            ExitCode::FAILURE
        }
    }
}

/// A workspace package.
#[derive(Debug)]
struct Package {
    name: String,
    version: String,
    /// The folder of its `Cargo.toml`.
    dir: PathBuf,
    /// The mod id from `[package.metadata.enshwick]`, else the name: the
    /// folder in `mods/` its release installs into.
    id: String,
    windows_only: bool,
    /// Whether it builds a DLL: a mod.
    cdylib: bool,
    /// Whether it has a Rust library, the target doctests run from.
    lib: bool,
}

/// `cargo metadata` of the workspace, without its dependencies.
///
/// # Errors
///
/// When cargo does not run or its output does not read.
fn metadata() -> Result<serde_json::Value, String> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))
}

/// The workspace packages, from `cargo metadata`.
fn packages() -> Result<Vec<Package>, String> {
    Ok(metadata()?["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|package| {
            let types = |wanted: &[&str]| {
                package["targets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|target| {
                        target["crate_types"].as_array().is_some_and(|types| {
                            types.iter().any(|t| wanted.iter().any(|w| t == w))
                        })
                    })
            };
            let name = package["name"].as_str().unwrap_or_default().to_owned();
            Package {
                id: package["metadata"]["enshwick"]["id"]
                    .as_str()
                    .map_or_else(|| name.clone(), str::to_owned),
                name,
                version: package["version"].as_str().unwrap_or_default().to_owned(),
                dir: Path::new(package["manifest_path"].as_str().unwrap_or_default())
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
                windows_only: package["metadata"]["xtask"]["windows-only"].as_bool() == Some(true),
                cdylib: types(&["cdylib"]),
                lib: types(&["lib", "rlib"]),
            }
        })
        .collect())
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

fn say(text: &str) {
    let _shown = writeln!(std::io::stderr(), "xtask: {text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_reads_each_task() {
        let parse = |args: &[&str]| Cli::try_parse_from(["cargo xtask"].iter().chain(args));
        assert!(parse(&["check", "--all", "--no-wine", "--base", "origin/main"]).is_ok());
        assert!(parse(&["check", "--windows", "--no-wine"]).is_err());
        assert!(parse(&["notes", "--unreleased"]).is_ok());
        assert!(parse(&["notes", "--unreleased", "--package", "a"]).is_err());
        assert!(parse(&["notes"]).is_err());
        assert!(parse(&["close", "--date", "13.11.2026"]).is_err());
        // The command lines of release-plz.yml and the release-files action.
        assert!(parse(&["notes", "--unfinished", "--out", "x"]).is_ok());
        assert!(parse(&["notes", "--unreleased", "--out", "x"]).is_ok());
        assert!(
            parse(&[
                "close",
                "--date",
                "2026-11-14",
                "--base",
                "origin/main",
                "--body",
                "b.md"
            ])
            .is_ok()
        );
        assert!(parse(&["dist", "--out", "x", "--package", "a", "--package", "b"]).is_ok());
        assert!(parse(&["dist", "--out", "x"]).is_ok());
        assert!(parse(&["pin", "v1.0.0-alpha.15"]).is_ok());
        assert!(parse(&["built-for", "v1.0.0-alpha.15"]).is_ok());
        assert!(parse(&["built-for"]).is_err());
        let Task::WineRun { args } = parse(&["wine-run", "a.exe", "--nocapture"]).unwrap().task
        else {
            panic!("not wine-run");
        };
        assert_eq!(args, ["a.exe", "--nocapture"]);
    }
}
