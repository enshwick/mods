---
id: enshwick.survivors-per-altar
name: Survivors per Altar
summary: A survivor may have a copy at every Flame Altar. Each player's base can have its own Blacksmith, Alchemist and other survivors.
needs: everyone
settings:
  - key: per_altar
    default: true
    meaning: A copy of each survivor may stand at every altar. Turn it off to keep survivors and staffs as in the base game in that world.
    applies: world-load
  - key: copy_staffs
    default: true
    meaning: Copy staffs drop and work. When it is off, players keep the staffs they hold.
    applies: at-once
  - key: copy_staff_boss_chance
    default: 0.10
    range: 0 to 1
    meaning: The chance that a boss drops a copy staff. 0 turns this off.
    applies: at-once
  - key: copy_staff_locked_chest_chance
    default: 0.02
    range: 0 to 1
    meaning: The chance that a chest which needs a key or a lockpick holds a copy staff. 0 turns this off.
    applies: at-once
  - key: copy_staff_charges
    default: 3
    range: 1 to 100
    meaning: The charges a full copy staff holds.
    applies: restart
  - key: copy_staff_charges_min
    default: 0
    range: 0 to 100, at most copy_staff_charges
    meaning: The fewest charges a dropped staff holds, at random up to full. 0 drops every staff full.
    applies: at-once
  - key: vanilla_staff_places_copies
    default: false
    meaning: The survivor's own staff places and moves copies too, as a copy staff does.
    applies: at-once
---

- A survivor may have a copy at every Flame Altar, one per altar zone.
- Each copy works and sleeps at its own altar.
- Copy staffs make the copies. Bosses and locked chests drop them.

## How do I place a copy?

Aim a copy staff at an altar zone that has neither a copy nor the survivor. A
new copy appears there and the staff spends a charge. Aimed at a zone with a
copy, the staff moves that copy for free.

A full staff holds 3 charges, and the last one uses the staff up.

## What happens when I remove an altar or the mod?

The copies of a removed altar disappear ten seconds later. The survivor itself
stays. Without the mod, the game removes the copies and the copy staffs.

## Install

Survivors per Altar comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It needs Enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

The server and every player need Survivors per Altar in the same version. On a
server, every player runs with the server's `per_altar` and `copy_staffs`
values.

## Problems

**The staff refuses to place a copy.** The survivor itself stands in that zone,
or you aim outside every altar zone.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-survivors-per-altar/issues).
Attach the files from the `logs/enshwick/` folder in your game folder.
