//! Actions on the whole server: the message of the day, restart warnings,
//! the time of day, the weather, saves, the status, the world's settings
//! and the logs; the tick that runs the schedule and the chat log.

use core::time::Duration;
use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::chat::{ChatKind, ChatLine};
use enshwick::config::{Config as _, Schema};
use enshwick::game_settings::GameSettings;
use enshwick::weather::Forecast;
use enshwick::{Off, Relock as _, WorldError};

use crate::schedule::Say;
use crate::settings::Settings;
use crate::{Tools, ids, journal, texts};

/// The weather an admin picks: its word, its label's key, and the forecast.
/// `Forecast` is the API's own list of what it starts.
const FORECASTS: [(&str, &str, Forecast); 4] = [
    ("clear", texts::WEATHER_CLEAR, Forecast::Clear),
    ("rain", texts::WEATHER_RAIN, Forecast::Rain),
    ("blizzard", texts::WEATHER_BLIZZARD, Forecast::Blizzard),
    ("any", texts::WEATHER_ANY, Forecast::Any),
];

/// The longest restart warning, in minutes.
const MAX_RESTART_MINUTES: f64 = 180.0;

/// The most lines one read of a log shows.
const MAX_LOG_LINES: f64 = 50.0;

/// Declares the actions on the server.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    tools.add(
        Action::new(ids::MOTD, texts::ACTION_MOTD).help(texts::ACTION_MOTD_HELP),
        motd,
    )?;
    tools.add(
        Action::new(ids::TIME, texts::ACTION_TIME)
            .help(texts::ACTION_TIME_HELP)
            .arg(Arg::number(texts::ARG_HOUR, 0.0..=24.0, 0.0)),
        time,
    )?;
    let kinds = FORECASTS.map(|(word, label, _)| (word, label));
    tools.add(
        Action::new(ids::WEATHER, texts::ACTION_WEATHER)
            .help(texts::ACTION_WEATHER_HELP)
            .arg(Arg::choice(texts::ARG_WEATHER, &kinds)),
        weather,
    )?;
    tools.add(
        Action::new(ids::RESTART, texts::ACTION_RESTART)
            .help(texts::ACTION_RESTART_HELP)
            .arg(Arg::number(
                texts::ARG_MINUTES,
                0.0..=MAX_RESTART_MINUTES,
                1.0,
            )),
        restart,
    )?;
    tools.add(
        Action::new(ids::AUDIT, texts::ACTION_AUDIT)
            .help(texts::ACTION_AUDIT_HELP)
            .arg(Arg::number(texts::ARG_LINES, 1.0..=MAX_LOG_LINES, 1.0)),
        read_audit,
    )?;
    tools.add(
        Action::new(ids::CHAT, texts::ACTION_CHAT)
            .help(texts::ACTION_CHAT_HELP)
            .arg(Arg::number(texts::ARG_LINES, 1.0..=MAX_LOG_LINES, 1.0)),
        read_chat,
    )?;
    tools.add(
        Action::new(ids::STATUS, texts::ACTION_STATUS).help(texts::ACTION_STATUS_HELP),
        status,
    )?;
    tools.add(
        Action::new(ids::SAVE, texts::ACTION_SAVE).help(texts::ACTION_SAVE_HELP),
        save,
    )?;
    tools.add(
        Action::new(ids::WORLD, texts::ACTION_WORLD).help(texts::ACTION_WORLD_HELP),
        world,
    )
}

/// The world's message of the day.
fn motd(tools: &Tools, request: &Request<'_>) -> Outcome {
    let settings = tools.settings.load();
    if settings.motd.is_empty() {
        return Outcome::done(request.say(texts::REPLY_MOTD_NONE, &[]));
    }
    Outcome::done(settings.motd.clone())
}

/// Sets the time of day, in hours.
fn time(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Number(hours)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_BAD_HOUR, &[]));
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an hour of the day fits an f32"
    )]
    let set = tools.clock.set_hours(*hours as f32);
    match set {
        Ok(()) => Outcome::done(request.say(
            texts::REPLY_TIME,
            &[(texts::value::TIME, &clock_text(*hours))],
        )),
        Err(why) => Tools::failed(request, ids::TIME, &why),
    }
}

/// `hours` as `HH:MM`, taken modulo a day.
fn clock_text(hours: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "minutes of one day, rounded, after rem_euclid made them positive"
    )]
    let minutes = (hours.rem_euclid(24.0) * 60.0).round() as u32 % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Starts the weather the admin picked.
fn weather(tools: &Tools, request: &Request<'_>) -> Outcome {
    let Some(&(_, label, forecast)) = request.args.first().and_then(|value| match value {
        Value::Choice(index) => FORECASTS.get(*index),
        _ => None,
    }) else {
        return Outcome::refused(request.say(texts::REPLY_BAD_WEATHER, &[]));
    };
    match tools.weather.set(forecast, None) {
        Ok(()) => {
            let kind = request.say(label, &[]);
            Outcome::done(request.say(texts::REPLY_WEATHER, &[(texts::value::WEATHER, &kind)]))
        }
        Err(why) => Tools::failed(request, ids::WEATHER, &why),
    }
}

/// Warns every player of a restart in the given minutes, or with 0 stops
/// the warning under way.
fn restart(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Number(minutes)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_RESTART_NONE, &[]));
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the argument's range is 0 to 180 in whole steps"
    )]
    let minutes = minutes.round() as u32;
    if minutes == 0 {
        if tools.ticker.relock().cancel() {
            tools
                .world
                .notify_all(&tools.say(texts::NOTICE_RESTART_CANCELLED, &[]));
            return Outcome::done(request.say(texts::REPLY_RESTART_CANCELLED, &[]));
        }
        return Outcome::refused(request.say(texts::REPLY_RESTART_NONE, &[]));
    }
    tools.ticker.relock().warn(minutes);
    tools.announce(&Say::RestartIn(minutes));
    let minutes = minutes.to_string();
    Outcome::done(request.say(texts::REPLY_RESTART, &[(texts::value::MINUTES, &minutes)]))
}

/// The last lines of the audit log.
fn read_audit(tools: &Tools, request: &Request<'_>) -> Outcome {
    let on = tools.settings.load().audit_on(tools.dedicated);
    read_log(tools, request, journal::AUDIT, on, texts::REPLY_AUDIT_OFF)
}

/// The last lines of the chat log.
fn read_chat(tools: &Tools, request: &Request<'_>) -> Outcome {
    let on = tools.settings.load().chat_log_on(tools.dedicated);
    read_log(tools, request, journal::CHAT, on, texts::REPLY_CHAT_OFF)
}

/// The last lines of the log `files`, refused with the text `off` while
/// the world's settings keep it off.
fn read_log(
    tools: &Tools,
    request: &Request<'_>,
    files: journal::Files,
    on: bool,
    off: &str,
) -> Outcome {
    let [Value::Number(count)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_LOG_NONE, &[]));
    };
    if !on {
        return Outcome::refused(request.say(off, &[]));
    }
    let Some(dir) = tools.world.data_dir() else {
        return Outcome::refused(request.say(texts::REPLY_LOG_NONE, &[]));
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the argument's range is 1 to 50 in whole steps"
    )]
    let count = count.round() as usize;
    match files.tail(&dir, count) {
        Ok(lines) if lines.is_empty() => Outcome::done(request.say(texts::REPLY_LOG_NONE, &[])),
        Ok(lines) => Outcome::done(lines.join("\n")),
        Err(why) => Tools::failed(request, files.now, &why),
    }
}

/// The loader's and every mod's version, the uptime and the world.
fn status(tools: &Tools, request: &Request<'_>) -> Outcome {
    let (hours, minutes) = hours_minutes(tools.started.elapsed().as_secs());
    let world = tools
        .world
        .key()
        .unwrap_or_else(|| request.say(texts::REPLY_STATUS_NO_WORLD, &[]));
    let mods: Vec<String> = enshwick::mods::all()
        .iter()
        .map(|entry| {
            let key = if entry.running {
                texts::REPLY_STATUS_MOD
            } else {
                texts::REPLY_STATUS_MOD_OFF
            };
            request.say(
                key,
                &[
                    (texts::value::NAME, &entry.label()),
                    (texts::value::VERSION, &entry.version),
                ],
            )
        })
        .collect();
    Outcome::done(request.say(
        texts::REPLY_STATUS,
        &[
            (texts::value::HOURS, &hours.to_string()),
            (texts::value::MINUTES, &minutes.to_string()),
            (texts::value::WORLD, &world),
            (texts::value::MODS, &mods.join(", ")),
        ],
    ))
}

/// `seconds` as whole hours and the minutes past them.
fn hours_minutes(seconds: u64) -> (u64, u64) {
    let minutes = seconds / 60;
    (minutes / 60, minutes % 60)
}

/// Saves the world now.
fn save(tools: &Tools, request: &Request<'_>) -> Outcome {
    match tools.world.save() {
        Ok(()) => Outcome::done(request.say(texts::REPLY_SAVED, &[])),
        Err(WorldError::Refused) => Outcome::refused(request.say(texts::REPLY_SAVE_REFUSED, &[])),
        Err(why) => Tools::failed(request, ids::SAVE, &why),
    }
}

/// The world, its last save and its game settings, one per line.
fn world(tools: &Tools, request: &Request<'_>) -> Outcome {
    let Some((settings, preset)) = tools.game_settings.as_ref().and_then(GameSettings::read) else {
        return Outcome::refused(request.say(texts::REPLY_WORLD_NONE, &[]));
    };
    let saved = tools.world.save_info().map_or_else(
        || request.say(texts::REPLY_WORLD_UNSAVED, &[]),
        |info| journal::utc(info.saved_at.as_secs()),
    );
    let world = tools
        .world
        .key()
        .unwrap_or_else(|| request.say(texts::REPLY_STATUS_NO_WORLD, &[]));
    let head = request.say(
        texts::REPLY_WORLD,
        &[
            (texts::value::WORLD, &world),
            (texts::value::PRESET, &format!("{preset:?}")),
            (texts::value::TIME, &saved),
        ],
    );
    Outcome::done(
        core::iter::once(head)
            .chain(fields(&format!("{settings:#?}")))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// The `name = value` lines of a struct's pretty `Debug` text.
fn fields(debug: &str) -> impl Iterator<Item = String> {
    debug
        .lines()
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|line| !line.starts_with(' '))
        .map(|line| line.trim_end_matches(',').replacen(": ", " = ", 1))
}

impl Tools {
    /// Writes a line the server's chat heard to the chat log while the
    /// world's settings keep it on: the sender's name and account, what
    /// the line is and its text.
    pub(crate) fn heard(&self, line: &ChatLine) {
        if !self.settings.load().chat_log_on(self.dedicated) {
            return;
        }
        let Some(dir) = self.world.data_dir() else {
            return;
        };
        let (name, account) = line.player.map_or_else(Default::default, |key| {
            let name = self.world.player_by_key(key).map(|player| player.name);
            (name.unwrap_or_default(), format!("{:016x}", key.0))
        });
        let kind = match line.kind {
            ChatKind::Player => "say",
            ChatKind::Emote => "emote",
            _ => "notice",
        };
        self.chat.record(dir, &[&name, &account, kind, &line.text]);
    }
}

impl Tools {
    /// Moves the schedule and a restart warning on by `elapsed`.
    pub(crate) fn tick(&self, elapsed: Duration) {
        let settings = self.settings.load();
        let every = Duration::from_secs(u64::from(settings.announce_minutes) * 60);
        let said = self
            .ticker
            .relock()
            .step(elapsed, &settings.announcements, every);
        for say in &said {
            self.announce(say);
        }
    }

    /// Shows `say` to every player, as a notice where the client runs
    /// Enshwick and as a chat line elsewhere.
    pub(crate) fn announce(&self, say: &Say) {
        let restart_text = &self.settings.load().restart_text;
        let with_text = |line: String| {
            if restart_text.is_empty() {
                line
            } else {
                format!("{line} {restart_text}")
            }
        };
        let line = match say {
            Say::Announcement(text) => text.clone(),
            Say::RestartIn(minutes) => with_text(self.say(
                texts::NOTICE_RESTART,
                &[(texts::value::MINUTES, &minutes.to_string())],
            )),
            Say::RestartNow => {
                // The save the restart would make, so the host's restart loses nothing.
                if let Err(why) = self.world.save() {
                    enshwick::warn!("no save before the restart: {why}");
                }
                with_text(self.say(texts::NOTICE_RESTART_NOW, &[]))
            }
        };
        enshwick::info!("announced: {line}");
        self.world.notify_all(&line);
    }
}

/// The settings `new` changes from `old`, as `key = value`, dotted for a
/// group's; a secret setting by its key alone.
pub(crate) fn changed_keys(old: &Settings, new: &Settings) -> Vec<String> {
    let (Ok(old), Ok(new)) = (toml::Table::try_from(old), toml::Table::try_from(new)) else {
        return Vec::new();
    };
    let schema = Settings::schema();
    let mut changed = Vec::new();
    diff("", &old, &new, &schema, &mut changed);
    changed
}

/// Adds to `changed` each key of `new` whose value differs in `old`.
fn diff(
    prefix: &str,
    old: &toml::Table,
    new: &toml::Table,
    schema: &Schema,
    changed: &mut Vec<String>,
) {
    for (key, value) in new {
        let full = format!("{prefix}{key}");
        match (old.get(key), value) {
            (Some(toml::Value::Table(old)), toml::Value::Table(new)) => {
                diff(&format!("{full}."), old, new, schema, changed);
            }
            (Some(was), now) if was == now => {}
            _ if schema.field(&full).is_some_and(|field| field.secret) => changed.push(full),
            (_, now) => changed.push(format!("{full} = {now}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_read_as_a_clock_modulo_a_day() {
        assert_eq!(clock_text(6.5), "06:30");
        assert_eq!(clock_text(0.0), "00:00");
        assert_eq!(clock_text(24.0), "00:00");
        assert_eq!(clock_text(23.999), "00:00");
        assert_eq!(clock_text(13.25), "13:15");
    }

    #[test]
    fn the_settings_diff_names_each_changed_key_and_group_key() {
        let old = Settings::default();
        let mut new = old.clone();
        new.motd = "Welcome".into();
        new.rights.kick = "nobody".into();
        assert_eq!(
            changed_keys(&old, &new),
            ["motd = \"Welcome\"", "rights.kick = \"nobody\""]
        );
        assert!(changed_keys(&old, &old).is_empty());
    }

    #[test]
    fn the_uptime_reads_as_whole_hours_and_the_minutes_past_them() {
        assert_eq!(hours_minutes(0), (0, 0));
        assert_eq!(hours_minutes(59), (0, 0));
        assert_eq!(hours_minutes(3_599), (0, 59));
        assert_eq!(hours_minutes(3_600 * 26 + 125), (26, 2));
    }

    #[test]
    fn the_world_view_takes_each_top_level_field_as_name_and_value() {
        #[derive(Debug)]
        #[expect(dead_code, reason = "the fields are read through Debug alone")]
        struct Shown {
            factor: f32,
            on: bool,
            length: Duration,
        }
        let shown = Shown {
            factor: 2.0,
            on: true,
            length: Duration::from_secs(600),
        };
        let lines: Vec<String> = fields(&format!("{shown:#?}")).collect();
        assert_eq!(lines, ["factor = 2.0", "on = true", "length = 600s"]);
    }

    #[test]
    fn a_changed_secret_is_named_by_its_key_and_never_its_value() {
        let old = Settings::default();
        let new = Settings {
            rcon_password: "hunter2".into(),
            ..old.clone()
        };
        assert_eq!(changed_keys(&old, &new), ["rcon_password"]);
    }
}
