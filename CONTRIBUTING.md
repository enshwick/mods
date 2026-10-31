# Contributing

You need Rust (`rust-toolchain.toml` installs the pinned version through
rustup), [lefthook](https://lefthook.dev),
[rumdl](https://github.com/rvben/rumdl), cargo-nextest,
[committed](https://github.com/crate-ci/committed) for commit messages,
[typos](https://github.com/crate-ci/typos), cargo-deny,
[ast-grep](https://ast-grep.github.io) and
[cargo-machete](https://github.com/bnjbvr/cargo-machete) for
`cargo xtask check`, and off Windows
[cargo-xwin](https://github.com/rust-cross/cargo-xwin).

```bash
lefthook install     # the git hooks
rumdl check .        # every document, as the hooks check it
cargo xtask check    # crate and code rules, unused dependencies, clippy, tests and doctests; --all adds fmt, rustdoc, cargo-deny
cargo xtask dist     # the player's archive of each mod, in target/dist
```

A package that builds only for Windows sets `windows-only = true` under
`[package.metadata.xtask]`. Off Windows it stays out of the host checks,
and `cargo xtask check --windows` (or `--all`) runs its tests under Wine:
directly on an amd64 Linux host that has Wine, otherwise in Docker, in an
image built from `tools/wine/Dockerfile` on first use. On such a host, use
the Wine of [GE-Proton](https://github.com/GloriousEggroll/proton-ge-custom):
unpack a release and put its `files/bin` first on `PATH`. On Apple
silicon the container runs through Rosetta, which cannot run tests of
crashes, detours, patches and threads, so those are skipped there.

The code rules clippy cannot express are ast-grep rules, listed in
[`sgconfig.yml`](sgconfig.yml) with their tests. A deliberate exception is
`// ast-grep-ignore: <rule id>` on the line above, with its reason in the
comment before it. A crate takes its locks through the `Relock` traits of
its own `locks.rs`, the one poison policy. Before them, `cargo xtask check`
holds every crate to three rules: a crate whose package has no unsafe code
declares `#![forbid(unsafe_code)]` at its root, only a dev-dependency turns
on a crate's `test` feature, and a mod names its version in `[package]`.
It also holds every mod, the developer mods included, to the repository's
own rules: no feature named `use-…` or `with-…`, `rust-version` equal to the
toolchain of `rust-toolchain.toml`, the platform only from
`[workspace.dependencies]` at a full commit hash, no `anyhow` and no other
mod as a dependency, a test that installs the fake host, and nothing in
`docs/` but `player.md`, its translations and `images/`.

The hooks check formatting, spelling, Markdown and commit messages and
compile nothing. A pull request title is a conventional commit,
`<type>(<scope>): <subject>`, and becomes the squashed commit; the scope
is the mod's folder, or `mods` for the whole repository.

CI runs on GitHub-hosted runners. On every pull request, `ci.yml` runs
`cargo fmt --all --check` and `cargo xtask check` over every mod in one
build, so the shared dependencies compile once. A push to `main` runs
`cargo xtask check --all --no-wine` instead. In both, the tests of the
Windows-only packages run natively on a Windows runner rather than under
Wine. A pull request that changes only documents or repository settings
compiles nothing. The job `check` sums up the others and is the check a
pull request needs to pass.

Every mod has its own version. release-plz, configured in
`release-plz.toml`, versions each mod from the commits that touch its folder
and tags it `<package>-v<version>`, such as `enshwick-area-loot-v0.2.0`.
`release-plz.yml` runs by hand: `release-pr` opens the release pull request
for the mods that changed, and `release`, once it merged, makes one GitHub
release per mod. A job per released mod then builds only that mod from its
tag and attaches its zip, PDB, SBOM, `SHA256SUMS` and build provenance.

Every night `dev.yml` builds every mod and replaces the files of the `dev`
prerelease, one `<package>.zip` per mod, when `main` moved since the last
build. A run by hand from `main` builds it at once; from any other branch it
does nothing.

A new mod is added in two places: the `members` list of the root
`Cargo.toml` and a `[[package]]` entry in `release-plz.toml`.
