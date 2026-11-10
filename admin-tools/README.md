# Admin Tools

Admin Tools gives a server's admins what the game lacks, in a window and as
commands.

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

Built on [Enshwick](https://github.com/enshwick/enshwick).

## Remote console

On a dedicated server the remote console is on from the first start. It
speaks Source RCON, the protocol of tools such as rcon-cli, mcrcon and
BattleMetrics. Send a command without the `/`, such as `adm online`.

| | |
| --- | --- |
| Address | `127.0.0.1`, the server's own computer |
| Port | `25575` |
| Password | in `rcon-password.txt` in `<save>-mod-data/enshwick.admin-tools/` beside the world's save |
| On by default | on a dedicated server; off in a game a player hosts |

RCON is not encrypted: open the port only to computers you trust, or use an
SSH tunnel or a VPN.

## Commands

Every action is a command in chat, in the console window of a dedicated
server and in an RCON tool: `/adm <action>` or `/admin-tools <action>`.
`/adm help` lists the actions you may use. An argument in square brackets
may be left out.

| Command | What it does | By default |
| --- | --- | --- |
| `/adm online` | Lists the players online, their account ids and time online. | Everyone |
| `/adm kick <player>` | Removes a player from the game. They may join again. | Admins |
| `/adm muted <player> on` or `off` | Mutes a player in chat or lifts the mute. | Admins |
| `/adm ban <player> <reason>` | Bans a player, online or away, with a reason. | Admins |
| `/adm bans` | Lists the newest 50 bans with their reasons. | Admins |
| `/adm banned <player> on` or `off` | Bans a player or lifts a ban. | Admins |
| `/adm allowed <player> on` or `off` | Puts a player on the allow-list or takes them off. | Admins |
| `/adm account <id> ban`, `unban`, `allow` or `disallow` | Changes an account by its id, also one that never joined. | Admins |
| `/adm mark <player>` | Moves a player to the waypoint on your map. | Admins |
| `/adm goto <player>` | Takes you to a player. | Admins |
| `/adm bring <player>` | Brings a player to you. | Admins |
| `/adm send <player> <player>` | Moves the first player to the second. | Admins |
| `/adm place <player> <x> <y> <z>` | Moves a player to a position in metres. | Admins |
| `/adm heal <player>` | Gives a player their full health. | Admins |
| `/adm stats <player> health`, `stamina` or `mana` `<value>` | Sets a player's health, stamina or mana. | Admins |
| `/adm learn <player> <recipe>` | Unlocks a recipe for a player. | Admins |
| `/adm god <player> on` or `off` | Nothing damages the player until they leave. | Admins |
| `/adm give <player> <item> [count]` | Gives a player items, one when the count is left out. | Admins |
| `/adm clear <player>` | Destroys everything a player carries and wears. | Admins |
| `/adm copy <player> <player>` | Gives the second player new items of everything the first carries. | Admins |
| `/adm chest <player> <item> [count]` | Places a chest of items beside a player. | Admins |
| `/adm chestat <x> <y> <z> <item> [count]` | Places a chest of items at a position. | Admins |
| `/adm motd` | Shows the message of the day. | Everyone |
| `/adm setmotd <text>` | Sets the message of the day. `-` goes back to the world's setting. | Admins |
| `/adm say <text>` | Shows every player a line now. | Admins |
| `/adm restart <minutes>` | Warns every player of a restart and saves when the time is up. `0` stops it. Only on a dedicated server. | Admins |
| `/adm status` | Shows the uptime, the world and the mod versions. | Everyone |
| `/adm time <time>` | Sets the time of day, such as `18:30`, or `6.5` for 06:30. | Admins |
| `/adm weather <weather>` | Starts `clear`, `rain`, `blizzard`, or `any` for the game's choice. | Admins |
| `/adm save` | Saves the world now. | Admins |
| `/adm world` | Shows the world, its last save and its game settings. | Admins |
| `/adm backup` | Copies the world to a backup folder while the server runs. | Admins |
| `/adm audit <lines>` | Shows the last lines of the audit log, up to 50. | Admins |
| `/adm chat <lines>` | Shows the last lines of the chat log, up to 50. | Admins |
| `/adm who <metres>` | Shows the last 10 changes to buildings and terrain near you, up to 200 metres. | Admins |
| `/adm panel` | Opens the Admin Tools window. | Admins |
| `/adm open <section>` | Opens one section of the window: `players`, `accounts`, `server` or `builds`. | Admins |
| `/adm player <player>` | Opens the window of one player. | Admins |

Admins are the players whose user group gives `CanKickBan`. Name an item by
its debug name, such as `Tool_Glider_T1`, its GUID or a mod's content id; in
a game a player hosts, also by the name the game shows.

## Window

Shift+F9 opens the Admin Tools window, with the players and each player's
actions, the bans and the allow-list, the server, and the build log. Each
section shows only the actions you may use.

## Who may use each action

Each world sets a rule for each action, under **Who may use each action** in
its settings.

| Rule | Who may use the action |
| --- | --- |
| `everyone` | Every player. |
| `nobody` | No player. The server's console and RCON tools still may. |
| `CanKickBan` | Admins. |
| `CanAccessInventories`, `CanEditBase`, `CanEditWorld`, `CanExtendBase` or `CanReceiveExp` | Players whose user group gives that permission. |

## Logs

The logs are in `<save>-mod-data/enshwick.admin-tools/` beside the world's
save.

| File | What it holds | On by default |
| --- | --- | --- |
| `audit.log` | Each admin action that changes something, done or refused, and RCON logins. | On a dedicated server |
| `chat.log` | Each chat line with the player's name and account id. | On a dedicated server |
| `builds.log` | Each building placed or removed and each terrain change, with who did it. | No |

## Settings

Each world has its own settings, behind the gear of Admin Tools on the Mods
screen.

| Setting | Default | What it does |
| --- | --- | --- |
| `motd` | empty | What each player reads when they join. |
| `allowlist` | off | Only accounts on the allow-list and admins may join. |
| `announcements` | none | Texts every player reads in turn, up to 16. |
| `announce_minutes` | 30 | Minutes between two announcements. |
| `restart_text` | empty | A line after the time of each restart warning. |
| `audit`, `chat_log` | auto | Whether the audit log and the chat log are written. Auto is on for a dedicated server. |
| `build_log` | off | Whether the build log is written. |
| `backups_kept` | 10 | How many backups the backup action keeps. |
| `backup_dir` | empty | The folder backups go to, beside the world's save. |
| `rcon` | auto | Whether RCON tools may connect. Auto is on for a dedicated server. |
| `rcon_ip`, `rcon_port` | `127.0.0.1`, 25575 | Where the remote console listens. |
| `rcon_password` | empty | The RCON password, at least 16 characters. Empty uses `rcon-password.txt`. |
| `rights.<action>` | `CanKickBan`; `everyone` for `online`, `status` and `motd` | Who may use each action. |

The [player page](docs/player.md) explains each task step by step.

## Install

Admin Tools has no release yet. It needs the Enshwick loader.

## Who needs it

Only the server. Players without Enshwick can join and use the commands in
chat.

Licensed under [MIT](LICENSE).
