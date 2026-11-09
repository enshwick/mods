# Fuzz targets

libFuzzer targets for what the mods read from the network. They form a
Cargo workspace of their own, built by cargo-fuzz with the nightly compiler
and AddressSanitizer.

| Target | Input |
| --- | --- |
| `rcon_packets` | the packets a remote console client sends Admin Tools |

## Running

```sh
cargo install cargo-fuzz --locked
cargo +nightly fuzz run -O --target-dir target/fuzz rcon_packets target/fuzz/corpus/rcon_packets fuzz/corpus/rcon_packets -- -max_total_time=60
```

On Windows the target needs the AddressSanitizer runtime of Visual Studio,
`clang_rt.asan_dynamic-x86_64.dll`: put the folder of the MSVC tools that
holds it, `VC/Tools/MSVC/<version>/bin/HostX64/x64`, on `PATH` first.
A target compiles the module it reads with `#[path]` instead of depending
on the mod, since a mod is also a DLL, which a fuzzer cannot link.

The first corpus directory takes what the run finds; `fuzz/corpus/<target>`
holds the small seed corpus kept in the repository. A crash is written to
`fuzz/artifacts/<target>/` and replays with
`cargo +nightly fuzz run -O --target-dir target/fuzz <target> <file>`.
A crash becomes a regression test beside the code it found, then a fix.
Run a target for a minute or more before a pull request that changes what
it reads.
