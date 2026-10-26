# More Altars

Sets how many Flame Altars a world allows at each Flame level, up to 32 altars
in 16 bases. The default doubles the vanilla count. An Enshrouded mod for the
[Enshwick](https://github.com/enshwick/enshwick) loader.

| Flame level | 0 | 1 | 2 | 3  | 4  | 5  | 6  | 7  | 8  | 9  | 10 | 11 and above |
| ----------- | - | - | - | -- | -- | -- | -- | -- | -- | -- | -- | ------------ |
| Vanilla     | 1 | 2 | 4 | 6  | 8  | 10 | 11 | 12 | 13 | 14 | 15 | 16           |
| Default     | 2 | 4 | 8 | 12 | 16 | 20 | 22 | 24 | 26 | 28 | 30 | 32           |

## How do I change the count?

Edit `mod-configs/flame-altar-count.toml`, which the mod writes on first
start. Each world keeps its own copy of it (see
[Settings](https://github.com/enshwick/enshwick#settings)). The file keeps the
mod's old name, so settings from before the rename still apply.

```toml
altars_per_flame_level = [2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32]
```

Levels past the end of the list use its last entry. An empty list is the
vanilla table. Values above 32 become 32, and 0 becomes 1; the log warns about
each. While the mod keeps the game's limit of 16, values above 16 become 16.
A change applies while the game runs.

The mod's gear on the Mods screen shows the table, in English or Ukrainian.

## What happens past 16 altars?

The vanilla game groups the zones of only 16 altars into bases, and holds 16
bases. The mod changes this grouping on the server and on every player's game,
so that it takes 32 altars:

- altars whose zones touch, directly or through other altars, share one base,
  as in vanilla;
- a world holds at most 16 bases. An altar whose zone touches no base would
  start a 17th, so the game refuses it as "too many altars". Place it so its
  zone touches one of your bases;
- if a 17th base comes about anyway, its zone still allows building. It is no
  base (no comfort, crafting from chests or survivors) until it touches
  another base's zone, and the log says so.

After a game update, the mod may not find the game code it changes. It then
keeps the limit of 16 and says so in its log.

## Why does my server stop before the world loads?

The save keeps every altar as vanilla does. Without the mod, the game groups
the altars past 16 on its own and may crash. So a dedicated server whose world
has more than 16 altars holds the game.

The server stops before the world loads when:

- a game update makes the build unknown to Enshwick;
- the mod is missing or off;
- the mod cannot find its places in the game.

`logs/enshwick/loader.log` says why. Update Enshwick and the mod, or remove
altars down to 16 with the mod running. The hold goes once the loaded world has
had 16 altar zones or fewer for a minute. To open the world anyway, at your own
risk, delete `mod-data/flame-altar-count/hold.txt`.

Only a dedicated server keeps the hold. A player who hosts such a world must
keep the mod running and up to date.

## How do I install it?

1. Download
   [`enshwick-more-altars.zip`](https://github.com/enshwick/mod-more-altars/releases/download/dev/enshwick-more-altars.zip).
2. Extract it into the folder that holds `enshrouded.exe` or
   `enshrouded_server.exe`.
3. If `mods/enshwick-flame-altar-count.dll` is there, delete it. It is this
   mod under its old name. Enshwick runs only the newer file and names the old
   one in `logs/enshwick/loader.log`.

The mod needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit. Until the
first release, 1.0.0, there are no others.

## Who needs it?

The server and every player. The count shows in the game's interface and
decides where an altar may go, and every player's game groups the altars into
bases. The mod counts in the join check, so a player with another version
cannot join. A player who joins runs with the server's table, whatever their
own file says.

## How do other mods change the limit?

Other mods change how many altars a Flame level allows at the extension
point `flame-altar-count:limit`. The answer stays between 1 and the cap. See
[Extending other mods](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md).

Licensed under [MIT](LICENSE).
