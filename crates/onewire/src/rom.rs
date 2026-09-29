//! A device's 64-bit ROM code.

/// A 64-bit 1-Wire address: an 8-byte family code followed by a 48-bit serial number.
///
/// A `Copy` array rather than a `heapless::String` of hex, because the value is passed to the bus
/// as bytes and a hex form would need converting at every call site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Address(pub [u8; 8]);

impl Address {
    /// The address of a device that is not there. Used as a starting point, never returned.
    pub const ZERO: Self = Self([0; 8]);

    /// The 8-bit family code, which for a DS18B20 is 0x28.
    pub const fn family(&self) -> u8 {
        self.0[0]
    }

    /// The 48-bit serial number, big-endian within the last seven bytes.
    ///
    /// Written out rather than looped: a `const fn` cannot iterate a slice deref on this target,
    /// and seven shifts are clearer than the alternative.
    pub const fn serial(&self) -> u64 {
        ((self.0[1] as u64) << 40)
            | ((self.0[2] as u64) << 32)
            | ((self.0[3] as u64) << 24)
            | ((self.0[4] as u64) << 16)
            | ((self.0[5] as u64) << 8)
            | (self.0[6] as u64)
    }

    /// Builds an address from a family code and a 48-bit serial number.
    pub const fn from_parts(family: u8, serial: u64) -> Self {
        Self([
            family,
            (serial >> 40) as u8,
            (serial >> 32) as u8,
            (serial >> 24) as u8,
            (serial >> 16) as u8,
            (serial >> 8) as u8,
            serial as u8,
            0,
        ])
    }

    /// The address as lower-case hex, for a log line.
    ///
    /// A method rather than a free function so no call site formats the bytes itself, which is where
    /// byte order and padding would drift apart.
    pub fn to_hex(self) -> heapless::String<16> {
        use core::fmt::Write;
        let mut out = heapless::String::new();
        for byte in self.0 {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    /// Whether this is a DS18B20.
    ///
    /// Checked rather than assumed: the C++ firmware read whatever answered the reset, and a bus
    /// with a DS1820 on it would have been read with DS18B20 commands and produced plausible
    /// nonsense.
    pub const fn is_ds18b20(self) -> bool {
        self.family() == 0x28
    }
}

/// The most devices `search_all` will report.
///
/// 64-bit addresses allow 2^64 devices in principle, but a bus long enough to hold this many has a
/// capacitance problem long before it has an addressing problem, and the result vector is on the
/// stack. Sixteen is well past a real installation and bounds the cost of a bus in a fault.
pub const MAX_DEVICES: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_eight_bytes_little_endian_on_the_wire() {
        // The 48-bit serial goes out least-significant bit first on the wire, which leaves it in
        // little-endian byte order in the buffer. It is read back here big-endian within bytes 1
        // to 6, matching how every DS18B20 tool prints it.
        let a = Address([0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);
        assert_eq!(a.family(), 0x28);
        assert_eq!(a.serial(), 0x0102_0304_0506);
    }

    #[test]
    fn parts_round_trip() {
        let serial = 0x1234_5678_9ABC;
        let a = Address::from_parts(0x28, serial);
        assert_eq!(a.family(), 0x28);
        assert_eq!(a.serial(), serial);
    }

    #[test]
    fn a_zero_address_has_a_zero_family_and_is_not_a_ds18b20() {
        assert_eq!(Address::ZERO.family(), 0);
        assert!(!Address::ZERO.is_ds18b20());
    }

    #[test]
    fn only_family_28_is_taken_for_a_ds18b20() {
        assert!(Address([0x28, 1, 2, 3, 4, 5, 6, 7]).is_ds18b20());
        // A DS1820 shares the bus and answers the reset; reading it with DS18B20 commands returns
        // plausible nonsense, which is why the family is checked.
        assert!(!Address([0x10, 1, 2, 3, 4, 5, 6, 7]).is_ds18b20());
    }

    #[test]
    fn hex_is_sixteen_characters_and_zero_padded() {
        let a = Address([0x28, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10]);
        assert_eq!(a.to_hex(), "280a0b0c0d0e0f10");
    }
}
