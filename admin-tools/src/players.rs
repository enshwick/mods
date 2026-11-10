//! Actions on players: who is online, kicks, the ban list, the allow-list,
//! chat mutes, healing and teleports; and the check of each player who
//! joins.

use std::sync::Arc;
use std::time::Instant;

use enshwick::actions::{Action, Arg, Outcome, Request, Value, Via};
use enshwick::effects::Teleport;
use enshwick::world::{AccountAction, Player};
use enshwick::{Entity, Off, PlayerKey, Relock as _, WorldError};

use crate::accounts::Ban;
use crate::{Tools, ids, journal, texts};

/// How far a position may lie from the world's origin, in metres, on each
/// axis; well past the edge of any map.
const REACH: f64 = 100_000.0;

/// Between the players of a list read on one line.
const LISTED: &str = ", ";

/// The argument for one axis of a position, in metres.
pub(crate) fn axis(label: &'static str) -> Arg {
    Arg::number(label, -REACH..=REACH, 0.0)
}

/// Declares the actions on players.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    let player = || Arg::player(texts::ARG_PLAYER);
    tools.add(
        Action::new(ids::ONLINE, texts::ACTION_ONLINE).help(texts::ACTION_ONLINE_HELP),
        online,
    )?;
    tools.add(
        Action::new(ids::KICK, texts::ACTION_KICK)
            .help(texts::ACTION_KICK_HELP)
            .arg(player()),
        kick,
    )?;
    let bans = Arc::clone(tools);
    tools.add(
        Action::per_player(ids::BANNED, texts::ACTION_BANNED, move |_, _| {
            bans.accounts
                .with(|accounts| {
                    accounts
                        .bans
                        .iter()
                        .map(|ban| (ban.key, ban.name.clone()))
                        .collect()
                })
                .unwrap_or_default()
        })
        .help(texts::ACTION_BANNED_HELP),
        banned,
    )?;
    let allows = Arc::clone(tools);
    tools.add(
        Action::per_player(ids::ALLOWED, texts::ACTION_ALLOWED, move |_, _| {
            allows
                .accounts
                .with(|accounts| {
                    accounts
                        .allowed
                        .iter()
                        .map(|known| (known.key, known.name.clone()))
                        .collect()
                })
                .unwrap_or_default()
        })
        .help(texts::ACTION_ALLOWED_HELP),
        allowed,
    )?;
    let mutes = Arc::clone(tools);
    tools.add(
        Action::per_player(ids::MUTED, texts::ACTION_MUTED, move |_, _| {
            mutes
                .world
                .players()
                .into_iter()
                .filter(|player| mutes.world.muted(player.key).unwrap_or(false))
                .map(|player| (player.key, player.name))
                .collect()
        })
        .help(texts::ACTION_MUTED_HELP),
        muted,
    )?;
    tools.add(
        Action::new(ids::HEAL, texts::ACTION_HEAL)
            .help(texts::ACTION_HEAL_HELP)
            .arg(player()),
        heal,
    )?;
    tools.add(
        Action::new(ids::GOTO, texts::ACTION_GOTO)
            .help(texts::ACTION_GOTO_HELP)
            .arg(player()),
        goto,
    )?;
    tools.add(
        Action::new(ids::BRING, texts::ACTION_BRING)
            .help(texts::ACTION_BRING_HELP)
            .arg(player()),
        bring,
    )?;
    tools.add(
        Action::new(ids::SEND, texts::ACTION_SEND)
            .help(texts::ACTION_SEND_HELP)
            .arg(player())
            .arg(Arg::player(texts::ARG_TO)),
        send,
    )?;
    tools.add(
        Action::new(ids::PLACE, texts::ACTION_PLACE)
            .help(texts::ACTION_PLACE_HELP)
            .arg(player())
            .arg(axis(texts::ARG_X))
            .arg(axis(texts::ARG_Y))
            .arg(axis(texts::ARG_Z)),
        place,
    )
}

/// The refusal for a run without a player online.
pub(crate) fn no_player(request: &Request<'_>) -> Outcome {
    Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]))
}

/// The players online, each with their account, their time online and
/// whether they are muted.
fn online(tools: &Tools, request: &Request<'_>) -> Outcome {
    let players = tools.world.players();
    if players.is_empty() {
        return Outcome::done(request.say(texts::REPLY_ONLINE_NONE, &[]));
    }
    let joined = tools.joined.relock();
    let rows: Vec<String> = players
        .iter()
        .map(|player| {
            let minutes = joined
                .get(&player.key)
                .map_or(0, |at| at.elapsed().as_secs() / 60)
                .to_string();
            let key = if tools.world.muted(player.key).unwrap_or(false) {
                texts::REPLY_ONLINE_MUTED
            } else {
                texts::REPLY_ONLINE_ROW
            };
            request.say(
                key,
                &[
                    (texts::value::NAME, &player.name),
                    (texts::value::ACCOUNT, &crate::account_id(player.key)),
                    (texts::value::MINUTES, &minutes),
                ],
            )
        })
        .collect();
    let count = players.len().to_string();
    let head = request.say(texts::REPLY_ONLINE, &[(texts::value::COUNT, &count)]);
    // A player a line where the caller reads lines; the interface shows one.
    let between = if matches!(request.via, Via::Interface) {
        LISTED
    } else {
        crate::LINE
    };
    Outcome::done(crate::lines(request, &[head, rows.join(between)]))
}

/// Removes a player from the game; administrators and the host stay.
fn kick(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return no_player(request);
    };
    let values = [(texts::value::NAME, name.as_str())];
    match tools.world.account_action(AccountAction::Kick, *key) {
        Ok(()) => Outcome::done(request.say(texts::REPLY_KICKED, &values)),
        Err(WorldError::Refused) => {
            Outcome::refused(request.say(texts::REPLY_PROTECTED_REMOVE, &values))
        }
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Bans a player online, or lifts a ban, online or offline; the mod keeps
/// the ban so it holds when the account comes back. The mod's list changes
/// first and goes back when the game refuses, so a refusal changed nothing.
fn banned(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return no_player(request);
    };
    let values = [(texts::value::NAME, name.as_str())];
    let older = if *on {
        let ban = Ban {
            key: *key,
            name: name.clone(),
            by: request.player.name.clone(),
            at: journal::now(),
        };
        tools.accounts.update(|accounts| accounts.ban(ban))
    } else {
        tools.accounts.update(|accounts| accounts.unban(*key))
    };
    let Some(older) = older else {
        return store_shut(tools, request);
    };
    // The game takes a ban only of a player online; a lift always.
    let game = if !*on {
        Some(AccountAction::UnBan)
    } else if tools.world.player_by_key(*key).is_some() {
        Some(AccountAction::Ban)
    } else {
        None
    };
    if let Some(Err(why)) = game.map(|action| tools.world.account_action(action, *key)) {
        let _restored = tools
            .accounts
            .update(|accounts| accounts.restore(*key, older));
        return match why {
            WorldError::Refused if *on => {
                Outcome::refused(request.say(texts::REPLY_PROTECTED_REMOVE, &values))
            }
            why => Tools::failed(request, name, &why),
        };
    }
    tools.keep_accounts();
    let reply = if *on {
        texts::REPLY_BANNED
    } else {
        texts::REPLY_UNBANNED
    };
    Outcome::done(request.say(reply, &values))
}

/// Puts an account on the allow-list or takes it off.
fn allowed(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return no_player(request);
    };
    let values = [(texts::value::NAME, name.as_str())];
    if tools
        .accounts
        .update(|accounts| accounts.allow(*key, name, *on))
        .is_none()
    {
        return store_shut(tools, request);
    }
    tools.keep_accounts();
    let reply = if *on {
        texts::REPLY_ALLOWED
    } else {
        texts::REPLY_DISALLOWED
    };
    Outcome::done(request.say(reply, &values))
}

/// The refusal while the world's lists do not read.
fn store_shut(tools: &Tools, request: &Request<'_>) -> Outcome {
    let why = tools.accounts.shut_reason().unwrap_or_default();
    Outcome::refused(request.say(texts::REPLY_STORE_SHUT, &[(texts::value::WHY, &why)]))
}

/// Mutes a player in chat or lifts the mute.
fn muted(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return no_player(request);
    };
    let values = [(texts::value::NAME, name.as_str())];
    match tools.world.set_muted(*key, *on) {
        Ok(()) if *on => Outcome::done(request.say(texts::REPLY_MUTED, &values)),
        Ok(()) => Outcome::done(request.say(texts::REPLY_HEARD, &values)),
        Err(WorldError::Refused) if *on => {
            Outcome::refused(request.say(texts::REPLY_PROTECTED, &values))
        }
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Gives a player their full health; it restores, it never adds.
fn heal(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return no_player(request);
    };
    let Some(player) = tools.world.player_by_key(*key) else {
        return no_player(request);
    };
    let Some(vitals) = tools.world.vitals(player.entity) else {
        return Tools::failed(request, name, &WorldError::Missing);
    };
    match tools.world.set_health(player.entity, vitals.health.max) {
        Ok(_) => Outcome::done(request.say(texts::REPLY_HEALED, &[(texts::value::NAME, name)])),
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// The caller, when they have a character to move or to move others to.
fn character<'a>(request: &'a Request<'_>) -> Result<&'a Player, Outcome> {
    if request.player.entity == Entity(0) {
        return Err(Outcome::refused(
            request.say(texts::REPLY_NO_CHARACTER, &[]),
        ));
    }
    Ok(request.player)
}

/// Moves `who` beside `to`, replying with both names.
fn move_to(
    tools: &Tools,
    request: &Request<'_>,
    who: (PlayerKey, &str),
    to: (PlayerKey, &str),
) -> Outcome {
    match tools
        .effects
        .teleport_player_to(who.0, to.0, Teleport::new())
    {
        Ok(()) => Outcome::done(request.say(
            texts::REPLY_MOVED,
            &[(texts::value::NAME, who.1), (texts::value::TO, to.1)],
        )),
        Err(why) => Tools::failed(request, who.1, &why),
    }
}

/// Takes the caller to a player.
fn goto(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return no_player(request);
    };
    match character(request) {
        Ok(me) => move_to(tools, request, (me.key, &me.name), (*key, name)),
        Err(refused) => refused,
    }
}

/// Brings a player to the caller.
fn bring(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return no_player(request);
    };
    match character(request) {
        Ok(me) => move_to(tools, request, (*key, name), (me.key, &me.name)),
        Err(refused) => refused,
    }
}

/// Sends one player to another.
fn send(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Player {
            key: to,
            name: to_name,
        },
    ] = request.args
    else {
        return no_player(request);
    };
    move_to(tools, request, (*key, name), (*to, to_name))
}

/// Moves a player to a position, on free ground near it.
fn place(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Player { key, name },
        Value::Number(x),
        Value::Number(y),
        Value::Number(z),
    ] = request.args
    else {
        return no_player(request);
    };
    let how = Teleport::new().search_best_spawn_position();
    match tools.effects.teleport_player(*key, [*x, *y, *z], how) {
        Ok(()) => {
            let [x, y, z] = [x, y, z].map(|axis| format!("{axis:.0}"));
            Outcome::done(request.say(
                texts::REPLY_PLACED,
                &[
                    (texts::value::NAME, name),
                    (texts::value::X, &x),
                    (texts::value::Y, &y),
                    (texts::value::Z, &z),
                ],
            ))
        }
        Err(why) => Tools::failed(request, name, &why),
    }
}

impl Tools {
    /// Checks a player who joined, on the game thread after the step: a
    /// banned account is banned again, one the allow-list leaves out is
    /// kicked, and the others read the message of the day.
    pub(crate) fn welcome(&self, key: PlayerKey) {
        let Some(player) = self.world.player_by_key(key) else {
            return;
        };
        self.joined.relock().insert(key, Instant::now());
        let settings = self.settings.load();
        let lists = self
            .accounts
            .with(|accounts| (accounts.banned(key), accounts.allowed(key)));
        // Lists that do not read keep the door shut: a ban in them may hold.
        let unread = lists.is_none() && self.accounts.shut_reason().is_some();
        let refusal = match lists {
            Some((true, _)) => Some(AccountAction::Ban),
            Some((false, false)) | None if (settings.allowlist || unread) && !player.is_admin() => {
                Some(AccountAction::Kick)
            }
            _ => None,
        };
        let Some(refusal) = refusal else {
            if !settings.motd.is_empty() {
                let _shown = self.world.notify(&player, &settings.motd);
            }
            return;
        };
        let what = if refusal == AccountAction::Ban {
            ids::BANNED
        } else {
            ids::ALLOWED
        };
        let who = crate::account(&player.name, key);
        match self.world.account_action(refusal, key) {
            Ok(()) => {
                enshwick::info!("{who} refused at the door: {what}");
                self.log(&["server", "join", what, &who, journal::REFUSED]);
            }
            Err(why) => enshwick::warn!("{who} not refused ({what}): {why}"),
        }
    }
}
