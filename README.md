# Enshwick mods

Mods for Enshrouded by the Enshwick team, each in its own folder and released
on its own. They run on [Enshwick](https://enshwick.com), the modding framework
and API for Enshrouded.

[![Coverage](https://codecov.io/gh/enshwick/mods/graph/badge.svg)](https://codecov.io/gh/enshwick/mods)

For Enshwick 1.0, the Enshwick team releases one mod:
[Admin Tools](admin-tools/README.md), in an archive of its own. The other mods
stay in this repository. Each mod has a page on
[enshwick.com](https://enshwick.com). The page says what the mod changes and
links its archive when the mod has a release.

<!--
Admin Tools' line in the table is its English line in src/data/mods.json of
enshwick/docs, which the site's front page shows. Change it there first. The
other lines are kept here by hand.
-->

| Mod | What it does |
| --- | --- |
| [Admin Tools](admin-tools/README.md) | Server admins manage players and the world from the game. |
| [Area Loot](area-loot/README.md) | Loot one corpse, take the loot of the enemies nearby. |
| [Better Altars](better-altars/README.md) | More Flame Altars, zones that grow together. |
| [Build Camera](build-camera/README.md) | The flying build camera, without the skill. |
| [Game Rates](game-rates/README.md) | Set how much recipes, loot and experience give. |
| [Optimizer](optimizer/README.md) | Less CPU for a dedicated server. |
| [Personal Safes](personal-safes/README.md) | A chest for its owner and the players they allow. |
| [Shared Experience](shared-experience/README.md) | All players stay on the same level. |
| [Survivors per Altar](survivors-per-altar/README.md) | A copy of each survivor at every Flame Altar. |

[`devtools/`](devtools/README.md) holds the developer mods, which never ship
to players.

Each folder keeps the history of the repository it came from. Licensed under
[MIT](LICENSE).
