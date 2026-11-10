---
id: enshwick.admin-tools
name: Admin Tools
summary: Server admins manage players, bans and the allow-list from a window, set time and weather, back up the world, and use RCON and logs.
needs: server
settings:
  - key: motd
    default: ""
    meaning: What each player reads when they join. Empty shows nothing.
    applies: at-once
  - key: allowlist
    default: false
    meaning: When on, only accounts on the allow-list and admins may join.
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

Admin Tools gives a server's admins what the game lacks, in a window
and as commands.

- **Players and access:** who is online, kicks, bans with reasons, chat
  mutes, an allow-list. Players who are away can be banned too.
- **Chat and announcements:** a message of the day, announcements on a
  timer, a line to everyone now, restart warnings.
- **World:** the time of day, the weather, saves, backups while the server
  runs, the world's game settings.
- **Characters and items:** heal, set health, stamina and mana, teach
  recipes, god mode, give items or a chest of items, clear or copy an
  inventory, move players.
- **Remote console:** every command from an RCON tool, without joining the
  game.
- **Logs:** an audit log of admin actions, a chat log and a build log.

## How do I connect an RCON tool?

On a dedicated server the remote console is on from the first start. It
speaks Source RCON, the protocol of tools such as rcon-cli, mcrcon and
BattleMetrics.

| | |
| --- | --- |
| Address | `127.0.0.1`, the server's own computer |
| Port | `25575` |
| Password | in `rcon-password.txt` in `<save>-mod-data/enshwick.admin-tools/` beside the world's save |
| On by default | on a dedicated server; off in a game a player hosts |

1. Open `rcon-password.txt`. Admin Tools writes a random password there at
   the first start, and the server's log names the file.
2. In the tool, enter the address, the port and that password.
3. Send a command without the `/`, such as `adm online`. Send `help` to list
   the commands of every mod.

RCON is not encrypted: the password and the commands cross the network as
plain text. To reach the console from another computer, use an SSH tunnel or
a VPN. Or set **Remote console address** to `0.0.0.0` and open the port only
to computers you trust.

To choose the password, set **Remote console password** in the world's
settings. To turn the console off, set **Remote console** to off. After three
wrong passwords from one address, the address waits a minute.

## What commands are there?

Every action is a command in chat, in the console window of a dedicated
server and in an RCON tool. It starts with `/adm` or `/admin-tools`. In the
console and an RCON tool, leave out the `/`. `/adm help` lists the actions
you may use, and `/adm help <action>` explains one. An argument in square
brackets may be left out.

| Command | What it does | By default |
| --- | --- | --- |
| `/adm panel` | Opens the Admin Tools window. | Admins |
| `/adm open <section>` | Opens one section of the window: `players`, `accounts`, `server` or `builds`. | Admins |
| `/adm player <player>` | Opens the window of one player. | Admins |
| `/adm online` | Lists the players online, their account ids and time online. | Everyone |
| `/adm kick <player>` | Removes a player from the game. They may join again. | Admins |
| `/adm muted <player> on` or `off` | Mutes a player in chat or lifts the mute. | Admins |
| `/adm ban <player> <reason>` | Bans a player, online or away, with a reason. | Admins |
| `/adm bans` | Lists the newest 50 bans with their reasons. | Admins |
| `/adm banned <player> on` or `off` | Bans a player or lifts a ban. | Admins |
| `/adm allowed <player> on` or `off` | Puts a player on the allow-list or takes them off. | Admins |
| `/adm account <id> ban`, `unban`, `allow` or `disallow` | Changes an account by its id, also one that never joined. | Admins |
| `/adm heal <player>` | Gives a player their full health. | Admins |
| `/adm stats <player> health`, `stamina` or `mana` `<value>` | Sets a player's health, stamina or mana. | Admins |
| `/adm learn <player> <recipe>` | Unlocks a recipe for a player. | Admins |
| `/adm god <player> on` or `off` | Nothing damages the player until they leave. | Admins |
| `/adm give <player> <item> [count]` | Gives a player items, one when the count is left out. | Admins |
| `/adm chest <player> <item> [count]` | Places a chest of items beside a player. | Admins |
| `/adm chestat <x> <y> <z> <item> [count]` | Places a chest of items at a position. | Admins |
| `/adm clear <player>` | Destroys everything a player carries and wears. | Admins |
| `/adm copy <player> <player>` | Gives the second player new items of everything the first carries. | Admins |
| `/adm goto <player>` | Takes you to a player. | Admins |
| `/adm bring <player>` | Brings a player to you. | Admins |
| `/adm send <player> <player>` | Moves the first player to the second. | Admins |
| `/adm place <player> <x> <y> <z>` | Moves a player to a position in metres. | Admins |
| `/adm mark <player>` | Moves a player to the waypoint on your map. | Admins |
| `/adm time <time>` | Sets the time of day, such as `18:30`, or `6.5` for 06:30. | Admins |
| `/adm weather <weather>` | Starts `clear`, `rain`, `blizzard`, or `any` for the game's choice. | Admins |
| `/adm save` | Saves the world now. | Admins |
| `/adm backup` | Copies the world to a backup folder while the server runs. | Admins |
| `/adm world` | Shows the world, its last save and its game settings. | Admins |
| `/adm status` | Shows the uptime, the world and the mod versions. | Everyone |
| `/adm motd` | Shows the message of the day. | Everyone |
| `/adm setmotd <text>` | Sets the message of the day. `-` goes back to the world's setting. | Admins |
| `/adm say <text>` | Shows every player a line now. | Admins |
| `/adm restart <minutes>` | Warns every player of a restart and saves when the time is up. `0` stops it. Only on a dedicated server. | Admins |
| `/adm who <metres>` | Shows the last 10 changes to buildings and terrain near you, up to 200 metres. | Admins |
| `/adm audit <lines>` | Shows the last lines of the audit log, up to 50. | Admins |
| `/adm chat <lines>` | Shows the last lines of the chat log, up to 50. | Admins |

Admins are the players whose user group gives `CanKickBan`. Admins and the
host cannot be kicked, banned or muted. `goto`, `bring`, `mark` and
`who` need a character, so the console cannot use them. A player who is away
is named as they last joined this world: Admin Tools remembers the last 256
accounts that tried to join, and every account on the ban list or the
allow-list.

## How do I name an item or a recipe?

- By its debug name, such as `Tool_Glider_T1`, in any case.
- By its GUID, or by a mod's content id, such as `you.lockers:locker`.
- In a game a player hosts, also by the name the game shows, such as
  `Glider`, in any language the game offers. A dedicated server does not
  have these names.
- An item by its number, and any knowledge by its number for `learn`.

An unknown name lists the names near it. `give`, `chest` and `chestat` hand
over as many stacks as the items need, and you read how many did not fit.
Items from `copy` start new, without wear or level, and the look and
currency slots are not copied. A chest outside every base is not saved.

## How do I open the Admin Tools window?

Press Shift+F9 in the game, or type `/adm panel`. The window has four
sections, and each shows only the actions you may use:

| Section | What it holds |
| --- | --- |
| Players | The players online. **Actions** opens one player's window: move, moderate, health, items and recipes. |
| Bans and the allow-list | The bans with their reasons, each with a button to lift it, and the allow-list. |
| Server | The message of the day, the restart warning, the time of day, the weather, saves and backups. |
| Build log | The changes near you, each with a button that takes you there. |

To change the key, open the Mods screen, select Admin Tools and change it
under **Key bindings**.

## How do I keep strangers out?

1. Put each player you trust on the allow-list with
   `/adm allowed <player> on`, or switch them on in the window's
   **Bans and the allow-list** section.
2. Open the Mods screen, select Admin Tools, and turn on **Allow-list** in the
   server's settings.

From then on, an account not on the list is kicked as it joins.
Admins always get in. A player kicked this way is remembered, so
you can allow them by name afterwards.

To allow an account that never joined, ask the player for their account id
and type `/adm account <id> allow`.

## How do I decide who may use each action?

Each action has a rule in the world's settings, under **Who may use each
action**. The table of commands shows each rule's default.

| Rule | Who may use the action |
| --- | --- |
| `everyone` | Every player. |
| `nobody` | No player. The server's console and RCON tools still may. |
| `CanKickBan` | Admins. |
| `CanAccessInventories`, `CanEditBase`, `CanEditWorld`, `CanExtendBase` or `CanReceiveExp` | Players whose user group in `enshrouded_server.json` gives that permission. |

A player sees only the actions their rule allows. A change applies within
two seconds.

## How do I warn players before a restart?

Type `/adm restart 10`. Every player reads that the server restarts in
10 minutes. The warning comes again at 5, 2 and 1 minutes left, and once more
when the time is up.

When the time is up, Admin Tools saves the world. It does not stop the
server: your host's panel or script stops it, so set the warning to end when
the panel restarts the server.

Type `/adm restart 0` to call the restart off. A game a player hosts has no
restart warning.

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
writes each building placed, picked up or torn down. It also writes each spot
where building blocks were placed or removed and terrain was dug or filled.

Each line has who did it, the time and the position.

Changes to terrain and building blocks name the nearest player within 16
metres, since the game does not say who made them.

1. Go to the spot.
2. Open the Admin Tools window and its **Build log** section, or type
   `/adm who 30` for the changes within 30 metres.
3. Press **Go there** beside a change to stand where it happened.

The log keeps nothing from before you turned it on.

## Where are the logs?

The logs are in `<save>-mod-data/enshwick.admin-tools/` beside the world's
save. Each line starts with the time in UTC.

| File | What it holds | On by default |
| --- | --- | --- |
| `audit.log` | Each run of an action that changes something, done or refused: who, from where, the action, its values and the answer. Also RCON logins, and a change the game made in part after the answer, such as a give. Reads such as `online` are left out. | On a dedicated server |
| `chat.log` | Each chat line: the player's name and account id, the kind of line and its text. | On a dedicated server |
| `builds.log` | Each building placed or removed, and each spot of building blocks or terrain changed, with who did it. | No |

A game a player hosts keeps no log until the world's settings turn it on.
When the chat log is on, everything players write in chat is kept on the
server. Past 1 MB a log moves to `audit.1.log`, `chat.1.log` or
`builds.1.log` and starts again.

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
