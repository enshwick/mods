//! Chests of items: a wooden chest placed beside a player or at a position,
//! then filled with items.

use std::sync::Arc;
use std::time::Duration;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::content::{Guid, vanilla};
use enshwick::world::{TransactionId, World, WorldTransform};
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

/// Declares the chest actions.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    if tools.inventories.is_none() {
        return Ok(());
    }
    tools.add(
        Action::new(ids::CHEST, texts::ACTION_CHEST)
            .help(texts::ACTION_CHEST_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(items::item_arg())
            .arg(items::count_arg()),
        chest,
    )?;
    tools.add(
        Action::new(ids::CHEST_AT, texts::ACTION_CHESTAT)
            .help(texts::ACTION_CHESTAT_HELP)
            .arg(axis(texts::ARG_X))
            .arg(axis(texts::ARG_Y))
            .arg(axis(texts::ARG_Z))
            .arg(items::item_arg())
            .arg(items::count_arg()),
        chest_at,
    )
}

/// Places a chest beside a player and fills it.
fn chest(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Item(item),
        Value::Number(count),
    ] = request.args
    else {
        return crate::players::bad_args(request);
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
        (*count, *item),
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
        Value::Item(item),
        Value::Number(count),
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
        (*count, *item),
        &at,
    )
    .unwrap_or_else(|refused| refused)
}

/// Places a wooden chest at `at` for the action `id` and, once the game
/// has placed it, fills it with `count` of `picked`; `whereabouts` names
/// the place in the replies. A refusal is the error.
fn place(
    tools: &Tools,
    request: &Request<'_>,
    id: &'static str,
    at: [f64; 3],
    (count, picked): (f64, Guid),
    whereabouts: &str,
) -> Result<Outcome, Outcome> {
    let count = crate::whole(request, count)?;
    let (item, shown) = tools.picked(picked, request.language);
    let Ok(chest) = vanilla::item(CHEST).map(ItemId::from_guid) else {
        return Err(Outcome::refused(request.say(texts::REPLY_NO_CHEST, &[])));
    };
    let Some(late) = Late::new(tools, request, id) else {
        return Err(Tools::failed(
            request,
            whereabouts,
            &WorldError::Unavailable,
        ));
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
    fill(late, placed, (item, count), shown, whereabouts.to_owned())
        .map_err(|why| Tools::failed(request, whereabouts, &why))?;
    Ok(Outcome::done(reply))
}

/// How long a placement gets to stand before the wait gives up on it.
const PLACING: Duration = Duration::from_secs(10);

/// How many times a chest that takes none of its items is given them, a
/// scheduler pass apart, before it is taken back: a chest that just stood
/// may get its inventory a step later.
const GIVE_TRIES: u32 = 2;

/// Waits for the chest the transaction `placed` puts in the world and
/// gives it `count` items `item`, shown as `shown`, in as many stacks as
/// fit. A chest that does not get placed, or takes none of its items, is
/// told to the caller, and its placement undone, so no empty chest follows.
///
/// # Errors
///
/// As [`once_placed`].
fn fill(
    late: Arc<Late>,
    placed: TransactionId,
    (item, count): (ItemId, u32),
    shown: String,
    whereabouts: String,
) -> Result<(), WorldError> {
    let tools = Arc::clone(&late.tools);
    once_placed(&tools, placed, move |chest| {
        let Some(chest) = chest else {
            return late.report(
                false,
                texts::REPLY_CHEST_NOT_FILLED,
                &[
                    (texts::value::NAME, &whereabouts),
                    (texts::value::COUNT, &count.to_string()),
                    (texts::value::ITEM, &shown),
                ],
            );
        };
        Filling {
            late,
            placed,
            chest,
            item,
            count,
            shown,
            whereabouts,
        }
        .give(GIVE_TRIES);
    })
}

/// A chest that stands and the items it is to hold.
struct Filling {
    late: Arc<Late>,
    placed: TransactionId,
    chest: Entity,
    item: ItemId,
    count: u32,
    shown: String,
    whereabouts: String,
}

impl Filling {
    /// Gives the chest its items, in as many stacks as fit. When it takes
    /// none, gives them again a scheduler pass later while `tries` last,
    /// then takes the chest back and tells the caller no chest went up, or
    /// that it stands empty when it cannot be taken back.
    fn give(self, tries: u32) {
        let Some(inventories) = self.late.tools.inventories else {
            return;
        };
        let (chest, item, count) = (self.chest, self.item, self.count);
        let _ticket = inventories.give_what_fits(chest, item, count, move |left, given| {
            if left < self.count {
                let late = Arc::clone(&self.late);
                return report(late, self.shown, self.count, self.whereabouts)(given);
            }
            if tries > 1 {
                let scheduler = self.late.tools.scheduler;
                scheduler.after(Duration::ZERO, move || self.give(tries - 1));
                return;
            }
            enshwick::warn!(
                "chest at {} took none of {} {}: {:?}",
                self.whereabouts,
                self.count,
                self.shown,
                given.done
            );
            let text = if undo(self.late.tools.world, self.placed) {
                texts::REPLY_CHEST_NOT_FILLED
            } else {
                texts::REPLY_CHEST_EMPTY
            };
            self.late.report(
                false,
                text,
                &[
                    (texts::value::NAME, &self.whereabouts),
                    (texts::value::COUNT, &self.count.to_string()),
                    (texts::value::ITEM, &self.shown),
                ],
            );
        });
    }
}

/// Waits for the prop the transaction `placed` puts in the world and hands
/// it to `then` once it stands, or `None` once it will not, after undoing
/// the placement so no stray prop follows.
///
/// # Errors
///
/// When the world refuses the wait: the placement is undone and `then`
/// never runs, so the action can refuse instead of replying done.
pub(crate) fn once_placed(
    tools: &Tools,
    placed: TransactionId,
    then: impl FnOnce(Option<Entity>) + Send + 'static,
) -> Result<(), WorldError> {
    let world = tools.world;
    world
        .when_settled(placed, PLACING, move |settled| {
            let stands = settled.standing();
            if stands.is_none() {
                lost(world, placed, &settled.status);
            }
            then(stands);
        })
        .inspect_err(|why| lost(world, placed, why))
}

/// Undoes the placement `placed` of a prop that will not stand.
fn lost(world: World, placed: TransactionId, why: &dyn core::fmt::Debug) {
    enshwick::warn!("prop not placed: {why:?}");
    undo(world, placed);
}

/// Undoes the placement `placed`; whether the world took it back.
fn undo(world: World, placed: TransactionId) -> bool {
    world
        .undo(placed)
        .inspect_err(|why| enshwick::warn!("prop placement not undone: {why}"))
        .is_ok()
}
