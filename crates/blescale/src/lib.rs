//! BLE coffee-scale protocols, transport-free: which scales are supported, the bytes
//! to write, and how to read weight notifications. The BLE connection lives in the
//! firmware.
//!
//! Protocol knowledge from AcaiaArduinoBLE v4.0.1 (MIT, © 2023 Tate Mazer, with
//! Felicita support by Pio Baettig), reimplemented.

#![no_std]

/// Protocol family, chosen by the GATT service the scale offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Acaia Lunar before 2021.
    AcaiaOld,
    /// Acaia Lunar 2021+, Pyxis.
    AcaiaNew,
    /// Felicita Arc and compatibles.
    Generic,
    Decent,
    Bookoo,
    WeighMyBru,
}

/// GATT service and characteristics, as 16-bit or 128-bit UUID strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gatt {
    /// `None`: find the characteristic in any service (old Acaia firmware).
    pub service: Option<&'static str>,
    pub write: &'static str,
    pub notify: &'static str,
}

impl Kind {
    /// Probed in this order after connecting.
    pub const PROBE_ORDER: [Kind; 6] = [
        Kind::AcaiaOld,
        Kind::AcaiaNew,
        Kind::Generic,
        Kind::Bookoo,
        Kind::Decent,
        Kind::WeighMyBru,
    ];

    pub const fn gatt(self) -> Gatt {
        match self {
            Kind::AcaiaOld => Gatt {
                service: None,
                write: "2a80",
                notify: "2a80",
            },
            Kind::AcaiaNew => Gatt {
                service: Some("49535343-fe7d-4ae5-8fa9-9fafd205e455"),
                write: "49535343-8841-43f4-a8d4-ecbe34729bb3",
                notify: "49535343-1e4d-4bd9-ba61-23c647249616",
            },
            Kind::Generic => Gatt {
                service: Some("ff10"),
                write: "ff12",
                notify: "ff11",
            },
            Kind::Bookoo => Gatt {
                service: Some("0ffe"),
                write: "ff12",
                notify: "ff11",
            },
            Kind::Decent => Gatt {
                service: Some("0000fff0-0000-1000-8000-00805f9b34fb"),
                write: "000036f5-0000-1000-8000-00805f9b34fb",
                notify: "0000fff4-0000-1000-8000-00805f9b34fb",
            },
            Kind::WeighMyBru => Gatt {
                service: Some("6e400001-b5a3-f393-e0a9-e50e24dcca9e"),
                write: "6e400003-b5a3-f393-e0a9-e50e24dcca9e",
                notify: "6e400002-b5a3-f393-e0a9-e50e24dcca9e",
            },
        }
    }

    /// Acaia scales disconnect without a periodic heartbeat.
    pub const fn needs_heartbeat(self) -> bool {
        matches!(self, Kind::AcaiaOld | Kind::AcaiaNew)
    }

    /// Bytes to write right after subscribing to notifications.
    pub fn handshake(self) -> &'static [&'static [u8]] {
        match self {
            Kind::AcaiaOld | Kind::AcaiaNew => &[IDENTIFY, NOTIFICATION_REQUEST],
            _ => &[],
        }
    }
}

pub const HEARTBEAT_PERIOD_MS: u64 = 2_750;
/// No packet for this long: the connection is considered dead.
pub const MAX_PACKET_PERIOD_MS: u64 = 5_000;

pub const IDENTIFY: &[u8] = &[
    0xef, 0xdd, 0x0b, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0x31, 0x32, 0x33,
    0x34, 0x9a, 0x6d,
];
pub const HEARTBEAT: &[u8] = &[0xef, 0xdd, 0x00, 0x02, 0x00, 0x02, 0x00];
pub const NOTIFICATION_REQUEST: &[u8] = &[
    0xef, 0xdd, 0x0c, 0x09, 0x00, 0x01, 0x01, 0x02, 0x02, 0x05, 0x03, 0x04, 0x15, 0x06,
];
const TARE_ACAIA: &[u8] = &[0xef, 0xdd, 0x04, 0x00, 0x00, 0x00];
const TARE_GENERIC: &[u8] = &[0x03, 0x0a, 0x01, 0x00, 0x00, 0x08];
const TARE_WEIGHMYBRU: &[u8] = &[0x03, 0x0a, 0x01, 0x01];

/// Advertised names that look like a supported scale.
pub fn is_supported_name(name: &str) -> bool {
    const PREFIXES: [&str; 12] = [
        "ACAIA",
        "LUNAR",
        "PYXIS",
        "PEARL",
        "CINCO",
        "PROCH",
        "BOOKOO",
        "DECENT",
        "ESPRESSISCALE",
        "WEIGHMYBRU",
        "FELICITA",
        "ARC",
    ];
    let n = name.trim();
    PREFIXES
        .iter()
        .any(|p| n.len() >= p.len() && n[..p.len()].eq_ignore_ascii_case(p))
}

/// The tare command. Decent scales count their tares; pass and keep the counter.
pub fn tare(kind: Kind, decent_counter: &mut u8, out: &mut [u8; 7]) -> usize {
    let src: &[u8] = match kind {
        Kind::Decent => {
            *decent_counter = decent_counter.wrapping_add(1);
            *out = [0x03, 0x0f, *decent_counter, 0, 0, 0, 0];
            out[6] = out[..6].iter().fold(0, |a, b| a ^ b);
            return 7;
        }
        Kind::Generic | Kind::Bookoo => TARE_GENERIC,
        Kind::WeighMyBru => TARE_WEIGHMYBRU,
        Kind::AcaiaOld | Kind::AcaiaNew => TARE_ACAIA,
    };
    out[..src.len()].copy_from_slice(src);
    src.len()
}

/// Reads the weight in grams from a notification, if it carries one.
pub fn parse_weight(kind: Kind, p: &[u8]) -> Option<f32> {
    let sign = |neg: bool, v: f32| if neg { -v } else { v };
    let be24 = |p: &[u8]| (u32::from(p[7]) << 16) | (u32::from(p[8]) << 8) | u32::from(p[9]);
    match kind {
        Kind::AcaiaOld | Kind::AcaiaNew if (p.len() == 13 || p.len() == 17) && p[4] == 0x05 => {
            if kind == Kind::AcaiaOld && p.len() != 13 {
                return None;
            }
            let raw = f32::from(u16::from_le_bytes([p[5], p[6]]));
            Some(sign(p[10] & 0x02 != 0, raw / pow10(p[9])))
        }
        Kind::AcaiaOld if p.len() == 10 || p.len() == 14 => {
            let raw = f32::from(u16::from_le_bytes([p[2], p[3]]));
            Some(sign(p[7] & 0x02 != 0, raw / pow10(p[6])))
        }
        Kind::Generic if p.len() == 20 => Some(sign(p[6] == b'-', be24(p) as f32 / 100.0)),
        Kind::Bookoo if p.len() == 20 && p[0] == 0x03 && p[1] == 0x0b => {
            Some(sign(p[6] == b'-', be24(p) as f32 / 100.0))
        }
        Kind::WeighMyBru if p.len() == 20 && p[0] == 0x03 && p[1] == 0x0b => {
            let x = p[..19].iter().fold(0, |a, b| a ^ b);
            (x == p[19]).then(|| sign(p[6] == b'-', be24(p) as f32 / 100.0))
        }
        Kind::Decent if p.len() == 7 && p[0] == 0x03 => {
            let x = p[..6].iter().fold(0, |a, b| a ^ b);
            (x == p[6] && (p[1] == 0xce || p[1] == 0xca))
                .then(|| f32::from(i16::from_be_bytes([p[2], p[3]])) * 0.1)
        }
        _ => None,
    }
}

fn pow10(e: u8) -> f32 {
    let mut v = 1.0;
    for _ in 0..e.min(6) {
        v *= 10.0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(is_supported_name("LUNAR-0123"));
        assert!(is_supported_name(" pyxis"));
        assert!(is_supported_name("BOOKOO_SC"));
        assert!(!is_supported_name("Fitbit"));
    }

    #[test]
    fn acaia_new_weight() {
        // 18.4 g: raw 184, scale exponent 1.
        let mut p = [0u8; 13];
        p[4] = 0x05;
        p[5] = 184;
        p[9] = 1;
        assert_eq!(parse_weight(Kind::AcaiaNew, &p), Some(18.4));
        p[10] = 0x02;
        assert_eq!(parse_weight(Kind::AcaiaNew, &p), Some(-18.4));
    }

    #[test]
    fn generic_and_bookoo_weight() {
        let mut p = [0u8; 20];
        p[0] = 0x03;
        p[1] = 0x0b;
        p[6] = b'+';
        p[8] = 0x0e; // 0x0e10 = 3600 -> 36.00 g
        p[9] = 0x10;
        assert_eq!(parse_weight(Kind::Generic, &p), Some(36.0));
        assert_eq!(parse_weight(Kind::Bookoo, &p), Some(36.0));
        p[6] = b'-';
        assert_eq!(parse_weight(Kind::Bookoo, &p), Some(-36.0));
    }

    #[test]
    fn weighmybru_checks_checksum() {
        let mut p = [0u8; 20];
        p[0] = 0x03;
        p[1] = 0x0b;
        p[9] = 100;
        p[19] = p[..19].iter().fold(0, |a, b| a ^ b);
        assert_eq!(parse_weight(Kind::WeighMyBru, &p), Some(1.0));
        p[19] ^= 1;
        assert_eq!(parse_weight(Kind::WeighMyBru, &p), None);
    }

    #[test]
    fn decent_weight_and_tare() {
        let mut p = [0x03, 0xce, 0x01, 0x68, 0, 0, 0]; // 360 -> 36.0 g
        p[6] = p[..6].iter().fold(0, |a, b| a ^ b);
        assert_eq!(parse_weight(Kind::Decent, &p), Some(36.0));
        let mut c = 0;
        let mut out = [0; 7];
        assert_eq!(tare(Kind::Decent, &mut c, &mut out), 7);
        assert_eq!(out, [0x03, 0x0f, 1, 0, 0, 0, 0x03 ^ 0x0f ^ 1]);
        assert_eq!(tare(Kind::AcaiaNew, &mut c, &mut out), 6);
        assert_eq!(&out[..6], TARE_ACAIA);
    }
}
