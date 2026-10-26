---
id: enshwick.personal-safes
name: Personal Safes
summary: Adds the Safe, a chest that only its owner and the players they allow can open. Only the owner picks it up.
needs: everyone
settings:
  - key: cost_multiplier
    default: 2
    range: 1 to 100
    meaning: The Safe costs the ingredients of the wooden chest times this.
    applies: at-once
  - key: admins_can_open
    default: true
    meaning: The server's administrators may open, remove and rename every Safe.
    applies: at-once
  - key: label_language
    default: en
    range: a language code, such as en, uk or de
    meaning: The language of the owner's name on a Safe. Safes named from then on use it.
    applies: at-once
---

- The Safe is a chest you craft at the same workshop as the wooden chest.
- Only you and the players you allow may open your Safe.
- Only you pick it up or take it apart.
- Hover over a Safe to see its owner, such as "Safe · Ann".

## How do I let another player in?

Stand within 4 metres of your Safe and press the actions key, F7 by default. A
window lists the players online, each with a switch.

A player you switch on gets the same use of the Safe as you, except picking it
up. You may switch them off at any time, even while they are away.

## What happens without the mod?

A Safe is a wooden chest underneath. Without the mod, it is an ordinary chest
with every item still in it. You cannot upgrade a Safe.

## Install

Personal Safes comes in enshwick.zip, and the
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It runs on enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

The server and every player need Personal Safes in the same version. On a
server, the crafting menu shows the server's cost.

## Problems

**You cannot open your own Safe after joining.** The server registers you a
moment after you arrive. Wait a moment and try again.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-personal-safes/issues).
Attach the files from the `logs/enshwick/` folder in your game folder.
