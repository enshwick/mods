//! `cargo xtask dist`: builds the release DLLs and writes what a player
//! downloads: `<package>.zip` with `mods/<id>/<package>.dll` and the
//! license, and the PDB beside it, for every mod in the workspace, or with
//! `--package` for the named ones only.

use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};

use crate::commands::check::{Step, WINDOWS_TARGET};
use crate::{Package, packages, say};

/// The command line `dist` takes.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The mods to package; every mod of the workspace when left out.
    #[arg(long)]
    package: Vec<String>,
    /// The folder to write into.
    #[arg(long, default_value = "target/dist")]
    out: PathBuf,
}

/// Builds the release DLLs and writes each mod's archive and PDB into
/// `--out`; with `--package`, only those mods.
///
/// # Errors
///
/// When a named package is not a mod of the workspace, or a file does not
/// read or write.
pub(crate) fn run(args: &Args) -> Result<bool, String> {
    let out = &args.out;
    let chosen: Vec<&str> = args.package.iter().map(String::as_str).collect();
    let mods: Vec<Package> = packages()?
        .into_iter()
        .filter(|p| p.cdylib && (chosen.is_empty() || chosen.contains(&p.name.as_str())))
        .collect();
    if let Some(missing) = chosen
        .iter()
        .find(|name| !mods.iter().any(|p| p.name == **name))
    {
        return Err(format!("{missing} is not a mod of this workspace"));
    }
    let mut command: Vec<&str> = Vec::new();
    // CI's dev builds embed the dependency tree in each DLL, for
    // `cargo audit bin` and other scanners.
    if std::env::var_os("ENSHWICK_AUDITABLE").is_some_and(|value| value == "1") {
        command.push("auditable");
    }
    if !cfg!(windows) {
        command.push("xwin");
    }
    command.extend(["build", "--release", "--locked", "--target", WINDOWS_TARGET]);
    if chosen.is_empty() {
        command.push("--workspace");
    } else {
        for name in &chosen {
            command.extend(["--package", name]);
        }
    }
    if !Step::new(&command).run(false) {
        return Ok(false);
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let license = read(Path::new("LICENSE"))?;
    for package in &mods {
        let name = &package.name;
        let built = Path::new("target")
            .join(WINDOWS_TARGET)
            .join("release")
            .join(name.replace('-', "_"));
        let entries = [
            (
                format!("mods/{}/{name}.dll", package.id),
                read(&built.with_extension("dll"))?,
            ),
            ("LICENSE".to_owned(), license.clone()),
        ];
        let archive = out.join(format!("{name}.zip"));
        std::fs::write(&archive, zip(&entries)?)
            .map_err(|e| format!("{}: {e}", archive.display()))?;
        let pdb = out.join(format!("{name}.pdb"));
        std::fs::copy(built.with_extension("pdb"), &pdb)
            .map_err(|e| format!("{}: {e}", pdb.display()))?;
        say(&format!(
            "wrote {} and {}",
            archive.display(),
            pdb.display()
        ));
    }
    Ok(true)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The archive of `entries` in their order and with the fixed time of the
/// format's epoch, so the same files give the same bytes.
fn zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for (path, content) in entries {
        archive
            .start_file(path.as_str(), options)
            .and_then(|()| archive.write_all(content).map_err(Into::into))
            .map_err(|e| format!("{path}: {e}"))?;
    }
    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|e| e.to_string())
}
