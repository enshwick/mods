//! Chests of items: a wooden chest placed beside a player or at a position,
//! then filled with items.

use core::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::content::vanilla;
use enshwick::inventory::{Batch, Op};
use enshwick::world::{TransactionId, TransactionStatus, WorldTransform};
use enshwick::{Entity, ItemId, Off, WorldError};

use crate::inventory::report;
use crate::players::{axis, no_player};
use crate::{Late, Tools, ids, items, texts};

/// The debug name of the vanilla wooden chest, as the item registry of
/// server 399 and client 426 lists it.
const CHEST: &str = "Prop_Storage_32_T2_Chest_Wood";

/// How far beside a player a chest stands, in metres along x, so it does
/// not stand in them.
const BESIDE: f64 = 2.0;

/// How often a placed chest is looked for while the game places it.
const LOOK: Duration = Duration::from_millis(250);

/// How many looks a chest gets to be placed before the fill gives up.
const LOOKS: u32 = 40;

/// Declares the chest actions.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    if tools.inventories.is_none() {
        return Ok(());
    }
    tools.add(
        Action::new(ids::CHEST, texts::ACTION_CHEST)
            .help(texts::ACTION_CHEST_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(items::count_arg())
            .arg(items::item_arg()),
        chest,
    )?;
    tools.add(
        Action::new(ids::CHEST_AT, texts::ACTION_CHESTAT)
            .help(texts::ACTION_CHESTAT_HELP)
            .arg(axis(texts::ARG_X))
            .arg(axis(texts::ARG_Y))
            .arg(axis(texts::ARG_Z))
            .arg(items::count_arg())
            .arg(items::item_arg()),
        chest_at,
    )
}

/// Places a chest beside a player and fills it.
fn chest(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Number(count),
        Value::Text(text),
    ] = request.args
    else {
        return no_player(request);
    };
    let Some([x, y, z]) = tools
        .world
        .player_by_key(*key)
        .and_then(|player| tools.world.position(player.entity))
    else {
        return no_player(request);
    };
    place(
        tools,
        request,
        ids::CHEST,
        [x + BESIDE, y, z],
        (*count, text),
        name,
    )
    .unwrap_or_else(|refused| refused)
}

/// Places a chest at a position and fills it.
fn chest_at(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Number(x),
        Value::Number(y),
        Value::Number(z),
        Value::Number(count),
        Value::Text(text),
    ] = request.args
    else {
        return Tools::failed(request, ids::CHEST_AT, &WorldError::Refused);
    };
    let at = format!("{x:.0}, {y:.0}, {z:.0}");
    place(
        tools,
        request,
        ids::CHEST_AT,
        [*x, *y, *z],
        (*count, text),
        &at,
    )
    .unwrap_or_else(|refused| refused)
}

/// Places a wooden chest at `at` for the action `id` and, once the game
/// has placed it, fills it with the `count` items `text` names;
/// `whereabouts` names the place in the replies. A refusal is the error.
fn place(
    tools: &Tools,
    request: &Request<'_>,
    id: &'static str,
    at: [f64; 3],
    (count, text): (f64, &str),
    whereabouts: &str,
) -> Result<Outcome, Outcome> {
    let count = crate::whole(request, count)?;
    let (item, shown) = items::known_item(tools, request, text)?;
    let Ok(chest) = vanilla::item(CHEST).map(ItemId::from_guid) else {
        return Err(Outcome::refused(request.say(texts::REPLY_NO_CHEST, &[])));
    };
    let Some(late) = Late::new(tools, request, id) else {
        return Err(Tools::failed(request, whereabouts, &WorldError::Missing));
    };
    let placed = tools
        .world
        .transaction()
        .place_prop(chest, WorldTransform::at(at))
        .commit()
        .map_err(|why| Tools::failed(request, whereabouts, &why))?;
    let reply = request.say(
        texts::REPLY_CHEST,
        &[
            (texts::value::NAME, whereabouts),
            (texts::value::COUNT, &count.to_string()),
            (texts::value::ITEM, &shown),
        ],
    );
    fill(late, placed, (item, count), shown, whereabouts.to_owned());
    Ok(Outcome::done(reply))
}

/// What a look at a chest's placement finds.
#[derive(Debug, PartialEq, Eq)]
enum Look {
    /// The game has not placed it yet.
    Wait,
    /// It stands in the world, as this entity.
    Placed(Entity),
    /// It will not stand: the placement ended otherwise, or took too long.
    Lost,
}

/// What the look number `looks` finds of a placement at `status` that lists
/// `chest`. A chest listed before its transaction is applied is not in the
/// world yet, so a give to it would roll back.
fn look(status: Option<&TransactionStatus>, chest: Option<Entity>, looks: u32) -> Look {
    match (status, chest) {
        (Some(TransactionStatus::Applied), Some(chest)) => Look::Placed(chest),
        (Some(TransactionStatus::Pending | TransactionStatus::Applied), _) if looks < LOOKS => {
            Look::Wait
        }
        _ => Look::Lost,
    }
}

/// Looks for the chest the transaction `placed` puts in the world and
/// gives it `count` items `item`, shown as `shown`, in as many stacks as
/// fit. A chest that does not get placed is told to the caller, and its
/// placement undone, so no empty chest follows.
fn fill(
    late: Arc<Late>,
    placed: TransactionId,
    (item, count): (ItemId, u32),
    shown: String,
    whereabouts: String,
) {
    let scheduler = late.tools.scheduler;
    let mut looks = 0;
    scheduler.every(LOOK, move || {
        let world = late.tools.world;
        let status = world.transaction_status(placed);
        let chest = world.transaction_entities(placed).first().copied();
        looks += 1;
        match look(status.as_ref(), chest, looks) {
            Look::Wait => return ControlFlow::Continue(()),
            Look::Placed(chest) => {
                if let Some(inventories) = &late.tools.inventories {
                    let batch = Batch::new()
                        .with(Op::Give {
                            to: chest,
                            item,
                            count,
                            fill: None,
                        })
                        .allow_partial();
                    let done = report(Arc::clone(&late), shown.clone(), count, whereabouts.clone());
                    let _ticket = inventories.submit(&batch, done);
                }
            }
            Look::Lost => {
                enshwick::warn!("chest of {item:?} not placed: {status:?}");
                if let Err(why) = world.undo(placed) {
                    enshwick::debug!("chest placement not undone: {why}");
                }
                late.report(
                    false,
                    texts::REPLY_CHEST_NOT_FILLED,
                    &[
                        (texts::value::NAME, &whereabouts),
                        (texts::value::COUNT, &count.to_string()),
                        (texts::value::ITEM, &shown),
                    ],
                );
            }
        }
        ControlFlow::Break(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chest_listed_while_its_placement_is_pending_waits_for_it_to_be_applied() {
        let chest = Some(Entity(7));
        assert_eq!(
            look(Some(&TransactionStatus::Pending), chest, 1),
            Look::Wait
        );
        assert_eq!(
            look(Some(&TransactionStatus::Applied), chest, 1),
            Look::Placed(Entity(7))
        );
        assert_eq!(look(Some(&TransactionStatus::Applied), None, 1), Look::Wait);
    }

    #[test]
    fn a_placement_that_ended_otherwise_or_took_every_look_is_lost() {
        let chest = Some(Entity(7));
        assert_eq!(
            look(Some(&TransactionStatus::Pending), chest, LOOKS),
            Look::Lost
        );
        assert_eq!(
            look(Some(&TransactionStatus::Applied), None, LOOKS),
            Look::Lost
        );
        assert_eq!(look(Some(&TransactionStatus::Undone), chest, 1), Look::Lost);
        let failed = TransactionStatus::Failed(WorldError::NotLoaded);
        assert_eq!(look(Some(&failed), None, 1), Look::Lost);
        assert_eq!(look(None, None, 1), Look::Lost);
    }
}
