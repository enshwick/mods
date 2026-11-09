//! Items and recipes as an administrator names them: the game's own name,
//! the GUID, a mod's content id or the number.

use enshwick::ItemId;
use enshwick::actions::{Outcome, Request};
use enshwick::content::Guid;
use enshwick::content::vanilla::{self, VanillaError};
use enshwick::knowledge::KnowledgeId;

use crate::texts;

/// The longest item or recipe name an action takes, in bytes.
pub(crate) const MAX_NAME: usize = 120;

/// Why a name names nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unknown {
    /// Nothing has the name; `near` lists names that look like it.
    Name { near: Vec<String> },
    /// Several have the name: each one's GUID.
    Several(Vec<Guid>),
    /// The game's list does not read.
    Unreadable,
}

/// The item `text` names, a GUID, a content id (`<mod>:<name>`), the game's
/// name of an item in any case, or the item's number, with the name to
/// show for it.
pub(crate) fn item(text: &str) -> Result<(ItemId, String), Unknown> {
    resolve(text, vanilla::item, ItemId::from_guid, ItemId::new)
}

/// The knowledge `text` names, a recipe's GUID, content id or game's name
/// in any case, or any knowledge's number, with the name to show for it.
pub(crate) fn knowledge(text: &str) -> Result<(KnowledgeId, String), Unknown> {
    resolve(
        text,
        vanilla::recipe,
        KnowledgeId::from_guid,
        KnowledgeId::new,
    )
}

/// What `text` names, a GUID or content id through `from_guid`, a name
/// through `lookup`, else a number through `new`, with the game's spelling
/// of a name or else the text as typed.
fn resolve<T>(
    text: &str,
    lookup: impl Fn(&str) -> Result<Guid, VanillaError>,
    from_guid: fn(Guid) -> T,
    new: fn(u32) -> T,
) -> Result<(T, String), Unknown> {
    let text = text.trim();
    if let Some(guid) = Guid::try_parse(text).or_else(|| Guid::try_named(text)) {
        return Ok((from_guid(guid), text.to_owned()));
    }
    match by_name(text, lookup) {
        Ok((guid, name)) => Ok((from_guid(guid), name)),
        Err(unknown) => text
            .parse()
            .map(|number| (new(number), text.to_owned()))
            .map_err(|_| unknown),
    }
}

/// What `lookup` finds named `name`, with the game's spelling; a name that
/// differs only in case from one the lookup suggests names that one.
fn by_name(
    name: &str,
    lookup: impl Fn(&str) -> Result<Guid, VanillaError>,
) -> Result<(Guid, String), Unknown> {
    match lookup(name) {
        Ok(guid) => Ok((guid, name.to_owned())),
        Err(VanillaError::Unknown { near, .. } | VanillaError::UnknownRecipe { near, .. }) => {
            match near.iter().find(|near| near.eq_ignore_ascii_case(name)) {
                Some(exact) => lookup(exact)
                    .map(|guid| (guid, exact.clone()))
                    .map_err(|_| Unknown::Name { near }),
                None => Err(Unknown::Name { near }),
            }
        }
        Err(
            VanillaError::Ambiguous { items: several, .. }
            | VanillaError::AmbiguousRecipe {
                recipes: several, ..
            },
        ) => Err(Unknown::Several(several)),
        Err(_) => Err(Unknown::Unreadable),
    }
}

/// The refusal of a run whose `text` names nothing, with the names near it
/// or the GUIDs to choose from.
pub(crate) fn refused(request: &Request<'_>, text: &str, why: &Unknown) -> Outcome {
    let named = |key, near: &str| {
        request.say(
            key,
            &[(texts::value::ITEM, text), (texts::value::NEAR, near)],
        )
    };
    Outcome::refused(match why {
        Unknown::Name { near } if near.is_empty() => named(texts::REPLY_NO_ITEM, ""),
        Unknown::Name { near } => named(texts::REPLY_NO_ITEM_NEAR, &near.join(", ")),
        Unknown::Several(guids) => {
            let guids: Vec<String> = guids.iter().map(ToString::to_string).collect();
            named(texts::REPLY_SEVERAL_ITEMS, &guids.join(", "))
        }
        Unknown::Unreadable => named(texts::REPLY_ITEMS_UNREAD, ""),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLIDER: Guid = Guid::parse("3f0c8d44-6b2a-4c1e-9d77-0a5be1c2d3e4");

    fn glider(name: &str) -> Result<Guid, VanillaError> {
        if name == "Tool_Glider_T1" {
            return Ok(GLIDER);
        }
        Err(VanillaError::Unknown {
            name: name.to_owned(),
            near: vec!["Tool_Glider_T1".into(), "Tool_Glider_T2".into()],
        })
    }

    #[test]
    fn a_name_in_another_case_names_the_item_the_game_spells_otherwise() {
        assert_eq!(
            by_name("tool_glider_t1", glider),
            Ok((GLIDER, "Tool_Glider_T1".to_owned()))
        );
    }

    #[test]
    fn an_unknown_name_offers_the_names_near_it() {
        assert_eq!(
            by_name("Glider", glider),
            Err(Unknown::Name {
                near: vec!["Tool_Glider_T1".into(), "Tool_Glider_T2".into()]
            })
        );
    }

    #[test]
    fn a_name_several_items_carry_lists_their_guids() {
        let several = |_: &str| {
            Err(VanillaError::Ambiguous {
                name: "Chest".into(),
                items: vec![GLIDER],
            })
        };
        assert_eq!(
            by_name("Chest", several),
            Err(Unknown::Several(vec![GLIDER]))
        );
    }

    #[test]
    fn a_guid_a_content_id_and_a_number_each_name_an_item_without_the_game() {
        assert_eq!(
            item(" 3f0c8d44-6b2a-4c1e-9d77-0a5be1c2d3e4 ").map(|(id, _)| id),
            Ok(ItemId::from_guid(GLIDER))
        );
        assert_eq!(
            item("you.lockers:locker"),
            Ok((
                ItemId::named("you.lockers:locker"),
                "you.lockers:locker".to_owned()
            ))
        );
        assert_eq!(item("3073"), Ok((ItemId::new(3073), "3073".to_owned())));
        assert_eq!(item("Tool_Glider_T1"), Err(Unknown::Unreadable));
    }

    #[test]
    fn a_recipe_s_guid_names_the_knowledge_the_game_keeps_for_it() {
        assert_eq!(
            knowledge("3f0c8d44-6b2a-4c1e-9d77-0a5be1c2d3e4").map(|(id, _)| id),
            Ok(enshwick::RecipeId::from_guid(GLIDER).into())
        );
        assert_eq!(knowledge("17"), Ok((KnowledgeId::new(17), "17".to_owned())));
    }

    #[test]
    fn an_unknown_recipe_offers_the_recipe_names_near_it() {
        let recipes = |name: &str| {
            Err(VanillaError::UnknownRecipe {
                name: name.to_owned(),
                near: vec!["Tool_Torch01".into()],
            })
        };
        assert_eq!(
            by_name("Tool_Torch1", recipes),
            Err(Unknown::Name {
                near: vec!["Tool_Torch01".into()]
            })
        );
    }
}
