//! Check scenarios in Luau: the probe's own VM runs the one the world's
//! `scenario` setting names once the world has loaded, and logs a line per
//! step and `PASS` or `FAIL` at the end.

use core::fmt::Write as _;

use enshwick::config::{Field, Schema};
use enshwick::{Load, Off};
use enshwick_luau::{Limits, Runtime, Scripts};

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

/// The scripts: the runner, each scenario under `scenarios/`, and the index
/// that requires them.
fn scripts() -> Scripts {
    let mut index = String::from("return {\n");
    let mut scripts = Scripts::new().with("main.luau", RUNNER);
    for (name, source) in SCENARIOS {
        scripts = scripts.with(&format!("scenarios/{name}.luau"), source);
        // Writing to a `String` cannot fail.
        let _ = writeln!(index, "\t[\"{name}\"] = require(\"./scenarios/{name}\"),");
    }
    index.push_str("}\n");
    scripts.with(INDEX, index)
}

/// The world setting: one of the scenarios, or none.
fn schema() -> Schema {
    let names: Vec<&str> = core::iter::once("")
        .chain(SCENARIOS.iter().map(|(name, _)| *name))
        .collect();
    let field = Field::new(SETTING, "").with_choices(&names).with_doc(
        "The check scenario to run once the world has loaded; empty runs none.",
        "",
    );
    Schema::new("", "config", vec![field])
}

/// Starts the runner in a VM of the probe's own.
///
/// # Errors
///
/// [`Off`] when a script does not compile or fails at start.
pub(crate) fn start(load: &mut Load<'_>) -> Result<Runtime, Off> {
    enshwick_luau::start(load, ID, scripts(), Some(schema()), Limits::default())
}
