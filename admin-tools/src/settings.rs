//! The world's settings: what players read, who may join, and who may use
//! each action.

use enshwick::world::{Player, PlayerPermissions};

use crate::ids;

/// The longest text a setting holds, the Mods screen's text field.
pub(crate) const MAX_TEXT: usize = 256;

/// The most announcements the schedule takes.
pub(crate) const MAX_ANNOUNCEMENTS: usize = 16;

/// The rule that lets every player use an action.
const EVERYONE: &str = "everyone";

/// The rule that lets no player use an action; the console still may.
const NOBODY: &str = "nobody";

/// The rule the actions that change something take unless the world says
/// otherwise: the server's administrators.
const ADMINS: &str = "CanKickBan";

/// When the audit log is on: by the kind of server, always, or never.
pub(crate) const AUDIT_CHOICES: [&str; 3] = ["auto", "on", "off"];

/// Admin Tools settings for this world.
#[enshwick::config(validate = Settings::check)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Settings {
    /// What each player reads when they join; empty for nothing.
    pub motd: String,
    /// Whether only accounts on the allow-list may join; administrators always may.
    pub allowlist: bool,
    /// Announcements shown to every player in turn, one each interval.
    pub announcements: Vec<String>,
    /// Minutes between two announcements.
    #[config(range = 1..=1440)]
    pub announce_minutes: u32,
    /// A line players read after each restart warning's time; empty for none.
    pub restart_text: String,
    /// Whether admin actions go to `audit.log` in this world's data folder: auto is on for a dedicated server and off for a game a player hosts.
    #[config(choices = ["auto", "on", "off"])]
    pub audit: String,
    /// Who may use each action: everyone, nobody, or the players whose user group gives a permission, such as `CanKickBan` for the administrators.
    #[config(group)]
    pub rights: Rights,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            motd: String::new(),
            allowlist: false,
            announcements: Vec::new(),
            announce_minutes: 30,
            restart_text: String::new(),
            audit: AUDIT_CHOICES[0].to_owned(),
            rights: Rights::default(),
        }
    }
}

impl Settings {
    /// Refuses texts past [`MAX_TEXT`], too many announcements and rules
    /// that name no permission.
    fn check(&self) -> Result<(), CheckSettingsError> {
        let texts = [&self.motd, &self.restart_text]
            .into_iter()
            .chain(&self.announcements);
        if let Some(long) = texts.into_iter().find(|text| text.len() > MAX_TEXT) {
            return Err(CheckSettingsError::TooLong(long.chars().take(24).collect()));
        }
        if self.announcements.len() > MAX_ANNOUNCEMENTS {
            return Err(CheckSettingsError::TooMany);
        }
        for id in ids::ALL {
            let rule = self.rights.rule(id);
            if Who::parse(rule).is_none() {
                return Err(CheckSettingsError::Rule {
                    field: id.replace('-', "_"),
                    rule: rule.to_owned(),
                    words: Who::words().join(", "),
                });
            }
        }
        Ok(())
    }

    /// Whether the audit log is on, `dedicated` saying whether this is a
    /// dedicated server.
    pub(crate) fn audit_on(&self, dedicated: bool) -> bool {
        let [_, on, off] = AUDIT_CHOICES;
        match self.audit.as_str() {
            word if word == on => true,
            word if word == off => false,
            _ => dedicated,
        }
    }
}

/// Why the world's settings were refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CheckSettingsError {
    /// A text past [`MAX_TEXT`]; its start.
    #[error("a text is longer than {MAX_TEXT} bytes: {0}...")]
    TooLong(String),
    /// More than [`MAX_ANNOUNCEMENTS`].
    #[error("at most {MAX_ANNOUNCEMENTS} announcements")]
    TooMany,
    /// A rule that names no permission.
    #[error("rights.{field}: {rule} is none of {words}")]
    Rule {
        /// The rule's key in the group.
        field: String,
        /// What the world's file says.
        rule: String,
        /// The words a rule takes.
        words: String,
    },
}

impl From<CheckSettingsError> for enshwick::config::ConfigError {
    /// The refusal in the reader's language, from the mod's texts.
    fn from(error: CheckSettingsError) -> Self {
        match &error {
            CheckSettingsError::TooLong(start) => Self::text(
                crate::texts::CONFIG_TOO_LONG,
                &[("max", &MAX_TEXT.to_string()), ("start", start)],
            ),
            CheckSettingsError::TooMany => Self::text(
                crate::texts::CONFIG_TOO_MANY,
                &[("max", &MAX_ANNOUNCEMENTS.to_string())],
            ),
            CheckSettingsError::Rule { field, rule, words } => Self::text(
                crate::texts::CONFIG_BAD_RULE,
                &[("field", field), ("rule", rule), ("words", words)],
            ),
        }
    }
}

/// Who may use each action, by its id: a rule the world's settings give.
#[enshwick::config(group)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rights {
    /// The list of players online.
    pub online: String,
    /// Reading the message of the day.
    pub motd: String,
    /// Kicking a player.
    pub kick: String,
    /// The ban list: banning and unbanning.
    pub banned: String,
    /// The allow-list.
    pub allowed: String,
    /// Muting in chat.
    pub muted: String,
    /// Healing a player.
    pub heal: String,
    /// Going to a player.
    pub goto: String,
    /// Bringing a player here.
    pub bring: String,
    /// Sending a player to another.
    pub send: String,
    /// Moving a player to a position.
    pub place: String,
    /// Setting the time of day.
    pub time: String,
    /// Setting the weather.
    pub weather: String,
    /// Warning players of a restart.
    pub restart: String,
    /// Reading the audit log.
    pub audit: String,
}

impl Default for Rights {
    fn default() -> Self {
        let admins = || ADMINS.to_owned();
        Self {
            online: EVERYONE.to_owned(),
            motd: EVERYONE.to_owned(),
            kick: admins(),
            banned: admins(),
            allowed: admins(),
            muted: admins(),
            heal: admins(),
            goto: admins(),
            bring: admins(),
            send: admins(),
            place: admins(),
            time: admins(),
            weather: admins(),
            restart: admins(),
            audit: admins(),
        }
    }
}

impl Rights {
    /// The rule of the action `id`; an id of no action takes the
    /// administrators' rule.
    pub(crate) fn rule(&self, id: &str) -> &str {
        match id {
            ids::ONLINE => &self.online,
            ids::MOTD => &self.motd,
            ids::KICK => &self.kick,
            ids::BANNED => &self.banned,
            ids::ALLOWED => &self.allowed,
            ids::MUTED => &self.muted,
            ids::HEAL => &self.heal,
            ids::GOTO => &self.goto,
            ids::BRING => &self.bring,
            ids::SEND => &self.send,
            ids::PLACE => &self.place,
            ids::TIME => &self.time,
            ids::WEATHER => &self.weather,
            ids::RESTART => &self.restart,
            ids::AUDIT => &self.audit,
            _ => ADMINS,
        }
    }

    /// Whether `player` may use the action `id`. The console and remote
    /// callers, which have no account, always may.
    pub(crate) fn admits(&self, id: &str, player: &Player) -> bool {
        if player.key.0 == 0 {
            return true;
        }
        match Who::parse(self.rule(id)) {
            Some(Who::Everyone) => true,
            Some(Who::Permission(permission)) => player.has(permission),
            Some(Who::Nobody) | None => false,
        }
    }
}

/// A rule's meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Who {
    Everyone,
    Nobody,
    Permission(PlayerPermissions),
}

impl Who {
    /// The rule `word` names: everyone, nobody or a permission's name, any
    /// case.
    fn parse(word: &str) -> Option<Self> {
        let word = word.trim();
        if word.eq_ignore_ascii_case(EVERYONE) {
            return Some(Self::Everyone);
        }
        if word.eq_ignore_ascii_case(NOBODY) {
            return Some(Self::Nobody);
        }
        PlayerPermissions::ALL
            .into_iter()
            .find(|permission| format!("{permission:?}").eq_ignore_ascii_case(word))
            .map(Self::Permission)
    }

    /// Every word a rule takes.
    fn words() -> Vec<String> {
        [EVERYONE.to_owned(), NOBODY.to_owned()]
            .into_iter()
            .chain(PlayerPermissions::ALL.iter().map(|p| format!("{p:?}")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use enshwick::config::Config as _;
    use enshwick::{Entity, PlayerKey};

    use super::*;

    fn player(key: u64, permissions: u8) -> Player {
        Player {
            entity: Entity(2),
            slot: 0,
            key: PlayerKey::new(key),
            permissions,
            name: "Ann".into(),
        }
    }

    #[test]
    fn the_defaults_pass_their_own_check_and_parse_from_the_template() {
        let settings = Settings::default();
        assert_eq!(settings.check(), Ok(()));
        let read: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(read, settings);
    }

    #[test]
    fn reads_are_for_everyone_and_changes_for_administrators_by_default() {
        let rights = Rights::default();
        let admin = player(1, PlayerPermissions::CanKickBan.bit());
        let guest = player(2, PlayerPermissions::CanEditBase.bit());
        assert!(rights.admits(ids::ONLINE, &guest));
        assert!(!rights.admits(ids::KICK, &guest));
        assert!(rights.admits(ids::KICK, &admin));
    }

    #[test]
    fn a_rule_names_a_permission_in_any_case_or_nobody_and_the_console_always_passes() {
        let rights = Rights {
            heal: "caneditbase".into(),
            kick: "Nobody".into(),
            ..Rights::default()
        };
        let builder = player(2, PlayerPermissions::CanEditBase.bit());
        let admin = player(1, PlayerPermissions::ALL_BITS);
        assert!(rights.admits(ids::HEAL, &builder));
        assert!(!rights.admits(ids::KICK, &admin));
        assert!(rights.admits(ids::KICK, &player(0, 0)));
    }

    #[test]
    fn the_check_refuses_an_unknown_rule_a_long_text_and_too_many_announcements() {
        let mut settings = Settings::default();
        settings.rights.place = "moderators".into();
        let refused = settings.check().unwrap_err().to_string();
        assert!(
            refused.contains("rights.place") && refused.contains("CanKickBan"),
            "{refused}"
        );

        let long = Settings {
            motd: "a".repeat(MAX_TEXT + 1),
            ..Settings::default()
        };
        assert!(long.check().is_err());

        let many = Settings {
            announcements: vec!["hi".into(); MAX_ANNOUNCEMENTS + 1],
            ..Settings::default()
        };
        assert!(many.check().is_err());
    }

    #[test]
    fn the_audit_log_follows_the_kind_of_server_unless_the_world_says() {
        let with = |audit: &str| Settings {
            audit: audit.into(),
            ..Settings::default()
        };
        let auto = Settings::default();
        assert!(auto.audit_on(true) && !auto.audit_on(false));
        assert!(with("on").audit_on(false));
        assert!(!with("off").audit_on(true));
    }

    #[test]
    fn every_action_has_its_own_rule() {
        let rights = Rights::default();
        let defaults: Vec<&str> = ids::ALL.iter().map(|id| rights.rule(id)).collect();
        assert_eq!(defaults.len(), ids::ALL.len());
        let schema = Settings::schema();
        for id in ids::ALL {
            let key = format!("rights.{}", id.replace('-', "_"));
            assert!(schema.field(&key).is_some(), "{key}");
        }
    }

    #[test]
    fn each_action_reads_its_own_rule_from_the_world_s_settings() {
        let mut table = String::new();
        for id in ids::ALL {
            writeln!(table, "{} = \"{id}-rule\"", id.replace('-', "_")).unwrap();
        }
        let rights: Rights = toml::from_str(&table).unwrap();
        for id in ids::ALL {
            assert_eq!(rights.rule(id), format!("{id}-rule"));
        }
    }

    #[test]
    fn texts_and_announcements_at_their_limits_pass_the_check() {
        let full = Settings {
            motd: "a".repeat(MAX_TEXT),
            announcements: vec!["hi".into(); MAX_ANNOUNCEMENTS],
            ..Settings::default()
        };
        assert_eq!(full.check(), Ok(()));
    }
}
