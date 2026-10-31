//! The rules this repository's mods are held to, read from `cargo metadata`,
//! the manifests and the tree without compiling. A mod is a package that
//! builds a DLL, the developer mods of `devtools/` included.

use std::path::Path;
use std::process::Command;

/// Where the platform crates come from.
const PLATFORM: &str = "https://github.com/enshwick/enshwick";
/// The dependency tables of a manifest.
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// The problems of the workspace's mods, one line each.
///
/// # Errors
///
/// When `cargo metadata` does not run, or a manifest or
/// `rust-toolchain.toml` does not read.
pub(crate) fn check() -> Result<Vec<String>, String> {
    let output = Command::new(crate::cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    let root = Path::new(metadata["workspace_root"].as_str().unwrap_or("."));
    let toolchain = toolchain(&manifest(&root.join("rust-toolchain.toml"))?)
        .ok_or("rust-toolchain.toml names no channel")?;
    let mut problems = platform_pins(&manifest(&root.join("Cargo.toml"))?);
    let packages: Vec<&serde_json::Value> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let mods: Vec<&str> = packages
        .iter()
        .filter(|package| builds_a_dll(package))
        .filter_map(|package| package["name"].as_str())
        .collect();
    for package in packages {
        let name = package["name"].as_str().unwrap_or_default();
        let path = Path::new(package["manifest_path"].as_str().unwrap_or_default());
        let dir = path.parent().unwrap_or(Path::new("."));
        problems.extend(feature_names(name, package));
        problems.extend(rust_version(name, package, &toolchain));
        problems.extend(platform_outside_workspace(name, &manifest(path)?));
        if !mods.contains(&name) {
            continue;
        }
        problems.extend(anyhow(name, package));
        problems.extend(other_mods(name, package, &mods));
        if !installs_the_fake_host(dir)? {
            problems.push(format!(
                "{name}: no test installs the fake host; add a lifecycle test"
            ));
        }
        problems.extend(docs_files(name, &dir.join("docs"))?);
    }
    Ok(problems)
}

fn manifest(path: &Path) -> Result<toml::Table, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether `package` builds a DLL: a mod.
fn builds_a_dll(package: &serde_json::Value) -> bool {
    package["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "cdylib"))
        })
}

/// The major and minor of the toolchain `rust-toolchain.toml` pins, such as
/// `1.99`.
fn toolchain(file: &toml::Table) -> Option<String> {
    let channel = file.get("toolchain")?.get("channel")?.as_str()?;
    let mut parts = channel.split('.');
    Some(format!("{}.{}", parts.next()?, parts.next()?))
}

/// A feature named for how it is added rather than what it adds.
fn feature_names(name: &str, package: &serde_json::Value) -> Vec<String> {
    package["features"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(feature, _)| feature)
        .filter(|feature| {
            ["use-", "use_", "with-", "with_"]
                .iter()
                .any(|prefix| feature.starts_with(prefix))
        })
        .map(|feature| {
            format!("{name}: the feature {feature} names what it adds, without use- or with-")
        })
        .collect()
}

/// A `rust-version` other than the pinned toolchain's.
fn rust_version(name: &str, package: &serde_json::Value, toolchain: &str) -> Option<String> {
    let version = package["rust_version"].as_str().unwrap_or_default();
    let same = version == toolchain
        || version
            .strip_prefix(toolchain)
            .is_some_and(|rest| rest.starts_with('.'));
    (!same).then(|| {
        format!("{name}: rust-version {version:?} is not the pinned toolchain's {toolchain}")
    })
}

/// A platform crate in `[workspace.dependencies]` of the root `manifest`
/// not pinned to a full commit.
fn platform_pins(manifest: &toml::Table) -> Vec<String> {
    manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter(|(_, spec)| spec.get("git").and_then(toml::Value::as_str) == Some(PLATFORM))
        .filter(|(_, spec)| {
            !spec
                .get("rev")
                .and_then(toml::Value::as_str)
                .is_some_and(|rev| rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        .map(|(crate_name, _)| {
            format!("{crate_name}: the platform is pinned by `rev` to a full commit hash")
        })
        .collect()
}

/// A platform crate a member's `manifest` takes from git itself rather than
/// from `[workspace.dependencies]`.
fn platform_outside_workspace(name: &str, manifest: &toml::Table) -> Vec<String> {
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(_, target)| target.as_table());
    std::iter::once(manifest)
        .chain(targets)
        .flat_map(|table| {
            DEPENDENCY_TABLES
                .iter()
                .filter_map(|key| table.get(*key).and_then(toml::Value::as_table))
        })
        .flatten()
        .filter(|(_, spec)| spec.get("git").and_then(toml::Value::as_str) == Some(PLATFORM))
        .map(|(dependency, _)| {
            format!(
                "{name}: {dependency} comes from [workspace.dependencies], the one platform pin"
            )
        })
        .collect()
}

/// `anyhow` as a normal dependency of a mod, which is a library.
fn anyhow(name: &str, package: &serde_json::Value) -> Option<String> {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|dependency| dependency["name"] == "anyhow" && dependency["kind"].is_null())
        .then(|| format!("{name}: a mod returns typed errors; anyhow is for binaries"))
}

/// A dependency of a mod on another mod of the workspace.
fn other_mods(name: &str, package: &serde_json::Value, mods: &[&str]) -> Vec<String> {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|dependency| dependency["name"].as_str())
        .filter(|dependency| *dependency != name && mods.contains(dependency))
        .map(|dependency| {
            format!("{name}: depends on the mod {dependency}; mods meet only at extension points")
        })
        .collect()
}

/// Whether a Rust file under `dir` names `FakeHost` outside a `//` comment.
fn installs_the_fake_host(dir: &Path) -> Result<bool, String> {
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let entries =
            std::fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|e| format!("{}: {e}", folder.display()))?
                .path();
            if path.is_dir() {
                if !path.ends_with("target") {
                    folders.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if names_the_fake_host(&text) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// Whether the Rust `text` names `FakeHost` outside a `//` comment.
fn names_the_fake_host(text: &str) -> bool {
    text.lines().any(|line| {
        line.split_once("//")
            .map_or(line, |(code, _)| code)
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| word == "FakeHost")
    })
}

/// The entries of a mod's `docs` folder other than its player pages and
/// their images.
fn docs_files(name: &str, docs: &Path) -> Result<Vec<String>, String> {
    let Ok(entries) = std::fs::read_dir(docs) else {
        return Ok(Vec::new());
    };
    let mut problems = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", docs.display()))?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if !player_doc(&file, entry.path().is_dir()) {
            problems.push(format!(
                "{name}: docs/{file} is not a player page; a mod's docs hold player.md, its translations and images/"
            ));
        }
    }
    Ok(problems)
}

/// Whether `file` in a mod's `docs` belongs there: `player.md`,
/// `player.<language>.md` or the `images` folder.
fn player_doc(file: &str, is_dir: bool) -> bool {
    if is_dir {
        return file == "images";
    }
    file.strip_prefix("player.")
        .and_then(|rest| rest.strip_suffix("md"))
        .is_some_and(|language| {
            language.is_empty()
                || language.strip_suffix('.').is_some_and(|language| {
                    !language.is_empty()
                        && language.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_named_for_how_it_is_added_is_a_problem() {
        let package = serde_json::json!({
            "features": { "use-luau": [], "with_extra": [], "luau": [], "user-texts": [] },
        });
        assert_eq!(
            feature_names("a", &package),
            [
                "a: the feature use-luau names what it adds, without use- or with-",
                "a: the feature with_extra names what it adds, without use- or with-",
            ]
        );
    }

    #[test]
    fn a_rust_version_off_the_toolchain_is_a_problem() {
        let toolchain = toolchain(&"[toolchain]\nchannel = \"1.99.1\"\n".parse().unwrap());
        assert_eq!(toolchain.as_deref(), Some("1.99"));
        let at = |version: &str| serde_json::json!({ "rust_version": version });
        assert!(rust_version("a", &at("1.99"), "1.99").is_none());
        assert!(rust_version("a", &at("1.99.1"), "1.99").is_none());
        assert!(rust_version("a", &at("1.98"), "1.99").is_some());
        assert!(rust_version("a", &at("1.990"), "1.99").is_some());
        assert!(rust_version("a", &serde_json::json!({}), "1.99").is_some());
    }

    #[test]
    fn the_platform_is_pinned_to_a_full_commit_in_the_workspace_only() {
        let root: toml::Table = format!(
            "[workspace.dependencies]\n\
             enshwick = {{ git = \"{PLATFORM}\", rev = \"{}\" }}\n\
             enshwick-gamedata = {{ git = \"{PLATFORM}\", rev = \"21fed9fb\" }}\n\
             enshwick-macros = {{ git = \"{PLATFORM}\", branch = \"main\" }}\n\
             serde = \"1\"\n",
            "a".repeat(40)
        )
        .parse()
        .unwrap();
        assert_eq!(
            platform_pins(&root),
            [
                "enshwick-gamedata: the platform is pinned by `rev` to a full commit hash",
                "enshwick-macros: the platform is pinned by `rev` to a full commit hash",
            ]
        );
        let member: toml::Table = format!(
            "[dependencies]\nenshwick.workspace = true\n\
             [target.'cfg(windows)'.dev-dependencies]\n\
             enshwick-gamedata = {{ git = \"{PLATFORM}\", rev = \"{}\" }}\n",
            "b".repeat(40)
        )
        .parse()
        .unwrap();
        assert_eq!(
            platform_outside_workspace("a", &member),
            ["a: enshwick-gamedata comes from [workspace.dependencies], the one platform pin"]
        );
    }

    #[test]
    fn anyhow_and_another_mod_as_dependencies_of_a_mod_are_problems() {
        let package = serde_json::json!({
            "dependencies": [
                { "name": "anyhow", "kind": null },
                { "name": "enshwick-area-loot", "kind": "dev" },
                { "name": "enshwick", "kind": null },
            ],
        });
        assert_eq!(
            anyhow("a", &package).as_deref(),
            Some("a: a mod returns typed errors; anyhow is for binaries")
        );
        assert_eq!(
            other_mods("a", &package, &["a", "enshwick-area-loot"]),
            ["a: depends on the mod enshwick-area-loot; mods meet only at extension points"]
        );
        let dev = serde_json::json!({ "dependencies": [{ "name": "anyhow", "kind": "dev" }] });
        assert!(anyhow("a", &dev).is_none());
    }

    #[test]
    fn the_fake_host_counts_in_code_not_in_comments() {
        assert!(names_the_fake_host(
            "let loader = enshwick::test::FakeHost::new().install();"
        ));
        assert!(!names_the_fake_host("// a FakeHost test would go here"));
        assert!(!names_the_fake_host("struct FakeHostish;"));
    }

    #[test]
    fn a_mod_s_docs_hold_its_player_pages_and_images_only() {
        assert!(player_doc("player.md", false));
        assert!(player_doc("player.uk.md", false));
        assert!(player_doc("player.pt-br.md", false));
        assert!(player_doc("images", true));
        assert!(!player_doc("developer.md", false));
        assert!(!player_doc("player..md", false));
        assert!(!player_doc("player.UK.md", false));
        assert!(!player_doc("notes", true));
        assert!(!player_doc("images", false));
    }
}
