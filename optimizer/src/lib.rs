//! Less CPU for the Enshrouded server, on Windows and under Wine or Proton,
//! and the numbers to tell.
//!
//! The mod knows where it runs, the dedicated server or the game, on
//! Windows or under Wine, and turns on by default only the changes that A/B
//! runs showed to help there (one table in `platform.rs`). A setting can force a
//! change anywhere.
//!
//! - Exact waits: the engine's event loop sleeps the last fraction of a
//!   millisecond before a deadline instead of polling its handles.
//! - Task workers: under Wine in a container with a CPU limit, the task
//!   groups start no more workers than the limit allows.
//! - Server wait cap: the server's game loop may wait longer between its
//!   passes while players are online, never past the next simulation step.
//! - Measure mode: a line a minute with the CPU used, by thread group, the
//!   loop and step rates and the server's "Bad Performance" warnings.
//! - At start, one line names what the server runs on: Wine or Windows, the
//!   fast synchronisation available, the CPU governor, the container's CPU
//!   limit and the CPUs the game saw.
//!
//! Waits are never throttled across the board: an earlier fix that delayed
//! every wait of every thread put players under the ground and broke
//! gliding. The exact waits only sleep time the game would have spent
//! polling.

use enshwick::abi::Side;
use enshwick::pacing::TaskWorkers;
use enshwick::state::ModLocal;
use enshwick::{Host, Load, Mod, Off};

use crate::env::CpuLimit;
use crate::platform::{Change, Platform, default_on};

mod env;
mod measure;
mod platform;
mod sys;
mod waits;

enshwick::texts!();

/// Whether a change runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Switch {
    /// Where it was measured to help.
    #[default]
    Auto,
    /// Everywhere.
    On,
    /// Nowhere.
    Off,
}

impl Switch {
    /// Whether the change runs where its default is `default`.
    #[must_use]
    pub fn applies(self, default: bool) -> bool {
        match self {
            Self::Auto => default,
            Self::On => true,
            Self::Off => false,
        }
    }
}

/// Optimizer settings.
#[enshwick::config]
#[derive(Debug)]
pub struct Settings {
    /// Exact short waits in the engine's event loop: "auto" on the dedicated
    /// server, on Windows and under Wine, where they were measured to help;
    /// "on" in the game too; "off" never.
    #[config(choices = [Switch::Auto, Switch::On, Switch::Off])]
    pub exact_waits: Switch,
    /// The longest the server's game loop waits between its passes while
    /// players are online, in milliseconds, 1 to 8, never past the next
    /// simulation step. 1 is vanilla. Check gliding, joining and combat
    /// before keeping more.
    #[config(range = 1..=8)]
    pub server_wait_cap_ms: u32,
    /// A line a minute in the log and the status summary: CPU, by thread
    /// group, loop and step rates, exact waits and the server's Bad
    /// Performance warnings.
    pub measure: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exact_waits: Switch::Auto,
            server_wait_cap_ms: 1,
            measure: true,
        }
    }
}

/// The task worker cap on `platform` under `limit`: `None` leaves the game's
/// own count. The game starts its task groups once, before any world, so no
/// setting can choose it.
#[must_use]
pub(crate) fn worker_cap(platform: Platform, limit: Option<CpuLimit>) -> Option<u8> {
    if !default_on(Change::TaskWorkers, platform) {
        return None;
    }
    match limit? {
        CpuLimit::Cpus(cpus) => {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 2 to 64 first"
            )]
            let cap = cpus.ceil().clamp(2.0, 64.0) as u8;
            Some(cap)
        }
        CpuLimit::Unlimited => None,
    }
}

/// What init found, for the status summary and the settings handler.
#[derive(Debug)]
struct Found {
    platform: Platform,
    summary: String,
    linked: waits::Linked,
    workers: Option<u8>,
}

/// What init found, once it has.
static FOUND: ModLocal<Found> = ModLocal::new();

/// The platform and what runs on it, for the measure lines and the status
/// summary.
fn running(found: &Found, settings: &Settings) -> String {
    let workers = match found.workers {
        Some(cap) => format!("task workers at most {cap}"),
        None => "task workers as the game picks".to_owned(),
    };
    let cap = if settings.server_wait_cap_ms > 1 {
        format!("server wait cap {} ms", settings.server_wait_cap_ms)
    } else {
        "server wait vanilla".to_owned()
    };
    format!(
        "{}: exact waits {}, {workers}, {cap}",
        found.platform,
        if waits::exact_on() { "on" } else { "off" },
    )
}

/// Logs a change forced on where it was not measured to help.
fn note_forced(change: Change, forced: bool, platform: Platform) {
    if forced && !default_on(change, platform) {
        enshwick::info!(
            "{change}: forced on; off by default on a {platform}, where it was not measured to help"
        );
    }
}

/// Applies `settings` to the running hooks.
fn apply(settings: &Settings) {
    let Some(found) = FOUND.get() else {
        return;
    };
    let exact = settings
        .exact_waits
        .applies(default_on(Change::ExactWaits, found.platform));
    note_forced(
        Change::ExactWaits,
        settings.exact_waits == Switch::On,
        found.platform,
    );
    waits::set_exact(exact && found.linked.exact);
    waits::set_cap(settings.server_wait_cap_ms);
    let running = running(found, settings);
    enshwick::info!(
        "{running}; measure mode {}",
        if settings.measure { "on" } else { "off" }
    );
    measure::set(settings.measure, running);
}

/// The status lines.
fn status() -> String {
    let Some(found) = FOUND.get() else {
        return String::new();
    };
    let mut lines = vec![found.summary.clone(), measure::context()];
    let last = measure::last();
    if !last.is_empty() {
        lines.push(last);
    }
    lines.join("\n")
}

/// The mod. Its running state is in mod-life statics the hooks read.
#[derive(Debug)]
pub struct Optimizer;

impl Mod for Optimizer {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let _texts = load.texts()?;
        let environment = env::Environment::probe(&measure::game_log_text());
        let wine = environment.is_wine();
        let platform = Platform::of(load.host().build().side == Side::SERVER, wine);
        let summary = environment.summary();
        enshwick::info!("environment: {summary}");
        for advice in environment.advice() {
            enshwick::info!("advice: {advice}");
        }
        if !wine {
            waits::tune_for_windows();
        }
        let mut workers = worker_cap(platform, environment.cpu_limit);
        if let Some(cap) = workers
            && !load.cap_task_workers(TaskWorkers::at_most(cap))?
        {
            enshwick::info!(
                "task workers: only the dedicated server starts fewer; here the game picks"
            );
            workers = None;
        }
        let linked = waits::link(load)?;
        enshwick::info!(
            "hooked: exact waits {}, message wait {}, server loop wait {}, task workers {}",
            linked.exact,
            linked.message,
            linked.server,
            workers.is_some()
        );
        load.on_every_step(|_| waits::stepped())?;
        let _first = FOUND.set(Found {
            platform,
            summary: format!("environment: {summary}"),
            linked,
            workers,
        });
        load.add_status(status)?;
        load.on_settings_change(apply)?;
        Ok(Self)
    }

    fn ready(&'static self, _host: Host) {
        let wine = FOUND.get().is_some_and(|found| {
            matches!(found.platform, Platform::WineServer | Platform::WineClient)
        });
        if let Err(e) = measure::start(wine) {
            enshwick::warn!("measure mode does not run: {e}");
        }
    }
}

enshwick::declare_mod!(Optimizer);

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::config::Config as _;
    use enshwick::events;

    use super::*;

    #[test]
    fn every_text_exists_in_all_languages() {
        use enshwick::i18n::{MOD_DESCRIPTION, MOD_NAME};

        let texts = enshwick::i18n::Texts::parse(texts::TOML).unwrap();
        assert_eq!(texts.untranslated(), []);
        let schema = Settings::schema();
        let mut keys = vec![
            MOD_NAME.to_owned(),
            MOD_DESCRIPTION.to_owned(),
            schema.text.clone(),
        ];
        for field in &schema.fields {
            keys.push(field.text.clone());
            keys.push(format!("{}.doc", field.text));
            keys.extend(
                field
                    .choices
                    .iter()
                    .map(|choice| format!("{}.{}", field.text, choice.trim_matches('"'))),
            );
        }
        for key in keys {
            let found = texts.get(&key, enshwick::i18n::Language::FALLBACK);
            assert!(found.is_some(), "{key}");
        }
    }

    #[test]
    fn auto_follows_the_table() {
        assert!(Switch::Auto.applies(true));
        assert!(!Switch::Auto.applies(false));
        assert!(Switch::On.applies(false));
        assert!(!Switch::Off.applies(true));
    }

    #[test]
    fn task_workers_follow_the_cpu_limit_under_wine_only() {
        let four = Some(CpuLimit::Cpus(3.5));
        assert_eq!(worker_cap(Platform::WineServer, four), Some(4));
        assert_eq!(
            worker_cap(Platform::WineServer, Some(CpuLimit::Cpus(0.5))),
            Some(2)
        );
        assert_eq!(
            worker_cap(Platform::WineServer, Some(CpuLimit::Unlimited)),
            None
        );
        assert_eq!(worker_cap(Platform::WineServer, None), None);
        assert_eq!(worker_cap(Platform::WindowsServer, four), None);
    }

    #[test]
    fn the_defaults_parse_and_ranges_hold() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.exact_waits, Switch::Auto);
        assert_eq!(settings.server_wait_cap_ms, 1);
        assert!(settings.measure);
        assert!(settings.validate().is_ok());
        let wide: Settings =
            toml::from_str("exact_waits = \"on\"\nserver_wait_cap_ms = 9\nmeasure = false")
                .unwrap();
        assert!(wide.validate().is_err());
        assert!(
            toml::from_str::<Settings>(
                "exact_waits = \"maybe\"\nserver_wait_cap_ms = 1\nmeasure = true"
            )
            .is_err()
        );
    }

    #[test]
    fn forced_on_the_waits_sleep_exactly_and_the_cap_stays_under_the_step() {
        use core::sync::atomic::Ordering;

        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .settings("exact_waits = \"on\"\nserver_wait_cap_ms = 4\nmeasure = true\n")
            .install();
        // SAFETY: the export of `declare_mod!` above. Without `ready` no
        // measuring thread sets the slack to the timer resolution.
        let status = unsafe { loader.init(enshwick_mod_init) };
        waits::set_slack(0);
        waits::set_wake(0);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        // The world loads, and its settings apply.
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);

        // A zero wait with 0.3 ms left sleeps them.
        let slept = waits::SLEEPS.load(Ordering::Relaxed);
        let started = std::time::Instant::now();
        loader.loop_wait(0, 300_000);
        assert!(started.elapsed() >= std::time::Duration::from_micros(100));
        assert_eq!(waits::SLEEPS.load(Ordering::Relaxed), slept + 1);
        // A timed wait is left alone.
        loader.loop_wait(16, 16_600_000);
        assert_eq!(waits::SLEEPS.load(Ordering::Relaxed), slept + 1);

        // Before any step the server wait stays vanilla; right after one it
        // grows to the cap; the idle wait never changes. Every step counts,
        // however short.
        let online = enshwick::pacing::SERVER_ONLINE_WAIT_NS;
        let idle = enshwick::pacing::SERVER_IDLE_WAIT_NS;
        assert_eq!(loader.server_wait(online), online);
        let steps = waits::STEPS.load(Ordering::Relaxed);
        assert_eq!(loader.tick(std::time::Duration::from_millis(16)), 1);
        assert_eq!(loader.tick(std::time::Duration::from_millis(16)), 1);
        assert_eq!(waits::STEPS.load(Ordering::Relaxed), steps + 2);
        assert_eq!(loader.server_wait(online), 4_000_000);
        assert_eq!(loader.server_wait(idle), idle);

        // Off at once.
        loader.set_settings(
            "settings",
            "exact_waits = \"off\"\nserver_wait_cap_ms = 1\nmeasure = true\n",
        );
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(loader.server_wait(online), online);
        loader.loop_wait(0, 300_000);
        assert_eq!(waits::SLEEPS.load(Ordering::Relaxed), slept + 1);
        assert!(
            loader
                .status()
                .iter()
                .any(|line| line.contains("environment: "))
        );
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_world_that_turns_the_waits_on_gets_them_at_its_load_without_a_restart() {
        use core::sync::atomic::Ordering;

        let off = "exact_waits = \"off\"\nserver_wait_cap_ms = 1\nmeasure = false\n";
        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .settings(off)
            .install();
        // SAFETY: the export of `declare_mod!` above.
        let status = unsafe { loader.init(enshwick_mod_init) };
        waits::set_slack(0);
        waits::set_wake(0);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        let slept = waits::SLEEPS.load(Ordering::Relaxed);
        loader.loop_wait(0, 300_000);
        assert_eq!(
            waits::SLEEPS.load(Ordering::Relaxed),
            slept,
            "off: no sleep"
        );

        loader.set_settings(
            "settings",
            "exact_waits = \"on\"\nserver_wait_cap_ms = 4\nmeasure = false\n",
        );
        assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
        loader.loop_wait(0, 300_000);
        assert_eq!(waits::SLEEPS.load(Ordering::Relaxed), slept + 1);
        // No step here: a later test in this process needs none to have run.
        assert_eq!(waits::CAP_NS.load(Ordering::Relaxed), 4_000_000);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn ready_starts_measuring() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .settings("exact_waits = \"auto\"\nserver_wait_cap_ms = 1\nmeasure = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above, `ready` after `init`
        // returned OK.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(measure::running(), "{:?}", loader.logs());
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }
}
