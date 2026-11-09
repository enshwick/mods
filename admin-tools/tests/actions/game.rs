//! The fake game the tests run in: a dedicated server or a hosted game
//! with Ann, an administrator, and Bob online.

use std::path::{Path, PathBuf};
use std::time::Duration;

use enshwick::abi::{ActionCall, DaytimeInfo, Header, PlayerData, PlayerInfo, Side, Status};
use enshwick::content::EngineFacts as _;
use enshwick::content::Guid;
use enshwick::events::{PlayerJoined, SettingsReplaced, WorldLoaded};
use enshwick::test::{Installed, facts};
use enshwick::{ItemId, PlayerKey};
use enshwick_admin_tools::{enshwick_mod_init, enshwick_mod_query, enshwick_mod_ready};

pub(crate) const ADMIN: u8 = PlayerInfo::CAN_KICK_BAN;

/// The game's skipped inventory entity, which the inventory reads compare to.
static SKIPPED: [u32; 1] = [0];

/// A glider in the fake's item registry, one to a stack.
pub(crate) const GLIDER: Guid = Guid([0x61; 16]);

/// Wood in the fake's item registry, a hundred to a stack.
pub(crate) const WOOD: Guid = Guid([0x62; 16]);

/// The wooden chest in the fake's item registry.
pub(crate) const CHEST: Guid = Guid([0x63; 16]);

/// The fake's type hash of a health attribute set; past the world
/// preset's own hashes.
pub(crate) const HEALTH: u32 = 0x4000_0001;

/// A health attribute set at `current` of `max`, in the fake's layout:
/// its id, the values' offset at 4, flags at 6, count at 7, then the
/// values with the maximum in slot 7.
pub(crate) fn health(current: i32, max: i32) -> Vec<u8> {
    let mut bytes = 10u32.to_le_bytes().to_vec();
    bytes.extend(8u16.to_le_bytes());
    bytes.extend([1, 8]);
    bytes.extend(
        [current, 0, 0, 0, 0, 0, 0, max]
            .iter()
            .flat_map(|v| v.to_le_bytes()),
    );
    bytes
}

pub(crate) fn player(entity: u32, key: u64, name: &str, permissions: u8) -> PlayerInfo {
    let mut info = PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: entity,
        entity: enshwick::Entity(entity),
        key: PlayerKey::new(key),
        permissions,
        name_len: u8::try_from(name.len()).unwrap(),
        name: [0; 64],
    };
    info.name[..name.len()].copy_from_slice(name.as_bytes());
    info
}

pub(crate) fn ann() -> PlayerInfo {
    player(10, 1, "Ann", ADMIN)
}

pub(crate) fn bob() -> PlayerInfo {
    player(11, 2, "Bob", 0)
}

/// A fresh data folder for one test, removed when the test ends.
pub(crate) struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _gone = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("admin-tools-{name}-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    Scratch(dir)
}

/// Waits up to five seconds for the file `name` in `dir` to satisfy
/// `holds`; its last text.
pub(crate) fn file_until(dir: &Path, name: &str, holds: impl Fn(&str) -> bool) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let text = std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        if holds(&text) || std::time::Instant::now() >= deadline {
            return text;
        }
        std::thread::yield_now();
    }
}

/// A dedicated server with Ann (admin) and Bob online, `settings` for
/// the world, its data in `dir`, and the mod started with the world
/// loaded.
pub(crate) fn server(settings: &str, dir: &Path) -> Installed {
    server_with(settings, dir, &[])
}

/// [`server`] with `more` players online beside Ann and Bob.
pub(crate) fn server_with(settings: &str, dir: &Path, more: &[PlayerInfo]) -> Installed {
    game(settings, dir, more, false)
}

/// [`server`] in a game a player hosts.
pub(crate) fn hosted(settings: &str, dir: &Path) -> Installed {
    game(settings, dir, &[], true)
}

/// [`server_with`] on a dedicated server, or in a game a player hosts
/// when `hosted`.
pub(crate) fn game(settings: &str, dir: &Path, more: &[PlayerInfo], hosted: bool) -> Installed {
    // Parallel tests would all take the remote console's default port.
    let settings = if settings.contains("rcon") {
        settings.to_owned()
    } else {
        format!("rcon = \"off\"\n{settings}")
    };
    let host = if hosted {
        enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .hosting(true)
    } else {
        enshwick::test::FakeHost::new().side(Side::SERVER)
    }
    .world();
    let transform = host.hash(facts::CURRENT_TRANSFORM.id()).unwrap();
    // Inventories resolve these; the actions under test never call them.
    let mut host = host
        .symbol("ecs.type_index", 0x1_4064_5cc0)
        .symbol("ecs.component_read_raw", 0x1_4064_6b00)
        .symbol("ecs.slot_root", 0x1_4093_5420)
        .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
        .constant("ecs.context_entity_world", 0x30)
        .constant("ecs.registry_type_map", 0x30)
        .constant("ecs.registry_type_map_fallback", 0x88)
        .items()
        .recipes()
        .workshops()
        .props()
        .inventory()
        .vanilla_items(&[
            (GLIDER, "Tool_Glider_T1"),
            (WOOD, "Resource_Wood"),
            (CHEST, "Prop_Storage_32_T2_Chest_Wood"),
        ])
        .item(ItemId::from_guid(GLIDER), 1, 0)
        .item(ItemId::from_guid(WOOD), 100, 0)
        .item(ItemId::from_guid(CHEST), 1, 0)
        .item_places(ItemId::from_guid(CHEST), [5; 16])
        .type_hash("attribute_set.health", HEALTH)
        .type_hash("attribute_set.stamina", HEALTH + 1)
        .type_hash("attribute_set.mana", HEALTH + 2)
        .type_hash("attribute_set.level", HEALTH + 3)
        .type_hash("attribute_set.experience", HEALTH + 4)
        .constant("ecs.level_data_offset", 4)
        .constant("ecs.level_flags", 6)
        .constant("ecs.attribute_count", 7)
        .constant("attribute.health", 10)
        .constant("attribute.health_max_slot", 7)
        .constant("attribute.stamina", 20)
        .constant("attribute.stamina_max_slot", 2)
        .constant("attribute.mana", 30)
        .constant("attribute.mana_max_slot", 1)
        .constant("attribute.level", 40)
        .constant("attribute.experience", 50)
        .constant("attribute.experience_needed_slot", 2)
        .world_key("world0")
        .data_dir(dir.to_str().unwrap())
        .settings(&settings);
    for (who, place) in [(ann(), [40, 12, -3]), (bob(), [0, 20, 7])] {
        host = host
            .player(who)
            .template(who.entity, [7; 16])
            .component(who.entity, transform, &standing(place))
            .component(who.entity, HEALTH, &health(150, 200));
    }
    for who in more {
        host = host.player(*who);
    }
    let loader = host.install();
    // Transactions need a world with terrain.
    loader.set_terrain(&[]);
    // SAFETY: the exports of the mod under test.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    loader.set_daytime(DaytimeInfo {
        header: Header::of::<DaytimeInfo>(),
        time: 0,
        day_start: 6 * 3_600_000_000_000,
        day_end: 18 * 3_600_000_000_000,
        day_length: 1_800_000_000_000,
        night_length: 720_000_000_000,
        speed: 1.0,
        flags: 0,
    });
    loader.set_weather([1.0, 0.0, 0.0, 0.0], &[]);
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert!(loader.raise::<WorldLoaded>(&()) >= 1);
    loader
}

/// A `CurrentTransform` standing at `[x, y, z]` metres.
pub(crate) fn standing([x, y, z]: [i64; 3]) -> Vec<u8> {
    [x << 32, y << 32, z << 32]
        .iter()
        .flat_map(|c| c.to_le_bytes())
        .collect()
}

pub(crate) fn run(loader: &Installed, who: &PlayerInfo, action: &str, args: &str) -> String {
    loader
        .ask_actions(ActionCall::RUN, who, 0, action, args)
        .unwrap()
}

pub(crate) fn joins(loader: &Installed, who: &PlayerInfo) {
    let data = PlayerData {
        header: Header::of::<PlayerData>(),
        slot: who.slot,
        entity: who.entity,
        key: who.key,
    };
    assert!(loader.raise::<PlayerJoined>(&data) >= 1);
    // The check runs after the step, at the scheduler's next turn.
    let _ran = loader.tick(Duration::from_secs(1));
}

/// Ticks the fake server a second at a time until `done` holds or ten
/// seconds of real time pass; whether it held.
pub(crate) fn tick_until(loader: &Installed, done: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if done() {
            return true;
        }
        let _ran = loader.tick(Duration::from_secs(1));
        // The game writes committed transactions between its steps.
        loader.run_transactions();
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

pub(crate) fn said(loader: &Installed, key: u64) -> Vec<String> {
    loader
        .chat()
        .into_iter()
        .chain(loader.notices())
        .filter(|(to, _)| *to == PlayerKey::new(key))
        .map(|(_, text)| text)
        .collect()
}
