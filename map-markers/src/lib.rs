//! Map Markers: the loot chests and resources around each player, on that
//! player's map.
//!
//! The world's entities are the server's to read, so the server part picks
//! each player's nearest marks every two seconds and sends them over the
//! mod's channel; the client part shows them with the map's custom markers.
//! A client without the mod gets nothing.

mod marks;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use enshwick::i18n::{Language, Texts};
use enshwick::net::{Channel, Peer};
use enshwick::state::ModLocal;
use enshwick::ui::{MAX_MARKERS, Marker, MarkerColor, Ui};
use enshwick::world::{ContainerKinds, World};
use enshwick::{Load, Mod, Off, Relock};

use crate::marks::{Kind, Mark};

enshwick::texts!();

/// The channel the server sends each player's marks on.
const CHANNEL: &str = "marks";
/// The version of [`CHANNEL`]'s message, a list of [`Mark`].
const CHANNEL_VERSION: u16 = 1;
/// How often the server picks the marks again.
const EVERY: Duration = Duration::from_secs(2);
/// The label key of a chest's mark.
const CHEST_LABEL: &str = "marker.chest";
/// The label key of a resource's mark.
const RESOURCE_LABEL: &str = "marker.resource";

/// Map Markers settings.
#[enshwick::config]
#[derive(Debug)]
struct Settings {
    /// Marks the world's loot chests.
    chests: bool,
    /// Marks plants and the other nodes that hold what they yield.
    resources: bool,
    /// Metres across the ground from the player.
    #[config(range = 10.0..=1000.0)]
    radius: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            chests: true,
            resources: true,
            radius: 150.0,
        }
    }
}

/// What the server marks, as the settings say.
#[derive(Debug)]
struct Wanted {
    chests: AtomicBool,
    resources: AtomicBool,
    radius: AtomicU64,
}

static WANTED: ModLocal<Wanted> = ModLocal::new();

fn wanted() -> &'static Wanted {
    WANTED.get_or_init(|| {
        let defaults = Settings::default();
        Wanted {
            chests: AtomicBool::new(defaults.chests),
            resources: AtomicBool::new(defaults.resources),
            radius: AtomicU64::new(defaults.radius.to_bits()),
        }
    })
}

fn apply(settings: &Settings) {
    let wanted = wanted();
    wanted.chests.store(settings.chests, Ordering::Relaxed);
    wanted
        .resources
        .store(settings.resources, Ordering::Relaxed);
    wanted
        .radius
        .store(settings.radius.to_bits(), Ordering::Relaxed);
    enshwick::info!(
        "marks within {} m: chests {}, resources {}",
        settings.radius,
        settings.chests,
        settings.resources
    );
}

/// On the server: sends every player who runs the mod the marks around them.
fn send_marks(world: World, channel: Channel<Vec<Mark>>) {
    let receivers: Vec<(Peer, [f64; 3])> = world
        .players()
        .into_iter()
        .map(|player| (Peer::Player(player.key), player.entity))
        .filter(|&(peer, _)| channel.is_ready(peer))
        .filter_map(|(peer, entity)| Some((peer, world.position(entity)?)))
        .collect();
    // The walk over the world's holders costs the game thread, so a server
    // nobody can see marks on skips it.
    if receivers.is_empty() {
        return;
    }
    let wanted = wanted();
    let mut found = Vec::new();
    for (kind, on, of) in [
        (Kind::Chest, &wanted.chests, ContainerKinds::LOOT),
        (Kind::Resource, &wanted.resources, ContainerKinds::PLANTS),
    ] {
        if on.load(Ordering::Relaxed) {
            match world.containers(of) {
                Ok(placed) => found.extend(placed.into_iter().map(|p| (kind, p.position))),
                Err(error) => {
                    enshwick::warn_every!(
                        Duration::from_secs(60),
                        "no marks sent: containers not listed: {error}"
                    );
                    return;
                }
            }
        }
    }
    let radius = f64::from_bits(wanted.radius.load(Ordering::Relaxed));
    for (peer, at) in receivers {
        let marks = marks::nearest(found.iter().copied(), at, radius);
        // Bounded to `MAX_MARKERS`, a set always fits one message, and at
        // one message every two seconds the rate never runs out.
        let _sent = channel.send(peer, &marks);
    }
}

/// The client's side: the map and the marks it shows now.
#[derive(Debug)]
struct Shown {
    ui: Ui,
    texts: Texts,
    /// The marks on the map and the language of their labels; `None` while
    /// none are.
    marks: Mutex<Option<(Language, Vec<Mark>)>>,
    /// Whether any marks are on the map, read every frame without the lock.
    any: AtomicBool,
}

static SHOWN: ModLocal<Shown> = ModLocal::new();

/// On a client: shows the marks the server sent in `language`, unless they
/// are the ones already shown in it.
fn show(shown: &Shown, mut marks: Vec<Mark>, language: Language) {
    marks.truncate(MAX_MARKERS);
    let mut current = shown.marks.relock();
    if current
        .as_ref()
        .is_some_and(|(was, them)| *was == language && *them == marks)
    {
        return;
    }
    let chest = shown.texts.fill(CHEST_LABEL, language, &[]);
    let resource = shown.texts.fill(RESOURCE_LABEL, language, &[]);
    let markers: Vec<Marker<'_>> = marks
        .iter()
        .map(|mark| {
            let (label, color) = match mark.kind {
                Kind::Chest => (chest.as_str(), MarkerColor::Yellow),
                Kind::Resource => (resource.as_str(), MarkerColor::Green),
            };
            Marker::new(mark.at.map(f64::from))
                .label(label)
                .color(color)
        })
        .collect();
    match shown.ui.set_markers(&markers) {
        Ok(()) => {
            shown.any.store(!marks.is_empty(), Ordering::Relaxed);
            *current = Some((language, marks));
        }
        Err(why) => enshwick::info_once!("marks not shown: {why}"),
    }
}

/// On a client: takes the marks off the map once the server that sent them
/// is gone, so they do not stay on the next world's map.
fn forget_without(shown: &Shown, channel: Channel<Vec<Mark>>) {
    if !shown.any.load(Ordering::Relaxed) || channel.is_ready(Peer::Server) {
        return;
    }
    let mut current = shown.marks.relock();
    if shown.ui.clear_markers().is_ok() {
        shown.any.store(false, Ordering::Relaxed);
        *current = None;
    }
}

/// The mod.
#[derive(Debug)]
pub struct MapMarkers;

impl Mod for MapMarkers {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = load.texts()?;
        if let Ok(ui) = load.ui() {
            let _shown = SHOWN.get_or_init(|| Shown {
                ui,
                texts,
                marks: Mutex::new(None),
                any: AtomicBool::new(false),
            });
        }
        load.on_settings_change(apply)?;
        let channel = load.channel::<Vec<Mark>, _>(CHANNEL, CHANNEL_VERSION, |from, marks| {
            if let (Peer::Server, Some(shown)) = (from, SHOWN.get()) {
                show(shown, marks, enshwick::i18n::language());
            }
        })?;
        if let Some(shown) = SHOWN.get() {
            load.on_frame(move |_| forget_without(shown, channel))?;
        }
        let world = load.world()?;
        load.on_tick(EVERY, move |_| send_marks(world, channel))?;
        Ok(Self)
    }
}

enshwick::declare_mod!(MapMarkers);

#[cfg(test)]
mod tests;
