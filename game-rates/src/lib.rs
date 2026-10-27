//! Game rates: the recipes for building blocks, roofs, overgrowth,
//! decor and furniture make more for the same cost, furniture gives more
//! comfort, and experience, workshop speed, drops, loot and skill points
//! follow their own multipliers. Functional props (stations, chests,
//! altars, water devices) keep their recipes.

use enshwick::content::recipes::RecipeKind;
use enshwick::rates::{ExperienceGroup, MAX_FACTOR, Rate, RateHandle};
use enshwick::{Load, Mod, Off};

mod migrations;
mod recipes;

use crate::recipes::Category;

/// The labels and descriptions of the settings, a table per language.
const TEXTS: &str = include_str!("texts.toml");

/// Game rates: building recipes, furniture comfort, then the game's rates.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    /// The building recipes: how much more they make, and of what.
    #[config(group)]
    recipes: Recipes,
    /// Comfort furniture gives inside a base.
    #[config(group)]
    comfort: Comfort,
    /// Experience by where it comes from.
    #[config(group)]
    experience: Experience,
    /// Timed workshops such as the smelter and the kiln.
    #[config(group)]
    workshops: Workshops,
    /// How much resources and loot hand out.
    #[config(group)]
    drops: Drops,
    /// Skill points.
    #[config(group)]
    skills: Skills,
}

/// How much more the building recipes make, and of what.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Recipes {
    /// The building recipes make their output times this, for the
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
    #[config(runtime, range = 0.0..=MAX_FACTOR)]
    multiplier: f32,
}

impl Default for Comfort {
    fn default() -> Self {
        Self { multiplier: 2.0 }
    }
}

/// Experience by where it comes from, each times its factor; the server's
/// values decide.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Experience {
    /// Experience from defeated enemies, times this.
    #[config(runtime, range = 0.0..=MAX_FACTOR)]
    combat: f32,
    /// Experience from mining, times this.
    #[config(runtime, range = 0.0..=MAX_FACTOR)]
    mining: f32,
    /// Experience from exploration, lore, quests and item sets, times this.
    #[config(runtime, range = 0.0..=MAX_FACTOR)]
    exploration: f32,
}

impl Default for Experience {
    fn default() -> Self {
        Self {
            combat: 1.0,
            mining: 1.0,
            exploration: 1.0,
        }
    }
}

/// How fast timed workshops produce.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Workshops {
    /// How fast timed workshops produce, from 0.1 to 100: 2 halves the
    /// time. A joined player runs with the server's value.
    #[config(sync, range = 0.1..=MAX_FACTOR)]
    speed: f32,
}

impl Default for Workshops {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
}

/// How much resources and random loot hand out; the server's values decide.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Drops {
    /// Resources from mining, chopping and harvesting, times this.
    #[config(runtime, range = 0.0..=MAX_FACTOR)]
    resources: f32,
    /// Each stack of random loot in chests, corpses and animals, times
    /// this, up to the item's stack size.
    #[config(runtime, range = 0.0..=MAX_FACTOR)]
    loot: f32,
}

impl Default for Drops {
    fn default() -> Self {
        Self {
            resources: 1.0,
            loot: 1.0,
        }
    }
}

/// How many skill points a level gives.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
struct Skills {
    /// Skill points each level gives, times this, rounded. A joined player
    /// runs with the server's value.
    #[config(sync, range = 0.0..=MAX_FACTOR)]
    points: f32,
}

impl Default for Skills {
    fn default() -> Self {
        Self { points: 1.0 }
    }
}

/// The rates this mod declared; one the game build lacks stays `None`.
#[derive(Clone, Debug)]
struct Rates {
    comfort: Option<RateHandle>,
    combat: Option<RateHandle>,
    mining: Option<RateHandle>,
    exploration: Option<RateHandle>,
    workshops: Option<RateHandle>,
    resources: Option<RateHandle>,
    loot: Option<RateHandle>,
    skill_points: Option<RateHandle>,
    /// The recipe output rate of each kind a setting scales.
    recipes: Vec<(Category, RateHandle)>,
}

impl Rates {
    /// Declares every rate; a rate the build lacks turns off alone.
    fn declare(load: &mut Load<'_>) -> Self {
        let mut rate = |rate: Rate| match load.rate(rate) {
            Ok(handle) => Some(handle),
            Err(off) => {
                enshwick::warn!("stays vanilla: {off}");
                None
            }
        };
        Self {
            comfort: rate(Rate::Comfort),
            combat: rate(Rate::Experience(ExperienceGroup::Combat)),
            mining: rate(Rate::Experience(ExperienceGroup::Mining)),
            exploration: rate(Rate::Experience(ExperienceGroup::Exploration)),
            workshops: rate(Rate::WorkshopSpeed),
            resources: rate(Rate::ResourceDrops),
            loot: rate(Rate::Loot),
            skill_points: rate(Rate::SkillPoints),
            recipes: RecipeKind::ALL
                .iter()
                .filter_map(|&kind| {
                    let category = recipes::category(kind)?;
                    Some((category, rate(Rate::RecipeOutput(kind))?))
                })
                .collect(),
        }
    }

    /// Each declared rate with its factor from `settings`.
    fn factors(&self, settings: &Settings) -> impl Iterator<Item = (RateHandle, f32)> {
        let recipes = &settings.recipes;
        let multiplier = u16::try_from(recipes.multiplier).map_or(MAX_FACTOR, f32::from);
        let recipe_factors = self.recipes.iter().map(move |&(category, handle)| {
            let on = match category {
                Category::Blocks => recipes.blocks,
                Category::Overgrowth => recipes.overgrowth,
            };
            (handle, if on { multiplier } else { 1.0 })
        });
        [
            (self.comfort, settings.comfort.multiplier),
            (self.combat, settings.experience.combat),
            (self.mining, settings.experience.mining),
            (self.exploration, settings.experience.exploration),
            (self.workshops, settings.workshops.speed),
            (self.resources, settings.drops.resources),
            (self.loot, settings.drops.loot),
            (self.skill_points, settings.skills.points),
        ]
        .into_iter()
        .filter_map(|(handle, factor)| Some((handle?, factor)))
        .chain(recipe_factors)
    }

    /// Puts the factors of `settings` in force.
    fn apply(&self, settings: &Settings) {
        for (handle, factor) in self.factors(settings) {
            if let Err(why) = handle.set(factor) {
                enshwick::warn!("{:?} unchanged: {why}", handle.rate());
            }
        }
    }
}

impl Settings {
    /// Puts these settings in force; whether the decor recipes changed.
    fn apply(&self, rates: &Rates) -> bool {
        let Recipes {
            multiplier,
            blocks,
            overgrowth,
            decor,
        } = self.recipes;
        let changed = recipes::set_decor(if decor { multiplier } else { 1 });
        rates.apply(self);
        enshwick::info!(
            "recipe output x{multiplier}: blocks {blocks}, overgrowth {overgrowth}, decor {decor}; {:?}",
            (
                &self.comfort,
                &self.experience,
                &self.workshops,
                &self.drops,
                &self.skills
            )
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
        let rates = Rates::declare(load);
        let settings: Settings = load.config()?;
        settings.apply(&rates);
        // Values that arrive later, an edited file or a joined server's
        // values and the file's again on leaving, rebuild the registry the
        // game already holds when they change decor; the loader rebuilds
        // it for the recipe output rates.
        load.on_config_change(move |settings: Settings| {
            if !settings.apply(&rates) {
                return;
            }
            match enshwick::content::recipes::rebuild_recipes() {
                Ok(()) => enshwick::info!("recipe counts follow the new values"),
                Err(off) => enshwick::warn!(
                    "recipes already built keep their counts until the next start: {off}"
                ),
            }
        })?;
        load.modify_recipes(recipes::scale_decor)?;
        // A dedicated server builds its recipes before it loads the items
        // that tell their kinds, so the kinds apply in a rebuild.
        load.on::<enshwick::events::WorldLoaded>(|()| {
            if let Err(off) = enshwick::content::recipes::rebuild_recipes() {
                enshwick::warn!("recipes keep their counts until the next start: {off}");
            }
        })?;
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
        let mut host = enshwick::test::FakeHost::new().type_hash(
            enshwick::content::types::RECIPE_REGISTRY_RESOURCE.id(),
            0x5e1f_0001,
        );
        for (id, size) in [
            ("recipe", 0xa8),
            ("recipe_ingredient", 48),
            ("recipe_stack", 24),
        ] {
            host = host.type_size(id, size);
        }
        for (id, at) in [
            ("recipe_registry.recipes", 8),
            ("recipe.guid", 0),
            ("recipe.id", 0x10),
            ("recipe.workshop_guid", 0x14),
            ("recipe.workshop", 0x24),
            ("recipe.input", 0x28),
            ("recipe.output", 0x30),
            ("recipe.required_props", 0x40),
            ("recipe.completion", 0x5c),
            ("recipe.debug_name", 0x78),
            ("recipe_stack.item", 0x10),
            ("recipe_stack.count", 0x14),
        ] {
            host = host.constant(id, at);
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
    fn the_default_settings_parse_and_only_what_clients_count_syncs() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.recipes.multiplier, 2);
        assert!(settings.recipes.blocks && settings.recipes.overgrowth && settings.recipes.decor);
        assert!((settings.comfort.multiplier - 2.0).abs() < f32::EPSILON);
        assert!((settings.experience.combat - 1.0).abs() < f32::EPSILON);
        assert!((settings.workshops.speed - 1.0).abs() < f32::EPSILON);
        assert!((settings.skills.points - 1.0).abs() < f32::EPSILON);
        assert!(settings.validate().is_ok());
        let synced = settings.synced().unwrap();
        for key in [
            "multiplier",
            "blocks",
            "overgrowth",
            "decor",
            "speed",
            "points",
        ] {
            assert!(synced.contains(key), "{key} in {synced}");
        }
        for key in [
            "comfort",
            "combat",
            "mining",
            "exploration",
            "resources",
            "loot",
        ] {
            assert!(!synced.contains(key), "{key} in {synced}");
        }
    }

    #[test]
    fn every_rate_takes_its_setting_and_follows_a_change() {
        use enshwick::abi::Status;
        use enshwick::events::SettingsChanged;
        use enshwick::rates::{ExperienceGroup, Rate};

        let loader = recipe_host()
            .side(enshwick::abi::Side::SERVER)
            .settings("[experience]\ncombat = 3.0\n[drops]\nloot = 2.0\n[skills]\npoints = 1.5\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let combat = Rate::Experience(ExperienceGroup::Combat);
        assert_eq!(loader.rate(combat), Some(3.0));
        assert_eq!(
            loader.rate(Rate::Experience(ExperienceGroup::Mining)),
            Some(1.0)
        );
        assert_eq!(loader.rate(Rate::Loot), Some(2.0));
        assert_eq!(loader.rate(Rate::SkillPoints), Some(1.5));
        assert_eq!(loader.rate(Rate::WorkshopSpeed), Some(1.0));
        assert_eq!(loader.rate(Rate::Comfort), Some(2.0));
        loader.set_settings("[experience]\ncombat = 0.5\n");
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(loader.rate(combat), Some(0.5));
        assert_eq!(loader.rate(Rate::Loot), Some(1.0));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn each_recipe_kind_takes_the_multiplier_its_switch_allows() {
        use enshwick::abi::Status;
        use enshwick::rates::Rate;

        let loader = recipe_host()
            .side(enshwick::abi::Side::SERVER)
            .settings(
                "[recipes]\nmultiplier = 4\nblocks = true\novergrowth = false\ndecor = false\n",
            )
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        for &kind in RecipeKind::ALL {
            let expected = match recipes::category(kind) {
                Some(Category::Blocks) => Some(4.0),
                Some(Category::Overgrowth) => Some(1.0),
                None => None,
            };
            assert_eq!(loader.rate(Rate::RecipeOutput(kind)), expected, "{kind:?}");
        }
        assert_eq!(recipes::decor_multiplier(), 1, "decor stays vanilla");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_rate_the_build_lacks_leaves_the_rest_running() {
        use enshwick::abi::Status;
        use enshwick::rates::Rate;

        let loader = recipe_host().without_rate(Rate::Loot).install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.rate(Rate::Loot), None);
        assert_eq!(loader.rate(Rate::Comfort), Some(2.0));
    }

    #[test]
    fn a_joined_server_s_values_and_leaving_rebuild_the_recipes() {
        use enshwick::abi::Status;
        use enshwick::events::SettingsChanged;
        use enshwick::rates::Rate;

        const BLOCKS: Rate = Rate::RecipeOutput(RecipeKind::BuildingBlock);

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
        assert_eq!(recipes::decor_multiplier(), 5);
        assert_eq!(loader.rate(BLOCKS), Some(5.0));
        assert_eq!(loader.rebuilds().len(), allowed + 1, "joined: rebuilt");
        loader.set_server_synced(None);
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(recipes::decor_multiplier(), 2);
        assert_eq!(loader.rate(BLOCKS), Some(2.0));
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
    fn every_text_exists_in_all_languages() {
        let texts = enshwick::i18n::Texts::parse(TEXTS).unwrap();
        assert_eq!(texts.untranslated(), []);
    }

    #[test]
    fn the_mod_and_every_setting_have_their_texts_in_each_table() {
        let schema = Settings::schema();
        let keys: Vec<String> = std::iter::once(schema.text.clone())
            .chain(schema.groups.iter().map(|group| group.text.clone()))
            .chain(schema.fields.iter().map(|field| field.text.clone()))
            .flat_map(|key| [format!("{key}.doc"), key])
            .filter(|key| key != "config.doc")
            .chain([enshwick::i18n::MOD_NAME, enshwick::i18n::MOD_DESCRIPTION].map(str::to_owned))
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
