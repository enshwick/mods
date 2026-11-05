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
pub(crate) const SCENARIOS: [(&str, &str); 9] = [
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
    ("teleport", include_str!("../scenarios/teleport.luau")),
    ("stats", include_str!("../scenarios/stats.luau")),
    ("knowledge", include_str!("../scenarios/knowledge.luau")),
];

/// The scripts: the runner, each scenario of `list` under `scenarios/`, and
/// the index that requires them.
fn scripts(list: &[(&str, &str)]) -> Scripts {
    let mut index = String::from("return {\n");
    let mut scripts = Scripts::new().with(enshwick::packs::SCRIPT_MAIN, RUNNER);
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

    /// The events the runner hears through a hook of the API rather than
    /// `enshwick.on`.
    const HOOKED: [&str; 4] = ["ChatSaid", "ChatTyped", "ItemAction", "FrontChanged"];

    /// The scenarios a test runs, set before the mod starts.
    static LIST: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();

    struct Runner;

    impl Mod for Runner {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            match LIST.get() {
                Some(list) => super::run(load, list)?,
                None => super::start(load)?,
            }
            crate::chat::register(load);
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

    /// Starts the runner over `list`, the shipped scenarios for `None`, with
    /// the world's setting naming `scenario`, with Ann online, and loads the
    /// world.
    fn start(list: Option<Vec<(&'static str, &'static str)>>, scenario: &str) -> Installed {
        if let Some(list) = list {
            LIST.set(list).unwrap();
        }
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
        let loader = start(None, "");
        run_for(&loader, Duration::from_secs(5));
        assert!(!logged(&loader, "starts in"), "{:#?}", loader.logs());
        let schema = loader.config_schema("settings").unwrap();
        let choices = &schema.field(super::SETTING).unwrap().choices;
        for (name, _) in SCENARIOS {
            assert!(
                choices.contains(&format!("\"{name}\"")),
                "{name} in {choices:?}"
            );
        }
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
        let loader = start(Some(vec![("t", PASSES)]), "t");
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
        let loader = start(Some(vec![("t", FAILS)]), "t");
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
        let loader = start(Some(vec![("t", WAITS)]), "t");
        run_for(&loader, Duration::from_secs(1));
        assert!(!logged(&loader, "FAIL"), "{:#?}", loader.logs());
        run_for(&loader, Duration::from_secs(2));
        assert_logged(&loader, "scenario t: FAIL at step 1 hears a save: ");
        assert_logged(&loader, "no WorldSaved within 2 s");
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(!logged(&loader, "scenario t: PASS"), "{:#?}", loader.logs());
    }

    const BAD_TEST: &str = r#"
        return {
            about = "tests badly",
            run = function(s)
                s.step("hears a save", function()
                    s.event("WorldSaved", 5, function() error("boom") end)
                end)
            end,
        }
    "#;

    #[test]
    fn an_error_in_an_event_test_fails_the_scenario_at_once() {
        let loader = start(Some(vec![("t", BAD_TEST)]), "t");
        let _handled = loader.raise::<WorldSaved>(&());
        run_for(&loader, Duration::from_millis(200));
        assert_logged(
            &loader,
            "FAIL at step 1 hears a save: the test of WorldSaved: ",
        );
        assert_logged(&loader, "boom");
    }

    #[test]
    fn a_scenario_still_waiting_when_the_world_loads_again_fails() {
        let loader = start(Some(vec![("t", WAITS)]), "t");
        let _handled = loader.raise::<WorldLoaded>(&());
        run_for(&loader, Duration::from_millis(200));
        assert_logged(
            &loader,
            "FAIL at step 1 hears a save: the world loaded again",
        );
    }

    #[test]
    fn the_chat_scenario_passes_when_the_filter_drops_only_drop_me() {
        let loader = start(None, "chat");
        assert_logged(&loader, "PASS chat filter registered");
        assert_logged(&loader, "PASS chat lines heard");
        assert_logged(&loader, "scenario chat: start");
        assert_eq!(loader.say(ANN, "drop me"), None);
        assert_logged(&loader, "chat filter dropped a line from 0000000000000007");
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
        let loader = start(None, "saves");
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

    #[test]
    fn the_runner_hears_every_event_it_hooks() {
        let loader = start(None, "");
        for event in HOOKED {
            assert!(
                !logged(&loader, &format!("{event} is not heard")),
                "{:#?}",
                loader.logs()
            );
        }
    }

    const HEARS_AN_ITEM_ACTION: &str = r#"
        return {
            about = "hears a drink",
            run = function(s)
                s.step("hears a drink", function()
                    local action = s.event("ItemAction", 5, function(action)
                        return action.kind == "Consume"
                    end)
                    s.equal(action.player, 10, "the player")
                end)
            end,
        }
    "#;

    #[test]
    fn a_scenario_hears_an_item_action_and_lets_it_happen() {
        use enshwick::abi::ItemAction;

        let loader = start(Some(vec![("t", HEARS_AN_ITEM_ACTION)]), "t");
        let call = ItemAction {
            header: Header::of::<ItemAction>(),
            kind: ItemAction::CONSUME,
            player: ANN_ENTITY,
            item: enshwick::ItemId::new(0x5e01),
            count: 1,
            inventory: Entity(2),
            slot: 0,
        };
        assert!(loader.item_action(&call));
        run_for(&loader, Duration::from_millis(200));
        assert_logged(&loader, "scenario t: PASS, 1 steps");
    }

    #[test]
    fn the_chat_scenario_fails_when_drop_me_was_never_typed() {
        let loader = start(None, "chat");
        assert_eq!(
            loader.say(ANN, "hello probe").as_deref(),
            Some("hello probe")
        );
        run_for(&loader, Duration::from_secs(301));
        assert_logged(&loader, "no ChatTyped within 300 s");
        assert!(
            !logged(&loader, "scenario chat: step 1"),
            "{:#?}",
            loader.logs()
        );
    }

    #[test]
    fn a_failed_saves_scenario_lets_the_saves_go() {
        let loader = start(None, "saves");
        run_for(&loader, Duration::from_secs(2));
        assert!(loader.saves_held(), "{:#?}", loader.logs());
        let _handled = loader.raise::<WorldSaved>(&());
        run_for(&loader, Duration::from_millis(200));
        assert_logged(&loader, "WorldSaved came within 5 s");
        assert!(!loader.saves_held(), "{:#?}", loader.logs());
    }

    #[test]
    fn a_failed_knowledge_scenario_takes_the_granted_quest_back() {
        use enshwick::abi::KnowledgeInfo;
        use enshwick::knowledge::KnowledgeId;

        let quest = KnowledgeId::new(42);
        let info = |value| KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id: quest,
            kind: KnowledgeInfo::BOOL,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value,
            flags: 0,
        };
        let loader = FakeHost::new()
            .world()
            .player(ann())
            .settings("scenario = \"knowledge\"\n")
            .install();
        loader.set_knowledge(&[(Some(ANN_ENTITY), info(0))], &[quest]);
        // SAFETY: both functions are the exports of this module's `declare_mod!(Runner)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let _handled = loader.raise::<SettingsReplaced>(&());
        let _handled = loader.raise::<WorldLoaded>(&());
        run_for(&loader, Duration::from_secs(12));
        assert_logged(&loader, "scenario knowledge: step 1");
        assert_eq!(loader.grants().len(), 1, "{:#?}", loader.logs());
        // The grant lands, but its event never comes.
        loader.set_knowledge(&[(Some(ANN_ENTITY), info(1))], &[quest]);
        run_for(&loader, Duration::from_secs(11));
        assert_logged(&loader, "no KnowledgeChanged within 10 s");
        assert_eq!(
            loader
                .revokes()
                .iter()
                .map(|revoke| revoke.1)
                .collect::<Vec<_>>(),
            [quest],
            "{:#?}",
            loader.logs()
        );
    }

    #[test]
    fn the_stats_scenario_reads_extra_maximum_health_of_a_plain_character() {
        let loader = start(None, "stats");
        run_for(&loader, Duration::from_secs(2));
        assert_logged(&loader, "scenario stats: step 1");
        assert!(
            !logged(&loader, "scenario stats: FAIL at step 1"),
            "{:#?}",
            loader.logs()
        );
    }
}
