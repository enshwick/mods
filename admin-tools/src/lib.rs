//! Admin Tools: what a server's administrators need in game. The player
//! list, kicks, bans, the allow-list and chat mutes; the message of the
//! day, scheduled announcements and restart warnings; the time of day and
//! the weather; healing and teleports; items, chests of items and
//! inventories; health, stamina and mana, recipes and god mode; saves,
//! backups and the world's
//! settings; who may use each, per world; an audit log of what was done
//! and a chat log.
//!
//! Every action also runs as a command, `/admin-tools <action>` or
//! `/adm <action>`, in chat, the server's console and the mod's remote
//! console (Source RCON).

mod accounts;
mod backup;
mod character;
mod chest;
mod ids;
mod inventory;
mod items;
mod journal;
mod players;
mod rcon;
mod schedule;
mod server;
mod settings;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use arc_swap::ArcSwap;
use enshwick::abi::Side;
use enshwick::actions::{Action, Actions, Outcome, Request, Value, Via};
use enshwick::daytime::Clock;
use enshwick::effects::Effects;
use enshwick::events::{PlayerJoined, PlayerLeft, ShutdownStarted, WorldLoaded};
use enshwick::game_settings::GameSettings;
use enshwick::i18n::{Language, Texts};
use enshwick::inventory::Inventories;
use enshwick::knowledge::Knowledge;
use enshwick::schedule::Scheduler;
use enshwick::state::WorldStore;
use enshwick::weather::Weather;
use enshwick::world::World;
use enshwick::{Entity, Load, Mod, Off, PlayerKey, Relock as _};
use rustc_hash::FxHashMap;

use accounts::Accounts;
use journal::Journal;
use schedule::Ticker;
use settings::{Rules, Settings};

enshwick::texts!();

/// How often the schedule and a restart warning move on.
const TICK: core::time::Duration = core::time::Duration::from_secs(1);

/// The mod; its state lives in the handlers it declares, which share it.
#[derive(Debug)]
pub struct AdminTools;

/// What the actions, the join check and the schedule share.
pub(crate) struct Tools {
    /// The tools themselves, for work that outlives an action's run.
    me: Weak<Tools>,
    world: World,
    effects: Effects,
    clock: Clock,
    weather: Weather,
    knowledge: Knowledge,
    /// The inventories, `None` on a build without them.
    inventories: Option<Inventories>,
    /// The game settings, `None` on a build without them.
    game_settings: Option<GameSettings>,
    scheduler: Scheduler,
    texts: Texts,
    actions: Actions,
    settings: ArcSwap<Settings>,
    /// The rights of the world's settings, parsed once as they arrive.
    rules: ArcSwap<Rules>,
    accounts: WorldStore<Accounts>,
    audit: Journal,
    chat: Journal,
    ticker: Mutex<Ticker>,
    /// The players in god mode and their characters, read without a lock
    /// by the damage filter.
    gods: ArcSwap<Vec<(PlayerKey, Entity)>>,
    /// When each player online joined, for the time online.
    joined: Mutex<FxHashMap<PlayerKey, Instant>>,
    /// When the mod started, for the server's uptime.
    started: Instant,
    /// Whether a backup is under way.
    backing_up: AtomicBool,
    /// The remote console while it listens.
    rcon: Mutex<Option<rcon::Listener>>,
    /// Whether the remote console stopped after a panic, which keeps it
    /// off until the mod loads again.
    rcon_fused: AtomicBool,
    /// Whether this is a dedicated server, for the defaults of the switches
    /// and the restart.
    dedicated: bool,
    /// Whether a world runs, from its load to its shutdown.
    running: AtomicBool,
}

/// A handler of one of the mod's actions.
type Handler = fn(&Tools, &Request<'_>) -> Outcome;

impl Mod for AdminTools {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = load.texts()?;
        let (world, effects, clock, weather) =
            (load.world()?, load.effects(), load.clock(), load.weather());
        let game_settings = load
            .game_settings()
            .inspect_err(|why| enshwick::warn!("the world action is off: {why}"))
            .ok();
        let scheduler = load.scheduler()?;
        let knowledge = load.knowledge();
        let inventories = load
            .inventories()
            .inspect_err(|why| enshwick::warn!("the inventory actions are off: {why}"))
            .ok();
        let actions = load.actions(texts.clone())?;
        let accounts = load.world_store(accounts::STORE)?;
        let tools = Arc::new_cyclic(|me| Tools {
            me: Weak::clone(me),
            world,
            effects,
            clock,
            weather,
            knowledge,
            inventories,
            game_settings,
            scheduler,
            actions,
            texts,
            settings: ArcSwap::from_pointee(Settings::default()),
            rules: ArcSwap::from_pointee(Settings::default().rights.rules()),
            accounts,
            audit: Journal::new(journal::AUDIT),
            chat: Journal::new(journal::CHAT),
            ticker: Mutex::new(Ticker::default()),
            gods: ArcSwap::from_pointee(Vec::new()),
            joined: Mutex::new(FxHashMap::default()),
            started: Instant::now(),
            backing_up: AtomicBool::new(false),
            rcon: Mutex::new(None),
            rcon_fused: AtomicBool::new(false),
            dedicated: enshwick::host().is_some_and(|host| host.build().side == Side::SERVER),
            running: AtomicBool::new(false),
        });
        let loaded = Arc::clone(&tools);
        load.on::<WorldLoaded>(move |()| loaded.start())?;
        let stopped = Arc::clone(&tools);
        load.on::<ShutdownStarted>(move |()| stopped.stop())?;
        players::add(&tools)?;
        inventory::add(&tools)?;
        chest::add(&tools)?;
        character::add(&tools, load)?;
        server::add(&tools)?;
        backup::add(&tools)?;
        tools.actions.commands(ids::COMMAND, ids::ALIAS)?;

        let changed = Arc::clone(&tools);
        load.on_settings_change(move |settings: &Settings| changed.apply(settings))?;
        let heard = Arc::clone(&tools);
        load.on_chat_said(move |line| heard.heard(line))?;
        let joins = Arc::clone(&tools);
        load.on::<PlayerJoined>(move |joined| {
            let (tools, key) = (Arc::clone(&joins), joined.key);
            // Kicks and bans wait for the step to end.
            scheduler.next_tick(move || tools.welcome(key));
        })?;
        let leaves = Arc::clone(&tools);
        load.on::<PlayerLeft>(move |left| {
            leaves.joined.relock().remove(&left.key);
            leaves.set_god(left.key, None);
        })?;
        let ticks = Arc::clone(&tools);
        load.on_tick(TICK, move |tick| {
            ticks.tick(core::time::Duration::from_nanos(tick.elapsed_nanos));
            ticks.drop_stale_gods();
        })?;
        Ok(Self)
    }
}

impl Tools {
    /// Declares `action` with `handler`: only players the world's rights
    /// admit see and run it, and every run of an action that changes
    /// something goes to the audit log, refused or done.
    fn add(self: &Arc<Self>, action: Action, handler: Handler) -> Result<(), Off> {
        let id = action.id().to_owned();
        let rules = Arc::clone(self);
        let rule_id = id.clone();
        let tools = Arc::clone(self);
        // A hosted game has no restart to warn of.
        let offered = id != ids::RESTART || self.dedicated;
        self.actions.add(
            action.allow(move |player| offered && rules.rules.load().admits(&rule_id, player)),
            move |request| {
                let outcome = handler(&tools, request);
                // A refusal goes in too, so an administrator sees what was tried.
                if !ids::READS.contains(&id.as_str()) {
                    tools.record(request, &id, &outcome);
                }
                outcome
            },
        )
    }

    /// Starts the world that loaded: the remote console listens when the
    /// world's settings turn it on.
    fn start(&self) {
        self.running.store(true, Ordering::Relaxed);
        self.listen(&self.settings.load());
    }

    /// Ends the running game: a restart warning, the schedule and the times
    /// online do not carry into the next hosted game, the remote console
    /// closes, and the log lines still on their way are written before the
    /// process may exit.
    fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        *self.ticker.relock() = Ticker::default();
        self.joined.relock().clear();
        self.gods.store(Arc::new(Vec::new()));
        // A copy still waiting to start ends with the game's tasks; the
        // loader lets the saves go at shutdown.
        self.backing_up.store(false, Ordering::Release);
        if let Some(listener) = self.rcon.relock().take() {
            listener.close();
        }
        self.audit.finish();
        self.chat.finish();
    }

    /// Writes the ban list and allow-list now, on a thread of their own, so
    /// a change holds even when the server stops before the world saves.
    fn keep_accounts(&self) {
        let accounts = self.accounts.clone();
        let spawned = std::thread::Builder::new()
            .name("admin-tools accounts".into())
            .spawn(move || {
                if let Err(why) = accounts.flush() {
                    enshwick::warn!("accounts not written yet: {why}");
                }
            });
        if let Err(why) = spawned {
            enshwick::warn!("accounts written when the world saves: {why}");
        }
    }

    /// Writes a run of the action `id` to the audit log while it is on.
    fn record(&self, request: &Request<'_>, id: &str, outcome: &Outcome) {
        let (who, via, args) = run_of(request);
        let ended = journal::outcome(outcome.is_done());
        self.log(&[&who, via, id, &args, outcome.text(), ended]);
    }

    /// Writes `fields` to the audit log while the world's settings keep it
    /// on.
    fn log(&self, fields: &[&str]) {
        self.log_if(self.settings.load().audit_on(self.dedicated), fields);
    }

    /// Writes `fields` to the audit log when `on`.
    fn log_if(&self, on: bool, fields: &[&str]) {
        if !on {
            return;
        }
        if let Some(dir) = self.world.data_dir() {
            self.audit.record(dir, fields);
        }
    }

    /// Takes the world's settings; a change of rights re-carries what each
    /// player may run, and a change goes to the audit log.
    fn apply(&self, settings: &Settings) {
        let old = self.settings.swap(Arc::new(settings.clone()));
        if old.rights != settings.rights {
            self.rules.store(Arc::new(settings.rights.rules()));
            if let Err(why) = self.actions.rules_changed() {
                enshwick::warn!("rights not carried again: {why}");
            }
        }
        // A world's first settings arrive before it runs: no one changed them.
        if !self.running.load(Ordering::Relaxed) {
            return;
        }
        self.listen(settings);
        let changed = server::changed_keys(&old, settings);
        // The change that turns the log off is its last line.
        let on = old.audit_on(self.dedicated) || settings.audit_on(self.dedicated);
        if !changed.is_empty() {
            self.log_if(on, &["settings", "", "settings", &changed.join(", "), ""]);
        }
    }

    /// The mod's text `key` in the server's language, for lines every player
    /// reads at once.
    fn say(&self, key: &str, values: &[(&str, &str)]) -> String {
        self.texts.fill(key, language(), values)
    }

    /// The refusal for a change the game would not make, with its reason.
    fn failed(request: &Request<'_>, name: &str, why: &dyn core::fmt::Display) -> Outcome {
        let why = why.to_string();
        Outcome::refused(request.say(
            texts::REPLY_FAILED,
            &[(texts::value::NAME, name), (texts::value::WHY, &why)],
        ))
    }
}

/// The end of a change the game runs after its action replied: the caller
/// reads it in chat and the audit log keeps it, since the reply came before
/// the game ran the change and a console or remote caller hears no chat.
pub(crate) struct Late {
    tools: Arc<Tools>,
    caller: enshwick::world::Player,
    language: Language,
    id: &'static str,
    /// Who ran the action, from where and with which values.
    run: (String, &'static str, String),
}

impl Late {
    /// The late end of the run `request` of the action `id`; `None` once
    /// the mod is gone.
    pub(crate) fn new(tools: &Tools, request: &Request<'_>, id: &'static str) -> Option<Arc<Self>> {
        Some(Arc::new(Self {
            tools: tools.me.upgrade()?,
            caller: request.player.clone(),
            language: request.language,
            id,
            run: run_of(request),
        }))
    }

    /// Tells the caller the text `key` and writes it to the audit log: as a
    /// change made in part when `done`, else as one the game turned down.
    pub(crate) fn report(&self, done: bool, key: &str, values: &[(&str, &str)]) {
        let line = self.tools.texts.fill(key, self.language, values);
        self.tools.world.tell(&self.caller, &line);
        let (who, via, args) = &self.run;
        let ended = journal::outcome(done);
        self.tools.log(&[who, via, self.id, args, &line, ended]);
    }
}

/// Who ran an action, from where and with which values, as the audit log
/// writes them.
fn run_of(request: &Request<'_>) -> (String, &'static str, String) {
    let via = match request.via {
        Via::Interface => "interface",
        Via::Console => "console",
        Via::Chat => "chat",
        _ => "remote",
    };
    let args: Vec<String> = request.args.iter().map(word).collect();
    let who = account(&request.player.name, request.player.key);
    (who, via, args.join(" "))
}

/// Between the lines of a reply.
pub(crate) const LINE: &str = "\n";

/// `parts` as one reply: a line each in chat, the console and a remote
/// console, and one line in the interface, whose notice shows only one.
pub(crate) fn lines(request: &Request<'_>, parts: &[String]) -> String {
    let between = if matches!(request.via, Via::Interface) {
        " "
    } else {
        LINE
    };
    parts.join(between)
}

/// `number` as the whole number an action's argument stands for, or the
/// refusal of one with a fraction: the loader checks an argument's range,
/// never its step.
pub(crate) fn whole(request: &Request<'_>, number: f64) -> Result<u32, Outcome> {
    if number.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&number) {
        let shown = number.to_string();
        return Err(Outcome::refused(
            request.say(texts::REPLY_NOT_WHOLE, &[(texts::value::VALUE, &shown)]),
        ));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number in 0..=u32::MAX, checked above"
    )]
    Ok(number as u32)
}

/// The language lines to every player are in: the server's own.
fn language() -> Language {
    enshwick::i18n::language()
}

/// An account as the audit log and the server's log name it: the player's
/// name and the key.
fn account(name: &str, key: PlayerKey) -> String {
    format!("{name} ({})", account_id(key))
}

/// An account's id as players, the logs and the ban file read it: 16 hex
/// digits.
pub(crate) fn account_id(key: PlayerKey) -> String {
    format!("{:016x}", key.0)
}

/// A value as a console line writes it, for the audit log.
fn word(value: &Value) -> String {
    match value {
        Value::Player { name, .. } => name.clone(),
        Value::Choice(index) => index.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Toggle(on) => if *on { "on" } else { "off" }.to_owned(),
        Value::Text(text) => text.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use enshwick::config::Config as _;
    use enshwick::i18n::{Language, Texts};

    use super::*;

    #[test]
    fn the_audit_log_writes_a_text_argument_as_typed() {
        assert_eq!(word(&Value::Text("Stone Axe".to_owned())), "Stone Axe");
    }

    #[test]
    fn every_text_exists_in_every_required_language() {
        let texts = Texts::parse(texts::TOML).unwrap();
        assert_eq!(texts.missing_in(Language::REQUIRED), []);
    }

    #[test]
    fn every_setting_and_group_has_its_label_and_description() {
        let schema = Settings::schema();
        let tables: toml::Table = toml::from_str(texts::TOML).unwrap();
        let keys = schema
            .groups
            .iter()
            .map(|group| group.text.clone())
            .chain(schema.fields.iter().map(|field| field.text.clone()))
            .flat_map(|key| [format!("{key}.doc"), key]);
        for key in keys {
            for (language, table) in &tables {
                assert!(
                    table.as_table().unwrap().contains_key(&key),
                    "{language}: {key}"
                );
            }
        }
    }

    #[test]
    fn every_action_s_label_and_help_are_texts() {
        let texts = Texts::parse(texts::TOML).unwrap();
        for id in ids::ALL {
            for key in [format!("action.{id}"), format!("action.{id}.help")] {
                assert!(texts.get(&key, Language::FALLBACK).is_some(), "{key}");
            }
        }
    }
}

enshwick::declare_mod!(AdminTools);
