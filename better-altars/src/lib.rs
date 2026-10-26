//! Better Altars: how many Flame Altars each Flame level allows, up to 32 in
//! 16 bases, and Flame Altar zones that touch grow together and can be built
//! across.
//!
//! The two parts were the mods More Altars and Flame altar zones; their
//! settings are the groups `[count]` and `[zones]`, and their former ids carry
//! over through the manifest's `was` and a migration.

mod count;
pub mod geometry;
mod hold;
mod migrations;
mod zones;

pub use count::AltarLimit;
use count::{AltarCount, Count, VANILLA_CAP};
use enshwick::{Load, Mod, Off};
use zones::{AltarZones, Zones};

/// The labels of the settings on the Mods screen, per language.
const TEXTS: &str = include_str!("texts.toml");

/// Better Altars: how many altars, then how their zones grow.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
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
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "enshwick.better-altars",
            side: BOTH,
            name: "Better Altars",
            join: true,
            was: [
                "enshwick.more-altars",
                "flame-altar-count",
                "enshwick.flame-altar-zones",
                "flame-altar-zones",
            ],
            points: [AltarLimit],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let _texts = load.texts(TEXTS)?;
        load.migrate(migrations::all())?;
        let settings: Settings = load.config()?;
        let count = AltarCount::start(load, &settings.count)?;
        let zones = AltarZones::start(load, &settings.zones)?;
        load.on_altar_limit(|level| {
            BetterAltars::instance().map_or(VANILLA_CAP, |altars| altars.count.limit(level))
        })?;
        load.on_config_change(|settings: Settings| {
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

    use enshwick::abi::{AltarInfo, Bounds, Side, Status};
    use enshwick::config::Config as _;
    use enshwick::events::{AltarsChanged, SettingsReplaced};

    use super::*;
    use crate::count::{AltarTable, CAP};

    /// The loader installed on `side` with `settings` and the settings files
    /// of `others`, each a mod id and its file, and the mod started.
    fn started(side: Side, settings: &str, others: &[(&str, &str)]) -> enshwick::test::Installed {
        let host = enshwick::test::FakeHost::new()
            .side(side)
            .settings(settings);
        let loader = others
            .iter()
            .fold(host, |host, &(id, text)| host.other_settings(id, text))
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// An altar at level 1 whose zone is 80 cells wide from `x`.
    fn altar(entity: u32, x: i64) -> AltarInfo {
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
    fn wait_for_growth(loader: &enshwick::test::Installed, altar: u32, resizes: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while (loader.altar_zone(altar, 1, 80).0 == 0 || loader.zone_resizes() < resizes)
            && Instant::now() < deadline
        {
            std::thread::yield_now();
        }
    }

    #[test]
    fn the_default_settings_parse_into_both_groups() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(
            AltarTable::new(&settings.count.altars_per_flame_level, CAP).altars(1),
            4
        );
        assert!(settings.zones.clusters && settings.zones.building_across_seams);
    }

    #[test]
    fn every_setting_and_group_has_its_label_and_description_in_all_languages() {
        let texts = enshwick::i18n::Texts::parse(TEXTS).unwrap();
        assert_eq!(texts.untranslated(), []);
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
    fn the_former_ids_are_the_manifest_s_was() {
        use enshwick::abi::Relation;

        let manifest = BetterAltars::manifest();
        // SAFETY: `manifest!` points `dependencies` at a static array of
        // `dependency_count` entries.
        let all =
            unsafe { std::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) };
        let mut was: Vec<&str> = all
            .iter()
            .filter(|dependency| dependency.relation == Relation::WAS)
            // SAFETY: each id is a static string literal of `manifest!`.
            .filter_map(|dependency| unsafe { dependency.id.as_str() })
            .collect();
        let mut former: Vec<&str> = migrations::FORMER.iter().map(|&(id, _)| id).collect();
        was.sort_unstable();
        former.sort_unstable();
        assert_eq!(was, former);
    }

    const MORE_ALTARS: &str = "altars_per_flame_level = [3, 5, 7]\n";
    const ZONES: &str = "clusters = false\nbuilding_across_seams = false\n";

    #[test]
    fn both_merged_mods_settings_carry_over_into_their_groups() {
        let loader = started(
            Side::CLIENT,
            "",
            &[
                ("enshwick.more-altars", MORE_ALTARS),
                ("enshwick.flame-altar-zones", ZONES),
            ],
        );
        let migrated = loader.migrate_settings("").unwrap();
        let settings: Settings = toml::from_str(&migrated).unwrap();
        assert_eq!(settings.count.altars_per_flame_level, [3, 5, 7]);
        assert!(!settings.zones.clusters);
        assert!(!settings.zones.building_across_seams);
    }

    #[test]
    fn the_file_carried_from_a_former_id_loses_its_flat_keys() {
        for (id, flat) in [
            ("enshwick.more-altars", MORE_ALTARS),
            ("enshwick.flame-altar-zones", ZONES),
        ] {
            let loader = started(Side::CLIENT, "", &[(id, flat)]);
            let migrated = loader.migrate_settings(flat).unwrap();
            let top: toml::Table = toml::from_str(&migrated).unwrap();
            assert!(
                top.values().all(toml::Value::is_table),
                "{id}: only groups are left: {migrated}"
            );
            assert!(
                toml::from_str::<Settings>(&migrated).is_ok(),
                "{id}: {migrated}"
            );
        }
    }

    #[test]
    fn each_mod_s_newer_id_wins_over_its_older_one() {
        let loader = started(
            Side::CLIENT,
            "",
            &[
                ("flame-altar-count", "altars_per_flame_level = [9]\n"),
                ("enshwick.more-altars", MORE_ALTARS),
                (
                    "flame-altar-zones",
                    "clusters = true\nbuilding_across_seams = true\n",
                ),
                ("enshwick.flame-altar-zones", ZONES),
            ],
        );
        let settings: Settings = toml::from_str(&loader.migrate_settings("").unwrap()).unwrap();
        assert_eq!(settings.count.altars_per_flame_level, [3, 5, 7]);
        assert!(!settings.zones.clusters);
    }

    #[test]
    fn a_key_no_former_id_gives_keeps_its_default() {
        let loader = started(
            Side::CLIENT,
            "",
            &[("flame-altar-zones", "clusters = false\n")],
        );
        let settings: Settings = toml::from_str(&loader.migrate_settings("").unwrap()).unwrap();
        assert!(!settings.zones.clusters);
        assert!(settings.zones.building_across_seams);
        assert_eq!(
            settings.count.altars_per_flame_level,
            Count::default().altars_per_flame_level
        );
    }

    #[test]
    fn a_level_outside_1_to_32_altars_is_held_to_the_range() {
        let loader = started(
            Side::CLIENT,
            "[count]\naltars_per_flame_level = [0, 40, 5]\n",
            &[],
        );
        assert_eq!(loader.altar_limit(0), Some(1));
        assert_eq!(loader.altar_limit(1), Some(32));
        assert_eq!(loader.altar_limit(2), Some(5));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn the_table_and_the_seams_are_the_join_values() {
        let server = Settings {
            zones: Zones {
                building_across_seams: false,
                ..Zones::default()
            },
            ..Settings::default()
        };
        let synced = server.synced().unwrap();
        let mut joined: Settings =
            toml::from_str("[count]\naltars_per_flame_level = [1]\n").unwrap();
        joined.merge_synced(&synced).unwrap();
        assert_eq!(
            AltarTable::new(&joined.count.altars_per_flame_level, CAP).altars(3),
            12
        );
        assert!(!joined.zones.building_across_seams);
        assert!(joined.zones.clusters, "clusters are each world's own");
    }

    #[test]
    fn the_table_answers_the_limit_with_the_base_limit_raised() {
        let loader = started(Side::CLIENT, "", &[]);
        assert!(loader.base_limit_raised());
        assert_eq!(loader.altar_limit(1), Some(4));
        assert_eq!(loader.altar_limit(11), Some(32));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn other_mods_change_the_limit_within_the_cap_at_its_former_point() {
        use enshwick::points::Verdict;

        let loader = started(Side::CLIENT, "", &[]);
        assert_eq!(loader.defined_points(), ["flame-altar-count:limit"]);
        assert!(loader.extend_point(|limit: &AltarLimit, altars: &u8| {
            assert_eq!(limit.cap, CAP);
            match limit.flame_level {
                0 => Verdict::Set(0),
                1 => Verdict::Set(altars + 2),
                _ => Verdict::Set(200),
            }
        }));
        assert_eq!(loader.altar_limit(0), Some(1), "at least one");
        assert_eq!(loader.altar_limit(1), Some(6));
        assert_eq!(loader.altar_limit(5), Some(32), "at most the cap");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn it_sizes_the_zones_of_touching_altars_and_accepts_a_box_across_them() {
        let loader = started(Side::CLIENT, &Settings::template(), &[]);
        assert_eq!(loader.altars_opened(), (true, true));
        loader.set_altars(&[altar(1, 0), altar(2, 40)]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        wait_for_growth(&loader, 1, 1);
        let (grow, height) = loader.altar_zone(1, 1, 80);
        assert!(grow > 0, "touching altars grow");
        assert_eq!(height, 80);
        assert_eq!(loader.altar_zone(2, 1, 80), (grow, height));
        assert_eq!(
            loader.altar_zone(2, 2, 80),
            (0, 80),
            "another level: vanilla"
        );
        assert_eq!(
            loader.altar_zone(3, 1, 80),
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
        assert!(loader.zone_check(&zones, across, 0));
        assert!(!loader.zone_check(&zones[..1], across, 0));
        AltarZones::set_seams(false);
        assert!(!loader.zone_check(&zones, across, 0), "seams off: vanilla");
        AltarZones::set_seams(true);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_world_s_own_clusters_value_applies_when_it_loads() {
        let off = "[zones]\nclusters = false\n";
        let loader = started(Side::CLIENT, off, &[]);
        loader.set_altars(&[altar(1, 0), altar(2, 40)]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        assert_eq!(loader.zone_resizes(), 0);
        assert_eq!(
            loader.altar_zone(1, 1, 80),
            (0, 80),
            "clusters off: vanilla"
        );
        loader.set_settings("[zones]\nclusters = true\n");
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        wait_for_growth(&loader, 1, 1);
        assert_eq!(loader.zone_resizes(), 1);
        assert!(
            loader.altar_zone(1, 1, 80).0 > 0,
            "the world turned clusters on"
        );
        loader.set_settings(off);
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        assert_eq!(loader.zone_resizes(), 2);
        assert_eq!(
            loader.altar_zone(1, 1, 80),
            (0, 80),
            "the next world turned them off"
        );
        assert!(loader.faults().is_empty());
    }
}
