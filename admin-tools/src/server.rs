//! Actions on the whole server: the message of the day, restart warnings,
//! the time of day, the weather, saves, the status, the world's settings
//! and the logs; the tick that runs the schedule and the chat log.

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::chat::{ChatKind, ChatLine};
use enshwick::config::{Config as _, Schema};
use enshwick::game_settings::{GameSettings, Settings as Game};
use enshwick::weather::Forecast;
use enshwick::{Off, Relock as _, WorldError};

use crate::schedule::Say;
use crate::settings::Settings;
use crate::{Tools, ids, journal, texts};

/// The weather an admin picks: its word, its label's key, and the forecast.
/// `Forecast` is the API's own list of what it starts.
pub(crate) const FORECASTS: [(&str, &str, Forecast); 4] = [
    ("clear", texts::WEATHER_CLEAR, Forecast::Clear),
    ("rain", texts::WEATHER_RAIN, Forecast::Rain),
    ("blizzard", texts::WEATHER_BLIZZARD, Forecast::Blizzard),
    ("any", texts::WEATHER_ANY, Forecast::Any),
];

/// The text that, set as the message of the day, takes the in-game one
/// away: a text argument is never empty.
pub(crate) const NO_MOTD: &str = "-";

/// The most bytes a time of day takes, `hh:mm` or hours with a fraction.
const MAX_TIME_TEXT: usize = 8;

/// The longest restart warning, in minutes.
const MAX_RESTART_MINUTES: f64 = 180.0;

/// The most lines one read of a log shows.
const MAX_LOG_LINES: f64 = 50.0;

/// The ticks a restart whose warning ran out tries to end: the first asks
/// the game to stop, and when it does not, each asks for the save the stop
/// would have made while the game refuses it.
const RESTART_TRIES: u32 = 30;

/// Declares the actions on the server.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    tools.add(
        Action::new(ids::MOTD, texts::ACTION_MOTD).help(texts::ACTION_MOTD_HELP),
        motd,
    )?;
    tools.add(
        Action::new(ids::SET_MOTD, texts::ACTION_SETMOTD)
            .help(texts::ACTION_SETMOTD_HELP)
            .arg(Arg::text(texts::ARG_MESSAGE, crate::settings::MAX_TEXT)),
        set_motd,
    )?;
    tools.add(
        Action::new(ids::SAY, texts::ACTION_SAY)
            .help(texts::ACTION_SAY_HELP)
            .arg(Arg::text(texts::ARG_MESSAGE, crate::settings::MAX_TEXT)),
        say,
    )?;
    tools.add(
        Action::new(ids::TIME, texts::ACTION_TIME)
            .help(texts::ACTION_TIME_HELP)
            .arg(Arg::text(texts::ARG_TIME, MAX_TIME_TEXT)),
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
    let motd = tools.motd();
    if motd.is_empty() {
        return Outcome::done(request.say(texts::REPLY_MOTD_NONE, &[]));
    }
    Outcome::done(motd)
}

/// Sets the message of the day in the world's data, where it holds over
/// the world's setting; [`NO_MOTD`] goes back to the setting's.
fn set_motd(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Text(text)] = request.args else {
        return crate::players::bad_args(request);
    };
    let motd = if text == NO_MOTD { "" } else { text.as_str() };
    if tools
        .accounts
        .update(|accounts| motd.clone_into(&mut accounts.motd))
        .is_none()
    {
        return crate::players::store_shut(tools, request);
    }
    tools.keep_accounts();
    if motd.is_empty() {
        return Outcome::done(request.say(texts::REPLY_MOTD_CLEARED, &[]));
    }
    Outcome::done(request.say(texts::REPLY_MOTD_SET, &[(texts::value::TEXT, motd)]))
}

/// Tells every player a line now.
fn say(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Text(text)] = request.args else {
        return crate::players::bad_args(request);
    };
    tools.announce(&Say::Announcement(text.clone()));
    let count = tools.world.players().len().to_string();
    Outcome::done(request.say(texts::REPLY_SAID, &[(texts::value::COUNT, &count)]))
}

/// The hours a time of day reads as: `18:30`, or hours from 0 to 24 such
/// as `6.5`.
pub(crate) fn hours_of(text: &str) -> Option<f64> {
    let hours = match text.split_once(':') {
        Some((hours, minutes)) => {
            let minutes = minutes.parse::<u8>().ok().filter(|minutes| *minutes < 60)?;
            f64::from(hours.parse::<u8>().ok()?) + f64::from(minutes) / 60.0
        }
        None => text.parse::<f64>().ok()?,
    };
    (0.0..=24.0).contains(&hours).then_some(hours)
}

/// Sets the time of day.
fn time(tools: &Tools, request: &Request<'_>) -> Outcome {
    let Some(hours) = request.args.first().and_then(|value| match value {
        Value::Text(text) => hours_of(text),
        _ => None,
    }) else {
        return Outcome::refused(request.say(texts::REPLY_BAD_HOUR, &[]));
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an hour of the day fits an f32"
    )]
    let set = tools.clock.set_hours(hours as f32);
    match set {
        Ok(()) => Outcome::done(request.say(
            texts::REPLY_TIME,
            &[(texts::value::TIME, &clock_text(hours))],
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
        return crate::players::bad_args(request);
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
        return crate::players::bad_args(request);
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
            (texts::value::MODS, &mods.join(crate::LINE)),
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
        |info| {
            request.say(
                texts::REPLY_WORLD_SAVED,
                &[(texts::value::TIME, &journal::utc(info.saved_at.as_secs()))],
            )
        },
    );
    let world = tools
        .world
        .key()
        .unwrap_or_else(|| request.say(texts::REPLY_STATUS_NO_WORLD, &[]));
    let head = request.say(
        texts::REPLY_WORLD,
        &[
            (texts::value::WORLD, &world),
            (texts::value::PRESET, &kind(request, PRESET, &preset)),
            (texts::value::TIME, &saved),
        ],
    );
    let lines = GAME_SETTINGS.iter().map(|(name, value)| {
        request.say(
            texts::REPLY_WORLD_ROW,
            &[
                (texts::value::NAME, &request.say(name, &[])),
                (texts::value::VALUE, &value(&settings, request)),
            ],
        )
    });
    Outcome::done(
        core::iter::once(head)
            .chain(lines)
            .collect::<Vec<_>>()
            .join(crate::LINE),
    )
}

/// A game setting the world view shows: its name's text key and its value
/// as the reader reads it.
type Shown = (&'static str, fn(&Game, &Request<'_>) -> String);

/// The text key the preset's kinds start with.
const PRESET: &str = "world.preset";

/// Every game setting the world view shows, in the API's order, named as
/// the game's world settings name them.
const GAME_SETTINGS: [Shown; 38] = [
    (texts::WORLD_PLAYER_HEALTH, |s, _| {
        percent(s.player_health_factor)
    }),
    (texts::WORLD_PLAYER_MANA, |s, _| {
        percent(s.player_mana_factor)
    }),
    (texts::WORLD_PLAYER_STAMINA, |s, _| {
        percent(s.player_stamina_factor)
    }),
    (texts::WORLD_BODY_HEAT, |s, _| {
        percent(s.player_body_heat_factor)
    }),
    (texts::WORLD_DIVING_TIME, |s, _| {
        percent(s.player_diving_time_factor)
    }),
    (texts::WORLD_DURABILITY, |s, r| {
        switch(r, s.enable_durability)
    }),
    (texts::WORLD_STARVATION, |s, r| {
        switch(r, s.enable_starving_debuff)
    }),
    (texts::WORLD_FOOD_BUFF_DURATION, |s, _| {
        percent(s.food_buff_duration_factor)
    }),
    (texts::WORLD_TIME_UNTIL_STARVATION, |s, r| {
        length(r, s.from_hunger_to_starving)
    }),
    (texts::WORLD_SHROUD_TIME, |s, _| {
        percent(s.shroud_time_factor)
    }),
    (texts::WORLD_TOMBSTONE_MODE, |s, r| {
        kind(r, texts::WORLD_TOMBSTONE_MODE, &s.tombstone_mode)
    }),
    (texts::WORLD_GLIDER_TURBULENCES, |s, r| {
        switch(r, s.enable_glider_turbulences)
    }),
    (texts::WORLD_WEATHER_FREQUENCY, |s, r| {
        kind(r, texts::WORLD_WEATHER_FREQUENCY, &s.weather_frequency)
    }),
    (texts::WORLD_FISHING_DIFFICULTY, |s, r| {
        kind(r, texts::WORLD_FISHING_DIFFICULTY, &s.fishing_difficulty)
    }),
    (texts::WORLD_MINING_DAMAGE, |s, _| {
        percent(s.mining_damage_factor)
    }),
    (texts::WORLD_PLANT_GROWTH_SPEED, |s, _| {
        percent(s.plant_growth_speed_factor)
    }),
    (texts::WORLD_LOOT_AMOUNT, |s, _| {
        percent(s.resource_drop_stack_amount_factor)
    }),
    (texts::WORLD_WORKSHOP_SPEED, |s, _| {
        percent(s.factory_production_speed_factor)
    }),
    (texts::WORLD_UPGRADE_RECYCLING, |s, _| {
        percent(s.perk_upgrade_recycling_factor)
    }),
    (texts::WORLD_UPGRADE_COST, |s, _| {
        percent(s.perk_cost_factor)
    }),
    (texts::WORLD_COMBAT_XP, |s, _| {
        percent(s.experience_combat_factor)
    }),
    (texts::WORLD_MINING_XP, |s, _| {
        percent(s.experience_mining_factor)
    }),
    (texts::WORLD_QUEST_XP, |s, _| {
        percent(s.experience_exploration_quests_factor)
    }),
    (texts::WORLD_ENEMY_AMOUNT, |s, r| {
        kind(r, texts::WORLD_ENEMY_AMOUNT, &s.random_spawner_amount)
    }),
    (texts::WORLD_ENEMY_ATTACKS, |s, r| {
        kind(r, texts::WORLD_ENEMY_ATTACKS, &s.aggro_pool_amount)
    }),
    (texts::WORLD_ENEMY_DAMAGE, |s, _| {
        percent(s.enemy_damage_factor)
    }),
    (texts::WORLD_ENEMY_HEALTH, |s, _| {
        percent(s.enemy_health_factor)
    }),
    (texts::WORLD_ENEMY_STAMINA, |s, _| {
        percent(s.enemy_stamina_factor)
    }),
    (texts::WORLD_ENEMY_PERCEPTION, |s, _| {
        percent(s.enemy_perception_range_factor)
    }),
    (texts::WORLD_BOSS_DAMAGE, |s, _| {
        percent(s.boss_damage_factor)
    }),
    (texts::WORLD_BOSS_HEALTH, |s, _| {
        percent(s.boss_health_factor)
    }),
    (texts::WORLD_THREAT_BONUS, |s, _| number(s.threat_bonus)),
    (texts::WORLD_PACIFY, |s, r| switch(r, s.pacify_all_enemies)),
    (texts::WORLD_TAMING_STARTLE, |s, r| {
        kind(
            r,
            texts::WORLD_TAMING_STARTLE,
            &s.taming_startle_repercussion,
        )
    }),
    (texts::WORLD_DAY_LENGTH, |s, r| {
        length(r, s.day_time_duration)
    }),
    (texts::WORLD_NIGHT_LENGTH, |s, r| {
        length(r, s.night_time_duration)
    }),
    (texts::WORLD_CURSE_CHANCE, |s, r| {
        kind(r, texts::WORLD_CURSE_CHANCE, &s.curse_modifier)
    }),
    (texts::WORLD_FOG_REMOVAL, |s, _| {
        s.flame_level_fog_removal.to_string()
    }),
];

/// `value` with at most two decimals and no trailing zeros.
fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A factor of the game's normal value as the game's settings show it, in
/// percent: 1.5 reads `150%`.
fn percent(value: f32) -> String {
    format!("{}%", number(value * 100.0))
}

/// A switch as the reader's on or off.
fn switch(request: &Request<'_>, on: bool) -> String {
    request.say(
        if on {
            texts::WORLD_ON
        } else {
            texts::WORLD_OFF
        },
        &[],
    )
}

/// A length of time in the reader's hours, minutes and seconds, leaving
/// out parts that are zero.
fn length(request: &Request<'_>, time: Duration) -> String {
    let text: Vec<String> = length_parts(time)
        .iter()
        .map(|(count, unit)| request.say(unit, &[(texts::value::COUNT, &count.to_string())]))
        .collect();
    text.join(" ")
}

/// The hours, minutes and seconds of `time` with their unit's text key,
/// leaving out parts that are zero; zero seconds for no time at all.
fn length_parts(time: Duration) -> Vec<(u64, &'static str)> {
    let seconds = time.as_secs();
    let parts: Vec<(u64, &str)> = [
        (seconds / 3600, texts::WORLD_HOURS),
        (seconds / 60 % 60, texts::WORLD_MINUTES),
        (seconds % 60, texts::WORLD_SECONDS),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .collect();
    if parts.is_empty() {
        vec![(0, texts::WORLD_SECONDS)]
    } else {
        parts
    }
}

/// The text key of the kind `value` of the setting `setting`: the setting's
/// key and the kind's name in kebab case, `AddBackpackMaterials` under
/// `world.tombstone-mode` reads `world.tombstone-mode.add-backpack-materials`;
/// `None` for a value the API does not name.
fn kind_key(setting: &str, value: &impl core::fmt::Debug) -> Option<String> {
    let name = format!("{value:?}");
    if name.contains('(') {
        return None;
    }
    let mut key = format!("{setting}.");
    for (at, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && at > 0 {
            key.push('-');
        }
        key.push(c.to_ascii_lowercase());
    }
    Some(key)
}

/// The kind `value` of the setting `setting` in the reader's words, or as
/// another kind for a value the API does not name or the texts lack.
fn kind(request: &Request<'_>, setting: &str, value: &impl core::fmt::Debug) -> String {
    if let Some(key) = kind_key(setting, value) {
        let said = request.say(&key, &[]);
        // The texts answer a key they lack with the key itself.
        if said != key {
            return said;
        }
    }
    request.say(
        texts::WORLD_OTHER,
        &[(texts::value::VALUE, &other_kind(&format!("{value:?}")))],
    )
}

/// What names a kind the texts lack: the game's number, else the API's name.
fn other_kind(name: &str) -> String {
    let number = name.trim_matches(|c: char| !c.is_ascii_digit());
    if number.is_empty() { name } else { number }.to_owned()
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
            (name.unwrap_or_default(), crate::account_id(key))
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
    /// What players read when they join: the message an administrator set
    /// in game, else the world's setting; empty for none.
    pub(crate) fn motd(&self) -> String {
        self.accounts
            .with(|accounts| accounts.motd.clone())
            .filter(|motd| !motd.is_empty())
            .unwrap_or_else(|| self.settings.load().motd.clone())
    }

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
            if matches!(say, Say::RestartNow) {
                self.restart_left.store(RESTART_TRIES, Ordering::Relaxed);
            }
        }
        self.end_restart();
    }

    /// Ends a restart whose warning ran out, once no backup holds the saves:
    /// stops the server as the game stops on Ctrl+C, which saves the world
    /// last, and the host's panel or service starts it again. A game that
    /// does not stop gets the save instead, asked again each tick it is
    /// refused, at most [`RESTART_TRIES`] times. The audit log keeps whether
    /// it stopped.
    fn end_restart(&self) {
        let left = self.restart_left.load(Ordering::Relaxed);
        if left == 0 || self.backing_up.load(Ordering::Acquire) {
            return;
        }
        if left == RESTART_TRIES {
            let stopped = self.world.stop();
            let said = stopped
                .as_ref()
                .map_or_else(ToString::to_string, |()| String::new());
            self.log(&[
                "server",
                "",
                ids::RESTART,
                "",
                &said,
                journal::outcome(stopped.is_ok()),
            ]);
            match stopped {
                Ok(()) => {
                    self.restart_left.store(0, Ordering::Relaxed);
                    return;
                }
                Err(why) => {
                    enshwick::warn!("the server did not stop for the restart, so it saves: {why}");
                }
            }
        }
        match self.world.save() {
            Ok(()) => self.restart_left.store(0, Ordering::Relaxed),
            Err(why) => {
                self.restart_left.store(left - 1, Ordering::Relaxed);
                if left == 1 {
                    enshwick::warn!("no save before the restart: {why}");
                }
            }
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
    use enshwick::game_settings::{
        AggroPoolAmount, CurseModifier, GameSettingsPreset, GenericDifficulty, RandomSpawnerAmount,
        TamingStartleRepercussion, TombstoneMode, WeatherFrequency,
    };
    use enshwick::i18n::{Language, Texts};

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
    fn the_world_view_writes_factors_in_percent_and_kinds_as_text_keys() {
        assert_eq!(percent(1.0), "100%");
        assert_eq!(percent(2.5), "250%");
        assert_eq!(percent(0.333_33), "33.33%");
        assert_eq!(number(10.0), "10");
        assert_eq!(
            kind_key(
                texts::WORLD_TOMBSTONE_MODE,
                &TombstoneMode::AddBackpackMaterials
            )
            .as_deref(),
            Some("world.tombstone-mode.add-backpack-materials")
        );
        assert_eq!(kind_key(PRESET, &GameSettingsPreset::Other(7)), None);
        assert_eq!(other_kind("Other(7)"), "7");
        assert_eq!(other_kind("NewMode"), "NewMode");
    }

    #[test]
    fn a_length_of_time_reads_as_its_hours_minutes_and_seconds_without_zero_parts() {
        let (h, min, s) = (
            texts::WORLD_HOURS,
            texts::WORLD_MINUTES,
            texts::WORLD_SECONDS,
        );
        assert_eq!(length_parts(Duration::from_mins(30)), [(30, min)]);
        assert_eq!(
            length_parts(Duration::from_secs(90 * 60 + 30)),
            [(1, h), (30, min), (30, s)]
        );
        assert_eq!(length_parts(Duration::from_hours(2)), [(2, h)]);
        assert_eq!(length_parts(Duration::from_secs(59)), [(59, s)]);
        assert_eq!(length_parts(Duration::ZERO), [(0, s)]);
    }

    #[test]
    fn every_game_setting_and_every_kind_it_takes_has_its_text() {
        let texts = Texts::parse(texts::TOML).unwrap();
        let has = |key: &str| {
            Language::REQUIRED
                .iter()
                .all(|language| texts.get(key, *language).is_some())
        };
        let mut names: Vec<&str> = GAME_SETTINGS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), GAME_SETTINGS.len());
        assert!(names.iter().all(|name| has(name)), "{names:?}");
        let kinds: Vec<Option<String>> = [
            GameSettingsPreset::Default,
            GameSettingsPreset::Relaxed,
            GameSettingsPreset::Hard,
            GameSettingsPreset::Survival,
            GameSettingsPreset::Custom,
        ]
        .iter()
        .map(|kind| kind_key(PRESET, kind))
        .chain(
            [
                TombstoneMode::AddBackpackMaterials,
                TombstoneMode::Everything,
                TombstoneMode::NoTombstone,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_TOMBSTONE_MODE, kind)),
        )
        .chain(
            [
                WeatherFrequency::Disabled,
                WeatherFrequency::Rare,
                WeatherFrequency::Normal,
                WeatherFrequency::Often,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_WEATHER_FREQUENCY, kind)),
        )
        .chain(
            [
                GenericDifficulty::VeryEasy,
                GenericDifficulty::Easy,
                GenericDifficulty::Normal,
                GenericDifficulty::Hard,
                GenericDifficulty::VeryHard,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_FISHING_DIFFICULTY, kind)),
        )
        .chain(
            [
                RandomSpawnerAmount::Few,
                RandomSpawnerAmount::Normal,
                RandomSpawnerAmount::Many,
                RandomSpawnerAmount::Extreme,
                RandomSpawnerAmount::Automatic,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_ENEMY_AMOUNT, kind)),
        )
        .chain(
            [
                AggroPoolAmount::Few,
                AggroPoolAmount::Normal,
                AggroPoolAmount::Many,
                AggroPoolAmount::Extreme,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_ENEMY_ATTACKS, kind)),
        )
        .chain(
            [
                TamingStartleRepercussion::KeepProgress,
                TamingStartleRepercussion::LoseSomeProgress,
                TamingStartleRepercussion::LoseAllProgress,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_TAMING_STARTLE, kind)),
        )
        .chain(
            [
                CurseModifier::Easy,
                CurseModifier::Normal,
                CurseModifier::Hard,
            ]
            .iter()
            .map(|kind| kind_key(texts::WORLD_CURSE_CHANCE, kind)),
        )
        .collect();
        for key in kinds {
            let key = key.unwrap();
            assert!(has(&key), "{key}");
        }
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
