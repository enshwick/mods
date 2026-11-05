//! Check scenarios in Luau: the probe's own VM runs the one the world's
//! `scenario` setting names once the world has loaded, and logs a line per
//! step and `PASS` or `FAIL` at the end.

use core::fmt::Write as _;

use enshwick::config::{Field, Schema};
use enshwick::{Load, Off};
use enshwick_luau::{Limits, Scripts};

/// The id the scripts run under, the probe's own.
const ID: &str = "enshwick.probe";
/// The world setting that names the scenario to run.
const SETTING: &str = "scenario";
/// The runner, the VM's `main.luau`.
const RUNNER: &str = include_str!("../scenarios/runner.luau");
/// The module the runner requires: every scenario by its name.
const INDEX: &str = "index.luau";

/// Every scenario, by the name the setting takes, with its source; a test
/// holds this list to the files in `scenarios/`.
pub(crate) const SCENARIOS: [(&str, &str); 6] = [
    ("chat", include_str!("../scenarios/chat.luau")),
    ("saves", include_str!("../scenarios/saves.luau")),
    ("weather", include_str!("../scenarios/weather.luau")),
    (
        "item-actions",
        include_str!("../scenarios/item-actions.luau"),
    ),
    (
        "craft-rewards",
        include_str!("../scenarios/craft-rewards.luau"),
    ),
    ("join-throw", include_str!("../scenarios/join-throw.luau")),
];

/// The scripts: the runner, each scenario of `list` under `scenarios/`, and
/// the index that requires them.
fn scripts(list: &[(&str, &str)]) -> Scripts {
    let mut index = String::from("return {\n");
    let mut scripts = Scripts::new().with("main.luau", RUNNER);
    for &(name, source) in list {
        scripts = scripts.with(&format!("scenarios/{name}.luau"), source);
        // Writing to a `String` cannot fail.
        let _ = writeln!(index, "\t[\"{name}\"] = require(\"./scenarios/{name}\"),");
    }
    index.push_str("}\n");
    scripts.with(INDEX, index)
}

/// The world setting: one of the scenarios of `list`, or none.
fn schema(list: &[(&str, &str)]) -> Schema {
    let names: Vec<&str> = core::iter::once("")
        .chain(list.iter().map(|(name, _)| *name))
        .collect();
    let field = Field::new(SETTING, "").with_choices(&names).with_doc(
        "The check scenario to run once the world has loaded; empty runs none.",
        "",
    );
    Schema::new("", "config", vec![field])
}

/// Starts the runner in a VM of the probe's own, which its callbacks keep.
///
/// # Errors
///
/// [`Off`] when a script does not compile or fails at start.
pub(crate) fn start(load: &mut Load<'_>) -> Result<(), Off> {
    run(load, &SCENARIOS)
}

/// Starts the runner over the scenarios of `list`.
fn run(load: &mut Load<'_>, list: &[(&str, &str)]) -> Result<(), Off> {
    let limits = Limits::default();
    enshwick_luau::start(load, ID, scripts(list), Some(schema(list)), limits).map(drop)
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::events::{SettingsReplaced, WorldLoaded, WorldSaved};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    use super::SCENARIOS;

    /// The scenarios a test runs, set before the mod starts.
    static LIST: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();

    struct Runner;

    impl Mod for Runner {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            crate::chat::register(load);
            super::run(load, LIST.get().map_or(&[], Vec::as_slice))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Runner,
        r#"
        id = "enshwick.probe"
        side = "server"
        "#
    );

    const ANN: PlayerKey = PlayerKey::new(7);
    const ANN_ENTITY: Entity = Entity(10);

    fn ann() -> PlayerInfo {
        let mut name = [0u8; 64];
        name[..3].copy_from_slice(b"Ann");
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: ANN_ENTITY,
            key: ANN,
            permissions: 0,
            name_len: 3,
            name,
        }
    }

    /// Starts the runner over `list` with the world's setting naming
    /// `scenario`, with Ann online, and loads the world.
    fn start(list: Vec<(&'static str, &'static str)>, scenario: &str) -> Installed {
        LIST.set(list).unwrap();
        let loader = FakeHost::new()
            .world()
            .player(ann())
            .settings(&format!("scenario = \"{scenario}\"\n"))
            .install();
        // SAFETY: both functions are the exports of this module's `declare_mod!(Runner)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(
            status,
            Status::OK,
            "{:?} {:?}",
            loader.faults(),
            loader.logs()
        );
        let _handled = loader.raise::<SettingsReplaced>(&());
        let _handled = loader.raise::<WorldLoaded>(&());
        run_for(&loader, Duration::from_secs(11));
        loader
    }

    /// Runs server steps of 100 ms for `time`.
    fn run_for(loader: &Installed, time: Duration) {
        let step = Duration::from_millis(100);
        for _ in 0..time.as_millis() / step.as_millis() {
            let _handlers = loader.tick(step);
        }
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    #[track_caller]
    fn assert_logged(loader: &Installed, text: &str) {
        assert!(logged(loader, text), "no {text:?} in {:#?}", loader.logs());
    }

    #[test]
    fn every_scenario_file_is_listed_once_by_its_name() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name != "runner.luau")
            .collect();
        files.sort();
        let mut listed: Vec<String> = SCENARIOS
            .iter()
            .map(|(name, _)| format!("{name}.luau"))
            .collect();
        listed.sort();
        assert_eq!(files, listed);
    }

    #[test]
    fn every_shipped_scenario_compiles_and_none_runs_without_the_setting() {
        let loader = start(SCENARIOS.to_vec(), "");
        run_for(&loader, Duration::from_secs(5));
        assert!(!logged(&loader, "starts in"), "{:#?}", loader.logs());
    }

    const PASSES: &str = r#"
        return {
            about = "waits and hears",
            run = function(s)
                s.step("waits a second", function() s.wait(1) end)
                s.step("hears a save", function() s.event("WorldSaved", 5) end)
            end,
        }
    "#;

    #[test]
    fn a_scenario_logs_a_pass_per_step_and_a_pass_at_the_end() {
        let loader = start(vec![("t", PASSES)], "t");
        assert_logged(&loader, "scenario t: start: waits and hears");
        run_for(&loader, Duration::from_secs(2));
        assert_logged(&loader, "scenario t: step 1 waits a second: PASS");
        assert!(!logged(&loader, "scenario t: PASS"));
        let _handled = loader.raise::<WorldSaved>(&());
        run_for(&loader, Duration::from_millis(200));
        assert_logged(&loader, "scenario t: step 2 hears a save: PASS");
        assert_logged(&loader, "scenario t: PASS, 2 steps");
    }

    const FAILS: &str = r#"
        return {
            about = "adds",
            run = function(s)
                s.step("adds", function() s.equal(1 + 1, 3, "the sum") end)
                s.step("never runs", function() end)
            end,
        }
    "#;

    #[test]
    fn a_failed_check_names_its_step_and_both_values_and_ends_the_scenario() {
        let loader = start(vec![("t", FAILS)], "t");
        assert_logged(&loader, "scenario t: FAIL at step 1 adds: ");
        assert_logged(&loader, "the sum: expected 3, got 2");
        assert!(!logged(&loader, "never runs"), "{:#?}", loader.logs());
    }

    const WAITS: &str = r#"
        return {
            about = "waits in vain",
            run = function(s)
                s.step("hears a save", function() s.event("WorldSaved", 2) end)
            end,
        }
    "#;

    #[test]
    fn an_event_that_does_not_come_in_time_fails_its_step() {
        let loader = start(vec![("t", WAITS)], "t");
        run_for(&loader, Duration::from_secs(1));
        assert!(!logged(&loader, "FAIL"), "{:#?}", loader.logs());
        run_for(&loader, Duration::from_secs(2));
        assert_logged(&loader, "scenario t: FAIL at step 1 hears a save: ");
        assert_logged(&loader, "no WorldSaved within 2 s");
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(!logged(&loader, "scenario t: PASS"), "{:#?}", loader.logs());
    }

    #[test]
    fn a_scenario_still_waiting_when_the_world_loads_again_fails() {
        let loader = start(vec![("t", WAITS)], "t");
        let _handled = loader.raise::<WorldLoaded>(&());
        run_for(&loader, Duration::from_millis(200));
        assert_logged(
            &loader,
            "FAIL at step 1 hears a save: the world loaded again",
        );
    }

    #[test]
    fn the_chat_scenario_passes_when_the_filter_drops_only_drop_me() {
        let loader = start(SCENARIOS.to_vec(), "chat");
        assert_logged(&loader, "scenario chat: start");
        assert_eq!(loader.say(ANN, "drop me"), None);
        assert_eq!(
            loader.say(ANN, "hello probe").as_deref(),
            Some("hello probe")
        );
        assert_logged(&loader, "scenario chat: step 1");
        assert_eq!(
            loader.say(ANN, "drop me please").as_deref(),
            Some("drop me please")
        );
        let _kept = loader.say(ANN, "who am I");
        assert_logged(&loader, "scenario chat: PASS, 3 steps");
    }

    #[test]
    fn the_saves_scenario_passes_when_a_held_save_is_refused() {
        let loader = start(SCENARIOS.to_vec(), "saves");
        run_for(&loader, Duration::from_secs(8));
        assert_logged(&loader, "scenario saves: step 2");
        assert!(loader.saves() > 0, "{:#?}", loader.logs());
        let _handled = loader.raise::<enshwick::events::WorldSaving>(&());
        run_for(&loader, Duration::from_millis(200));
        loader.set_save_info(Some(enshwick::abi::SaveInfo {
            header: Header::of::<enshwick::abi::SaveInfo>(),
            world: [1; 16],
            saved_at: 60,
            size: 10,
            backups: 0,
            reserved: 0,
        }));
        let _handled = loader.raise::<WorldSaved>(&());
        run_for(&loader, Duration::from_millis(200));
        assert_logged(&loader, "scenario saves: step 3");
        assert_logged(&loader, "scenario saves: PASS, 4 steps");
    }
}
