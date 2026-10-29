# Game Rates

Game Rates changes how much parts of the game give. Building recipes make more,
furniture gives more comfort, and experience, workshops, loot and skill points
each have their own multiplier.

Built on [Enshwick](https://github.com/enshwick/enshwick).

- Recipes for building blocks, roofs, overgrowth, decor and furniture make twice
  as much for the same cost by default. The crafting menu shows the new counts.
- The mod reads what a recipe makes from the game's item data. Recipes of these
  kinds from other mods make more too. Another mod's multiplier for them
  multiplies with this one.
- Decor and furniture are the props in the crafting groups Decorative and
  Comfort Furniture. Comfort furniture makes more too.
- Functional props keep their recipes: stations, chests, storage shelves, altars
  and water devices.
- Each piece of furniture gives twice its comfort inside a base by default. A
  small house gets a long rested bonus. Only the best piece of each comfort
  category counts, as in vanilla.
- Experience from combat, mining and exploration, workshop speed, resource
  drops, loot stacks and skill points per level each have a multiplier. They
  start at 1, as in vanilla.

## Settings

The settings are in `mod-configs/enshwick.game-rates.toml`. The mod writes the
file on first start. Each world keeps its own copy (see
[Settings](https://github.com/enshwick/enshwick#settings)). The Mods screen
shows the groups under the mod's gear:

```toml
[recipes]
multiplier = 2
blocks = true
overgrowth = true
decor = true

[comfort]
multiplier = 2.0

[experience]
combat = 1.0
mining = 1.0
exploration = 1.0

[workshops]
speed = 1.0

[drops]
resources = 1.0
loot = 1.0

[skills]
points = 1.0
```

| Key                      | Default | Range      | Meaning                                                                               |
| ------------------------ | ------- | ---------- | ------------------------------------------------------------------------------------- |
| `recipes.multiplier`     | 2       | 2 to 100   | the output of each building recipe for the same cost                                  |
| `recipes.blocks`         | true    |            | building blocks and roofs make more                                                   |
| `recipes.overgrowth`     | true    |            | overgrowth materials make more                                                        |
| `recipes.decor`          | true    |            | decor and furniture of the crafting groups Decorative and Comfort Furniture make more |
| `comfort.multiplier`     | 2.0     | 0 to 100   | comfort of each piece of furniture; 1 is vanilla, 0 takes all comfort from furniture  |
| `experience.combat`      | 1.0     | 0 to 100   | experience from defeated enemies                                                      |
| `experience.mining`      | 1.0     | 0 to 100   | experience from mining                                                                |
| `experience.exploration` | 1.0     | 0 to 100   | experience from exploration, lore, quests and item sets                               |
| `workshops.speed`        | 1.0     | 0.1 to 100 | how fast timed workshops such as the smelter and the kiln produce; 2 halves the time  |
| `drops.resources`        | 1.0     | 0 to 100   | resources from mining, chopping and harvesting                                        |
| `drops.loot`             | 1.0     | 0 to 100   | each stack of random loot in chests, corpses and animals, up to the item's stack size |
| `skills.points`          | 1.0     | 0 to 100   | skill points each level gives, rounded                                                |

The mod rounds the comfort of each piece to a whole number. The server's values
are the ones in force. A player who joins a server sees the server's counts in
the crafting menu. After leaving, the player has the file's own values again.

A change applies while the game runs. When the game cannot rebuild its recipes,
the log says so. The new counts then apply at the next start.

## Coming from older versions and mods

Game Rates had the id `game-rates`, and it replaces Building materials, Building
yield and Furniture comfort. The loader does not start the old files of these
mods in `mods/` and logs that you can delete them. On the first start their
settings move into Game Rates, in `mod-configs/` and in each world's copy:

| Old setting                                                                                         | New key                                                                       |
| --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `recipe_multiplier`, `recipe_blocks`, `recipe_overgrowth`, `recipe_decor` of `game-rates.toml`      | `recipes.multiplier`, `recipes.blocks`, `recipes.overgrowth`, `recipes.decor` |
| `comfort_multiplier` of `game-rates.toml`                                                           | `comfort.multiplier`                                                          |
| `multiplier`, `blocks`, `overgrowth`, `decor` of `building-materials.toml` or `building-yield.toml` | `recipes.multiplier`, `recipes.blocks`, `recipes.overgrowth`, `recipes.decor` |
| `multiplier` of `furniture-comfort.toml`                                                            | `comfort.multiplier`                                                          |

A value from `game-rates.toml` wins over the values of the older mods. Building
materials wins over Building yield. Game Rates drops the `own_workbench` key of
Building yield. Placed Bulk Workbenches load as plain workbenches. Bulk
Workbench items in inventories are lost.

## Install

Download
[`enshwick-game-rates.zip`](https://github.com/enshwick/mod-game-rates/releases/download/dev/enshwick-game-rates.zip).
Extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. The mod needs the Enshwick loader. Get
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
for the loader alone, or
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
for the loader with every mod. These are development builds of the newest
commit. Until the first release, 1.0.0, these are the only builds.

## Who needs it

The server and every player need it. The recipes change on each side, and the
mod counts in the join check. The server decides comfort, experience and drops.
Players see the higher comfort and rested time. Item tooltips keep their vanilla
numbers.

Licensed under [MIT](LICENSE).
