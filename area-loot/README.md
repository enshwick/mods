# Area Loot

Area Loot loots the enemies around you in one step. Empty one enemy corpse, and
the loot of the other enemies nearby goes into your inventory too.

Built on [Enshwick](https://github.com/enshwick/enshwick).

## In the game

When a player takes the last item from an enemy corpse or an enemy loot bag, the
mod takes the other enemy loot within 12 m. It also reaches 4 m up and down. The
loot goes to the same player, nearest container first. The player sees the usual
pickup messages. The emptied corpses dissolve as in vanilla.

- When the inventory is full, the mod stops at that container. What does not fit
  stays in it. Containers further away stay as they are.
- The mod skips a container that another player has open. It also skips a
  container that another player opened in the last 10 seconds. It also skips a
  container whose last opener stands within 4 m of it.
- The mod checks each container again right before it takes from it. So a player
  who opens a container or walks up to it during the pass keeps it.
- The 10 seconds count only while the server runs. A server that stalls gives no
  container to the wrong player.
- Chests, quest containers, items a player dropped and items on the ground work
  as in vanilla.

## Settings

Each world keeps its settings in
`<save>-mod-settings/enshwick.area-loot/settings.toml` beside its save. The
loader writes the file as the world first loads (see
[Settings](https://github.com/enshwick/enshwick#settings)):

```toml
radius = 12.0
height = 4.0
enabled = true
```

- `radius` is the distance across the ground from the emptied container, in
  metres, from 0.1 to 50.
- `height` is the distance up and down from the emptied container, in metres,
  from 0.1 to 20.
- `enabled` turns the mod on or off.

The mod skips a file with a value outside these ranges and keeps the values it
runs with. A change applies at once, without a restart. When you turn the mod
back on, it starts with a clean state: it takes nothing that a player emptied
while it was off. Settings saved under the old id, `area-loot`, move to the new
id on the first start.

## Install

Download
`enshwick-area-loot.zip` from its [newest release](https://github.com/enshwick/mods/releases?q=enshwick-area-loot&expanded=true).
Extract it into the folder with `enshrouded.exe` or `enshrouded_server.exe`. The
mod needs the Enshwick loader. Get
`enshwick.zip` from the [Enshwick release](https://github.com/enshwick/enshwick/releases)
that the mod's release notes name, and extract it into the same folder. Until 1.0 both are alphas, and anything may change from one alpha
to the next. Back up your saves before you update.

## Who needs it

Only the server needs it, or the player who hosts the game. Players without the
mod get the loot and see the corpses dissolve, and the join check does not count
the mod.

## For mod authors

Other mods can remove a container from the list that Area Loot takes from. They
do this at the extension point `enshwick.area-loot:eligible`. They cannot add a
container that the mod skipped, such as a container another player is at. See
[Extending other mods](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md).

Licensed under [MIT](LICENSE).
