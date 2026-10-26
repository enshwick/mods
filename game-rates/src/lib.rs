//! Game rates: the vanilla recipes for building blocks, roofs, overgrowth,
//! decor and furniture make more for the same cost, and furniture gives more
//! comfort. Functional props (stations, chests, altars, water devices) stay
//! as they are.
//!
//! Each tick every comfort provider in a base registers its comfort for its
//! category, and the base keeps the best per category; a player's comfort is
//! the sum. The mod scales what each provider registers, on the server, which
//! decides comfort.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick::content::recipes::RecipeEdit;
use enshwick::{Load, Mod, Off};

mod carry_over;

/// The mod's id, as in its manifest; it names the settings file.
const ID: &str = "game-rates";

/// The largest comfort factor: the game sums the comfort of every category
/// into one number, which a larger factor could overflow.
const MAX_COMFORT: f32 = 100.0;

/// The comfort factor in force, as `f32` bits, read on every registration.
static COMFORT: AtomicU32 = AtomicU32::new(0x3f80_0000);
/// The output multiplier in force.
static MULTIPLIER: AtomicU32 = AtomicU32::new(1);
/// Whether each [`Category`] is scaled, in its order.
static SCALED: [AtomicBool; 3] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

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
    #[config(runtime, range = 0.0..=MAX_COMFORT)]
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
        let multiplier = self.recipe_multiplier;
        let mut changed = MULTIPLIER.swap(multiplier, Ordering::Relaxed) != multiplier;
        for (scaled, on) in SCALED.iter().zip([
            self.recipe_blocks,
            self.recipe_overgrowth,
            self.recipe_decor,
        ]) {
            changed |= scaled.swap(on, Ordering::Relaxed) != on;
        }
        COMFORT.store(self.comfort_multiplier.to_bits(), Ordering::Relaxed);
        enshwick::info!(
            "recipe output x{}: blocks {}, overgrowth {}, decor {}; furniture comfort x{}",
            multiplier,
            self.recipe_blocks,
            self.recipe_overgrowth,
            self.recipe_decor,
            self.comfort_multiplier
        );
        changed
    }
}

/// What a recipe makes, by its debug name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Building blocks and roofs.
    Blocks,
    /// Overgrowth materials.
    Overgrowth,
    /// Decor and furniture.
    Decor,
}

/// The category of the recipe named `name`, `None` for everything else.
#[must_use]
pub fn category(name: &str) -> Option<Category> {
    if name.starts_with("Block_") {
        Some(Category::Blocks)
    } else if name.starts_with("Material_") && name.contains("_Overgrowth_") {
        Some(Category::Overgrowth)
    } else if name.starts_with("Prop_Decoration_")
        // Decoration with a function: it holds water or items.
        && !["Water", "Storage", "Chest"].iter().any(|word| name.contains(word))
    {
        Some(Category::Decor)
    } else {
        None
    }
}

/// Scales the output of `recipe` when its category is in force.
fn scale(recipe: &mut RecipeEdit<'_>) {
    let Some(category) = category(&recipe.view().debug_name()) else {
        return;
    };
    if SCALED[category as usize].load(Ordering::Relaxed) {
        recipe.scale_output(MULTIPLIER.load(Ordering::Relaxed));
    }
}

/// `comfort` scaled by `factor`, rounded and kept in range.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "clamped to the i32 range first"
)]
pub fn scaled_comfort(comfort: i32, factor: f32) -> i32 {
    let value = (f64::from(comfort) * f64::from(factor)).round();
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
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
        carry_over::carry_over(
            load,
            ID,
            &<Settings as enshwick::config::Config>::template(),
        )?;
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
        load.modify_recipes(scale)?;
        if let Err(off) = load.on_comfort(|comfort| {
            scaled_comfort(
                comfort.value,
                f32::from_bits(COMFORT.load(Ordering::Relaxed)),
            )
        }) {
            enshwick::warn!("furniture comfort stays vanilla: {off}");
        }
        Ok(Self)
    }
}

enshwick::declare_mod!(GameRates);

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick::config::Config;

    #[test]
    fn recipes_fall_in_their_categories() {
        assert_eq!(
            category("Block_T2_Metal_Copper_CRAFTING"),
            Some(Category::Blocks)
        );
        assert_eq!(category("Block_Roof_T6_Palmleaves"), Some(Category::Blocks));
        assert_eq!(
            category("Material_T1_Overgrowth_Ivy"),
            Some(Category::Overgrowth)
        );
        assert_eq!(
            category("Prop_Decoration_Comfort1_T0_Bench"),
            Some(Category::Decor)
        );
        assert_eq!(category("Prop_Decoration_T2_Barrel_Water"), None);
        assert_eq!(category("Prop_Decoration_T1_Storage_Shelf"), None);
        assert_eq!(category("Prop_Storage_32_T2_Chest_Wood"), None);
        assert_eq!(category("Recipe_Overgrowth_Tool"), None);
        assert_eq!(category("Craft_Weapon_T7_1H_Lore_Axe_Ministory_10"), None);
    }

    #[test]
    fn the_categories_index_their_switches() {
        assert_eq!(Category::Blocks as usize, 0);
        assert_eq!(Category::Overgrowth as usize, 1);
        assert_eq!(Category::Decor as usize, 2);
    }

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
        assert_eq!(MULTIPLIER.load(Ordering::Relaxed), 5);
        assert_eq!(loader.rebuilds().len(), allowed + 1, "joined: rebuilt");
        loader.set_server_synced(None);
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(MULTIPLIER.load(Ordering::Relaxed), 2);
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
    fn comfort_scales_rounds_and_stays_in_range() {
        assert_eq!(scaled_comfort(7, 2.0), 14);
        assert_eq!(scaled_comfort(3, 1.5), 5);
        assert_eq!(scaled_comfort(i32::MAX, 2.0), i32::MAX);
        assert_eq!(scaled_comfort(5, 0.0), 0);
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
