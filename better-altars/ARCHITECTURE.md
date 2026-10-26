# Architecture

One crate, a Windows DLL the Enshwick loader loads from the game's `mods/`
folder. The mod holds only its own logic: what the game needs from it goes
through the `enshwick` API, and what another mod could use belongs in the
API, not here.

| Path | What |
| --- | --- |
| `src/lib.rs` | the manifest, the settings groups and start-up |
| `src/count.rs`, `src/hold.rs` | altars per Flame level, the extension point, the server's hold on a world past 16 altar zones |
| `src/zones.rs`, `src/zones/` | the zones of touching altars: the solver thread and the seam check |
| `src/geometry.rs`, `src/geometry/` | the pure geometry of clusters and their growth |
| `src/migrations.rs` | the former mods' settings carried into the groups |
| `tools/xtask/` | `cargo xtask check`, the checks before a merge |
