# Map Markers

Map Markers shows the loot chests and resources around each player on that
player's map.

Built on [Enshwick](https://github.com/enshwick/enshwick).

## In the game

Every two seconds, the server picks the loot chests and resources within 150 m
of each player across the ground. It sends them to that player's game, and the
map shows them as custom markers:

- Loot chests the world comes with show in yellow, labelled Chest.
- Plants and the other nodes that hold what they yield show in green, labelled
  Resource.
- The map shows the nearest 256 at most, the limit the loader gives a mod.
- The mod marks a loot chest whether a player looted it or not.

## Settings

Each world keeps its settings in
`<save>-mod-settings/enshwick.map-markers/settings.toml` beside its save. The
loader writes the file as the world first loads (see
[Settings](https://github.com/enshwick/enshwick#settings)). The mod's gear on
the Mods screen shows the settings in the language of the game:

```toml
chests = true
resources = true
radius = 150.0
```

- `chests` marks the world's loot chests.
- `resources` marks plants and the other nodes players gather from.
- `radius` is the distance across the ground from the player, in metres, from
  10 to 1000.

A change applies at the next two-second pass, without a restart.

## Install

Download
`enshwick-map-markers.zip` from its [newest release](https://github.com/enshwick/mods/releases?q=enshwick-map-markers&expanded=true).
Extract it into the folder with `enshrouded.exe` or `enshrouded_server.exe`. The
mod needs the Enshwick loader. Get
`enshwick.zip` from the [Enshwick release](https://github.com/enshwick/enshwick/releases)
that the mod's release notes name, and extract it into the same folder. Until
1.0 both are alphas, and anything may change from one alpha to the next. Back
up your saves before you update.

## Who needs it

The server and every player need it. The server reads where the chests and
resources are, and each player's game shows them on the map. The join check
counts the mod. On a server, the server's settings decide what every map marks.

Licensed under [MIT](LICENSE).
