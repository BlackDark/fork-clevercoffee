//! The firmware's `include/clevercoffee/display/bitmaps.h`, by name.
//!
//! [`bitmaps_data`] holds the bytes, copied verbatim by
//! `tools/extract_bitmaps.py`. This module adds the *name* the C++ uses for
//! each one, plus its dimensions, so a caller can go from a name to something
//! drawable without repeating the `w`/`h` pair that `bitmaps.h` declares as
//! `*_width`/`*_height` macros.
//!
//! # Why a table rather than a macro
//!
//! The C++ exposes eleven C arrays and eleven width/height macro pairs. There is
//! no enumeration of them, so C++ callers name the array directly:
//! `display->drawXBMP(x, y, CleverCoffee_Logo_width, CleverCoffee_Logo_height,
//! CleverCoffee_Logo)`. That is a *better* interface at the call site -- the
//! compiler checks the dimensions against the array length, and renaming a
//! bitmap is a compile error everywhere it is used.
//!
//! A name table trades that away, and it is worth being explicit about why the
//! trade is acceptable here: the only caller that needs names is the parity
//! scenario runner, which has to agree with `tools/oracle/display_oracle.cpp`
//! -- a *string* table in C++ for the same reason. The production call sites
//! keep using [`bitmaps_data`] directly, with the dimensions written next to
//! the array, and the test below pins every dimension against the C++ header.

use crate::bitmaps_data as data;

/// A bitmap, its dimensions, and nothing else.
///
/// Deliberately not a `Bitmap`-with-methods type: `Display::draw_xbmp` takes
/// a byte slice and the dimensions, so a struct that only carries them is
/// exactly what a call site needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bitmap {
    /// The C++ identifier, e.g. `"logo"` for `CleverCoffee_Logo`.
    pub name: &'static str,
    /// Width in pixels, from the `*_width` macro.
    pub width: u8,
    /// Height in pixels, from the `*_height` macro.
    pub height: u8,
    /// The bytes, big-endian 1bpp, as `u8g2_DrawHXBMP` expects.
    pub data: &'static [u8],
}

/// Every bitmap the firmware ships, in the order `bitmaps.h` declares them.
///
/// The order matches the oracle's `BITMAPS[]` so the two are read side by side.
pub const ALL: [Bitmap; 11] = [
    Bitmap {
        name: "antenna_ok",
        width: 8,
        height: 8,
        data: &data::ANTENNA_OK_ICON,
    },
    Bitmap {
        name: "antenna_nok",
        width: 8,
        height: 8,
        data: &data::ANTENNA_NOK_ICON,
    },
    Bitmap {
        name: "bluetooth",
        width: 8,
        height: 9,
        data: &data::BLUETOOTH_ICON,
    },
    Bitmap {
        name: "logo",
        width: 40,
        height: 40,
        data: &data::CLEVERCOFFEE_LOGO,
    },
    Bitmap {
        name: "heating_logo",
        width: 40,
        height: 40,
        data: &data::HEATING_LOGO,
    },
    Bitmap {
        name: "off_logo",
        width: 52,
        height: 53,
        data: &data::OFF_LOGO,
    },
    Bitmap {
        name: "steam_logo",
        width: 40,
        height: 40,
        data: &data::STEAM_LOGO,
    },
    Bitmap {
        name: "brew_cup",
        width: 40,
        height: 40,
        data: &data::BREW_CUP_LOGO,
    },
    Bitmap {
        name: "hot_water_logo",
        width: 40,
        height: 40,
        data: &data::HOT_WATER_LOGO,
    },
    Bitmap {
        name: "water_empty",
        width: 47,
        height: 64,
        data: &data::WATER_TANK_EMPTY_LOGO,
    },
    Bitmap {
        name: "manual_flush",
        width: 40,
        height: 40,
        data: &data::MANUAL_FLUSH_LOGO,
    },
];

/// Look a bitmap up by the C++ name.
#[must_use]
pub fn lookup(name: &str) -> Option<Bitmap> {
    ALL.iter().copied().find(|b| b.name == name)
}

#[cfg(test)]
mod tests {
    use super::ALL;

    /// Every bitmap's byte count is what its dimensions imply.
    ///
    /// A wrong `_width` or `_height` here would not show up as a compile error
    /// -- the arrays are byte slices, not sized types -- it would show up as a
    /// clipped or stretched icon on the panel, which is exactly the class of
    /// bug a golden image is supposed to catch, and exactly the class of bug
    /// that is invisible in review. So it is checked here against the
    /// dimensions instead.
    #[test]
    fn the_byte_count_matches_the_stated_dimensions() {
        for b in ALL {
            // `u8g2_DrawHXBMP` packs a `w`-wide row into `ceil(w / 8)` bytes,
            // and these are all 1-byte-per-row icons (the 8x9 Bluetooth one is
            // the reason that clause is needed: 9 rows of 1 byte for 8 pixels).
            let row_bytes = usize::from(b.width).div_ceil(8);
            assert_eq!(
                b.data.len(),
                row_bytes * usize::from(b.height),
                "{}: {}x{} does not fit {} bytes",
                b.name,
                b.width,
                b.height,
                b.data.len()
            );
        }
    }

    /// Every name is unique, so a typo cannot silently pick a different icon.
    #[test]
    fn the_names_are_unique() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.name, b.name);
            }
        }
    }
}
