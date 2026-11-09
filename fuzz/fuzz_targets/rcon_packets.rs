//! What a remote console client sends Admin Tools: reading packets never
//! panics, never takes more bytes than it has, and every packet it reads
//! encodes back to the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;

// The module itself, since the mod's crate is also a DLL a fuzzer cannot link.
#[expect(dead_code, reason = "the target reads packets and leaves the rest")]
#[path = "../../admin-tools/src/rcon/packet.rs"]
mod packet;

use packet::{Packet, parse};

fuzz_target!(|bytes: &[u8]| {
    let mut rest = bytes;
    while let Ok(Some((packet, used))) = parse(rest) {
        assert!(used <= rest.len());
        assert_eq!(packet.encode(), rest[..used]);
        for answer in Packet::answer(packet.id, &String::from_utf8_lossy(&packet.body)) {
            let bytes = answer.encode();
            assert_eq!(parse(&bytes), Ok(Some((answer, bytes.len()))));
        }
        rest = &rest[used..];
    }
});
