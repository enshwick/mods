# Shared Experience

Friends who play at different hours stay on the same character level. Every
character catches up with the character that has the most experience in the
world.

Built on [Enshwick](https://github.com/enshwick/enshwick).

- Each world keeps a mark: the most experience any character has reached in it.
  A new world starts without a mark.
- A character below the mark catches up while online. A character who was away
  catches up on the next join.
- Experience that any character earns raises the mark. A character who is
  catching up also raises it with new experience.
- Playing together gives no extra speed. When two or more players earn
  experience at the same moment, such as from one kill, only the largest gain
  counts, once.
- A character behind the mark gains the difference one level per tick. Each
  level comes with the level-up banner, skill points and a saved level, as in
  normal play.
- A character who is already ahead, such as one brought from another world,
  raises the mark for everyone.
- No character goes past the level cap. Flame level is shared in the base game
  already. Skill points from knowledge stay with each character.

The experience a character gains is part of that character. It goes with the
character to other worlds.

## Settings

The settings file is `mod-configs/enshwick.shared-experience.toml`. The mod
writes it on the first start. Each world keeps its own copy (see
[Settings](https://github.com/enshwick/enshwick#settings)). The mod's gear on
the Mods screen shows the setting in the player's game language.

```toml
enabled = true
```

A change applies at once, without a restart.

## Install

Download
[`enshwick-shared-experience.zip`](https://github.com/enshwick/mod-shared-experience/releases/download/dev/enshwick-shared-experience.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. The mod needs the Enshwick loader. Get
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
for the loader alone, or
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
for the loader with every mod. These files are development builds of the newest
commit. Until the first release, 1.0.0, there are no other builds.

## Who needs it

Only the server, or the player who hosts the game, needs the mod. Players
without the mod see their experience bar fill and their level rise. The mod does
not count in the join check.

The mark of each world is saved in
`mod-data/enshwick.shared-experience/worlds/<world>/experience.toml`. It stays
across restarts, on a dedicated server and in a hosted game.

Licensed under [MIT](LICENSE).
