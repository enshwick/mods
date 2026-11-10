//! Actions on what players carry: giving an item, clearing an inventory
//! and copying one into another.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::inventory::{Batch, InventoryCategory, Op, Outcome as Given, Slot};
use enshwick::{Entity, Off, WorldError};

use crate::players::no_player;
use crate::{Late, Tools, ids, items, texts};

/// The slots a copy reads: those whose stacks a give hands over as items.
/// What the look and currency slots hold in game is not known yet, so a
/// copy leaves them out.
const COPIED: [InventoryCategory; 2] = [InventoryCategory::EQUIPMENT, InventoryCategory::GENERIC];

/// Declares the actions on what players carry.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    if tools.inventories.is_none() {
        return Ok(());
    }
    tools.add(
        Action::new(ids::GIVE, texts::ACTION_GIVE)
            .help(texts::ACTION_GIVE_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(items::item_arg())
            .arg(items::count_arg()),
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

/// Destroys every stack in `slots`, whatever its slot holds.
fn clear_ops(slots: &[Slot]) -> Vec<Op> {
    slots
        .iter()
        .filter(|slot| !slot.is_empty())
        .map(|slot| Op::DestroySlot(slot.at))
        .collect()
}

/// Gives `to` new items of each stack in the [`COPIED`] slots of `slots`,
/// as many as it holds.
fn copy_ops(slots: &[Slot], to: Entity) -> Vec<Op> {
    slots
        .iter()
        .filter(|slot| !slot.is_empty() && COPIED.contains(&slot.category))
        .map(|slot| Op::Give {
            to,
            item: slot.item,
            count: slot.count,
            fill: None,
        })
        .collect()
}

/// How many items the gives among `ops` hand over.
fn asked(ops: &[Op]) -> u32 {
    ops.iter().fold(0, |sum, op| match op {
        Op::Give { count, .. } => sum.saturating_add(*count),
        _ => sum,
    })
}

/// How many of the `asked` items a request left out: all of them when the
/// game turned it down, else what its operations could not place.
fn shortfall(outcome: &Given, asked: u32) -> u32 {
    if !outcome.is_committed() {
        return asked;
    }
    let left = |sum: u32, op: &enshwick::inventory::OpResult| sum.saturating_add(op.remaining);
    outcome.ops.iter().fold(0, left).min(asked)
}

/// Destroys everything a player carries, equipment and action bar
/// included; each transaction the stacks need is whole or not at all.
fn clear(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return crate::players::bad_args(request);
    };
    let (Some(inventories), Some(player), Some(late)) = (
        &tools.inventories,
        tools.world.player_by_key(*key),
        Late::new(tools, request, ids::CLEAR),
    ) else {
        return no_player(request);
    };
    let values = [(texts::value::NAME, name.as_str())];
    let ops = clear_ops(&inventories.slots(player.entity));
    if ops.is_empty() {
        return Outcome::refused(request.say(texts::REPLY_NOTHING_CARRIED, &values));
    }
    for part in ops.into_iter().fold(Batch::new(), Batch::with).split() {
        let (late, name) = (Arc::clone(&late), name.clone());
        let _ticket = inventories.submit(&part, move |outcome| {
            if !outcome.is_committed() {
                enshwick::warn!("clear of {name} turned down: {:?}", outcome.done);
                late.report(
                    false,
                    texts::REPLY_NOT_CHANGED,
                    &[(texts::value::NAME, &name)],
                );
            }
        });
    }
    Outcome::done(request.say(texts::REPLY_CLEARED, &values))
}

/// Gives a player new items of every stack another carries; what does not
/// fit stays out and is told to the caller, and the items start new,
/// without their wear or level.
fn copy(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Player {
            key: to,
            name: to_name,
        },
    ] = request.args
    else {
        return crate::players::bad_args(request);
    };
    let (Some(inventories), Some(from), Some(target), Some(late)) = (
        &tools.inventories,
        tools.world.player_by_key(*key),
        tools.world.player_by_key(*to),
        Late::new(tools, request, ids::COPY),
    ) else {
        return no_player(request);
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
    let batch = ops.into_iter().fold(Batch::new(), Batch::with);
    for part in batch.allow_partial().split() {
        let (late, to, asked) = (Arc::clone(&late), to_name.clone(), asked(part.ops()));
        let _ticket = inventories.submit(&part, move |outcome| {
            let short = shortfall(outcome, asked);
            if short == 0 {
                return;
            }
            enshwick::warn!(
                "{short} of {asked} items for {to} not copied: {:?}",
                outcome.done
            );
            late.report(
                short < asked,
                texts::REPLY_COPY_SHORT,
                &[
                    (texts::value::TO, &to),
                    (texts::value::SHORT, &short.to_string()),
                ],
            );
        });
    }
    Outcome::done(request.say(texts::REPLY_COPIED, &values))
}

/// Gives a player `count` new items in as many stacks as they need; what
/// does not fit, or a give the game turns down, is told to the caller.
fn give(tools: &Tools, request: &Request<'_>) -> Outcome {
    try_give(tools, request).unwrap_or_else(|refused| refused)
}

/// [`give`], with a refusal as its error.
fn try_give(tools: &Tools, request: &Request<'_>) -> Result<Outcome, Outcome> {
    let [
        Value::Player { key, name },
        Value::Item(picked),
        Value::Number(count),
    ] = request.args
    else {
        return Err(no_player(request));
    };
    let Some(inventories) = &tools.inventories else {
        return Err(Tools::failed(request, name, &WorldError::Missing));
    };
    let count = crate::whole(request, *count)?;
    let (item, shown) = tools.picked(*picked, request.language);
    let (Some(player), Some(late)) = (
        tools.world.player_by_key(*key),
        Late::new(tools, request, ids::GIVE),
    ) else {
        return Err(no_player(request));
    };
    let batch = Batch::new()
        .with(Op::Give {
            to: player.entity,
            item,
            count,
            fill: None,
        })
        .allow_partial();
    let reply = request.say(
        texts::REPLY_GIVEN,
        &[
            (texts::value::NAME, name),
            (texts::value::COUNT, &count.to_string()),
            (texts::value::ITEM, &shown),
        ],
    );
    let _ticket = inventories.submit(&batch, report(late, shown, count, name.clone()));
    Ok(Outcome::done(reply))
}

/// What runs once a give of `count` items, shown as `item`, to `to` ends:
/// the caller reads, in their language, how many did not arrive.
pub(crate) fn report(
    late: Arc<Late>,
    item: String,
    count: u32,
    to: String,
) -> impl FnOnce(&Given) + Send + 'static {
    move |outcome| {
        let short = shortfall(outcome, count);
        if short == 0 {
            return;
        }
        enshwick::warn!(
            "{short} of {count} {item} for {to} not given: {:?}",
            outcome.done
        );
        late.report(
            short < count,
            texts::REPLY_SHORT,
            &[
                (texts::value::NAME, &to),
                (texts::value::ITEM, &item),
                (texts::value::COUNT, &count.to_string()),
                (texts::value::GIVEN, &(count - short).to_string()),
                (texts::value::SHORT, &short.to_string()),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use enshwick::ItemId;
    use enshwick::inventory::{Done, OpResult, SlotRef};

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

    fn give(item: u32, count: u32) -> Op {
        Op::Give {
            to: Entity(2),
            item: ItemId::new(item),
            count,
            fill: None,
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
        assert_eq!(copy_ops(&slots, Entity(2)), [give(7, 3), give(8, 1)]);
    }

    #[test]
    fn copying_leaves_out_the_look_and_currency_slots_and_takes_what_is_worn() {
        let in_category = |index, category| Slot {
            category,
            ..slot(index, 7, 1)
        };
        let slots = [
            in_category(0, InventoryCategory::CUSTOMIZATION),
            in_category(1, InventoryCategory::CURRENCY),
            in_category(2, InventoryCategory::EQUIPMENT),
        ];
        assert_eq!(copy_ops(&slots, Entity(2)), [give(7, 1)]);
        assert_eq!(clear_ops(&slots).len(), 3, "a clear takes every slot");
    }

    #[test]
    fn what_stayed_out_is_every_operation_s_rest_or_all_of_a_request_turned_down() {
        let ops = [give(7, 30), Op::DestroySlot(SlotRef::new(Entity(9), 0))];
        assert_eq!(asked(&ops), 30);
        let rest = |remaining| OpResult {
            error: None,
            remaining,
        };
        let committed = Given {
            done: Done::Committed,
            ops: vec![rest(3), rest(4)],
        };
        assert_eq!(shortfall(&committed, 30), 7);
        assert_eq!(shortfall(&committed, 5), 5, "never more than asked");
        let turned_down = Given {
            done: Done::RolledBack,
            ops: Vec::new(),
        };
        assert_eq!(shortfall(&turned_down, 30), 30);
    }
}
