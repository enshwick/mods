# Build Camera

Build Camera gives every player the building fly mode of the Airborne Architect
skill. The player needs no skill points for it.

Built on [Enshwick](https://github.com/enshwick/enshwick).

## In the game

Near a base, the character stays in place and a flying spark carries the camera.
The player places and removes blocks and props from the spark.

Everything else works as in vanilla:

- Fly mode starts only near a building zone. The player must be allowed to edit
  the base, be alive and not glide. Fly mode ends when one of these conditions
  stops being true.
- Build reach is the vanilla reach, measured from the spark.
- The skill tree still shows Airborne Architect as locked, and the mod does not
  refund points spent on it.
- Other skills and their prerequisites do not change.

## Settings

The settings file is `mod-configs/enshwick.build-camera.toml`. The mod writes it
on the first start. Each world keeps its own copy (see
[Settings](https://github.com/enshwick/enshwick#settings)). The mod's gear on
the Mods screen shows the setting in the language of the game. On the first
start, the mod copies its settings from its old id, `build-camera`:

```toml
enabled = true
```

With `false`, fly mode needs the skill again, as in vanilla. A change applies
while the game runs.

## Install

Download
[`enshwick-build-camera.zip`](https://github.com/enshwick/mod-build-camera/releases/download/dev/enshwick-build-camera.zip).
Extract it into the folder with `enshrouded.exe` or `enshrouded_server.exe`. The
mod needs the Enshwick loader. Get
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
for the loader alone, or
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
for the loader with every mod. These files are development builds of the newest
commit. Until the first release, 1.0.0, there are no other builds.

## Who needs it

The server and every player need it. The server decides about fly mode, and the
player's game predicts the same decision, so both must agree. The join check
counts the mod. `enabled` is a join value: a player who joins uses the server's
value, whatever their own file says.

Licensed under [MIT](LICENSE).
