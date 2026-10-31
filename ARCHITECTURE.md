# Architecture

Each top-level folder is one mod, a crate of the one Cargo workspace at the
root. Every mod is a Windows DLL the Enshwick loader loads from the game's
`mods/` folder, built on the `enshwick` API from git at one commit of its
`main` branch, the same for every mod. `cargo xtask check` at the root
checks every mod.

- A mod never depends on another mod or reads its data or services. It may
  offer typed extension points that other mods link.
- What another mod could use belongs in the `enshwick` API, not in a mod.
- Each mod has its own version and release; no release bundles several mods.

| Path | What |
| --- | --- |
| `area-loot/` | Area Loot |
| `better-altars/` | Better Altars |
| `build-camera/` | Build Camera |
| `game-rates/` | Game Rates |
| `optimizer/` | Optimizer |
| `personal-safes/` | Personal Safes |
| `shared-experience/` | Shared Experience |
| `survivors-per-altar/` | Survivors per Altar |
| `devtools/` | the developer mods: the API probe and the client research mod |
| `tools/xtask/` | `cargo xtask check`, the checks before a merge, and `cargo xtask dist`, each mod's archive |
| `tools/wine/` | the image `cargo xtask check --windows` runs Windows-only tests in off Windows |
