//! The settings of this mod's former id and of the mods it folded in,
//! carried into the groups of its settings.

use enshwick::migrations::{Migration, Migrations};

/// The id the mod had before ids named their author: the loader copies its
/// files under the new id.
pub(crate) const FORMER_ID: &str = "game-rates";

/// Each key of the former id's flat settings and the grouped key it became.
const OWN_KEYS: [(&str, &str); 5] = [
    ("recipe_multiplier", "recipes.multiplier"),
    ("recipe_blocks", "recipes.blocks"),
    ("recipe_overgrowth", "recipes.overgrowth"),
    ("recipe_decor", "recipes.decor"),
    ("comfort_multiplier", "comfort.multiplier"),
];

/// The keys of the older recipe mods and the keys they became.
const RECIPE_KEYS: &[(&str, &str)] = &[
    ("multiplier", "recipes.multiplier"),
    ("blocks", "recipes.blocks"),
    ("overgrowth", "recipes.overgrowth"),
    ("decor", "recipes.decor"),
];

/// The mods this one folded in, the manifest's `replaces`, with their keys
/// and the keys they became. A value taken later replaces one taken
/// earlier, so Building materials wins over Building yield, and the former
/// id's own values, taken last, win over both.
pub(crate) const FOLDED: [(&str, &[(&str, &str)]); 3] = [
    ("building-yield", RECIPE_KEYS),
    ("building-materials", RECIPE_KEYS),
    ("furniture-comfort", &[("multiplier", "comfort.multiplier")]),
];

/// Every migration the mod made, oldest first.
pub(crate) fn all() -> Migrations {
    Migrations::new().add("0.2.0", into_groups)
}

/// Takes the folded mods' settings and the former id's flat keys into the
/// groups, and drops the flat keys.
fn into_groups(migration: Migration) -> Migration {
    let folded = FOLDED.iter().fold(migration, |m, &(id, keys)| {
        keys.iter()
            .fold(m, |m, &(key, to)| m.take_setting(id, key, to))
    });
    OWN_KEYS.iter().fold(folded, |m, &(key, to)| {
        m.take_setting(FORMER_ID, key, to).remove_setting(key)
    })
}
