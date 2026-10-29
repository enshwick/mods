---
id: enshwick.optimizer
name: Optimizer
summary: A dedicated server uses less CPU, on Windows and under Wine or Proton. Players notice nothing.
needs: server
settings:
  - key: exact_waits
    default: auto
    range: auto, on or off
    meaning: Short waits sleep instead of checking the clock over and over. "auto" turns them on for the dedicated server only. Turning them on after a start without them waits for the next start.
    applies: at-once
  - key: task_workers
    default: 0
    range: 0 to 64
    meaning: The most worker threads each task group of the dedicated server starts. 0 picks the number for you.
    applies: restart
  - key: server_wait_cap_ms
    default: 1
    range: 1 to 8
    meaning: The longest the server waits between its passes while players are online, in milliseconds. 1 is the base game. Raising it after a start at 1 with measure off waits for the next start.
    applies: at-once
  - key: measure
    default: true
    meaning: Writes a line a minute to the log with the server's CPU use and loop rates.
    applies: at-once
---

- A dedicated server uses less CPU, on Windows and under Wine or Proton.
- An idle Windows server went from 19.9 % to 7.9 % of one CPU core.
- An idle server under Proton in Docker went from 37.7 % to 16.1 %.
- Players notice nothing.

## What does it change?

The mod checks where it runs and turns on only what helped there. The server
sleeps through the last fraction of a short wait instead of checking the clock
over and over. Under Wine with a CPU limit, the server starts no more worker
threads than that limit.

The game keeps its 60 steps a second, so gliding and spawning work as before.

## Does my Linux host need anything?

Under Wine, ntsync and the `performance` CPU governor help more than any mod. At
start, a log line names what your host lacks.

## Install

Optimizer comes in enshwick.zip. The
[install guide](https://enshwick.github.io/docs/guides/install/) shows where to
put it. It needs Enshwick.

## Settings

You change these values in the game, as the
[settings guide](https://enshwick.github.io/docs/guides/change-mod-settings/)
shows.

## Multiplayer

Only the dedicated server needs Optimizer. In a game you host, every change
stays off unless a setting turns it on.

## Problems

**Gliding, joining or combat feels different.** If you raised
`server_wait_cap_ms`, set it back to 1.

**Something else goes wrong.** Open an issue in the
[mod's repository](https://github.com/enshwick/mod-optimizer/issues). Attach the
files from the `logs/enshwick/` folder in your game folder.
