# Admin Tools

Admin Tools gives a server's administrators what the game lacks: a player
list, kicks, bans, an allow-list, chat mutes, announcements, restart
warnings, time and weather, healing, teleports, items and chests of items,
inventories, health, stamina and mana, recipes, god mode, saves and backups,
a remote console, and audit and chat logs.

Built on [Enshwick](https://github.com/enshwick/enshwick).

## On the server

Every action is a command in chat and in the dedicated server's console:
`/adm <action>` or `/admin-tools <action>`, such as `/adm kick Bob`.
`/adm help` lists the actions the caller may use. Each world decides who may
use each action, by the permissions of the game's user groups. A ban holds
when the account comes back, and the allow-list keeps out accounts not on it.
Changes go to `audit.log` and chat lines to `chat.log` beside the world's
save. `/adm backup` copies the world while the server runs. On a dedicated
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
