# Admin Tools

Admin Tools gives a server's administrators what the game lacks: a window
with the players and their actions, kicks, bans with reasons, an allow-list,
chat mutes, announcements, restart warnings, time and weather, healing,
teleports to a player, a position or a map waypoint, items and chests of
items, inventories, health, stamina and mana, recipes, god mode, saves and
backups, a remote console, a build log, and audit and chat logs.

Built on [Enshwick](https://github.com/enshwick/enshwick).

## On the server

Shift+F9 opens the Admin Tools window: the players online, each with a
window of every action on them, the bans and the allow-list, the server, and
the build log. Every action is also a command in chat and in the dedicated
server's console: `/adm <action>` or `/admin-tools <action>`, such as
`/adm kick Bob`. `/adm help` lists the actions the caller may use. Each world
decides who may use each action, by the permissions of the game's user
groups. A ban keeps its reason and holds when the account comes back, the
allow-list keeps out accounts not on it, and both take players who are away.
The build log, off by default, shows who placed or removed a building or
changed terrain near a spot.
Every run of an action that changes something goes to `audit.log`, done or
refused, and chat lines go to `chat.log` beside the world's save. `/adm backup` copies the world while the server runs. On a dedicated
server, RCON tools such as BattleMetrics or rcon-cli run every command on
port 25575 with the password Admin Tools generates; RCON is not encrypted,
so open the port only to hosts you trust. The
[player page](docs/player.md) lists every command and setting.

## Install

Download
`enshwick-admin-tools.zip` from its [newest release](https://github.com/enshwick/mods/releases?q=enshwick-admin-tools&expanded=true).
Extract it into the folder with `enshrouded_server.exe`. The mod needs the
Enshwick loader. Get
`enshwick.zip` from the [Enshwick release](https://github.com/enshwick/enshwick/releases)
that the mod's release notes name, and extract it into the same folder. Until 1.0 both are alphas, and anything may change from one alpha
to the next. Back up your saves before you update.

## Who needs it

Only the server.

Licensed under [MIT](LICENSE).
