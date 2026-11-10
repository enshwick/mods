---
id: enshwick.admin-tools
name: Admin Tools
summary: Server admins manage players, bans and the allow-list from a window, set time and weather, save and back up the world, and use RCON, a build log and audit and chat logs.
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
  - key: build_log
    default: false
    meaning: When on, the buildings players place and remove and the terrain they change go to builds.log. The log grows fast in a busy world.
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

- An Admin Tools window opens with Shift+F9: the players online with their actions, the bans and the allow-list, the server, and the build log.
- Administrators see who is online, and kick, ban or mute players. A ban keeps its reason and holds after a restart. Players who are away can be banned, unbanned and put on the allow-list.
- An allow-list lets only chosen accounts and administrators join. Each world decides who may use each action. Players read a message of the day, announcements, scheduled announcements and restart warnings.
- Administrators set the time and the weather, save the world and back it up while the server runs. They heal players, move them to a player, a position or a map waypoint, give items and chests of items, clear or copy an inventory, set health, stamina and mana, teach recipes and turn on god mode.
- RCON tools such as BattleMetrics run every command on a dedicated server. Audit and chat logs keep what happened, and a build log, off by default, shows who placed or removed a building or changed terrain near a spot.
- Administrators give items, place chests of items, clear or copy an inventory, set health, stamina and mana, teach recipes and turn on god mode.
- RCON tools such as BattleMetrics run every command on a dedicated server, and audit and chat logs keep what happened.

## How do I open the Admin Tools window?

Press Shift+F9 in the game. The window lists its sections: players, bans
and the allow-list, server, and build log. Each section shows only the
actions you may use, with a button for each.

- **Players** lists the players online. **Actions** opens the window of one
  player, with every action on them: move, moderate, health, items and
  recipes.
- **Bans and the allow-list** lists the bans with their reasons, each with a
  button to lift it, and switches players on the allow-list or off.
- **Server** shows the message of the day and the restart warning, and sets
  the time of day, the weather, saves and backups.
- **Build log** shows the changes near you, each with a button that takes you
  there.

You can change the key on the controls page of the Mods screen. The Mods
screen also opens the window: select Admin Tools, then
**Admin Tools window**.

## How do I use an action?

You press its button in the Admin Tools window, or type the action as a
command in chat or in the console window of a dedicated server. Every
command starts with `/adm` or `/admin-tools`.

1. Open the chat in the game.
2. Type `/adm help` to see the actions you may use.
3. Type a command, such as `/adm kick Bob`.

The answer shows only to you. In the console window of a dedicated server,
type the same line without the `/`, such as `adm kick Bob`.

The console and RCON tools may use every action.

| Command | What it does |
| --- | --- |
| `/adm panel` | Opens the Admin Tools window. |
| `/adm open <section>` | Opens one section of the window: `players`, `accounts`, `server` or `builds`. |
| `/adm player <player>` | Opens the window of one player, with every action on them. |
| `/adm online` | Lists the players online, their account ids, how long each has played, and who is muted. |
| `/adm status` | Shows how long the server runs, the world, and the Enshwick and mod versions. |
| `/adm motd` | Shows the message of the day. |
| `/adm setmotd <text>` | Sets the message of the day in place of the world's setting. `/adm setmotd -` goes back to the setting. |
| `/adm say <text>` | Shows every player a line now. |
| `/adm kick <player>` | Removes a player from the game. They may join again. |
| `/adm ban <player> <reason>` | Bans a player, online or away, and keeps the reason, the time and who banned them. |
| `/adm bans` | Lists every ban, newest first, with its reason, who banned and when. |
| `/adm banned <player> on` or `off` | Bans a player, online or away, or lifts a ban. |
| `/adm allowed <player> on` or `off` | Puts a player on the allow-list or takes them off, online or away. |
| `/adm account <id> ban`, `unban`, `allow` or `disallow` | Changes an account by its id of up to 16 hex digits, also one that never joined this world. |
| `/adm muted <player> on` or `off` | Mutes a player in chat or lifts the mute. |
| `/adm heal <player>` | Gives a player their full health. |
| `/adm stats <player> health`, `stamina` or `mana` `<value>` | Sets a player's health, stamina or mana. The game keeps the value within the maximum. |
| `/adm learn <player> <recipe>` | Unlocks a recipe for a player. Name it by the game's name, such as `Tool_Torch01`, in any case; by its GUID; or by a mod's content id. A number grants any knowledge the game keeps for the player. |
| `/adm god <player> on` or `off` | Nothing damages a player in god mode. It ends when the player leaves or the world stops. |
| `/adm give <player> <count> <item>` | Gives a player new items, in as many stacks as they need; you read how many did not fit. Name the item by the game's name, such as `Tool_Glider_T1`, in any case; by its GUID; by a mod's content id; or by its number. An unknown name lists the names near it. |
| `/adm chest <player> <count> <item>` | Places a wooden chest beside a player and fills it with items, named as for `give`. A chest outside every base is not saved. |
| `/adm chestat <x> <y> <z> <count> <item>` | Places a wooden chest at a position and fills it with items. A chest outside every base is not saved. |
| `/adm clear <player>` | Destroys every stack in every slot a player has, equipment, action bar and look slots included. |
| `/adm copy <player> <player>` | Gives the second player new items of every stack the first carries or wears. The items start new, without wear or level. What does not fit stays out, and you read how many; the look and currency slots are not copied. |
| `/adm goto <player>` | Takes you to a player. |
| `/adm bring <player>` | Brings a player to you. |
| `/adm send <player> <player>` | Moves the first player to the second. |
| `/adm place <player> <x> <y> <z>` | Moves a player to a position in metres, on free ground near it. |
| `/adm mark <player>` | Moves a player to the waypoint you set on your map, on free ground near it. |
| `/adm time <time>` | Sets the time of day, such as `18:30`, or in hours, such as `6.5` for 06:30. |
| `/adm weather clear` | Starts the weather: `clear`, `rain`, `blizzard`, or `any` for the game's choice. |
| `/adm restart <minutes>` | Warns every player of a restart, and saves the world when the time is up. `0` stops the warning. Only on a dedicated server. |
| `/adm save` | Saves the world now. |
| `/adm backup` | Copies the world to a backup folder while the server runs. |
| `/adm world` | Shows the world, its last save and its game settings. |
| `/adm who <metres>` | Shows the last 10 buildings placed or removed and terrain changed within up to 200 metres of you, newest first, while the build log is on. |
| `/adm audit <lines>` | Shows the last lines of the audit log, up to 50. |
| `/adm chat <lines>` | Shows the last lines of the chat log, up to 50. |

Administrators and the host cannot be kicked, banned or muted. `goto`,
`bring`, `mark` and `who` need a character, so the console cannot use them.

A player who is away is named as they last joined this world. Admin Tools
remembers the last 256 accounts that tried to join, and every account on the
ban list or the allow-list.

On a dedicated server, name an item or recipe by the game's name, its GUID or
a mod's content id.

## How do I keep strangers out?

1. Put each player you trust on the allow-list with
   `/adm allowed <player> on`, or switch them on in the window's
   **Bans and the allow-list** section.
2. Open the Mods screen, select Admin Tools, and turn on **Allow-list** in the
   server's settings.

From then on, an account not on the list is kicked as it joins.
Administrators always get in. A player kicked this way is remembered, so
you can allow them by name afterwards. To allow an account that never
joined, ask the player for their account id and type
`/adm account <id> allow`.

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

Type `/adm restart 10`. Every player reads that the server restarts in
10 minutes. The warning comes again at 5, 2 and 1 minutes left, and once more
when the time is up.

When the time is up, Admin Tools saves the world. It does not stop the
server: your host's panel or script stops it, so set the warning to end when
the panel restarts the server.

Type `/adm restart 0` to call the restart off. A game a player hosts has no
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
3. Send a command without the `/`, such as `adm online`. Send `help` to list
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

Type `/adm backup`. Admin Tools holds the world's saves and copies the save,
its mod settings and its mod data to a folder named by the UTC time. Then it lets the saves go again, also when the copy fails.

Players keep playing. You read where the backup went when it is done.

Backups go to `backups/` in `<save>-mod-data/enshwick.admin-tools/`. If you
set **Backup folder**, they go to a folder named after the save inside it.

Admin Tools keeps the newest 10, or as many as **Backups kept** says, and
removes older ones. To restore a
backup, stop the server and copy the files from the backup folder back
beside the world's save.

## How do I see who built or broke something?

Turn on **Build log** in the world's settings first. From then on, Admin Tools
writes each building placed, picked up or torn down, and each spot of terrain
dug or filled, with who did it, the time and the position. Terrain changes
name the nearest player within 16 metres, since the game does not say who
changed it.

1. Go to the spot.
2. Open the Admin Tools window and its **Build log** section, or type
   `/adm who 30` for the changes within 30 metres.
3. Press **Go there** beside a change to stand where it happened.

The log keeps nothing from before you turned it on.

## Where are the audit log and the chat log?

Both are in `<save>-mod-data/enshwick.admin-tools/` beside the world's save.

- `audit.log` holds one line per run of an action that changes something,
  done or refused. A line has the time in UTC, who acted, from where, the
  action, its values, the answer, and `done` or `refused`. Reads such as
  `online` are not written.
- A change the game makes after the answer, such as a give, adds a line to
  `audit.log` when part of it failed. RCON logins are written too.
- `chat.log` holds one line per chat line: the time in UTC, the player's
  name and account id, the kind of line and its text.

When the chat log is on, everything players write in chat is kept on the
server. It is on for a dedicated server and off for a game a player hosts,
unless the world's **Chat log** setting says otherwise.

`builds.log` sits beside them while the build log is on. Past 1 MB a log
moves to `audit.1.log`, `chat.1.log` or `builds.1.log` and starts again.

## Install

Admin Tools comes in its own archive, enshwick-admin-tools.zip. It needs
Enshwick, which comes in enshwick.zip. The
[install guide](https://enshwick.com/guides/install/) shows where to
put both archives.

## Multiplayer

Only the server needs Admin Tools. Players without Enshwick can join and use
the commands in chat.

## Problems

**`/adm` posts the line to chat.** Check that Admin Tools is in the server's
`mods/` folder. If it is, the server's log says why Admin Tools is off.

**The answer says no player has that name.** Type the name as the game shows
it. A player who is away is known only after they tried to join this world;
use `/adm account <id>` for any other account.

**A player is kicked as they join.** The allow-list is on, and the player is
not on it, or the player is banned. Lift a ban with
`/adm banned <player> off`.

**An RCON tool cannot connect.** Check that the port is open in the
firewall, and that the server's log says the remote console listens. When
another program uses the port, set another in **Remote console port**.

**Something else goes wrong.** Open an issue in the
[mods repository](https://github.com/enshwick/mods/issues). Attach
the files from the `logs/` folder in the server folder.
