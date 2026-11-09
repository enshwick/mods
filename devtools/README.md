# Developer mods

Development-only mods for the [Enshwick](https://github.com/enshwick/enshwick)
loader, never shipped to players:

- [`enshwick-probe`](enshwick-probe/README.md) exercises the platform API on
  a test server and logs what it sees;
- [`enshwick-research`](enshwick-research/README.md) logs what a running
  client does where the platform still needs engine facts;
- [`enshwick-test-content`](enshwick-test-content/README.md) adds content of
  its own for checking the platform in game;
- [`enshwick-test-ui`](enshwick-test-ui/README.md) adds the screen
  extensions, sounds and input reads the interface checks read.

They are never released. Build their archives with
`cargo xtask dist --package enshwick-probe --package enshwick-research --package enshwick-test-content --package enshwick-test-ui`.
