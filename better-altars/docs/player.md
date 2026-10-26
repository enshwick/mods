---
id: enshwick.more-altars
name: More Altars
summary: Sets how many Flame Altars a world allows at each Flame level, up to 32 altars in 16 bases. The default doubles the usual count.
needs: everyone
settings:
  - key: altars_per_flame_level
    default: "[2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32]"
    range: 1 to 32 for each level
    meaning: How many altars each Flame level allows. The first number is level 0, which comes before the Flame level 1 a new world starts at. Later levels use the last number. An empty list keeps the usual counts.
    applies: at-once
---

- By default, each Flame level allows twice as many Flame Altars as in the base
  game: 4 at Flame level 1, up to 32 from level 11 on.
- You choose the count for each Flame level.
- A world holds at most 16 bases.

## What happens past 16 altars?

Altars whose zones touch share one base, as in the base game. The game refuses
an altar whose zone touches no base once a world has 16 bases.

## Install

More Altars comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It runs on enshwick.

## Settings

You change the counts in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows: each Flame level has its own slider from 1 to 32. A value outside 1 to
32 in the file is set to the nearest limit, with a warning in the log. On the
first start the mod takes the counts you set under its former name,
`flame-altar-count`.

## Multiplayer

The server and every player need More Altars in the same version. On a server,
every player runs with the server's counts. If you host a world with more than
16 altars, keep the mod on and up to date.

## Problems

**The game refuses a new altar as "too many altars".** Its zone would start a
17th base. Place it so its zone touches one of your bases.

**A dedicated server stops before a world with more than 16 altars loads.** The
mod is missing, off or does not fit the game version, and the game might crash
without it. Update enshwick and the mod. `logs/enshwick/loader.log` says why the
server stopped.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-more-altars/issues). Attach
the files from the `logs/enshwick/` folder in your game folder.
