//! Better Altars: how many Flame Altars each Flame level allows, up to 255 in
//! up to 179 bases, and Flame Altar zones that touch grow together and can be
//! built across.
//!
//! The two parts keep their settings in the groups `[count]` and `[zones]`.

mod count;
pub mod geometry;
mod hold;
mod zones;

use count::{AltarCount, Count};
use enshwick::{Load, Mod, Off};
use zones::{AltarZones, Zones};

enshwick::texts!();

/// Better Altars: how many altars, then how their zones grow.
#[enshwick::config]
#[derive(Debug, Default)]
struct Settings {
    /// How many Flame Altars each Flame level allows.
    #[config(group)]
    count: Count,
    /// How the zones of touching altars grow and join.
    #[config(group)]
    zones: Zones,
}

/// The mod: the altar count and the zones, each with its own settings group.
#[derive(Debug)]
pub struct BetterAltars {
    count: AltarCount,
    zones: AltarZones,
}

impl Mod for BetterAltars {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let _texts = load.texts()?;
        let count = AltarCount::start(load)?;
        let zones = AltarZones::start(load, &Zones::default())?;
        load.on_altar_limit(|limit| {
            if let Some(altars) = BetterAltars::instance() {
                limit.allowed = altars.count.limit(limit.flame_level, limit.vanilla);
            }
        })?;
        load.on_settings_change(|settings: &Settings| {
            if let Some(altars) = BetterAltars::instance() {
                altars.count.apply(&settings.count);
                altars.zones.apply(&settings.zones);
            }
        })?;
        Ok(Self { count, zones })
    }

    fn ready(&'static self, _host: enshwick::Host) {
        AltarZones::ready();
    }
}

enshwick::declare_mod!(BetterAltars);

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use enshwick::Entity;
    use enshwick::abi::{AltarInfo, Bounds, Side, Status};
    use enshwick::config::Config as _;
    use enshwick::events::{AltarsChanged, SettingsReplaced};

    use super::*;
    use crate::count::{AltarTable, CAP};

    /// The loader installed on `side` with `settings`, and the mod started.
    fn started(side: Side, settings: &str) -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .side(side)
            .settings(settings)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        // The world loads, and its settings apply.
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        loader
    }

    /// An altar at level 1 whose zone is 80 cells wide from `x`.
    fn altar(entity: Entity, x: i64) -> AltarInfo {
        let zone = Bounds {
            min: [x, 0, 0],
            max: [x + 80, 80, 80],
        };
        let mut info = AltarInfo::EMPTY;
        info.entity = entity;
        info.level = 1;
        info.level_count = 1;
        info.zone = zone;
        info.levels[0] = zone;
        info
    }

    /// Waits up to ten seconds for the solver to grow `altar`'s zone at
    /// level 1, 80 cells tall, and to have the zones sized again `resizes`
    /// times in all.
    fn wait_for_growth(loader: &enshwick::test::Installed, altar: Entity, resizes: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while (loader.altar_zone(altar, 1, 80).0 == 0 || loader.zone_resizes() < resizes)
            && Instant::now() < deadline
        {
            std::thread::yield_now();
        }
    }

    #[test]
    fn altars_the_game_has_not_levelled_are_named_in_the_log_once() {
        let loader = started(Side::CLIENT, &Settings::template());
        let mut unleveled = AltarInfo::EMPTY;
        unleveled.entity = Entity(2);
        loader.set_altars(&[altar(Entity(1), 0), unleveled]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        let line = "1 altar has no Flame level yet; it keeps its vanilla zone";
        // The solve the start began may still run, and the line then comes
        // from its thread.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !loader.logs().iter().any(|(_, l)| l == line) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        let logged = loader.logs().iter().filter(|(_, l)| l == line).count();
        assert_eq!(logged, 1, "{:?}", loader.logs());
    }

    #[test]
    fn the_default_settings_parse_into_both_groups() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(
            AltarTable::new(&settings.count.altars_per_flame_level, CAP).altars(1, 2),
            4
        );
        assert!(settings.zones.clusters && settings.zones.building_across_seams);
    }

    #[test]
    fn every_text_exists_in_all_languages() {
        let texts = enshwick::i18n::Texts::parse(texts::TOML).unwrap();
        assert_eq!(texts.untranslated(), []);
        assert_eq!(texts.missing_in(enshwick::i18n::Language::OFFERED), []);
    }

    #[test]
    fn the_mod_and_every_setting_and_group_have_their_texts_in_each_table() {
        let schema = Settings::schema();
        let lists = schema
            .fields
            .iter()
            .filter(|field| field.kind == Some(enshwick::config::Kind::Array));
        let keys: Vec<String> = std::iter::once(schema.text.clone())
            .chain(schema.groups.iter().map(|group| group.text.clone()))
            .chain(schema.fields.iter().map(|field| field.text.clone()))
            .flat_map(|key| [format!("{key}.doc"), key])
            .filter(|key| key != "config.doc")
            .chain(lists.map(|field| format!("{}.element", field.text)))
            .chain([enshwick::i18n::MOD_NAME, enshwick::i18n::MOD_DESCRIPTION].map(str::to_owned))
            .chain([texts::HOLD_PAST, texts::OFF_THREAD, texts::OFF_GROUPING].map(str::to_owned))
            .collect();
        let tables: toml::Table = toml::from_str(texts::TOML).unwrap();
        for (language, table) in &tables {
            let table = table.as_table().unwrap();
            for key in &keys {
                assert!(table.contains_key(key), "{language}: {key}");
            }
            assert_eq!(table.len(), keys.len(), "{language}: no stale keys");
        }
    }

    #[test]
    fn a_level_outside_the_settings_range_is_held_to_it() {
        let loader = started(
            Side::CLIENT,
            "[count]\naltars_per_flame_level = [0, 40, 300, 5]\n",
        );
        assert_eq!(loader.altar_limit(0, 1), 1);
        assert_eq!(loader.altar_limit(1, 2), 40);
        assert_eq!(loader.altar_limit(2, 4), CAP);
        assert_eq!(loader.altar_limit(3, 6), 5);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn the_mod_asks_for_the_most_bases_at_init_and_a_world_keeps_the_game_s_16_by_default() {
        let loader = started(Side::CLIENT, "");
        assert_eq!(loader.requested_base_limits(), Some((u32::from(CAP), 179)));
        let limit = enshwick::altars::base_limit().unwrap();
        assert_eq!((limit.zones, limit.bases), (u32::from(CAP), 16));
    }

    #[test]
    fn a_world_s_bases_apply_at_its_load_below_the_request() {
        let loader = started(Side::SERVER, "[count]\nbases = 40\n");
        assert_eq!(loader.requested_base_limits(), Some((u32::from(CAP), 179)));
        assert_eq!(enshwick::altars::base_limit().unwrap().bases, 40);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_build_that_keeps_16_bases_keeps_the_altar_table() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .without_bases_past_16()
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(enshwick::altars::base_limit().unwrap().bases, 16);
        assert_eq!(loader.altar_limit(11, 16), 32);
    }

    #[test]
    fn the_table_answers_the_limit_past_32_altars() {
        let loader = started(
            Side::CLIENT,
            "[count]\naltars_per_flame_level = [2, 64, 200, 255]\n",
        );
        assert_eq!(loader.altar_limit(1, 2), 64);
        assert_eq!(loader.altar_limit(2, 4), 200);
        assert_eq!(loader.altar_limit(11, 16), 255);
        assert!(loader.faults().is_empty());
    }

    /// The hold's reason and its text, waiting up to `wait` for it, on a
    /// server whose world groups `zones` altar zones in `bases` bases.
    fn held(zones: u32, bases: u32, wait: Duration) -> Option<(String, String)> {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-better-altars-hold-{}-{zones}-{bases}",
            std::process::id()
        ));
        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .mod_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader.set_grouping(zones, bases);
        let hold = enshwick::state::Hold::new(&dir);
        let deadline = Instant::now() + wait;
        while hold.reason().is_none() && Instant::now() < deadline {
            assert!(loader.raise::<AltarsChanged>(&()) >= 1);
            std::thread::sleep(Duration::from_millis(20));
        }
        let reason = hold.reason().zip(hold.text());
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        reason
    }

    /// A reason's `text` in every offered language, its values filled in.
    fn in_every_language(text: &str) -> Vec<String> {
        let texts = enshwick::i18n::Texts::parse(texts::TOML).unwrap();
        let mut parts = text.split('\t');
        let key = parts.next().unwrap();
        let args: Vec<(&str, &str)> = parts.filter_map(|arg| arg.split_once('=')).collect();
        enshwick::i18n::Language::OFFERED
            .iter()
            .map(|&language| texts.fill(key, language, &args))
            .collect()
    }

    #[test]
    fn a_server_holds_a_world_past_the_game_s_own_bases() {
        let (reason, text) = held(16, 17, Duration::from_secs(10)).expect("held");
        assert!(reason.contains("16 altar zones in 17 bases"), "{reason}");
        assert!(reason.contains("lose their base features"), "{reason}");
        assert!(text.starts_with(texts::HOLD_PAST), "{text}");
    }

    #[test]
    fn a_server_holds_a_world_past_the_game_s_own_altar_zones() {
        let (reason, _) = held(66, 5, Duration::from_secs(10)).expect("held");
        assert!(reason.contains("66 altar zones in 5 bases"), "{reason}");
    }

    #[test]
    fn the_hold_s_reason_reads_in_every_language_with_its_values() {
        let (_, text) = held(40, 20, Duration::from_secs(10)).expect("held");
        for line in in_every_language(&text) {
            assert!(!line.contains('{'), "{line}");
            for value in ["40", "20", "16"] {
                assert!(line.contains(value), "{value}: {line}");
            }
        }
    }

    #[test]
    fn a_held_world_on_a_build_with_an_unknown_grouping_turns_the_mod_off_with_its_text() {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-better-altars-unknown-{}",
            std::process::id()
        ));
        let hold = enshwick::state::Hold::new(&dir);
        hold.set("held").unwrap();
        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .mod_dir(dir.to_str().unwrap())
            .without_base_limit()
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(status, Status::REFUSED);
        assert_eq!(loader.faults(), [texts::OFF_GROUPING]);
    }

    #[test]
    fn a_server_does_not_hold_a_world_within_the_game_s_own_limits() {
        assert_eq!(held(16, 16, Duration::from_secs(1)), None);
    }

    #[test]
    fn the_table_answers_the_limit_with_the_base_limits_requested() {
        let loader = started(Side::CLIENT, "");
        assert!(loader.requested_base_limits().is_some());
        assert_eq!(loader.altar_limit(1, 2), 4);
        assert_eq!(loader.altar_limit(11, 16), 32);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn it_sizes_the_zones_of_touching_altars_and_accepts_a_box_across_them() {
        let loader = started(Side::CLIENT, &Settings::template());
        assert_eq!(loader.altars_opened(), (true, true));
        loader.set_altars(&[altar(Entity(1), 0), altar(Entity(2), 40)]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        wait_for_growth(&loader, Entity(1), 1);
        let (grow, height) = loader.altar_zone(Entity(1), 1, 80);
        assert!(grow > 0, "touching altars grow");
        assert_eq!(height, 80);
        assert_eq!(loader.altar_zone(Entity(2), 1, 80), (grow, height));
        assert_eq!(
            loader.altar_zone(Entity(2), 2, 80),
            (0, 80),
            "another level: vanilla"
        );
        assert_eq!(
            loader.altar_zone(Entity(3), 1, 80),
            (0, 80),
            "unknown altar: vanilla"
        );

        let zones = [
            Bounds {
                min: [0, 0, 0],
                max: [10, 10, 10],
            },
            Bounds {
                min: [10, 0, 0],
                max: [20, 10, 10],
            },
        ];
        let across = Bounds {
            min: [5, 1, 1],
            max: [15, 9, 9],
        };
        assert!(loader.building_zone_check(&zones, across, 0));
        assert!(!loader.building_zone_check(&zones[..1], across, 0));
        AltarZones::set_seams(false);
        assert!(
            !loader.building_zone_check(&zones, across, 0),
            "seams off: vanilla"
        );
        AltarZones::set_seams(true);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_world_s_own_clusters_value_applies_when_it_loads() {
        let off = "[zones]\nclusters = false\n";
        let loader = started(Side::CLIENT, off);
        loader.set_altars(&[altar(Entity(1), 0), altar(Entity(2), 40)]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        let before = loader.zone_resizes();
        assert_eq!(
            loader.altar_zone(Entity(1), 1, 80),
            (0, 80),
            "clusters off: vanilla"
        );
        loader.set_settings("settings", "[zones]\nclusters = true\n");
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        wait_for_growth(&loader, Entity(1), before + 1);
        assert_eq!(loader.zone_resizes(), before + 1);
        assert!(
            loader.altar_zone(Entity(1), 1, 80).0 > 0,
            "the world turned clusters on"
        );
        loader.set_settings("settings", off);
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        assert_eq!(loader.zone_resizes(), before + 2);
        assert_eq!(
            loader.altar_zone(Entity(1), 1, 80),
            (0, 80),
            "the next world turned them off"
        );
        assert!(loader.faults().is_empty());
    }
}
