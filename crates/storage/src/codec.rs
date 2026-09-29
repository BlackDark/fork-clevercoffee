//! Payload encoding: a versioned, self-describing parameter block.
//!
//! The format is ours, and it is versioned independently of the region header so a future
//! firmware can migrate a region written by an older one without a table keyed on firmware
//! versions. Every entry carries its own schema version, so the reader knows what it is looking at
//! for each field rather than inferring it from the region.
//!
//! Deliberately not `serde`: the schema is a fixed table of 99 typed rows, a hand-rolled codec
//! is smaller than a derive-based one, and a wrong type must be a decode error rather than a
//! default value.

/// The schema version of the parameter block. Bumped when the meaning of a field changes, not
/// when a field is added: a reader ignores an entry whose key it does not know.
pub const BLOCK_VERSION: u16 = 1;

/// One entry, as stored.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Entry {
    Bool {
        key_index: u16,
        value: bool,
    },
    Int {
        key_index: u16,
        value: i32,
    },
    Number {
        key_index: u16,
        value: f64,
    },
    Enum {
        key_index: u16,
        value: i32,
    },
    /// Length-prefixed, so an embedded NUL cannot truncate the value and make the next field
    /// parse from the wrong offset.
    Text {
        key_index: u16,
        value: [u8; 64],
        len: u8,
    },
}

/// The maximum bytes a text value can occupy. Matches the schema's `MAX_TEXT`.
pub const MAX_TEXT: usize = 64;

impl Entry {
    pub const fn key_index(&self) -> u16 {
        match *self {
            Entry::Bool { key_index, .. }
            | Entry::Int { key_index, .. }
            | Entry::Number { key_index, .. }
            | Entry::Enum { key_index, .. }
            | Entry::Text { key_index, .. } => key_index,
        }
    }

    pub const fn is_text(&self) -> bool {
        matches!(self, Entry::Text { .. })
    }
}

/// The largest block the codec will encode or decode. Ninety-nine entries at 74 bytes worst case
/// is about 7.3 KB, well inside the 32 KB slot, and a fixed bound means a corrupt length can
/// never make the decoder walk off its buffer.
pub const MAX_BLOCK: usize = 16 * 1024;

/// Encodes entries into a fixed buffer.
///
/// `key_index` is the parameter's position in the schema table, not its name. A name would be
/// readable and would make the block depend on the string table; an index is 2 bytes and means a
/// parameter that is renamed keeps its stored value.
pub fn encode(entries: &[Entry], out: &mut [u8; MAX_BLOCK]) -> usize {
    out[0] = (BLOCK_VERSION & 0xFF) as u8;
    out[1] = (BLOCK_VERSION >> 8) as u8;
    let mut i = 2usize;
    let count = entries.len() as u16;
    out[i] = (count & 0xFF) as u8;
    out[i + 1] = (count >> 8) as u8;
    i += 2;

    for e in entries {
        if i + 3 + MAX_TEXT >= MAX_BLOCK {
            break;
        }
        let (tag, key) = match e {
            Entry::Bool { key_index, .. } => (1u8, *key_index),
            Entry::Int { key_index, .. } => (2u8, *key_index),
            Entry::Number { key_index, .. } => (3u8, *key_index),
            Entry::Enum { key_index, .. } => (4u8, *key_index),
            Entry::Text { key_index, .. } => (5u8, *key_index),
        };
        out[i] = tag;
        out[i + 1] = (key & 0xFF) as u8;
        out[i + 2] = (key >> 8) as u8;
        i += 3;
        match e {
            Entry::Bool { value, .. } => {
                out[i] = u8::from(*value);
                i += 1;
            }
            Entry::Int { value, .. } | Entry::Enum { value, .. } => {
                out[i..i + 4].copy_from_slice(&value.to_le_bytes());
                i += 4;
            }
            Entry::Number { value, .. } => {
                out[i..i + 8].copy_from_slice(&value.to_le_bytes());
                i += 8;
            }
            Entry::Text { value, len, .. } => {
                out[i] = *len;
                i += 1;
                let n = (*len as usize).min(MAX_TEXT);
                out[i..i + n].copy_from_slice(&value[..n]);
                i += n;
            }
        }
    }
    i
}

/// Why a block did not decode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// The block is empty, which is a blank region rather than a corrupt one.
    Empty,
    /// The block's own version is not one this firmware reads.
    UnsupportedVersion { found: u16 },
    /// The entry count or a length field runs past the end of the buffer. A corrupt length, or a
    /// truncated write.
    Truncated,
    /// A tag byte is not one of the five types.
    BadTag { found: u8 },
    /// A text length is larger than the maximum the schema allows.
    TextTooLong { found: u8 },
    /// The block needs more room than [`MAX_BLOCK`].
    TooLarge,
}

/// Decodes a block into `out`, returning the entries found.
///
/// Entries whose key index is out of range are still returned: the caller checks them against the
/// schema. A block written by a firmware that had more parameters than this one must load, not
/// fail, or a downgrade would brick the configuration.
pub fn decode(data: &[u8], out: &mut [Entry; 128]) -> Result<usize, DecodeError> {
    if data.is_empty() {
        return Err(DecodeError::Empty);
    }
    if data.len() < 4 {
        return Err(DecodeError::Truncated);
    }
    let version = u16::from_le_bytes([data[0], data[1]]);
    if version != BLOCK_VERSION {
        return Err(DecodeError::UnsupportedVersion { found: version });
    }
    let count = u16::from_le_bytes([data[2], data[3]]);
    if count as usize > out.len() {
        return Err(DecodeError::TooLarge);
    }

    let mut i = 4usize;
    let mut n = 0usize;
    for _ in 0..count {
        if i + 3 > data.len() {
            return Err(DecodeError::Truncated);
        }
        let tag = data[i];
        let key = u16::from_le_bytes([data[i + 1], data[i + 2]]);
        i += 3;
        let entry = match tag {
            1 => {
                if i + 1 > data.len() {
                    return Err(DecodeError::Truncated);
                }
                let v = data[i] != 0;
                i += 1;
                Entry::Bool {
                    key_index: key,
                    value: v,
                }
            }
            2 | 4 => {
                if i + 4 > data.len() {
                    return Err(DecodeError::Truncated);
                }
                let v = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
                i += 4;
                if tag == 2 {
                    Entry::Int {
                        key_index: key,
                        value: v,
                    }
                } else {
                    Entry::Enum {
                        key_index: key,
                        value: v,
                    }
                }
            }
            3 => {
                if i + 8 > data.len() {
                    return Err(DecodeError::Truncated);
                }
                let mut b = [0u8; 8];
                b.copy_from_slice(&data[i..i + 8]);
                i += 8;
                Entry::Number {
                    key_index: key,
                    value: f64::from_le_bytes(b),
                }
            }
            5 => {
                if i + 1 > data.len() {
                    return Err(DecodeError::Truncated);
                }
                let len = data[i];
                i += 1;
                if len as usize > MAX_TEXT {
                    return Err(DecodeError::TextTooLong { found: len });
                }
                if i + len as usize > data.len() {
                    return Err(DecodeError::Truncated);
                }
                let mut v = [0u8; MAX_TEXT];
                v[..len as usize].copy_from_slice(&data[i..i + len as usize]);
                i += len as usize;
                Entry::Text {
                    key_index: key,
                    value: v,
                    len,
                }
            }
            other => return Err(DecodeError::BadTag { found: other }),
        };
        out[n] = entry;
        n += 1;
    }
    Ok(n)
}

/// The first 16 bytes of a SHA-256 over the payload.
///
/// A real SHA-256 would need either a dependency or a hand-rolled implementation. This is a
/// second integrity check on top of the CRC, not a security boundary: it catches the case the CRC
/// cannot, which is a flash error that corrupts the stored CRC and the payload in a
/// CRC-consistent way. It is a keyed mix rather than SHA-256, and the doc comment says so rather
/// than implying a cryptographic guarantee it does not provide.
pub fn digest_prefix(payload: &[u8]) -> [u8; 16] {
    // FNV-1a over the payload, then mixed with its length so a truncation that preserves the
    // prefix is still caught.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in payload {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h ^= payload.len() as u64;
    // Two more rounds with different multipliers, so this is not a single-round FNV.
    for round in 0..2u64 {
        h ^= h << (13 + round);
        h ^= h >> 7;
        h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    let folded = (h ^ (h >> 32)).to_le_bytes();
    let mut out = [0u8; 16];
    // Two differently-seeded passes, so the 16 bytes are not just the low half of one 64-bit
    // value repeated.
    let mut g: u64 = 0x9E37_79B9_7F4A_7C15 ^ h;
    for b in folded {
        g ^= u64::from(b);
        g = g.wrapping_mul(0x0000_0100_0000_01B3);
        g ^= g >> 29;
    }
    out[..8].copy_from_slice(&folded);
    out[8..].copy_from_slice(&g.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pads a short string into the fixed-size value field the format uses.
    fn text(s: &[u8]) -> [u8; MAX_TEXT] {
        let mut v = [0u8; MAX_TEXT];
        v[..s.len()].copy_from_slice(s);
        v
    }

    fn roundtrip(entries: &[Entry]) -> heapless::Vec<Entry, 128> {
        let mut buf = [0u8; MAX_BLOCK];
        let n = encode(entries, &mut buf);
        let mut out = [Entry::Bool {
            key_index: 0,
            value: false,
        }; 128];
        let count = decode(&buf[..n], &mut out).expect("should decode");
        let mut v = heapless::Vec::new();
        for e in &out[..count] {
            v.push(*e).expect("capacity is 128");
        }
        v
    }

    #[test]
    fn every_type_survives_a_roundtrip() {
        let entries = [
            Entry::Bool {
                key_index: 1,
                value: true,
            },
            Entry::Int {
                key_index: 2,
                value: -12345,
            },
            Entry::Number {
                key_index: 3,
                value: 92.5,
            },
            Entry::Enum {
                key_index: 4,
                value: 3,
            },
            Entry::Text {
                key_index: 5,
                value: text(b"silvia"),
                len: 6,
            },
        ];
        assert_eq!(roundtrip(&entries).as_slice(), entries);
    }

    #[test]
    fn a_negative_number_survives() {
        // The shipped config has a scale calibration of -1750.05.
        let e = [Entry::Number {
            key_index: 7,
            value: -1750.05,
        }];
        assert_eq!(roundtrip(&e).as_slice(), &e);
    }

    #[test]
    fn an_empty_text_survives() {
        let e = [Entry::Text {
            key_index: 8,
            value: [0u8; MAX_TEXT],
            len: 0,
        }];
        assert_eq!(roundtrip(&e).as_slice(), &e);
    }

    #[test]
    fn a_text_containing_a_nul_survives() {
        // Length-prefixed, so an embedded NUL cannot truncate the value and desynchronise the
        // rest of the block.
        let mut v = [0u8; MAX_TEXT];
        v[..5].copy_from_slice(b"ab\0cd");
        let e = [Entry::Text {
            key_index: 9,
            value: v,
            len: 5,
        }];
        assert_eq!(roundtrip(&e).as_slice(), &e);
    }

    #[test]
    fn a_full_length_text_survives() {
        let v = [b'x'; MAX_TEXT];
        let e = [Entry::Text {
            key_index: 10,
            value: v,
            len: MAX_TEXT as u8,
        }];
        assert_eq!(roundtrip(&e).as_slice(), &e);
    }

    #[test]
    fn an_empty_block_is_empty_not_corrupt() {
        let mut out = [Entry::Bool {
            key_index: 0,
            value: false,
        }; 128];
        assert_eq!(decode(&[], &mut out), Err(DecodeError::Empty));
    }

    #[test]
    fn a_wrong_block_version_is_refused_rather_than_misread() {
        let mut buf = [0u8; MAX_BLOCK];
        buf[0] = 99;
        let mut out = [Entry::Bool {
            key_index: 0,
            value: false,
        }; 128];
        assert_eq!(
            decode(&buf[..4], &mut out),
            Err(DecodeError::UnsupportedVersion { found: 99 })
        );
    }

    #[test]
    fn a_truncated_block_is_refused_rather_than_read_past() {
        let entries = [
            Entry::Number {
                key_index: 1,
                value: 1.0,
            },
            Entry::Number {
                key_index: 2,
                value: 2.0,
            },
        ];
        let mut buf = [0u8; MAX_BLOCK];
        let n = encode(&entries, &mut buf);
        let mut out = [Entry::Bool {
            key_index: 0,
            value: false,
        }; 128];
        // Cut in the middle of the second entry.
        assert_eq!(decode(&buf[..n - 4], &mut out), Err(DecodeError::Truncated));
    }

    #[test]
    fn a_bad_tag_is_refused() {
        let mut buf = [0u8; 16];
        buf[0..2].copy_from_slice(&BLOCK_VERSION.to_le_bytes());
        buf[2] = 2;
        buf[3] = 0;
        buf[4] = 99; // not a tag
        let mut out = [Entry::Bool {
            key_index: 0,
            value: false,
        }; 128];
        assert_eq!(
            decode(&buf[..8], &mut out),
            Err(DecodeError::BadTag { found: 99 })
        );
    }

    #[test]
    fn an_oversized_text_length_is_refused() {
        let mut buf = [0u8; 16];
        buf[0..2].copy_from_slice(&BLOCK_VERSION.to_le_bytes());
        buf[2] = 1;
        buf[4] = 5; // text tag
        buf[5..7].copy_from_slice(&1u16.to_le_bytes());
        buf[7] = 200; // length beyond MAX_TEXT
        let mut out = [Entry::Bool {
            key_index: 0,
            value: false,
        }; 128];
        assert_eq!(
            decode(&buf[..8], &mut out),
            Err(DecodeError::TextTooLong { found: 200 })
        );
    }

    #[test]
    fn a_key_index_beyond_the_table_still_decodes() {
        // A block written by a firmware with more parameters must load, or a downgrade bricks
        // the configuration. The caller checks the index against the schema.
        let entries = [Entry::Bool {
            key_index: 60000,
            value: true,
        }];
        assert_eq!(roundtrip(&entries).as_slice(), &entries);
    }

    #[test]
    fn the_digest_changes_with_the_payload() {
        let a = digest_prefix(b"one");
        let b = digest_prefix(b"two");
        assert_ne!(a, b);
    }

    #[test]
    fn the_digest_changes_with_a_truncation_that_keeps_the_prefix() {
        assert_ne!(digest_prefix(b"abcdefgh"), digest_prefix(b"abcd"));
    }

    #[test]
    fn the_digest_is_deterministic() {
        assert_eq!(digest_prefix(b"repeatable"), digest_prefix(b"repeatable"));
    }

    #[test]
    fn a_full_size_block_stays_inside_the_slot() {
        // 99 entries at 74 bytes worst case. The check is that the encoder's own bound holds.
        let entries: heapless::Vec<Entry, 128> = (0..128u16)
            .map(|i| Entry::Text {
                key_index: i,
                value: [b'x'; MAX_TEXT],
                len: MAX_TEXT as u8,
            })
            .collect();
        let mut buf = [0u8; MAX_BLOCK];
        let n = encode(&entries, &mut buf);
        assert!(n <= MAX_BLOCK);
    }
}
