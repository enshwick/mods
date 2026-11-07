//! What every function the mod hooks or calls holds: it is pinned once on
//! every build of each side that pins it, no other target's prologue opens
//! like its own, and on a kept executable its code opens with one of its
//! prologues and with no other target's.

use core::fmt::Write as _;

use enshwick::abi::Side;
use enshwick_gamedata::reflect::{Fingerprint, Image, Layout};

use crate::memory::Target;

/// Every target of the mod.
fn all() -> Vec<&'static Target> {
    let tooltip: &'static [crate::tooltip::Site] = &crate::tooltip::SITES;
    let ui: &'static [crate::ui::Site] = &crate::ui::SITES;
    let called: &'static [Target] = &crate::entities::CALLED;
    let mut all: Vec<&'static Target> = tooltip.iter().map(|site| &site.target).collect();
    all.extend(ui.iter().map(|site| &site.target));
    all.push(&crate::entities::CURSOR);
    all.extend(called);
    all.extend([&crate::health::TARGET, &crate::skills::TARGET]);
    all
}

/// One row the manifest pins.
struct Row {
    symbol: String,
    rva: u64,
    side: Side,
    build: Fingerprint,
}

/// The rows the manifest pins, on every build of both sides.
fn pinned() -> Vec<Row> {
    // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
    // `declare_mod!`, which no thread writes.
    let manifest = unsafe { &*crate::enshwick_mod_query() };
    // SAFETY: `declare_mod!` points at static rows of these counts.
    let builds = unsafe { core::slice::from_raw_parts(manifest.builds, manifest.build_count) };
    let mut rows = Vec::new();
    for build in builds {
        // SAFETY: `declare_mod!` points each build at static rows of this
        // count.
        let symbols = unsafe { core::slice::from_raw_parts(build.symbols, build.symbol_count) };
        for symbol in symbols {
            // SAFETY: a static string.
            let name = unsafe { symbol.name.as_str() }.unwrap_or_default();
            rows.push(Row {
                symbol: name.to_owned(),
                rva: symbol.rva,
                side: build.side,
                build: Fingerprint::new(build.fingerprint.guid, build.fingerprint.age),
            });
        }
    }
    rows
}

fn build_name(build: Fingerprint) -> String {
    let mut name = build.guid().iter().fold(String::new(), |mut hex, b| {
        write!(hex, "{b:02x}").unwrap();
        hex
    });
    write!(name, "/{}", build.age()).unwrap();
    name
}

#[test]
fn a_target_opens_only_code_that_starts_with_one_of_its_prologues() {
    let target = Target {
        symbol: "test.target",
        prologues: &[&[0x48, 0x89, 0x5c], &[0x40, 0x55]],
    };
    assert!(target.opens(&[0x48, 0x89, 0x5c, 0xcc]));
    assert!(target.opens(&[0x40, 0x55]));
    assert!(!target.opens(&[0x48, 0x89]));
    assert!(!target.opens(&[0xcc, 0x48, 0x89, 0x5c]));
    assert!(!target.opens(&[]));
    assert_eq!(target.longest(), 3);
    let empty = Target {
        symbol: "test.empty",
        prologues: &[&[]],
    };
    assert!(!empty.opens(&[0x90]));
}

#[test]
fn every_target_is_pinned_once_on_every_build_of_each_side_that_pins_it() {
    let rows = pinned();
    let targets = all();
    for row in &rows {
        assert!(
            targets.iter().any(|t| t.symbol == row.symbol),
            "{} is pinned but no target names it",
            row.symbol
        );
    }
    for target in &targets {
        let sides: Vec<Side> = rows
            .iter()
            .filter(|row| row.symbol == target.symbol)
            .map(|row| row.side)
            .collect();
        assert!(!sides.is_empty(), "{} is pinned on no build", target.symbol);
        let mut builds: Vec<Fingerprint> = rows
            .iter()
            .filter(|row| sides.contains(&row.side))
            .map(|row| row.build)
            .collect();
        builds.sort_unstable_by_key(|b| (b.guid(), b.age()));
        builds.dedup();
        for build in builds {
            let count = rows
                .iter()
                .filter(|row| row.symbol == target.symbol && row.build == build)
                .count();
            assert_eq!(count, 1, "{} on build {}", target.symbol, build_name(build));
        }
    }
}

/// A swapped pin would pass a check whose prologue another target's code
/// also opens with.
#[test]
fn no_prologue_of_one_target_opens_like_one_of_another() {
    let targets = all();
    for (n, target) in targets.iter().enumerate() {
        for other in &targets[n + 1..] {
            assert_ne!(target.symbol, other.symbol);
            for mine in target.prologues {
                for theirs in other.prologues {
                    assert!(
                        !mine.starts_with(theirs) && !theirs.starts_with(mine),
                        "{} and {} open alike: {mine:02x?}, {theirs:02x?}",
                        target.symbol,
                        other.symbol
                    );
                }
            }
        }
    }
}

/// On the executables `ENSH_CLIENT_EXE` and `ENSH_SERVER_EXE` name, each
/// pinned target's code opens with one of its prologues and no other
/// target's code does; skipped without them, or for a build the manifest
/// does not pin yet.
#[test]
fn every_pinned_target_opens_with_its_prologue_and_no_other_in_the_executables() {
    let rows = pinned();
    let targets = all();
    for variable in ["ENSH_CLIENT_EXE", "ENSH_SERVER_EXE"] {
        let Some(bytes) = std::env::var_os(variable).and_then(|p| std::fs::read(p).ok()) else {
            continue;
        };
        let Ok(image) = Image::parse(&bytes, Layout::File, None) else {
            unreachable!("{variable} parses");
        };
        let Some(build) = image.fingerprint() else {
            unreachable!("{variable} has a fingerprint");
        };
        let here: Vec<&Row> = rows.iter().filter(|row| row.build == build).collect();
        if here.is_empty() {
            eprintln!(
                "skipped: {variable} is build {}, which the manifest does not pin",
                build_name(build)
            );
            continue;
        }
        let code = |row: &Row| {
            let longest = targets.iter().map(|t| t.longest()).max().unwrap_or(0);
            image
                .read(image.base() + row.rva, longest as u64)
                .unwrap_or_default()
        };
        for row in &here {
            let found = code(row);
            for target in &targets {
                let opens = target.opens(found);
                if target.symbol == row.symbol {
                    assert!(opens, "{}: {found:02x?}", row.symbol);
                } else {
                    assert!(
                        !opens,
                        "{}'s code opens as {} does",
                        row.symbol, target.symbol
                    );
                }
            }
        }
    }
}
