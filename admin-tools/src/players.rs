//! Actions on players: who is online, kicks, the ban list, the allow-list,
//! chat mutes, healing and teleports; and the check of each player who
//! joins.

use std::sync::Arc;
use std::time::Instant;

use enshwick::actions::{Action, Arg, Ask, Outcome, Request, Value, Via};
use enshwick::effects::Teleport;
use enshwick::world::{AccountAction, Player};
use enshwick::{Entity, Off, PlayerKey, Relock as _, WorldError};

use crate::accounts::{Accounts, Ban, MAX_REASON};
use crate::{Tools, ids, journal, texts};

/// How far a position may lie from the world's origin, in metres, on each
/// axis; well past the edge of any map.
pub(crate) const REACH: f64 = 100_000.0;

/// Between the players of a list read on one line.
const LISTED: &str = ", ";

/// What the ban list reads where a ban has no reason.
const NO_REASON: &str = "-";

/// The most bans the ban list reads out, as many as the audit log's lines.
const MAX_BANS_READ: usize = 50;

/// The most hex digits an account id has.
const ACCOUNT_DIGITS: usize = 16;

/// The argument for one axis of a position, in metres.
pub(crate) fn axis(label: &'static str) -> Arg {
    Arg::number(label, -REACH..=REACH, 0.0)
}

/// What the account action does to the account it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Ban,
    Unban,
    Allow,
    Disallow,
}

/// The account action's changes: each a word for console lines, its
/// label's key and the change.
const CHANGES: [(&str, &str, Change); 4] = [
    ("ban", texts::CHANGE_BAN, Change::Ban),
    ("unban", texts::CHANGE_UNBAN, Change::Unban),
    ("allow", texts::CHANGE_ALLOW, Change::Allow),
    ("disallow", texts::CHANGE_DISALLOW, Change::Disallow),
];

/// The accounts the world's lists know beyond the players online, for an
/// action's player arguments.
fn known(tools: &Arc<Tools>) -> impl Fn(&Ask<'_>) -> Vec<(PlayerKey, String)> + use<> {
    let tools = Arc::clone(tools);
    move |_| tools.accounts.with(Accounts::known).unwrap_or_default()
}

/// Declares the ban list, the allow-list and the changes of either by
/// account id.
fn add_accounts(tools: &Arc<Tools>) -> Result<(), Off> {
    let player = || Arg::player(texts::ARG_PLAYER);
    tools.add(
        Action::new(ids::BAN, texts::ACTION_BAN)
            .help(texts::ACTION_BAN_HELP)
            .arg(player())
            .arg(Arg::text(texts::ARG_REASON, MAX_REASON))
            .known(known(tools)),
        ban,
    )?;
    tools.add(
        Action::new(ids::BANS, texts::ACTION_BANS).help(texts::ACTION_BANS_HELP),
        bans,
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
        .help(texts::ACTION_BANNED_HELP)
        .known(known(tools)),
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
        .help(texts::ACTION_ALLOWED_HELP)
        .known(known(tools)),
        allowed,
    )?;
    tools.add(
        Action::new(ids::ACCOUNT, texts::ACTION_ACCOUNT)
            .help(texts::ACTION_ACCOUNT_HELP)
            .arg(Arg::text(texts::ARG_ACCOUNT, ACCOUNT_DIGITS))
            .arg(Arg::choice(
                texts::ARG_CHANGE,
                &CHANGES.map(|(word, label, _)| (word, label)),
            )),
        account,
    )?;
    Ok(())
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
    add_accounts(tools)?;
    tools.add(
        Action::new(ids::MARK, texts::ACTION_MARK)
            .help(texts::ACTION_MARK_HELP)
            .arg(player()),
        mark,
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

/// The refusal of values other than those the action declares. The
/// platform checks them before a handler runs, so no run reaches it; a
/// handler's pattern still needs an answer for every other shape.
pub(crate) fn bad_args(request: &Request<'_>) -> Outcome {
    Outcome::refused(request.say(texts::REPLY_BAD_ARGS, &[]))
}

/// The refusal of a kick, ban or allow-list removal of the caller, which
/// would shut out a moderator the game does not shield as it shields
/// administrators.
fn not_yourself(request: &Request<'_>) -> Outcome {
    Outcome::refused(request.say(texts::REPLY_NOT_YOURSELF, &[]))
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
        return bad_args(request);
    };
    if *key == request.player.key {
        return not_yourself(request);
    }
    let values = [(texts::value::NAME, name.as_str())];
    match tools.world.account_action(AccountAction::Kick, *key) {
        Ok(()) => Outcome::done(request.say(texts::REPLY_KICKED, &values)),
        Err(WorldError::Refused) => {
            Outcome::refused(request.say(texts::REPLY_PROTECTED_REMOVE, &values))
        }
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Bans a player, online or away, or lifts a ban, from the list's switch.
fn banned(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return bad_args(request);
    };
    set_ban(tools, request, *key, name, on.then(String::new))
}

/// Bans a player, online or away, with a reason.
fn ban(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Text(reason)] = request.args else {
        return bad_args(request);
    };
    set_ban(tools, request, *key, name, Some(reason.clone()))
}

/// Bans the account `key` for `reason`, or lifts its ban for `None`; the
/// mod keeps the ban so it holds when the account comes back. The mod's
/// list changes first and goes back when the game refuses, so a refusal
/// changed nothing.
fn set_ban(
    tools: &Tools,
    request: &Request<'_>,
    key: PlayerKey,
    name: &str,
    reason: Option<String>,
) -> Outcome {
    let values = [(texts::value::NAME, name)];
    let on = reason.is_some();
    // The game shields only administrators and the host from a ban.
    if on && key == request.player.key {
        return not_yourself(request);
    }
    let older = match reason {
        Some(reason) => {
            let ban = Ban {
                key,
                name: name.to_owned(),
                by: request.player.name.clone(),
                at: journal::now(),
                reason,
            };
            tools.accounts.update(|accounts| accounts.ban(ban))
        }
        None => tools.accounts.update(|accounts| accounts.unban(key)),
    };
    let Some(older) = older else {
        return store_shut(tools, request);
    };
    // The game takes a ban only of a player online; a lift always.
    let game = if !on {
        Some(AccountAction::UnBan)
    } else if tools.world.player_by_key(key).is_some() {
        Some(AccountAction::Ban)
    } else {
        None
    };
    if let Some(Err(why)) = game.map(|action| tools.world.account_action(action, key)) {
        let _restored = tools
            .accounts
            .update(|accounts| accounts.restore(key, older));
        return match why {
            WorldError::Refused if on => {
                Outcome::refused(request.say(texts::REPLY_PROTECTED_REMOVE, &values))
            }
            why => Tools::failed(request, name, &why),
        };
    }
    tools.keep_accounts();
    let reply = if on {
        texts::REPLY_BANNED
    } else {
        texts::REPLY_UNBANNED
    };
    Outcome::done(request.say(reply, &values))
}

/// A ban as a line of the ban list reads it: the values of
/// [`texts::REPLY_BANS_ROW`].
pub(crate) fn ban_values(ban: &Ban) -> [(&'static str, String); 5] {
    let reason = if ban.reason.is_empty() {
        NO_REASON
    } else {
        &ban.reason
    };
    [
        (texts::value::NAME, ban.name.clone()),
        (texts::value::ACCOUNT, crate::account_id(ban.key)),
        (texts::value::WHY, reason.to_owned()),
        (texts::value::BY, ban.by.clone()),
        (texts::value::TIME, journal::utc(ban.at)),
    ]
}

/// `values` as a text's values borrow them.
pub(crate) fn borrowed<'a, const N: usize>(
    values: &'a [(&'static str, String); N],
) -> [(&'a str, &'a str); N] {
    values
        .each_ref()
        .map(|(name, value)| (*name, value.as_str()))
}

/// The ban list, newest first: who, why, by whom and when, at most
/// [`MAX_BANS_READ`] under the count of all.
fn bans(tools: &Tools, request: &Request<'_>) -> Outcome {
    let Some((count, rows)) = tools.accounts.with(|accounts| {
        let rows = accounts
            .bans
            .iter()
            .rev()
            .take(MAX_BANS_READ)
            .map(|ban| request.say(texts::REPLY_BANS_ROW, &borrowed(&ban_values(ban))))
            .collect::<Vec<_>>();
        (accounts.bans.len(), rows)
    }) else {
        return store_shut(tools, request);
    };
    if rows.is_empty() {
        return Outcome::done(request.say(texts::REPLY_BANS_NONE, &[]));
    }
    let count = count.to_string();
    let head = request.say(texts::REPLY_BANS, &[(texts::value::COUNT, &count)]);
    Outcome::done(crate::lines(request, &[head, rows.join(crate::LINE)]))
}

/// The account a text of 1 to 16 hex digits names; `None` for any other
/// text and for 0, which is no account.
pub(crate) fn account_key(text: &str) -> Option<PlayerKey> {
    if text.len() > ACCOUNT_DIGITS || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let key = u64::from_str_radix(text, 16).ok()?;
    (key != 0).then(|| PlayerKey::new(key))
}

/// Bans, unbans, allows or disallows an account by its id, so a change
/// reaches an account that never came to this world.
fn account(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Text(id), Value::Choice(change)] = request.args else {
        return bad_args(request);
    };
    let Some(key) = account_key(id) else {
        return Outcome::refused(
            request.say(texts::REPLY_BAD_ACCOUNT, &[(texts::value::ACCOUNT, id)]),
        );
    };
    let name = tools
        .world
        .player_by_key(key)
        .map(|player| player.name)
        .or_else(|| tools.accounts.with(|accounts| accounts.name_of(key))?)
        .unwrap_or_else(|| crate::account_id(key));
    match CHANGES.get(*change).map(|(.., change)| *change) {
        Some(Change::Ban) => set_ban(tools, request, key, &name, Some(String::new())),
        Some(Change::Unban) => set_ban(tools, request, key, &name, None),
        Some(Change::Allow) => set_allowed(tools, request, key, &name, true),
        Some(Change::Disallow) => set_allowed(tools, request, key, &name, false),
        None => bad_args(request),
    }
}

/// Puts an account on the allow-list or takes it off, from the list's
/// switch.
fn allowed(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return bad_args(request);
    };
    set_allowed(tools, request, *key, name, *on)
}

/// Puts the account `key` on the allow-list or takes it off.
fn set_allowed(
    tools: &Tools,
    request: &Request<'_>,
    key: PlayerKey,
    name: &str,
    on: bool,
) -> Outcome {
    if !on && key == request.player.key {
        return not_yourself(request);
    }
    let values = [(texts::value::NAME, name)];
    if tools
        .accounts
        .update(|accounts| accounts.allow(key, name, on))
        .is_none()
    {
        return store_shut(tools, request);
    }
    tools.keep_accounts();
    let reply = if on {
        texts::REPLY_ALLOWED
    } else {
        texts::REPLY_DISALLOWED
    };
    Outcome::done(request.say(reply, &values))
}

/// The refusal while the world's lists do not read.
pub(crate) fn store_shut(tools: &Tools, request: &Request<'_>) -> Outcome {
    let why = tools.accounts.shut_reason().unwrap_or_default();
    Outcome::refused(request.say(texts::REPLY_STORE_SHUT, &[(texts::value::WHY, &why)]))
}

/// Mutes a player in chat or lifts the mute.
fn muted(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return bad_args(request);
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
        return bad_args(request);
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
        return bad_args(request);
    };
    match character(request) {
        Ok(me) => move_to(tools, request, (me.key, &me.name), (*key, name)),
        Err(refused) => refused,
    }
}

/// Brings a player to the caller.
fn bring(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return bad_args(request);
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
        return bad_args(request);
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
        return bad_args(request);
    };
    place_at(tools, request, *key, name, [*x, *y, *z])
}

/// Moves a player to the map waypoint the caller set.
fn mark(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }] = request.args else {
        return bad_args(request);
    };
    let me = match character(request) {
        Ok(me) => me,
        Err(refused) => return refused,
    };
    let waypoint = tools
        .map
        .ok_or(WorldError::Missing)
        .and_then(|map| map.waypoint(me.entity));
    match waypoint {
        Ok(Some(at)) => place_at(tools, request, *key, name, at),
        Ok(None) => Outcome::refused(request.say(texts::REPLY_NO_WAYPOINT, &[])),
        Err(why) => Tools::failed(request, name, &why),
    }
}

/// Moves the player `key` to the position `at`, on free ground near it.
fn place_at(
    tools: &Tools,
    request: &Request<'_>,
    key: PlayerKey,
    name: &str,
    at: [f64; 3],
) -> Outcome {
    let how = Teleport::new().search_best_spawn_position();
    match tools.effects.teleport_player(key, at, how) {
        Ok(()) => {
            let [x, y, z] = at.map(|axis| format!("{axis:.0}"));
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
        let lists = self.accounts.with(|accounts| {
            let new = !accounts.newest_at_door(key, &player.name);
            (accounts.banned(key), accounts.allowed(key), new)
        });
        // Only a change marks the lists for writing, not every join.
        if lists.is_some_and(|(.., new)| new)
            && self
                .accounts
                .update(|accounts| accounts.see(key, &player.name))
                .is_some()
        {
            // Written now, so an account refused here can be named after a crash too.
            self.keep_accounts();
        }
        let lists = lists.map(|(banned, allowed, _)| (banned, allowed));
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
            let motd = self.motd();
            if !motd.is_empty() {
                let _shown = self.world.notify(&player, &motd);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_id_takes_one_to_sixteen_hex_digits_and_never_zero() {
        assert_eq!(account_key("ab"), Some(PlayerKey::new(0xab)));
        assert_eq!(
            account_key("ffffffffffffffff"),
            Some(PlayerKey::new(u64::MAX))
        );
        assert_eq!(account_key("00000000000000001"), None, "17 digits");
        for refused in ["", "0", "+7", "xyz", "1 2"] {
            assert_eq!(account_key(refused), None, "{refused:?}");
        }
    }
}
