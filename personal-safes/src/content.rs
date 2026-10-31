//! The Safe as game content: an item cloned from the wooden chest that places
//! the same vanilla chest, and a recipe beside the chest's.

use enshwick::ItemId;
use enshwick::content::items::NewItem;
use enshwick::content::recipes::NewRecipe;
use enshwick::content::{Guid, Id};

/// The vanilla wooden chest item.
pub const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// The Safe item.
pub const SAFE: Id = Id::new("enshwick.personal-safes:safe");
/// The Safe's recipe.
pub const SAFE_RECIPE: Id = Id::new("enshwick.personal-safes:safe.recipe");
/// The Safe's name text.
pub const SAFE_NAME: Id = Id::new("enshwick.personal-safes:safe.name");
/// The Safe's description text.
pub const SAFE_DESCRIPTION: Id = Id::new("enshwick.personal-safes:safe.description");
/// Whether `item` is a Safe.
#[must_use]
pub fn is_safe(item: ItemId) -> bool {
    item == ItemId::from_guid(SAFE.guid())
}

/// The mod's texts, per language.
pub const TEXTS: &str = include_str!("texts.toml");

/// The Safe item.
#[must_use]
pub fn item() -> NewItem {
    NewItem {
        name: Some(SAFE_NAME.guid()),
        description: Some(SAFE_DESCRIPTION.guid()),
        ..NewItem::new(SAFE, CHEST)
    }
}

/// The Safe's recipe: the chest's, its ingredients times `cost`.
#[must_use]
pub const fn recipe(cost: u32) -> NewRecipe {
    NewRecipe {
        id: SAFE_RECIPE,
        like: CHEST,
        output: SAFE.guid(),
        output_count: 1,
        cost,
        workshop: None,
    }
}

#[cfg(test)]
mod tests {
    use enshwick::Off;
    use enshwick::i18n::{Language, Texts};

    use super::*;

    #[test]
    fn every_text_exists_in_all_languages() -> Result<(), Off> {
        use enshwick::i18n::{MOD_DESCRIPTION, MOD_NAME};

        let texts = Texts::parse(TEXTS)?;
        assert_eq!(texts.untranslated(), []);
        assert_eq!(texts.missing_in(Language::OFFERED), []);
        for key in [
            MOD_NAME,
            MOD_DESCRIPTION,
            crate::keys::SAFE_NAME,
            crate::keys::SAFE_DESCRIPTION,
        ] {
            assert!(texts.get(key, Language::FALLBACK).is_some(), "{key}");
        }
        Ok(())
    }

    #[test]
    fn the_safe_is_a_chest_with_its_own_identity() {
        assert_ne!(SAFE.guid(), CHEST);
        assert_eq!(item().like, CHEST);
        assert_eq!(recipe(3).cost, 3);
        assert_eq!(recipe(3).output, SAFE.guid());
    }
}
