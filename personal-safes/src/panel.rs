//! Who may open a Safe, in a panel beside the game's storage chest screen,
//! while the player has a Safe they manage open.
//!
//! The server tells a player's client which Safe they opened when it is one
//! they may manage, and that none is open when they open anything else
//! after it. The client draws a switch per player from the last message;
//! a switch runs the access action, which the server checks again and
//! answers with a notice and a fresh message.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use rustc_hash::FxHashMap;

use enshwick::actions::{Actions, Value};
use enshwick::game_events::AcceptOfferedInteractionEvent;
use enshwick::i18n::{Texts, language};
use enshwick::net::{Channel, Peer, SendError};
use enshwick::ui::{Edge, Frame, HudScreenType, Label, Style, Toggle};
use enshwick::world::World;
use enshwick::{Entity, Load, Off, PlayerKey};

use crate::access::{Cell, Safe, Seen, snapshots};
use crate::actions::ACCESS_ACTION;
use crate::texts;

/// The panel's name as a feature of the mod, on the Mods screen and in the
/// log.
pub const FEATURE: &str = "panel";

/// The channel the server tells each client's panel what to show on.
const CHANNEL: &str = "panel";
/// The channel's version; a client of another version hears nothing.
const VERSION: u16 = 1;

/// The most players one message lists; with names of at most 64 bytes the
/// list stays well inside a message.
pub const MAX_LISTED: usize = 64;

/// How long before the chest screen opens a message still counts for it:
/// on a hosted game the server's message can come in just before the
/// screen draws.
const GRACE: Duration = Duration::from_secs(1);

/// A pause in drawing this long means the chest screen closed and opened
/// again.
const REOPENED: Duration = Duration::from_millis(250);

/// What a player's panel shows, as the server tells their client.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Shown {
    /// No Safe the player may manage is open.
    Nothing,
    /// The Safe the player opened, which they may manage, and the players
    /// they may switch on or off there.
    Safe {
        /// The Safe's cell.
        at: Cell,
        /// The players online and those allowed, by name, at most
        /// [`MAX_LISTED`].
        players: Vec<Listed>,
    },
}

/// A player in a panel's list.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Listed {
    /// Their account key.
    pub key: PlayerKey,
    /// Their name.
    pub name: String,
    /// Whether they may open the Safe.
    pub allowed: bool,
}

/// The panel of `viewer` for `safe` at `at`: the players `online` but the
/// viewer and the owner, and the players allowed who are away, by name.
#[must_use]
pub fn listing(
    at: Cell,
    safe: &Safe,
    viewer: PlayerKey,
    online: impl IntoIterator<Item = (PlayerKey, String)>,
) -> Shown {
    let allowed = |key: PlayerKey| safe.allowed.iter().any(|a| a.key == key);
    let mut players: Vec<Listed> = online
        .into_iter()
        .filter(|(key, _)| *key != viewer && *key != safe.owner)
        .map(|(key, name)| Listed {
            allowed: allowed(key),
            key,
            name,
        })
        .collect();
    for away in &safe.allowed {
        if away.key != viewer && !players.iter().any(|p| p.key == away.key) {
            players.push(Listed {
                key: away.key,
                name: away.name.clone(),
                allowed: true,
            });
        }
    }
    players.sort_by_key(|p| p.name.to_lowercase());
    players.truncate(MAX_LISTED);
    Shown::Safe { at, players }
}

/// Tells `key`'s client what their panel shows: the Safe at `at`, or
/// nothing.
fn tell(channel: Channel<Shown>, world: World, key: PlayerKey, at: Option<Cell>) {
    let owners = snapshots().owners.load();
    let shown = at
        .and_then(|at| {
            let online = world.players().into_iter().map(|p| (p.key, p.name));
            Some(listing(at, owners.get(&at)?, key, online))
        })
        .unwrap_or(Shown::Nothing);
    match channel.send(Peer::Player(key), &shown) {
        // A client without the mod's panel takes no messages.
        Ok(()) | Err(SendError::NotReady) => {}
        Err(error) => enshwick::warn_every!(
            Duration::from_secs(60),
            "a Safe's panel was not sent to {key:016x}: {error}"
        ),
    }
}

/// The player with the character `character` opened `chest`: their client
/// hears of a Safe they may manage, or that none is open once one was.
pub fn accepted(channel: Channel<Shown>, world: World, character: Entity, chest: Entity) {
    let Some(player) = world.player(character) else {
        return;
    };
    let seen = Seen {
        key: player.key,
        admin: player.is_admin(),
    };
    let at = snapshots()
        .owner(chest, world.used_item(chest), world.position(chest))
        .map(|(_, at)| at)
        .filter(|&at| snapshots().may_manage(at, seen, character));
    if snapshots().open(player.key, at) || at.is_some() {
        tell(channel, world, player.key, at);
    }
}

/// Tells `key`'s client what their panel shows after an access run, while
/// they have a Safe open: the fresh list, or nothing once they may no
/// longer manage it.
pub fn refresh(channel: Channel<Shown>, world: World, seen: Seen, character: Entity) {
    let Some(at) = snapshots().opened.load().get(&seen.key).copied() else {
        return;
    };
    let at = Some(at).filter(|&at| snapshots().may_manage(at, seen, character));
    let _changed = snapshots().open(seen.key, at);
    tell(channel, world, seen.key, at);
}

/// The server's last message to this client and when it came.
pub type Last = ArcSwapOption<(Shown, Instant)>;

/// Keeps `shown` from the server as the panel's last message, bounded;
/// anything from a player is dropped.
fn receive(last: &Last, from: Peer, mut shown: Shown) {
    if from != Peer::Server {
        return;
    }
    if let Shown::Safe { players, .. } = &mut shown {
        players.truncate(MAX_LISTED);
    }
    last.store(Some(Arc::new((shown, Instant::now()))));
}

/// Opens the panel's channel, whose messages from the server go to `last`,
/// and, for the server, follows what players open.
///
/// # Errors
///
/// [`Off`] when the loader refuses the channel or the game build lacks the
/// event of an accepted interaction.
pub fn open(load: &mut Load<'_>, world: World, last: Arc<Last>) -> Result<Channel<Shown>, Off> {
    let channel = load.channel::<Shown, _>(CHANNEL, VERSION, move |from, shown| {
        receive(&last, from, shown);
    })?;
    load.on::<AcceptOfferedInteractionEvent>(move |used| {
        accepted(channel, world, used.guest_entity_id, used.host_entity_id);
    })?;
    Ok(channel)
}

/// What the panel draws, kept between frames.
struct Panel {
    last: Arc<Last>,
    actions: Actions,
    texts: Texts,
    /// When the chest screen last opened.
    opened: Instant,
    /// When the panel last drew.
    drawn: Option<Instant>,
    /// When the message drawn came, to tell a new one.
    since: Option<Instant>,
    /// The title and the help line, in the player's language.
    words: (String, String),
    /// Switches the player changed since the message drawn, until the
    /// server's answer.
    pending: FxHashMap<PlayerKey, bool>,
}

impl Panel {
    fn draw(&mut self, frame: &mut Frame<'_>) {
        let now = Instant::now();
        if self
            .drawn
            .is_none_or(|drawn| now.saturating_duration_since(drawn) > REOPENED)
        {
            self.opened = now;
        }
        self.drawn = Some(now);
        let Some(last) = self.last.load_full() else {
            return;
        };
        let (shown, received) = &*last;
        let Shown::Safe { players, .. } = shown else {
            return;
        };
        if *received + GRACE < self.opened {
            return;
        }
        if self.since != Some(*received) {
            self.since = Some(*received);
            self.pending.clear();
            let say = |key| self.texts.fill(key, language(), &[]);
            self.words = (say(texts::ACTION_ACCESS), say(texts::ACTION_ACCESS_HELP));
        }
        frame.label(Label::new(&self.words.0).style(Style::H3));
        frame.label(Label::new(&self.words.1).style(Style::Secondary));
        for player in players {
            let mut on = self
                .pending
                .get(&player.key)
                .copied()
                .unwrap_or(player.allowed);
            if !frame.toggle(Toggle::new(&player.name), &mut on) {
                continue;
            }
            self.pending.insert(player.key, on);
            let args = [
                Value::Player {
                    key: player.key,
                    name: player.name.clone(),
                },
                Value::Toggle(on),
            ];
            if let Err(off) = self.actions.run(ACCESS_ACTION, &args) {
                enshwick::warn!("the switch for {} was not sent: {off}", player.name);
            }
        }
    }
}

/// Adds the panel beside the game's storage chest screen, drawn from
/// `last`; a switch runs the access action of `actions`.
///
/// # Errors
///
/// [`Off`] on a server, or when the game build lacks the screen.
pub fn extend(
    load: &mut Load<'_>,
    last: Arc<Last>,
    actions: Actions,
    texts: Texts,
) -> Result<(), Off> {
    let mut panel = Panel {
        last,
        actions,
        texts,
        opened: Instant::now(),
        drawn: None,
        since: None,
        words: (String::new(), String::new()),
        pending: FxHashMap::default(),
    };
    load.extend_screen(HudScreenType::StorageChest, Edge::Right, move |frame| {
        panel.draw(frame);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::access::Allowed;

    #[test]
    fn the_list_leaves_out_the_viewer_and_owner_and_keeps_allowed_players_away() {
        let mut safe = Safe::new(PlayerKey::new(1), "Ann");
        safe.allowed.push(Allowed {
            key: PlayerKey::new(4),
            name: "dan".into(),
        });
        safe.allowed.push(Allowed {
            key: PlayerKey::new(3),
            name: "Cat".into(),
        });
        let online = [(1, "Ann"), (2, "Bob"), (3, "Cat"), (5, "Root")]
            .map(|(key, name)| (PlayerKey::new(key), name.to_owned()));
        let Shown::Safe { at, players } = listing((2, 0, 0), &safe, PlayerKey::new(5), online)
        else {
            panic!("a Safe")
        };
        assert_eq!(at, (2, 0, 0));
        let named: Vec<(&str, bool)> = players
            .iter()
            .map(|p| (p.name.as_str(), p.allowed))
            .collect();
        assert_eq!(named, [("Bob", false), ("Cat", true), ("dan", true)]);
    }

    #[test]
    fn a_long_list_is_cut_to_fit_one_message() {
        let safe = Safe::new(PlayerKey::new(1), "Ann");
        let online = (2..200).map(|key| (PlayerKey::new(key), "x".repeat(64)));
        let shown = listing((0, 0, 0), &safe, PlayerKey::new(1), online);
        let Shown::Safe { players, .. } = &shown else {
            panic!("a Safe")
        };
        assert_eq!(players.len(), MAX_LISTED);
        let bytes = postcard::to_allocvec(&shown).unwrap();
        assert!(bytes.len() <= enshwick::net::MAX_MESSAGE, "{}", bytes.len());
    }

    #[test]
    fn only_the_server_s_messages_reach_the_panel_and_they_are_bounded() {
        let last = Last::empty();
        let many = Shown::Safe {
            at: (0, 0, 0),
            players: (0..100)
                .map(|key| Listed {
                    key: PlayerKey::new(key),
                    name: String::new(),
                    allowed: false,
                })
                .collect(),
        };
        receive(&last, Peer::Player(PlayerKey::new(7)), many.clone());
        assert!(last.load().is_none(), "a player's message is dropped");
        receive(&last, Peer::Server, many);
        let kept = last.load_full().unwrap();
        assert!(matches!(&kept.0, Shown::Safe { players, .. } if players.len() == MAX_LISTED));
    }
}
