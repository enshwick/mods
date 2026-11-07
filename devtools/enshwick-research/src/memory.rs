//! Reads of memory the game hands a function, which may not be mapped: a
//! bad pointer reads as nothing, never as a fault, and the lines that show
//! what was read.

use core::fmt::Write as _;

use enshwick::Off;
use enshwick::code::{FindError, Pattern, find_each_unique};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// The lowest and the highest address a user-mode pointer takes.
const USER: core::ops::Range<usize> = 0x1_0000..0x7fff_ffff_ffff;
/// The size of a page, the unit a read fails by.
const PAGE: usize = 0x1000;
/// Where the game's image sits in the addresses of the research notes.
pub(crate) const NOTES_BASE: usize = 0x1_4000_0000;
/// Bytes shown per line.
const LINE: usize = 0x40;

/// Whether `address` can be a pointer into this process.
pub(crate) fn plausible(address: usize) -> bool {
    USER.contains(&address)
}

/// Reads up to `out.len()` bytes at `address`: all of them, the part up to
/// the next page when only that is mapped, or none. Returns the count read.
pub(crate) fn read(address: usize, out: &mut [u8]) -> usize {
    if !plausible(address) || out.is_empty() {
        return 0;
    }
    if read_exact(address, out) {
        return out.len();
    }
    let to_page = PAGE - address % PAGE;
    if to_page < out.len() && read_exact(address, &mut out[..to_page]) {
        return to_page;
    }
    0
}

/// The `u64` at `address`, if mapped.
pub(crate) fn read_u64(address: usize) -> Option<u64> {
    let mut bytes = [0u8; 8];
    (read(address, &mut bytes) == bytes.len()).then(|| u64::from_le_bytes(bytes))
}

fn read_exact(address: usize, out: &mut [u8]) -> bool {
    let mut done = 0usize;
    // SAFETY: the pseudo handle of this process, which needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: ReadProcessMemory on this process checks the source range and
    // fails instead of faulting on memory that is not mapped; `out` is
    // writable for its length and `done` for a usize.
    let ok = unsafe {
        ReadProcessMemory(
            process,
            core::ptr::with_exposed_provenance(address),
            out.as_mut_ptr().cast(),
            out.len(),
            &raw mut done,
        )
    };
    ok != 0 && done == out.len()
}

/// A game function the mod hooks or calls, found by the bytes it opens with.
#[derive(Debug)]
pub(crate) struct Target {
    /// The name its log lines and tests give it.
    pub(crate) name: &'static str,
    /// The bytes it opens with, one entry per prologue the kept builds have,
    /// each opening it and no other place in the code of a kept executable.
    pub(crate) prologues: &'static [&'static [u8]],
}

impl Target {
    /// Whether `code` opens with one of the target's prologues.
    pub(crate) fn opens(&self, code: &[u8]) -> bool {
        self.prologues
            .iter()
            .any(|prologue| !prologue.is_empty() && code.starts_with(prologue))
    }

    /// The longest prologue's length.
    pub(crate) fn longest(&self) -> usize {
        self.prologues.iter().map(|p| p.len()).max().unwrap_or(0)
    }
}

/// The search pattern of `prologue`; `None` for an empty one.
fn pattern(prologue: &[u8]) -> Option<Pattern> {
    let text: Vec<String> = prologue.iter().map(|b| format!("{b:02x}")).collect();
    Pattern::parse(&text.join(" ")).ok()
}

/// Where each of `targets` sits in `code`, as offsets from its start: the one
/// place one of its prologues opens, in one pass for all of them.
pub(crate) fn offsets<const N: usize>(
    targets: [&Target; N],
    code: &[u8],
) -> [Result<usize, FindError>; N] {
    let owned: Vec<(usize, Pattern)> = targets
        .iter()
        .enumerate()
        .flat_map(|(n, target)| {
            target
                .prologues
                .iter()
                .filter_map(move |p| pattern(p).map(|p| (n, p)))
        })
        .collect();
    let patterns: Vec<&Pattern> = owned.iter().map(|(_, p)| p).collect();
    let found = find_each_unique(&patterns, code);
    core::array::from_fn(|n| {
        owned
            .iter()
            .zip(&found)
            .filter(|((owner, _), _)| *owner == n)
            .fold(Err(FindError::None), |at, (_, next)| match (at, *next) {
                (Err(FindError::None), next) => next,
                (at, Err(FindError::None)) => at,
                (Ok(at), Ok(next)) if at == next => Ok(at),
                _ => Err(FindError::Many),
            })
    })
}

/// The address of each of `targets` in the game's `code`, as
/// [`Load::code`](enshwick::Load::code)
/// gives it, found as [`offsets`] finds it; the bytes there are its prologue.
///
/// # Errors
///
/// [`Off`] per target, naming it and the running build, when none of its
/// prologues opens the code or they open more than one place.
pub(crate) fn find<const N: usize>(targets: [&Target; N], code: &[u8]) -> [Result<u64, Off>; N] {
    let base = code.as_ptr().addr() as u64;
    let found = offsets(targets, code);
    core::array::from_fn(|n| {
        found[n].map(|at| base + at as u64).map_err(|e| {
            let why = if e == FindError::Many {
                "its prologues open more than one place"
            } else {
                "no prologue of it opens any place"
            };
            Off::new(format!(
                "{}: {why} in the game's code; the game is {}",
                targets[n].name,
                running_build()
            ))
        })
    })
}

/// The running game's build fingerprint, as `guid/age`, for a warning.
fn running_build() -> String {
    let Some(host) = enshwick::host() else {
        return "of an unknown build".to_owned();
    };
    let fingerprint = host.build().fingerprint;
    let mut name = fingerprint
        .guid
        .iter()
        .fold("build ".to_owned(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
    let _ = write!(name, "/{}", fingerprint.age);
    name
}

/// The game image's base in this process.
pub(crate) fn image_base() -> usize {
    // SAFETY: a null name asks for the process's own image, which stays
    // loaded for the life of the process.
    let module = unsafe { GetModuleHandleW(core::ptr::null()) };
    module.addr()
}

/// The span of the game's image, its code and data.
const IMAGE_SPAN: usize = 0x400_0000;

/// `address` relative to the game's image at `base`, when it lies in it.
pub(crate) fn in_image(address: usize, base: usize) -> Option<usize> {
    address
        .checked_sub(base)
        .filter(|&rva| base != 0 && rva < IMAGE_SPAN)
}

/// `address` in the terms of the research notes (the image at
/// `0x140000000`) when it lies in the game's image, else as it is.
pub(crate) fn as_note(address: usize, base: usize) -> String {
    match in_image(address, base) {
        Some(rva) => format!("{:#x}", NOTES_BASE + rva),
        None => format!("{address:#x}"),
    }
}

/// `bytes` as little-endian `u32` words, `LINE` bytes a line, each line
/// opened by `label` and the offset.
pub(crate) fn words(label: &str, bytes: &[u8]) -> Vec<String> {
    bytes
        .chunks(LINE)
        .enumerate()
        .map(|(n, chunk)| {
            let mut line = format!("  {label}+{:03x}:", n * LINE);
            for word in chunk.chunks(4) {
                let mut padded = [0u8; 4];
                padded[..word.len()].copy_from_slice(word);
                let _infallible = write!(line, " {:08x}", u32::from_le_bytes(padded));
            }
            line
        })
        .collect()
}

/// Where `bytes` hold one of `values`: as a `u16` at an even offset, a
/// `u32` at an offset of four, or an `f32` of the same value.
pub(crate) fn matches(bytes: &[u8], values: &[u32]) -> Vec<String> {
    let mut found = Vec::new();
    if values.is_empty() {
        return found;
    }
    for (index, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        let offset = index * 2;
        let half = u32::from(u16::from_le_bytes(*pair));
        if values.contains(&half) {
            found.push(format!("u16 {half} at +{offset:#x}"));
        }
        if offset % 4 == 0
            && let Some(word) = bytes.get(offset..offset + 4)
        {
            let word = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            if word > u32::from(u16::MAX) && values.contains(&word) {
                found.push(format!("u32 {word} at +{offset:#x}"));
            }
            let float = f32::from_bits(word);
            #[expect(clippy::float_cmp, reason = "a count is looked for exactly")]
            let counted = values
                .iter()
                .any(|&v| v != 0 && f64::from(float) == f64::from(v));
            if word != 0 && counted {
                found.push(format!("f32 {float} at +{offset:#x}"));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_opens_only_code_that_starts_with_one_of_its_prologues() {
        let target = Target {
            name: "test.target",
            prologues: &[&[0x48, 0x89, 0x5c], &[0x40, 0x55]],
        };
        assert!(target.opens(&[0x48, 0x89, 0x5c, 0xcc]));
        assert!(target.opens(&[0x40, 0x55]));
        assert!(!target.opens(&[0x48, 0x89]));
        assert!(!target.opens(&[0xcc, 0x48, 0x89, 0x5c]));
        assert!(!target.opens(&[]));
        assert_eq!(target.longest(), 3);
        let empty = Target {
            name: "test.empty",
            prologues: &[&[]],
        };
        assert!(!empty.opens(&[0x90]));
    }

    #[test]
    fn a_target_is_found_only_where_its_prologues_open_exactly_one_place() {
        let two = Target {
            name: "test.two",
            prologues: &[&[0x48, 0x89, 0x5c], &[0x40, 0x55]],
        };
        let prefix = Target {
            name: "test.prefix",
            prologues: &[&[0x41, 0x57], &[0x41, 0x57, 0x48]],
        };
        let empty = Target {
            name: "test.empty",
            prologues: &[&[]],
        };
        let code = [0xcc, 0x48, 0x89, 0x5c, 0xcc, 0x41, 0x57, 0x48, 0xcc];
        assert_eq!(
            offsets([&two, &prefix, &empty], &code),
            [Ok(1), Ok(5), Err(FindError::None)]
        );
        let both = [0x40, 0x55, 0xcc, 0x48, 0x89, 0x5c];
        assert_eq!(offsets([&two], &both), [Err(FindError::Many)]);
        let twice = [0x40, 0x55, 0xcc, 0x40, 0x55];
        assert_eq!(offsets([&two], &twice), [Err(FindError::Many)]);
        assert_eq!(offsets([&two], &[0x90; 4]), [Err(FindError::None)]);
    }

    #[test]
    fn a_null_or_kernel_pointer_reads_nothing() {
        let mut out = [0u8; 8];
        assert_eq!(read(0, &mut out), 0);
        assert_eq!(read(0xffff_8000_0000_0000, &mut out), 0);
        assert_eq!(read_u64(0x10), None);
    }

    #[test]
    fn mapped_memory_reads_back() {
        let source = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut out = [0u8; 8];
        assert_eq!(read(source.as_ptr().addr(), &mut out), 8);
        assert_eq!(out, source);
        assert_eq!(
            read_u64(source.as_ptr().addr()),
            Some(u64::from_le_bytes(source))
        );
    }

    #[test]
    fn words_are_grouped_by_line() {
        let bytes: Vec<u8> = (0..0x44u8).collect();
        let lines = words("rcx", &bytes);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("  rcx+000: 03020100 07060504"));
        assert_eq!(lines[1], "  rcx+040: 43424140");
    }

    #[test]
    fn matches_name_the_width_and_offset() {
        let mut bytes = [0u8; 16];
        bytes[6..8].copy_from_slice(&3u16.to_le_bytes());
        bytes[8..12].copy_from_slice(&70_000u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&3.0f32.to_le_bytes());
        let found = matches(&bytes, &[3, 70_000]);
        assert!(found.contains(&"u16 3 at +0x6".to_owned()), "{found:?}");
        assert!(found.contains(&"u32 70000 at +0x8".to_owned()), "{found:?}");
        assert!(found.contains(&"f32 3 at +0xc".to_owned()), "{found:?}");
        assert!(matches(&bytes, &[]).is_empty());
    }

    #[test]
    fn addresses_in_the_image_read_as_in_the_notes() {
        assert_eq!(as_note(0x7ff6_0000_1234, 0x7ff6_0000_0000), "0x140001234");
        assert_eq!(as_note(0x1234, 0x7ff6_0000_0000), "0x1234");
    }
}
