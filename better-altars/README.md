# Better Altars

More Flame Altars at each Flame Level. Altar zones that touch grow together, so
you can build big or build next to friends.

Built on [Enshwick](https://github.com/enshwick/enshwick).

- **More altars.** Each Flame Level allows its own number of altars, up to 255,
  in up to 179 bases. The default is twice the vanilla count, in 16 bases.
- **Zones that grow together.** Altars whose zones touch form a group. Each
  altar grows until the group covers as much ground as its altars would cover
  apart. A piece may stand across the seam between two zones of a group.

| Flame Level | 0 | 1 | 2 | 3  | 4  | 5  | 6  | 7  | 8  | 9  | 10 | 11 and above |
| ----------- | - | - | - | -- | -- | -- | -- | -- | -- | -- | -- | ------------ |
| Vanilla     | 1 | 2 | 4 | 6  | 8  | 10 | 11 | 12 | 13 | 14 | 15 | 16           |
| Default     | 2 | 4 | 8 | 12 | 16 | 20 | 22 | 24 | 26 | 28 | 30 | 32           |

## How do I change the settings?

Open the mod's gear on the Mods screen. The settings are in
`mod-configs/enshwick.better-altars.toml`. The mod writes this file on first
start, and each world keeps its own copy of it (see
[Settings](https://github.com/enshwick/enshwick#settings)).

```toml
[count]
altars_per_flame_level = [2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32]
bases = 16

[zones]
clusters = true
building_across_seams = true
```

### Altar count

The first entry of `altars_per_flame_level` is Flame Level 0. Level 0 comes
before Flame Level 1, where a new world starts. Levels past the end of the list
use its last entry. An empty list is the vanilla table.

Each level takes 1 to 255 altars. The mod moves a value outside that range to
the nearest limit and writes a warning in the log. Where the mod keeps the
game's limit of 16, it lowers larger values to 16. A change applies while the
game runs. You change the number of levels in the list in the file only.

`bases` is how many bases a world holds, from 16 to 179. Each world keeps this
value in its own settings file. The server does not send it to joined players.
The value applies when the game starts again. A joined
player groups the bases that the server holds. Where the game cannot hold more
than 16 bases, the mod keeps 16 and says so in its log.

### Altar zones

When the zones of two or more Flame Altars touch or overlap, they form a
cluster:

- Every altar of the cluster grows by the same amount on every horizontal side.
  It grows until the cluster covers as much ground as its altars would cover
  apart, to the nearest grid cell. Overlap loses no ground. Clusters that grow
  into each other become one cluster.
- No zone shrinks. This holds when an altar is placed, when a Flame Level rises
  and when the server stalls for a moment.
- Every altar of the cluster takes the height of the tallest one.
- With `building_across_seams`, a piece may stand across the seam between two
  zones of a cluster. Vanilla refuses such a piece.

Altars that stand apart keep their vanilla zones. A tilted altar also keeps its
vanilla zone, even next to other altars. `clusters` applies when the world
loads: each world uses its own value, with no restart. `building_across_seams`
applies while the game runs.

The mod works out the clusters on a thread of its own. If that thread cannot
start, the log says so, clusters stay off in that world and every altar keeps
its vanilla zone.

## What happens past 16 altars?

The vanilla game puts the zones of only its first 16 altars into bases, and it
holds 16 bases. The mod changes this grouping on the server and in every
player's game. The game then takes up to 255 altars in up to 179 bases:

- Altars whose zones touch, directly or through other altars, share one base, as
  in vanilla.
- A world holds at most `bases` bases, 16 by default. An altar whose zone
  touches no base would start one more base. The game refuses such an altar as
  "too many altars". Place it so its zone touches one of your bases.
- One more base can still come about. Its zone allows building, but it is not a
  base: it lacks comfort and crafting from chests or survivors. This lasts until
  it touches the zone of another base. The log says so.

After a game update, the mod may not find the game code it changes. The mod then
keeps the limit of 16 and says so in its log.

## Why does my server stop before the world loads?

The save keeps every altar, as in vanilla. Without the mod, the game puts only
the first 16 altar zones into bases. The altars past them keep their zones but
lose their base features, such as comfort and crafting from chests. So a
dedicated server stops a world that has more than 16 altar zones or more than 16
bases.

The server stops before the world loads when:

- a game update makes the build unknown to Enshwick;
- the mod is missing or off;
- the mod cannot find its places in the game.

`logs/enshwick/loader.log` says why. Update Enshwick and the mod, or remove
altars down to 16 while the mod runs. The stop ends once the loaded world has
had 16 altar zones or fewer, in 16 bases or fewer, for a minute. To open the
world anyway, delete `mod-data/enshwick.better-altars/hold.txt`. The altars past
16 then lose their base features.

Only a dedicated server stops the world. A player who hosts such a world must
keep the mod running and up to date.

## How do I install it?

1. Download
   [`enshwick-better-altars.zip`](https://github.com/enshwick/mods/releases/download/dev/enshwick-better-altars.zip).
2. Extract it into the folder that holds `enshrouded.exe` or
   `enshrouded_server.exe`.

The mod needs the Enshwick loader. Get
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
and extract it into the same folder. These are development builds of the newest commit. Until the
first release, 1.0.0, there are no other builds.

## Who needs it?

The server and every player:

- The count shows in the game's interface and decides where an altar may go.
- Every player's game groups the altars into bases.
- The game checks building across seams on both sides.
- The server sizes the zones.

The mod counts in the join check, so a player with another version cannot join.
A player who joins runs with the server's table and its `building_across_seams`,
whatever their own file says.

## How do other mods change the limit?

Other mods change how many altars a Flame Level allows with the platform's
altar-limit chain. `load.on_altar_limit` adds a filter. The filter sees the
Flame Level, the game's own count and the count so far. A filter of a mod loaded
after Better Altars sees this mod's table. The loader keeps the answer within
the zones that the game groups. See
[Altars per level and bases past 16](https://github.com/enshwick/enshwick/blob/main/book/src/altars.md#altars-per-level-and-bases-past-16).

Licensed under [MIT](LICENSE).
