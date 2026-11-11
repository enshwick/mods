# Test Content

A development mod, never released: content of its own that the in-game
checks of the platform's content API name. It runs on the server and on
every client that joins, since a client draws the weather from its own copy
of the game's data. Like every developer mod, it runs only where the file
`mods/enshwick/developer-tools` exists, on the server and on each client.
Keep it off servers people play on.

## What it adds

| Content | What it is |
| --- | --- |
| `enshwick.test-content:rain` | a copy of one of the game's rain phenomena in the first weather layer, which holds no rain of its own, picked there with probability 1, 15 km across and lasting 5 to 10 minutes |
| `enshwick.test-content:wolf` | an enemy copied from the vanilla wolf `Enemy_Wildbeast_wolf_earlygame`, at level 30 with twice its health |
| `enshwick.test-content:altar` | a Flame Altar kind copied from the vanilla Flame Altar, named Test Flame Altar: its level-1 zone is 40 cells wider on each side, twice the vanilla width, at the vanilla height, and it stays at level 1, as the game refuses its upgrade to level 2 |
| `enshwick.test-content:chest` | a prop copied from the vanilla wooden chest, named Test Chest, with a recipe that costs what the wooden chest's does, listed after it in the build menu |

A rain front in the first weather layer is therefore this mod's rain: the
probe's `weather` scenario waits for one.

## What a player gets

Where a server runs, on a dedicated server or in a game a client hosts:

- each player who joins gets one Test Flame Altar and one Test Chest, and
  learns the Test Chest's recipe;
- 30 seconds after the first player joins, the wolf spawns 10 metres in
  front of them, facing them. It stays out of the world's save.

The log says what each part did, and the Mods screen names any part that
turned off and why; the others keep working.

## Keep the test world for this mod

The game saves a weather front by its index among the phenomena, and this mod
adds one. A world saved with this mod and loaded without it shows each
saved front of a later layer as the phenomenon next to it until the front
ends. Load a test world with this mod every time, or use a copy of a world.

Licensed under [MIT](../LICENSE).
