---
id: enshwick.area-loot
name: Area Loot
summary: Empty one enemy corpse, and the mod also gives you the loot of the enemy corpses and loot bags nearby.
needs: server
settings:
  - key: radius
    default: 12.0
    range: 0.1 to 50
    meaning: How far the mod reaches across the ground from the emptied corpse, in metres.
    applies: at-once
  - key: height
    default: 4.0
    range: 0.1 to 20
    meaning: How far the mod reaches up and down from the emptied corpse, in metres.
    applies: at-once
  - key: enabled
    default: true
    meaning: Takes the loot around. Turn it off to loot as in the base game.
    applies: at-once
---

- When you empty an enemy corpse or loot bag, the enemy loot within 12 metres
  goes into your inventory too.
- You get the nearest loot first, with the usual pickup messages.
- The emptied corpses dissolve as usual.

## What if my inventory is full?

When your inventory fills up, the mod stops there. What does not fit stays in
its container, and loot further away stays where it is.

## Does it take loot another player wants?

No. The mod skips a container that another player has open or opened in the last
10 seconds. It also skips a container whose last opener stands within 4 metres
of it.

## What stays as it was?

Chests, quest containers, items a player dropped and items on the ground work as
in the base game.

## Install

Area Loot comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It needs Enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

Only the server, or the player who hosts the game, needs Area Loot. Players
without it get the loot too.

## Problems

**You get no extra loot when you empty a corpse.** Check that `enabled` is on
and that the other loot is within `radius` and `height`.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-area-loot/issues). Attach the
files from the `logs/enshwick/` folder in your game folder.
