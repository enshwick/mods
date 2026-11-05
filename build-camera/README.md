# Build Camera

Build Camera gives every player the building fly mode of the Flying Architect
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
- The skill tree still shows Flying Architect as locked, and the mod does not
  refund points spent on it.
- Other skills and their prerequisites do not change.

If a game version does not have the Flying Architect skill where the mod
expects it, the mod turns off and the Mods screen shows the reason. Fly mode
then needs the skill, as in vanilla.

## Settings

Each world keeps its settings in
`<save>-mod-settings/enshwick.build-camera/settings.toml` beside its save. The
loader writes the file as the world first loads (see
[Settings](https://github.com/enshwick/enshwick#settings)). The mod's gear on
the Mods screen shows the setting in the language of the game:

```toml
enabled = true
```

With `false`, fly mode needs the skill again, as in vanilla. A change applies
while the game runs.

## Install

Download
`enshwick-build-camera.zip` from its [newest release](https://github.com/enshwick/mods/releases?q=enshwick-build-camera&expanded=true).
Extract it into the folder with `enshrouded.exe` or `enshrouded_server.exe`. The
mod needs the Enshwick loader. Get
`enshwick.zip` from the [Enshwick release](https://github.com/enshwick/enshwick/releases)
that the mod's release notes name, and extract it into the same folder. Until 1.0 both are alphas, and anything may change from one alpha
to the next. Back up your saves before you update.

## Who needs it

The server and every player need it. The server decides about fly mode, and the
player's game predicts the same decision, so both must agree. The join check
counts the mod. A player who joins uses the world's `enabled`, which the server
sends.

Licensed under [MIT](LICENSE).
