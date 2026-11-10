//! The platform's reads of the running game and its world that no other
//! check covers: reflection, the code section, the archive, the altars and
//! their bases, the map, the console and the loop's waits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use enshwick::abi::HookFlags;
use enshwick::{Next, hook};

use enshwick::altars::{Altars, ZoneSizes, base_limit, grouping, set_base_cap};
use enshwick::map::Map;
use enshwick::pacing::TaskWorkers;
use enshwick::resources::Registry;
use enshwick::schedule::Scheduler;
use enshwick::voxels::{VoxelPos, Voxels};
use enshwick::world::World;
use enshwick::{Load, Off, WorldError};

/// A reflected type every build has, and a field of it.
// ast-grep-ignore: engine-names-stay-in-loader
const SHAPE: &str = "keen::ecs::TriggerShape";
/// The field of [`SHAPE`] the probe looks up.
const SHAPE_FIELD: &str = "rangeX";
/// An ECS system every server build has.
const SYSTEM: &str = "add_buildzones_server";
/// A symbol the loader pins for every build.
const SYMBOL: &str = "npc.lookup";
/// Altar zones and bases the probe asks the game to group.
const ZONES: u32 = 20;
/// Bases the probe asks for, and the cap it sets.
const BASES: u32 = 16;
/// The console line the probe runs: its own action, as the console would.
const CONSOLE_LINE: &str = "enshwick.probe census";
/// How long the probe counts the loop's waits and the zone filter's calls.
const COUNT_FOR: Duration = Duration::from_secs(5);

/// Calls of the hooked system since start.
static SYSTEM_RUNS: AtomicUsize = AtomicUsize::new(0);
/// Whether a hooked call read a caller frame with a stack pointer.
static FRAME_SEEN: AtomicBool = AtomicBool::new(false);

/// Counts the system's runs and reads the caller's frame, then runs it.
#[hook(flags = HookFlags::CALLER_FRAME)]
fn system_update(next: Next<'_, unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    SYSTEM_RUNS.fetch_add(1, Ordering::Relaxed);
    if enshwick::host().is_some_and(|host| host.caller_frame().rsp != 0) {
        FRAME_SEEN.store(true, Ordering::Relaxed);
    }
    // SAFETY: the rest of the chain takes the context the game passed.
    unsafe { next.call(ctx) }
}

fn pass(what: &str) {
    enshwick::info!("PASS {what}");
}

fn fail(what: &str) {
    enshwick::warn!("FAIL {what}");
}

/// Logs `PASS what: value` for `Ok`, `FAIL what: why` for `Err`.
fn report<T: core::fmt::Debug, E: core::fmt::Display>(what: &str, result: Result<T, E>) {
    match result {
        Ok(value) => pass(&format!("{what}: {value:?}")),
        Err(why) => fail(&format!("{what}: {why}")),
    }
}

/// The checks, and the counts their hooks keep.
#[derive(Debug, Clone)]
pub(crate) struct PlatformChecks {
    world: World,
    voxels: Voxels,
    altars: Altars,
    zones: ZoneSizes,
    map: Map,
    scheduler: Scheduler,
    zone_calls: Arc<AtomicUsize>,
    loop_waits: Arc<AtomicUsize>,
    server_waits: Arc<AtomicUsize>,
}

impl PlatformChecks {
    /// Runs the checks that only init may make, and sets up the rest.
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let system = load.system(SYSTEM);
        report("reflection: system", system.as_ref().map(|_| ()));
        if let Ok(target) = system {
            // SAFETY: an ECS system's update function takes its query
            // context alone, as `SYSTEM_UPDATE` does.
            report("system hooked", unsafe {
                load.hook(&SYSTEM_UPDATE, target)
            });
        }
        report(
            "reflection: field offset",
            load.field_offset(SHAPE, SHAPE_FIELD),
        );
        report("reflection: type size", load.type_size(SHAPE));
        report("symbol", load.symbol(SYMBOL).map(|address| address != 0));
        report("code section bytes", load.code().map(<[u8]>::len));
        report(
            "task worker caps found",
            load.cap_task_workers(TaskWorkers::at_most(u8::MAX)),
        );
        report("base limit raised", load.raise_base_limit());
        report(
            "base limits requested",
            load.request_base_limits(ZONES, BASES),
        );

        let loop_waits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&loop_waits);
        report(
            "loop wait hooked",
            load.on_loop_wait(move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let server_waits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&server_waits);
        report(
            "server wait hooked",
            load.on_server_wait(move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let zone_calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&zone_calls);
        let zones = load.on_altar_zone(move |_| {
            counted.fetch_add(1, Ordering::Relaxed);
        })?;
        Ok(Self {
            world: load.world()?,
            voxels: load.voxels(),
            altars: load.altars()?,
            zones,
            map: load.map()?,
            scheduler,
            zone_calls,
            loop_waits,
            server_waits,
        })
    }

    /// Runs every check on the game thread.
    pub(crate) fn run(&self) {
        Self::host();
        Self::archive();
        self.world();
        self.altars();
        self.map();
        report("console line", enshwick::console::run(CONSOLE_LINE));
        let (loops, servers) = (Arc::clone(&self.loop_waits), Arc::clone(&self.server_waits));
        let (loops_before, servers_before) = (
            loops.load(Ordering::Relaxed),
            servers.load(Ordering::Relaxed),
        );
        let runs_before = SYSTEM_RUNS.load(Ordering::Relaxed);
        self.scheduler.after(COUNT_FOR, move || {
            let runs = SYSTEM_RUNS.load(Ordering::Relaxed) - runs_before;
            match (runs, FRAME_SEEN.load(Ordering::Relaxed)) {
                (0, _) => enshwick::info!("SKIP hook: {SYSTEM} did not run in {COUNT_FOR:?}"),
                (runs, true) => pass(&format!(
                    "hook: {SYSTEM} ran {runs} times in {COUNT_FOR:?}, its caller's frame read"
                )),
                (runs, false) => fail(&format!(
                    "hook: {SYSTEM} ran {runs} times in {COUNT_FOR:?}, no caller frame"
                )),
            }
            let loops = loops.load(Ordering::Relaxed) - loops_before;
            let servers = servers.load(Ordering::Relaxed) - servers_before;
            if loops > 0 && servers > 0 {
                pass(&format!(
                    "waits in {COUNT_FOR:?}: {loops} of the event loop, {servers} of the server loop"
                ));
            } else {
                fail(&format!(
                    "waits in {COUNT_FOR:?}: {loops} of the event loop, {servers} of the server loop"
                ));
            }
        });
    }

    fn host() {
        let Some(host) = enshwick::host() else {
            fail("host: none");
            return;
        };
        if host.hosting() {
            pass("hosting: this process runs the server");
        } else {
            fail("hosting: this process runs no server");
        }
        match host.game_version() {
            Ok(version) if version.contains('|') => pass(&format!("game version {version}")),
            other => fail(&format!("game version: {other:?}")),
        }
        let limit = enshwick::content::recipes::workshop_limit();
        if limit >= 512 {
            pass(&format!("workshop recipe limit {limit}"));
        } else {
            fail(&format!(
                "workshop recipe limit {limit}, under the game's 512"
            ));
        }
        match Registry::game() {
            Ok(registry) if registry.named(SHAPE).is_some() => {
                pass(&format!("reflected types name {SHAPE}"));
            }
            Ok(_) => fail(&format!("reflected types lack {SHAPE}")),
            Err(why) => fail(&format!("reflected types: {why}")),
        }
    }

    fn archive() {
        let Some(host) = enshwick::host() else {
            return;
        };
        let keys = match host.resource_keys(None) {
            Ok(keys) if !keys.is_empty() => keys,
            other => {
                fail(&format!("resource keys: {other:?}"));
                return;
            }
        };
        pass(&format!("resource keys: {}", keys.len()));
        let read = keys
            .iter()
            .find_map(|&key| host.game_resource(key).map(|bytes| (key, bytes)));
        match read {
            Some((key, bytes)) => {
                pass(&format!("resource read: {} bytes", bytes.len()));
                match host.served_resource(key) {
                    Ok(served) if served == bytes => {
                        pass("served resource: an unserved key reads as the archive's");
                    }
                    Ok(served) => fail(&format!(
                        "served resource: {} bytes, the archive's {}",
                        served.len(),
                        bytes.len()
                    )),
                    Err(why) => fail(&format!("served resource: {why}")),
                }
            }
            None => fail("resource read: no key reads"),
        }
        match host.content_hashes() {
            Ok(hashes) => match hashes.iter().find_map(|&hash| host.game_content(hash)) {
                Some(bytes) => pass(&format!(
                    "contents: {} hashes, one read of {} bytes",
                    hashes.len(),
                    bytes.len()
                )),
                None => fail(&format!("contents: {} hashes, none reads", hashes.len())),
            },
            Err(why) => fail(&format!("content hashes: {why}")),
        }
    }

    fn world(&self) {
        let Some((entity, at)) = self
            .world
            .entities(self.world.transform_type())
            .first()
            .map(|placed| (placed.entity, placed.position))
        else {
            enshwick::info!("SKIP world reads: no placed entity");
            return;
        };
        if self.world.set_label(entity, "probe") && self.world.set_label(entity, "") {
            pass(&format!("label set on entity {} and removed", entity.0));
        } else {
            fail(&format!("label on entity {}: refused", entity.0));
        }
        let [x, y, z] = at;
        report(
            "a save keeps the box at the first entity",
            self.world
                .persists([x - 1.0, y - 1.0, z - 1.0], [x + 1.0, y + 1.0, z + 1.0]),
        );
        match VoxelPos::from_metres(at) {
            Some(voxel) => report(
                "voxel sample at the first entity",
                self.voxels.sample(voxel),
            ),
            None => fail(&format!("voxel position of {at:?}: outside the world")),
        }
    }

    fn altars(&self) {
        let altars = self.altars.list();
        pass(&format!("altars: {}", altars.len()));
        match base_limit() {
            Some(limit) if limit.zones >= ZONES && limit.bases >= BASES => {
                pass(&format!("base limit {limit:?}"));
            }
            other => fail(&format!(
                "base limit {other:?}, asked {ZONES} zones in {BASES} bases"
            )),
        }
        enshwick::info!("grouping: {:?}", grouping());
        match set_base_cap(BASES) {
            Ok(()) if base_limit().is_some_and(|limit| limit.bases == BASES) => {
                pass(&format!("base cap {BASES} reads back"));
            }
            other => fail(&format!(
                "base cap {BASES}: {other:?}, reads {:?}",
                base_limit()
            )),
        }
        if altars.is_empty() {
            enshwick::info!("SKIP altar zone filter: no altar in the world");
            return;
        }
        let calls = Arc::clone(&self.zone_calls);
        let before = calls.load(Ordering::Relaxed);
        if let Err(why) = self.zones.changed() {
            fail(&format!("altar zones changed: {why}"));
            return;
        }
        let count = altars.len();
        self.scheduler.after(COUNT_FOR, move || {
            let seen = calls.load(Ordering::Relaxed) - before;
            if seen >= count {
                pass(&format!(
                    "altar zone filter: {seen} calls for {count} altars"
                ));
            } else {
                fail(&format!(
                    "altar zone filter: {seen} calls for {count} altars"
                ));
            }
        });
    }

    fn map(&self) {
        map_reads(self.map, self.scheduler, MAP_TRIES);
    }
}

/// How often the map is asked again while the loader still reads the scene.
const MAP_TRIES: u32 = 12;

/// Reads the map's frame and markers, asking again after [`COUNT_FOR`]
/// while the loader still reads the scene, at most `tries` times.
fn map_reads(map: Map, scheduler: Scheduler, tries: u32) {
    match map.frame() {
        Err(WorldError::NotLoaded) if tries > 1 => {
            scheduler.after(COUNT_FOR, move || map_reads(map, scheduler, tries - 1));
        }
        frame => {
            report("map frame", frame);
            report("map markers", map.markers().map(|markers| markers.len()));
        }
    }
}
