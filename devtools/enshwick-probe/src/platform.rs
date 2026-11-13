//! The platform's reads of the running game and its world that no other
//! check covers: reflection, the code section, the archive, the altars and
//! their bases, the map, the console and the loop's waits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use enshwick::abi::HookFlags;
use enshwick::{Next, hook};

use enshwick::altars::{Altars, ZoneSizes, base_limit, grouping, set_base_cap};
use enshwick::content::recipes::{WORKSHOP_RECIPES, workshop_limit};
use enshwick::effects::{Effects, Spawn};
use enshwick::map::Map;
use enshwick::pacing::TaskWorkers;
use enshwick::resources::Registry;
use enshwick::schedule::Scheduler;
use enshwick::state::ModLocal;
use enshwick::voxels::{VoxelPos, Voxels};
use enshwick::world::World;
use enshwick::{Load, WorldError};

use crate::checks::{fail, pass};

/// The field of the reflected type the probe looks up.
const SHAPE_FIELD: &str = "rangeX";
/// An ECS system every server build has.
const SYSTEM: &str = "add_buildzones_server";
/// A symbol the loader pins for every build.
const SYMBOL: &str = "npc.lookup";
/// Altar zones and bases the probe asks for: more than `raise_base_limit`'s
/// 32 zones in 16 bases, so the limit read shows the request took.
const ZONES: u32 = 40;
/// See [`ZONES`].
const BASES: u32 = 20;
/// The cap the probe sets for a moment, below its request.
const CAP: u32 = 18;
/// The console line the probe runs: its own action, as the console would.
const CONSOLE_LINE: &str = "enshwick.probe census";
/// How long the probe counts the loop's waits and the zone filter's calls.
const COUNT_FOR: Duration = Duration::from_secs(5);
/// How long a spawned entity may take to join the world, as the spawn
/// check waits.
const SPAWN_WAIT: Duration = Duration::from_secs(2);
/// How many archive keys and content hashes the probe tries to read; a
/// broken read then stays short instead of trying every key.
const READ_TRIES: usize = 64;

/// Calls of the hooked system in this mod's life.
static SYSTEM_RUNS: ModLocal<AtomicUsize> = ModLocal::new();
/// Whether a hooked call read a caller frame with a stack pointer since the
/// last count began.
static FRAME_SEEN: ModLocal<AtomicBool> = ModLocal::new();

/// Whether the base limit holds the probe's request: its zones, and bases
/// past 16 within reach (`max_bases` falls to 16 when they fail to arm).
/// The bases themselves may sit lower under another mod's cap.
fn limit_holds(zones: u32, max_bases: u32) -> bool {
    zones >= ZONES && max_bases >= BASES
}

/// Whether the probe's cap of [`CAP`] read back and letting go of it lifted
/// the bases above it again; `None` when the bases read before were at or
/// below the cap already, so a cap cannot show.
fn cap_reads_back(before: u32, capped: u32, released: u32) -> Option<bool> {
    (before > CAP).then_some(capped == CAP && released > CAP)
}

fn system_runs() -> &'static AtomicUsize {
    SYSTEM_RUNS.get_or_init(AtomicUsize::default)
}

fn frame_seen() -> &'static AtomicBool {
    FRAME_SEEN.get_or_init(AtomicBool::default)
}

/// Counts the system's runs and reads the caller's frame, then runs it.
#[hook(flags = HookFlags::CALLER_FRAME)]
fn system_update(next: Next<'_, unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    system_runs().fetch_add(1, Ordering::Relaxed);
    if enshwick::host().is_some_and(|host| host.caller_frame().rsp != 0) {
        frame_seen().store(true, Ordering::Relaxed);
    }
    // SAFETY: the rest of the chain takes the context the game passed.
    unsafe { next.call(ctx) }
}

/// Logs `PASS what: value` for `Ok`, `FAIL what: why` for `Err`.
fn report<T: core::fmt::Debug, E: core::fmt::Display>(what: &str, result: Result<T, E>) {
    match result {
        Ok(value) => pass(&format!("{what}: {value:?}")),
        Err(why) => fail(&format!("{what}: {why}")),
    }
}

/// Logs `PASS` for `Ok(true)`; `Ok(false)`, the loader not finding or
/// arming what the call needs on this build, is a `FAIL`.
fn report_found<E: core::fmt::Display>(what: &str, result: Result<bool, E>) {
    match result {
        Ok(true) => pass(what),
        Ok(false) => fail(&format!("{what}: not found on this build")),
        Err(why) => fail(&format!("{what}: {why}")),
    }
}

/// The part `what` some checks need, or `None` with a `FAIL` naming it, so
/// only those checks stay off.
fn part<T, E: core::fmt::Display>(what: &str, got: Result<T, E>) -> Option<T> {
    got.map_err(|why| fail(&format!("platform: {what}: {why}; its checks are off")))
        .ok()
}

/// The base limit the probe requested, and a cap below it that reads back
/// and is let go again.
fn bases() {
    let before = base_limit();
    match before {
        Some(limit) if limit_holds(limit.zones, limit.max_bases) => {
            pass(&format!("base limit {limit:?}"));
            if limit.bases < BASES {
                enshwick::info!(
                    "bases held at {} below the {BASES} asked, by another mod's lower cap",
                    limit.bases
                );
            }
        }
        other => fail(&format!(
            "base limit {other:?}, asked {ZONES} zones in {BASES} bases"
        )),
    }
    enshwick::info!("grouping: {:?}", grouping());
    // The lowest cap of every mod holds, so the probe lets go of its own at
    // once: other mods keep the bases they asked for.
    let capped = set_base_cap(CAP).map(|()| base_limit());
    let released = set_base_cap(u32::MAX).map(|()| base_limit());
    match (before, capped, released) {
        (Some(before), Ok(Some(low)), Ok(Some(back))) => {
            match cap_reads_back(before.bases, low.bases, back.bases) {
                Some(true) => pass(&format!(
                    "base cap {CAP} reads back, and {} once let go",
                    back.bases
                )),
                Some(false) => fail(&format!(
                    "base cap {CAP} read {}, then {} once let go, from {}",
                    low.bases, back.bases, before.bases
                )),
                None => enshwick::info!(
                    "SKIP base cap {CAP}: the bases are at {} already",
                    before.bases
                ),
            }
        }
        (_, capped, released) => fail(&format!(
            "base cap {CAP}, then let go: {capped:?}, {released:?}, from {before:?}"
        )),
    }
}

pub(crate) struct PlatformChecks {
    world: Option<World>,
    effects: Effects,
    voxels: Voxels,
    altars: Option<Altars>,
    zones: Option<ZoneSizes>,
    map: Option<Map>,
    scheduler: Scheduler,
    zone_calls: Arc<AtomicUsize>,
    loop_waits: Arc<AtomicUsize>,
    server_waits: Arc<AtomicUsize>,
}

impl PlatformChecks {
    /// Runs the checks that only init may make, and sets up the rest; a part
    /// the loader does not give turns off only the checks that need it.
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Self {
        let system = load.system(SYSTEM);
        report("reflection: system", system.as_ref().map(|_| ()));
        if let Ok(target) = system {
            // SAFETY: an ECS system's update function takes its query
            // context alone, as `SYSTEM_UPDATE` does.
            report("system hooked", unsafe {
                load.hook(&SYSTEM_UPDATE, target)
            });
        }
        // The smoke test checks that reflection resolves a type by the name
        // the engine gives it, as a mod's low-level code would.
        // ast-grep-ignore: engine-names-stay-in-loader
        let shape = "keen::ecs::TriggerShape";
        report(
            "reflection: field offset",
            load.field_offset(shape, SHAPE_FIELD),
        );
        report("reflection: type size", load.type_size(shape));
        report("symbol", load.symbol(SYMBOL).map(|_| ()));
        report("code section bytes", load.code().map(<[u8]>::len));
        report_found(
            "task worker caps found",
            load.cap_task_workers(TaskWorkers::at_most(u8::MAX)),
        );
        report_found("base limit raised", load.raise_base_limit());
        report_found(
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
        let zones = part(
            "altar zone filter",
            load.on_altar_zone(move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
        );
        Self {
            world: part("world", load.world()),
            effects: load.effects(),
            voxels: load.voxels(),
            altars: part("altars", load.altars()),
            zones,
            map: part("map", load.map()),
            scheduler,
            zone_calls,
            loop_waits,
            server_waits,
        }
    }

    /// Runs every check on the game thread.
    pub(crate) fn run(&self) {
        Self::host();
        Self::archive();
        if let Some(world) = self.world {
            self.world(world);
        }
        bases();
        if let Some(altars) = &self.altars {
            self.altars(altars);
        }
        if let Some(map) = self.map {
            map_reads(map, self.scheduler, MAP_TRIES);
        }
        report("console line", enshwick::console::run(CONSOLE_LINE));
        let (loops, servers) = (Arc::clone(&self.loop_waits), Arc::clone(&self.server_waits));
        let (loops_before, servers_before) = (
            loops.load(Ordering::Relaxed),
            servers.load(Ordering::Relaxed),
        );
        let runs_before = system_runs().load(Ordering::Relaxed);
        frame_seen().store(false, Ordering::Relaxed);
        self.scheduler.after(COUNT_FOR, move || {
            let runs = system_runs().load(Ordering::Relaxed) - runs_before;
            match (runs, frame_seen().load(Ordering::Relaxed)) {
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
        match workshop_limit() {
            usize::MAX => pass("workshop recipe limit lifted"),
            WORKSHOP_RECIPES => enshwick::info!(
                "SKIP workshop recipe limit: the game's own {WORKSHOP_RECIPES} holds, the lift is not armed"
            ),
            limit => fail(&format!("workshop recipe limit {limit}")),
        }
        // The smoke test checks that the reflected types name a type as the
        // engine does.
        // ast-grep-ignore: engine-names-stay-in-loader
        let shape = "keen::ecs::TriggerShape";
        match Registry::game() {
            Ok(registry) if registry.named(shape).is_some() => {
                pass(&format!("reflected types name {shape}"));
            }
            Ok(_) => fail(&format!("reflected types lack {shape}")),
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
            .take(READ_TRIES)
            .find_map(|&key| host.game_resource(key).map(|bytes| (key, bytes)));
        match read {
            Some((key, bytes)) => {
                pass(&format!("resource read: {} bytes", bytes.len()));
                match host.served_resource(key) {
                    Ok(served) if served == bytes => {
                        pass("served resource: reads as the archive's");
                    }
                    // A mod or the loader may serve this key changed.
                    Ok(served) => enshwick::info!(
                        "SKIP served resource: {} bytes, the archive's {}; the key is served changed",
                        served.len(),
                        bytes.len()
                    ),
                    Err(why) => fail(&format!("served resource: {why}")),
                }
            }
            None => fail(&format!("resource read: none of {READ_TRIES} keys reads")),
        }
        match host.content_hashes() {
            Ok(hashes) => match hashes
                .iter()
                .take(READ_TRIES)
                .find_map(|&hash| host.game_content(hash))
            {
                Some(bytes) => pass(&format!(
                    "contents: {} hashes, one read of {} bytes",
                    hashes.len(),
                    bytes.len()
                )),
                None => fail(&format!(
                    "contents: {} hashes, none of the first {READ_TRIES} reads",
                    hashes.len()
                )),
            },
            Err(why) => fail(&format!("content hashes: {why}")),
        }
    }

    fn world(&self, world: World) {
        let Some((entity, at)) = world
            .entities(world.transform_type())
            .first()
            .map(|placed| (placed.entity, placed.position))
        else {
            enshwick::info!("SKIP world reads: no placed entity");
            return;
        };
        let [x, y, z] = at;
        self.label_own_copy(world, entity, [x, y + 3.0, z]);
        report(
            "a save keeps the box at the first entity",
            world.persists([x - 1.0, y - 1.0, z - 1.0], [x + 1.0, y + 1.0, z + 1.0]),
        );
        match VoxelPos::from_metres(at) {
            Some(voxel) => report(
                "voxel sample at the first entity",
                self.voxels.sample(voxel),
            ),
            None => fail(&format!("voxel position of {at:?}: outside the world")),
        }
    }

    /// Labels a copy of `entity` the probe spawns at `at`, out of the save,
    /// then destroys it: a label is saved with the world and would replace
    /// a name the entity already had.
    fn label_own_copy(&self, world: World, entity: enshwick::Entity, at: [f64; 3]) {
        let Some(template) = world.template(entity) else {
            enshwick::info!("SKIP label: entity {} has no template", entity.0);
            return;
        };
        let copy = match self.effects.spawn(template, at, Spawn::new().do_not_save()) {
            Ok(copy) => copy,
            Err(why) => {
                fail(&format!("label: spawn of its own entity: {why}"));
                return;
            }
        };
        // A spawned entity joins the world with the game's next steps.
        self.scheduler.after(SPAWN_WAIT, move || {
            if world.set_label(copy, "probe") && world.set_label(copy, "") {
                pass(&format!("label set on entity {} and removed", copy.0));
            } else {
                fail(&format!("label on entity {}: refused", copy.0));
            }
            if let Err(why) = world.destroy(copy) {
                fail(&format!("label: destroy of entity {}: {why}", copy.0));
            }
        });
    }

    fn altars(&self, altars: &Altars) {
        let altars = altars.list();
        pass(&format!("altars: {}", altars.len()));
        if altars.is_empty() {
            enshwick::info!("SKIP altar zone filter: no altar in the world");
            return;
        }
        let Some(zones) = &self.zones else {
            return;
        };
        let calls = Arc::clone(&self.zone_calls);
        let before = calls.load(Ordering::Relaxed);
        if let Err(why) = zones.changed() {
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
}

/// How often the map is asked again while the loader still reads the scene.
const MAP_TRIES: u32 = 12;

/// Reads the map's frame and markers, asking again after [`COUNT_FOR`]
/// while the loader still reads the scene, at most `tries` times.
fn map_reads(map: Map, scheduler: Scheduler, tries: u32) {
    let (frame, markers) = (map.frame(), map.markers());
    let loading = matches!(frame, Err(WorldError::NotLoaded))
        || matches!(markers, Err(WorldError::NotLoaded));
    if loading && tries > 1 {
        scheduler.after(COUNT_FOR, move || map_reads(map, scheduler, tries - 1));
        return;
    }
    report("map frame", frame);
    report("map markers", markers.map(|markers| markers.len()));
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{MapFrameInfo, Status};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Mod, Off};

    use super::*;

    static CHECKS: OnceLock<PlatformChecks> = OnceLock::new();

    struct Platform;

    impl Mod for Platform {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            report_found::<&str>("found", Ok(true));
            report_found::<&str>("absent", Ok(false));
            report_found("broken", Err("refused"));
            let scheduler = load.scheduler()?;
            let _first = CHECKS.set(PlatformChecks::new(load, scheduler));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Platform,
        r#"
        id = "probe.platform"
        side = "server"
        "#
    );

    fn started(host: FakeHost) -> (Installed, &'static PlatformChecks) {
        let loader = host.install();
        // SAFETY: both functions are the exports of this module's `declare_mod!(Platform)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        (loader, CHECKS.get().unwrap())
    }

    fn logged(loader: &Installed, line: &str) -> bool {
        loader.logs().iter().any(|(_, logged)| logged == line)
    }

    fn logged_starting(loader: &Installed, start: &str) -> bool {
        loader
            .logs()
            .iter()
            .any(|(_, logged)| logged.starts_with(start))
    }

    #[test]
    fn only_a_found_capability_passes_and_one_not_found_fails() {
        let (loader, _) = started(FakeHost::new().world());
        assert!(logged(&loader, "PASS found"), "{:?}", loader.logs());
        assert!(logged(&loader, "FAIL absent: not found on this build"));
        assert!(logged(&loader, "FAIL broken: refused"));
    }

    #[test]
    fn a_missing_world_turns_off_only_the_checks_that_need_it() {
        let (loader, checks) =
            started(FakeHost::new().without_fact(enshwick::test::facts::CURRENT_TRANSFORM));
        assert!(
            logged_starting(&loader, "FAIL platform: world: "),
            "{:?}",
            loader.logs()
        );
        assert!(logged_starting(&loader, "FAIL platform: map: "));
        assert!(checks.world.is_none() && checks.map.is_none());
        assert!(checks.altars.is_some() && checks.zones.is_some());
        checks.run();
        assert!(
            logged_starting(&loader, "PASS altars: "),
            "{:?}",
            loader.logs()
        );
        assert!(logged_starting(&loader, "PASS base cap "));
    }

    #[test]
    fn the_map_is_read_again_while_the_loader_reads_the_scene() {
        let frame = MapFrameInfo {
            size: [10240.0; 2],
            metres_per_cell: 8.0,
            ..MapFrameInfo::EMPTY
        };
        let (loader, checks) = started(FakeHost::new().world().map_frame(frame));
        let map = checks.map.unwrap();
        // The loader answers a world's first read of the map that it still
        // reads the scene. The scheduler waits on the wall clock, so the
        // test makes the retry's call itself.
        map_reads(map, checks.scheduler, MAP_TRIES);
        assert!(
            !logged_starting(&loader, "PASS map") && !logged_starting(&loader, "FAIL map"),
            "{:?}",
            loader.logs()
        );
        map_reads(map, checks.scheduler, MAP_TRIES - 1);
        assert!(
            logged_starting(&loader, "PASS map frame"),
            "{:?}",
            loader.logs()
        );
        assert!(logged_starting(&loader, "PASS map markers"));
    }

    #[test]
    fn a_map_still_loading_after_the_last_try_fails() {
        let (loader, checks) = started(FakeHost::new().world());
        loader.read_map_markers(true);
        map_reads(checks.map.unwrap(), checks.scheduler, 1);
        assert!(
            logged_starting(&loader, "FAIL map markers: "),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn the_base_cap_is_let_go_back_to_the_request() {
        let (loader, _) = started(FakeHost::new().world());
        assert_eq!(loader.requested_base_limits(), Some((ZONES, BASES)));
        bases();
        assert!(
            logged(
                &loader,
                &format!("PASS base cap {CAP} reads back, and {BASES} once let go")
            ),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn the_limit_holds_while_bases_past_16_are_within_reach() {
        assert!(limit_holds(ZONES, 179));
        assert!(limit_holds(ZONES, BASES));
    }

    #[test]
    fn bases_past_16_out_of_reach_fail_the_limit() {
        assert!(!limit_holds(ZONES, 16));
        assert!(!limit_holds(ZONES - 1, 179));
    }

    #[test]
    fn alone_the_cap_reads_back_and_letting_go_lifts_the_bases() {
        assert_eq!(cap_reads_back(BASES, CAP, BASES), Some(true));
    }

    #[test]
    fn under_another_mods_cap_above_the_probes_the_cap_still_shows() {
        assert_eq!(cap_reads_back(CAP + 1, CAP, CAP + 1), Some(true));
    }

    #[test]
    fn bases_already_at_or_below_the_cap_skip_the_cap_check() {
        assert_eq!(cap_reads_back(16, 16, 16), None);
        assert_eq!(cap_reads_back(CAP, CAP, CAP), None);
    }

    #[test]
    fn a_cap_that_does_not_hold_or_does_not_let_go_fails() {
        assert_eq!(cap_reads_back(BASES, BASES, BASES), Some(false));
        assert_eq!(cap_reads_back(BASES, CAP, CAP), Some(false));
    }
}
