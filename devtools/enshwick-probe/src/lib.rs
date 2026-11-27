//! Development only, never shipped: exercises the platform API on a test
//! server without a player and logs what it sees to
//! `logs/enshwick.probe_<date>_<time>.log`, each check as a `PASS`, `FAIL`
//! or `SKIP` line.
//!
//! Off until `enabled = true` in `settings.toml` in its own folder in `mods/`;
//! read at start. While on, it counts every game event by type and logs the counts
//! every minute, logs each typed game event it receives, and runs its checks
//! once the world has loaded: world key, entity queries, stats, component
//! writes, actions, spawns, a per-world store and a save on demand. Its
//! actions, for administrators on Shift and F2 to F6, F9 and F11 or as
//! console lines, run them again and try single calls.

mod affixes;
mod buffs;
mod census;
mod chat;
mod checks;
mod client;
mod clock;
mod containers;
mod damage;
mod knowledge;
mod notifiers;
mod platform;
mod raw_stats;
mod respawns;
mod saves;
mod scenarios;
mod settings;
mod stats;
mod terrain;
mod ticks;
mod weather;

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick::Entity;
use enshwick::actions::{Action, Actions, Arg, Outcome, Value};
use enshwick::controls::Key;
use enshwick::events::{Kind, WorldLoaded, WorldSaved};
use enshwick::game_events::{
    AcceptOfferedInteractionEvent, BuildingPickUpEvent, BuildingPlaceEvent, BuildingTearDownEvent,
    CraftRecipeEvent, CraftRecipeSuccessEvent, EntityDiedEvent, FlameAltarEvent,
    HandleEquipmentSlotChangedEvent, HitEvent, LevelUpEvent, LootInteractionEvent,
    PlayerSpawnInfoChangeEvent, RevivedPlayerEvent, XpGainedEvent,
};
use enshwick::i18n::Texts;
use enshwick::schedule::Scheduler;
use enshwick::{Load, Mod, Off};

use crate::census::Census;
use crate::checks::Checks;

/// How often the census is logged.
const CENSUS_PERIOD: Duration = Duration::from_secs(60);
/// How long after the world loads the checks run.
const CHECK_DELAY: Duration = Duration::from_secs(10);
/// The text `notice` shows.
const NOTICE: &str = "Probe notice";
/// How many notices `notices` sends at once: four past the 16 a client
/// queues for its next steps, so the burst overflows that queue.
const NOTICES: u32 = 20;

/// Probe settings, read at start.
#[enshwick::config]
#[derive(Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches of a development mod's settings file"
)]
struct Settings {
    /// Exercise the platform API and log what it sees; development only.
    enabled: bool,
    /// Refuse every craft request, to see the craft filter in game.
    refuse_crafting: bool,
    /// Refuse every placement, to see the build filter in game.
    refuse_building: bool,
    /// Halve every damage, to see the damage filter in game.
    soften_damage: bool,
    /// Keep the bosses' health at 1.25 for the next start, to see a kept
    /// game setting come back with the world; the next run with this on
    /// forgets it. Off, a run keeps and forgets it at once.
    keep_across_restart: bool,
    /// Run none of the checks when the world loads, only the scenario the
    /// world's settings name, so the checks' weather and clock leave it be.
    scenario_only: bool,
}

/// A check of a single feature, run after the general ones.
pub(crate) type Check = Box<dyn Fn() + Send + Sync>;

/// The checks of single features whose part of the API this build has.
fn extras(load: &mut Load<'_>, scheduler: Scheduler, settings: &Settings) -> Vec<Check> {
    let mut extras: Vec<Check> = Vec::new();
    match clock::ClockChecks::new(load, scheduler) {
        Ok(clock) => extras.push(Box::new(move || clock.run())),
        Err(off) => enshwick::warn!("FAIL clock: {off}"),
    }
    match settings::SettingsChecks::new(load, settings.keep_across_restart) {
        Ok(settings) => extras.push(Box::new(move || settings.run())),
        Err(off) => enshwick::warn!("FAIL game settings: {off}"),
    }
    match buffs::BuffChecks::new(load, scheduler) {
        Ok(buffs) => extras.push(Box::new(move || buffs.run())),
        Err(off) => enshwick::warn!("FAIL buffs: {off}"),
    }
    match knowledge::KnowledgeChecks::new(load, scheduler) {
        Ok(knowledge) => extras.push(Box::new(move || knowledge.run())),
        Err(off) => enshwick::warn!("FAIL knowledge: {off}"),
    }
    match weather::WeatherChecks::new(load, scheduler) {
        Ok(weather) => extras.push(Box::new(move || weather.run())),
        Err(off) => enshwick::warn!("FAIL weather: {off}"),
    }
    match stats::StatChecks::new(load, scheduler) {
        Ok(stats) => extras.push(Box::new(move || stats.run())),
        Err(off) => enshwick::warn!("FAIL stats: {off}"),
    }
    match ticks::TickChecks::new(load, scheduler) {
        Ok(ticks) => extras.push(Box::new(move || ticks.run())),
        Err(off) => enshwick::warn!("FAIL ticks: {off}"),
    }
    match terrain::TerrainChecks::new(load) {
        Ok(terrain) => extras.push(Box::new(move || terrain.run())),
        Err(off) => enshwick::warn!("FAIL terrain: {off}"),
    }
    match saves::SaveChecks::new(load, scheduler) {
        Ok(saves) => extras.push(Box::new(move || saves.run())),
        Err(off) => enshwick::warn!("FAIL saves: {off}"),
    }
    match containers::ContainerChecks::new(load) {
        Ok(containers) => extras.push(Box::new(move || containers.run())),
        Err(off) => enshwick::warn!("FAIL containers: {off}"),
    }
    match notifiers::NotifierChecks::new(load, scheduler) {
        Ok(notifiers) => extras.push(Box::new(move || notifiers.run())),
        Err(off) => enshwick::warn!("FAIL notifiers: {off}"),
    }
    match client::ClientChecks::new(load) {
        Ok(client) => extras.push(Box::new(move || client.run())),
        Err(off) => enshwick::warn!("FAIL client: {off}"),
    }
    extras.push(Box::new(client::loader_off));
    // Its hooks size altar zones and raise the base limits, so a run that
    // leaves the world be goes without it.
    if settings.scenario_only {
        enshwick::info!("SKIP platform: the run plays only its scenario");
    } else {
        let platform = platform::PlatformChecks::new(load, scheduler);
        extras.push(Box::new(move || platform.run()));
    }
    extras
}

/// The probe.
#[derive(Debug)]
pub struct Probe;

/// The characters, survivors and animals in the world: what holds health
/// and buffs.
fn living(world: enshwick::world::World) -> Vec<Entity> {
    use enshwick::world::ContainerKinds;

    checks::listed(
        "containers",
        world.containers(
            ContainerKinds::CHARACTERS | ContainerKinds::NPCS | ContainerKinds::ANIMALS,
        ),
    )
    .into_iter()
    .map(|placed| placed.entity)
    .collect()
}

/// Logs every event of kind `E`, or that its fields are missing.
fn log_typed<E: Kind<Data: core::fmt::Debug>>(load: &mut Load<'_>) {
    let name = core::any::type_name::<E>();
    match load.on::<E>(|event| enshwick::info!("event: {event:?}")) {
        Ok(()) => enshwick::info!("PASS typed {name} resolves"),
        Err(off) => enshwick::warn!("FAIL typed {name}: {off}"),
    }
}

/// Logs the craft requests and the first placement the filters see, and
/// refuses them when the settings say so.
fn vetoes(load: &mut Load<'_>, settings: &Settings) {
    let refuse_crafting = settings.refuse_crafting;
    let crafts = load.on_craft(move |craft| {
        enshwick::info!("craft filter: {craft:?}, refused: {refuse_crafting}");
        !refuse_crafting
    });
    match crafts {
        Ok(()) => enshwick::info!("PASS craft filter registered"),
        Err(off) => enshwick::warn!("FAIL craft filter: {off}"),
    }
    let refuse_building = settings.refuse_building;
    let seen = AtomicBool::new(false);
    let builds = load.on_build(move |placement| {
        if !seen.swap(true, Ordering::Relaxed) {
            enshwick::info!("build filter, first call: {placement:?}, refused: {refuse_building}");
        }
        !refuse_building
    });
    match builds {
        Ok(()) => enshwick::info!("PASS build filter registered"),
        Err(off) => enshwick::warn!("FAIL build filter: {off}"),
    }
    let renames = load.on_rename(|rename| {
        let allowed = rename_allowed(&rename.text);
        enshwick::info!(
            "PASS rename filter: entity {} to {:?} by {:?}, refused: {}",
            rename.entity.get(),
            rename.text,
            rename.player.as_ref().map(|player| player.entity),
            !allowed
        );
        allowed
    });
    match renames {
        Ok(()) => enshwick::info!("PASS rename filter registered"),
        Err(off) => enshwick::warn!("FAIL rename filter: {off}"),
    }
}

/// The one name the probe's rename filter refuses, so a player sees a
/// refusal the game's own way.
const REFUSED_NAME: &str = "probe refuses";

/// Whether the rename filter lets a player give an entity `text`.
fn rename_allowed(text: &str) -> bool {
    text != REFUSED_NAME
}

/// The entity a number argument names, which the platform kept within its
/// range; `None` for 0, the engine's no entity.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the argument's range is within u32"
)]
const fn entity_of(value: f64) -> Option<Entity> {
    Entity::from_raw(value as u32)
}

/// The answer of an action whose one argument is an entity: `then`'s text
/// for the entity, a refusal for 0 or any other arguments.
fn on_entity(args: &[Value], then: impl FnOnce(Entity) -> String) -> Outcome {
    match args {
        [Value::Number(n)] => entity_of(*n).map_or_else(
            || Outcome::refused("0 is no entity"),
            |entity| Outcome::done(then(entity)),
        ),
        _ => Outcome::refused(""),
    }
}

/// The keys of the actions, as action and default key: Shift with F2 to F6
/// and F11, Ctrl+Shift on F4 and F5 for the respawn point beside the key
/// and vitals, on F6 for the burst beside the single notice and on F9,
/// where Admin Tools' panel takes Shift+F9. The set is clear of the
/// game's F1 (its tutorial), the research mod's F7 and F8 and the test UI's
/// Shift+F10. The last two need the affix checks.
const CONTROLS: [(&str, &str); 10] = [
    ("census", "Shift+F2"),
    ("check", "Shift+F3"),
    ("key", "Shift+F4"),
    ("respawn", "Ctrl+Shift+F4"),
    ("set-respawn", "Ctrl+Shift+F5"),
    ("my-vitals", "Shift+F5"),
    ("notice", "Shift+F6"),
    ("notices", "Ctrl+Shift+F6"),
    ("stats", "Ctrl+Shift+F9"),
    ("affix", "Shift+F11"),
];

/// Declares the probe's actions, for administrators: those without arguments
/// on the keys of [`CONTROLS`], those with an entity as console lines.
fn actions(
    load: &mut Load<'_>,
    census: Arc<Census>,
    checks: &Arc<Checks>,
    run_all: Arc<dyn Fn() + Send + Sync>,
    affix_checks: Option<Arc<affixes::AffixChecks>>,
) -> Result<Actions, Off> {
    let raw_stats = raw_stats::RawStats::new(load)
        .inspect_err(|off| enshwick::warn!("FAIL raw stats: {off}"))
        .ok();
    let actions = load.actions(Texts::default())?;
    let admin = |id: &str, label: &str| Action::new(id, label).admins();
    let entity = || Arg::number("Entity", 1.0..=f64::from(u32::MAX), 1.0);

    actions.add(admin("census", "Game events counted"), move |_| {
        Outcome::done(census.lines().join("\n"))
    })?;
    actions.add(admin("check", "Run the checks again"), move |_| {
        run_all();
        Outcome::done("checks run; see probe.log")
    })?;
    actions.add(admin("key", "Your account key"), |request| {
        Outcome::done(format!("your account key: {:016x}", request.player.key))
    })?;
    let mine = Arc::clone(checks);
    actions.add(admin("my-vitals", "Your raw stats"), move |request| {
        let me = request.player.entity;
        Outcome::done(match &raw_stats {
            Some(raw) => raw.dump(me, &[]),
            None => mine.vitals_of(me),
        })
    })?;
    let vitals = Arc::clone(checks);
    actions.add(
        admin("vitals", "An entity's vitals").arg(entity()),
        move |request| on_entity(request.args, |entity| vitals.vitals_of(entity)),
    )?;
    let destroying = Arc::clone(checks);
    actions.add(
        admin("destroy", "Destroy an entity").arg(entity()),
        move |request| on_entity(request.args, |entity| destroying.destroy(entity)),
    )?;
    let spawning = Arc::clone(checks);
    actions.add(
        admin("spawn", "Spawn a copy of an entity").arg(entity()),
        move |request| on_entity(request.args, |entity| spawning.spawn_copy(entity)),
    )?;
    let respawns = Arc::clone(checks);
    actions.add(admin("respawn", "Your respawn point"), move |request| {
        respawns.respawns.show(request.player)
    })?;
    let moving = Arc::clone(checks);
    actions.add(
        admin("set-respawn", "Respawn at the point last shown"),
        move |request| moving.respawns.set(request.player),
    )?;
    let noticing = Arc::clone(checks);
    actions.add(admin("notice", "Show yourself a notice"), move |request| {
        Outcome::done(noticing.notice(request.player, NOTICE))
    })?;
    let bursting = Arc::clone(checks);
    actions.add(
        admin("notices", "Show yourself numbered notices at once"),
        move |request| {
            bursting.notices(request.player);
            Outcome::done("")
        },
    )?;
    if let Some(affix_checks) = affix_checks {
        let rolling = Arc::clone(&affix_checks);
        actions.add(admin("stats", "Your changed stats"), move |request| {
            Outcome::done(affix_checks.stats_of(request.player))
        })?;
        actions.add(admin("affix", "Roll affixes again"), move |request| {
            Outcome::done(rolling.reroll(request.player))
        })?;
    }
    Ok(actions)
}

/// The controls the probe declares: every one of [`CONTROLS`] with the affix
/// checks, else all but the last two.
fn keyed(affixes: bool) -> &'static [(&'static str, &'static str)] {
    let count = if affixes {
        CONTROLS.len()
    } else {
        CONTROLS.len() - 2
    };
    &CONTROLS[..count]
}

/// Puts the actions of [`CONTROLS`] on their keys, the affix ones only with
/// `affixes`; each key is a feature of its own, so a refused one leaves the
/// rest. Declared on every side: a dedicated server offers them to the
/// clients that join, and a client that hosts reads them itself.
fn controls(load: &mut Load<'_>, actions: &Actions, affixes: bool) {
    for (name, key) in keyed(affixes) {
        let _keyed = load.feature(name, |_| actions.control(name, Key::parse(key)?));
    }
}

/// This machine's switches, `settings.toml` in the mod's own folder, which the
/// mod reads itself, beside the world setting of its scenarios. The defaults
/// without the file, or with a file that does not read.
fn own_settings<T: enshwick::config::Config>(load: &Load<'_>) -> T {
    let Ok(path) = load.mod_dir().map(|dir| dir.join("settings.toml")) else {
        return T::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        enshwick::warn!("{}: {e}; the defaults apply", path.display());
        T::default()
    })
}

impl Mod for Probe {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = own_settings(load);
        if !settings.enabled {
            return Ok(Self);
        }
        let census = Arc::new(Census::new(load));
        let counted = Arc::clone(&census);
        load.on_every_raw_game_event(move |event| counted.count(event))?;
        log_typed::<HitEvent>(load);
        log_typed::<EntityDiedEvent>(load);
        log_typed::<LevelUpEvent>(load);
        log_typed::<XpGainedEvent>(load);
        log_typed::<CraftRecipeEvent>(load);
        log_typed::<CraftRecipeSuccessEvent>(load);
        log_typed::<AcceptOfferedInteractionEvent>(load);
        log_typed::<BuildingPlaceEvent>(load);
        log_typed::<BuildingTearDownEvent>(load);
        log_typed::<BuildingPickUpEvent>(load);
        log_typed::<FlameAltarEvent>(load);
        log_typed::<LootInteractionEvent>(load);
        log_typed::<HandleEquipmentSlotChangedEvent>(load);
        log_typed::<RevivedPlayerEvent>(load);
        log_typed::<PlayerSpawnInfoChangeEvent>(load);

        vetoes(load, &settings);
        // Before the chat filter, so the scenarios see the lines it drops.
        if let Err(off) = scenarios::start(load) {
            enshwick::warn!("FAIL scenarios: {off}");
        }
        chat::register(load);
        // The probe is a server mod, so the loader refuses it a channel.
        match load.channel::<u32, _>("probe", 1, |_, _| ()) {
            Err(off) => {
                enshwick::info!("PASS a channel is refused to a server mod: {off}");
            }
            Ok(_) => {
                enshwick::warn!("FAIL a channel opened for a server mod");
            }
        }
        let checks = Arc::new(Checks::new(load)?);
        let ping = load.game_event_type("ui_ping_event")?;
        let pinged = Arc::clone(&checks);
        load.on_raw_game_event(ping, move |event| pinged.pinged(event))?;
        let scheduler = load.scheduler()?;
        let damage = match damage::DamageSeen::register(load, settings.soften_damage) {
            Ok(seen) => {
                enshwick::info!("PASS damage filter registered");
                Some(seen)
            }
            Err(off) => {
                enshwick::warn!("FAIL damage filter: {off}");
                None
            }
        };
        let logged = Arc::clone(&census);
        scheduler.every(CENSUS_PERIOD, move || {
            for line in logged.lines() {
                enshwick::info!("census: {line}");
            }
            if let Some(damage) = &damage {
                enshwick::info!("{}", damage.line());
            }
            ControlFlow::Continue(())
        });
        let mut extras = extras(load, scheduler, &settings);
        let affix_checks = affixes::AffixChecks::register(load, &mut extras);
        let extras: Arc<[Check]> = extras.into();
        let run_all = {
            let checks = Arc::clone(&checks);
            move || {
                checks.run();
                for extra in extras.iter() {
                    extra();
                }
            }
        };
        let run_all: Arc<dyn Fn() + Send + Sync> = Arc::new(run_all);
        if !settings.scenario_only {
            let first = Arc::clone(&run_all);
            load.on::<WorldLoaded>(move |()| {
                let run = Arc::clone(&first);
                scheduler.after(CHECK_DELAY, move || run());
            })?;
        }
        let loaded = Arc::clone(&checks);
        load.on::<WorldLoaded>(move |()| loaded.respawns.forget())?;
        let saved = Arc::clone(&checks);
        load.on::<WorldSaved>(move |()| saved.saved())?;
        let affixes = affix_checks.is_some();
        let actions = actions(load, census, &checks, run_all, affix_checks)?;
        controls(load, &actions, affixes);
        enshwick::info!("probe on");
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

    use super::*;

    /// Starts the probe on a fake host with `settings` as the file in its
    /// own folder; the status and the host to look at.
    fn start(settings: &str) -> (Status, enshwick::test::Installed) {
        let loader = enshwick::test::FakeHost::new()
            .developer_tools(true)
            .mod_file("settings.toml", settings)
            .install();
        // SAFETY: the exports of the `declare_mod!` below.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        (status, loader)
    }

    #[test]
    fn the_rename_filter_refuses_only_its_one_name() {
        assert!(!rename_allowed(REFUSED_NAME));
        assert!(rename_allowed("Chest"));
        assert!(rename_allowed(""));
    }

    #[test]
    fn an_entity_action_refuses_entity_0_without_running() {
        let mut ran = false;
        let answer = on_entity(&[Value::Number(0.0)], |_| {
            ran = true;
            String::new()
        });
        assert_eq!(answer, Outcome::refused("0 is no entity"));
        assert!(!ran);
    }

    #[test]
    fn an_entity_action_runs_on_the_entity_its_argument_names() {
        let answer = on_entity(&[Value::Number(10.0)], |entity| entity.get().to_string());
        assert_eq!(answer, Outcome::done("10"));
        assert_eq!(on_entity(&[], |_| String::new()), Outcome::refused(""));
    }

    #[test]
    fn the_probe_is_a_developer_tool() {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let flags = unsafe { &*enshwick_mod_query() }.flags;
        assert!(flags.contains(enshwick::abi::ManifestFlags::DEVELOPER));
    }

    #[test]
    fn off_by_default_it_starts_and_registers_nothing() {
        let (status, loader) = start("");
        assert_eq!(
            status,
            Status::OK,
            "{:?} {:?}",
            loader.faults(),
            loader.logs()
        );
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        assert!(
            !loader.logs().iter().any(|(_, line)| line == "probe on"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn on_it_starts_or_turns_off_with_one_reason_and_never_panics() {
        let (status, loader) = start("enabled = true\n");
        let faults = loader.faults();
        let on = loader.logs().iter().any(|(_, line)| line == "probe on");
        if status == Status::OK {
            assert!(on, "{:?}", loader.logs());
            assert!(faults.is_empty(), "{faults:?}");
        } else {
            // The fake host lacks some game data the probe links: it turns
            // off with that reason, as on a game build without it.
            assert_eq!(status, Status::REFUSED);
            assert!(!on);
            assert_eq!(faults.len(), 1, "{faults:?}");
            assert!(!faults[0].contains("panic"), "{faults:?}");
        }
    }
}

#[cfg(test)]
mod keys {
    use std::time::Duration;

    use enshwick::abi::{Side, Status};
    use enshwick::actions::{Action, Outcome};
    use enshwick::controls::Key;
    use enshwick::i18n::Texts;
    use enshwick::{Load, Mod, Off};

    use super::CONTROLS;

    /// The probe's actions as stubs, on the probe's keys.
    struct Keys;

    impl Mod for Keys {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            for (name, _) in CONTROLS {
                actions.add(Action::new(name, name).admins(), |_| Outcome::done(""))?;
            }
            super::controls(load, &actions, true);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Keys,
        r#"
        id = "enshwick.probe"
        side = "server"
        "#
    );

    #[test]
    fn without_the_affix_checks_the_stats_and_affix_keys_are_left_out() {
        let names: Vec<&str> = super::keyed(false).iter().map(|c| c.0).collect();
        assert_eq!(
            names,
            [
                "census",
                "check",
                "key",
                "respawn",
                "set-respawn",
                "my-vitals",
                "notice",
                "notices"
            ]
        );
        assert_eq!(super::keyed(true), CONTROLS);
    }

    #[test]
    fn no_key_shares_a_key_with_the_game_tutorial_or_the_research_mod() {
        // F1 opens the game's tutorial, which a mod key never takes from the
        // game; F7 and F8 are the research mod's, which a Shift chord on the
        // same key would beat while the player sprints.
        let taken: Vec<u32> = ["F1", "F7", "F8"]
            .iter()
            .map(|key| Key::parse(key).unwrap().code())
            .collect();
        for (name, key) in CONTROLS {
            let code = Key::parse(key).unwrap().code();
            assert!(!taken.contains(&code), "{name} on {key}");
        }
    }

    #[test]
    fn no_two_keys_fire_together_when_all_their_modifiers_are_held() {
        // The loader fires every control on a key whose modifiers are held
        // and as many as the most specific one's, so two controls on one key
        // with equally many modifiers both fire while both chords are held.
        for (i, (name, key)) in CONTROLS.iter().enumerate() {
            let key = Key::parse(key).unwrap();
            for (other, other_key) in &CONTROLS[i + 1..] {
                let other_key = Key::parse(other_key).unwrap();
                assert!(
                    key.code() != other_key.code()
                        || key.modifiers().count_ones() != other_key.modifiers().count_ones(),
                    "{name} and {other}"
                );
            }
        }
    }

    /// The probe's stub keys started on a fake host of `side`.
    fn start(side: Side, hosting: bool) -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .side(side)
            .hosting(hosting)
            .install();
        // SAFETY: the exports of this module's `declare_mod!(Keys)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    #[test]
    fn a_dedicated_server_declares_every_key_for_the_clients_that_join() {
        let loader = start(Side::SERVER, false);
        let declared: Vec<String> = loader.controls().into_iter().map(|c| c.0).collect();
        let names: Vec<&str> = CONTROLS.iter().map(|c| c.0).collect();
        assert_eq!(declared, names);
    }

    #[test]
    fn on_a_hosting_client_each_key_runs_its_action() {
        let loader = start(Side::CLIENT, true);
        assert_eq!(
            loader.controls().len(),
            CONTROLS.len(),
            "{:?}",
            loader.controls()
        );
        for (name, key) in CONTROLS {
            let key = Key::parse(key).unwrap();
            assert!(
                loader
                    .controls()
                    .iter()
                    .any(|(declared, _, keys)| declared == name && keys[0] == Some(key)),
                "{name}: {:?}",
                loader.controls()
            );
            assert_eq!(loader.keys(&[key], Duration::from_millis(16)), 1, "{name}");
            assert_eq!(loader.keys(&[], Duration::from_millis(16)), 0, "{name}");
            assert_eq!(
                loader.action_runs().last(),
                Some(&(name.to_owned(), String::new()))
            );
        }
    }
}

enshwick::declare_mod!(Probe);
