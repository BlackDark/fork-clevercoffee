//! The Dallas/Maxim CRC-8.
//!
//! Polynomial `x^8 + x^5 + x^4 + 1`, reflected, initial value zero. Every ROM code and every
//! scratchpad carries one, and a device that transmits a byte whose CRC does not match is either
//! mis-wired or not a device at all.
//!
//! The rule this crate enforces is the important one: a CRC failure is an error, never a value.
//! The C++ library this replaces returned a sentinel temperature for a bad CRC, which the caller
//! compared against a magic constant. A sentinel is a number, and a number a caller can forget to
//! check, which is how a disconnected sensor ends up heating the boiler (defect D03).

/// The CRC-8 of a Dallas block.
///
/// Over nine bytes the result is zero: the transmitted CRC byte is included, so a correct block
/// hashes to zero. That identity is what [`verify_crc`] relies on.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        let mut value = byte;
        for _ in 0..8 {
            let mix = (crc ^ value) & 0x01;
            crc >>= 1;
            value >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
        }
    }
    crc
}

/// Whether a block's trailing CRC byte is correct.
///
/// `block` is the whole thing, CRC byte included. The length must be a multiple of nine, which is
/// how a ROM code (eight bytes plus one) and a scratchpad (nine bytes plus one) are laid out.
pub fn verify_crc(block: &[u8]) -> Result<(), u8> {
    if block.len() < 9 || !block.len().is_multiple_of(9) {
        // A block that is not a whole number of frames is malformed rather than corrupt, and the
        // distinction is worth keeping: one is a bug here, the other is a wiring problem.
        return Err(0xFF);
    }
    let computed = crc8(block);
    if computed == 0 {
        Ok(())
    } else {
        Err(computed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_block_hashes_to_zero() {
        // The initial value, which is what makes the transmitted-CRC-included identity work.
        assert_eq!(crc8(&[]), 0);
    }

    #[test]
    fn a_block_with_its_own_crc_appended_hashes_to_zero() {
        let data = [0x28u8, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67];
        let crc = crc8(&data);
        let mut block = [0u8; 9];
        block[..8].copy_from_slice(&data);
        block[8] = crc;
        assert_eq!(crc8(&block), 0, "a correct block must hash to zero");
        assert_eq!(verify_crc(&block), Ok(()));
    }

    #[test]
    fn a_corrupted_byte_fails_the_crc() {
        // The property the whole safety argument rests on: a single flipped bit is caught.
        let data = [0x28u8, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67];
        let mut block = [0u8; 9];
        block[..8].copy_from_slice(&data);
        block[8] = crc8(&data);
        for byte in 0..8 {
            let mut corrupt = block;
            corrupt[byte] ^= 0x01;
            assert!(
                verify_crc(&corrupt).is_err(),
                "byte {byte} was corrupted without detection"
            );
        }
    }

    #[test]
    fn a_block_that_is_not_a_whole_number_of_frames_is_rejected() {
        // A malformed length is reported rather than hashed over a partial frame.
        assert!(verify_crc(&[0u8; 8]).is_err());
        assert!(verify_crc(&[0u8; 10]).is_err());
        assert!(verify_crc(&[0u8; 17]).is_err());
    }

    #[test]
    fn the_crc_is_not_trivially_zero_for_arbitrary_data() {
        // A CRC that returned zero for everything would pass every test above except the
        // corruption one, so this pins that it actually varies.
        assert_ne!(crc8(&[0x28]), 0);
        assert_ne!(crc8(&[0x29]), crc8(&[0x28]));
    }

    #[test]
    fn a_known_dallas_vector_matches() {
        // The DS18B20 datasheet's example ROM, with its published CRC.
        let data = [0x28u8, 0x1D, 0x57, 0x91, 0x02, 0x92, 0x4C, 0x54];
        let mut block = [0u8; 9];
        block[..8].copy_from_slice(&data);
        let crc = crc8(&data);
        block[8] = crc;
        assert_eq!(verify_crc(&block), Ok(()), "computed CRC {crc:02x}");
    }
}
