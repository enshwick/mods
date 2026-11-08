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
    let members: Vec<&str> = packages
        .iter()
        .filter_map(|package| package["name"].as_str())
        .collect();
    let shipping: Vec<&str> = packages
        .iter()
        .filter(|package| {
            let manifest = Path::new(package["manifest_path"].as_str().unwrap_or_default());
            crate::notes::for_players(
                manifest.parent().unwrap_or(Path::new(".")),
                builds_a_dll(package),
            )
        })
        .filter_map(|package| package["name"].as_str())
        .collect();
    let release_config = crate::notes::release_config(root)?;
    let held = crate::notes::held_back(&release_config);
    problems.extend(release_entries(&members, &shipping, &release_config));
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
        problems.extend(text_apostrophes(name, dir)?);
        let ships = shipping.contains(&name) && !held.iter().any(|held| held == name);
        problems.extend(changelog_problem(name, dir, ships));
    }
    Ok(problems)
}

/// A mod players get without a changelog, which its release notes come
/// from (`cargo xtask notes`).
fn changelog_problem(name: &str, dir: &Path, shipping: bool) -> Option<String> {
    (shipping && !dir.join(crate::changelog::CHANGELOG).is_file())
        .then(|| format!("{name}: no CHANGELOG.md; a mod players get keeps one by hand"))
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

/// A platform crate in `[workspace.dependencies]` or a `[patch]` table of
/// the root `manifest` not pinned to a full commit.
fn platform_pins(manifest: &toml::Table) -> Vec<String> {
    let workspace = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table);
    let patches = manifest
        .get("patch")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(_, table)| table.as_table());
    workspace
        .into_iter()
        .chain(patches)
        .flatten()
        .filter(|(_, spec)| from_the_platform(spec))
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

/// Whether the dependency `spec` takes the platform from git, its URL
/// written with or without `.git` and a trailing slash.
pub(crate) fn from_the_platform(spec: &toml::Value) -> bool {
    spec.get("git")
        .and_then(toml::Value::as_str)
        .is_some_and(|url| {
            let url = url.trim_end_matches('/');
            url.strip_suffix(".git").unwrap_or(url) == PLATFORM
        })
}

/// What `release-plz.toml` leaves out or gets wrong: every workspace member
/// has an entry there, released or `release = false`, so release-plz never
/// releases a developer mod by default; every entry names a member; and a
/// released one is a mod players get, whose notes `cargo xtask notes` writes.
fn release_entries(members: &[&str], shipping: &[&str], entries: &[toml::Table]) -> Vec<String> {
    fn name(entry: &toml::Table) -> Option<&str> {
        entry.get("name").and_then(toml::Value::as_str)
    }
    let mut problems: Vec<String> = members
        .iter()
        .filter_map(|member| {
            let Some(entry) = entries.iter().find(|entry| name(entry) == Some(*member)) else {
                return Some(format!(
                    "{member}: no [[package]] in release-plz.toml; name it there, with `release = false` unless players get it"
                ));
            };
            let released = entry.get("release").and_then(toml::Value::as_bool) != Some(false);
            (released && !shipping.contains(member)).then(|| {
                format!(
                    "{member}: release-plz.toml releases it, but players get only a mod with docs/player.md; set `release = false`"
                )
            })
        })
        .collect();
    problems.extend(
        entries
            .iter()
            .map(|entry| name(entry).unwrap_or_default())
            .filter(|entry| !members.contains(entry))
            .map(|entry| format!("release-plz.toml: {entry:?} is no workspace member")),
    );
    problems
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
        .filter(|(_, spec)| from_the_platform(spec))
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

/// Whether a Rust file under `dir` names `FakeHost` outside comments and
/// literals. A folder with a `Cargo.toml` of its own, and `target`, belong
/// to no package here.
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
                let own = path.join("Cargo.toml").is_file() || path.ends_with("target");
                if !own {
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

/// Whether the Rust `text` names `FakeHost` outside comments and literals.
fn names_the_fake_host(text: &str) -> bool {
    crate::crate_rules::code(text)
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| word == "FakeHost")
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

/// Apostrophes other than `'`, the one the game's Ukrainian texts use: 2,074
/// times, U+2019 261 times and the others never (game texts of build 1269579).
/// The game font draws U+02BC as a box (client 26430625).
const OTHER_APOSTROPHES: [char; 9] = [
    '\u{2018}', '\u{2019}', '\u{201B}', '\u{02B9}', '\u{02BB}', '\u{02BC}', '\u{02BD}', '\u{2032}',
    '\u{FF07}',
];

const OTHER_APOSTROPHE: &str = "an apostrophe other than ', the one the game's Ukrainian texts use";

/// A mod's texts, which `texts!()` reads and the game draws.
const TEXTS: &str = "src/texts.toml";

/// The Ukrainian texts of a mod with an apostrophe other than `'`, which the
/// game font draws; web pages keep U+02BC and other languages their own
/// typography.
fn text_apostrophes(name: &str, dir: &Path) -> Result<Vec<String>, String> {
    let texts = dir.join(TEXTS);
    if !texts.is_file() {
        return Ok(Vec::new());
    }
    let table = manifest(&texts)?;
    let Some(uk) = table.get("uk").and_then(toml::Value::as_table) else {
        return Ok(Vec::new());
    };
    Ok(uk
        .iter()
        .filter(|(_, text)| {
            text.as_str()
                .is_some_and(|text| text.contains(OTHER_APOSTROPHES))
        })
        .map(|(key, _)| format!("{name}: {TEXTS}: uk.{key}: {OTHER_APOSTROPHE}"))
        .collect())
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
        let suffixed: toml::Table = format!(
            "[workspace.dependencies]\nenshwick = {{ git = \"{PLATFORM}.git\", rev = \"main\" }}\n\
             [patch.\"{PLATFORM}/\"]\nenshwick = {{ git = \"{PLATFORM}\", branch = \"main\" }}\n"
        )
        .parse()
        .unwrap();
        assert_eq!(
            platform_pins(&suffixed),
            [
                "enshwick: the platform is pinned by `rev` to a full commit hash",
                "enshwick: the platform is pinned by `rev` to a full commit hash",
            ]
        );
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
    fn every_member_has_a_release_entry_and_only_a_shipped_mod_is_released() {
        let entries = crate::notes::entries(
            "[[package]]\nname = \"loot\"\n\
             [[package]]\nname = \"probe\"\nrelease = false\n\
             [[package]]\nname = \"tool\"\n\
             [[package]]\nname = \"gone\"\nrelease = false\n",
        )
        .unwrap();
        assert_eq!(
            release_entries(&["loot", "probe", "tool", "content"], &["loot"], &entries),
            [
                "tool: release-plz.toml releases it, but players get only a mod with docs/player.md; set `release = false`",
                "content: no [[package]] in release-plz.toml; name it there, with `release = false` unless players get it",
                "release-plz.toml: \"gone\" is no workspace member",
            ]
        );
        assert!(release_entries(&["loot", "probe"], &["loot"], &entries[..2]).is_empty());
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
        assert!(!names_the_fake_host("/* FakeHost */ fn f() {}"));
        assert!(!names_the_fake_host("const S: &str = \"FakeHost\";"));
        assert!(names_the_fake_host(
            "let url = \"https://example.com\"; let loader = FakeHost::new();"
        ));
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

    #[test]
    fn only_a_mod_players_get_needs_a_changelog() {
        let dir = std::env::temp_dir().join(format!("xtask-changelog-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        let without = changelog_problem("m", &dir, true);
        let unshipped = changelog_problem("m", &dir, false);
        std::fs::write(dir.join(crate::changelog::CHANGELOG), "## [Unreleased]\n").unwrap();
        let with = changelog_problem("m", &dir, true);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(without.is_some());
        assert_eq!(unshipped, None);
        assert_eq!(with, None);
    }

    #[test]
    fn only_ukrainian_texts_with_another_apostrophe_are_problems() {
        let dir = std::env::temp_dir().join(format!("xtask-apostrophes-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let none = text_apostrophes("a", &dir);
        std::fs::write(
            dir.join("src/texts.toml"),
            "# don\u{2019}t\n[en]\n\"a\" = \"it\u{2019}s\"\n[fr]\n\"a\" = \"l\u{2019}arbre\"\n\
             [uk]\n\"a\" = \"здоров'я\"\n\"b\" = \"ім\u{2019}я\"\n\"c\" = \"з\u{02BC}єднуються\"\n\
             \"d\" = \"ім\\u02BCя\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("docs/player.uk.md"), "пам\u{02BC}ять\n").unwrap();
        let problems = text_apostrophes("a", &dir);
        std::fs::write(dir.join("src/texts.toml"), [0xFF]).unwrap();
        let unreadable = text_apostrophes("a", &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(none.unwrap(), Vec::<String>::new());
        assert!(unreadable.is_err());
        let problem = OTHER_APOSTROPHE;
        assert_eq!(
            problems.unwrap(),
            [
                format!("a: src/texts.toml: uk.b: {problem}"),
                format!("a: src/texts.toml: uk.c: {problem}"),
                format!("a: src/texts.toml: uk.d: {problem}"),
            ]
        );
    }
}
