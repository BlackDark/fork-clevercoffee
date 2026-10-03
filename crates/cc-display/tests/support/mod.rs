//! Golden-image and PPM support. Host-only (`std`).
//!
//! `cc-display` itself is `no_std` with no `alloc`, so nothing that needs a
//! heap lives in `src/`. The PPM writer and the golden comparator are here
//! instead, and are pulled in by `tests/` only.
//!
//! # Why P4 and not P6
//!
//! Binary P4 packs eight pixels per byte, MSB first, **1 = white**. That is the
//! *transpose* of the framebuffer's own page layout and of the SSD1306 wire
//! format, and deliberately so: a golden image should be in the orientation a
//! human or an image-diff tool sees, not the orientation the panel is clocked
//! in. The 1 = white polarity is because PPM has no concept of an inverted
//! display, where a lit OLED pixel is a *zero* bit.

// Every integration test binary is its own crate, and each uses a different
// subset of this module: the PPM/golden helpers are only reached once the golden
// tests exist, the measurement tables only by the parity tests. Without this,
// every binary reports the other's helpers as dead code.
#![allow(
    dead_code,
    reason = "shared by several test binaries; each uses a subset"
)]

pub mod glyph_widths;
pub mod string_widths;

use std::path::Path;

use cc_display::display::{Framebuffer, DISPLAY_HEIGHT, DISPLAY_WIDTH};

/// Packed bytes per image row: 128 pixels, 8 per byte.
pub const BYTES_PER_ROW: usize = 16;

/// The P4 header for a 128x64 image. Fixed width, so the offset of the first
/// pixel byte is a constant.
pub const PPM_HEADER: &[u8] = b"P4\n128 64\n";

/// Packed pixel bytes: [`BYTES_PER_ROW`] for each of the 64 rows.
pub const PIXEL_BYTES: usize = BYTES_PER_ROW * 64;

/// Total bytes a P4 image of the panel occupies, header included.
///
/// Derived from [`PPM_HEADER`] rather than carrying a second copy of the header
/// length, because the two drifting apart is a silent truncation on read.
pub const PPM_LEN: usize = PPM_HEADER.len() + PIXEL_BYTES;

/// Read a P4 PPM back into a framebuffer.
///
/// The oracle writes one, so this is how its output becomes comparable.
///
/// # Panics
///
/// If the file is missing or is not a well-formed 128x64 P4.
#[must_use]
pub fn read_ppm(path: &Path) -> Framebuffer {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    assert!(
        bytes.starts_with(PPM_HEADER),
        "{} is not a 128x64 P4 PPM",
        path.display()
    );
    let data = &bytes[PPM_HEADER.len()..];
    assert_eq!(
        data.len(),
        PIXEL_BYTES,
        "{} has the wrong length: {} pixel bytes, expected {PIXEL_BYTES}",
        path.display(),
        data.len()
    );
    let mut fb = Framebuffer::new();
    for y in 0..DISPLAY_HEIGHT {
        let row = usize::try_from(y).unwrap_or(0) * BYTES_PER_ROW;
        for x in 0..DISPLAY_WIDTH {
            let xu = usize::try_from(x).unwrap_or(0);
            if data[row + xu / 8] & (0x80 >> (xu % 8)) != 0 {
                fb.set_pixel(x, y);
            }
        }
    }
    fb
}

/// Render a framebuffer as a P4 PPM.
///
/// # Panics
///
/// Never; the output length is always [`PPM_LEN`].
#[must_use]
pub fn to_ppm(fb: &Framebuffer) -> Vec<u8> {
    let mut out = Vec::with_capacity(PPM_LEN);
    out.extend_from_slice(PPM_HEADER);
    for y in 0..DISPLAY_HEIGHT {
        let mut byte = 0u8;
        let mut bit = 7i32;
        for x in 0..DISPLAY_WIDTH {
            if fb.pixel(x, y) {
                byte |= 1 << bit;
            }
            bit -= 1;
            if bit < 0 {
                out.push(byte);
                byte = 0;
                bit = 7;
            }
        }
    }
    out
}

/// How two images differ, at the pixel level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diff {
    /// Pixels set in the golden but not in the actual.
    pub only_in_golden: u32,
    /// Pixels set in the actual but not in the golden.
    pub only_in_actual: u32,
    /// Pixels set in both.
    pub common: u32,
    /// `0.0 ..= 1.0`. Symmetric difference over the union, so a completely
    /// black-vs-black pair scores 0 rather than dividing by zero.
    /// Fraction of differing pixels, 0.0..=1.0. `f64` because the report
    /// divides counts and a fraction of a pixel is meaningful.
    pub rate: f64,
    /// The bounding box of the differing pixels, inclusive. `None` if identical.
    pub bounds: Option<(i32, i32, i32, i32)>,
}

impl Diff {
    /// Total pixels that differ in either direction.
    #[must_use]
    pub const fn differing(&self) -> u32 {
        self.only_in_golden + self.only_in_actual
    }

    /// A one-line summary for a test failure message.
    #[must_use]
    pub fn summary(&self) -> String {
        let bounds = match self.bounds {
            Some((x0, y0, x1, y1)) => format!("bounds=({x0},{y0})-({x1},{y1})"),
            None => String::from("bounds=none"),
        };
        format!(
            "diff rate {:.4}% ({}/{} px differ; golden-only {}, actual-only {}; {bounds})",
            self.rate * 100.0,
            self.differing(),
            self.common + self.differing(),
            self.only_in_golden,
            self.only_in_actual,
        )
    }
}

/// Compare two framebuffers pixel by pixel.
#[must_use]
pub fn diff(golden: &Framebuffer, actual: &Framebuffer) -> Diff {
    let mut only_in_golden = 0;
    let mut only_in_actual = 0;
    let mut common = 0;
    let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
    let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);

    for y in 0..DISPLAY_HEIGHT {
        for x in 0..DISPLAY_WIDTH {
            let g = golden.pixel(x, y);
            let a = actual.pixel(x, y);
            match (g, a) {
                (true, true) => common += 1,
                (true, false) | (false, true) => {
                    if g {
                        only_in_golden += 1;
                    } else {
                        only_in_actual += 1;
                    }
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
                (false, false) => {}
            }
        }
    }

    // u32 -> f64 is lossless for every value a 128x64 framebuffer can produce
    // (the union is at most 8192), so there is nothing to lose here.
    let union = f64::from(common + only_in_golden + only_in_actual);
    let rate = if union == 0.0 {
        0.0
    } else {
        f64::from(only_in_golden + only_in_actual) / union
    };
    let bounds = if min_x > max_x {
        None
    } else {
        Some((min_x, min_y, max_x, max_y))
    };
    Diff {
        only_in_golden,
        only_in_actual,
        common,
        rate,
        bounds,
    }
}

/// Read a golden PPM back into a framebuffer.
///
/// P4 only; there is nothing else in `tests/golden/`.
///
/// # Panics
///
/// If the file is missing or is not a well-formed 128x64 P4.
#[must_use]
pub fn read_golden(path: &Path) -> Framebuffer {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("cannot read golden {}: {e}", path.display()));
    assert!(
        bytes.starts_with(PPM_HEADER),
        "golden {} is not a 128x64 P4 PPM",
        path.display()
    );
    let mut fb = Framebuffer::new();
    let data = &bytes[PPM_HEADER.len()..];
    assert_eq!(
        data.len(),
        PIXEL_BYTES,
        "golden {} has the wrong length",
        path.display()
    );
    for y in 0..DISPLAY_HEIGHT {
        let row = usize::try_from(y).unwrap_or(0) * BYTES_PER_ROW;
        for x in 0..DISPLAY_WIDTH {
            let xu = usize::try_from(x).unwrap_or(0);
            if data[row + xu / 8] & (0x80 >> (xu % 8)) != 0 {
                fb.set_pixel(x, y);
            }
        }
    }
    fb
}
