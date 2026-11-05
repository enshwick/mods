# Developer mods

Development-only mods for the [Enshwick](https://github.com/enshwick/enshwick)
loader, never shipped to players:

- [`enshwick-probe`](enshwick-probe/README.md) exercises the platform API on
  a test server and logs what it sees;
- [`enshwick-research`](enshwick-research/README.md) logs what a running
  client does where the platform still needs engine facts.

They are never released. Build their archives with
`cargo xtask dist --package enshwick-probe --package enshwick-research`.
