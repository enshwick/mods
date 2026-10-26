---
id: build-camera
name: Build camera
summary: Every player may use the building fly mode of the Airborne Architect skill near a base, with no skill points spent.
needs: everyone
settings:
  - key: enabled
    default: true
    meaning: Every player may use the building fly mode without the skill. Turn it off to need the skill again.
    applies: at-once
---

- Every player may use the building fly mode near a base.
- No skill points go into Airborne Architect for it.
- Your character stays put while a flying spark carries the camera.
- You place and remove blocks and props from the spark.

## When does fly mode work?

The usual limits of the base game stay. Fly mode starts only near a building
zone, for a player allowed to edit the base. You must be alive and not gliding.
Fly mode ends when one of these no longer holds.

Build reach is the usual one, measured from the spark.

## What happens to the skill tree?

The skill tree still shows Airborne Architect as locked. Points you already
spent on it stay spent. Other skills and what they need stay the same.

## Install

Build camera comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It runs on enshwick.

## Settings

You change this value in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

The server and every player need Build camera in the same version. On a server,
every player runs with the server's value of `enabled`.

## Problems

**Fly mode does not start.** Move closer to a building zone where you may edit
the base. Land first if you are gliding.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-build-camera/issues). Attach
the files from the `logs/enshwick/` folder in your game folder.
