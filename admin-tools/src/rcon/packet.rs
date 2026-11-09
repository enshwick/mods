//! Source RCON packets: a little-endian `i32` size, then an `i32` id, an
//! `i32` type, the body and two zero bytes; the size counts the bytes
//! after it.
//!
//! The format is Valve's "Source RCON Protocol", which RCON tools speak.

/// The type of a login packet from a client.
pub(crate) const AUTH: i32 = 3;

/// The type of a command from a client, and of the server's answer to a
/// login.
pub(crate) const EXEC: i32 = 2;

/// The type of the server's answer to a command.
pub(crate) const RESPONSE: i32 = 0;

/// The longest body a client may send, in bytes.
pub(crate) const MAX_BODY: usize = 4096;

/// The longest body of one answer packet; a longer answer goes in several.
pub(crate) const MAX_REPLY_BODY: usize = 4096;

/// The bytes of a packet's size, id and type fields and closing zeros
/// beside its body.
const FRAME: usize = 4 + 4 + 4 + 2;

/// The smallest size field: an id, a type and an empty body's two zeros.
const MIN_SIZE: usize = FRAME - 4;

/// One packet, as a client sends it or the server answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Packet {
    /// The id the client chose, which the answer repeats.
    pub(crate) id: i32,
    /// [`AUTH`], [`EXEC`] or [`RESPONSE`].
    pub(crate) kind: i32,
    /// The body without its closing zero.
    pub(crate) body: Vec<u8>,
}

/// Why bytes are no packet a client may send.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReadPacketError {
    /// A size field below the smallest packet or past [`MAX_BODY`].
    #[error("a packet size of {0} bytes")]
    Size(i32),
    /// A packet whose last two bytes are not zero.
    #[error("a packet without its closing zero bytes")]
    Unterminated,
}

/// The bytes after a size field `size` announces: checked against the
/// smallest packet and [`MAX_BODY`].
///
/// # Errors
///
/// [`ReadPacketError::Size`] for a size out of bounds.
pub(crate) fn length(size: [u8; 4]) -> Result<usize, ReadPacketError> {
    let size = i32::from_le_bytes(size);
    usize::try_from(size)
        .ok()
        .filter(|&n| (MIN_SIZE..=MIN_SIZE + MAX_BODY).contains(&n))
        .ok_or(ReadPacketError::Size(size))
}

/// The packet of the `rest` bytes that follow a size field [`length`]
/// accepted.
///
/// # Errors
///
/// [`ReadPacketError::Unterminated`] when the last two bytes are not zero.
pub(crate) fn decode(rest: &[u8]) -> Result<Packet, ReadPacketError> {
    let (head, body) = rest.split_at_checked(8).ok_or(ReadPacketError::Size(0))?;
    let body = body
        .strip_suffix(&[0, 0])
        .ok_or(ReadPacketError::Unterminated)?;
    let field =
        |at: usize| i32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
    Ok(Packet {
        id: field(0),
        kind: field(4),
        body: body.to_vec(),
    })
}

/// The first packet in `bytes` and the bytes it takes; `None` when `bytes`
/// holds only part of one.
///
/// # Errors
///
/// As [`length`] and [`decode`].
pub(crate) fn parse(bytes: &[u8]) -> Result<Option<(Packet, usize)>, ReadPacketError> {
    let Some((size, rest)) = bytes.split_first_chunk::<4>() else {
        return Ok(None);
    };
    let length = length(*size)?;
    let Some(rest) = rest.get(..length) else {
        return Ok(None);
    };
    Ok(Some((decode(rest)?, 4 + length)))
}

impl Packet {
    /// The packet's bytes on the wire.
    #[must_use]
    pub(crate) fn encode(&self) -> Vec<u8> {
        let size = i32::try_from(MIN_SIZE + self.body.len()).unwrap_or(i32::MAX);
        let mut bytes = Vec::with_capacity(FRAME + self.body.len());
        bytes.extend(size.to_le_bytes());
        bytes.extend(self.id.to_le_bytes());
        bytes.extend(self.kind.to_le_bytes());
        bytes.extend(&self.body);
        bytes.extend([0, 0]);
        bytes
    }

    /// The answer to the command `id`: `text` in packets of at most
    /// [`MAX_REPLY_BODY`] bytes, cut at character boundaries; one empty
    /// packet for an empty text.
    #[must_use]
    pub(crate) fn answer(id: i32, text: &str) -> Vec<Self> {
        let mut packets = Vec::new();
        let mut rest = text;
        loop {
            let mut end = rest.len().min(MAX_REPLY_BODY);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            let (part, after) = rest.split_at(end);
            packets.push(Self {
                id,
                kind: RESPONSE,
                body: part.as_bytes().to_vec(),
            });
            rest = after;
            if rest.is_empty() {
                return packets;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_login_packet_from_a_client_reads() {
        let bytes = Packet {
            id: 7,
            kind: AUTH,
            body: b"secret".to_vec(),
        }
        .encode();
        assert_eq!(&bytes[..4], &16i32.to_le_bytes());
        let (packet, used) = parse(&bytes).unwrap().unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!((packet.id, packet.kind), (7, AUTH));
        assert_eq!(packet.body, b"secret");
    }

    #[test]
    fn hostile_sizes_are_refused_before_any_body_is_read() {
        for size in [-1, 0, 9, i32::MAX, 10 + 4097] {
            assert_eq!(
                length(size.to_le_bytes()),
                Err(ReadPacketError::Size(size)),
                "{size}"
            );
        }
        assert_eq!(length(10i32.to_le_bytes()), Ok(10));
        assert_eq!(length(4106i32.to_le_bytes()), Ok(4106));
    }

    #[test]
    fn a_packet_without_its_closing_zeros_is_refused_and_part_of_one_waits() {
        let mut bytes = Packet {
            id: 1,
            kind: EXEC,
            body: b"adl online".to_vec(),
        }
        .encode();
        assert_eq!(parse(&bytes[..bytes.len() - 1]), Ok(None));
        assert_eq!(parse(&bytes[..3]), Ok(None));
        let last = bytes.len() - 1;
        bytes[last] = b'x';
        assert_eq!(parse(&bytes), Err(ReadPacketError::Unterminated));
    }

    #[test]
    fn a_long_answer_splits_at_character_boundaries_and_an_empty_one_is_one_packet() {
        let text = "ї".repeat(MAX_REPLY_BODY);
        let packets = Packet::answer(3, &text);
        assert!(packets.len() > 1);
        assert!(packets.iter().all(|p| p.body.len() <= MAX_REPLY_BODY));
        let joined: Vec<u8> = packets.iter().flat_map(|p| p.body.clone()).collect();
        assert_eq!(String::from_utf8(joined).unwrap(), text);
        assert_eq!(Packet::answer(4, "").len(), 1);
    }

    proptest! {
        #[test]
        fn any_packet_a_client_may_send_reads_back_as_itself(
            id in any::<i32>(),
            kind in any::<i32>(),
            body in proptest::collection::vec(any::<u8>(), 0..=MAX_BODY),
        ) {
            let packet = Packet { id, kind, body };
            let bytes = packet.encode();
            prop_assert_eq!(parse(&bytes), Ok(Some((packet, bytes.len()))));
        }

        #[test]
        fn any_bytes_parse_without_panicking_and_never_past_their_end(
            bytes in proptest::collection::vec(any::<u8>(), 0..64),
        ) {
            if let Ok(Some((_, used))) = parse(&bytes) {
                prop_assert!(used <= bytes.len());
            }
        }
    }
}
