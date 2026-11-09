# Test UI

A development mod, never released: the screen extensions, sounds and input
reads that the in-game checks of the platform's interface are read against.
It runs on the server and on clients. Keep it off servers people play on.

## What it adds

On the server, and in a game the client hosts:

- a build filter that refuses every placement within 5 m of a Flame Altar,
  with the reason `Too close to the altar`; a client joined to a dedicated
  server lists no altars, so there the server's refusal decides and tells
  the reason as a notice;
- the action `mark`, which names the survivor nearest the player
  `Test survivor`, as a player's rename does, so its prompt says so;
- the action `spawn-kept-and-unsaved`, which spawns two chests of the template
  of the placed container nearest the player, 4 m apart in front of them,
  logged as `Kept` and `Unsaved`, and 200 ms later takes `Unsaved` out of the
  world's save, logging the result of `World::set_saved`;
- the action `hello`, which answers with a notice.

On a client:

- a `Hello` button beside the survivor dialog, shown only after the last
  prompt the player looked at said `Test survivor`;
- a line of text beside the Flame Altar screen;
- a panel beside the storage chest screen of a Personal Safes Safe, by the
  Safe's GUID, that shows `Safe` and the host's position as x, y, z;
- a log line for each screen change, with its host;
- a window on Shift+F10 with the buttons `Play vanilla sound` (the interface
  sound Confirm), `Play mod sound` (a 1-second 440 Hz tone, 16-bit, 48 kHz,
  added as an interface sound), `Play creature sound`, `Spawn kept and
  unsaved` and `Mark the nearest survivor`, and two input hints for the
  interact control, one labelled `Interact` and one without a label;
- once a second while in a game, one log line when it changed: the controls
  held (`input::local_held`), the open screen, its host and template, the
  prompt, and what the two input hints drew.

The mod writes the tone into its own folder as `tone.wav` at start. It has no
button that raises a notifier at the player: the platform has no call for it
yet.

## Settings

`settings.toml` in the mod's own folder in `mods/`, read at start. Without the
file, both are empty:

```toml
# The GUID of a creature's sound container, for "Play creature sound".
creature_sound = ""
# The GUID of a vanilla sound container to play at volume 0.2.
quiet_sound = ""
```

## Install

1. Create the empty file `mods/enshwick/developer-tools` in the game folder,
   so the loader runs developer mods.
2. Unpack the archive from
   `cargo xtask dist --package enshwick-test-ui` into `mods/`, on the server
   and on each client.
3. Read `logs/enshwick.test-ui_<date>_<time>.log` in the game folder.

Licensed under [MIT](../LICENSE).
