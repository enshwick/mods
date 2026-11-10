//! The actions' ids: their words in commands and console lines, and their
//! keys in the rights settings.

use crate::texts;

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

/// Bans a player with a reason.
pub(crate) const BAN: &str = "ban";
/// Reads the ban list.
pub(crate) const BANS: &str = "bans";
/// Bans, unbans, allows or disallows an account by its id.
pub(crate) const ACCOUNT: &str = "account";
/// Tells every player a line now.
pub(crate) const SAY: &str = "say";
/// Sets the message of the day.
pub(crate) const SET_MOTD: &str = "setmotd";
/// Moves a player to the caller's map waypoint.
pub(crate) const MARK: &str = "mark";
/// Who built, removed or dug near the caller.
pub(crate) const WHO: &str = "who";
/// Opens the mod's window.
pub(crate) const PANEL: &str = "panel";
/// Opens one section of the mod's window.
pub(crate) const OPEN: &str = "open";
/// Opens the window of one player.
pub(crate) const PLAYER: &str = "player";

/// Every action, in the order the help lists them.
pub(crate) const ALL: [&str; 38] = [
    PANEL, OPEN, PLAYER, ONLINE, STATUS, MOTD, SET_MOTD, SAY, KICK, BAN, BANS, BANNED, ALLOWED,
    ACCOUNT, MUTED, HEAL, STATS, LEARN, GOD, GIVE, CHEST, CHEST_AT, CLEAR, COPY, GOTO, BRING, SEND,
    PLACE, MARK, TIME, WEATHER, RESTART, SAVE, BACKUP, WORLD, WHO, AUDIT, CHAT,
];

/// The groups the command help lists the actions under: each group's text
/// key and its actions.
pub(crate) const GROUPS: [(&str, &[&str]); 9] = [
    (texts::GROUP_WINDOWS, &[PANEL, OPEN, PLAYER]),
    (texts::GROUP_PLAYERS, &[ONLINE, KICK, MUTED]),
    (
        texts::GROUP_ACCOUNTS,
        &[BAN, BANS, BANNED, ALLOWED, ACCOUNT],
    ),
    (texts::GROUP_CHARACTER, &[HEAL, STATS, LEARN, GOD]),
    (texts::GROUP_ITEMS, &[GIVE, CHEST, CHEST_AT, CLEAR, COPY]),
    (texts::GROUP_MOVES, &[GOTO, BRING, SEND, PLACE, MARK]),
    (texts::GROUP_WORLD, &[TIME, WEATHER, SAVE, BACKUP, WORLD]),
    (texts::GROUP_SERVER, &[STATUS, MOTD, SET_MOTD, SAY, RESTART]),
    (texts::GROUP_LOGS, &[WHO, AUDIT, CHAT]),
];

/// The group of the action `id`, a text key.
pub(crate) fn group(id: &str) -> Option<&'static str> {
    GROUPS
        .iter()
        .find(|(_, ids)| ids.contains(&id))
        .map(|(group, _)| *group)
}

/// The actions whose player lists offer the caller too: those an
/// administrator uses on themselves as on anyone.
pub(crate) const WITH_SELF: [&str; 11] = [
    PLAYER, HEAL, STATS, LEARN, GOD, GIVE, CHEST, CLEAR, COPY, PLACE, MARK,
];

/// The actions that only read or open a window, which the audit log
/// leaves out.
pub(crate) const READS: [&str; 11] = [
    PANEL, OPEN, PLAYER, ONLINE, STATUS, MOTD, BANS, WORLD, WHO, AUDIT, CHAT,
];

/// The command's short alias.
pub(crate) const ALIAS: &str = "adm";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_is_in_exactly_one_group() {
        for id in ALL {
            let groups = GROUPS.iter().filter(|(_, ids)| ids.contains(&id)).count();
            assert_eq!(groups, 1, "{id}");
        }
        let grouped: usize = GROUPS.iter().map(|(_, ids)| ids.len()).sum();
        assert_eq!(grouped, ALL.len());
    }
}
