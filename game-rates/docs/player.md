---
id: enshwick.game-rates
name: Game Rates
summary: Building recipes make more, and furniture gives more comfort. Experience, workshops, loot and skill points each have a multiplier.
needs: everyone
settings:
  - key: recipes.multiplier
    default: 2
    range: 2 to 100
    meaning: How many times more each building recipe makes for the same cost.
    applies: at-once
  - key: recipes.blocks
    default: true
    meaning: Building blocks and roofs make more.
    applies: at-once
  - key: recipes.overgrowth
    default: true
    meaning: Overgrowth materials make more.
    applies: at-once
  - key: recipes.decor
    default: true
    meaning: Decor and furniture of the crafting menu's Decorative and Comfort Furniture groups make more.
    applies: at-once
  - key: comfort.multiplier
    default: 2.0
    range: 0 to 100
    meaning: How many times more comfort each piece of furniture gives. 1 is the base game, 0 takes all comfort from furniture.
    applies: at-once
  - key: experience.combat
    default: 1.0
    range: 0 to 100
    meaning: How many times more experience combat gives. 1 is the base game.
    applies: at-once
  - key: experience.mining
    default: 1.0
    range: 0 to 100
    meaning: How many times more experience mining gives. 1 is the base game.
    applies: at-once
  - key: experience.exploration
    default: 1.0
    range: 0 to 100
    meaning: How many times more experience exploring, lore, quests and item sets give. 1 is the base game.
    applies: at-once
  - key: workshops.speed
    default: 1.0
    range: 0.1 to 100
    meaning: How many times faster workshops such as the smelter produce. 2 halves the time.
    applies: at-once
  - key: drops.resources
    default: 1.0
    range: 0 to 100
    meaning: How many times more materials each loot stack and resource drop gives, as the game's own Loot Amount does.
    applies: at-once
  - key: drops.loot
    default: 1.0
    range: 0 to 100
    meaning: How many times larger each stack of loot in chests, corpses and animals is, up to a full stack.
    applies: at-once
  - key: skills.points
    default: 1.0
    range: 0 to 100
    meaning: How many times more skill points each level gives, rounded. Points from knowledge stay.
    applies: at-once
---

- Recipes for building blocks, roofs, overgrowth, decor and furniture make twice
  as much for the same cost.
- Each piece of furniture gives twice its comfort inside a base.
- The crafting menu shows the new counts.
- Experience from combat, mining and quests, workshop speed, loot and skill
  points per level each have a multiplier. They start as in the base game.

## What stays as it was?

Crafting stations, chests, storage shelves, altars and water devices keep their
usual recipes. Comfort furniture makes more, like other decor and furniture.
Only the best piece of each comfort category counts, as in the base game. Item
tooltips show the usual comfort numbers. Your rested time uses the new ones.

## Did you use an older building or comfort mod?

Game Rates replaces Building materials, Building yield and Furniture comfort.
Their old files no longer load, and you can delete them. On the first start,
their settings move into Game Rates. Settings under the former id `game-rates`
move too.

Placed Bulk Workbenches turn into plain workbenches. You lose the Bulk Workbench
items in inventories.

## Install

Game Rates comes in enshwick.zip. The
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It needs Enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

The server and every player need Game Rates in the same version. On a server,
everyone gets the server's values. After leaving, each player has their own
values again.

## Problems

**The crafting menu still shows the old counts.** The game could not rebuild its
recipes while it runs. Restart the game to see the new counts.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-game-rates/issues). Attach
the files from the `logs/enshwick/` folder in your game folder.
