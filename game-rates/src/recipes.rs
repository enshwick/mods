//! More output from the building recipes, for the same cost: blocks, roofs
//! and overgrowth through the recipe output rates of their kinds, decor
//! through a recipe patch of its own.

use core::sync::atomic::{AtomicU32, Ordering};

use enshwick::content::recipes::{RecipeEdit, RecipeKind};
use enshwick::state::ModLocal;

/// What a recipe makes, as the settings switch it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    /// Building blocks and roofs.
    Blocks,
    /// Overgrowth materials.
    Overgrowth,
}

/// The category whose switch scales recipes of `kind`; `None` for a kind
/// the settings leave alone.
pub(crate) const fn category(kind: RecipeKind) -> Option<Category> {
    match kind {
        RecipeKind::BuildingBlock | RecipeKind::Roof => Some(Category::Blocks),
        RecipeKind::Overgrowth => Some(Category::Overgrowth),
        _ => None,
    }
}

/// The decor multiplier in force, 1 when decor stays vanilla; read while
/// the game builds its recipes.
static DECOR: ModLocal<AtomicU32> = ModLocal::new();

/// The decor multiplier of this mod life, vanilla until the settings are
/// read.
fn decor() -> &'static AtomicU32 {
    DECOR.get_or_init(|| AtomicU32::new(1))
}

/// Puts `multiplier` in force for decor; whether it changed.
pub(crate) fn set_decor(multiplier: u32) -> bool {
    decor().swap(multiplier, Ordering::Relaxed) != multiplier
}

/// The decor multiplier in force.
pub(crate) fn decor_multiplier() -> u32 {
    decor().load(Ordering::Relaxed)
}

/// Scales the output of `recipe` when it makes decor and decor is scaled.
pub(crate) fn scale_decor(recipe: &mut RecipeEdit<'_>) {
    let multiplier = decor_multiplier();
    if multiplier == 1 {
        return;
    }
    let view = recipe.view();
    if view.kind() == Some(RecipeKind::Other) && is_decor(&view.debug_name()) {
        recipe.scale_output(multiplier);
    }
}

/// Whether the prop named `name` is decor or furniture without a function.
///
/// The game's item data gives decor no kind of its own, so the recipe's
/// debug name tells decor and furniture from other props.
fn is_decor(name: &str) -> bool {
    name.starts_with("Prop_Decoration_")
        // Decoration with a function: it holds water or items.
        && !["Water", "Storage", "Chest"].iter().any(|word| name.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_fall_in_their_categories() {
        assert_eq!(category(RecipeKind::BuildingBlock), Some(Category::Blocks));
        assert_eq!(category(RecipeKind::Roof), Some(Category::Blocks));
        assert_eq!(category(RecipeKind::Overgrowth), Some(Category::Overgrowth));
        assert_eq!(category(RecipeKind::Terrain), None);
        assert_eq!(category(RecipeKind::Other), None);
    }

    #[test]
    fn decor_is_told_from_props_with_a_function() {
        assert!(is_decor("Prop_Decoration_Comfort1_T0_Bench"));
        assert!(!is_decor("Prop_Decoration_T2_Barrel_Water"));
        assert!(!is_decor("Prop_Decoration_T1_Storage_Shelf"));
        assert!(!is_decor("Prop_Storage_32_T2_Chest_Wood"));
        assert!(!is_decor("Craft_Weapon_T7_1H_Lore_Axe_Ministory_10"));
    }

    #[test]
    fn only_a_changed_decor_multiplier_counts_as_a_change() {
        assert!(set_decor(2));
        assert!(!set_decor(2));
        assert!(set_decor(1));
        assert_eq!(decor_multiplier(), 1);
    }
}
