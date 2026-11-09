//! Actions on what players carry: giving an item.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::world::Player;
use enshwick::{Off, WorldError};

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
    )
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
