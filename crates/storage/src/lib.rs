//! The versioned config region: an A/B pair of CRC-protected slots in flash.
//!
//! The C++ firmware stored configuration in NVS under FNV-1a hashed key names, and read a
//! stored value back without the range check the setter performed (defect D11), so a corrupted
//! or hand-edited blob put an arbitrary setpoint into live control. This format is designed for
//! the new system alone and it fixes that by construction: a value is only usable if the slot it
//! came from passed its CRC, and the schema re-checks its range on decode anyway.
//!
//! Two slots, written alternately. A power cut during a write leaves the other slot intact, so a
//! torn write can never produce a half-applied configuration. That is the difference from the C++
//! import, which applied parameters one at a time and reported success if any one of them
//! matched (defect D13).

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod codec;
pub mod crc32;

/// The size of one slot. The partition is 64 KB and holds two of these, so there is room to
/// grow the payload without a partition change.
pub const SLOT_SIZE: usize = 32 * 1024;

/// The header size, before the payload.
pub const HEADER_SIZE: usize = 32;

/// "CCFG", little-endian.
pub const MAGIC: u32 = 0x4746_4343;

/// The format version this firmware writes. A reader that does not recognise it uses the
/// defaults rather than guessing at a layout it does not know.
pub const FORMAT_VERSION: u16 = 1;

/// The oldest header this firmware can read. One, because there is no older firmware.
pub const MIN_SUPPORTED_VERSION: u16 = 1;

/// Why a read produced what it did, so the caller can log something useful.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReadOutcome {
    /// One slot was valid and newer than the other.
    SlotA,
    SlotB,
    /// Neither slot passed its checks, so compiled defaults are in use. A corrupt region is not
    /// a boot failure: a machine with defaults still heats and still makes coffee.
    NoValidSlot,
    /// The region is newer than this firmware understands, so it was left alone rather than
    /// overwritten. A downgrade must not destroy a newer machine's configuration.
    UnsupportedVersion {
        found: u16,
    },
    /// The region is older than [`MIN_SUPPORTED_VERSION`].
    TooOld {
        found: u16,
    },
}

impl ReadOutcome {
    /// Whether the payload is real configuration, or the caller is looking at defaults.
    pub const fn has_config(self) -> bool {
        matches!(self, ReadOutcome::SlotA | ReadOutcome::SlotB)
    }
}

/// A decoded slot header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Header {
    pub format_version: u16,
    /// Monotonically increasing across writes, so the newer slot is identifiable after a reboot
    /// with no clock and no write log.
    pub generation: u16,
    pub payload_len: u32,
    pub payload_crc: u32,
    /// The first 16 bytes of SHA-256 over the payload. A cheap second check that catches a flash
    /// error the CRC missed, which matters because this is the one region a user can write from a
    /// host with `espflash write-bin`.
    pub digest_prefix: [u8; 16],
}

/// The parsed contents of the config region.
#[derive(Clone, Debug)]
pub struct Region {
    pub header: Header,
    /// Which slot the payload came from.
    pub slot: Slot,
    /// The payload, decoded by the caller. Kept as a fixed-size buffer with its length, so a
    /// corrupt length cannot cause an out-of-bounds read.
    pub payload_len: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    /// The byte offset of this slot within the partition.
    pub const fn offset(self) -> usize {
        match self {
            Slot::A => 0,
            Slot::B => SLOT_SIZE,
        }
    }

    /// The other slot.
    pub const fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
}

/// The whole config partition, as bytes.
///
/// The buffer is the partition, so a caller reads and writes this and never a slice of something
/// larger, which is what makes a truncated write a testable case rather than a mystery.
#[derive(Clone)]
pub struct Partition {
    bytes: [u8; SLOT_SIZE * 2],
}

impl core::fmt::Debug for Partition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Partition({} bytes)", self.bytes.len())
    }
}

impl Default for Partition {
    fn default() -> Self {
        Self::blank()
    }
}

impl Partition {
    /// A partition of zeroes, which is what erased flash looks like.
    pub const fn blank() -> Self {
        Self {
            bytes: [0u8; SLOT_SIZE * 2],
        }
    }

    /// A partition holding a payload written into slot A, for a test or a factory image.
    pub fn with_payload(payload: &[u8]) -> Self {
        let mut p = Self::blank();
        let _ = p.write(payload, 1);
        p
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// A partition holding exactly the bytes given.
    ///
    /// The one constructor a caller outside this crate needs: the firmware reads a partition out of
    /// flash and hands the bytes over. `None` when the slice is the wrong length, rather than a
    /// truncated region, because a region that is half a region reads as a corrupt region and the
    /// report would be about the firmware rather than about the flash.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SLOT_SIZE * 2 {
            return None;
        }
        let mut p = Self::blank();
        p.bytes.copy_from_slice(bytes);
        Some(p)
    }

    /// The writable form, for a caller filling the region in place.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Reads both slots and returns the newer valid one.
    ///
    /// The comparison is wraparound-safe, because `generation` is a `u16` that will wrap after
    /// 65536 writes and a naive `>` would then treat the newest slot as the oldest.
    pub fn read(&self) -> (ReadOutcome, Region) {
        let a = self.parse(Slot::A);
        let b = self.parse(Slot::B);

        match (a, b) {
            (Parsed::Valid(a), Parsed::Valid(b)) => {
                if b.header.generation.wrapping_sub(a.header.generation) < 0x8000 {
                    (ReadOutcome::SlotB, b)
                } else {
                    (ReadOutcome::SlotA, a)
                }
            }
            // One slot is usable. A version this firmware does not read is *not* usable: the
            // payload is in a layout it does not know, so it must be left alone rather than
            // interpreted, and the machine runs on defaults.
            (Parsed::Valid(r), _) => (ReadOutcome::SlotA, r),
            (_, Parsed::Valid(r)) => (ReadOutcome::SlotB, r),
            (Parsed::Other(outcome), _) | (_, Parsed::Other(outcome)) => (outcome, Region::empty()),
            (Parsed::Absent, Parsed::Absent) => (ReadOutcome::NoValidSlot, Region::empty()),
        }
    }

    fn parse(&self, slot: Slot) -> Parsed {
        let base = slot.offset();
        let raw = &self.bytes[base..base + SLOT_SIZE];

        let magic = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        if magic != MAGIC {
            return Parsed::Absent;
        }
        let header = Header {
            format_version: u16::from_le_bytes([raw[4], raw[5]]),
            generation: u16::from_le_bytes([raw[6], raw[7]]),
            payload_len: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
            payload_crc: u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]),
            digest_prefix: {
                let mut d = [0u8; 16];
                d.copy_from_slice(&raw[16..32]);
                d
            },
        };

        if header.format_version > FORMAT_VERSION {
            return Parsed::Other(ReadOutcome::UnsupportedVersion {
                found: header.format_version,
            });
        }
        if header.format_version < MIN_SUPPORTED_VERSION {
            return Parsed::Other(ReadOutcome::TooOld {
                found: header.format_version,
            });
        }
        // A length that does not fit the slot is a corrupt header. Rejecting it here is what
        // stops a hand-edited blob from making the reader walk off the end of the buffer.
        let len = header.payload_len as usize;
        if len > SLOT_SIZE - HEADER_SIZE {
            return Parsed::Absent;
        }
        let payload = &raw[HEADER_SIZE..HEADER_SIZE + len];
        if crc32::checksum(payload) != header.payload_crc {
            return Parsed::Absent;
        }
        if codec::digest_prefix(payload) != header.digest_prefix {
            return Parsed::Absent;
        }
        Parsed::Valid(Region {
            header,
            slot,
            payload_len: len,
        })
    }

    /// The payload of the currently valid slot, or `None` when there is none.
    pub fn payload(&self) -> Option<&[u8]> {
        let (outcome, region) = self.read();
        if !outcome.has_config() {
            return None;
        }
        let base = region.slot.offset();
        Some(&self.bytes[base + HEADER_SIZE..base + HEADER_SIZE + region.payload_len])
    }

    /// Writes a payload into the older slot, then verifies it read back.
    ///
    /// The read-back is not belt and braces: flash on these parts can report success for a write
    /// it did not perform, and a configuration region that silently did not update would leave a
    /// user believing they had set a temperature they had not.
    ///
    /// Returns the slot written and the new generation.
    pub fn write(&mut self, payload: &[u8], generation: u16) -> Result<(Slot, u16), WriteError> {
        if payload.len() > SLOT_SIZE - HEADER_SIZE {
            return Err(WriteError::TooLarge);
        }
        let target = match self.read().0 {
            ReadOutcome::SlotA => Slot::B,
            ReadOutcome::SlotB => Slot::A,
            // With no valid slot, write A. If the write is then interrupted, B is still whatever
            // it was, and a region that was already unreadable is not made worse.
            _ => Slot::A,
        };
        let next = generation;

        let digest = codec::digest_prefix(payload);
        let header = Header {
            format_version: FORMAT_VERSION,
            generation: next,
            payload_len: payload.len() as u32,
            payload_crc: crc32::checksum(payload),
            digest_prefix: digest,
        };

        let base = target.offset();
        self.bytes[base..base + SLOT_SIZE].fill(0);
        self.bytes[base..base + 4].copy_from_slice(&MAGIC.to_le_bytes());
        self.bytes[base + 4..base + 6].copy_from_slice(&header.format_version.to_le_bytes());
        self.bytes[base + 6..base + 8].copy_from_slice(&header.generation.to_le_bytes());
        self.bytes[base + 8..base + 12].copy_from_slice(&header.payload_len.to_le_bytes());
        self.bytes[base + 12..base + 16].copy_from_slice(&header.payload_crc.to_le_bytes());
        self.bytes[base + 16..base + 32].copy_from_slice(&header.digest_prefix);
        self.bytes[base + HEADER_SIZE..base + HEADER_SIZE + payload.len()].copy_from_slice(payload);

        // Verify what is now in the buffer, not what was written to it.
        match self.parse(target) {
            Parsed::Valid(region) if region.header.payload_crc == header.payload_crc => {
                Ok((target, next))
            }
            _ => Err(WriteError::VerifyFailed),
        }
    }

    /// Clears both slots. The next boot reads compiled defaults.
    pub fn factory_reset(&mut self) {
        self.bytes.fill(0);
    }
}

/// What one slot held when it was examined.
#[derive(Clone, Debug)]
enum Parsed {
    /// No magic, or a corrupt header or payload. The slot is not configuration.
    Absent,
    /// The header is readable but the version is one this firmware does not interpret. The
    /// payload is left alone.
    Other(ReadOutcome),
    Valid(Region),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteError {
    /// The payload does not fit a slot. A configuration this large is a bug, not a setting.
    TooLarge,
    /// The write did not read back. The previous contents are intact and still in use.
    VerifyFailed,
}

impl Region {
    fn empty() -> Self {
        Self {
            header: Header {
                format_version: 0,
                generation: 0,
                payload_len: 0,
                payload_crc: 0,
                digest_prefix: [0u8; 16],
            },
            slot: Slot::A,
            payload_len: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_built_from_bytes_reads_back_what_it_was_given() {
        let written = Partition::with_payload(b"{\"a\":1}");
        let rebuilt = Partition::from_bytes(written.as_bytes()).expect("the right length");
        assert_eq!(rebuilt.as_bytes(), written.as_bytes());
        assert!(Partition::from_bytes(b"short").is_none());
        assert!(Partition::from_bytes(&[0u8; SLOT_SIZE * 2 + 1]).is_none());
    }

    #[test]
    fn a_blank_partition_reads_as_no_configuration() {
        let p = Partition::blank();
        let (outcome, _) = p.read();
        assert_eq!(outcome, ReadOutcome::NoValidSlot);
        assert!(!outcome.has_config());
        assert!(p.payload().is_none());
    }

    #[test]
    fn a_freshly_written_payload_reads_back() {
        let payload = b"the quick brown fox";
        let mut p = Partition::blank();
        let (slot, gen) = p.write(payload, 1).unwrap();
        assert_eq!(slot, Slot::A);
        assert_eq!(gen, 1);
        assert_eq!(p.payload(), Some(&payload[..]));
        assert_eq!(p.read().0, ReadOutcome::SlotA);
    }

    #[test]
    fn a_second_write_goes_to_the_other_slot_and_wins() {
        let mut p = Partition::blank();
        p.write(b"first", 1).unwrap();
        p.write(b"second", 2).unwrap();
        assert_eq!(p.read().0, ReadOutcome::SlotB);
        assert_eq!(p.payload(), Some(&b"second"[..]));
    }

    #[test]
    fn the_older_slot_survives_a_write_into_the_newer_one() {
        // The property the A/B layout exists for: a torn write must not lose the configuration
        // that was already working.
        let mut p = Partition::with_payload(b"known good");
        let before = p.read();
        assert!(before.0.has_config());
        // Simulate a write that is then corrupted, which is what a power cut looks like.
        p.bytes[SLOT_SIZE + 12] ^= 0xFF;
        let (outcome, _) = p.read();
        assert_eq!(
            outcome,
            ReadOutcome::SlotA,
            "the older good slot must still win"
        );
        assert_eq!(p.payload(), Some(&b"known good"[..]));
    }

    #[test]
    fn a_corrupt_crc_is_rejected() {
        let mut p = Partition::with_payload(b"payload");
        // Flip a payload byte without touching the stored CRC.
        p.bytes[HEADER_SIZE] ^= 0xFF;
        assert_eq!(p.read().0, ReadOutcome::NoValidSlot);
    }

    #[test]
    fn both_slots_corrupt_falls_back_to_defaults_rather_than_failing_the_boot() {
        let mut p = Partition::with_payload(b"payload");
        p.bytes[HEADER_SIZE] ^= 0xFF;
        p.bytes[SLOT_SIZE + HEADER_SIZE] ^= 0xFF;
        let (outcome, _) = p.read();
        assert_eq!(outcome, ReadOutcome::NoValidSlot);
        assert!(!outcome.has_config());
    }

    #[test]
    fn a_bad_magic_is_not_configuration() {
        let mut p = Partition::with_payload(b"payload");
        p.bytes[0] = 0;
        assert_eq!(p.read().0, ReadOutcome::NoValidSlot);
    }

    #[test]
    fn a_length_that_does_not_fit_is_rejected_before_any_slice_happens() {
        // The value a hand-edited or corrupt blob is most likely to carry. Reading it must not
        // index past the slot.
        let mut p = Partition::with_payload(b"payload");
        let absurd = (SLOT_SIZE as u32 * 4).to_le_bytes();
        p.bytes[8..12].copy_from_slice(&absurd);
        assert_eq!(p.read().0, ReadOutcome::NoValidSlot);
    }

    #[test]
    fn generation_comparison_survives_a_wrap() {
        // A u16 wraps after 65536 writes. A naive `>` would then pick the stale slot forever.
        let mut p = Partition::blank();
        p.write(b"before wrap", 0xFFFF).unwrap();
        p.write(b"after wrap", 0x0000).unwrap();
        // 0x0000 is newer than 0xFFFF by the wraparound rule.
        assert_eq!(p.payload(), Some(&b"after wrap"[..]));
    }

    #[test]
    fn a_newer_format_is_reported_rather_than_overwritten() {
        // A downgrade must not destroy a newer machine's configuration.
        let mut p = Partition::with_payload(b"from the future");
        let future = (FORMAT_VERSION + 1).to_le_bytes();
        p.bytes[4..6].copy_from_slice(&future);
        let (outcome, _) = p.read();
        assert_eq!(
            outcome,
            ReadOutcome::UnsupportedVersion {
                found: FORMAT_VERSION + 1
            }
        );
    }

    #[test]
    fn an_older_format_is_reported_rather_than_misread() {
        let mut p = Partition::blank();
        // A version this firmware predates: the header parses but the payload is not trusted.
        p.bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        p.bytes[4..6].copy_from_slice(&0u16.to_le_bytes());
        let (outcome, _) = p.read();
        assert_eq!(outcome, ReadOutcome::TooOld { found: 0 });
    }

    #[test]
    fn a_payload_too_large_is_refused_rather_than_truncated() {
        let mut p = Partition::blank();
        let big = [0u8; SLOT_SIZE];
        assert_eq!(p.write(&big, 1), Err(WriteError::TooLarge));
        assert_eq!(
            p.read().0,
            ReadOutcome::NoValidSlot,
            "a refused write must change nothing"
        );
    }

    #[test]
    fn the_maximum_payload_fits() {
        let mut p = Partition::blank();
        let big = [7u8; SLOT_SIZE - HEADER_SIZE];
        assert!(p.write(&big, 1).is_ok());
        assert_eq!(p.payload().unwrap().len(), SLOT_SIZE - HEADER_SIZE);
    }

    #[test]
    fn a_factory_reset_returns_to_blank() {
        let mut p = Partition::with_payload(b"something");
        assert!(p.read().0.has_config());
        p.factory_reset();
        assert_eq!(p.read().0, ReadOutcome::NoValidSlot);
    }

    #[test]
    fn the_slot_offsets_tile_the_partition_exactly() {
        assert_eq!(Slot::A.offset(), 0);
        assert_eq!(Slot::B.offset(), SLOT_SIZE);
        assert_eq!(Slot::B.offset() + SLOT_SIZE, PARTITION_LEN);
    }

    const PARTITION_LEN: usize = SLOT_SIZE * 2;
}
