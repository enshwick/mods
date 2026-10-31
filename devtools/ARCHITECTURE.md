# Architecture

Development-only mods, each a Windows DLL the Enshwick
loader loads from the game's `mods/` folder. They hold only their own
logic and build on the `enshwick` API; neither depends on the other.

| Path | What |
| --- | --- |
| `enshwick-probe/` | the API probe for test servers |
| `enshwick-research/` | the client research mod; builds only for Windows |
