//! Game rates: the vanilla recipes for building blocks, roofs, overgrowth,
//! decor and furniture make more for the same cost, and furniture gives more
//! comfort. Functional props (stations, chests, altars, water devices) stay
//! as they are.

use enshwick::{Load, Mod, Off};

mod comfort;
mod migrations;
mod recipes;

/// The labels and descriptions of the settings, a table per language.
const TEXTS: &str = include_str!("texts.toml");

/// Game rates: building recipes, then furniture comfort.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    /// The vanilla building recipes: how much more they make, and of what.
    #[config(group)]
    recipes: Recipes,
    /// Comfort furniture gives inside a base.
    #[config(group)]
    comfort: Comfort,
}

/// How much more the vanilla building recipes make, and of what.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Recipes {
    /// The vanilla building recipes make their output times this, for the
    /// same cost. A joined player runs with the server's value.
    #[config(sync, range = 2..=100)]
    multiplier: u32,
    /// Building blocks and roofs make more.
    #[config(sync)]
    blocks: bool,
    /// Overgrowth materials make more.
    #[config(sync)]
    overgrowth: bool,
    /// Decor and furniture make more.
    #[config(sync)]
    decor: bool,
}

impl Default for Recipes {
    fn default() -> Self {
        Self {
            multiplier: 2,
            blocks: true,
            overgrowth: true,
            decor: true,
        }
    }
}

/// How much comfort furniture gives.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Comfort {
    /// Comfort each piece of furniture gives inside a base, times this, from
    /// 0 to 100: 1 is vanilla, 0 takes all comfort from furniture. The
    /// server's value decides.
    #[config(runtime, range = 0.0..=comfort::MAX_FACTOR)]
    multiplier: f32,
}

impl Default for Comfort {
    fn default() -> Self {
        Self { multiplier: 2.0 }
    }
}

impl Settings {
    /// Puts these settings in force; whether the recipe values changed.
    fn apply(&self) -> bool {
        let Recipes {
            multiplier,
            blocks,
            overgrowth,
            decor,
        } = self.recipes;
        let changed = recipes::rates().set(multiplier, [blocks, overgrowth, decor]);
        comfort::rate().set(self.comfort.multiplier);
        enshwick::info!(
            "recipe output x{multiplier}: blocks {blocks}, overgrowth {overgrowth}, decor {decor}; furniture comfort x{}",
            self.comfort.multiplier
        );
        changed
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct GameRates;

impl Mod for GameRates {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "enshwick.game-rates",
            side: BOTH,
            name: "Game Rates",
            join: true,
            replaces: ["building-materials", "building-yield", "furniture-comfort"],
            was: ["game-rates"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.texts(TEXTS)?;
        load.migrate(migrations::all())?;
        let settings: Settings = load.config()?;
        settings.apply();
        // Values that arrive later, an edited file or a joined server's
        // values and the file's again on leaving, rebuild the registry the
        // game already holds when they change a recipe.
        load.on_config_change(|settings: Settings| {
            if !settings.apply() {
                return;
            }
            match enshwick::content::recipes::rebuild_recipes() {
                Ok(()) => enshwick::info!("recipe counts follow the new values"),
                Err(off) => enshwick::warn!(
                    "recipes already built keep their counts until the next start: {off}"
                ),
            }
        })?;
        load.modify_recipes(|recipe| recipes::rates().scale(recipe))?;
        if let Err(off) = load.on_comfort(|provider| comfort::rate().scale(provider.value)) {
            enshwick::warn!("furniture comfort stays vanilla: {off}");
        }
        Ok(Self)
    }
}

enshwick::declare_mod!(GameRates);

#[cfg(test)]
mod tests {
    use enshwick::config::Config as _;

    use super::*;

    /// A host with the recipe layout the recipe patch reads.
    fn recipe_host() -> enshwick::test::FakeHost {
        let mut host = enshwick::test::FakeHost::new()
            .field_offset("keen::RecipeRegistryResource", "recipes", 8)
            .type_size("keen::RecipeInfo", 0xa8)
            .type_size("keen::RecipeInputResource", 48)
            .type_size("keen::RecipeItemStackResource", 24)
            .field_offset("keen::RecipeItemStackResource", "item", 0x10)
            .field_offset("keen::RecipeItemStackResource", "count", 0x14);
        for (field, at) in [
            ("recipeGuid", 0),
            ("recipeId", 0x10),
            ("workshopGuid", 0x14),
            ("workshopId", 0x24),
            ("input", 0x28),
            ("output", 0x30),
            ("requiredProps", 0x40),
            ("completionRequirementQuery", 0x5c),
            ("debugName", 0x78),
        ] {
            host = host.field_offset("keen::RecipeInfo", field, at);
        }
        host
    }

    fn comfort(value: i32) -> enshwick::abi::Comfort {
        enshwick::abi::Comfort {
            header: enshwick::abi::Header::of::<enshwick::abi::Comfort>(),
            category: 1,
            value,
            position: [0.0; 3],
        }
    }

    #[test]
    fn the_default_settings_parse_and_only_the_recipe_rates_sync() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.recipes.multiplier, 2);
        assert!(settings.recipes.blocks && settings.recipes.overgrowth && settings.recipes.decor);
        assert!((settings.comfort.multiplier - 2.0).abs() < f32::EPSILON);
        assert!(settings.validate().is_ok());
        let synced = settings.synced().unwrap();
        for key in ["multiplier", "blocks", "overgrowth", "decor"] {
            assert!(synced.contains(key), "{key} in {synced}");
        }
        assert!(!synced.contains("comfort"), "{synced}");
    }

    #[test]
    fn a_joined_server_s_values_and_leaving_rebuild_the_recipes() {
        use enshwick::abi::Status;
        use enshwick::events::SettingsChanged;

        let loader = recipe_host()
            .side(enshwick::abi::Side::CLIENT)
            .settings("[recipes]\nmultiplier = 2\nblocks = true\novergrowth = true\ndecor = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let allowed = loader.rebuilds().len();
        assert_eq!(allowed, 1, "the registry may be rebuilt");
        loader.set_server_synced(Some("{ recipes = { multiplier = 5 } }"));
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(recipes::rates().multiplier(), 5);
        assert_eq!(loader.rebuilds().len(), allowed + 1, "joined: rebuilt");
        loader.set_server_synced(None);
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(recipes::rates().multiplier(), 2);
        assert_eq!(loader.rebuilds().len(), allowed + 2, "left: rebuilt");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn furniture_comfort_follows_a_live_change_without_a_recipe_rebuild() {
        use enshwick::abi::Status;
        use enshwick::events::SettingsChanged;

        let loader = recipe_host().side(enshwick::abi::Side::SERVER).install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.comfort(&comfort(3)), 6, "twice by default");
        let rebuilds = loader.rebuilds().len();
        loader.set_settings("[comfort]\nmultiplier = 1.5\n");
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(loader.comfort(&comfort(3)), 5);
        assert_eq!(loader.rebuilds().len(), rebuilds, "no recipe changed");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn the_migrated_ids_are_the_manifest_s_former_and_replaced_ids() {
        use enshwick::abi::Relation;

        let manifest = GameRates::manifest();
        // SAFETY: `manifest!` points `dependencies` at a static array of
        // `dependency_count` entries.
        let all =
            unsafe { std::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) };
        let ids = |relation| -> Vec<&str> {
            all.iter()
                .filter(|dependency| dependency.relation == relation)
                // SAFETY: each id is a static string literal of `manifest!`.
                .filter_map(|dependency| unsafe { dependency.id.as_str() })
                .collect()
        };
        assert_eq!(ids(Relation::WAS), [migrations::FORMER_ID]);
        let mut folded: Vec<&str> = migrations::FOLDED.iter().map(|&(id, _)| id).collect();
        let mut replaced = ids(Relation::REPLACES);
        folded.sort_unstable();
        replaced.sort_unstable();
        assert_eq!(folded, replaced);
    }

    #[test]
    fn every_setting_has_its_label_and_description_in_all_languages() {
        let texts = enshwick::i18n::Texts::parse(TEXTS).unwrap();
        assert_eq!(texts.untranslated(), []);
        let schema = Settings::schema();
        let keys: Vec<String> = std::iter::once(schema.text.clone())
            .chain(schema.groups.iter().map(|group| group.text.clone()))
            .chain(schema.fields.iter().map(|field| field.text.clone()))
            .flat_map(|key| [format!("{key}.doc"), key])
            .filter(|key| key != "config.doc")
            .collect();
        let tables: toml::Table = toml::from_str(TEXTS).unwrap();
        for (language, table) in &tables {
            let table = table.as_table().unwrap();
            for key in &keys {
                assert!(table.contains_key(key), "{language}: {key}");
            }
            assert_eq!(table.len(), keys.len(), "{language}: no stale keys");
        }
    }

    #[test]
    fn rates_out_of_range_are_refused() {
        for recipe_multiplier in [0, 1, 101] {
            let settings = Settings {
                recipes: Recipes {
                    multiplier: recipe_multiplier,
                    ..Recipes::default()
                },
                ..Settings::default()
            };
            assert!(settings.validate().is_err(), "{recipe_multiplier}");
        }
        for comfort_multiplier in [-1.0, 100.5, 1e9, f32::INFINITY, f32::NAN] {
            let settings = Settings {
                comfort: Comfort {
                    multiplier: comfort_multiplier,
                },
                ..Settings::default()
            };
            assert!(settings.validate().is_err(), "{comfort_multiplier}");
        }
        for comfort_multiplier in [0.0, 100.0] {
            let settings = Settings {
                comfort: Comfort {
                    multiplier: comfort_multiplier,
                },
                ..Settings::default()
            };
            assert!(settings.validate().is_ok(), "{comfort_multiplier}");
        }
    }

    /// The loader installed with `others`, each a mod id and its settings
    /// file, and the mod started.
    fn started_with(others: &[(&str, &str)]) -> enshwick::test::Installed {
        let host = others.iter().fold(recipe_host(), |host, &(id, text)| {
            host.other_settings(id, text)
        });
        let loader = host.install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick::abi::Status::OK, "{:?}", loader.logs());
        loader
    }

    const FLAT: &str = "recipe_multiplier = 4\nrecipe_blocks = true\nrecipe_overgrowth = false\nrecipe_decor = true\ncomfort_multiplier = 3.0\n";

    #[test]
    fn the_former_id_s_flat_settings_move_into_their_groups() {
        let loader = started_with(&[(migrations::FORMER_ID, FLAT)]);
        let migrated = loader.migrate_settings(FLAT).unwrap();
        let settings: Settings = toml::from_str(&migrated).unwrap();
        assert_eq!(settings.recipes.multiplier, 4);
        assert!(!settings.recipes.overgrowth);
        assert!((settings.comfort.multiplier - 3.0).abs() < f32::EPSILON);
        assert!(!migrated.contains("recipe_"), "{migrated}");
    }

    #[test]
    fn the_folded_mods_settings_move_into_their_groups() {
        let loader = started_with(&[
            (
                "building-yield",
                "multiplier = 3\ndecor = false\nown_workbench = true\n",
            ),
            ("furniture-comfort", "multiplier = 4.5\n"),
        ]);
        let migrated = loader.migrate_settings("").unwrap();
        let settings: Settings = toml::from_str(&migrated).unwrap();
        assert_eq!(settings.recipes.multiplier, 3);
        assert!(!settings.recipes.decor);
        assert!(
            settings.recipes.blocks,
            "a key no mod gives keeps its default"
        );
        assert!((settings.comfort.multiplier - 4.5).abs() < f32::EPSILON);
    }

    #[test]
    fn building_materials_wins_over_building_yield() {
        let loader = started_with(&[
            ("building-materials", "multiplier = 5\nblocks = false\n"),
            (
                "building-yield",
                "multiplier = 3\nblocks = true\novergrowth = false\n",
            ),
        ]);
        let migrated = loader.migrate_settings("").unwrap();
        let settings: Settings = toml::from_str(&migrated).unwrap();
        assert_eq!(settings.recipes.multiplier, 5);
        assert!(!settings.recipes.blocks);
        assert!(!settings.recipes.overgrowth);
    }

    #[test]
    fn the_former_id_s_values_win_over_the_folded_mods() {
        let own = "recipe_multiplier = 7\n";
        let loader = started_with(&[
            ("building-materials", "multiplier = 5\n"),
            (migrations::FORMER_ID, own),
        ]);
        let settings: Settings = toml::from_str(&loader.migrate_settings(own).unwrap()).unwrap();
        assert_eq!(settings.recipes.multiplier, 7);
    }
}
