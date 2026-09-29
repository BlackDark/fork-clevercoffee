//! Splitting a config file into protocol chunks.
//!
//! The device buffers a chunk before appending it, so a chunk is small enough to arrive intact on
//! a line that a terminal and a microcontroller's line buffer both accept. 512 bytes encodes to 684
//! base64 characters, which fits a 1024-byte line buffer with room for the command and the
//! terminator.
//!
//! The CRC-32 goes in the `CONFIG BEGIN` line rather than after the data, so the device knows what
//! it is being given before a single byte of it arrives and can refuse a mismatched length without
//! buffering the whole thing first.

/// How many payload bytes go in one chunk.
pub const CHUNK_BYTES: usize = 512;

/// Splits a payload into chunks of at most [`CHUNK_BYTES`].
///
/// The final chunk may be shorter; an empty payload yields no chunks at all, which the device sees
/// as a zero-length config rather than as one chunk of nothing.
pub fn split(payload: &[u8]) -> Vec<&[u8]> {
    payload.chunks(CHUNK_BYTES).collect()
}

/// The number of chunks a payload of `len` bytes needs.
///
/// On the host this is a convenience for reporting progress; the real loop must send
/// [`split`]'s output, because a caller that computed the count one way and chunked another would
/// send a count the device does not agree with.
pub fn count(len: usize) -> usize {
    len.div_ceil(CHUNK_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_payload_needs_no_chunks() {
        assert!(split(b"").is_empty());
        assert_eq!(count(0), 0);
    }

    #[test]
    fn a_payload_smaller_than_one_chunk_is_one_chunk() {
        let payload = b"{}";
        assert_eq!(split(payload), vec![&payload[..]]);
        assert_eq!(count(payload.len()), 1);
    }

    #[test]
    fn a_payload_larger_than_one_chunk_is_split() {
        // This is the case that matters: a real config export is about 4 KB, so it takes eight
        // chunks and a host that sends it as one line overflows the device's line buffer.
        let payload: Vec<u8> = (0..(CHUNK_BYTES * 3 + 7)).map(|i| i as u8).collect();
        let chunks = split(&payload);
        assert_eq!(chunks.len(), 4);
        assert_eq!(count(payload.len()), 4);
        for (i, c) in chunks.iter().enumerate() {
            let expected = if i == 3 { 7 } else { CHUNK_BYTES };
            assert_eq!(c.len(), expected, "chunk {i} has the wrong length");
        }
    }

    #[test]
    fn a_payload_of_exactly_one_chunk_is_not_split() {
        let payload = vec![7u8; CHUNK_BYTES];
        assert_eq!(split(&payload).len(), 1);
        assert_eq!(count(CHUNK_BYTES), 1);
    }

    #[test]
    fn a_payload_one_byte_over_a_chunk_is_split() {
        let payload = vec![7u8; CHUNK_BYTES + 1];
        assert_eq!(split(&payload).len(), 2);
        assert_eq!(count(CHUNK_BYTES + 1), 2);
    }

    #[test]
    fn the_chunks_reassemble_into_the_original() {
        // The property the device depends on. A chunker that dropped or reordered a byte would
        // corrupt a config, and the CRC would report a mismatch with no way to tell which side is
        // wrong.
        let payload: Vec<u8> = (0..10_000).map(|i| (i * 7 % 251) as u8).collect();
        let mut rebuilt = Vec::new();
        for c in split(&payload) {
            rebuilt.extend_from_slice(c);
        }
        assert_eq!(rebuilt, payload);
    }

    #[test]
    fn count_agrees_with_split_at_every_boundary() {
        // A host that reported a chunk count from `count` and sent `split`'s output would tell the
        // user one thing and send another.
        for len in [0, 1, CHUNK_BYTES - 1, CHUNK_BYTES, CHUNK_BYTES + 1, 4096] {
            assert_eq!(split(&vec![0u8; len]).len(), count(len), "length {len}");
        }
    }

    #[test]
    fn a_chunk_base64_encodes_within_a_devices_line_buffer() {
        // 512 bytes is 684 base64 characters. A device with a 1024-byte line buffer has room for
        // the command, the terminator and a future field, which is why the bound is this value
        // rather than something larger.
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode([0u8; CHUNK_BYTES]);
        assert_eq!(encoded.len(), 684);
        assert!(encoded.len() + "CONFIG ".len() + 2 <= 1024);
    }
}
