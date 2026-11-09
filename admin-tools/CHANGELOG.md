# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry says what players notice; a release's notes show its section.

## [Unreleased]

### Added

- First alpha. A server's administrators mute players in chat from the
  dedicated server's console.
- Every action is a command in chat and the console: `/adl` or
  `/admin-tools`, with `/adl help` listing what the caller may use.
- Administrators list the players online, kick them, and heal them.
- A ban holds when the account comes back, also after a restart, and lifts
  while the player is away.
- An allow-list, on in the world's settings, keeps out accounts not on it.
- Players read a message of the day when they join, and announcements repeat
  on a schedule.
- A restart warning counts down for every player and can be called off.
- Administrators set the time of day and the weather.
- Administrators take themselves to a player, bring a player to them, send
  one player to another, or move a player to a position.
- Each world decides who may use each action, by the permissions of the
  game's user groups.
- An audit log beside the world's save keeps every change an administrator
  made.
- The players list shows each player's account id, and `/adl status` shows
  the uptime, the world and the Enshwick and mod versions.
- RCON tools such as BattleMetrics or rcon-cli run every command on a
  dedicated server, on port 25575 of the server's own computer with a
  password Admin Tools generates; a world setting opens it to other
  addresses, and a game a player hosts has it off unless the world turns it
  on.
- A chat log beside the world's save keeps every chat line with the
  player's name and account id; it is on for a dedicated server.
- `/adl backup` copies the world's save, mod settings and mod data while the
  server runs, and keeps the newest 10 backups.
- `/adl save` saves the world now, and `/adl world` shows its last save and
  its game settings.
- A restart warning saves the world when its time is up; a game a player
  hosts offers no restart warning.
