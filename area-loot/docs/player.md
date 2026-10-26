---
id: enshwick.area-loot
name: Area loot
summary: Empty one enemy corpse, and the loot of the enemy corpses and loot bags around it comes along.
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
  comes to you too.
- The nearest loot comes first, with the usual pickup messages.
- The emptied corpses dissolve as usual.

## What if my inventory is full?

When your inventory fills up, the mod stops there. What does not fit stays in
its container, and loot further away stays where it is.

## Does it take loot another player wants?

No. A container stays with another player who has it open or opened it in the
last 10 seconds. It also stays with a player who opened it last and stands
within 4 metres of it.

## What stays as it was?

Chests, quest containers, items a player dropped and items on the ground work as
in the base game.

## Install

Area loot comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It runs on enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

Only the server, or the player who hosts the game, needs Area loot. Players
without it get the loot all the same.

## Problems

**Nothing comes along when you empty a corpse.** Check that `enabled` is on and
that the other loot is within `radius` and `height`.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-area-loot/issues). Attach the
files from the `logs/enshwick/` folder in your game folder.
