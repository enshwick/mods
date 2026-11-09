//! The actions' ids: their words in commands and console lines, and their
//! keys in the rights settings.

/// The players online.
pub(crate) const ONLINE: &str = "online";
/// The message of the day.
pub(crate) const MOTD: &str = "motd";
/// Kicks a player.
pub(crate) const KICK: &str = "kick";
/// The ban list.
pub(crate) const BANNED: &str = "banned";
/// The allow-list.
pub(crate) const ALLOWED: &str = "allowed";
/// Mutes in chat.
pub(crate) const MUTED: &str = "muted";
/// Heals a player.
pub(crate) const HEAL: &str = "heal";
/// Takes the caller to a player.
pub(crate) const GOTO: &str = "goto";
/// Brings a player to the caller.
pub(crate) const BRING: &str = "bring";
/// Sends a player to another.
pub(crate) const SEND: &str = "send";
/// Moves a player to a position.
pub(crate) const PLACE: &str = "place";
/// Sets the time of day.
pub(crate) const TIME: &str = "time";
/// Sets the weather.
pub(crate) const WEATHER: &str = "weather";
/// Warns of a restart.
pub(crate) const RESTART: &str = "restart";
/// Reads the audit log.
pub(crate) const AUDIT: &str = "audit";

/// Every action, in the order the help lists them.
pub(crate) const ALL: [&str; 15] = [
    ONLINE, MOTD, KICK, BANNED, ALLOWED, MUTED, HEAL, GOTO, BRING, SEND, PLACE, TIME, WEATHER,
    RESTART, AUDIT,
];

/// The actions that only read, which the audit log leaves out.
pub(crate) const READS: [&str; 3] = [ONLINE, MOTD, AUDIT];

/// The mod's command name, the part of its id after the author.
pub(crate) const COMMAND: &str = "admin-tools";
/// The command's short alias.
pub(crate) const ALIAS: &str = "adl";
