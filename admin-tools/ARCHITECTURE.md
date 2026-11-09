# Architecture

One crate, a Windows DLL the Enshwick loader loads from the game's `mods/`
folder. The mod holds only its own logic: what the game needs from it goes
through the `enshwick` API, and what another mod could use belongs in the
API, not here.

Every feature is an action of the API's `actions` module, so the server
checks each run, and the same action runs from chat, the console and remote
consoles as a command. `Tools::add` declares each one behind the world's
rights and writes a run that changed something to the audit log.

| Path | What |
| --- | --- |
| `src/lib.rs` | the mod's start, the state its handlers share, and the rights and audit wrapper |
| `src/ids.rs` | the actions' ids and the command names |
| `src/players.rs` | actions on players, and the check of each player who joins |
| `src/server.rs` | actions on the whole server, and the tick that runs the schedule |
| `src/settings.rs` | the world's settings and who may use each action |
| `src/accounts.rs` | the ban list and the allow-list, kept in the world's data |
| `src/schedule.rs` | announcements and restart warnings as plain state |
| `src/audit.rs` | the audit log, written by a thread of its own |
| `tests/` | each action through the fake loader |
