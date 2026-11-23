# Architecture

One crate, a Windows DLL the Enshwick loader loads from the game's `mods/`
folder, of side `both`. The mod holds only its own logic: what the game needs
from it goes through the `enshwick` API, and what another mod could use belongs
in the API, not here.

The world's entities are the server's to read, so the mod has two halves that
meet on one message channel:

| Path | What |
| --- | --- |
| `src/lib.rs` | the settings; the server half, which picks each player's marks on a tick and sends them; the client half, which shows what the server sent on the map |
| `src/marks.rs` | the message and the pure choice of marks: the kinds, the radius across the ground, the nearest 256 |
| `src/tests.rs` | the fake host tests of both halves |
