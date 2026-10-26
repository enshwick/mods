//! The settings of the two mods this one merged, under each id they had,
//! carried into the groups of its settings.

use enshwick::migrations::{Migration, Migrations};

/// The count's flat key and the grouped key it became.
const COUNT_KEYS: &[(&str, &str)] = &[("altars_per_flame_level", "count.altars_per_flame_level")];

/// The zones' flat keys and the grouped keys they became.
const ZONE_KEYS: &[(&str, &str)] = &[
    ("clusters", "zones.clusters"),
    ("building_across_seams", "zones.building_across_seams"),
];

/// Every id the merged mods had, the manifest's `was`, with their keys and
/// the keys they became. A value taken later replaces one taken earlier, so
/// each mod's newer id wins over its older one.
pub(crate) const FORMER: [(&str, &[(&str, &str)]); 4] = [
    ("flame-altar-count", COUNT_KEYS),
    ("enshwick.more-altars", COUNT_KEYS),
    ("flame-altar-zones", ZONE_KEYS),
    ("enshwick.flame-altar-zones", ZONE_KEYS),
];

/// Every migration the mod made, oldest first.
pub(crate) fn all() -> Migrations {
    Migrations::new().add("0.2.0", into_groups)
}

/// Takes the former ids' settings into the groups, and drops the flat keys
/// of the file the loader carried over from one of them.
fn into_groups(migration: Migration) -> Migration {
    let taken = FORMER.iter().fold(migration, |m, &(id, keys)| {
        keys.iter()
            .fold(m, |m, &(key, to)| m.take_setting(id, key, to))
    });
    [COUNT_KEYS, ZONE_KEYS]
        .into_iter()
        .flatten()
        .fold(taken, |m, &(key, _)| m.remove_setting(key))
}
