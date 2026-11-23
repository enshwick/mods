---
id: enshwick.map-markers
name: Map Markers
summary: Your map marks the loot chests and the resources around you, such as plants, within 150 metres by default. The markers follow you as you move.
needs: everyone
settings:
  - key: chests
    default: true
    meaning: Marks the loot chests the world comes with. Turn it off to hide them.
    applies: at-once
  - key: resources
    default: true
    meaning: Marks plants and the other nodes you gather from. Turn it off to hide them.
    applies: at-once
  - key: radius
    default: 150.0
    range: 10 to 1000
    meaning: How far from you the mod marks things across the ground, in metres.
    applies: at-once
---

- Your map marks the loot chests around you in yellow.
- Your map marks plants and the other nodes you gather from in green.
- Each marker has a label: Chest or Resource.
- The markers follow you. Every two seconds, the map shows what is near you
  now.

## How many markers does the map show?

The map shows at most 256 markers from the mod, the nearest ones. In a place
with more, the markers further away show once you come closer.

## Does the map mark chests I have already looted?

Yes. The mod marks every loot chest the world comes with, looted or not.

## Install

Map Markers comes in its own archive, enshwick-map-markers.zip. It needs
Enshwick, which comes in enshwick.zip. The
[install guide](https://enshwick.com/guides/install/) shows where to
put both archives.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.com/guides/change-mod-settings/)
shows.

## Multiplayer

The server and every player need Map Markers in the same version. On a server,
the server's values decide what the maps of all players mark.

## Problems

**The map shows no markers.** Check that `chests` or `resources` is on. Walk
closer to a chest or a plant than the `radius`.

**Something else goes wrong.** Open an issue in the
[mods repository](https://github.com/enshwick/mods/issues). Attach
the files from the `logs/` folder in your game folder.
