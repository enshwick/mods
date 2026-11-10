# Architecture

One crate, a Windows DLL the Enshwick loader loads from the game's `mods/`
folder. The mod holds only its own logic: what the game needs from it goes
through the `enshwick` API, and what another mod could use belongs in the
API, not here.

Every feature is an action of the API's `actions` module, so the server
checks each run, and the same action runs from chat, the console and remote
consoles as a command. `Tools::add` declares each one behind the world's
rights and writes a run that changed something to the audit log. The
windows only name declared actions: their buttons run the same actions, so
the server checks a press as it checks a command.

| Path | What |
| --- | --- |
| `src/lib.rs` | the mod's start, the state its handlers share, and the rights and audit wrapper |
| `src/ids.rs` | the actions' ids and the command names |
| `src/players.rs` | actions on players, and the check of each player who joins |
| `src/character.rs` | health, stamina and mana, recipes and other knowledge, and god mode through a damage filter |
| `src/inventory.rs` | giving items, clearing an inventory and copying one to another |
| `src/chest.rs` | a wooden chest placed by a world transaction, filled once it stands |
| `src/items.rs` | items and recipes by the game's name, GUID, content id or number, with near names |
| `src/server.rs` | actions on the whole server, the tick that runs the schedule, and the chat log's lines |
| `src/settings.rs` | the world's settings and who may use each action |
| `src/accounts.rs` | the ban list, the allow-list, the accounts last seen at the door and the message of the day set in game, kept in the world's data |
| `src/windows.rs` | the Admin Tools window, its sections and each player's window, built per viewer from the actions they may use, and shown again after their change |
| `src/builds.rs` | the build log: buildings placed and removed, building blocks and terrain changed, and the lookup of what changed near a spot |
| `src/schedule.rs` | announcements and restart warnings as plain state |
| `src/journal.rs` | the audit and chat logs, each written by a thread of its own |
| `src/backup.rs` | backups: the saves held, the copy on a thread of its own, old backups removed |
| `src/rcon.rs` | the remote console: a Source RCON listener, logins and its bounds |
| `src/rcon/packet.rs` | RCON packets, read and written without I/O |
| `tests/` | each action through the fake loader |
| `../fuzz/` | the fuzz target of the RCON packet reader |

The remote console runs on threads of its own and never touches the game:
each command line goes to the game thread through the API's
`commands::run_later`, with the rights of the server's console, as a
command typed there.
