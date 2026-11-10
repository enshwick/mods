//! Actions on a player's character: health, stamina and mana set to a
//! value, recipes and other knowledge learnt, and god mode, in which
//! nothing damages the character.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::world::{Meter, World};
use enshwick::{Entity, Load, Off, PlayerKey, WorldError};

use enshwick::knowledge::{Grant, KnowledgeType, Scope};

use crate::players::no_player;
use crate::{Tools, ids, items, texts};

/// The highest value the stats action takes; the game keeps each pool at
/// its maximum anyway.
const MAX_STAT: f64 = 100_000.0;

/// How the world sets one pool of a character to a value: the pool as it
/// now is, or `None` for one the game sets in its next step.
type SetPool = fn(World, Entity, i32) -> Result<Option<Meter>, WorldError>;

/// The pools the stats action sets: each one's word, its label and how the
/// world sets it.
const POOLS: [(&str, &str, SetPool); 4] = [
    ("health", texts::POOL_HEALTH, |w, e, v| {
        w.set_health(e, v).map(Some)
    }),
    ("stamina", texts::POOL_STAMINA, |w, e, v| {
        w.set_stamina(e, v).map(Some)
    }),
    ("mana", texts::POOL_MANA, |w, e, v| {
        w.set_mana(e, v).map(Some)
    }),
    ("level", texts::POOL_LEVEL, |w, e, v| {
        w.set_level(e, v.unsigned_abs()).map(|()| None)
    }),
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
    tools.add(
        Action::new(ids::REVIVE, texts::ACTION_REVIVE)
            .help(texts::ACTION_REVIVE_HELP)
            .arg(Arg::player(texts::ARG_PLAYER)),
        revive,
    )?;
    tools.add(
        Action::new(ids::LEARN, texts::ACTION_LEARN)
            .help(texts::ACTION_LEARN_HELP)
            .arg(Arg::player(texts::ARG_PLAYER))
            .arg(Arg::text(texts::ARG_KNOWLEDGE, items::MAX_NAME)),
        learn,
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
        return crate::players::bad_args(request);
    };
    let (Some(player), Some((_, label, set))) = (tools.world.player_by_key(*key), POOLS.get(*pool))
    else {
        return no_player(request);
    };
    let value = match crate::whole(request, *value) {
        Ok(value) => i32::try_from(value).unwrap_or(i32::MAX),
        Err(refused) => return refused,
    };
    match set(tools.world, player.entity, value) {
        Ok(None) => Outcome::done(request.say(
            texts::REPLY_LEVEL_SET,
            &[
                (texts::value::NAME, name),
                (texts::value::CURRENT, &value.to_string()),
            ],
        )),
        Ok(Some(now)) => {
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

/// Revives a dead player where they lie, with their full health.
fn revive(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return crate::players::bad_args(request);
    };
    let Some(player) = tools.world.player_by_key(*key) else {
        return no_player(request);
    };
    let full = tools
        .world
        .vitals(player.entity)
        .map_or(1, |v| v.health.max);
    match tools.world.revive(player.entity, full) {
        Ok(now) => Outcome::done(request.say(
            texts::REPLY_REVIVED,
            &[
                (texts::value::NAME, name),
                (texts::value::CURRENT, &now.current.to_string()),
                (texts::value::MAX, &now.max.to_string()),
            ],
        )),
        Err(WorldError::Refused) => {
            Outcome::refused(request.say(texts::REPLY_NOT_DEAD, &[(texts::value::NAME, name)]))
        }
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Grants a player a recipe or another piece of knowledge as the game
/// grants it: a recipe or flags their first bit, a bool set; a counter is
/// refused, since unlocking does not count.
fn learn(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Text(text)] = request.args else {
        return crate::players::bad_args(request);
    };
    let (id, shown) = match items::knowledge(text) {
        Ok(found) => found,
        Err(why) => return items::refused(request, text, &why),
    };
    let Some(player) = tools.world.player_by_key(*key) else {
        return no_player(request);
    };
    let values = [
        (texts::value::NAME, name.as_str()),
        (texts::value::ITEM, shown.as_str()),
    ];
    let Some(known) = tools.knowledge.get(player.entity, id) else {
        return items::refused(
            request,
            text,
            &items::ResolveItemError::Name { near: Vec::new() },
        );
    };
    // A grant of the world's knowledge would unlock it for every player.
    if known.scope != Scope::Player {
        return Outcome::refused(request.say(texts::REPLY_WORLD_KNOWLEDGE, &values));
    }
    let grant = match known.r#type {
        KnowledgeType::Bool if known.is_set() => None,
        KnowledgeType::Bool => Some(Grant::Set),
        KnowledgeType::Flags if known.has_flag(0) => None,
        KnowledgeType::Flags => Some(Grant::Flag(0)),
        _ => return Outcome::refused(request.say(texts::REPLY_NOT_UNLOCKED, &values)),
    };
    let Some(grant) = grant else {
        return Outcome::refused(request.say(texts::REPLY_KNOWN, &values));
    };
    match tools.knowledge.grant(player.entity, id, grant) {
        Ok(()) => Outcome::done(request.say(texts::REPLY_LEARNED, &values)),
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Turns god mode on or off for a player online: while on, the damage
/// filter cancels every damage to their character.
fn god_mode(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return crate::players::bad_args(request);
    };
    let Some(player) = tools.world.player_by_key(*key) else {
        return no_player(request);
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

    /// Ends god mode for each player whose character is no longer the one
    /// it was given to, so the filter never guards an entity the game gave
    /// to something else.
    pub(crate) fn drop_stale_gods(&self) {
        let gods = self.gods.load();
        if gods.is_empty() {
            return;
        }
        // shortcut: god mode ends where it could follow the player to a new
        // character; follow it once it is known that a respawn makes one.
        let kept = current(&gods, |key| {
            self.world.player_by_key(key).map(|player| player.entity)
        });
        if kept.len() != gods.len() {
            self.gods.store(Arc::new(kept));
        }
    }
}

/// The pairs of `gods` whose player still has the character god mode was
/// given to, as `character` finds it now.
fn current(
    gods: &[(PlayerKey, Entity)],
    character: impl Fn(PlayerKey) -> Option<Entity>,
) -> Vec<(PlayerKey, Entity)> {
    gods.iter()
        .copied()
        .filter(|(key, entity)| character(*key) == Some(*entity))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn god_mode_ends_for_a_player_whose_character_changed_or_who_is_gone() {
        let (ann, bob, cid) = (PlayerKey::new(1), PlayerKey::new(2), PlayerKey::new(3));
        let gods = [(ann, Entity(10)), (bob, Entity(11)), (cid, Entity(12))];
        let now = |key| match key {
            key if key == ann => Some(Entity(10)),
            key if key == bob => Some(Entity(40)),
            _ => None,
        };
        assert_eq!(current(&gods, now), [(ann, Entity(10))]);
    }
}
