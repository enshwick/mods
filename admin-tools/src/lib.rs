//! Admin Tools: what a server's administrators need in game. The player
//! list, kicks, bans, the allow-list and chat mutes; the message of the
//! day, scheduled announcements and restart warnings; the time of day and
//! the weather; healing and teleports; who may use each, per world; and an
//! audit log of what was done.
//!
//! Every action also runs as a command, `/admin-tools <action>` or
//! `/adl <action>`, in chat, the server's console and remote consoles.

mod accounts;
mod audit;
mod ids;
mod players;
mod schedule;
mod server;
mod settings;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use arc_swap::ArcSwap;
use enshwick::abi::Side;
use enshwick::actions::{Action, Actions, Outcome, Request, Value, Via};
use enshwick::daytime::Clock;
use enshwick::effects::Effects;
use enshwick::events::{PlayerJoined, PlayerLeft};
use enshwick::i18n::{Language, Texts};
use enshwick::state::WorldStore;
use enshwick::weather::Weather;
use enshwick::world::World;
use enshwick::{Load, Mod, Off, PlayerKey, Relock as _};
use rustc_hash::FxHashMap;

use accounts::Accounts;
use audit::Audit;
use schedule::Ticker;
use settings::Settings;

enshwick::texts!();

/// How often the schedule and a restart warning move on.
const TICK: core::time::Duration = core::time::Duration::from_secs(1);

/// The mod; its state lives in [`Tools`], which its handlers share.
#[derive(Debug)]
pub struct AdminTools;

/// What the actions, the join check and the schedule share.
pub(crate) struct Tools {
    world: World,
    effects: Effects,
    clock: Clock,
    weather: Weather,
    texts: Texts,
    actions: Actions,
    settings: ArcSwap<Settings>,
    accounts: WorldStore<Accounts>,
    audit: Audit,
    ticker: Mutex<Ticker>,
    /// When each player online joined, for the time online.
    joined: Mutex<FxHashMap<PlayerKey, Instant>>,
    /// Whether this is a dedicated server, for the audit log's default.
    dedicated: bool,
}

/// A handler of one of the mod's actions.
type Handler = fn(&Tools, &Request<'_>) -> Outcome;

impl Mod for AdminTools {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = load.texts()?;
        let tools = Arc::new(Tools {
            world: load.world()?,
            effects: load.effects(),
            clock: load.clock(),
            weather: load.weather(),
            actions: load.actions(texts.clone())?,
            texts,
            settings: ArcSwap::from_pointee(Settings::default()),
            accounts: load.world_store(accounts::STORE)?,
            audit: Audit::new(),
            ticker: Mutex::new(Ticker::default()),
            joined: Mutex::new(FxHashMap::default()),
            dedicated: enshwick::host().is_some_and(|host| host.build().side == Side::SERVER),
        });
        players::add(&tools)?;
        server::add(&tools)?;
        tools.actions.commands(ids::COMMAND, ids::ALIAS)?;

        let changed = Arc::clone(&tools);
        load.on_settings_change(move |settings: &Settings| changed.apply(settings))?;
        let scheduler = load.scheduler()?;
        let joins = Arc::clone(&tools);
        load.on::<PlayerJoined>(move |joined| {
            let (tools, key) = (Arc::clone(&joins), joined.key);
            // Kicks and bans wait for the step to end.
            scheduler.next_tick(move || tools.welcome(key));
        })?;
        let leaves = Arc::clone(&tools);
        load.on::<PlayerLeft>(move |left| {
            leaves.joined.relock().remove(&left.key);
        })?;
        let ticks = Arc::clone(&tools);
        load.on_tick(TICK, move |tick| {
            ticks.tick(core::time::Duration::from_nanos(tick.elapsed_nanos));
        })?;
        Ok(Self)
    }
}

impl Tools {
    /// Declares `action` with `handler`: only players the world's rights
    /// admit see and run it, and a run that changed something goes to the
    /// audit log.
    fn add(self: &Arc<Self>, action: Action, handler: Handler) -> Result<(), Off> {
        let id = action.id().to_owned();
        let rules = Arc::clone(self);
        let rule_id = id.clone();
        let tools = Arc::clone(self);
        self.actions.add(
            action.allow(move |player| rules.settings.load().rights.admits(&rule_id, player)),
            move |request| {
                let outcome = handler(&tools, request);
                if outcome.is_done() && !ids::READS.contains(&id.as_str()) {
                    tools.record(request, &id, &outcome);
                }
                outcome
            },
        )
    }

    /// Writes a run of the action `id` to the audit log while it is on.
    fn record(&self, request: &Request<'_>, id: &str, outcome: &Outcome) {
        let who = format!("{} ({:016x})", request.player.name, request.player.key.0);
        let via = match request.via {
            Via::Interface => "interface",
            Via::Console => "console",
            Via::Chat => "chat",
            _ => "remote",
        };
        let args: Vec<String> = request.args.iter().map(word).collect();
        self.log(&[&who, via, id, &args.join(" "), outcome.text()]);
    }

    /// Writes `fields` to the audit log while the world's settings keep it
    /// on.
    fn log(&self, fields: &[&str]) {
        if !self.settings.load().audit_on(self.dedicated) {
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
        if old.rights != settings.rights
            && let Err(why) = self.actions.rules_changed()
        {
            enshwick::warn!("rights not carried again: {why}");
        }
        let changed = server::changed_keys(&old, settings);
        if !changed.is_empty() {
            self.log(&["settings", "", "settings", &changed.join(", "), ""]);
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

/// The language lines to every player are in: the server's own.
fn language() -> Language {
    enshwick::i18n::language()
}

/// A value as a console line writes it, for the audit log.
fn word(value: &Value) -> String {
    match value {
        Value::Player { name, .. } => name.clone(),
        Value::Choice(index) => index.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Toggle(on) => if *on { "on" } else { "off" }.to_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use enshwick::config::Config as _;
    use enshwick::i18n::{Language, Texts};

    use super::*;

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
