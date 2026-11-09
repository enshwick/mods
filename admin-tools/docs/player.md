---
id: enshwick.admin-tools
name: Admin Tools
summary: A server's administrators list, kick, ban and mute players, keep an allow-list, announce, warn of restarts, set the time and the weather, heal and teleport players, and read an audit log.
needs: server
settings:
  - key: motd
    default: ""
    meaning: What each player reads when they join. Empty shows nothing.
    applies: at-once
  - key: allowlist
    default: false
    meaning: When on, only accounts on the allow-list and administrators may join.
    applies: at-once
  - key: announcements
    default: "[]"
    meaning: Texts every player reads in turn, one each interval, up to 16.
    applies: at-once
  - key: announce_minutes
    default: 30
    range: 1 to 1440
    meaning: Minutes between two announcements.
    applies: at-once
  - key: restart_text
    default: ""
    meaning: A line players read after the time of each restart warning.
    applies: at-once
  - key: audit
    default: auto
    choices: auto, on, off
    meaning: Whether admin actions go to audit.log. Auto is on for a dedicated server and off for a game a player hosts.
    applies: at-once
  - key: rights.<action>
    default: CanKickBan, and everyone for online and motd
    meaning: Who may use each action. everyone, nobody, or a permission of the player's user group.
    applies: at-once
---

- Administrators see who is online, and kick, ban or mute players.
- A ban holds when the account comes back, also after a restart.
- An allow-list lets only chosen accounts and administrators join.
- Players read a message of the day when they join.
- Announcements repeat on a schedule.
- A restart warning counts down for every player.
- Administrators set the time of day and the weather.
- Administrators heal players and move them to each other or to a position.
- Each world decides who may use each action.
- An audit log keeps every change an administrator made.

## How do I use an action?

You type the action as a command in chat, or in the console window of a
dedicated server. Every command starts with `/adl` or `/admin-tools`.

1. Open the chat in the game.
2. Type `/adl help` to see the actions you may use.
3. Type a command, such as `/adl kick Bob`.

The answer shows only to you. In the console window of a dedicated server,
type the same line without the `/`, such as `adl kick Bob`. The console may
use every action. Windows for these actions in the game come in a later
release.

| Command | What it does |
| --- | --- |
| `/adl online` | Lists the players online, how long each has played, and who is muted. |
| `/adl motd` | Shows the message of the day. |
| `/adl kick <player>` | Removes a player from the game. They may join again. |
| `/adl banned <player> on` | Bans a player who is online. |
| `/adl banned <player> off` | Lifts a ban, also while the player is away. |
| `/adl allowed <player> on` or `off` | Puts a player on the allow-list or takes them off. |
| `/adl muted <player> on` or `off` | Mutes a player in chat or lifts the mute. |
| `/adl heal <player>` | Gives a player their full health. |
| `/adl goto <player>` | Takes you to a player. |
| `/adl bring <player>` | Brings a player to you. |
| `/adl send <player> <player>` | Moves the first player to the second. |
| `/adl place <player> <x> <y> <z>` | Moves a player to a position in metres, on free ground near it. |
| `/adl time <hour>` | Sets the time of day, such as `6.5` for 06:30. |
| `/adl weather clear` | Starts the weather: `clear`, `rain`, `blizzard`, or `any` for the game's choice. |
| `/adl restart <minutes>` | Warns every player of a restart. `0` stops the warning. |
| `/adl audit <lines>` | Shows the last lines of the audit log, up to 50. |

Administrators and the host cannot be kicked, banned or muted. `goto` and
`bring` need a character, so the console cannot use them.

## How do I keep strangers out?

1. Put each player you trust on the allow-list with
   `/adl allowed <player> on` while they are online.
2. Open the Mods screen, select Admin Tools, and turn on **Allow-list** in the
   server's settings.

From then on, an account not on the list is kicked as it joins.
Administrators always get in. To add a player, have them join while the
allow-list is off, or let an administrator add them.

## How do I decide who may use each action?

Each action has a rule in the world's settings, under **Who may use each
action**. A rule is one of these:

- `everyone`
- `nobody`. Only the console may then use the action.
- A permission of the player's user group in `enshrouded_server.json`:
  `CanKickBan` for administrators, `CanAccessInventories`, `CanEditBase`,
  `CanEditWorld`, `CanExtendBase` or `CanReceiveExp`.

By default, `online` and `motd` are for everyone and every other action is for
administrators. A player sees only the actions their rule allows. A change
applies within two seconds.

## How do I warn players before a restart?

Type `/adl restart 10`. Every player reads that the server restarts in
10 minutes. The warning comes again at 5, 2 and 1 minutes left, and once more
when the time is up. Admin Tools does not stop the server. Your host's panel
or script stops it. Type `/adl restart 0` to call the restart off.

## Where is the audit log?

It is `audit.log` in `<save>-mod-data/enshwick.admin-tools/` beside the
world's save. Each line holds the time in UTC, who acted, from where, the
action, its values and the result. Past 1 MB the log moves to `audit.1.log`
and starts again. Reads such as `online` are not written.

## Install

Admin Tools comes in its own archive, enshwick-admin-tools.zip. It needs
Enshwick, which comes in enshwick.zip. The
[install guide](https://enshwick.com/guides/install/) shows where to
put both archives.

## Multiplayer

Only the server needs Admin Tools. Players without Enshwick can join and use
the commands in chat.

## Problems

**`/adl` posts the line to chat.** Check that Admin Tools is in the server's
`mods/` folder. If it is, the server's log says why Admin Tools is off.

**The answer says no player has that name.** Check that the player is online.
Type the name as the game shows it.

**A player is kicked as they join.** The allow-list is on, and the player is
not on it, or the player is banned. Lift a ban with
`/adl banned <player> off`.

**Something else goes wrong.** Open an issue in the
[mods repository](https://github.com/enshwick/mods/issues). Attach
the files from the `logs/` folder in the server folder.
