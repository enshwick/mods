//! Actions on a player's character: health, stamina and mana set to a
//! value, and god mode, in which nothing damages the character.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::world::{Meter, World};
use enshwick::{Entity, Load, Off, PlayerKey, WorldError};

use crate::{Tools, ids, texts};

/// The highest value the stats action takes; the game keeps each pool at
/// its maximum anyway.
const MAX_STAT: f64 = 100_000.0;

/// How the world sets one pool of a character to a value.
type SetPool = fn(World, Entity, i32) -> Result<Meter, WorldError>;

/// The pools the stats action sets: each one's word, its label and how the
/// world sets it.
const POOLS: [(&str, &str, SetPool); 3] = [
    ("health", texts::POOL_HEALTH, World::set_health),
    ("stamina", texts::POOL_STAMINA, World::set_stamina),
    ("mana", texts::POOL_MANA, World::set_mana),
];

/// Declares the actions on characters and the filter that keeps god mode.
pub(crate) fn add(tools: &Arc<Tools>, load: &mut Load<'_>) -> Result<(), Off> {
    let pools = POOLS.map(|(word, label, _)| (word, label));
    tools.add(
        Action::new(ids::STATS, texts::ACTION_STATS)
            .help(texts::ACTION_STATS_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(Arg::choice(texts::ARG_POOL, &pools))
            .arg(Arg::number(texts::ARG_VALUE, 0.0..=MAX_STAT, 1.0)),
        stats,
    )?;
    let filtered = Arc::clone(tools);
    let god = load.on_damage(move |damage| {
        let target = damage.target();
        if filtered
            .gods
            .load()
            .iter()
            .any(|(_, entity)| *entity == target)
        {
            damage.cancel();
        }
    });
    if let Err(why) = god {
        enshwick::warn!("god mode is off: {why}");
        return Ok(());
    }
    let listed = Arc::clone(tools);
    tools.add(
        Action::per_player(ids::GOD, texts::ACTION_GOD, move |_, _| {
            listed
                .gods
                .load()
                .iter()
                .filter_map(|(key, _)| listed.world.player_by_key(*key))
                .map(|player| (player.key, player.name))
                .collect()
        })
        .help(texts::ACTION_GOD_HELP),
        god_mode,
    )
}

/// Sets one of a player's pools to a value, which the game keeps within
/// the pool's maximum.
fn stats(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Choice(pool),
        Value::Number(value),
    ] = request.args
    else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let (Some(player), Some((_, label, set))) = (tools.world.player_by_key(*key), POOLS.get(*pool))
    else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    // The argument's range and step make it a whole number in 0..=MAX_STAT.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a whole number in 0..=MAX_STAT"
    )]
    let value = *value as i32;
    match set(tools.world, player.entity, value) {
        Ok(now) => {
            let (current, max) = (now.current.to_string(), now.max.to_string());
            let pool = request.say(label, &[]);
            Outcome::done(request.say(
                texts::REPLY_STAT_SET,
                &[
                    (texts::value::NAME, name),
                    (texts::value::POOL, &pool),
                    (texts::value::CURRENT, &current),
                    (texts::value::MAX, &max),
                ],
            ))
        }
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Turns god mode on or off for a player online: while on, the damage
/// filter cancels every damage to their character.
fn god_mode(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let Some(player) = tools.world.player_by_key(*key) else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    tools.set_god(*key, on.then_some(player.entity));
    let reply = if *on {
        texts::REPLY_GOD_ON
    } else {
        texts::REPLY_GOD_OFF
    };
    Outcome::done(request.say(reply, &[(texts::value::NAME, name)]))
}

impl Tools {
    /// Puts the player `key` in god mode with their character `entity`, or
    /// takes them out with `None`.
    pub(crate) fn set_god(&self, key: PlayerKey, entity: Option<Entity>) {
        self.gods.rcu(|gods| {
            let mut next: Vec<(PlayerKey, Entity)> = gods
                .iter()
                .copied()
                .filter(|(god, _)| *god != key)
                .collect();
            next.extend(entity.map(|entity| (key, entity)));
            next
        });
    }
}
