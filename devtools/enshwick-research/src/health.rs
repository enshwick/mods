//! R17: health set by the game's impact programs, outside `ApplyDamage`:
//! heals, revives and scripted kills. The writer every program node goes
//! through takes the attribute, the entity and a value pin into the
//! program's scratch; for the health attribute this logs the entity, the
//! pin's type and the value it sets, for the first lines of a run.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::state::ModLocal;
use enshwick::{Load, Next, Off};

use crate::memory;

/// The current-health attribute.
const HEALTH: u32 = 0x8eb8_4995;
/// Value pin types: `s32`, `u32`, `bool`, `f32`.
const PIN_TYPES: [(u32, &str); 4] = [
    (0x3294_0bec, "s32"),
    (0x0985_f31e, "u32"),
    (0xc894_953d, "bool"),
    (0xe89f_7410, "f32"),
];
/// The most health writes logged in a run.
const MOST: u32 = 300;

/// The health writes seen in this mod life: the bosses' and the others'.
#[derive(Debug, Default)]
struct Writes {
    seen: AtomicU32,
    other: AtomicU32,
}

static WRITES: ModLocal<Writes> = ModLocal::new();

fn writes() -> &'static Writes {
    WRITES.get_or_init(Writes::default)
}

type Writer = unsafe extern "system" fn(
    out: *mut u8,
    attribute: u32,
    entity: u32,
    value: *const u8,
    program: *const u8,
) -> *mut u8;

enshwick::detour! {
    static WRITER: unsafe extern "system" fn(
        out: *mut u8,
        attribute: u32,
        entity: u32,
        value: *const u8,
        program: *const u8,
    ) -> *mut u8 = writer;
}

/// The attribute writer, the same on every kept client and server build.
pub(crate) const TARGET: memory::Target = memory::Target {
    name: "research.impact_attribute",
    prologues: &[&[
        0x48, 0x89, 0x5c, 0x24, 0x08, 0x48, 0x89, 0x74, 0x24, 0x10, 0x48, 0x89, 0x7c, 0x24, 0x18,
        0x55, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48, 0x8b, 0xec, 0x48, 0x83, 0xec,
        0x70, 0x4c, 0x8b, 0x6d, 0x50,
    ]],
};

/// Links the impact programs' attribute writer.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let [writer] = memory::find([&TARGET], load.code()?);
    let writer = writer?;
    // SAFETY: the one place in the code that opens with the prologue of the
    // writer `WRITER` declares, as on every kept client and server build,
    // found on 1269579.
    unsafe { load.hook(&WRITER, writer) }
}

/// How many health writes and other attribute writes were seen.
pub(crate) fn counts() -> (u32, u32) {
    (
        writes().seen.load(Ordering::Relaxed),
        writes().other.load(Ordering::Relaxed),
    )
}

fn writer(
    next: Next<'_, Writer>,
    out: *mut u8,
    attribute: u32,
    entity: u32,
    value: *const u8,
    program: *const u8,
) -> *mut u8 {
    if attribute == HEALTH {
        let seen = writes().seen.fetch_add(1, Ordering::Relaxed);
        if seen < MOST {
            log(seen + 1, entity, value.addr(), program.addr());
        }
    } else {
        writes().other.fetch_add(1, Ordering::Relaxed);
    }
    // SAFETY: the game's own arguments, forwarded unchanged.
    unsafe { next.call(out, attribute, entity, value, program) }
}

/// The pin's type at `+8`, its scratch offset at `+0xe`, and the program's
/// scratch base at `+0x10`; build 1269579.
fn log(number: u32, entity: u32, value: usize, program: usize) {
    let mut pin = [0u8; 16];
    if memory::read(value, &mut pin) < pin.len() {
        enshwick::info!("R17 health write {number}: entity {entity}, unreadable pin");
        return;
    }
    let ty = u32::from_le_bytes([pin[8], pin[9], pin[10], pin[11]]);
    let offset = usize::from(u16::from_le_bytes([pin[14], pin[15]]));
    let name = PIN_TYPES
        .iter()
        .find(|(hash, _)| *hash == ty)
        .map_or("?", |(_, name)| name);
    let raw = memory::read_u64(program + 0x10)
        .and_then(|base| usize::try_from(base).ok())
        .and_then(|base| {
            let mut word = [0u8; 4];
            (memory::read(base + offset, &mut word) == 4).then(|| u32::from_le_bytes(word))
        });
    match raw {
        Some(raw) => enshwick::info!(
            "R17 health write {number}: entity {entity}, {name} pin {ty:#010x}, value {} (f32 {})",
            raw.cast_signed(),
            f32::from_bits(raw)
        ),
        None => enshwick::info!(
            "R17 health write {number}: entity {entity}, {name} pin {ty:#010x}, value unreadable"
        ),
    }
}
