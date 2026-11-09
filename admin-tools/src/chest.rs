//! Chests of items: a wooden chest placed beside a player or at a position,
//! then filled with items.

use core::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::content::vanilla;
use enshwick::world::{Player, TransactionId, TransactionStatus, WorldTransform};
use enshwick::{ItemId, Off, WorldError};

use enshwick::inventory::{Batch, Op, Outcome as Given};

use crate::inventory::{MAX_COUNT, report, tell, whole};
use crate::items::{self, Unknown};
use crate::{Tools, ids, texts};

/// The debug name of the vanilla wooden chest, as the item registry of
/// server 399 and client 426 lists it.
const CHEST: &str = "Prop_Storage_32_T2_Chest_Wood";

/// How far beside a player a chest stands, in metres along x, so it does
/// not stand in them.
const BESIDE: f64 = 2.0;

/// How far a position may lie from the world's origin, in metres, on each
/// axis.
const REACH: f64 = 100_000.0;

/// How often a placed chest is looked for while the game places it.
const LOOK: Duration = Duration::from_millis(250);

/// How many looks a chest gets to appear before the fill gives up.
const LOOKS: u32 = 40;

/// Declares the chest actions.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    if tools.inventories.is_none() {
        return Ok(());
    }
    let count = || Arg::number(texts::ARG_COUNT, 1.0..=MAX_COUNT, 1.0);
    let item = || Arg::text(texts::ARG_ITEM, items::MAX_NAME);
    tools.add(
        Action::new(ids::CHEST, texts::ACTION_CHEST)
            .help(texts::ACTION_CHEST_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(count())
            .arg(item()),
        chest,
    )?;
    let axis = |label| Arg::number(label, -REACH..=REACH, 0.0);
    tools.add(
        Action::new(ids::CHEST_AT, texts::ACTION_CHESTAT)
            .help(texts::ACTION_CHESTAT_HELP)
            .arg(axis(texts::ARG_X))
            .arg(axis(texts::ARG_Y))
            .arg(axis(texts::ARG_Z))
            .arg(count())
            .arg(item()),
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
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let Some([x, y, z]) = tools
        .world
        .player_by_key(*key)
        .and_then(|player| tools.world.position(player.entity))
    else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    place(tools, request, [x + BESIDE, y, z], *count, text, name)
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
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let at = format!("{x:.0}, {y:.0}, {z:.0}");
    place(tools, request, [*x, *y, *z], *count, text, &at)
}

/// Places a wooden chest at `at` and, once the game has placed it, fills
/// it with `count` items `text` names; `whereabouts` names the place in the
/// replies.
fn place(
    tools: &Tools,
    request: &Request<'_>,
    at: [f64; 3],
    count: f64,
    text: &str,
    whereabouts: &str,
) -> Outcome {
    let (item, shown) = match items::item(text) {
        Ok(found) => found,
        Err(why) => return items::refused(request, text, &why),
    };
    let known = tools
        .inventories
        .as_ref()
        .is_some_and(|inventories| inventories.item_info(item).is_some());
    if !known {
        return items::refused(request, text, &Unknown::Name { near: Vec::new() });
    }
    let Ok(chest) = vanilla::item(CHEST).map(ItemId::from_guid) else {
        return Outcome::refused(request.say(texts::REPLY_NO_CHEST, &[]));
    };
    let Some(me) = tools.me.upgrade() else {
        return Tools::failed(request, whereabouts, &WorldError::Missing);
    };
    let placed = match tools
        .world
        .transaction()
        .place_prop(chest, WorldTransform::at(at))
        .commit()
    {
        Ok(placed) => placed,
        Err(why) => return Tools::failed(request, whereabouts, &why),
    };
    let count = whole(count);
    let shown_count = count.to_string();
    let values = [
        (texts::value::NAME, whereabouts),
        (texts::value::COUNT, shown_count.as_str()),
        (texts::value::ITEM, shown.as_str()),
    ];
    let not_placed = request.say(texts::REPLY_CHEST_NOT_FILLED, &values);
    let reply = request.say(texts::REPLY_CHEST, &values);
    let done = report(tools, request, shown, count, whereabouts.to_owned());
    fill(
        &me,
        placed,
        (item, count),
        request.player.clone(),
        not_placed,
        done,
    );
    Outcome::done(reply)
}

/// Looks for the chest transaction `placed` puts in the world and gives it
/// `count` items `item` in as many stacks as fit, with `done` on the
/// outcome; a chest that does not appear is told to `caller` as
/// `not_placed`.
fn fill(
    tools: &Arc<Tools>,
    placed: TransactionId,
    (item, count): (ItemId, u32),
    caller: Player,
    not_placed: String,
    done: impl FnOnce(&Given) + Send + 'static,
) {
    let (filling, mut looks, mut done) = (Arc::clone(tools), 0, Some(done));
    tools.scheduler.every(LOOK, move || {
        let world = filling.world;
        let chest = world.transaction_entities(placed).first().copied();
        let status = world.transaction_status(placed);
        looks += 1;
        let gone = matches!(
            status,
            None | Some(TransactionStatus::Failed(_) | TransactionStatus::Undone)
        );
        let Some(chest) = chest else {
            if gone || looks >= LOOKS {
                enshwick::warn!("chest of {item:?} not placed: {status:?}");
                tell(world, &caller, &not_placed);
                return ControlFlow::Break(());
            }
            return ControlFlow::Continue(());
        };
        if let (Some(inventories), Some(done)) = (&filling.inventories, done.take()) {
            let batch = Batch::new()
                .with(Op::Give {
                    to: chest,
                    item,
                    count,
                    fill: None,
                })
                .allow_partial();
            let _ticket = inventories.submit(&batch, done);
        }
        ControlFlow::Break(())
    });
}
