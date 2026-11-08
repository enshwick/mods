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
comment before it. A mod takes its locks through the API's `Relock`,
`RelockRw` and `RelockWait`, the one poison policy. Before them, `cargo xtask check`
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

CI runs on GitHub-hosted runners. On every pull request that is not a
draft, `ci.yml` calls `check.yml`, which runs
`cargo fmt --all --check` and `cargo xtask check` over every mod in one
build, so the shared dependencies compile once. A push to `main` runs
`cargo xtask check --all --no-wine` instead. In both, the tests of the
Windows-only packages run natively on a Windows runner rather than under
Wine. A pull request that changes only documents or repository settings
compiles nothing. The job `check` sums up the others, and `ci / check` is
the check a pull request needs to pass.

Every mod has its own version. release-plz, configured in
`release-plz.toml`, versions each mod from the commits that touch its folder
and tags it `<package>-v<version>`, such as `enshwick-area-loot-v0.2.0`.
Until 1.0 each release is an alpha: a GitHub prerelease built on a loader
alpha.

Each mod's `CHANGELOG.md` is written by hand for players; release-plz never
writes it. A change players notice adds a line under `## [Unreleased]`.

A release goes in three steps:

1. Pin the platform to the loader alpha the mods are built for:
   `cargo xtask pin v1.0.0-alpha.14` sets the commit of that release in the
   root `Cargo.toml` and `Cargo.lock`. A mod whose released version was built
   for another loader and that has nothing under `## [Unreleased]` gets the
   line `- Built for Enshwick 1.0.0-alpha.14.` there, so its folder changes
   and release-plz bumps it as well: release-plz does not count a changed
   git commit of a dependency as a change. A mod whose version has no tag
   yet is left alone, and pinning back to the loader a mod was released on
   drops its line. Merge this in an ordinary pull request.
2. Run `release-pr`. It opens or updates the release pull request, which
   bumps the mods whose folders changed, and then closes each bumped mod's
   changelog on its branch: the `## [Unreleased]` section becomes
   `## [0.2.0] - 2026-11-05`, with an empty `## [Unreleased]` above it. A
   bumped mod with nothing under Unreleased keeps its released version, and
   so does a mod whose released version was tagged by a `release` run that
   stopped before its files: the next `release` finishes that version, and
   the release pull request after it bumps the mod; if the bumped version
   was closed already, it is refused: move that section back under
   Unreleased by hand. A
   version closed already but not released, with new lines under
   Unreleased, is refused: move those lines into its section by hand.
   Review and merge it. A mod never released has no tag to bump from, so
   release-plz opens no pull request for it: set its version and close its
   section in an ordinary pull request.
3. Run `release` on `main`.

`release-plz.yml` runs by hand:

- `build`, from any branch, does what a release would and publishes
  nothing: it writes the notes of every mod whose version has no tag yet and
  builds those mods, every mod when a mod's notes are refused, and shows both
  in the run's summary.
- `release-pr` opens the release pull request for the mods that changed and
  closes their changelogs with `cargo xtask close`.
- `release` makes one prerelease for each mod whose version has no finished
  release yet, with the notes of `cargo xtask notes`. A job per released mod
  then builds only that mod from its tag and attaches its zip, PDB, SBOM,
  `SHA256SUMS` and build provenance. Then the docs site builds again. A run
  that stops partway, after some tags or before the files, is finished by
  running `release` again: a tagged version without its release or its
  files counts as not released yet, and its notes come from its tag.

`cargo xtask notes` opens each mod's notes with the alpha warning in English
and Ukrainian, names the loader alpha and gives the mod's section of its
version. It refuses a mod with changes under Unreleased or without its
version's section, a released mod built for another loader than the pinned
one, and a workspace whose `enshwick` commit no loader release tags.
`release` stops on a refusal before it tags anything.

A new mod is added in two places: the `members` list of the root
`Cargo.toml` and a `[[package]]` entry in `release-plz.toml`. A mod that is
not ready to ship, a developer mod and a tool have `release = false` in
their entry: release-plz never tags them, and `cargo xtask notes` leaves
them out. `cargo xtask check` refuses a member without an entry, an entry
without a member, and a released entry that is not a mod with a player
page.
