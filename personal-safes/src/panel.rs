//! Who may open a Safe, in a panel beside the game's storage chest screen,
//! while the player has a Safe they manage open.
//!
//! The server tells a player's client which Safe they opened when it is one
//! they may manage, and that none is open when they open anything else
//! after it. The client draws a switch per player from the last message;
//! a switch runs the access action on that Safe, which the server checks
//! again and answers with a notice and a fresh message.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use rustc_hash::FxHashMap;

use enshwick::abi::Side;
use enshwick::actions::{ActionTarget, Actions, Value};
use enshwick::game_events::AcceptOfferedInteractionEvent;
use enshwick::i18n::{Texts, language};
use enshwick::net::{Channel, Peer, SendError};
use enshwick::ui::{Edge, Frame, HudScreenType, Label, Style, Toggle};
use enshwick::world::World;
use enshwick::{Entity, Load, Off, PlayerKey};

use crate::access::{Cell, Safe, Seen, centre, snapshots};
use crate::actions::ACCESS_ACTION;
use crate::texts;

/// The panel's name as a feature of the mod, on the Mods screen and in the
/// log.
pub const FEATURE: &str = "panel";

/// The channel the server tells each client's panel what to show on.
const CHANNEL: &str = "panel";
/// The channel's version; a client of another version hears nothing.
const VERSION: u16 = 1;

/// The most players one message lists; with names cut to [`MAX_NAME`] bytes
/// the list stays well inside a message.
pub const MAX_LISTED: usize = 64;

/// The longest name a list carries, in bytes, as the game keeps a player's
/// name; a longer one from the owners file is cut.
pub const MAX_NAME: usize = 64;

/// `name` cut to at most [`MAX_NAME`] bytes, on a character boundary.
fn bounded(mut name: String) -> String {
    name.truncate(name.floor_char_boundary(MAX_NAME));
    name
}

/// How long before the chest screen opens a message still counts for it:
/// on a hosted game the server's message can come in just before the
/// screen draws.
const GRACE: Duration = Duration::from_secs(1);

/// How long a switch shows as changed with no answer from the server, which
/// drops a run too soon after the last and refuses some before the action.
const UNANSWERED: Duration = Duration::from_secs(3);

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
    // The allowed are kept over the cut, so each can still be switched off.
    players.sort_by_key(|p| (!p.allowed, p.name.to_lowercase()));
    players.truncate(MAX_LISTED);
    players.sort_by_key(|p| p.name.to_lowercase());
    for player in &mut players {
        player.name = bounded(core::mem::take(&mut player.name));
    }
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
    let seen = crate::seen(&player);
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
/// [`Off`] when the loader refuses the channel, or on a server whose game
/// build lacks the event of an accepted interaction.
pub fn open(load: &mut Load<'_>, world: World, last: Arc<Last>) -> Result<Channel<Shown>, Off> {
    let channel = load.channel::<Shown, _>(CHANNEL, VERSION, move |from, shown| {
        receive(&last, from, shown);
    })?;
    let followed = load.on::<AcceptOfferedInteractionEvent>(move |used| {
        accepted(channel, world, used.guest_entity_id, used.host_entity_id);
    });
    match followed {
        Err(off) if load.host().build().side == Side::SERVER => Err(off),
        // A client hears the server; the event is the server's part.
        Err(off) => {
            enshwick::warn!("the Safe's panel follows no opened chest on this side: {off}");
            Ok(channel)
        }
        Ok(()) => Ok(channel),
    }
}

/// The open screen's host as the frames see it, how often it changed, and
/// the interface's frames so far.
#[derive(Debug, Default)]
struct Hosts {
    last: AtomicU32,
    changes: AtomicU64,
    /// Counted every frame whatever the screen; zero while the loader runs
    /// no frame callback.
    frames: AtomicU64,
}

impl Hosts {
    /// Notes `host`, the open screen's host in a frame (`None` for none),
    /// and returns how often the host changed so far.
    fn see(&self, host: Option<Entity>) -> u64 {
        let host = host.map_or(0, |e| e.0);
        if self.last.swap(host, Ordering::Relaxed) == host {
            self.changes.load(Ordering::Relaxed)
        } else {
            self.changes.fetch_add(1, Ordering::Relaxed) + 1
        }
    }

    /// Counts one frame of the interface, whose open screen has `host`.
    fn frame(&self, host: Option<Entity>) {
        self.frames.fetch_add(1, Ordering::Relaxed);
        let _count = self.see(host);
    }

    /// The host changes and frames so far, as a panel drawing with `host`
    /// marks them.
    fn mark(&self, host: Option<Entity>) -> (u64, u64) {
        (self.see(host), self.frames.load(Ordering::Relaxed))
    }

    /// Whether the chest screen opened again between the draws marked
    /// `drawn` and `now`: another host, or a frame without the panel, which
    /// still tells a reopened chest on a build without the host read.
    fn reopened(drawn: Option<(u64, u64)>, now: (u64, u64)) -> bool {
        drawn.is_none_or(|(changes, frames)| changes != now.0 || now.1.wrapping_sub(frames) > 1)
    }
}

/// What the panel draws, kept between frames.
struct Panel {
    last: Arc<Last>,
    actions: Actions,
    texts: Texts,
    /// How often the open screen's host changed, closing included, as every
    /// frame sees it.
    hosts: Arc<Hosts>,
    /// When the chest screen last opened.
    opened: Instant,
    /// The host changes and frames when the panel last drew.
    drawn: Option<(u64, u64)>,
    /// When the message drawn came, to tell a new one.
    since: Option<Instant>,
    /// When the message drawn while the screen was open before came: it
    /// was for that chest, so a chest opened soon after waits for the
    /// server's word.
    stale: Option<Instant>,
    /// The title and the help line, in the player's language.
    words: (String, String),
    /// Switches the player changed since the message drawn and when, until
    /// the server's answer or [`UNANSWERED`].
    pending: FxHashMap<PlayerKey, (bool, Instant)>,
}

impl Panel {
    fn draw(&mut self, frame: &mut Frame<'_>) {
        let now = Instant::now();
        let mark = self.hosts.mark(frame.screen_host());
        if Hosts::reopened(self.drawn, mark) {
            self.opened = now;
            self.stale = self.since.take();
        }
        self.drawn = Some(mark);
        let Some(last) = self.last.load_full() else {
            return;
        };
        let (shown, received) = &*last;
        let Shown::Safe { at, players } = shown else {
            return;
        };
        if *received + GRACE < self.opened || self.stale == Some(*received) {
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
                .filter(|(_, at)| now.saturating_duration_since(*at) < UNANSWERED)
                .map_or(player.allowed, |&(on, _)| on);
            if !frame.toggle(Toggle::new(&player.name), &mut on) {
                continue;
            }
            let args = [
                Value::Player {
                    key: player.key,
                    name: player.name.clone(),
                },
                Value::Toggle(on),
            ];
            let safe = ActionTarget::at(centre(*at));
            match self.actions.run_at(ACCESS_ACTION, &args, safe) {
                Ok(()) => {
                    self.pending.insert(player.key, (on, now));
                }
                Err(off) => enshwick::warn!("the switch for {} was not sent: {off}", player.name),
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
    let hosts = Arc::new(Hosts::default());
    let seen = Arc::clone(&hosts);
    if let Err(off) = load.on_frame(move |frame| {
        seen.frame(frame.screen_host());
    }) {
        enshwick::warn!(
            "the Safe's panel tells only another chest, not the same one reopened: {off}"
        );
    }
    let mut panel = Panel {
        last,
        actions,
        texts,
        hosts,
        opened: Instant::now(),
        drawn: None,
        since: None,
        stale: None,
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
    fn a_chest_counts_as_reopened_when_its_screen_closed_or_another_opened() {
        let hosts = Hosts::default();
        let chest = Some(Entity(512));
        let first = hosts.see(chest);
        assert_eq!(
            hosts.see(chest),
            first,
            "the same screen, frame after frame"
        );
        let _closed = hosts.see(None);
        let again = hosts.see(chest);
        assert_ne!(again, first, "closed and opened again");
        assert_ne!(hosts.see(Some(Entity(513))), again, "another chest");
    }

    #[test]
    fn a_chest_closed_for_a_frame_counts_as_reopened_on_a_build_without_the_host_read() {
        let hosts = Hosts::default();
        hosts.frame(None);
        let drawn = hosts.mark(None);
        hosts.frame(None);
        let next = hosts.mark(None);
        assert!(!Hosts::reopened(Some(drawn), next), "the next frame");
        hosts.frame(None);
        hosts.frame(None);
        assert!(
            Hosts::reopened(Some(next), hosts.mark(None)),
            "a frame without the panel"
        );
        assert!(Hosts::reopened(None, next), "never drawn");
    }

    #[test]
    fn a_long_list_is_cut_to_fit_one_message() {
        let mut safe = Safe::new(PlayerKey::new(1), "Ann");
        safe.allowed.push(Allowed {
            key: PlayerKey::new(500),
            name: "ё".repeat(200),
        });
        let online = (2..200).map(|key| (PlayerKey::new(u64::MAX - key), "x".repeat(64)));
        let shown = listing((0, 0, 0), &safe, PlayerKey::new(1), online);
        let Shown::Safe { players, .. } = &shown else {
            panic!("a Safe")
        };
        assert_eq!(players.len(), MAX_LISTED);
        assert_eq!(players[0].name, "x".repeat(64));
        assert_eq!(
            players.last().map(|p| (p.key, p.name.as_str(), p.allowed)),
            Some((PlayerKey::new(500), "ё".repeat(32).as_str(), true)),
            "an allowed player sorting last stays, cut"
        );
        assert_eq!(bounded("ё".repeat(200)), "ё".repeat(32));
        assert_eq!(
            bounded(format!("a{}", "ё".repeat(100))),
            format!("a{}", "ё".repeat(31)),
            "cut between characters"
        );
        assert_eq!(bounded("Ann".into()), "Ann");
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
