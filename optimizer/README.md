# Optimizer

A dedicated Enshrouded server uses less CPU, on Windows and under Wine or
Proton, such as in the Docker images for Linux hosts. The mod also measures the
result. Players notice no change.

Built on [Enshwick](https://github.com/enshwick/enshwick).

The mod checks where it runs: on the dedicated server or in the game, on Windows
or under Wine. It turns on only the changes that measurements showed to help
there.

| Change          | Windows server | Wine server         | Windows client | Wine client |
| --------------- | -------------- | ------------------- | -------------- | ----------- |
| Exact waits     | on             | on                  | off            | off         |
| Task workers    | off            | on with a CPU limit | off            | off         |
| Server wait cap | off            | off                 | off            | off         |

CPU use of an idle dedicated server, where 100 % is one core (see
[Measurements](#measurements)):

| Server                        | Without the mod | With the mod |
| ----------------------------- | --------------- | ------------ |
| Windows                       | 19.9 %          | 7.9 %        |
| Proton in Docker              | 37.7 %          | 16.1 %       |
| Proton in Docker, 4-CPU limit | 38.2 %          | 6.7 %        |

## What it changes

- **Exact waits.** The engine's event loop waits for its next deadline in whole
  milliseconds. With less than a millisecond left, it asks for a 0 ms wait again
  and again until the deadline passes. This costs about 0.6 ms of every 60 Hz
  frame of the main loop. It costs about 0.9 ms of every 5 ms of the network
  session's loop, and 0.1 ms after every slow server pass. The mod sleeps for
  that last fraction instead. Then it lets the game's own 0 ms wait run once, so
  the game still sees every event. The mod waits on no handle of the game. Under
  Wine the sleep is a plain Unix sleep. On Windows it is a high-resolution
  timer, which wakes on average 0.17 ms after the deadline; the polling would
  end on the deadline. By default, exact waits are on for the dedicated server
  only, because the frame pacing of the game window has not been measured.
- **Task workers.** The engine starts one task worker thread per logical CPU, up
  to 8 per group, plus the ECS queue. Under Wine the engine counts the host's
  CPUs and ignores the container's limit. A server in a container limited to 4
  CPUs on a 32-thread host runs 8 workers per group. Each of them wakes hundreds
  of times a second. With a CPU limit, the mod starts no more workers than the
  limit rounded up, and at least 2. The game's own log shows the result:
  `[task] Group 'Default' Worker:6 Main:2` means 4 workers and 2 main slots. The
  dedicated server waits for the loader at start-up, so the cap is in place
  before the task system starts.
- **Server wait cap.** With players online, the server's game loop waits 1 ms
  after each pass. That makes 700 to 900 passes a second for 60 simulation
  steps. The cap lets the loop wait longer, but never past the next step. A wait
  can wake one timer tick late: the timer resolution on Windows, or a
  millisecond under Wine. So the longer wait ends one tick and half a
  millisecond before the step is due. Where that leaves less than 1 ms, the wait
  stays at 1 ms. The cap is off by default everywhere. Its saving was measured
  only with an emulated player (see below). Input that arrives between two steps
  can wait up to the cap.
- **Measure mode.** The mod writes a line a minute to the log and the status
  summary. The line holds the platform, the CPU of the process, its main thread,
  the server loop's thread and its busiest thread groups. It also holds the loop
  and step rates, the exact waits slept and how late they woke, and the server's
  "Bad Performance" warnings.
- **Environment report.** At start, one line names what the server runs on.
  Under Wine it names the Wine version and host, the fast synchronisation
  available (ntsync, esync, fsync), the CPU governor, the container's CPU limit
  and the CPUs the game saw. It adds a line of advice for each missing host
  feature that is known to help. On Windows it names the logical CPUs.

An earlier community fix made every wait of every thread longer. CPU use went
down, but players spawned under the ground and gliding broke. This mod does not
make waits longer. The exact waits sleep only for the time the game would spend
polling, and the simulation keeps its 60 steps a second.

```text
info  [enshwick.optimizer] environment: Wine 11.0 on Linux 7.0.14-linuxkit, Proton variables set; sync: WINEESYNC=1, WINEFSYNC=1, WINENTSYNC=1; governor: unknown; cgroup CPU limit: 4.00 CPUs; logical CPUs: 16
info  [enshwick.optimizer] the game started task workers: Default 6, Background 4, Shader 0 (it saw 16 logical CPUs)
info  [enshwick.optimizer] measure: Wine server: exact waits on, task workers at most 4, server wait vanilla; CPU: process 7.7%, main thread 1.1%, server loop thread 2.3% (100% = one core); server loop 24/s, steps 24.4/s; exact waits 257/s of 962 µs on average, 81 µs late; Bad Performance 0; threads: GameServer 2.3%, Session 1.8%, Task-Default x4 1.3%, main 1.1%, P2pReceiveThread 0.4%, SteamworksSystem 0.2%
```

## Settings

The settings file is `mod-configs/enshwick.optimizer.toml`. The mod writes it on
the first start. Each world keeps its own copy (see
[Settings](https://github.com/enshwick/enshwick#settings)). The mod's gear on
the Mods screen shows the settings in the player's game language.

```toml
exact_waits = "auto"
task_workers = 0
server_wait_cap_ms = 1
measure = true
```

- `exact_waits`: `"auto"` turns them on where the table above says on. `"on"`
  turns them on everywhere, the game included. `"off"` turns them off. Turning
  them off takes effect at once. Turning them on takes effect at the next start
  if they were off at start.
- `task_workers`: the most workers each task group of the dedicated server
  starts, from 1 to 64. The server starts its workers once. So the mod reads
  this value from `mod-configs/enshwick.optimizer.toml` for every world, and a
  change takes effect at the next server start. 0 means automatic: under Wine,
  the container's CPU limit rounded up and at least 2; otherwise as many as the
  game picks.
- `server_wait_cap_ms`: from 1 to 8. 1 is the base game. Before you keep a
  higher value, check in the game that gliding, joining, respawning and combat
  feel the same. A change applies at once, with one exception. The mod hooks the
  server loop's wait only at start. If the game started with the cap at 1 and
  `measure = false`, a higher cap takes effect at the next start, and the log
  says so.
- `measure`: the line a minute. It costs a few atomic counters and a look at the
  threads once a minute.

When a setting forces exact waits or a task worker cap where the table says off,
the log says so.

## On the host

Under Wine, two host settings save more CPU than any mod:

- **ntsync.** Recent Wine and Proton builds wait through the kernel's
  `/dev/ntsync` (Linux 6.14 and later) instead of the Wine server. Load the
  module (`modprobe ntsync`) and pass the device into the container
  (`--device /dev/ntsync`). Then check that the environment line says
  `/dev/ntsync present`. Without ntsync, fsync (`WINEFSYNC=1`, the Proton
  default) is still faster than the Wine server.
- **The `performance` governor.** With `powersave` or `schedutil`, the CPU
  lowers its clock between the short bursts of a game loop, and every pass takes
  longer. Run `cpupower frequency-set -g performance` on the host.

Wine also counts the host's CPUs, not the container's limit. The mod caps the
task workers to the limit. A node of the size you want, with requests and no
limit, avoids the problem.

## Measurements

The test server was idle, on a copy of a played world, with nobody online. Each
setup ran twice, 3 minutes each after 2 minutes of loading. The tables show
process CPU, where 100 % is one core. Windows: Windows 11, Ryzen 9 9950X3D.
Proton: the `mornedhels/enshrouded-server:stable-proton` image (GE-Proton 11-9,
Wine 11.0, fsync, no ntsync) in Docker Desktop's Hyper-V virtual machine with 16
CPUs on the same machine. Compare the Proton rows with each other, not with the
Windows rows.

| Server and settings                            | CPU    | Busiest threads                                   |
| ---------------------------------------------- | ------ | ------------------------------------------------- |
| Windows, no mod                                | 19.9 % |                                                   |
| Windows, the mod with everything off           | 19.9 % | Session 9.8 %, main 3.7 %, Task-Default x8 3.2 %  |
| Windows, defaults                              | 7.9 %  | Task-Default x8 3.4 %, GameServer 2.6 %           |
| Windows, defaults and `task_workers = 4`       | 4.7 %  | GameServer 2.3 %, Task-Default x4 0.9 %           |
| Proton, no mod                                 | 37.7 % |                                                   |
| Proton, the mod with everything off            | 38.0 % | Session 19.6 %, Task-Default x8 8.6 %, main 4.7 % |
| Proton, defaults                               | 16.1 % | Task-Default x8 9.3 %, GameServer 3.3 %           |
| Proton, defaults and `task_workers = 4`        | 7.1 %  | GameServer 2.3 %, Task-Default x4 1.3 %           |
| Proton, 4-CPU limit, no mod                    | 38.2 % |                                                   |
| Proton, 4-CPU limit, exact waits only          | 16.5 % | Task-Default x8 10.0 %, GameServer 3.5 %          |
| Proton, 4-CPU limit, defaults (4 task workers) | 6.7 %  | GameServer 2.3 %, Task-Default x4 1.3 %           |

Every run kept 24 simulation steps a second, the idle rate, and logged no Bad
Performance warning. The exact waits slept about 240 times a second on Windows
and 260 times under Proton, for about 0.9 ms each. They woke on average 0.17 ms
after the deadline on Windows and 0.08 ms under Proton.

With players online, the server loop runs 700 to 900 passes a second. Real
players were not available, so the online loop was emulated: the server loop was
forced to its online wait. Each setup ran once. Under Proton the process used
49.8 % with everything off, 20.0 % with the defaults and 14.6 % with
`task_workers = 4`. With a 4-CPU limit it used 20.1 % with exact waits only and
10.9 % with the defaults. The server wait cap at 4 ms took the Proton server
from 29.6 % to 20.3 %. On Windows it changed nothing measurable, so it stays off
until a check with real players. On a server without a CPU limit, a task worker
cap halves idle CPU again. Its cost with players was not measured, so there it
stays a setting.

## How to measure your own server

Compare a run with a change on against a run with it off. Use a test copy of the
server, never the server people play on:

1. Copy the server and its save to a test instance with the same image and tag.
2. Start it with the change off and let it load. Keep it running a few minutes
   idle, then a few minutes with one player standing still. Note the `measure:`
   lines in `logs/enshwick/mods/enshwick.optimizer.log`.
3. Turn the change on, restart and repeat.
4. Compare the process and thread CPU and the Bad Performance count. On a Linux
   host, `pidstat -t -p <pid> 5` shows each thread, and
   `ps -o %cpu -C wineserver` shows the Wine server.

## Install

Download
[`enshwick-optimizer.zip`](https://github.com/enshwick/mod-optimizer/releases/download/dev/enshwick-optimizer.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. The mod needs the Enshwick loader. Get
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
for the loader alone, or
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
for the loader with every mod. These files are development builds of the newest
commit. Until the first release, 1.0.0, there are no other builds.

## Who needs it

The dedicated server needs it. In the game, hosted or not, every change is off
by default, as the table shows, because none was measured there. A setting can
force a change on. The task workers stay as the game picks them, because only
the dedicated server lets the mod change them. Measure mode and the environment
line run in the game too. Players need nothing, and the mod does not count in
the join check.

Licensed under [MIT](LICENSE).
