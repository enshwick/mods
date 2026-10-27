# Better Altars

More Flame Altars per Flame level, and altar zones that grow together. An
Enshrouded mod for the [Enshwick](https://github.com/enshwick/enshwick) loader,
for anyone who builds big or builds next to friends.

- **More altars.** Each Flame level allows its own number of altars, up to 32 in
  16 bases. The default doubles the vanilla count.
- **Zones that grow together.** Altars whose zones touch form a group: each
  grows until the group covers as much ground as its altars would apart, and a
  piece may sit across the seam between two zones of a group.

| Flame level | 0 | 1 | 2 | 3  | 4  | 5  | 6  | 7  | 8  | 9  | 10 | 11 and above |
| ----------- | - | - | - | -- | -- | -- | -- | -- | -- | -- | -- | ------------ |
| Vanilla     | 1 | 2 | 4 | 6  | 8  | 10 | 11 | 12 | 13 | 14 | 15 | 16           |
| Default     | 2 | 4 | 8 | 12 | 16 | 20 | 22 | 24 | 26 | 28 | 30 | 32           |

## How do I change the settings?

Open the mod's gear on the Mods screen. The settings live in
`mod-configs/enshwick.better-altars.toml`, which the mod writes on first start,
and each world keeps its own copy of it (see
[Settings](https://github.com/enshwick/enshwick#settings)).

```toml
[count]
altars_per_flame_level = [2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32]

[zones]
clusters = true
building_across_seams = true
```

This mod joins two former mods, More Altars and Flame altar zones. On the first
start it takes the settings and data of both, under each id they had
(`enshwick.more-altars`, `flame-altar-count`, `enshwick.flame-altar-zones`,
`flame-altar-zones`), into these groups.

### Altar count

The first entry of `altars_per_flame_level` is Flame level 0, which comes before
the Flame level 1 a new world starts at. Levels past the end of the list use its
last entry. An empty list is the vanilla table. Each level takes 1 to 32 altars,
and the mod moves a value outside that range to the nearest limit, with a
warning in the log. Where the mod keeps the game's limit of 16, it lowers larger
values to 16. A change applies while the game runs. The number of levels in the
list changes in the file only.

### Altar zones

When the zones of two or more Flame Altars touch or overlap, they form a
cluster:

- every altar of the cluster grows by the same amount on every horizontal side,
  until the cluster covers as much ground as its altars would apart, to the
  nearest grid cell, so overlap loses nothing; clusters that grow into each
  other become one;
- no zone ever shrinks, when an altar is placed or a Flame level rises, nor
  while the server stalls for a moment;
- every altar of the cluster takes the height of the tallest one;
- with `building_across_seams`, a piece may be placed across the seam between
  two zones of a cluster, which vanilla refuses.

Altars that stand apart keep their vanilla zones, and so does a tilted altar,
even beside others. `clusters` applies when the world loads: each world clusters
its zones or keeps them vanilla by its own value, with no restart.
`building_across_seams` applies while the game runs.

## What happens past 16 altars?

The vanilla game groups the zones of only 16 altars into bases, and holds 16
bases. The mod changes this grouping on the server and on every player's game,
so that it takes 32 altars:

- altars whose zones touch, directly or through other altars, share one base, as
  in vanilla;
- a world holds at most 16 bases. An altar whose zone touches no base would
  start a 17th, so the game refuses it as "too many altars". Place it so its
  zone touches one of your bases;
- if a 17th base comes about anyway, its zone still allows building. It is no
  base (no comfort, crafting from chests or survivors) until it touches another
  base's zone, and the log says so.

After a game update, the mod may not find the game code it changes. It then
keeps the limit of 16 and says so in its log.

## Why does my server stop before the world loads?

The save keeps every altar as vanilla does. Without the mod, the game groups the
altars past 16 on its own and may crash. So a dedicated server whose world has
more than 16 altars holds the game.

The server stops before the world loads when:

- a game update makes the build unknown to Enshwick;
- the mod is missing or off;
- the mod cannot find its places in the game.

`logs/enshwick/loader.log` says why. Update Enshwick and the mod, or remove
altars down to 16 with the mod running. The hold goes once the loaded world has
had 16 altar zones or fewer for a minute. To open the world anyway, at your own
risk, delete `mod-data/enshwick.better-altars/hold.txt`.

Only a dedicated server keeps the hold. A player who hosts such a world must
keep the mod running and up to date.

## How do I install it?

1. Download
   [`enshwick-better-altars.zip`](https://github.com/enshwick/mod-better-altars/releases/download/dev/enshwick-better-altars.zip).
2. Extract it into the folder that holds `enshrouded.exe` or
   `enshrouded_server.exe`.
3. Delete the former mods from `mods/` if they are there:
   `enshwick-more-altars.dll`, `enshwick-flame-altar-count.dll` and
   `enshwick-flame-altar-zones.dll`. Enshwick runs only Better Altars and names
   each old file in `logs/enshwick/loader.log`.

The mod needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit. Until the
first release, 1.0.0, there are no others.

## Who needs it?

The server and every player. The count shows in the game's interface and decides
where an altar may go, every player's game groups the altars into bases, and the
game checks building across seams on both sides. The server sizes the zones. The
mod counts in the join check, so a player with another version cannot join. A
player who joins runs with the server's table and its `building_across_seams`,
whatever their own file says.

## How do other mods change the limit?

Other mods change how many altars a Flame level allows with the platform's
altar-limit chain: `load.on_altar_limit` adds a filter that sees the Flame
level, the game's own count and the count so far. A filter of a mod loaded
after Better Altars sees this mod's table. The loader keeps the answer within
the zones the game groups. See
[Altars per level and bases past 16](https://github.com/enshwick/enshwick/blob/main/book/src/altars.md#altars-per-level-and-bases-past-16).

Licensed under [MIT](LICENSE).
