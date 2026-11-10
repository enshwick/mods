//! Items as an administrator picks them and recipes as they name them: the
//! game's own name, the GUID, a mod's content id or the number.

use enshwick::actions::{Arg, Outcome, Request, Value};
use enshwick::content::Guid;
use enshwick::content::items::vanilla_name;
use enshwick::content::vanilla::{self, Vanilla, VanillaError};
use enshwick::i18n::Language;
use enshwick::knowledge::KnowledgeId;

use crate::texts;

/// The longest item or recipe name an action takes, in bytes.
pub(crate) const MAX_NAME: usize = 120;

/// The most items one give hands over, in as many stacks as they need.
pub(crate) const MAX_COUNT: f64 = 9_999.0;

/// The argument that counts the items of a give, the last one, one when
/// left out.
pub(crate) fn count_arg() -> Arg {
    Arg::number(texts::ARG_COUNT, 1.0..=MAX_COUNT, 1.0).optional(Value::Number(1.0))
}

/// The item of a give: a window lists the game's items by the names the
/// admin's own game shows; the platform checks the server knows the pick.
pub(crate) fn item_arg() -> Arg {
    Arg::item(texts::ARG_ITEM)
}

/// The name to tell for `item`: the one the game shows in `language` where
/// its texts read, else its debug name, else its GUID, as for a mod's item.
pub(crate) fn name(item: Guid, language: Language) -> String {
    if let Some(shown) = vanilla_name(item, language) {
        return shown.to_owned();
    }
    Vanilla::game()
        .and_then(|game| game.items().ok()?.iter().find(|i| i.guid == item))
        .map_or_else(|| item.to_string(), |found| found.name.clone())
}

/// Why a name names nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ResolveItemError {
    /// Nothing has the name; `near` lists names that look like it.
    #[error("nothing has the name")]
    Name { near: Vec<String> },
    /// Several have the name: each one's GUID.
    #[error("several have the name")]
    Several(Vec<Guid>),
    /// The game's list does not read.
    #[error("the game's list does not read")]
    Unreadable,
}

/// The knowledge `text` names, a recipe's GUID, content id or game's name
/// in any case, or any knowledge's number, with the name to show for it.
pub(crate) fn knowledge(text: &str) -> Result<(KnowledgeId, String), ResolveItemError> {
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
) -> Result<(T, String), ResolveItemError> {
    let text = text.trim();
    if let Some(guid) = Guid::try_parse(text).or_else(|| Guid::try_named(text)) {
        return Ok((from_guid(guid), text.to_owned()));
    }
    // No name is all digits, and the name searches cost the game thread.
    if let Ok(number) = text.parse() {
        return Ok((new(number), text.to_owned()));
    }
    by_name(text, lookup).map(|(guid, name)| (from_guid(guid), name))
}

/// What `lookup` finds named `name`, with the game's spelling; a name that
/// differs only in case from one the lookup suggests names that one.
fn by_name(
    name: &str,
    lookup: impl Fn(&str) -> Result<Guid, VanillaError>,
) -> Result<(Guid, String), ResolveItemError> {
    match lookup(name) {
        Ok(guid) => Ok((guid, name.to_owned())),
        Err(VanillaError::Unknown { near, .. } | VanillaError::UnknownRecipe { near, .. }) => {
            match near.iter().find(|near| near.eq_ignore_ascii_case(name)) {
                Some(exact) => lookup(exact)
                    .map(|guid| (guid, exact.clone()))
                    .map_err(|_| ResolveItemError::Name { near }),
                None => Err(ResolveItemError::Name { near }),
            }
        }
        Err(
            VanillaError::Ambiguous { items: several, .. }
            | VanillaError::AmbiguousRecipe {
                recipes: several, ..
            },
        ) => Err(ResolveItemError::Several(several)),
        Err(_) => Err(ResolveItemError::Unreadable),
    }
}

/// The refusal of a run whose `text` names nothing, with the names near it
/// or the GUIDs to choose from.
pub(crate) fn refused(request: &Request<'_>, text: &str, why: &ResolveItemError) -> Outcome {
    let say = |key, near: &str| {
        request.say(
            key,
            &[(texts::value::ITEM, text), (texts::value::NEAR, near)],
        )
    };
    let parts = match why {
        ResolveItemError::Name { near } if near.is_empty() => vec![say(texts::REPLY_NO_ITEM, "")],
        ResolveItemError::Name { near } => vec![
            say(texts::REPLY_NO_ITEM, ""),
            say(texts::REPLY_NEAR, &near.join(", ")),
        ],
        ResolveItemError::Several(guids) => {
            let guids: Vec<String> = guids.iter().map(ToString::to_string).collect();
            vec![
                say(texts::REPLY_SEVERAL_ITEMS, ""),
                say(texts::REPLY_PICK_GUID, &guids.join(", ")),
            ]
        }
        ResolveItemError::Unreadable => vec![
            say(texts::REPLY_ITEMS_UNREAD, ""),
            say(texts::REPLY_BY_GUID, ""),
        ],
    };
    Outcome::refused(crate::lines(request, &parts))
}

#[cfg(test)]
mod tests {
    use enshwick::ItemId;

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
            Err(ResolveItemError::Name {
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
            Err(ResolveItemError::Several(vec![GLIDER]))
        );
    }

    #[test]
    fn a_number_names_its_item_without_searching_the_names() {
        let searched = |_: &str| -> Result<Guid, VanillaError> { panic!("a name search") };
        assert_eq!(
            resolve("3073", searched, ItemId::from_guid, ItemId::new),
            Ok((ItemId::new(3073), "3073".to_owned()))
        );
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
            Err(ResolveItemError::Name {
                near: vec!["Tool_Torch01".into()]
            })
        );
    }

    #[test]
    fn a_picked_item_outside_the_game_is_told_by_its_guid() {
        assert_eq!(name(GLIDER, Language::EN_US), GLIDER.to_string());
    }
}
