//! More output from the vanilla building recipes, for the same cost.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick::content::recipes::{RecipeEdit, RecipeKind};
use enshwick::state::ModLocal;

/// The recipe rates in force, read while the game builds its recipes.
#[derive(Debug)]
pub(crate) struct RecipeRates {
    /// The output multiplier.
    multiplier: AtomicU32,
    /// Whether each [`Category`] is scaled, in its order.
    scaled: [AtomicBool; 3],
}

/// The recipe rates of this mod life.
static RATES: ModLocal<RecipeRates> = ModLocal::new();

/// The recipe rates in force, none scaled until the settings are read.
pub(crate) fn rates() -> &'static RecipeRates {
    RATES.get_or_init(|| RecipeRates {
        multiplier: AtomicU32::new(1),
        scaled: [const { AtomicBool::new(false) }; 3],
    })
}

impl RecipeRates {
    /// Puts `multiplier` and the switches of each [`Category`], in its
    /// order, in force; whether any of them changed.
    pub(crate) fn set(&self, multiplier: u32, scaled: [bool; 3]) -> bool {
        let mut changed = self.multiplier.swap(multiplier, Ordering::Relaxed) != multiplier;
        for (slot, on) in self.scaled.iter().zip(scaled) {
            changed |= slot.swap(on, Ordering::Relaxed) != on;
        }
        changed
    }

    /// The output multiplier in force.
    pub(crate) fn multiplier(&self) -> u32 {
        self.multiplier.load(Ordering::Relaxed)
    }

    /// Scales the output of `recipe` when its category is in force.
    pub(crate) fn scale(&self, recipe: &mut RecipeEdit<'_>) {
        let view = recipe.view();
        let Some(category) = category(view.kind(), &view.debug_name()) else {
            return;
        };
        if self.scaled[category as usize].load(Ordering::Relaxed) {
            recipe.scale_output(self.multiplier());
        }
    }
}

/// What a recipe makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    /// Building blocks and roofs.
    Blocks,
    /// Overgrowth materials.
    Overgrowth,
    /// Decor and furniture.
    Decor,
}

/// The category of a recipe whose output is of `kind`, `None` for
/// everything else and before the game has loaded the item.
///
/// The game's item data has no kind for decor, so its debug name tells
/// decor and furniture from other props.
fn category(kind: Option<RecipeKind>, name: &str) -> Option<Category> {
    match kind? {
        RecipeKind::BuildingBlock | RecipeKind::Roof => Some(Category::Blocks),
        RecipeKind::Overgrowth => Some(Category::Overgrowth),
        RecipeKind::Other if is_decor(name) => Some(Category::Decor),
        _ => None,
    }
}

/// Whether the prop named `name` is decor or furniture without a function.
fn is_decor(name: &str) -> bool {
    name.starts_with("Prop_Decoration_")
        // Decoration with a function: it holds water or items.
        && !["Water", "Storage", "Chest"].iter().any(|word| name.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_fall_in_their_categories() {
        use RecipeKind::{BuildingBlock, Other, Overgrowth, Roof, Terrain};

        assert_eq!(
            category(Some(BuildingBlock), "Block_T2_Metal_Copper_CRAFTING"),
            Some(Category::Blocks)
        );
        assert_eq!(
            category(Some(Roof), "Block_Roof_T6_Palmleaves"),
            Some(Category::Blocks)
        );
        assert_eq!(
            category(Some(Overgrowth), "Material_T1_Overgrowth_Ivy"),
            Some(Category::Overgrowth)
        );
        assert_eq!(
            category(Some(Other), "Prop_Decoration_Comfort1_T0_Bench"),
            Some(Category::Decor)
        );
        assert_eq!(
            category(Some(Other), "Prop_Decoration_T2_Barrel_Water"),
            None
        );
        assert_eq!(
            category(Some(Other), "Prop_Decoration_T1_Storage_Shelf"),
            None
        );
        assert_eq!(category(Some(Other), "Prop_Storage_32_T2_Chest_Wood"), None);
        assert_eq!(category(Some(Terrain), "Material_T1_Dirt"), None);
        assert_eq!(
            category(Some(Other), "Craft_Weapon_T7_1H_Lore_Axe_Ministory_10"),
            None
        );
        assert_eq!(
            category(None, "Block_T2_Metal_Copper_CRAFTING"),
            None,
            "not loaded yet"
        );
    }

    #[test]
    fn the_categories_index_their_switches() {
        assert_eq!(Category::Blocks as usize, 0);
        assert_eq!(Category::Overgrowth as usize, 1);
        assert_eq!(Category::Decor as usize, 2);
    }

    #[test]
    fn only_a_changed_rate_counts_as_a_change() {
        let rates = RecipeRates {
            multiplier: AtomicU32::new(1),
            scaled: [const { AtomicBool::new(false) }; 3],
        };
        assert!(rates.set(2, [true, true, false]));
        assert!(!rates.set(2, [true, true, false]));
        assert!(rates.set(2, [true, true, true]));
        assert!(rates.set(3, [true, true, true]));
        assert_eq!(rates.multiplier(), 3);
    }
}
