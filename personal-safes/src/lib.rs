//! The Safe: a chest crafted beside the wooden chest that only the player who
//! placed it, and the players they let in from the access window, can open or
//! empty; only the owner picks it up, and nobody upgrades it away. The owner
//! shows on hover, as the chest's name.
//!
//! The Safe item places the vanilla wooden chest, so a Safe without the mod is
//! an ordinary chest with every item still in it. The placed chest keeps the
//! Safe's item id in the save, which tells the mod which chests are Safes;
//! owners and the players they allowed are kept per world, in
//! `mod-data/enshwick.personal-safes/worlds/<world>/owners.toml`, by position.
//!
//! The refusals run on the dedicated server and, on a client, in the built-in
//! server while it hosts a game; the client's own world is left alone.

pub mod access;
pub mod actions;
pub mod content;
mod keys;
pub mod points;

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use enshwick::abi::BuildingData;
use enshwick::abi::Side;
use enshwick::config::ConfigError;
use enshwick::events::{
    BuildingPlaced, BuildingRemoved, PlayerJoined, PlayerLeft, ShutdownStarted, Ticked,
    WorldLoaded, WorldSaved,
};
use enshwick::i18n::Language;
use enshwick::state::{ModLocal, Store, WorldStore};
use enshwick::world::Rename;
use enshwick::world::{ContainerKinds, World};
use enshwick::{Entity, Load, Mod, Off, PlayerKey};

use crate::access::{Action, Allowed, Cell, Safe, Seen, snapshots};
use crate::actions::Phrases;

/// Safe settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// The Safe costs the wooden chest's ingredients times this. A joined
    /// player's crafting menu shows the server's value.
    #[config(sync, range = 1..=100)]
    cost_multiplier: u32,
    /// Whether the server's administrators may open, remove and rename every Safe.
    #[config(runtime)]
    admins_can_open: bool,
    /// The language of the owner's name on a Safe: "en", "uk", "de", ...
    #[config(runtime)]
    label_language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cost_multiplier: 2,
            admins_can_open: true,
            label_language: "en".into(),
        }
    }
}

impl Settings {
    fn check(&self) -> Result<(), ConfigError> {
        self.language().map(|_| ()).ok_or_else(|| {
            ConfigError::text(
                keys::UNKNOWN_LANGUAGE,
                &[(keys::value::CODE, &self.label_language)],
            )
        })
    }

    fn language(&self) -> Option<Language> {
        Language::from_code(&self.label_language)
    }
}

/// The Safe's cost, times the wooden chest's, from the settings in force.
static COST: ModLocal<AtomicU32> = ModLocal::new();

fn cost_in_force() -> &'static AtomicU32 {
    COST.get_or_init(|| AtomicU32::new(Settings::default().cost_multiplier))
}

/// Puts `cost` in force, building the recipes again when it changed.
fn set_cost(cost: u32) {
    if cost_in_force().swap(cost, Ordering::Relaxed) == cost {
        return;
    }
    match enshwick::content::recipes::rebuild_recipes() {
        Ok(()) => enshwick::info!("the Safe costs the wooden chest x{cost}"),
        Err(off) => {
            enshwick::warn!("the Safe's cost x{cost} waits for a restart: {off}");
        }
    }
}

/// The name of the owners' store, per world and in the mod-wide file of
/// earlier versions.
const OWNERS: &str = "owners";
/// The mod-wide owners file from before owners were kept per world.
const SHARED_OWNERS_FILE: &str = "owners.toml";
/// Where the mod-wide owners file goes once a world took its owners.
const MOVED_OWNERS_FILE: &str = "owners.toml.moved";

/// The owners as stored: one record per Safe.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Owners {
    safes: Vec<Record>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Record {
    /// Half-metre cell of the Safe.
    cell: [i64; 3],
    /// The owner's account key, in hex: TOML integers stop at i64.
    owner: String,
    /// The owner's name as last seen; absent in files from 0.1.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
    /// Players the owner allowed; absent in files from 0.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowed: Vec<AllowedRecord>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct AllowedRecord {
    /// The player's account key, in hex.
    key: String,
    /// Their name when they were allowed.
    name: String,
}

fn hex(key: PlayerKey) -> String {
    format!("{key:016x}")
}

impl Owners {
    fn from_map(owners: &FxHashMap<Cell, Safe>) -> Self {
        let mut safes: Vec<Record> = owners
            .iter()
            .map(|(&(x, y, z), safe)| Record {
                cell: [x, y, z],
                owner: hex(safe.owner),
                name: safe.owner_name.clone(),
                allowed: safe
                    .allowed
                    .iter()
                    .map(|a| AllowedRecord {
                        key: hex(a.key),
                        name: a.name.clone(),
                    })
                    .collect(),
            })
            .collect();
        safes.sort_by_key(|r| r.cell);
        Self { safes }
    }

    fn to_map(&self) -> FxHashMap<Cell, Safe> {
        self.safes
            .iter()
            .filter_map(|r| {
                let safe = Safe {
                    owner: PlayerKey::new(u64::from_str_radix(&r.owner, 16).ok()?),
                    owner_name: r.name.clone(),
                    allowed: r
                        .allowed
                        .iter()
                        .filter_map(|a| {
                            Some(Allowed {
                                key: PlayerKey::new(u64::from_str_radix(&a.key, 16).ok()?),
                                name: a.name.clone(),
                            })
                        })
                        .collect(),
                };
                Some(((r.cell[0], r.cell[1], r.cell[2]), safe))
            })
            .collect()
    }
}

/// Safes removed since the world last saved, with their owners: the world
/// keeps a removal only when it saves, so a game that stops before that
/// brings the Safe back, and its owner must still be on disk.
static REMOVED: ModLocal<Mutex<Vec<(Cell, Safe)>>> = ModLocal::new();

/// The running world's owners store, once the mod has asked for it.
static STORE: ModLocal<&'static WorldStore<Owners>> = ModLocal::new();

fn removed() -> MutexGuard<'static, Vec<(Cell, Safe)>> {
    REMOVED
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The owners as the file keeps them: the Safes in force and those removed
/// since the world last saved, unless a Safe stands in their cell again.
fn on_disk() -> Owners {
    let removed = removed();
    if removed.is_empty() {
        return Owners::from_map(&snapshots().owners.load());
    }
    let mut owners = FxHashMap::clone(&snapshots().owners.load());
    for (at, safe) in removed.iter() {
        owners.entry(*at).or_insert_with(|| safe.clone());
    }
    Owners::from_map(&owners)
}

/// Hands the owners to the running world's store, which writes them when
/// the world saves; false while no world is open.
fn hand_over(store: &WorldStore<Owners>) -> bool {
    let owners = on_disk();
    if store.update(|kept| *kept = owners).is_none() {
        enshwick::warn!("owners not saved: no world is open");
        return false;
    }
    true
}

/// Hands the owners to the running world's store and writes them on a
/// thread of their own; a write that finds nothing changed since the last
/// one skips, so the newest owners are always the ones on disk.
fn save(store: &'static WorldStore<Owners>) {
    if !hand_over(store) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("safes-save".into())
        .spawn(move || {
            if let Err(error) = store.flush() {
                enshwick::warn!("owners not saved yet: {error}");
            }
        });
    if spawned.is_err() {
        enshwick::warn!("owners saved with the world: no thread");
    }
}

/// Puts the running world's owners in force for the checks. A world
/// without owners of its own takes those of the mod-wide `owners.toml`
/// written before owners were kept per world, once; the file is then
/// renamed out of the way.
fn open_world(store: &WorldStore<Owners>, dir: &Path) {
    let mut owners = store.with(Owners::to_map).unwrap_or_default();
    let shared = dir.join(SHARED_OWNERS_FILE);
    if owners.is_empty() && store.key().is_some() && shared.exists() {
        match Store::<Owners>::new(dir, OWNERS).load() {
            Ok(old) => {
                owners = old.to_map();
                store.update(|kept| *kept = old);
                let moved = store.flush().map_err(|e| e.to_string()).and_then(|()| {
                    std::fs::rename(&shared, dir.join(MOVED_OWNERS_FILE)).map_err(|e| e.to_string())
                });
                match moved {
                    Ok(()) => enshwick::info!(
                        "{} Safe owners moved from owners.toml into this world",
                        owners.len()
                    ),
                    Err(why) => enshwick::warn!("owners.toml stays: {why}"),
                }
            }
            Err(error) => enshwick::warn!("owners.toml not read: {error}"),
        }
    }
    enshwick::info!(
        "{} Safe owners in world {}",
        owners.len(),
        store.key().unwrap_or_default()
    );
    removed().clear();
    snapshots().forget_session();
    snapshots().load_owners(owners);
}

/// The mod's state.
#[derive(Debug)]
pub struct PersonalSafes;

impl Mod for PersonalSafes {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "enshwick.personal-safes",
            side: BOTH,
            name: "Personal Safes",
            join: true,
            was: ["safes"],
            points: [points::Access],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.migrate(content::migrations())?;
        let settings: Settings = load.config()?;
        let cost = settings.cost_multiplier;
        cost_in_force().store(cost, Ordering::Relaxed);
        let texts = load.texts(content::TEXTS)?;
        load.add_texts(
            &texts,
            &[
                (keys::SAFE_NAME, content::SAFE_NAME),
                (keys::SAFE_DESCRIPTION, content::SAFE_DESCRIPTION),
            ],
        )?;
        load.add_item(content::item())?;
        load.add_recipe_with(|| content::recipe(cost_in_force().load(Ordering::Relaxed)))?;
        snapshots()
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
        let phrases: &'static Phrases = Box::leak(Box::new(Phrases::new(
            texts,
            settings.language().unwrap_or(Language::FALLBACK),
        )));
        guard(load, phrases)?;
        enshwick::info!(
            "the Safe costs the wooden chest x{cost}; administrators may open Safes: {}",
            settings.admins_can_open
        );
        Ok(Self)
    }
}

/// The server half: owners, players, the refusals, labels and the actions.
fn guard(load: &mut Load<'_>, phrases: &'static Phrases) -> Result<(), Off> {
    let world = load.world()?;
    let client = load.host().build().side == Side::CLIENT;
    let dir = load.data_dir()?;
    // Before the store's own handler, which writes what this hands over.
    load.on::<WorldSaved>(|()| {
        removed().clear();
        if let Some(store) = STORE.get() {
            let _open = hand_over(store);
        }
    })?;
    // The store opens on the world's load, before the handler below runs.
    let store: &'static WorldStore<Owners> =
        Box::leak(Box::new(load.world_store::<Owners>(OWNERS)?));
    let _first = STORE.set(store);
    load.on::<WorldLoaded>(move |()| open_world(store, &dir))?;

    let _first = access::POINT.set(load.define_point()?);
    load.on_interaction(access::filter)?;
    track_players(load, world)?;
    load.on::<BuildingPlaced>(move |placed| {
        if content::is_safe(placed.item) {
            placed_safe(world, phrases, placed);
            save(store);
        }
    })?;
    load.on::<BuildingRemoved>(move |removed| {
        if let Some(gone) = snapshots().removed(removed.entity) {
            enshwick::info!("Safe {} removed", removed.entity);
            self::removed().push(gone);
            let _open = hand_over(store);
        }
    })?;
    if client {
        // Entities, players and owners belong to one hosted game's world.
        load.on::<ShutdownStarted>(|()| {
            removed().clear();
            snapshots().forget_session();
            snapshots().load_owners(FxHashMap::default());
        })?;
        enshwick::info!("the Safe's checks act while this client hosts a game");
    }
    load.on_config_change(move |settings: Settings| {
        snapshots()
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
        phrases.set_language(settings.language().unwrap_or(Language::FALLBACK));
        set_cost(settings.cost_multiplier);
    })?;
    label_seen(load, world, phrases, store)?;
    load.on_rename(move |rename| may_rename(world, rename))?;
    actions::declare(load, world, phrases.texts.clone(), move || save(store))?;
    Ok(())
}

/// Records a Safe just placed, with its owner, and names the owner on it.
fn placed_safe(world: World, phrases: &Phrases, placed: &BuildingData) {
    let entity = placed.player;
    let player = world.player(entity);
    let key = player
        .as_ref()
        .map(|p| p.key)
        .or_else(|| snapshots().players.load().get(&entity).map(|seen| seen.key));
    let Some(key) = key else {
        enshwick::warn!("a Safe was placed by no known player and stays open to all");
        return;
    };
    let name = player.map(|p| p.name).unwrap_or_default();
    let position = placed.position.map(f64::from);
    snapshots().placed(placed.entity, access::cell(position), key, &name);
    if !name.is_empty() {
        let label = phrases.say(keys::LABEL, &[(keys::value::OWNER, &name)]);
        if !world.set_label(placed.entity, &label) {
            enshwick::warn!("Safe {} not labelled", placed.entity);
        }
    }
    enshwick::info!(
        "Safe {} placed at {position:?} by {key:016x}",
        placed.entity
    );
}

/// Whether the player behind `rename` may rename its entity: anyone for
/// what is not a Safe; the owner, or an administrator when allowed, for a
/// Safe.
fn may_rename(world: World, rename: &Rename) -> bool {
    let known = snapshots().safes.load().get(&rename.entity).copied();
    let found = known.or_else(|| {
        if !content::is_safe(world.used_item(rename.entity)?) {
            return None;
        }
        snapshots().seen(rename.entity, access::cell(world.position(rename.entity)?))
    });
    let Some((owner, at)) = found else {
        return true;
    };
    let Some(player) = &rename.player else {
        return false;
    };
    let seen = Seen {
        key: player.key,
        admin: player.is_admin(),
    };
    let admins_open = snapshots().admins_open.load(Ordering::Relaxed);
    let owners = snapshots().owners.load();
    let fallback = Safe::new(owner, "");
    let safe = owners.get(&at).unwrap_or(&fallback);
    let entities = (player.entity, rename.entity);
    let allowed = access::decide(safe, Some(seen), admins_open, Action::Manage, entities);
    if !allowed {
        enshwick::info!("{} may not rename Safe {}", player.name, rename.entity);
    }
    allowed
}

/// Learns every Safe the world holds that this session has not seen yet:
/// base stock asks without the chest's item or position, so a Safe nobody
/// looked at would otherwise lend its stock to anyone crafting nearby.
fn learn_safes(world: World) {
    let known = snapshots().safes.load();
    for placed in world.containers(ContainerKinds::default()) {
        if known.contains_key(&placed.entity)
            || !world.used_item(placed.entity).is_some_and(content::is_safe)
        {
            continue;
        }
        let _owner = snapshots().seen(placed.entity, access::cell(placed.position));
    }
}

/// Names the owner on every Safe seen this session that has no name yet,
/// on the game thread; the owner's name comes from the record, or from the
/// owner when they are online. Learns the world's Safes first.
fn label_seen(
    load: &mut Load<'_>,
    world: World,
    phrases: &'static Phrases,
    store: &'static WorldStore<Owners>,
) -> Result<(), Off> {
    let mut done: FxHashSet<Entity> = FxHashSet::default();
    load.scheduler()?.every(Duration::from_secs(2), move || {
        learn_safes(world);
        let safes = snapshots().safes.load();
        let mut learned = false;
        for (&entity, &(owner, at)) in safes.iter() {
            if done.contains(&entity) {
                continue;
            }
            let named = world.has_label(entity) == Some(true);
            if named {
                done.insert(entity);
                continue;
            }
            let mut name = snapshots()
                .owners
                .load()
                .get(&at)
                .map(|safe| safe.owner_name.clone())
                .unwrap_or_default();
            if name.is_empty()
                && let Some(online) = world.player_by_key(owner)
            {
                name = online.name;
                snapshots().change(at, |safe| safe.owner_name.clone_from(&name));
                learned = true;
            }
            if name.is_empty() {
                continue;
            }
            let label = phrases.say(keys::LABEL, &[(keys::value::OWNER, &name)]);
            if world.set_label(entity, &label) {
                done.insert(entity);
            }
        }
        if learned {
            save(store);
        }
        done.retain(|entity| safes.contains_key(entity));
        std::ops::ControlFlow::Continue(())
    });
    Ok(())
}

fn seen(player: &enshwick::world::Player) -> Seen {
    Seen {
        key: player.key,
        admin: player.is_admin(),
    }
}

/// Retakes the players snapshot from the world, on the game thread.
fn snapshot_players(world: World) {
    let players: FxHashMap<Entity, Seen> = world
        .players()
        .iter()
        .map(|p| (p.entity, seen(p)))
        .collect();
    snapshots().players.store(std::sync::Arc::new(players));
}

/// Keeps the connected players' keys and administrator flags current: on
/// joining and leaving, every 10 s for permissions, and at the next tick
/// after a check that met a character the snapshot lacks.
fn track_players(load: &mut Load<'_>, world: World) -> Result<(), Off> {
    load.on::<PlayerJoined>(move |joined| {
        let entity = joined.entity;
        let player = world.player(entity).map_or(
            Seen {
                key: joined.key,
                admin: false,
            },
            |p| seen(&p),
        );
        snapshots().players.rcu(|players| {
            let mut players = FxHashMap::clone(players);
            players.insert(entity, player);
            players
        });
    })?;
    load.on::<PlayerLeft>(|left| {
        snapshots().players.rcu(|players| {
            let mut players = FxHashMap::clone(players);
            players.remove(&left.entity);
            players
        });
    })?;
    load.on::<Ticked>(move |_| {
        if snapshots().unregistered.swap(false, Ordering::Relaxed) {
            snapshot_players(world);
        }
    })?;
    // Permissions can change while a player is online.
    load.scheduler()?.every(Duration::from_secs(10), move || {
        snapshot_players(world);
        std::ops::ControlFlow::Continue(())
    });
    Ok(())
}

enshwick::declare_mod!(PersonalSafes);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        use enshwick::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.cost_multiplier, 2);
        assert!(settings.admins_can_open);
        assert!(settings.validate().is_ok());
        assert_eq!(settings.label_language, "en");
        let zero = Settings {
            cost_multiplier: 0,
            ..Settings::default()
        };
        assert!(zero.validate().is_err());
        let unknown = Settings {
            label_language: "xx".into(),
            ..Settings::default()
        };
        assert_eq!(
            unknown.check().map_err(|refused| refused.to_string()),
            Err(format!("{} (code = xx)", keys::UNKNOWN_LANGUAGE)),
            "refused with a text of the mod's own"
        );
        assert!(unknown.validate().is_err());
    }

    #[test]
    fn a_world_s_cost_applies_when_it_loads() {
        use enshwick::abi::Status;
        use enshwick::events::SettingsReplaced;

        let mut host = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2);
        for (ty, hash) in [
            (enshwick::content::types::ITEM_INFO, 10),
            (enshwick::content::types::ITEM_REGISTRY_RESOURCE, 11),
            (enshwick::content::types::RECIPE_REGISTRY_RESOURCE, 12),
        ] {
            host = host.type_hash(ty.id(), hash);
        }
        for (id, size) in [
            ("item_stat", 0x28),
            ("recipe", 0xa8),
            ("recipe_ingredient", 48),
            ("recipe_stack", 24),
        ] {
            host = host.type_size(id, size);
        }
        for (id, at) in [
            ("recipe_registry.recipes", 8),
            ("recipe.guid", 0),
            ("recipe.id", 0x10),
            ("recipe.workshop_guid", 0x14),
            ("recipe.workshop", 0x24),
            ("recipe.input", 0x28),
            ("recipe.output", 0x30),
            ("recipe.required_props", 0x40),
            ("recipe.completion", 0x5c),
            ("recipe.debug_name", 0x78),
            ("recipe_stack.item", 0x10),
            ("recipe_stack.count", 0x14),
            ("item_info.id", 0),
            ("item_info.object", 4),
            ("item_info.name", 0x48),
            ("item_info.icon_image", 0x58),
            ("item_info.description", 0x68),
            ("item_info.flags", 0x78),
            ("item_info.pid_entity", 0x80),
            ("item_info.ui_values", 0x90),
            ("item_stat.label", 0),
            ("item_stat.format", 4),
            ("item_stat.value", 8),
            ("item_stat.signed", 0x24),
            ("item_registry.refs", 0),
            ("item_registry.names", 0x38),
        ] {
            host = host.constant(id, at);
        }
        let loader = host
            .settings(
                "cost_multiplier = 2
label_language = \"en\"
",
            )
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let allowed = loader.rebuilds().len();
        assert_eq!(allowed, 1, "the registry may be rebuilt");
        loader.set_settings(
            "cost_multiplier = 2
label_language = \"en\"
",
        );
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        assert_eq!(
            loader.rebuilds().len(),
            allowed,
            "the same cost: no rebuild"
        );
        loader.set_settings(
            "cost_multiplier = 5
label_language = \"uk\"
",
        );
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        assert_eq!(cost_in_force().load(Ordering::Relaxed), 5);
        assert_eq!(
            loader.rebuilds().len(),
            allowed + 1,
            "the world's cost: rebuilt"
        );
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn settings_from_0_1_still_parse() {
        use enshwick::config::Config;
        let old: Settings =
            toml::from_str("cost_multiplier = 3\nadmins_can_open = false\n").unwrap();
        assert_eq!(old.cost_multiplier, 3);
        assert_eq!(old.label_language, "en");
        assert!(old.validate().is_ok());
    }

    #[test]
    fn owners_survive_the_file_with_keys_past_i64() {
        let mut friendly = Safe::new(PlayerKey::new(5), "Ann");
        friendly.allow(PlayerKey::new(u64::MAX - 1), "Bob Lee");
        let owners = FxHashMap::from_iter([
            ((1, -2, 3), Safe::new(PlayerKey::new(u64::MAX), "")),
            ((0, 0, 0), friendly),
        ]);
        let text = toml::to_string(&Owners::from_map(&owners)).unwrap();
        let back: Owners = toml::from_str(&text).unwrap();
        assert_eq!(back.to_map(), owners);
    }

    #[test]
    fn owners_files_from_0_1_still_load() {
        let old = "[[safes]]\ncell = [1, 2, 3]\nowner = \"00000000000000ff\"\n";
        let back: Owners = toml::from_str(old).unwrap();
        assert_eq!(
            back.to_map(),
            FxHashMap::from_iter([((1, 2, 3), Safe::new(PlayerKey::new(0xff), ""))])
        );
    }
}
