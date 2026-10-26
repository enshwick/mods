# Game Rates

Game Rates changes how much some parts of the game give:

- The vanilla recipes for building blocks, roofs, overgrowth, decor and
  furniture make twice as much for the same cost by default. Functional props
  (stations, chests, storage shelves, altars, water devices) stay as they are.
  The crafting menu shows the new counts.
- Every piece of furniture gives twice its comfort inside a base by default,
  so a modest house reaches a long rested bonus. Only the best piece of each
  comfort category counts, as in vanilla.

## Settings

`mod-configs/game-rates.toml`, written on first start; each world keeps its
own copy of it (see [Settings](https://github.com/enshwick/enshwick#settings)):

```toml
recipe_multiplier = 2
recipe_blocks = true
recipe_overgrowth = true
recipe_decor = true
comfort_multiplier = 2.0
```

| Key | Default | Range | Meaning |
| --- | --- | --- | --- |
| `recipe_multiplier` | 2 | 2 to 100 | the output of each building recipe for the same cost |
| `recipe_blocks` | true | | building blocks and roofs make more |
| `recipe_overgrowth` | true | | overgrowth materials make more |
| `recipe_decor` | true | | decor and furniture make more |
| `comfort_multiplier` | 2.0 | 0 to 100 | comfort of each piece of furniture; 1 is vanilla, 0 takes all comfort from furniture |

Each piece's comfort is rounded to a whole number. The server's recipe values
are the ones in force: a player who joins sees the server's counts in the
crafting menu, and the file's own again after leaving. A change applies while
the game runs; when the game cannot rebuild its recipes, the log says so and
the new counts wait for the next start.

## Coming from older mods

Game Rates replaces Building materials, Building yield and Furniture comfort.
The loader never starts their old files in `mods/` beside it and logs that
they can be deleted. On the first start their settings move into Game Rates,
in `mod-configs/` and in each world's copy:

| Old setting | New key |
| --- | --- |
| `multiplier`, `blocks`, `overgrowth`, `decor` of `building-materials.toml` or `building-yield.toml` | `recipe_multiplier`, `recipe_blocks`, `recipe_overgrowth`, `recipe_decor` |
| `multiplier` of `furniture-comfort.toml` | `comfort_multiplier` |

A key that `game-rates.toml` already sets is never overwritten. Building
materials wins over Building yield when both are there. The `own_workbench`
key of Building yield is dropped: placed Bulk Workbenches load as plain
workbenches, and Bulk Workbench items in inventories are lost.

## Install

Download [`enshwick-game-rates.zip`](https://github.com/enshwick/mod-game-rates/releases/download/dev/enshwick-game-rates.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

The server and every player: the recipes change on each side, and the mod
counts in the join check. Comfort is decided by the server, so players see
the higher comfort and rested time; item tooltips keep their vanilla numbers.

Licensed under [MIT](LICENSE).
