//! The world's settings: what players read, who may join, and who may use
//! each action.

use std::net::IpAddr;

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

/// A switch that is on for a dedicated server and off for a hosted game.
const AUTO: &str = "auto";

/// A switch that is always on.
const ON: &str = "on";

/// A switch that is always off.
const OFF: &str = "off";

/// The default port of the remote console, the one most RCON tools try.
const RCON_PORT: u32 = 25_575;

/// The longest remote console password.
pub(crate) const MAX_PASSWORD: usize = 128;

/// The shortest remote console password a world may set, so a guess per
/// failed login stays hopeless; the generated one is longer.
pub(crate) const MIN_PASSWORD: usize = 16;

/// The address the remote console listens on unless the world says
/// otherwise: this computer only, as RCON sends its password in plain text.
const RCON_IP: &str = "127.0.0.1";

/// Admin Tools settings for this world.
#[enshwick::config(validate = Settings::check)]
#[derive(Clone, PartialEq)]
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
    #[config(choices = [AUTO, ON, OFF])]
    pub audit: String,
    /// Whether chat lines go to `chat.log` in this world's data folder: auto is on for a dedicated server and off for a game a player hosts.
    #[config(choices = [AUTO, ON, OFF])]
    pub chat_log: String,
    /// How many backups of the world the backup action keeps; the oldest go first.
    #[config(range = 1..=100)]
    pub backups_kept: u32,
    /// The folder backups go to, beside the world's save; empty for `backups` in this world's data folder.
    pub backup_dir: String,
    /// Whether a remote console (Source RCON) listens: auto is on for a dedicated server and off for a game a player hosts.
    #[config(choices = [AUTO, ON, OFF])]
    pub rcon: String,
    /// The address the remote console listens on: 127.0.0.1 for this computer only, 0.0.0.0 for every network.
    pub rcon_ip: String,
    /// The TCP port the remote console listens on.
    #[config(range = 1..=65535)]
    pub rcon_port: u32,
    /// The remote console's password; empty to use the one generated in `rcon-password.txt` in this world's data folder.
    #[config(secret)]
    pub rcon_password: String,
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
            audit: AUTO.to_owned(),
            chat_log: AUTO.to_owned(),
            backups_kept: 10,
            backup_dir: String::new(),
            rcon: AUTO.to_owned(),
            rcon_ip: RCON_IP.to_owned(),
            rcon_port: RCON_PORT,
            rcon_password: String::new(),
            rights: Rights::default(),
        }
    }
}

impl core::fmt::Debug for Settings {
    /// Every setting but the secret password, which is never shown.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Settings")
            .field("motd", &self.motd)
            .field("allowlist", &self.allowlist)
            .field("announcements", &self.announcements)
            .field("announce_minutes", &self.announce_minutes)
            .field("restart_text", &self.restart_text)
            .field("audit", &self.audit)
            .field("chat_log", &self.chat_log)
            .field("backups_kept", &self.backups_kept)
            .field("backup_dir", &self.backup_dir)
            .field("rcon", &self.rcon)
            .field("rcon_ip", &self.rcon_ip)
            .field("rcon_port", &self.rcon_port)
            .field("rights", &self.rights)
            .finish_non_exhaustive()
    }
}

impl Settings {
    /// Refuses texts past [`MAX_TEXT`], too many announcements and rules
    /// that name no permission.
    fn check(&self) -> Result<(), CheckSettingsError> {
        let texts = [&self.motd, &self.restart_text, &self.backup_dir]
            .into_iter()
            .chain(&self.announcements);
        if let Some(long) = texts.into_iter().find(|text| text.len() > MAX_TEXT) {
            return Err(CheckSettingsError::TooLong(long.chars().take(24).collect()));
        }
        if self.announcements.len() > MAX_ANNOUNCEMENTS {
            return Err(CheckSettingsError::TooMany);
        }
        if self.rcon_ip.parse::<IpAddr>().is_err() {
            return Err(CheckSettingsError::Address(self.rcon_ip.clone()));
        }
        if !self.rcon_password.is_empty()
            && !(MIN_PASSWORD..=MAX_PASSWORD).contains(&self.rcon_password.len())
        {
            return Err(CheckSettingsError::Password);
        }
        if !beside_the_save(&self.backup_dir) {
            return Err(CheckSettingsError::Folder(self.backup_dir.clone()));
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
        switch(&self.audit, dedicated)
    }

    /// Whether the chat log is on, as [`Settings::audit_on`].
    pub(crate) fn chat_log_on(&self, dedicated: bool) -> bool {
        switch(&self.chat_log, dedicated)
    }

    /// Whether the remote console listens, as [`Settings::audit_on`].
    pub(crate) fn rcon_on(&self, dedicated: bool) -> bool {
        switch(&self.rcon, dedicated)
    }
}

/// Whether the folder `dir` stays inside the saves' folder it is joined to:
/// relative, with no `..`, drive or root, read the same way on every system.
pub(crate) fn beside_the_save(dir: &str) -> bool {
    !dir.starts_with(['/', '\\'])
        && !dir.contains(':')
        && dir.split(['/', '\\']).all(|part| part != "..")
}

/// Whether the switch `word` is on, `dedicated` saying whether this is a
/// dedicated server, which auto follows.
fn switch(word: &str, dedicated: bool) -> bool {
    match word {
        ON => true,
        OFF => false,
        _ => dedicated,
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
    /// A remote console address that is no IP address.
    #[error("rcon_ip: {0} is no IP address")]
    Address(String),
    /// A remote console password shorter than [`MIN_PASSWORD`] or past
    /// [`MAX_PASSWORD`].
    #[error("rcon_password must be {MIN_PASSWORD} to {MAX_PASSWORD} bytes long")]
    Password,
    /// A backup folder that leaves the saves' folder.
    #[error("backup_dir: {0} is no folder beside the save")]
    Folder(String),
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
            CheckSettingsError::Address(address) => {
                Self::text(crate::texts::CONFIG_BAD_ADDRESS, &[("address", address)])
            }
            CheckSettingsError::Password => Self::text(
                crate::texts::CONFIG_BAD_PASSWORD,
                &[
                    ("min", &MIN_PASSWORD.to_string()),
                    ("max", &MAX_PASSWORD.to_string()),
                ],
            ),
            CheckSettingsError::Folder(folder) => {
                Self::text(crate::texts::CONFIG_BAD_FOLDER, &[("folder", folder)])
            }
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
    /// The versions, the uptime and the world.
    pub status: String,
    /// Saving the world now.
    pub save: String,
    /// Copying the world's save aside.
    pub backup: String,
    /// Reading the world's game settings.
    pub world: String,
    /// Reading the chat log.
    pub chat: String,
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
            status: EVERYONE.to_owned(),
            save: admins(),
            backup: admins(),
            world: admins(),
            chat: admins(),
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
            ids::STATUS => &self.status,
            ids::SAVE => &self.save,
            ids::BACKUP => &self.backup,
            ids::WORLD => &self.world,
            ids::CHAT => &self.chat,
            _ => ADMINS,
        }
    }

    /// Each action's rule, parsed once; a rule the check would refuse lets
    /// no player use its action.
    pub(crate) fn rules(&self) -> Rules {
        Rules(ids::ALL.map(|id| Who::parse(self.rule(id)).unwrap_or(Who::Nobody)))
    }
}

/// The rule of each action, in the order of [`ids::ALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rules([Who; ids::ALL.len()]);

impl Rules {
    /// Whether `player` may use the action `id`; an id of no action takes
    /// the administrators' rule. The server's console, which has no account
    /// and every permission, always may; a remote caller with no account
    /// passes only by its permissions.
    pub(crate) fn admits(&self, id: &str, player: &Player) -> bool {
        if player.key.0 == 0 && player.permissions == PlayerPermissions::ALL_BITS {
            return true;
        }
        let who = ids::ALL
            .iter()
            .position(|known| *known == id)
            .map_or(Who::Permission(PlayerPermissions::CanKickBan), |at| {
                self.0[at]
            });
        match who {
            Who::Everyone => true,
            Who::Permission(permission) => player.has(permission),
            Who::Nobody => false,
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
        let rules = Rights::default().rules();
        let admin = player(1, PlayerPermissions::CanKickBan.bit());
        let guest = player(2, PlayerPermissions::CanEditBase.bit());
        assert!(rules.admits(ids::ONLINE, &guest));
        assert!(!rules.admits(ids::KICK, &guest));
        assert!(rules.admits(ids::KICK, &admin));
    }

    #[test]
    fn a_rule_names_a_permission_in_any_case_or_nobody_and_the_console_always_passes() {
        let rules = Rights {
            heal: "caneditbase".into(),
            kick: "Nobody".into(),
            ..Rights::default()
        }
        .rules();
        let builder = player(2, PlayerPermissions::CanEditBase.bit());
        let admin = player(1, PlayerPermissions::ALL_BITS);
        assert!(rules.admits(ids::HEAL, &builder));
        assert!(!rules.admits(ids::KICK, &admin));
        assert!(rules.admits(ids::KICK, &player(0, PlayerPermissions::ALL_BITS)));
    }

    #[test]
    fn a_caller_with_no_account_passes_only_with_every_permission() {
        let rules = Rights::default().rules();
        assert!(!rules.admits(ids::KICK, &player(0, 0)));
        assert!(!rules.admits(ids::AUDIT, &player(0, 0)));
        assert!(rules.admits(ids::ONLINE, &player(0, 0)), "everyone");
        let moderator = player(0, PlayerPermissions::CanKickBan.bit());
        assert!(rules.admits(ids::KICK, &moderator));
        let nobody = Rights {
            kick: "nobody".into(),
            ..Rights::default()
        }
        .rules();
        assert!(!nobody.admits(ids::KICK, &moderator));
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
    fn the_settings_debug_text_never_shows_the_password() {
        let settings = Settings {
            rcon_password: "a secret of the world".into(),
            ..Settings::default()
        };
        let text = format!("{settings:?} {settings:#?}");
        assert!(!text.contains("a secret of the world"), "{text}");
        assert!(text.contains("rcon_port"), "{text}");
    }

    #[test]
    fn the_remote_console_listens_on_this_computer_only_unless_the_world_says() {
        let ip: IpAddr = Settings::default().rcon_ip.parse().unwrap();
        assert!(ip.is_loopback(), "{ip}");
    }

    #[test]
    fn a_password_of_the_world_s_own_is_refused_when_short() {
        let short = Settings {
            rcon_password: "p".repeat(MIN_PASSWORD - 1),
            ..Settings::default()
        };
        assert_eq!(short.check(), Err(CheckSettingsError::Password));
        let generated = Settings::default();
        assert_eq!(generated.check(), Ok(()), "empty uses the generated one");
    }

    #[test]
    fn a_backup_folder_stays_beside_the_save() {
        for refused in [
            "..",
            "../elsewhere",
            "backups/../../up",
            "/root",
            "C:\\Windows",
            "C:x",
            "\\\\host\\share",
        ] {
            let settings = Settings {
                backup_dir: refused.into(),
                ..Settings::default()
            };
            assert_eq!(
                settings.check(),
                Err(CheckSettingsError::Folder(refused.into())),
                "{refused}"
            );
        }
        for kept in ["", "backups", "world backups/nightly", "./backups"] {
            let settings = Settings {
                backup_dir: kept.into(),
                ..Settings::default()
            };
            assert_eq!(settings.check(), Ok(()), "{kept}");
        }
    }

    #[test]
    fn texts_and_announcements_at_their_limits_pass_the_check() {
        let long_password = Settings {
            rcon_password: "p".repeat(MAX_PASSWORD + 1),
            ..Settings::default()
        };
        assert!(long_password.check().is_err());
        let bad_address = Settings {
            rcon_ip: "localhost".into(),
            ..Settings::default()
        };
        assert!(bad_address.check().is_err());
        let full = Settings {
            rcon_password: "p".repeat(MAX_PASSWORD),
            motd: "a".repeat(MAX_TEXT),
            announcements: vec!["hi".into(); MAX_ANNOUNCEMENTS],
            ..Settings::default()
        };
        assert_eq!(full.check(), Ok(()));
    }
}
