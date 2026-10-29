---
id: enshwick.better-altars
name: Better Altars
summary: More Flame Altars at each Flame level, up to 255, in up to 179 bases, and altar zones that touch grow together so friends can build side by side.
needs: everyone
settings:
  - key: count.altars_per_flame_level
    default: "[2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32]"
    range: 1 to 255 for each level
    meaning: How many altars each Flame level allows. The first number is level 0, which comes before the Flame level 1 a new world starts at. Later levels use the last number. An empty list keeps the usual counts.
    applies: at-once
  - key: count.bases
    default: 16
    range: 16 to 179
    meaning: How many bases a world holds. Altars whose zones touch share a base. The game refuses an altar that would start one base more.
    applies: restart
  - key: zones.clusters
    default: true
    meaning: Altars whose zones touch grow together and share their area.
    applies: world-load
  - key: zones.building_across_seams
    default: true
    meaning: You may place pieces across the seam between two zones of one group.
    applies: at-once
---

- Each Flame level allows twice as many Flame Altars as in the base game: 4 at
  Flame level 1, up to 32 from level 11 on. You choose the count.
- Flame Altar zones that touch form a group. Each altar of the group grows, so
  the group covers as much ground as its altars would apart. No zone shrinks.
- You may place a piece across the seam between two zones of a group.

## Who needs it?

The server and every player need it.

## What happens past 16 altars?

Up to 255 altars fit in 16 bases: altars whose zones touch share one. The game
refuses an altar that would start a 17th base. Set `count.bases` to allow up to
179 bases, then start the game again. Without the mod, only the first 16 altar
zones work as bases.

## Install

Better Altars comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It needs Enshwick. Delete More Altars and Flame altar zones: Better
Altars takes over their settings.

## Settings

The
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows how to change them in the game.

## Problems

**The game refuses a new altar as "too many altars".** Place it touching one of
your bases.

**The zones did not grow after you changed `clusters`.** Load the world again.

**A dedicated server stops before a world with more than 16 altars loads.**
Update Enshwick and the mod. `logs/enshwick/loader.log` says why.

**Anything else.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-better-altars/issues). Attach
the files from the `logs/enshwick/` folder in your game folder.
