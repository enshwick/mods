---
id: enshwick.admin-tools
name: Admin Tools
summary: Server admins manage players, bans and the allow-list, set time and weather, save and back up the world, and use RCON and audit and chat logs.
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
  - key: chat_log
    default: auto
    choices: auto, on, off
    meaning: Whether chat lines go to chat.log. Auto is on for a dedicated server and off for a game a player hosts.
    applies: at-once
  - key: backups_kept
    default: 10
    range: 1 to 100
    meaning: How many backups the backup action keeps. The oldest go first.
    applies: at-once
  - key: backup_dir
    default: ""
    meaning: The folder backups go to, beside the world's save. A full path or .. is refused. Empty uses backups in the world's data folder.
    applies: at-once
  - key: rcon
    default: auto
    choices: auto, on, off
    meaning: Whether RCON tools may connect. Auto is on for a dedicated server and off for a game a player hosts.
    applies: at-once
  - key: rcon_ip
    default: 127.0.0.1
    meaning: The address the remote console listens on. 127.0.0.1 is this computer only. 0.0.0.0 is every network.
    applies: at-once
  - key: rcon_port
    default: 25575
    range: 1 to 65535
    meaning: The TCP port the remote console listens on.
    applies: at-once
  - key: rcon_password
    default: ""
    meaning: The password RCON tools log in with, at least 16 characters. Empty uses the generated one in rcon-password.txt. Never shown or logged.
    applies: at-once
  - key: rights.<action>
    default: CanKickBan, and everyone for online, status and motd
    meaning: Who may use each action. everyone, nobody, or a permission of the player's user group.
    applies: at-once
---

- Administrators see who is online, and kick, ban or mute players. A ban holds after a restart.
- An allow-list lets only chosen accounts and administrators join, and each world decides who may use each action.
- Players read a message of the day, scheduled announcements and restart warnings.
- Administrators set the time and the weather, heal and teleport players, save the world and back it up while the server runs.
- RCON tools such as BattleMetrics run every command on a dedicated server, and audit and chat logs keep what happened.

## How do I use an action?

You type the action as a command in chat, or in the console window of a
dedicated server. Every command starts with `/adl` or `/admin-tools`.

1. Open the chat in the game.
2. Type `/adl help` to see the actions you may use.
3. Type a command, such as `/adl kick Bob`.

The answer shows only to you. In the console window of a dedicated server,
type the same line without the `/`, such as `adl kick Bob`.

The console and RCON tools may use every action.

| Command | What it does |
| --- | --- |
| `/adl online` | Lists the players online, their account ids, how long each has played, and who is muted. |
| `/adl status` | Shows how long the server runs, the world, and the Enshwick and mod versions. |
| `/adl motd` | Shows the message of the day. |
| `/adl kick <player>` | Removes a player from the game. They may join again. |
| `/adl banned <player> on` | Bans a player who is online. |
| `/adl banned <player> off` | Lifts a ban, also while the player is away. |
| `/adl allowed <player> on` or `off` | Puts a player on the allow-list or takes them off. |
| `/adl muted <player> on` or `off` | Mutes a player in chat or lifts the mute. |
| `/adl heal <player>` | Gives a player their full health. |
| `/adl give <player> <count> <item>` | Gives a player new items, up to one stack. Name the item by the game's name, such as `Tool_Glider_T1`, in any case; by its GUID; by a mod's content id; or by its number. An unknown name lists the names near it. |
| `/adl goto <player>` | Takes you to a player. |
| `/adl bring <player>` | Brings a player to you. |
| `/adl send <player> <player>` | Moves the first player to the second. |
| `/adl place <player> <x> <y> <z>` | Moves a player to a position in metres, on free ground near it. |
| `/adl time <hour>` | Sets the time of day, such as `6.5` for 06:30. |
| `/adl weather clear` | Starts the weather: `clear`, `rain`, `blizzard`, or `any` for the game's choice. |
| `/adl restart <minutes>` | Warns every player of a restart, and saves the world when the time is up. `0` stops the warning. Only on a dedicated server. |
| `/adl save` | Saves the world now. |
| `/adl backup` | Copies the world to a backup folder while the server runs. |
| `/adl world` | Shows the world, its last save and its game settings. |
| `/adl audit <lines>` | Shows the last lines of the audit log, up to 50. |
| `/adl chat <lines>` | Shows the last lines of the chat log, up to 50. |

Administrators and the host cannot be kicked, banned or muted. `goto` and
`bring` need a character, so the console cannot use them.

## How do I keep strangers out?

1. Put each player you trust on the allow-list with
   `/adl allowed <player> on` while they are online.
2. Open the Mods screen, select Admin Tools, and turn on **Allow-list** in the
   server's settings.

From then on, an account not on the list is kicked as it joins.
Administrators always get in. To add a player, turn the allow-list off, add
them while they are online, and turn it on again.

## How do I decide who may use each action?

Each action has a rule in the world's settings, under **Who may use each
action**. A rule is one of these:

- `everyone`
- `nobody`. Only the server's console may then use the action.
- A permission of the player's user group in `enshrouded_server.json`:
  `CanKickBan` for administrators, `CanAccessInventories`, `CanEditBase`,
  `CanEditWorld`, `CanExtendBase` or `CanReceiveExp`.

By default, `online`, `status` and `motd` are for everyone and every other
action is for administrators. A player sees only the actions their rule allows. A change
applies within two seconds.

## How do I warn players before a restart?

Type `/adl restart 10`. Every player reads that the server restarts in
10 minutes. The warning comes again at 5, 2 and 1 minutes left, and once more
when the time is up.

When the time is up, Admin Tools saves the world. It does not stop the
server: your host's panel or script stops it, so set the warning to end when
the panel restarts the server.

Type `/adl restart 0` to call the restart off. A game a player hosts has no
restart warning.

## How do I connect an RCON tool?

On a dedicated server the remote console is on from the first start. It
listens on the server's own computer only. It speaks Source RCON, which tools
such as BattleMetrics, rcon-cli and mcrcon use.

1. Open `rcon-password.txt` in `<save>-mod-data/enshwick.admin-tools/` beside
   the world's save. Admin Tools writes a random password there at the first
   start, and the server's log names the file.
2. In the tool on the server's computer, enter `127.0.0.1`, port `25575` and
   that password.
3. Send a command without the `/`, such as `adl online`. Send `help` to list
   the commands of every mod.

To choose the password yourself, set **Remote console password** in the
world's settings. To turn the console off, set **Remote console** to off.

RCON is not encrypted: the password and commands cross the network as
plain text. To reach it from another computer, use an SSH tunnel or a VPN.
Or set **Remote console address** to `0.0.0.0` and open the port only to
computers you trust.

After three wrong passwords from one address, the address waits a minute. A
tool that does not log in within 10 seconds is disconnected.

## How do I back up the world?

Type `/adl backup`. Admin Tools holds the world's saves and copies the save,
its mod settings and its mod data to a folder named by the UTC time. Then it lets the saves go again, also when the copy fails.

Players keep playing. You read where the backup went when it is done.

Backups go to `backups/` in `<save>-mod-data/enshwick.admin-tools/`. If you
set **Backup folder**, they go to a folder named after the save inside it.

Admin Tools keeps the newest 10, or as many as **Backups kept** says, and
removes older ones. To restore a
backup, stop the server and copy the files from the backup folder back
beside the world's save.

## Where are the audit log and the chat log?

Both are in `<save>-mod-data/enshwick.admin-tools/` beside the world's save.

- `audit.log` holds one line per change an administrator made: the time in
  UTC, who acted, from where, the action, its values and the result.
  Reads such as `online` are not written. RCON logins are written too.
- `chat.log` holds one line per chat line: the time in UTC, the player's
  name and account id, the kind of line and its text.

When the chat log is on, everything players write in chat is kept on the
server. It is on for a dedicated server and off for a game a player hosts,
unless the world's **Chat log** setting says otherwise. Past 1 MB a log moves
to `audit.1.log` or `chat.1.log` and starts again.

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

**An RCON tool cannot connect.** Check that the port is open in the
firewall, and that the server's log says the remote console listens. When
another program uses the port, set another in **Remote console port**.

**Something else goes wrong.** Open an issue in the
[mods repository](https://github.com/enshwick/mods/issues). Attach
the files from the `logs/` folder in the server folder.
