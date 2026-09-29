//! CRC-32, the IEEE polynomial, computed without a table.
//!
//! A table-driven CRC would be faster and would need 1 KB of flash or a computed table at boot.
//! The config region is written a few times in a machine's life, so speed is irrelevant and a
//! table is not worth the flash on a C6.

/// Computes the CRC-32 of a byte slice. Matches the IEEE 802.3 polynomial, reflected, which is
/// what every tool that inspects a flash image expects.
pub fn checksum(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published check value for the IEEE CRC-32 of "123456789".
    #[test]
    fn it_matches_the_published_check_value() {
        assert_eq!(checksum(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn an_empty_slice_has_the_standard_value() {
        assert_eq!(checksum(b""), 0);
    }

    #[test]
    fn a_single_bit_change_changes_the_checksum() {
        let base = checksum(b"the quick brown fox");
        let mut flipped = *b"the quick brown fox";
        flipped[3] ^= 0x01;
        assert_ne!(base, checksum(&flipped));
    }

    #[test]
    fn the_checksum_is_order_sensitive() {
        assert_ne!(checksum(b"ab"), checksum(b"ba"));
    }

    #[test]
    fn it_is_deterministic() {
        let data = b"repeatable";
        assert_eq!(checksum(data), checksum(data));
    }
}
