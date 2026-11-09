//! Actions on what players carry: giving an item, clearing an inventory
//! and copying one into another.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::inventory::{Batch, Inventories, Op, Slot};
use enshwick::world::Player;
use enshwick::{Entity, Off, WorldError};

use crate::items::{self, Unknown};
use crate::{Tools, ids, texts};

/// The most items one give hands over; the item's stack size bounds it
/// further.
pub(crate) const MAX_COUNT: f64 = 9_999.0;

/// Declares the actions on what players carry.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    if tools.inventories.is_none() {
        return Ok(());
    }
    tools.add(
        Action::new(ids::GIVE, texts::ACTION_GIVE)
            .help(texts::ACTION_GIVE_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(Arg::number(texts::ARG_COUNT, 1.0..=MAX_COUNT, 1.0))
            .arg(Arg::text(texts::ARG_ITEM, items::MAX_NAME)),
        give,
    )?;
    tools.add(
        Action::new(ids::CLEAR, texts::ACTION_CLEAR)
            .help(texts::ACTION_CLEAR_HELP)
            .arg(Arg::player(texts::ARG_PLAYER)),
        clear,
    )?;
    tools.add(
        Action::new(ids::COPY, texts::ACTION_COPY)
            .help(texts::ACTION_COPY_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(Arg::player(texts::ARG_TO)),
        copy,
    )
}

/// Destroys every stack in `slots`.
fn clear_ops(slots: &[Slot]) -> Vec<Op> {
    slots
        .iter()
        .filter(|slot| !slot.is_empty())
        .map(|slot| Op::DestroySlot(slot.at))
        .collect()
}

/// Gives `to` new items of each stack in `slots`, as many as it holds.
fn copy_ops(slots: &[Slot], to: Entity) -> Vec<Op> {
    slots
        .iter()
        .filter(|slot| !slot.is_empty())
        .map(|slot| Op::Give {
            to,
            item: slot.item,
            count: slot.count,
            fill: None,
        })
        .collect()
}

/// Submits `ops` in as many transactions as they need; a part the game
/// turns down is logged and told to the caller as `failed`.
fn submit(
    tools: &Tools,
    inventories: &Inventories,
    request: &Request<'_>,
    ops: &[Op],
    failed: &str,
) {
    let mut batch = Batch::new();
    for op in ops {
        batch.push(*op);
    }
    for part in batch.split() {
        let (caller, world, failed) = (request.player.clone(), tools.world, failed.to_owned());
        let _ticket = inventories.submit(&part, move |outcome| {
            if !outcome.is_committed() {
                enshwick::warn!("inventory change turned down: {:?}", outcome.done);
                tell(world, &caller, &failed);
            }
        });
    }
}

/// Destroys everything a player carries, equipment and action bar
/// included.
fn clear(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let (Some(inventories), Some(player)) = (&tools.inventories, tools.world.player_by_key(*key))
    else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let values = [(texts::value::NAME, name.as_str())];
    let ops = clear_ops(&inventories.slots(player.entity));
    if ops.is_empty() {
        return Outcome::refused(request.say(texts::REPLY_NOTHING_CARRIED, &values));
    }
    let failed = request.say(texts::REPLY_NOT_CHANGED, &values);
    submit(tools, inventories, request, &ops, &failed);
    Outcome::done(request.say(texts::REPLY_CLEARED, &values))
}

/// Gives a player new items of every stack another carries; what does not
/// fit stays out, and the items start new, without their wear or level.
fn copy(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Player {
            key: to,
            name: to_name,
        },
    ] = request.args
    else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let (Some(inventories), Some(from), Some(target)) = (
        &tools.inventories,
        tools.world.player_by_key(*key),
        tools.world.player_by_key(*to),
    ) else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let values = [
        (texts::value::NAME, name.as_str()),
        (texts::value::TO, to_name.as_str()),
    ];
    if from.key == target.key {
        return Outcome::refused(request.say(texts::REPLY_COPY_SELF, &values));
    }
    let ops = copy_ops(&inventories.slots(from.entity), target.entity);
    if ops.is_empty() {
        return Outcome::refused(request.say(texts::REPLY_NOTHING_CARRIED, &values));
    }
    let failed = request.say(texts::REPLY_NOT_CHANGED, &[(texts::value::NAME, to_name)]);
    submit(tools, inventories, request, &ops, &failed);
    Outcome::done(request.say(texts::REPLY_COPIED, &values))
}

/// The refusal for an item text that names no item the game knows.
fn unknown(request: &Request<'_>, text: &str, why: &Unknown) -> Outcome {
    let item = [(texts::value::ITEM, text)];
    let reply = match why {
        Unknown::Name { near } if near.is_empty() => request.say(texts::REPLY_NO_ITEM, &item),
        Unknown::Name { near } => request.say(
            texts::REPLY_NO_ITEM_NEAR,
            &[
                (texts::value::ITEM, text),
                (texts::value::NEAR, &near.join(", ")),
            ],
        ),
        Unknown::Several(guids) => {
            let guids: Vec<String> = guids.iter().map(ToString::to_string).collect();
            request.say(
                texts::REPLY_SEVERAL_ITEMS,
                &[
                    (texts::value::ITEM, text),
                    (texts::value::NEAR, &guids.join(", ")),
                ],
            )
        }
        Unknown::Unreadable => request.say(texts::REPLY_ITEMS_UNREAD, &item),
    };
    Outcome::refused(reply)
}

/// Gives a player `count` new items, at most one stack; a give the game
/// turns down later, such as for a full inventory, is told to the caller.
fn give(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Number(count),
        Value::Text(text),
    ] = request.args
    else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let Some(inventories) = &tools.inventories else {
        return Tools::failed(request, name, &WorldError::Missing);
    };
    let item = match items::resolve(text) {
        Ok(item) => item,
        Err(why) => return unknown(request, text, &why),
    };
    let Some(info) = inventories.item_info(item) else {
        return unknown(request, text, &Unknown::Name { near: Vec::new() });
    };
    let Some(player) = tools.world.player_by_key(*key) else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    // The argument's range and step make it a whole number from 1 up.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number in 1..=MAX_COUNT"
    )]
    let count = *count as u32;
    let stack = u32::from(info.max_stack_size.max(1));
    let shown = count.to_string();
    let values = [
        (texts::value::NAME, name.as_str()),
        (texts::value::COUNT, shown.as_str()),
        (texts::value::ITEM, text.as_str()),
    ];
    if count > stack {
        let stack = stack.to_string();
        return Outcome::refused(request.say(
            texts::REPLY_OVER_STACK,
            &[(texts::value::ITEM, text), (texts::value::COUNT, &stack)],
        ));
    }
    let failed = request.say(texts::REPLY_NOT_GIVEN, &values);
    let caller = request.player.clone();
    let world = tools.world;
    let _ticket = inventories.give(player.entity, item, count, move |outcome| {
        if !outcome.is_committed() {
            enshwick::warn!(
                "give of {item:?} to {} turned down: {:?}",
                player.name,
                outcome.done
            );
            tell(world, &caller, &failed);
        }
    });
    Outcome::done(request.say(texts::REPLY_GIVEN, &values))
}

/// Tells `caller` `text` in chat, when the caller is a player in the game.
fn tell(world: enshwick::world::World, caller: &Player, text: &str) {
    if caller.key.0 != 0 {
        world.tell(caller, text);
    }
}

#[cfg(test)]
mod tests {
    use enshwick::ItemId;
    use enshwick::inventory::{InventoryCategory, SlotRef};

    use super::*;

    fn slot(index: u8, item: u32, count: u32) -> Slot {
        Slot {
            at: SlotRef::new(Entity(9), index),
            category: InventoryCategory::GENERIC,
            item: ItemId::new(item),
            count,
            pide: None,
        }
    }

    #[test]
    fn clearing_destroys_each_stack_and_skips_empty_slots() {
        let slots = [slot(0, 7, 3), slot(1, 0, 0), slot(2, 8, 1)];
        assert_eq!(
            clear_ops(&slots),
            [Op::DestroySlot(slots[0].at), Op::DestroySlot(slots[2].at)]
        );
    }

    #[test]
    fn copying_gives_each_stack_s_item_and_count_to_the_other_player() {
        let slots = [slot(0, 7, 3), slot(1, 7, 0), slot(2, 8, 1)];
        let give = |item, count| Op::Give {
            to: Entity(2),
            item: ItemId::new(item),
            count,
            fill: None,
        };
        assert_eq!(copy_ops(&slots, Entity(2)), [give(7, 3), give(8, 1)]);
    }
}
