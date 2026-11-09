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
