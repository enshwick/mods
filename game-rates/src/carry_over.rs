//! The settings of the mods this one replaces, carried into its own once.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use enshwick::{Load, Off};

/// A mod this one replaces, with the key each of its settings becomes.
#[derive(Debug)]
pub(crate) struct Replaced {
    /// Its id, which names its settings file and data folder.
    pub(crate) id: &'static str,
    /// Its keys and the keys of this mod they become.
    pub(crate) keys: &'static [(&'static str, &'static str)],
}

/// The recipe settings of the older recipe mods, under their new keys.
const RECIPE_KEYS: &[(&str, &str)] = &[
    ("multiplier", "recipe_multiplier"),
    ("blocks", "recipe_blocks"),
    ("overgrowth", "recipe_overgrowth"),
    ("decor", "recipe_decor"),
];

/// The mods this one replaces, the manifest's `replaces`, the newest first:
/// a key one of them gives is not taken again from those after it.
pub(crate) const REPLACED: [Replaced; 3] = [
    Replaced {
        id: "building-materials",
        keys: RECIPE_KEYS,
    },
    Replaced {
        id: "building-yield",
        keys: RECIPE_KEYS,
    },
    Replaced {
        id: "furniture-comfort",
        keys: &[("multiplier", "comfort_multiplier")],
    },
];

/// The folder of the mods' settings files, beside the game.
const CONFIGS: &str = "mod-configs";
/// The folder of a mod's world copies, in its data folder.
const WORLDS: &str = "worlds";
/// The file name of a world's copy of a mod's settings.
const WORLD_SETTINGS: &str = "settings.toml";

/// The settings `own` with each key it lacks taken from the replaced mods'
/// files `old`, then from `fallback`, written over `template`; with the ids
/// of the mods a key came from. `None` when no replaced mod gives a key.
pub(crate) fn carried_over(
    template: &str,
    own: &toml::Table,
    old: &[(&Replaced, toml::Table)],
    fallback: &toml::Table,
) -> Option<(String, Vec<&'static str>)> {
    let mut values = own.clone();
    let mut from = Vec::new();
    for (replaced, table) in old {
        for &(old_key, key) in replaced.keys {
            if values.contains_key(key) {
                continue;
            }
            if let Some(value) = table.get(old_key) {
                values.insert(key.to_owned(), value.clone());
                if !from.contains(&replaced.id) {
                    from.push(replaced.id);
                }
            }
        }
    }
    if from.is_empty() {
        return None;
    }
    for (key, value) in fallback {
        values.entry(key).or_insert_with(|| value.clone());
    }
    let lines = template.lines().map(|line| {
        line.split_once(" = ")
            .and_then(|(key, _)| Some(format!("{key} = {}", values.get(key)?)))
            .unwrap_or_else(|| line.to_owned())
    });
    Some((lines.map(|line| line + "\n").collect(), from))
}

/// Carries the replaced mods' settings into the settings file of mod `id`
/// and into each world's copy of it, before the mod reads them. A world
/// copy the mod does not have yet starts from its settings file.
///
/// # Errors
///
/// When the loader does not name the mod's data folder.
pub(crate) fn run(load: &Load<'_>, id: &str, template: &str) -> Result<(), Off> {
    // `<game>/mod-data/<id>` beside `<game>/mod-configs/`.
    let data = load.data_dir()?;
    let Some((mod_data, game)) = data.parent().and_then(|d| Some((d, d.parent()?))) else {
        return Ok(());
    };
    let configs = game.join(CONFIGS);
    let settings = configs.join(format!("{id}.toml"));
    carry_into(&settings, template, &toml::Table::new(), |replaced| {
        configs.join(format!("{}.toml", replaced.id))
    });
    let fallback = std::fs::read_to_string(&settings)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    let worlds = |id: &str| mod_data.join(id).join(WORLDS);
    let keys: BTreeSet<OsString> = std::iter::once(id)
        .chain(REPLACED.iter().map(|replaced| replaced.id))
        .filter_map(|id| std::fs::read_dir(worlds(id)).ok())
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name())
        .collect();
    for key in keys {
        let own = data.join(WORLDS).join(&key).join(WORLD_SETTINGS);
        carry_into(&own, template, &fallback, |replaced| {
            worlds(replaced.id).join(&key).join(WORLD_SETTINGS)
        });
    }
    Ok(())
}

/// Carries the replaced mods' settings, each at `old_path`, into the file at
/// `own_path`, which is made when it does not exist yet.
fn carry_into(
    own_path: &Path,
    template: &str,
    fallback: &toml::Table,
    old_path: impl Fn(&Replaced) -> PathBuf,
) {
    let own = match std::fs::read_to_string(own_path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(own) => own,
            // The mod's own read reports it.
            Err(_) => return,
        },
        Err(e) if e.kind() == ErrorKind::NotFound => toml::Table::new(),
        Err(_) => return,
    };
    let old: Vec<_> = REPLACED
        .iter()
        .filter_map(|replaced| {
            let path = old_path(replaced);
            let text = std::fs::read_to_string(&path).ok()?;
            match toml::from_str(&text) {
                Ok(table) => Some((replaced, table)),
                Err(e) => {
                    enshwick::warn!("{} does not parse, not carried over: {e}", path.display());
                    None
                }
            }
        })
        .collect();
    let Some((text, from)) = carried_over(template, &own, &old, fallback) else {
        return;
    };
    let from = from.join(", ");
    let written = own_path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(own_path, text));
    match written {
        Ok(()) => enshwick::info!(
            "settings of {from} carried over into {}",
            own_path.display()
        ),
        Err(e) => enshwick::warn!(
            "settings of {from} not carried over into {}: {e}",
            own_path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "# Output.\nrecipe_multiplier = 2\nrecipe_blocks = true\nrecipe_overgrowth = true\nrecipe_decor = true\n# Comfort.\ncomfort_multiplier = 2.0\n";

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn a_first_start_takes_every_old_file_under_the_new_keys() {
        let old = [
            (
                &REPLACED[1],
                table("multiplier = 3\ndecor = false\nown_workbench = true\n"),
            ),
            (&REPLACED[2], table("multiplier = 4.5\n")),
        ];
        let (text, from) =
            carried_over(TEMPLATE, &toml::Table::new(), &old, &toml::Table::new()).unwrap();
        assert_eq!(
            text,
            "# Output.\nrecipe_multiplier = 3\nrecipe_blocks = true\nrecipe_overgrowth = true\nrecipe_decor = false\n# Comfort.\ncomfort_multiplier = 4.5\n"
        );
        assert_eq!(from, ["building-yield", "furniture-comfort"]);
    }

    #[test]
    fn building_materials_wins_over_building_yield() {
        let old = [
            (&REPLACED[0], table("multiplier = 5\nblocks = false\n")),
            (&REPLACED[1], table("multiplier = 3\novergrowth = false\n")),
        ];
        let (text, from) =
            carried_over(TEMPLATE, &toml::Table::new(), &old, &toml::Table::new()).unwrap();
        assert!(
            text.contains(
                "recipe_multiplier = 5\nrecipe_blocks = false\nrecipe_overgrowth = false\n"
            ),
            "{text}"
        );
        assert_eq!(from, ["building-materials", "building-yield"]);
    }

    #[test]
    fn a_new_world_copy_starts_from_the_settings_file() {
        let old = [(&REPLACED[2], table("multiplier = 0.5\n"))];
        let fallback = table("recipe_multiplier = 7\ncomfort_multiplier = 3.0\n");
        let (text, _) = carried_over(TEMPLATE, &toml::Table::new(), &old, &fallback).unwrap();
        assert!(text.contains("recipe_multiplier = 7\n"), "{text}");
        assert!(text.contains("comfort_multiplier = 0.5\n"), "{text}");
    }

    #[test]
    fn a_set_key_is_never_overwritten() {
        let own = table(TEMPLATE);
        let old = [(&REPLACED[2], table("multiplier = 9.0\n"))];
        assert!(carried_over(TEMPLATE, &own, &old, &toml::Table::new()).is_none());
        let none = toml::Table::new();
        assert!(carried_over(TEMPLATE, &none, &[], &table(TEMPLATE)).is_none());
    }
}
