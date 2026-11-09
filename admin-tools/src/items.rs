//! Items as an administrator names them: the game's own name of an item,
//! its GUID, a mod's content id or the item's number.

use enshwick::ItemId;
use enshwick::content::Guid;
use enshwick::content::vanilla::{self, VanillaError};

/// The longest item name an action takes, in bytes.
pub(crate) const MAX_NAME: usize = 120;

/// Why an item name names no item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unknown {
    /// No item has the name; `near` lists names that look like it.
    Name { near: Vec<String> },
    /// Several items have the name: each one's GUID.
    Several(Vec<Guid>),
    /// The game's item list does not read.
    Unreadable,
}

/// The item `text` names: a GUID, a content id (`<mod>:<name>`), the
/// game's name of an item in any case, or the item's number.
pub(crate) fn resolve(text: &str) -> Result<ItemId, Unknown> {
    let text = text.trim();
    if let Some(guid) = Guid::try_parse(text).or_else(|| Guid::try_named(text)) {
        return Ok(ItemId::from_guid(guid));
    }
    match by_name(text, vanilla::item) {
        Ok(guid) => Ok(ItemId::from_guid(guid)),
        Err(unknown) => text.parse().map(ItemId::new).map_err(|_| unknown),
    }
}

/// The item named `name` by `lookup`; a name that differs only in case
/// from one the lookup suggests names that one.
fn by_name(
    name: &str,
    lookup: impl Fn(&str) -> Result<Guid, VanillaError>,
) -> Result<Guid, Unknown> {
    match lookup(name) {
        Ok(guid) => Ok(guid),
        Err(VanillaError::Unknown { near, .. }) => {
            match near.iter().find(|near| near.eq_ignore_ascii_case(name)) {
                Some(exact) => lookup(exact).map_err(|_| Unknown::Name { near }),
                None => Err(Unknown::Name { near }),
            }
        }
        Err(VanillaError::Ambiguous { items, .. }) => Err(Unknown::Several(items)),
        Err(_) => Err(Unknown::Unreadable),
    }
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
        assert_eq!(by_name("tool_glider_t1", glider), Ok(GLIDER));
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
            resolve(" 3f0c8d44-6b2a-4c1e-9d77-0a5be1c2d3e4 "),
            Ok(ItemId::from_guid(GLIDER))
        );
        assert_eq!(
            resolve("you.lockers:locker"),
            Ok(ItemId::named("you.lockers:locker"))
        );
        assert_eq!(resolve("3073"), Ok(ItemId::new(3073)));
        assert_eq!(resolve("Tool_Glider_T1"), Err(Unknown::Unreadable));
    }
}
