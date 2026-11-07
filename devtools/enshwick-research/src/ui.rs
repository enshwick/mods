//! What the client's interface does where the loader's screens still need
//! engine facts:
//!
//! - R2, scroll areas: each scroll area's start (who opens it, its kept
//!   state) and its offset whenever it moves, frame to frame, as the wheel,
//!   the scrollbar or the gamepad scroll it;
//! - R3, tab bars: the tab labels and the chosen tab, of the world settings
//!   and every other tab bar of the same function, whenever they change;
//! - R5, text input: the game's text input dialog when it opens, and each
//!   text field's text whenever it changes.
//!
//! Every hook only reads, at the entry of a function, through reads that
//! cannot fault. Each site is found by the prologue it has on the kept
//! builds; a site it finds nowhere, or in more than one place, stays
//! unhooked.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use enshwick::abi::Registers;
use enshwick::{Load, Off};
use rustc_hash::FxHashMap;

use crate::memory;
use enshwick::Relock;

/// What a site is and so what it logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    /// A scroll area's start: `rcx` the area, `dl` vertical, `r8` its state.
    ScrollBegin,
    /// A scroll area's end: `rcx` the area; its state at +0x160.
    ScrollEnd,
    /// A tab bar: `rcx` the chosen tab, `rdx` a slice of labels.
    Tabs,
    /// The text input dialog: `rcx` its state.
    TextDialog,
    /// A text field's start: `rdx` the slice of its text. Two copies of it
    /// serve different callers.
    TextField,
}

/// One function looked at; its prologue found on client build 1269579 and
/// the same on every kept client build.
#[derive(Debug)]
pub(crate) struct Site {
    name: &'static str,
    pub(crate) target: memory::Target,
    kind: Kind,
}

pub(crate) const SITES: [Site; 5] = [
    Site {
        name: "R2 scroll begin",
        target: memory::Target {
            name: "research.scroll_begin",
            body: &[],
            copies: false,
            prologues: &[&[
                0x48, 0x8b, 0xc4, 0x55, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x57, 0x48, 0x8d, 0xa8,
                0xc8, 0xfd, 0xff, 0xff,
            ]],
        },
        kind: Kind::ScrollBegin,
    },
    Site {
        name: "R2 scroll end",
        target: memory::Target {
            name: "research.scroll_end",
            body: &[],
            copies: false,
            prologues: &[&[
                0x48, 0x8b, 0xc4, 0x55, 0x53, 0x56, 0x57, 0x41, 0x56, 0x41, 0x57, 0x48, 0x8d, 0xa8,
                0xd8, 0xfc, 0xff, 0xff,
            ]],
        },
        kind: Kind::ScrollEnd,
    },
    Site {
        name: "R3 tabs",
        target: memory::Target {
            name: "research.tabs",
            body: &[],
            copies: false,
            prologues: &[&[
                0x48, 0x8b, 0xc4, 0x55, 0x41, 0x55, 0x48, 0x8d, 0xa8, 0x98, 0xfa, 0xff, 0xff,
            ]],
        },
        kind: Kind::Tabs,
    },
    Site {
        name: "R5 text dialog",
        target: memory::Target {
            name: "research.text_dialog",
            body: &[],
            copies: false,
            prologues: &[&[0x48, 0x8b, 0xc4, 0x55, 0x53, 0x41, 0x55, 0x41]],
        },
        kind: Kind::TextDialog,
    },
    Site {
        name: "R5 text field",
        target: memory::Target {
            name: "research.text_field",
            copies: true,
            prologues: &[&[
                0x48, 0x89, 0x5c, 0x24, 0x10, 0x48, 0x89, 0x74, 0x24, 0x18, 0x57, 0x48, 0x83, 0xec,
                0x60, 0x4c, 0x8b, 0x5a, 0x08,
            ]],
            // The FNV-1a offset basis and prime of the hash it takes.
            body: &[
                (
                    0x29,
                    &[0x48, 0xb8, 0x25, 0x23, 0x22, 0x84, 0xe4, 0x9c, 0xf2, 0xcb],
                ),
                (
                    0x38,
                    &[0x48, 0xba, 0xb3, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00],
                ),
            ],
        },
        kind: Kind::TextField,
    },
];

/// A scroll area's kept state's offset at +0 and the area's state at +0x160.
const STATE_OFFSET: usize = 0;
const SCROLL_STATE: usize = 0x160;
/// A text slice: data, then length.
const SLICE_LEN: usize = 8;
/// A UI text: pointer, length, hash; 24 bytes each in a slice of labels.
const TEXT_SIZE: usize = 0x18;
/// The most bytes of a text logged.
const MAX_TEXT: usize = 96;
/// The most tabs logged.
const MAX_TABS: usize = 12;
/// The most distinct calls kept for comparing.
const MAX_KEPT: usize = 512;

/// What was last logged per call site and object, to log changes only.
struct Seen {
    start: Instant,
    base: usize,
    last: Mutex<FxHashMap<(usize, usize, Kind), String>>,
}

static SEEN: OnceLock<Seen> = OnceLock::new();

/// Links each site found, and each copy of a site with copies; how many.
///
/// # Errors
///
/// [`Off`] when the game's code cannot be read.
pub(crate) fn link(load: &mut Load<'_>) -> Result<usize, Off> {
    let code = load.code()?;
    let checked = memory::find(SITES.each_ref().map(|site| &site.target), code);
    let _first = SEEN.set(Seen {
        start: Instant::now(),
        base: memory::image_base(),
        last: Mutex::new(FxHashMap::default()),
    });
    let base = code.as_ptr().addr() as u64;
    let addresses: Vec<Result<Vec<u64>, Off>> = SITES
        .iter()
        .zip(checked)
        .map(|(site, address)| {
            address.map(|at| {
                if site.target.copies {
                    let places = memory::places(&site.target, code);
                    places.iter().map(|&place| base + place as u64).collect()
                } else {
                    vec![at]
                }
            })
        })
        .collect();
    let mut hooked = 0;
    for (site, addresses) in SITES.iter().zip(addresses) {
        let addresses: Vec<u64> = match addresses {
            Ok(addresses) => addresses,
            Err(off) => {
                enshwick::warn!("{} off: {off}", site.name);
                continue;
            }
        };
        let kind = site.kind;
        for address in addresses {
            // SAFETY: the one place in the code that opens with a prologue
            // the site has on the kept client builds, found on 1269579, or
            // each copy of it, where it starts an instruction; the handler
            // only reads the registers.
            match unsafe { load.hook_at(address, move |registers| seen(kind, registers)) } {
                Ok(()) => hooked += 1,
                Err(off) => enshwick::warn!("{} off: {off}", site.name),
            }
        }
    }
    Ok(hooked)
}

fn reg(value: u64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// The text of `len` bytes at `address`, cut and with control bytes
/// escaped; empty when not mapped.
fn text(address: usize, len: usize) -> String {
    let mut bytes = vec![0u8; len.min(MAX_TEXT)];
    let read = memory::read(address, &mut bytes);
    bytes.truncate(read);
    String::from_utf8_lossy(&bytes).escape_debug().to_string()
}

fn f32_at(address: usize) -> Option<f32> {
    let mut bytes = [0u8; 4];
    (memory::read(address, &mut bytes) == 4).then(|| f32::from_le_bytes(bytes))
}

fn seen(kind: Kind, registers: &mut Registers) {
    let Some(seen) = SEEN.get() else {
        return;
    };
    let caller = memory::read_u64(reg(registers.rsp)).map_or(0, reg);
    let (object, line) = match kind {
        Kind::ScrollBegin => {
            let state = reg(registers.r8);
            let offset = f32_at(state + STATE_OFFSET);
            (
                state,
                format!(
                    "area {:#x} vertical {} state {state:#x} offset {offset:?}",
                    reg(registers.rcx),
                    registers.rdx & 0xff
                ),
            )
        }
        Kind::ScrollEnd => {
            let area = reg(registers.rcx);
            let state = memory::read_u64(area + SCROLL_STATE).map_or(0, reg);
            let offset = f32_at(state + STATE_OFFSET);
            (
                state,
                format!("area {area:#x} state {state:#x} offset {offset:?}"),
            )
        }
        Kind::Tabs => {
            let chosen = reg(registers.rcx);
            let mut byte = [0u8; 1];
            let picked = (memory::read(chosen, &mut byte) == 1).then_some(byte[0]);
            let slice = reg(registers.rdx);
            let labels = memory::read_u64(slice).map_or(0, reg);
            let count = memory::read_u64(slice + 8).map_or(0, reg).min(MAX_TABS);
            let names: Vec<String> = (0..count)
                .map(|i| {
                    let entry = labels + i * TEXT_SIZE;
                    let ptr = memory::read_u64(entry).map_or(0, reg);
                    let len = memory::read_u64(entry + 8).map_or(0, reg);
                    text(ptr, len)
                })
                .collect();
            (chosen, format!("chosen {picked:?} of {count}: {names:?}"))
        }
        Kind::TextDialog => {
            let state = reg(registers.rcx);
            let mut head = [0u8; 0x40];
            let read = memory::read(state, &mut head);
            (
                state,
                format!("state {state:#x} head {:02x?}", &head[..read]),
            )
        }
        Kind::TextField => {
            let slice = reg(registers.rdx);
            let data = memory::read_u64(slice).map_or(0, reg);
            let len = memory::read_u64(slice + SLICE_LEN).map_or(0, reg);
            let cap = memory::read_u64(slice + 16).map_or(0, reg);
            (
                slice,
                format!(
                    "widget {:#x} slice {slice:#x} data {data:#x} len {len} cap {cap}: {:?}",
                    reg(registers.rcx),
                    text(data, len)
                ),
            )
        }
    };
    let key = (caller, object, kind);
    {
        let mut last = seen.last.relock();
        if last.get(&key) == Some(&line) {
            return;
        }
        if last.len() >= MAX_KEPT {
            last.clear();
        }
        last.insert(key, line.clone());
    }
    let site = SITES.iter().find(|s| s.kind == kind).map_or("", |s| s.name);
    enshwick::info!(
        "{site} at {:.3}s from {}: {line}",
        seen.start.elapsed().as_secs_f64(),
        memory::as_note(caller, seen.base)
    );
}
