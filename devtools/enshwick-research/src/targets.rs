//! What every function the mod hooks or calls holds: no other target's
//! prologue opens like its own, and on a kept executable its prologues open
//! one place in the code, the one it was found at on that build.

use core::fmt::Write as _;

use enshwick::abi::{Side, guid};
use enshwick_gamedata::reflect::{Fingerprint, Image, Layout};

use crate::memory::{self, Target};

/// Every target of the mod: the tooltip's ten, the interface's five, the
/// entity walk's five, then R17's and R19's.
pub(crate) fn all() -> Vec<&'static Target> {
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

/// The targets the mod looks for on `side`: all on a client, R17's and
/// R19's on a server.
fn on(side: Side) -> Vec<&'static Target> {
    let mut all = all();
    if side != Side::CLIENT {
        all.drain(..all.len() - 2);
    }
    all
}

/// Where each target of [`on`] its side opened on the kept builds, as side,
/// GUID, age and the addresses relative to the image, in [`on`]'s order.
const FOUND: [(Side, &str, u32, &[u64]); 8] = [
    (
        Side::CLIENT,
        "69f0e9573b32c24a8838d3fff550824c",
        410,
        &[
            0xbf_ef20, 0xbf_f750, 0xc9_dd10, 0xb6_7e00, 0xb5_62c0, 0xb5_6b80, 0xbe_b220, 0xbe_bb60,
            0xc3_da50, 0xb5_5140, 0xa2_fb10, 0xa3_1c50, 0xcc_a5a0, 0xcc_8e30, 0xb1_2120, 0x28_fa70,
            0x9a_71d0, 0x9a_8070, 0x9a_5080, 0x9a_5ec0, 0x3e_2cc0, 0xe5_f220,
        ],
    ),
    (
        Side::CLIENT,
        "69f0e9573b32c24a8838d3fff550824c",
        426,
        &[
            0xc0_0aa0, 0xc0_12d0, 0xc9_f7a0, 0xb6_98e0, 0xb5_7da0, 0xb5_8660, 0xbe_cda0, 0xbe_d6e0,
            0xc3_f5d0, 0xb5_6c20, 0xa3_1640, 0xa3_3780, 0xcc_c030, 0xcc_a8c0, 0xb1_3c30, 0x28_fc10,
            0x9a_85b0, 0x9a_9450, 0x9a_6460, 0x9a_72a0, 0x3e_3040, 0xe6_1de0,
        ],
    ),
    (
        Side::CLIENT,
        "69f0e9573b32c24a8838d3fff550824c",
        520,
        &[
            0xc1_00f0, 0xc1_0930, 0xcb_2400, 0xb7_8820, 0xb6_7aa0, 0xb6_8370, 0xbf_c2f0, 0xbf_cc30,
            0xc4_f0f0, 0xb6_6920, 0xa3_df60, 0xa4_00a0, 0xcd_f0c0, 0xcd_d950, 0xb2_3200, 0x29_4540,
            0x9b_3e60, 0x9b_4d00, 0x9b_1cf0, 0x9b_2b30, 0x3e_9d60, 0xe7_8c60,
        ],
    ),
    (
        Side::CLIENT,
        "e378da3d6079c049b30cd1604a0e7846",
        210,
        &[
            0xc1_01b0, 0xc1_09f0, 0xcb_24c0, 0xb7_88e0, 0xb6_7b60, 0xb6_8430, 0xbf_c3b0, 0xbf_ccf0,
            0xc4_f1b0, 0xb6_69e0, 0xa3_e020, 0xa4_0160, 0xcd_f180, 0xcd_da10, 0xb2_32c0, 0x29_4620,
            0x9b_3f20, 0x9b_4dc0, 0x9b_1db0, 0x9b_2bf0, 0x3e_9e40, 0xe7_8d20,
        ],
    ),
    (
        Side::SERVER,
        "1c129660cd6efc4f9f0b21652d5ff2bf",
        387,
        &[0x18_4b90, 0x91_3960],
    ),
    (
        Side::SERVER,
        "1c129660cd6efc4f9f0b21652d5ff2bf",
        399,
        &[0x18_5a40, 0x91_5fb0],
    ),
    (
        Side::SERVER,
        "1c129660cd6efc4f9f0b21652d5ff2bf",
        479,
        &[0x18_b0e0, 0x91_fe10],
    ),
    (
        Side::SERVER,
        "f1e760a53ff247409ab91462e88b8f49",
        224,
        &[0x18_b0e0, 0x91_fe90],
    ),
];

fn build_name(build: Fingerprint) -> String {
    let mut name = build.guid().iter().fold(String::new(), |mut hex, b| {
        write!(hex, "{b:02x}").unwrap();
        hex
    });
    write!(name, "/{}", build.age()).unwrap();
    name
}

fn reported(side: Side, build: Fingerprint) -> bool {
    let side = if side == Side::CLIENT {
        enshwick_gamedata::Side::Client
    } else {
        enshwick_gamedata::Side::Server
    };
    enshwick_gamedata::reported(side, build)
}

/// A mistyped GUID or age names no build, so its addresses would never be
/// compared.
#[test]
fn every_build_found_on_is_reported_and_lists_every_target_of_its_side() {
    for (side, hex, age, found) in FOUND {
        let build = Fingerprint::new(guid(hex), age);
        assert!(reported(side, build), "{} has no report", build_name(build));
        assert_eq!(found.len(), on(side).len(), "{}", build_name(build));
    }
}

/// A swapped prologue would find another target's code.
#[test]
fn no_prologue_of_one_target_opens_like_one_of_another() {
    let targets = all();
    for (n, target) in targets.iter().enumerate() {
        for other in &targets[n + 1..] {
            assert_ne!(target.name, other.name);
            for mine in target.prologues {
                for theirs in other.prologues {
                    assert!(
                        !mine.starts_with(theirs) && !theirs.starts_with(mine),
                        "{} and {} open alike: {mine:02x?}, {theirs:02x?}",
                        target.name,
                        other.name
                    );
                }
            }
        }
    }
}

/// On the executables `ENSH_CLIENT_EXE` and `ENSH_SERVER_EXE` name, each
/// target of their side is found, as the mod finds it, at one place, the one
/// it was found at on that build where [`FOUND`] lists the build; skipped
/// without them.
#[test]
fn every_target_is_found_once_in_the_executables_where_it_was_found_before() {
    for (variable, side) in [
        ("ENSH_CLIENT_EXE", Side::CLIENT),
        ("ENSH_SERVER_EXE", Side::SERVER),
    ] {
        let Some(bytes) = std::env::var_os(variable).and_then(|p| std::fs::read(p).ok()) else {
            continue;
        };
        let Ok(image) = Image::parse(&bytes, Layout::File, None) else {
            unreachable!("{variable} parses");
        };
        let Some(build) = image.fingerprint() else {
            unreachable!("{variable} has a fingerprint");
        };
        let Some(text) = image.section(".text").map(|s| image.range(s)) else {
            unreachable!("{variable} has code");
        };
        let Some(code) = image.read(text.start, text.end - text.start) else {
            unreachable!("{variable} reads its code");
        };
        let before = FOUND
            .iter()
            .find(|&&(s, hex, age, _)| s == side && Fingerprint::new(guid(hex), age) == build)
            .map(|&(_, _, _, found)| found);
        let targets = on(side);
        let rva = |at: usize| text.start - image.base() + at as u64;
        let found = memory::offsets(&targets, code);
        let mut wrong = Vec::new();
        for (n, (target, at)) in targets.iter().zip(found).enumerate() {
            match (at, before) {
                (Ok(at), Some(before)) if rva(at) != before[n] => wrong.push(format!(
                    "{} at {:#x}, before at {:#x}",
                    target.name,
                    rva(at),
                    before[n]
                )),
                (Ok(_), _) => {}
                (Err(e), _) => {
                    let places: Vec<String> = target
                        .prologues
                        .iter()
                        .flat_map(|p| {
                            code.windows(p.len())
                                .enumerate()
                                .filter(move |(_, w)| w == p)
                                .map(|(at, _)| format!("{:#x}", rva(at)))
                        })
                        .collect();
                    if let Some(before) = before {
                        let mine = image.read(image.base() + before[n], 0x100).unwrap();
                        let diffs: Vec<String> = places
                            .iter()
                            .map(|p| {
                                let at = u64::from_str_radix(&p[2..], 16).unwrap();
                                let other = image.read(image.base() + at, 0x100).unwrap();
                                let d = mine.iter().zip(other).position(|(a, b)| a != b);
                                if target.name.ends_with("text_field") {
                                    eprintln!("OTHER {p}: {:02x?}", &other[..0xc0]);
                                }
                                format!("{p}:{d:x?}")
                            })
                            .collect();
                        eprintln!("DIFF {}: {diffs:?}", target.name);
                        eprintln!("MINE {}: {:02x?}", target.name, &mine[..0xa0]);
                    }
                    wrong.push(format!("{}: {e}, at {places:?}", target.name));
                }
            }
        }
        assert!(wrong.is_empty(), "on {}: {wrong:#?}", build_name(build));
        eprintln!(
            "{variable}: build {}, {} targets found{}",
            build_name(build),
            targets.len(),
            if before.is_some() {
                " where they were before"
            } else {
                ""
            }
        );
    }
}
