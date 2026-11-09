//! Actions on the whole server: the message of the day, restart warnings,
//! the time of day, the weather and the audit log; and the tick that runs
//! the schedule.

use core::time::Duration;
use std::sync::Arc;

use enshwick::Off;
use enshwick::Relock as _;
use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::weather::Forecast;

use crate::schedule::Say;
use crate::settings::Settings;
use crate::{Tools, audit, ids, texts};

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

/// The most audit lines one read shows.
const MAX_AUDIT_LINES: f64 = 50.0;

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
            .arg(Arg::number(texts::ARG_LINES, 1.0..=MAX_AUDIT_LINES, 1.0)),
        read_audit,
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
    let [Value::Number(count)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_AUDIT_NONE, &[]));
    };
    if !tools.settings.load().audit_on(tools.dedicated) {
        return Outcome::refused(request.say(texts::REPLY_AUDIT_OFF, &[]));
    }
    let Some(dir) = tools.world.data_dir() else {
        return Outcome::refused(request.say(texts::REPLY_AUDIT_NONE, &[]));
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the argument's range is 1 to 50 in whole steps"
    )]
    let count = count.round() as usize;
    match audit::tail(&dir, count) {
        Ok(lines) if lines.is_empty() => Outcome::done(request.say(texts::REPLY_AUDIT_NONE, &[])),
        Ok(lines) => Outcome::done(lines.join("\n")),
        Err(why) => Tools::failed(request, audit::FILE, &why),
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
            Say::RestartNow => with_text(self.say(texts::NOTICE_RESTART_NOW, &[])),
        };
        enshwick::info!("announced: {line}");
        self.world.notify_all(&line);
    }
}

/// The settings `new` changes from `old`, as `key = value`, dotted for a
/// group's.
pub(crate) fn changed_keys(old: &Settings, new: &Settings) -> Vec<String> {
    let (Ok(old), Ok(new)) = (toml::Table::try_from(old), toml::Table::try_from(new)) else {
        return Vec::new();
    };
    let mut changed = Vec::new();
    diff("", &old, &new, &mut changed);
    changed
}

/// Adds to `changed` each key of `new` whose value differs in `old`.
fn diff(prefix: &str, old: &toml::Table, new: &toml::Table, changed: &mut Vec<String>) {
    for (key, value) in new {
        match (old.get(key), value) {
            (Some(toml::Value::Table(old)), toml::Value::Table(new)) => {
                diff(&format!("{prefix}{key}."), old, new, changed);
            }
            (Some(was), now) if was == now => {}
            (_, now) => changed.push(format!("{prefix}{key} = {now}")),
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
}
