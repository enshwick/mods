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

A rain front in the first weather layer is therefore this mod's rain: the
probe's `weather` scenario waits for one.

## Keep the test world for this mod

The game saves a weather front by its index among the phenomena, and this mod
adds one. A world saved with this mod and loaded without it shows each
saved front of a later layer as the phenomenon next to it until the front
ends. Load a test world with this mod every time, or use a copy of a world.

Licensed under [MIT](../LICENSE).
