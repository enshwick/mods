//! Development only, never shipped: logs what a running client does where
//! the platform still needs engine facts, to
//! `logs/enshwick.research_<date>_<time>.log`.
//!
//! Off until `enabled = true` in `settings.toml` in its own folder in `mods/`;
//! read at start. On a client it looks at the item tooltip (which argument names the
//! hovered item), at the interface (scroll areas, tab bars, text input) and
//! at join attempts (what a refused join leaves behind). On both sides it
//! logs the entities near each player, so a client's and a server's log can
//! be joined (which ids a client gives the server's entities), the health
//! the game's impact programs set (heals, revives, scripted kills), how
//! skills are unlocked and taken back, where a player's spawn target
//! points, and whether any thread runs on a fiber.
//! Its `mark` action, F7 for administrators in a game the client hosts or on
//! a dedicated server, starts a new section of the log.

mod entities;
mod fibers;
mod health;
mod join;
mod memory;
mod respawn;
mod skills;
#[cfg(test)]
mod targets;
mod tooltip;
mod ui;

use enshwick::abi::Side;
use enshwick::actions::{Action, Actions, Outcome, Request};
use enshwick::controls::Key;
use enshwick::i18n::Texts;
use enshwick::{Load, Mod, Off};

const fn yes() -> bool {
    true
}

const fn interval_ms() -> u64 {
    1000
}

const fn max_dumps() -> u32 {
    300
}

/// Research settings, read at start.
#[enshwick::config]
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches of a development mod's settings file"
)]
struct Settings {
    /// Log what the client does where the platform needs engine facts;
    /// development only.
    #[serde(default)]
    enabled: bool,
    /// Look at the item tooltip: what each of its functions is handed.
    #[serde(default = "yes")]
    tooltip: bool,
    /// Look at the interface: scroll areas, tab bars and text input.
    #[serde(default = "yes")]
    ui: bool,
    /// Look at join attempts and the menus after them.
    #[serde(default = "yes")]
    join: bool,
    /// Log the entities near each player every five seconds, on the client
    /// and on the server, with their ids, templates and positions.
    #[serde(default = "yes")]
    entities: bool,
    /// Log the health the game's impact programs set, outside hits: heals,
    /// revives, scripted kills; on the client and on the server.
    #[serde(default = "yes")]
    impact_health: bool,
    /// Look for threads on fibers for five minutes after the start.
    #[serde(default = "yes")]
    fibers: bool,
    /// Log every unlock and refund of a skill node.
    #[serde(default = "yes")]
    skills: bool,
    /// Log each player's spawn target when it changes; on the server.
    #[serde(default = "yes")]
    respawn: bool,
    /// The least time between two dumps of one tooltip function, in
    /// milliseconds, unless a mark comes between.
    #[serde(default = "interval_ms")]
    #[config(range = 0..=60_000)]
    dump_interval_ms: u64,
    /// The most tooltip dumps between two marks.
    #[serde(default = "max_dumps")]
    #[config(range = 1..=100_000)]
    max_dumps: u32,
    /// The values each mark looks for in what the tooltip functions are
    /// handed; read again at each mark.
    #[serde(default)]
    mark_values: Vec<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            tooltip: yes(),
            ui: yes(),
            join: yes(),
            entities: yes(),
            impact_health: yes(),
            fibers: yes(),
            skills: yes(),
            respawn: yes(),
            dump_interval_ms: interval_ms(),
            max_dumps: max_dumps(),
            mark_values: Vec::new(),
        }
    }
}

/// This machine's switches, `settings.toml` in the mod's own folder, which the
/// mod reads itself: a developer mod keeps no world settings. The defaults
/// without the file, or with a file that does not read.
fn own_settings<T: enshwick::config::Config>(load: &Load<'_>) -> T {
    load.mod_dir()
        .map_or_else(|_| T::default(), |dir| settings_in(&dir))
}

/// The switches of `settings.toml` in `dir`, as [`own_settings`] reads them.
fn settings_in<T: enshwick::config::Config>(dir: &std::path::Path) -> T {
    let path = dir.join("settings.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        enshwick::warn!("{}: {e}; its defaults apply this time", path.display());
        T::default()
    })
}

/// The research mod: whether it looks for fibers once ready.
#[derive(Debug)]
pub struct Research {
    fibers: bool,
}

impl Mod for Research {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = own_settings(load);
        if !settings.enabled {
            return Ok(Self { fibers: false });
        }
        let client = load.host().build().side == Side::CLIENT;
        if settings.entities {
            entities_link(load, client);
        }
        if settings.impact_health {
            match health::link(load) {
                Ok(()) => enshwick::info!("R17 impact health writes looked at"),
                Err(off) => enshwick::warn!("R17 off: {off}"),
            }
        }
        if settings.skills {
            match skills::link(load) {
                Ok(()) => enshwick::info!("R19 skill changes looked at"),
                Err(off) => enshwick::warn!("R19 off: {off}"),
            }
        }
        if settings.respawn {
            match load.world().and_then(|world| respawn::link(load, world)) {
                Ok(()) => enshwick::info!("R20 spawn targets read every 5 s"),
                Err(off) => enshwick::warn!("R20 off: {off}"),
            }
        }
        if client {
            if settings.tooltip {
                match tooltip::link(load, settings.dump_interval_ms, settings.max_dumps) {
                    Ok(hooked) => {
                        enshwick::info!("R1 {hooked} tooltip functions looked at");
                    }
                    Err(off) => enshwick::warn!("R1 off: {off}"),
                }
            }
            if settings.ui {
                match ui::link(load) {
                    Ok(hooked) => {
                        enshwick::info!("R2 R3 R5 {hooked} interface functions looked at");
                    }
                    Err(off) => enshwick::warn!("R2 R3 R5 off: {off}"),
                }
            }
            if settings.join {
                match join::link(load) {
                    Ok(()) => enshwick::info!("R8 join attempts looked at"),
                    Err(off) => enshwick::warn!("R8 off: {off}"),
                }
            }
        }
        let actions = actions(load)?;
        for (name, key) in CONTROLS {
            let _keyed = load.feature(name, |_| actions.control(name, Key::parse(key)?));
        }
        enshwick::info!("research on");
        Ok(Self {
            fibers: settings.fibers,
        })
    }

    fn ready(&'static self, _host: enshwick::Host) {
        if self.fibers {
            fibers::start();
        }
    }
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status};

    use super::*;

    /// This test's own folder for the mod, so tests on threads of one
    /// process never write each other's `settings.toml`.
    fn own_dir() -> std::path::PathBuf {
        let test = std::thread::current()
            .name()
            .unwrap_or_default()
            .replace(':', "_");
        std::env::temp_dir().join(format!("enshwick-research-{}-{test}", std::process::id()))
    }

    /// Starts the research mod on a fake host of `side` with `settings` as
    /// the file in its own folder; the status and the host to look at.
    fn start(side: Side, settings: &str) -> (Status, enshwick::test::Installed) {
        let dir = own_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), settings).unwrap();
        // A client hosts, as a server mod's keys need.
        let loader = enshwick::test::FakeHost::new()
            .developer_tools(true)
            .side(side)
            .hosting(side == Side::CLIENT)
            .mod_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the exports of the `declare_mod!` below.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        (status, loader)
    }

    #[test]
    fn the_research_mod_is_a_developer_tool() {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let flags = unsafe { &*enshwick_mod_query() }.flags;
        assert!(flags.contains(enshwick::abi::ManifestFlags::DEVELOPER));
    }

    #[test]
    fn off_by_default_it_starts_and_registers_nothing() {
        let (status, loader) = start(Side::SERVER, "");
        assert_eq!(status, Status::OK);
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        assert!(
            !loader.logs().iter().any(|(_, line)| line == "research on"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn f7_marks_the_log_and_f8_asks_the_status_on_a_client() {
        let (status, loader) = start(Side::CLIENT, "enabled = true\n");
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        for (name, key) in CONTROLS {
            let key = Key::parse(key).unwrap();
            assert_eq!(
                loader.keys(&[key], core::time::Duration::from_millis(16)),
                1
            );
            assert_eq!(loader.keys(&[], core::time::Duration::from_millis(16)), 0);
            assert_eq!(
                loader.action_runs().last(),
                Some(&(name.to_owned(), String::new()))
            );
        }
    }

    #[test]
    fn each_mark_looks_for_the_values_the_settings_name_at_that_moment() {
        use enshwick::abi::{ActionCall, Header, PlayerInfo};

        let (status, loader) = start(Side::SERVER, "enabled = true\nmark_values = [7, 42]\n");
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let admin = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(2),
            key: enshwick::PlayerKey::new(1),
            permissions: PlayerInfo::CAN_KICK_BAN,
            name_len: 0,
            name: [0; 64],
        };
        let marked = |values: &str| {
            let reply = loader.ask_actions(ActionCall::RUN, &admin, 0, "mark", "");
            assert!(reply.is_some_and(|r| r.starts_with('1')));
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains(&format!("(values looked for: {values})")))
        };
        assert!(marked("[7, 42]"), "{:?}", loader.logs());
        std::fs::write(
            own_dir().join("settings.toml"),
            "enabled = true\nmark_values = [5]\n",
        )
        .unwrap();
        assert!(marked("[5]"), "{:?}", loader.logs());
    }

    /// The game code of a fake host: each target's first prologue in a slot
    /// of its own, padded with traps, and each target's symbol `skew` bytes
    /// into its slot.
    fn code_of(
        targets: &[&memory::Target],
        skew: u64,
    ) -> (&'static [u8], Vec<(&'static str, u64)>) {
        const SLOT: usize = 64;
        let mut code = vec![0xccu8; SLOT * targets.len()];
        for (slot, target) in code.chunks_mut(SLOT).zip(targets) {
            slot[..target.prologues[0].len()].copy_from_slice(target.prologues[0]);
        }
        let code: &'static [u8] = Vec::leak(code);
        let symbols = targets
            .iter()
            .enumerate()
            .map(|(n, t)| (t.symbol, (code.as_ptr().addr() + n * SLOT) as u64 + skew))
            .collect();
        (code, symbols)
    }

    /// A feature's setting, side, targets, how many of them it hooks, the
    /// line when every target opens as pinned, and the line when none does.
    type Case = (
        &'static str,
        Side,
        Vec<&'static memory::Target>,
        usize,
        &'static str,
        &'static str,
    );

    /// Starts the mod with only `setting` on, over `code` with `symbols` and
    /// `constants`; its log lines and how many hooks landed on a symbol.
    fn started_with_only(
        setting: &str,
        side: Side,
        code: &'static [u8],
        symbols: &[(&str, u64)],
        constants: &[(&str, u64)],
    ) -> (Vec<String>, usize) {
        let features = [
            "tooltip",
            "ui",
            "join",
            "entities",
            "impact_health",
            "fibers",
            "skills",
            "respawn",
        ];
        let dir = own_dir().join(format!("{setting}-{:x}", code.as_ptr().addr()));
        std::fs::create_dir_all(&dir).unwrap();
        let only = features
            .iter()
            .fold("enabled = true\n".to_owned(), |text, f| {
                format!("{text}{f} = {}\n", *f == setting)
            });
        std::fs::write(dir.join("settings.toml"), only).unwrap();
        let pinned: Vec<u64> = symbols.iter().map(|&(_, at)| at).collect();
        let host = enshwick::test::FakeHost::new()
            .developer_tools(true)
            .side(side)
            .hosting(side == Side::CLIENT)
            .mod_dir(dir.to_str().unwrap())
            .world()
            .type_hash(entities::LOCAL_PLAYER, 0x7e57)
            .code(code);
        let host = symbols
            .iter()
            .fold(host, |host, &(name, at)| host.symbol(name, at));
        let loader = constants
            .iter()
            .fold(host, |host, &(name, value)| host.constant(name, value))
            .install();
        // SAFETY: the exports of the `declare_mod!` below.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let hooked = loader
            .hooks()
            .iter()
            .map(|hook| hook.target)
            .chain(loader.mid_hooks())
            .filter(|at| pinned.contains(at))
            .count();
        let logs = loader.logs().into_iter().map(|(_, line)| line).collect();
        (logs, hooked)
    }

    #[test]
    fn the_join_watches_no_menu_entry_whose_first_bytes_are_not_the_loaders() {
        let code: &'static [u8] = Vec::leak(vec![0xccu8; 48]);
        let at = |n: usize| (code.as_ptr().addr() + n * 16) as u64;
        let traps = u64::from_le_bytes([0xcc; 8]);
        let symbols = [
            ("net.join_game", at(0)),
            ("ui.main_entry", at(1)),
            ("ui.pause_entry", at(2)),
        ];
        for (main, pause, hooks) in [(traps, traps, 3), (traps, !traps, 1), (!traps, traps, 1)] {
            let constants = [
                ("net.join_game.bytes", traps),
                ("ui.main_entry.bytes", main),
                ("ui.pause_entry.bytes", pause),
            ];
            let (logs, hooked) =
                started_with_only("join", Side::CLIENT, code, &symbols, &constants);
            assert_eq!(hooked, hooks, "{logs:?}");
            let off = logs
                .iter()
                .any(|line| line.starts_with("R8 off: the code at ui."));
            assert_eq!(off, hooks == 1, "{logs:?}");
        }
    }

    #[test]
    fn each_feature_hooks_or_calls_only_code_that_opens_with_a_prologue_of_its_targets() {
        let tooltip_sites: &'static [tooltip::Site] = &tooltip::SITES;
        let ui_sites: &'static [ui::Site] = &ui::SITES;
        let called: &'static [memory::Target] = &entities::CALLED;
        let tooltip = tooltip_sites.iter().map(|site| &site.target).collect();
        let ui = ui_sites.iter().map(|site| &site.target).collect();
        let entities = core::iter::once(&entities::CURSOR).chain(called).collect();
        let cases: [Case; 5] = [
            (
                "impact_health",
                Side::SERVER,
                vec![&health::TARGET],
                1,
                "R17 impact health writes looked at",
                "R17 off: research.impact_attribute: its bytes at",
            ),
            (
                "skills",
                Side::SERVER,
                vec![&skills::TARGET],
                1,
                "R19 skill changes looked at",
                "R19 off: research.skill_change: its bytes at",
            ),
            (
                "tooltip",
                Side::CLIENT,
                tooltip,
                tooltip::SITES.len(),
                "R1 10 tooltip functions looked at",
                "R1 0 tooltip functions looked at",
            ),
            (
                "ui",
                Side::CLIENT,
                ui,
                ui::SITES.len(),
                "R2 R3 R5 5 interface functions looked at",
                "R2 R3 R5 0 interface functions looked at",
            ),
            (
                "entities",
                Side::CLIENT,
                entities,
                1,
                "R11 client sweeps every 5 s in a game",
                "R11 client side off: research.client_cursor: its bytes at",
            ),
        ];
        for (setting, side, targets, hooks, on, off) in cases {
            for skew in [0, 1] {
                let (code, symbols) = code_of(&targets, skew);
                let (logs, hooked) = started_with_only(setting, side, code, &symbols, &[]);
                let logged = |text: &str| logs.iter().any(|line| line.starts_with(text));
                let named = logs
                    .iter()
                    .filter(|line| line.contains("its bytes at"))
                    .inspect(|line| assert!(line.contains("; the game is build "), "{line}"))
                    .count();
                if skew == 0 {
                    assert!(logged(on), "{setting}: {logs:?}");
                    assert_eq!((named, hooked), (0, hooks), "{setting}: {logs:?}");
                } else {
                    assert!(logged(off), "{setting}: {logs:?}");
                    assert_eq!((named, hooked), (hooks, 0), "{setting}: {logs:?}");
                }
            }
        }
    }

    #[test]
    fn on_either_side_it_starts_or_turns_off_with_one_reason_and_never_panics() {
        for side in [Side::SERVER, Side::CLIENT] {
            let (status, loader) = start(side, "enabled = true\n");
            let faults = loader.faults();
            let on = loader.logs().iter().any(|(_, line)| line == "research on");
            if status == Status::OK {
                assert!(on, "{side:?}: {:?}", loader.logs());
                assert!(faults.is_empty(), "{side:?}: {faults:?}");
            } else {
                // The fake host lacks some game data the mod links: it turns
                // off with that reason, as on a game build without it.
                assert_eq!(status, Status::REFUSED, "{side:?}");
                assert!(!on, "{side:?}");
                assert_eq!(faults.len(), 1, "{side:?}: {faults:?}");
                assert!(!faults[0].contains("panic"), "{side:?}: {faults:?}");
            }
        }
    }
}

/// Links R11 on the server side and, on a client, on the client side.
fn entities_link(load: &mut Load<'_>, client: bool) {
    let world = match load.world() {
        Ok(world) => world,
        Err(off) => {
            enshwick::warn!("R11 off: {off}");
            return;
        }
    };
    match entities::link_server(load, world) {
        Ok(()) => enshwick::info!("R11 server sweeps every 5 s while players are in"),
        Err(off) => enshwick::warn!("R11 server side off: {off}"),
    }
    if client {
        match entities::link_client(load, world) {
            Ok(()) => enshwick::info!("R11 client sweeps every 5 s in a game"),
            Err(off) => enshwick::warn!("R11 client side off: {off}"),
        }
    }
}

/// Starts a new section of the log for `request`, looking for `values`.
fn mark(request: &Request<'_>, values: &[u32]) -> Outcome {
    let mark = tooltip::mark(values);
    enshwick::info!(
        "===== mark {mark} by {} (values looked for: {values:?}) =====",
        request.player.name
    );
    Outcome::done(format!("mark {mark}; hover the item now"))
}

/// The keys of the actions, as action and default key; F7 and up, which the
/// game leaves free.
const CONTROLS: [(&str, &str); 2] = [("mark", "F7"), ("status", "F8")];

/// Declares the research actions, for administrators.
fn actions(load: &mut Load<'_>) -> Result<Actions, Off> {
    let dir = load.mod_dir().ok();
    let actions = load.actions(Texts::default())?;
    actions.add(
        Action::new("mark", "Mark the log").admins(),
        move |request| {
            let values = dir
                .as_deref()
                .map_or_else(Vec::new, |dir| settings_in::<Settings>(dir).mark_values);
            mark(request, &values)
        },
    )?;
    actions.add(Action::new("status", "Research status").admins(), |_| {
        let (health, other) = health::counts();
        let impact = format!("; impact writes: {health} health, {other} other");
        Outcome::done(match tooltip::dumps() {
            Some((dumps, most)) => {
                format!("{dumps} of {most} tooltip dumps since the last mark{impact}")
            }
            None => format!("the tooltip is not looked at in this process{impact}"),
        })
    })?;
    Ok(actions)
}

enshwick::declare_mod!(Research);
