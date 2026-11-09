---
id: enshwick.admin-tools
name: Admin Tools
summary: A server's administrators mute players in chat from the dedicated server's console. A muted player's lines reach no other player.
needs: server
settings: []
---

- Administrators mute and unmute players in chat.
- A muted player's chat lines reach no other player.
- The muted player sees a notice that says so.
- Administrators and the host are never muted.

## How do I mute a player?

You type a line in the console window of a dedicated server. The server runs
it as an administrator.

1. Open the console window of the dedicated server.
2. Type `enshwick.admin-tools muted <player> on` to mute a player. Use the
   player's name as the game shows it.
3. Type `enshwick.admin-tools muted <player> off` to let the player write
   again.

The console answers on the next line and says what changed. The player must be
online. Other players never see these lines.

## Can I mute a player from inside the game?

No. Admin Tools has no window in the game. A game that a player hosts has no
console window, so its players cannot be muted.

## How long does a mute last?

A mute lasts in its world until an administrator lifts it. It stays after the
server restarts. Each world keeps its own mutes in
`<save>-mod-data/enshwick/muted.txt` beside its save.

## Install

Admin Tools comes in its own archive, enshwick-admin-tools.zip. It needs
Enshwick, which comes in enshwick.zip. The
[install guide](https://enshwick.com/guides/install/) shows where to
put both archives.

## Multiplayer

Only the server needs Admin Tools.

## Problems

**The console does not know the line.** Check that Admin Tools is in the
server's `mods/` folder. Check that the line starts with `enshwick.admin-tools`.

**The console finds no player with that name.** Check that the player is
online. Type the name as the game shows it.

**Something else goes wrong.** Open an issue in the
[mods repository](https://github.com/enshwick/mods/issues). Attach
the files from the `logs/` folder in the server folder.
