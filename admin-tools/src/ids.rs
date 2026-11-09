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
/// Gives a player items.
pub(crate) const GIVE: &str = "give";
/// Places a chest of items beside a player.
pub(crate) const CHEST: &str = "chest";
/// Places a chest of items at a position.
pub(crate) const CHEST_AT: &str = "chestat";
/// Clears what a player carries.
pub(crate) const CLEAR: &str = "clear";
/// Copies what one player carries to another.
pub(crate) const COPY: &str = "copy";
/// Sets a player's health, stamina or mana.
pub(crate) const STATS: &str = "stats";
/// Grants a player a recipe or other knowledge.
pub(crate) const LEARN: &str = "learn";
/// God mode: nothing damages a player.
pub(crate) const GOD: &str = "god";
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
/// The versions, the uptime and the world.
pub(crate) const STATUS: &str = "status";
/// Saves the world now.
pub(crate) const SAVE: &str = "save";
/// The world's game settings.
pub(crate) const WORLD: &str = "world";
/// Copies the world's save aside.
pub(crate) const BACKUP: &str = "backup";
/// Reads the chat log.
pub(crate) const CHAT: &str = "chat";

/// Every action, in the order the help lists them.
pub(crate) const ALL: [&str; 28] = [
    ONLINE, STATUS, MOTD, KICK, BANNED, ALLOWED, MUTED, HEAL, STATS, LEARN, GOD, GIVE, CHEST,
    CHEST_AT, CLEAR, COPY, GOTO, BRING, SEND, PLACE, TIME, WEATHER, RESTART, SAVE, BACKUP, WORLD,
    AUDIT, CHAT,
];

/// The actions that only read, which the audit log leaves out.
pub(crate) const READS: [&str; 6] = [ONLINE, STATUS, MOTD, WORLD, AUDIT, CHAT];

/// The mod's command name, the part of its id after the author.
pub(crate) const COMMAND: &str = "admin-tools";
/// The command's short alias.
pub(crate) const ALIAS: &str = "adl";
