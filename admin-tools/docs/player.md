---
id: enshwick.admin-tools
name: Admin Tools
summary: A server's administrators mute players in chat from the game's interface, with a switch per player.
needs: server
settings: []
---

- Administrators get a list of the players online with a switch each.
- A muted player's chat lines reach no other player.
- The muted player sees a notice that says so.
- Administrators and the host are never muted.

## How do I mute a player?

You must be an administrator of the server.

1. Open the pause menu, then **Mods**.
2. Select **Actions**, then **Muted in chat**.
3. Switch the player on to mute them. Switch them off to let them write again.

The reply under the list says what changed. Other players never see this list.

## How long does a mute last?

A mute lasts until an administrator lifts it. It stays after the server
restarts. The server keeps the mutes in `mod-data/enshwick/muted.txt`.

## Install

Admin Tools comes in its own archive, enshwick-admin-tools.zip. It needs
Enshwick, which comes in enshwick.zip. The
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put both archives.

## Multiplayer

Only the server needs Admin Tools. Players need Enshwick to see the list.

## Problems

**The list does not show.** Check that you are an administrator of the server.

**Something else goes wrong.** Open an issue in the
[mods repository](https://github.com/enshwick/mods/issues). Attach
the files from the `logs/enshwick/` folder in the server folder.
