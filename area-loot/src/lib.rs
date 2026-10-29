//! Area Loot: when a player empties an enemy's loot, the player also gets the
//! loot of the other enemies nearby. Corpses and enemy loot bags within the radius
//! are emptied into the same player, nearest first, until one does not fit;
//! containers another player has open stay as they are. Items on the ground
//! keep the game's own pickup.

pub mod points;
mod take;
mod tracker;

use enshwick::{Load, Mod, Off};

/// The labels and descriptions of the settings and the mod, a table per
/// language.
const TEXTS: &str = include_str!("texts.toml");

/// Area Loot settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Metres across the ground from the emptied container.
    #[config(runtime, range = 0.1..=50.0)]
    radius: f32,
    /// Metres above or below the emptied container.
    #[config(runtime, range = 0.1..=20.0)]
    height: f32,
    /// Take the loot around; false leaves looting vanilla.
    #[config(runtime)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radius: 12.0,
            height: 4.0,
            enabled: true,
        }
    }
}

fn describe(settings: &Settings) {
    if settings.enabled {
        enshwick::info!(
            "area loot within {} m, {} m up or down",
            settings.radius,
            settings.height
        );
    } else {
        enshwick::info!("area loot disabled");
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct AreaLoot;

impl Mod for AreaLoot {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "enshwick.area-loot",
            side: SERVER,
            name: "Area Loot",
            was: ["area-loot"],
            points: [points::Eligible],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let _texts = load.texts(TEXTS)?;
        let settings: Settings = load.config()?;
        take::apply(&settings);
        take::link(load)?;
        load.on_config_change(|settings: Settings| {
            take::apply(&settings);
            describe(&settings);
        })?;
        describe(&settings);
        Ok(Self)
    }
}

enshwick::declare_mod!(AreaLoot);

#[cfg(test)]
mod tests {
    use enshwick::config::Config;

    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!((settings.radius - 12.0).abs() < f32::EPSILON);
        assert!((settings.height - 4.0).abs() < f32::EPSILON);
        assert!(settings.enabled);
        assert!(settings.validate().is_ok());
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
    fn out_of_range_settings_are_refused() {
        let with = |radius, height| Settings {
            radius,
            height,
            enabled: true,
        };
        assert!(with(0.0, 4.0).validate().is_err());
        assert!(with(51.0, 4.0).validate().is_err());
        assert!(with(f32::NAN, 4.0).validate().is_err());
        assert!(with(12.0, 0.0).validate().is_err());
        assert!(with(12.0, 21.0).validate().is_err());
        assert!(with(50.0, 20.0).validate().is_ok());
    }
}
