//! Spawns and props: creatures and other templates spawned beside a player
//! or at a position, as the game's admin spawn does, and props placed by a
//! world transaction, the caller's last one taken back on request.

use std::sync::{Arc, Mutex};

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::content::{Guid, vanilla};
use enshwick::effects::Spawn;
use enshwick::world::{TransactionId, WorldTransform};
use enshwick::{Off, PlayerKey, Relock as _, WorldError};
use rustc_hash::FxHashMap;

use crate::items::{self, ResolveKnowledgeError};
use crate::players::{axis, bad_args, no_player};
use crate::{Late, Tools, ids, texts};

/// The most creatures one spawn makes.
const MAX_COUNT: f64 = 20.0;

/// The highest level a spawn takes; the game scales to its own cap.
const MAX_LEVEL: f64 = 100.0;

/// The longest time a spawned creature stays, in minutes; 0 keeps it.
const MAX_MINUTES: f64 = 1_440.0;

/// How far around a player, or around a position for more than one, the
/// creatures of a spawn stand, in metres, so they do not stand in each
/// other.
const RING: f64 = 3.0;

/// How far beside a player a prop stands, in metres along x.
const BESIDE: f64 = 2.0;

/// A prop a caller placed, which they may take back.
#[derive(Debug, Clone)]
pub(crate) struct Placed {
    transaction: TransactionId,
    /// What it is, as the caller read it.
    shown: String,
}

/// Who placed a prop: the account, and the name that tells apart the
/// callers without one (the server's console, each remote console's label).
pub(crate) type Caller = (PlayerKey, String);

/// The caller of `request`.
fn caller(request: &Request<'_>) -> Caller {
    (request.player.key, request.player.name.clone())
}

/// Reads the game's template names while the mod starts, as the API asks:
/// a first read later stalls the game thread for about 139 ms on a
/// dedicated server.
pub(crate) fn name_templates() {
    // Any name reads them all; a miss is the answer expected.
    let _read = vanilla::template("");
}

/// The arguments of a spawn after where it goes: the template, then how
/// many, the level, the minutes it stays and whether the save keeps it.
fn spawn_args(action: Action) -> Action {
    action
        .arg(Arg::text(texts::ARG_TEMPLATE, items::MAX_NAME))
        .arg(Arg::number(texts::ARG_COUNT, 1.0..=MAX_COUNT, 1.0).optional(Value::Number(1.0)))
        .arg(Arg::number(texts::ARG_LEVEL, 0.0..=MAX_LEVEL, 1.0).optional(Value::Number(0.0)))
        .arg(Arg::number(texts::ARG_LIFETIME, 0.0..=MAX_MINUTES, 1.0).optional(Value::Number(0.0)))
        .arg(Arg::toggle(texts::ARG_KEEP).optional(Value::Toggle(false)))
}

/// Declares the spawn and prop actions.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    tools.add(
        spawn_args(
            Action::new(ids::SPAWN, texts::ACTION_SPAWN)
                .help(texts::ACTION_SPAWN_HELP)
                .arg(Arg::player(texts::ARG_PLAYER)),
        ),
        spawn,
    )?;
    tools.add(
        spawn_args(
            Action::new(ids::SPAWN_AT, texts::ACTION_SPAWNAT)
                .help(texts::ACTION_SPAWNAT_HELP)
                .arg(axis(texts::ARG_X))
                .arg(axis(texts::ARG_Y))
                .arg(axis(texts::ARG_Z)),
        ),
        spawn_at,
    )?;
    tools.add(
        Action::new(ids::PROP, texts::ACTION_PROP)
            .help(texts::ACTION_PROP_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(items::item_arg()),
        prop,
    )?;
    tools.add(
        Action::new(ids::PROP_AT, texts::ACTION_PROPAT)
            .help(texts::ACTION_PROPAT_HELP)
            .arg(axis(texts::ARG_X))
            .arg(axis(texts::ARG_Y))
            .arg(axis(texts::ARG_Z))
            .arg(items::item_arg()),
        prop_at,
    )?;
    tools.add(
        Action::new(ids::UNPROP, texts::ACTION_UNPROP).help(texts::ACTION_UNPROP_HELP),
        unprop,
    )
}

/// Where a player stands, by their account.
fn standing(tools: &Tools, key: PlayerKey) -> Option<[f64; 3]> {
    tools
        .world
        .player_by_key(key)
        .and_then(|player| tools.world.position(player.entity))
}

/// Spawns creatures around a player.
fn spawn(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, how @ ..] = request.args else {
        return bad_args(request);
    };
    let Some(at) = standing(tools, *key) else {
        return no_player(request);
    };
    // Even one stands beside the player, not in them.
    spawn_from(tools, request, at, RING, name, how)
}

/// Spawns creatures at a position, around it for more than one.
fn spawn_at(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Number(x),
        Value::Number(y),
        Value::Number(z),
        how @ ..,
    ] = request.args
    else {
        return bad_args(request);
    };
    let whereabouts = format!("{x:.0}, {y:.0}, {z:.0}");
    spawn_from(tools, request, [*x, *y, *z], 0.0, &whereabouts, how)
}

/// What a spawn's values after where it goes ask for: the template's text,
/// how many, and how each starts.
fn asked(request: &Request<'_>, how: &[Value]) -> Result<(String, u32, Spawn), Outcome> {
    let [
        Value::Text(template),
        Value::Number(count),
        Value::Number(level),
        Value::Number(minutes),
        Value::Toggle(keep),
    ] = how
    else {
        return Err(bad_args(request));
    };
    let count = crate::whole(request, *count)?;
    let level = crate::whole(request, *level)?;
    let minutes = crate::whole(request, *minutes)?;
    let mut start = if *keep {
        Spawn::new()
    } else {
        Spawn::new().do_not_save()
    };
    if level > 0 {
        start = start.level(level);
    }
    if minutes > 0 {
        start = start.despawn_after(core::time::Duration::from_secs(u64::from(minutes) * 60));
    }
    Ok((template.clone(), count, start))
}

/// Spawns what `how` asks for on a ring of [`RING`] around `centre`, or
/// `alone` away for a single creature; `whereabouts` names the place in the
/// reply.
fn spawn_from(
    tools: &Tools,
    request: &Request<'_>,
    centre: [f64; 3],
    alone: f64,
    whereabouts: &str,
    how: &[Value],
) -> Outcome {
    let (text, count, start) = match asked(request, how) {
        Ok(asked) => asked,
        Err(refused) => return refused,
    };
    let template = match template(&text) {
        Ok(template) => template,
        // Templates take no number, as the item text's advice offers.
        Err(ResolveKnowledgeError::Unreadable) => {
            return Outcome::refused(request.say(
                texts::REPLY_TEMPLATES_UNREAD,
                &[(texts::value::ITEM, &text)],
            ));
        }
        Err(why) => return items::refused(request, &text, &why),
    };
    let ring = if count > 1 { RING } else { alone };
    let mut made = 0;
    let mut failed = None;
    for at in spots(centre, count, ring) {
        match tools.effects.spawn(template.0, at, start) {
            Ok(_) => made += 1,
            Err(why) => {
                failed = Some(why);
                break;
            }
        }
    }
    let values = [
        (texts::value::NAME, whereabouts.to_owned()),
        (texts::value::ITEM, text),
        (texts::value::COUNT, count.to_string()),
        (texts::value::GIVEN, made.to_string()),
    ];
    match failed {
        None => Outcome::done(request.say(texts::REPLY_SPAWNED, &crate::pairs(&values))),
        Some(why) if made == 0 => Tools::failed(request, whereabouts, &why),
        Some(why) => {
            let why = why.to_string();
            let mut values = values.to_vec();
            values.push((texts::value::WHY, why));
            Outcome::done(request.say(texts::REPLY_SPAWNED_PART, &crate::pairs(&values)))
        }
    }
}

/// The template `text` names: its GUID, or a vanilla template's name in
/// any case, such as `Enemy_Wildbeast_Wolf`.
fn template(text: &str) -> Result<Guid, ResolveKnowledgeError> {
    let text = text.trim();
    if let Some(guid) = Guid::try_parse(text) {
        return Ok(guid);
    }
    items::by_name(text, vanilla::template).map(|(guid, _)| guid)
}

/// Where each of `count` creatures stands: evenly on a circle of `radius`
/// around `centre`, all on it for a radius of 0.
fn spots(centre: [f64; 3], count: u32, radius: f64) -> impl Iterator<Item = [f64; 3]> {
    let [x, y, z] = centre;
    (0..count).map(move |at| {
        let turn = core::f64::consts::TAU * f64::from(at) / f64::from(count);
        [x + radius * turn.cos(), y, z + radius * turn.sin()]
    })
}

/// Places a prop beside a player.
fn prop(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Item(item)] = request.args else {
        return bad_args(request);
    };
    let Some([x, y, z]) = standing(tools, *key) else {
        return no_player(request);
    };
    place(tools, request, ids::PROP, [x + BESIDE, y, z], *item, name)
}

/// Places a prop at a position.
fn prop_at(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Number(x),
        Value::Number(y),
        Value::Number(z),
        Value::Item(item),
    ] = request.args
    else {
        return bad_args(request);
    };
    let whereabouts = format!("{x:.0}, {y:.0}, {z:.0}");
    place(
        tools,
        request,
        ids::PROP_AT,
        [*x, *y, *z],
        *item,
        &whereabouts,
    )
}

/// Places `item` at `at` for the action `id` by a world transaction and,
/// once the game has placed it, remembers it as the caller's last prop; a
/// prop that does not stand is told to the caller. `whereabouts` names the
/// place in the replies.
fn place(
    tools: &Tools,
    request: &Request<'_>,
    id: &'static str,
    at: [f64; 3],
    item: Guid,
    whereabouts: &str,
) -> Outcome {
    let (prop, shown) = tools.picked(item, request.language);
    let Some(late) = Late::new(tools, request, id) else {
        return Tools::failed(request, whereabouts, &WorldError::Missing);
    };
    let placed = tools
        .world
        .transaction()
        .place_prop(prop, WorldTransform::at(at))
        .commit();
    let transaction = match placed {
        Ok(transaction) => transaction,
        Err(why) => return Tools::failed(request, whereabouts, &why),
    };
    let reply = request.say(
        texts::REPLY_PROP,
        &[
            (texts::value::NAME, whereabouts),
            (texts::value::ITEM, &shown),
        ],
    );
    let (by, whereabouts) = (caller(request), whereabouts.to_owned());
    crate::chest::once_placed(tools, transaction, move |stands| {
        if stands.is_some() {
            late.tools
                .props
                .relock()
                .insert(by, Placed { transaction, shown });
        } else {
            late.report(
                false,
                texts::REPLY_PROP_LOST,
                &[
                    (texts::value::NAME, &whereabouts),
                    (texts::value::ITEM, &shown),
                ],
            );
        }
    });
    Outcome::done(reply)
}

/// Takes back the last prop the caller placed, as the game undoes a world
/// transaction.
fn unprop(tools: &Tools, request: &Request<'_>) -> Outcome {
    let undone = take_back(&tools.props, &caller(request), |placed| {
        tools.world.undo(placed.transaction)
    });
    match undone {
        Ok(Some(placed)) => {
            Outcome::done(request.say(texts::REPLY_UNPROP, &[(texts::value::ITEM, &placed.shown)]))
        }
        Ok(None) => Outcome::refused(request.say(texts::REPLY_NO_PROP, &[])),
        Err((placed, why)) => Tools::failed(request, &placed.shown, &why),
    }
}

/// Undoes `caller`'s last prop among `props` by `undo` and forgets it once
/// the undo is under way, so a refused undo may be asked again: the prop,
/// `None` for a caller with none, or the prop kept and why it stays.
fn take_back<P: Clone>(
    props: &Mutex<FxHashMap<Caller, P>>,
    caller: &Caller,
    undo: impl FnOnce(&P) -> Result<(), WorldError>,
) -> Result<Option<P>, (P, WorldError)> {
    let mut props = props.relock();
    let Some(placed) = props.get(caller) else {
        return Ok(None);
    };
    undo(placed).map_err(|why| (placed.clone(), why))?;
    Ok(props.remove(caller))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-9)
    }

    #[test]
    fn creatures_stand_evenly_on_a_ring_around_the_centre() {
        let at: Vec<[f64; 3]> = spots([10.0, 5.0, 0.0], 4, 3.0).collect();
        assert_eq!(at.len(), 4);
        for (spot, want) in at.iter().zip([
            [13.0, 5.0, 0.0],
            [10.0, 5.0, 3.0],
            [7.0, 5.0, 0.0],
            [10.0, 5.0, -3.0],
        ]) {
            assert!(near(*spot, want), "{spot:?} {want:?}");
        }
    }

    #[test]
    fn one_creature_at_a_position_stands_on_it() {
        let at: Vec<[f64; 3]> = spots([1.0, 2.0, 3.0], 1, 0.0).collect();
        assert_eq!(at, [[1.0, 2.0, 3.0]]);
    }

    #[test]
    fn a_template_s_guid_names_it_without_the_game_s_list() {
        let guid = "7ced0410-b6d4-4927-a64f-da77e19e42e4";
        assert_eq!(template(guid), Ok(Guid::parse(guid)));
    }

    #[test]
    fn a_template_name_resolves_in_any_case_or_tells_the_names_near_it_or_the_guids_to_pick() {
        use vanilla::VanillaError;
        const WOLF: Guid = Guid([0x40; 16]);
        let game = |name: &str| match name {
            "Enemy_Wildbeast_Wolf" => Ok(WOLF),
            "Twin" => Err(VanillaError::AmbiguousTemplate {
                name: name.to_owned(),
                templates: vec![WOLF, Guid([0x41; 16])],
            }),
            "Lost" => Err(VanillaError::Unreadable("templates")),
            _ => Err(VanillaError::UnknownTemplate {
                name: name.to_owned(),
                near: vec!["Enemy_Wildbeast_Wolf".to_owned()],
            }),
        };
        let by = |name| items::by_name(name, game).map(|(guid, _)| guid);
        assert_eq!(by("Enemy_Wildbeast_Wolf"), Ok(WOLF));
        assert_eq!(by("enemy_wildbeast_wolf"), Ok(WOLF), "any case");
        assert_eq!(
            by("Enemy_Wolv"),
            Err(ResolveKnowledgeError::Name {
                near: vec!["Enemy_Wildbeast_Wolf".to_owned()]
            })
        );
        assert_eq!(
            by("Twin"),
            Err(ResolveKnowledgeError::Several(vec![WOLF, Guid([0x41; 16])]))
        );
        assert_eq!(by("Lost"), Err(ResolveKnowledgeError::Unreadable));
    }

    #[test]
    fn a_refused_undo_keeps_the_prop_to_ask_again_and_a_taken_one_is_forgotten() {
        let ann: Caller = (PlayerKey(0), "rcon a".to_owned());
        let other: Caller = (PlayerKey(0), "rcon b".to_owned());
        let props = Mutex::new(FxHashMap::default());
        props.relock().insert(ann.clone(), "workbench");
        props.relock().insert(other.clone(), "chest");
        assert_eq!(
            take_back(&props, &ann, |_| Err(WorldError::Refused)),
            Err(("workbench", WorldError::Refused))
        );
        assert_eq!(take_back(&props, &ann, |_| Ok(())), Ok(Some("workbench")));
        assert_eq!(take_back(&props, &ann, |_| Ok(())), Ok(None));
        assert_eq!(
            props.relock().get(&other),
            Some(&"chest"),
            "another caller without an account keeps theirs"
        );
    }
}
