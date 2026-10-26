---
id: game-rates
name: Game Rates
summary: Building recipes make more for the same cost, and furniture gives more comfort inside a base.
needs: everyone
settings:
  - key: recipe_multiplier
    default: 2
    range: 2 to 100
    meaning: How many times more each building recipe makes for the same cost.
    applies: at-once
  - key: recipe_blocks
    default: true
    meaning: Building blocks and roofs make more.
    applies: at-once
  - key: recipe_overgrowth
    default: true
    meaning: Overgrowth materials make more.
    applies: at-once
  - key: recipe_decor
    default: true
    meaning: Decor and furniture make more.
    applies: at-once
  - key: comfort_multiplier
    default: 2.0
    range: 0 to 100
    meaning: How many times more comfort each piece of furniture gives. 1 is the base game, 0 takes all comfort from furniture.
    applies: at-once
---

- Recipes for building blocks, roofs, overgrowth, decor and furniture make twice
  as much for the same cost.
- Each piece of furniture gives twice its comfort inside a base.
- The crafting menu shows the new counts.

## What stays as it was?

Crafting stations, chests, storage shelves, altars and water devices keep their
usual recipes. Only the best piece of each comfort category counts, as in the
base game. Item tooltips show the usual comfort numbers, but your rested time
uses the new ones.

## Did you use an older building or comfort mod?

Game Rates takes the place of Building materials, Building yield and Furniture
comfort. Their old files no longer load, and you can delete them. Their settings
move into Game Rates on the first start.

Placed Bulk Workbenches turn into plain workbenches. You lose the Bulk Workbench
items in inventories.

## Install

Game Rates comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It runs on enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

The server and every player need Game Rates in the same version. On a server,
everyone gets the server's values, and their own again after leaving.

## Problems

**The crafting menu still shows the old counts.** The game could not rebuild its
recipes while it runs. Restart the game to see the new counts.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-game-rates/issues). Attach
the files from the `logs/enshwick/` folder in your game folder.
