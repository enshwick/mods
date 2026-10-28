---
id: enshwick.game-rates
name: Game Rates
summary: Building recipes make more, furniture gives more comfort, and experience, workshops, loot and skill points follow their own multipliers.
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
    meaning: Decor and furniture of the crafting menu's Decorative group make more.
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
usual recipes. So does comfort furniture: only the crafting menu's Decorative
group makes more. Only the best piece of each comfort category counts, as in the
base game. Item tooltips show the usual comfort numbers, but your rested time
uses the new ones.

## Did you use an older building or comfort mod?

Game Rates takes the place of Building materials, Building yield and Furniture
comfort. Their old files no longer load, and you can delete them. Their settings,
and those Game Rates kept under its former id `game-rates`, move into Game Rates
on the first start.

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
