# Architecture

A Cargo workspace of development-only mods, each a Windows DLL the Enshwick
loader loads from the game's `mods/` folder. They hold only their own
logic and build on the `enshwick` API; neither depends on the other.

| Path | What |
| --- | --- |
| `enshwick-probe/` | the API probe for test servers |
| `enshwick-research/` | the client research mod; builds only for Windows |
| `tools/xtask/` | `cargo xtask check`, the checks before a merge, and `cargo xtask dist` |
