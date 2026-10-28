//! More output from the building recipes, for the same cost, through the
//! recipe output rates of their kinds.

use enshwick::content::recipes::RecipeKind;

/// What a recipe makes, as the settings switch it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    /// Building blocks and roofs.
    Blocks,
    /// Overgrowth materials.
    Overgrowth,
    /// Decorations and furniture of the game's Decorative group.
    Decor,
}

/// The category whose switch scales recipes of `kind`; `None` for a kind
/// the settings leave alone.
pub(crate) const fn category(kind: RecipeKind) -> Option<Category> {
    match kind {
        RecipeKind::BuildingBlock | RecipeKind::Roof => Some(Category::Blocks),
        RecipeKind::Overgrowth => Some(Category::Overgrowth),
        RecipeKind::Decorative => Some(Category::Decor),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_fall_in_their_categories() {
        assert_eq!(category(RecipeKind::BuildingBlock), Some(Category::Blocks));
        assert_eq!(category(RecipeKind::Roof), Some(Category::Blocks));
        assert_eq!(category(RecipeKind::Overgrowth), Some(Category::Overgrowth));
        assert_eq!(category(RecipeKind::Decorative), Some(Category::Decor));
        assert_eq!(category(RecipeKind::Terrain), None);
        assert_eq!(category(RecipeKind::Other), None);
    }
}
