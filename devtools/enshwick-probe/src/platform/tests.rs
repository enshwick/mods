use std::sync::Mutex;

use enshwick::abi::{MapFrameInfo, Status};
use enshwick::test::{FakeHost, Installed};
use enshwick::{Mod, Off};

use super::*;

/// The checks the last init made; each test's init replaces them, so
/// tests that share a process under `cargo test` never read another's.
static CHECKS: Mutex<Option<PlatformChecks>> = Mutex::new(None);

struct Platform;

impl Mod for Platform {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let scheduler = load.scheduler()?;
        *CHECKS.lock().unwrap() = Some(PlatformChecks::new(load, scheduler));
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

fn started(host: FakeHost) -> (Installed, PlatformChecks) {
    let loader = host.install();
    // SAFETY: both functions are the exports of this module's `declare_mod!(Platform)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.faults());
    let checks = CHECKS.lock().unwrap().take().unwrap();
    (loader, checks)
}

fn logged_exactly(loader: &Installed, line: &str) -> bool {
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
    report_found::<&str>("found", Ok(true));
    report_found::<&str>("absent", Ok(false));
    report_found("broken", Err("refused"));
    assert!(logged_exactly(&loader, "PASS found"), "{:?}", loader.logs());
    assert!(logged_exactly(
        &loader,
        "FAIL absent: not found on this build"
    ));
    assert!(logged_exactly(&loader, "FAIL broken: refused"));
}

#[test]
fn a_capability_promised_unlinked_passes_only_while_unlinked() {
    let (loader, _) = started(FakeHost::new().world());
    report_linked::<&str>("unlinked", Ok(false), false);
    report_linked::<&str>("linked", Ok(true), false);
    assert!(
        logged_exactly(&loader, "PASS unlinked"),
        "{:?}",
        loader.logs()
    );
    assert!(logged_exactly(&loader, "FAIL linked: linked on this side"));
}

#[test]
fn a_dedicated_server_expects_the_task_worker_caps_linked() {
    let (loader, _) = started(FakeHost::new().world());
    assert!(
        logged_exactly(&loader, "PASS task worker caps found"),
        "{:?}",
        loader.logs()
    );
}

#[test]
fn a_client_expects_the_task_worker_caps_unlinked() {
    let (loader, _) = started(FakeHost::new().side(Side::CLIENT).world());
    assert!(
        logged_exactly(
            &loader,
            "PASS task worker caps unlinked on a client, as promised"
        ),
        "{:?}",
        loader.logs()
    );
}

/// Bytes standing in for the game's code section, with a run of padding.
static CODE: [u8; 64] =
    *b"0123456789abcdef\xcc\xcc\xcc\xcc\xcc\xcc\xcc\xccopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/";
/// The same section with no run of padding long enough for the patch.
static NO_PADDING: [u8; 64] =
    *b"0123456789abcdef\xcc\xcc\xcc\xcc\xcc\xcc\xccnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/";
/// Where the padding starts in [`CODE`].
const PADDING_AT: usize = 16;
/// Where the counted system starts in [`CODE`].
const COUNTED_AT: usize = 32;

fn code_at(offset: usize) -> u64 {
    u64::try_from(CODE.as_ptr().addr() + offset).unwrap()
}

/// A host with every part the checks need.
fn every_part() -> FakeHost {
    every_part_but_inventories().inventory_requests()
}

/// A host with every part the checks need but inventory batches.
fn every_part_but_inventories() -> FakeHost {
    FakeHost::new()
        .world()
        .items()
        .workshops()
        .vanilla_items(&[(CHEST_GUID, writes::CHEST)])
        .code(&CODE)
        .system(writes::COUNTED, code_at(COUNTED_AT))
        .server_world(0x7ff0_0000)
}

#[test]
fn the_patch_asks_to_write_over_padding_and_the_hook_counts_its_hits() {
    let (loader, checks) = started(every_part());
    assert_eq!(
        loader.code_patches(),
        vec![(
            code_at(PADDING_AT),
            vec![writes::PADDING; writes::PATCH_LEN],
            writes::WRITTEN.to_vec()
        )]
    );
    assert_eq!(checks.writes.hits(), Some(0));
    let mut registers = enshwick::abi::Registers::default();
    assert_eq!(loader.run_at(code_at(COUNTED_AT), &mut registers), 1);
    assert_eq!(checks.writes.hits(), Some(1));
    checks.run();
    assert!(
        logged_exactly(&loader, "PASS server world: set while the world runs"),
        "{:?}",
        loader.logs()
    );
}

#[test]
fn a_patch_the_loader_never_wrote_fails_its_check() {
    let (loader, checks) = started(every_part());
    checks.run();
    assert!(
        logged_starting(
            &loader,
            &format!(
                "FAIL code patch: padding at {:#x} reads [cc",
                code_at(PADDING_AT)
            )
        ),
        "{:?}",
        loader.logs()
    );
}

#[test]
fn a_host_without_inventory_batches_still_runs_the_transaction_check() {
    let (loader, checks) = started(every_part_but_inventories());
    checks.run();
    assert!(
        !logged_exactly(&loader, "SKIP platform: world writes is off")
            && logged_exactly(&loader, "SKIP transaction: no placed entity"),
        "{:?}",
        loader.logs()
    );
}

/// The GUID the fake's vanilla items give [`writes::CHEST`].
const CHEST_GUID: enshwick::content::Guid =
    enshwick::content::Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// The chest the transaction check places, as the fake knows it.
const CHEST: enshwick::ItemId = enshwick::ItemId::from_guid(CHEST_GUID);

/// The write checks started with a placed entity for the chest to go
/// above and the chest placeable, run once.
fn placing() -> Installed {
    use enshwick::content::EngineFacts as _;
    let host = every_part_but_inventories()
        .item(CHEST, 1, 0)
        .item_places(CHEST, enshwick::content::Guid([0x3a; 16]));
    let transform = host
        .hash(enshwick::test::facts::CURRENT_TRANSFORM.id())
        .unwrap();
    let host = host.placed(
        transform,
        enshwick::Entity::from_engine(40),
        [50.0, 40.0, 50.0],
    );
    let (loader, checks) = started(host);
    loader.set_terrain(&[]);
    checks.writes.run();
    loader
}

/// The id of the transaction that placed the chest, the only one the
/// write checks commit.
fn placed(loader: &Installed) -> u64 {
    let transactions = loader.transactions();
    assert_eq!(transactions.len(), 1, "{:?}", loader.logs());
    transactions[0].id
}

/// Drives the fake server until a line starting with `start` is logged,
/// for at most twenty seconds of its clock: twice the longest an undo's
/// looks may take.
fn drive_until(loader: &Installed, start: &str) -> bool {
    loader.drive_until(Duration::from_secs(20), || logged_starting(loader, start))
}

#[test]
fn a_chest_that_stands_passes_the_transaction_check_and_is_undone() {
    let loader = placing();
    assert!(
        drive_until(
            &loader,
            "PASS transaction status Applied, its entities name chest"
        ),
        "{:?}",
        loader.logs()
    );
    assert!(
        drive_until(
            &loader,
            "PASS transaction undo: status Undone, the chest gone"
        ),
        "{:?}",
        loader.logs()
    );
}

#[test]
fn a_chest_whose_undo_is_refused_fails_the_undo_check() {
    let loader = placing();
    assert!(
        drive_until(
            &loader,
            "PASS transaction status Applied, its entities name chest"
        ),
        "{:?}",
        loader.logs()
    );
    loader.fail_transaction(placed(&loader), Status::REFUSED);
    assert!(
        drive_until(&loader, "FAIL transaction undo: status Some(UndoRefused("),
        "{:?}",
        loader.logs()
    );
}

#[test]
fn a_chest_whose_placement_fails_fails_the_transaction_check() {
    let loader = placing();
    loader.fail_transaction(placed(&loader), Status::NOT_LOADED);
    assert!(
        drive_until(&loader, "FAIL transaction status Some(Failed("),
        "{:?}",
        loader.logs()
    );
    assert!(
        !logged_starting(&loader, "PASS transaction undo")
            && !logged_starting(&loader, "FAIL transaction undo"),
        "a placement that never applied leaves nothing to undo: {:?}",
        loader.logs()
    );
}

#[test]
fn a_code_section_without_padding_turns_off_only_the_patch() {
    let (loader, _) = started(every_part().code(&NO_PADDING));
    assert!(loader.code_patches().is_empty());
    assert!(
        logged_starting(&loader, "FAIL platform: code patch: no 8 bytes of padding"),
        "{:?}",
        loader.logs()
    );
    assert_eq!(loader.mid_hooks(), vec![code_at(COUNTED_AT)]);
}

#[test]
fn no_server_world_fails_its_check() {
    let (loader, checks) = started(every_part().server_world(0));
    checks.run();
    assert!(
        logged_starting(&loader, "FAIL server world: 0x0"),
        "{:?}",
        loader.logs()
    );
}

#[test]
fn a_run_with_every_part_runs_the_checks_of_each() {
    let (loader, checks) = started(every_part());
    assert!(checks.world.is_some() && checks.map.is_some());
    assert!(checks.altars.is_some() && checks.zones.is_some());
    checks.run();
    let logs = loader.logs();
    assert!(
        logged_exactly(&loader, "SKIP world reads: no placed entity"),
        "{logs:?}"
    );
    assert!(logged_starting(&loader, "PASS altars: "), "{logs:?}");
    assert!(logged_starting(&loader, "PASS base cap "), "{logs:?}");
    assert!(!logged_starting(&loader, "SKIP platform: "), "{logs:?}");
}

#[test]
fn a_missing_world_turns_off_only_the_checks_that_need_it() {
    let (loader, checks) = started(
        FakeHost::new()
            .world()
            .without_fact(enshwick::test::facts::CURRENT_TRANSFORM),
    );
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
    assert!(logged_exactly(&loader, "SKIP platform: world is off"));
    assert!(logged_exactly(&loader, "SKIP platform: map is off"));
}

#[test]
fn the_map_is_read_again_while_the_loader_reads_the_scene() {
    let frame = enshwick::abi::headed!(MapFrameInfo {
        size: [10240.0; 2],
        metres_per_cell: 8.0,
        ..MapFrameInfo::EMPTY
    });
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
        logged_starting(&loader, "PASS base limit BaseLimit "),
        "{:?}",
        loader.logs()
    );
    assert!(
        logged_exactly(
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

#[test]
fn a_build_without_bases_past_16_fails_the_limit_and_skips_the_cap() {
    let (loader, _) = started(FakeHost::new().world().without_bases_past_16());
    bases();
    assert!(
        logged_starting(&loader, "FAIL base limit "),
        "{:?}",
        loader.logs()
    );
    assert!(logged_starting(&loader, &format!("SKIP base cap {CAP}: ")));
}

#[test]
fn a_build_without_the_altar_code_skips_the_altars_and_still_checks_the_bases() {
    let (loader, checks) = started(FakeHost::new().world().without_altar_code());
    let logs = loader.logs();
    assert!(
        logged_starting(&loader, "FAIL platform: altars: "),
        "{logs:?}"
    );
    assert!(
        logged_starting(&loader, "FAIL platform: altar zone filter: "),
        "{logs:?}"
    );
    assert!(checks.altars.is_none() && checks.zones.is_none());
    assert!(checks.world.is_some() && checks.map.is_some());
    checks.run();
    let logs = loader.logs();
    assert!(
        logged_exactly(&loader, "SKIP platform: altars is off"),
        "{logs:?}"
    );
    assert!(
        logged_starting(&loader, "PASS base limit BaseLimit "),
        "{logs:?}"
    );
    assert!(
        logged_exactly(
            &loader,
            &format!("PASS base cap {CAP} reads back, and {BASES} once let go")
        ),
        "{logs:?}"
    );
}
