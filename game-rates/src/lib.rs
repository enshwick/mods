//! Game rates: the vanilla recipes for building blocks, roofs, overgrowth,
//! decor and furniture make more for the same cost, and furniture gives more
//! comfort. Functional props (stations, chests, altars, water devices) stay
//! as they are.

use enshwick::config::Config;
use enshwick::{Load, Mod, Off};

mod carry_over;
mod comfort;
mod recipes;

/// The mod's id, as in its manifest; it names the settings file.
const ID: &str = "game-rates";

/// The labels and descriptions of the settings, in English and Ukrainian.
const TEXTS: &str = include_str!("texts.toml");

/// Game rates: building recipes, then furniture comfort.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    /// The vanilla building recipes make their output times this, for the
    /// same cost. A joined player runs with the server's value.
    #[config(sync, range = 2..=100)]
    recipe_multiplier: u32,
    /// Building blocks and roofs make more.
    #[config(sync)]
    recipe_blocks: bool,
    /// Overgrowth materials make more.
    #[config(sync)]
    recipe_overgrowth: bool,
    /// Decor and furniture make more.
    #[config(sync)]
    recipe_decor: bool,
    /// Comfort each piece of furniture gives inside a base, times this, from
    /// 0 to 100: 1 is vanilla, 0 takes all comfort from furniture. The
    /// server's value decides.
    #[config(runtime, range = 0.0..=comfort::MAX_FACTOR)]
    comfort_multiplier: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recipe_multiplier: 2,
            recipe_blocks: true,
            recipe_overgrowth: true,
            recipe_decor: true,
            comfort_multiplier: 2.0,
        }
    }
}

impl Settings {
    /// Puts these settings in force; whether the recipe values changed.
    fn apply(&self) -> bool {
        let changed = recipes::RATES.set(
            self.recipe_multiplier,
            [
                self.recipe_blocks,
                self.recipe_overgrowth,
                self.recipe_decor,
            ],
        );
        comfort::RATE.set(self.comfort_multiplier);
        enshwick::info!(
            "recipe output x{}: blocks {}, overgrowth {}, decor {}; furniture comfort x{}",
            self.recipe_multiplier,
            self.recipe_blocks,
            self.recipe_overgrowth,
            self.recipe_decor,
            self.comfort_multiplier
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
            id: "game-rates",
            side: BOTH,
            name: "Game Rates",
            join: true,
            replaces: ["building-materials", "building-yield", "furniture-comfort"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.texts(TEXTS)?;
        carry_over::run(load, ID, &Settings::template())?;
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
        load.modify_recipes(|recipe| recipes::RATES.scale(recipe))?;
        if let Err(off) = load.on_comfort(|provider| comfort::RATE.scale(provider.value)) {
            enshwick::warn!("furniture comfort stays vanilla: {off}");
        }
        Ok(Self)
    }
}

enshwick::declare_mod!(GameRates);

#[cfg(test)]
mod tests {
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
        assert_eq!(settings.recipe_multiplier, 2);
        assert!(settings.recipe_blocks && settings.recipe_overgrowth && settings.recipe_decor);
        assert!((settings.comfort_multiplier - 2.0).abs() < f32::EPSILON);
        assert!(settings.validate().is_ok());
        let synced = settings.synced().unwrap();
        for key in [
            "recipe_multiplier",
            "recipe_blocks",
            "recipe_overgrowth",
            "recipe_decor",
        ] {
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
            .settings("recipe_multiplier = 2\nrecipe_blocks = true\nrecipe_overgrowth = true\nrecipe_decor = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let allowed = loader.rebuilds().len();
        assert_eq!(allowed, 1, "the registry may be rebuilt");
        loader.set_server_synced(Some("{ recipe_multiplier = 5 }"));
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(recipes::RATES.multiplier(), 5);
        assert_eq!(loader.rebuilds().len(), allowed + 1, "joined: rebuilt");
        loader.set_server_synced(None);
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(recipes::RATES.multiplier(), 2);
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
        loader.set_settings("comfort_multiplier = 1.5\n");
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(loader.comfort(&comfort(3)), 5);
        assert_eq!(loader.rebuilds().len(), rebuilds, "no recipe changed");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn the_id_is_the_manifest_s() {
        // SAFETY: the manifest's strings are static literals.
        let id = unsafe { GameRates::manifest().id.as_str() };
        assert_eq!(id, Some(ID));
    }

    #[test]
    fn every_setting_has_its_label_and_description_in_each_language() {
        let texts: toml::Table = toml::from_str(TEXTS).unwrap();
        let schema = Settings::schema();
        let keys: Vec<String> = std::iter::once(schema.text.clone())
            .chain(
                schema
                    .fields
                    .iter()
                    .flat_map(|field| [field.text.clone(), format!("{}.doc", field.text)]),
            )
            .collect();
        for language in ["en", "uk"] {
            let table = texts[language].as_table().unwrap();
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
                recipe_multiplier,
                ..Settings::default()
            };
            assert!(settings.validate().is_err(), "{recipe_multiplier}");
        }
        for comfort_multiplier in [-1.0, 100.5, 1e9, f32::INFINITY, f32::NAN] {
            let settings = Settings {
                comfort_multiplier,
                ..Settings::default()
            };
            assert!(settings.validate().is_err(), "{comfort_multiplier}");
        }
        for comfort_multiplier in [0.0, 100.0] {
            let settings = Settings {
                comfort_multiplier,
                ..Settings::default()
            };
            assert!(settings.validate().is_ok(), "{comfort_multiplier}");
        }
    }

    #[test]
    fn the_replaced_mods_files_carry_over_on_the_first_start() {
        use std::fs::{create_dir_all, read_to_string, write};

        let game = std::env::temp_dir().join(format!("game-rates-carry-{}", std::process::id()));
        let configs = game.join("mod-configs");
        let worlds = |id: &str, key: &str| game.join("mod-data").join(id).join("worlds").join(key);
        for dir in [
            configs.clone(),
            worlds("building-materials", "w1"),
            worlds("furniture-comfort", "w2"),
        ] {
            create_dir_all(dir).unwrap();
        }
        write(
            configs.join("building-materials.toml"),
            "multiplier = 4\nblocks = true\novergrowth = false\ndecor = true\n",
        )
        .unwrap();
        write(configs.join("furniture-comfort.toml"), "multiplier = 3.0\n").unwrap();
        write(
            worlds("building-materials", "w1").join("settings.toml"),
            "multiplier = 6\nblocks = true\novergrowth = true\ndecor = true\n",
        )
        .unwrap();
        write(
            worlds("furniture-comfort", "w2").join("settings.toml"),
            "multiplier = 0.5\n",
        )
        .unwrap();
        let data = game.join("mod-data").join(ID);
        let loader = recipe_host().data_dir(data.to_str().unwrap()).install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick::abi::Status::OK, "{:?}", loader.logs());
        let read = |path: std::path::PathBuf| -> Settings {
            toml::from_str(&read_to_string(path).unwrap()).unwrap()
        };
        let file = read(configs.join("game-rates.toml"));
        assert_eq!(file.recipe_multiplier, 4);
        assert!(!file.recipe_overgrowth);
        assert!((file.comfort_multiplier - 3.0).abs() < f32::EPSILON);
        let w1 = read(worlds(ID, "w1").join("settings.toml"));
        assert_eq!(w1.recipe_multiplier, 6);
        assert!(w1.recipe_overgrowth);
        assert!((w1.comfort_multiplier - 3.0).abs() < f32::EPSILON);
        let w2 = read(worlds(ID, "w2").join("settings.toml"));
        assert_eq!(w2.recipe_multiplier, 4);
        assert!((w2.comfort_multiplier - 0.5).abs() < f32::EPSILON);
        std::fs::remove_dir_all(game).unwrap();
    }
}
