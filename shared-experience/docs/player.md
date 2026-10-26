---
id: enshwick.shared-experience
name: Shared experience
summary: Friends who play at different hours stay on the same character level. Every character catches up with the one furthest ahead.
needs: server
settings:
  - key: enabled
    default: true
    meaning: Every character catches up with the one furthest ahead in experience.
    applies: at-once
---

- Each world remembers the most experience any character has reached in it.
- Every character below that mark catches up, while online or on the next join.
- What anyone earns raises the mark for everyone.
- A new world starts without a mark.

## How does a character catch up?

A character behind the leader gains the difference one level at a time. Each
level comes with the level-up banner and skill points, as if earned in play.

## Does playing together level faster?

No. When two or more players earn experience at the same moment, such as from
one kill, only the largest gain counts, once.

## What stays as it was?

Nothing grows past the level cap. Skill points from knowledge stay with each
character. The experience a character gains here travels with it to other
worlds.

## Install

Shared experience comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It runs on enshwick.

## Settings

You change this value in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows. On the first start the mod keeps the value and each world's mark from
its former name, `shared-experience`.

## Multiplayer

Only the server, or the player who hosts the game, needs Shared experience.
Players without it see their bar fill and their level rise all the same.

## Problems

**A character who was away has not caught up.** The catch-up starts when that
character joins the world again. Join and wait a moment.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-shared-experience/issues).
Attach the files from the `logs/enshwick/` folder in your game folder.
