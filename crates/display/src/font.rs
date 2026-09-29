//! Font metrics and glyphs.
//!
//! The C++ firmware drew with U8G2 fonts (`u8g2_font_fub20_tf`, `u8g2_font_profont11_tf` and
//! friends) whose bitmaps and advance widths live in the U8G2 library, which PlatformIO fetched
//! and which is not in this repository. Metrics therefore cannot be *read* from the C++ tree, and
//! the task list records that as the single largest unknown in the display port.
//!
//! What is here instead is one 5x7 glyph table with two nominal sizes, small (1:1) and large
//! (2:1), and the metrics derived from it. Two consequences the tests pin down:
//!
//! - Every width in this crate is `advance * scale`, computed by [`str_width`], never a constant
//!   copied from a font name. A layout that assumes a proportional font cannot be written here.
//! - Every Y coordinate is the **top of the glyph box**, the `setFontPosTop()` convention the C++
//!   templates used, so a row's occupied band is `y .. y + height(scale)` and two rows overlap if
//!   and only if their bands intersect.

/// Glyph box width in unscaled pixels.
pub const GLYPH_W: u8 = 5;
/// Glyph box height in unscaled pixels. The table uses bits 0..=6 of each column, so seven rows.
pub const GLYPH_H: u8 = 7;
/// Horizontal advance in unscaled pixels: the glyph box plus one column of spacing.
pub const ADVANCE: u8 = 6;
/// The first character in the table.
pub const FIRST_CHAR: char = ' ';
/// The last character in the table.
pub const LAST_CHAR: char = '~';
/// The degree sign, which is not ASCII and so is handled outside the table.
pub const DEGREE: char = '\u{b0}';

/// A glyph size. `scale` is an integer magnification, because a 5x7 cell magnified by an
/// integer is still pixel-aligned and still has computable metrics.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Font {
    /// 5x7, the equivalent of the C++ `profont10`/`profont11` rows.
    Small,
    /// 10x14, the equivalent of the C++ `fub20` large temperature.
    Large,
}

impl Font {
    /// The integer magnification.
    pub const fn scale(self) -> u8 {
        match self {
            Font::Small => 1,
            Font::Large => 2,
        }
    }

    /// Height of the glyph box, which is the row height of a text row at this size.
    pub const fn height(self) -> u8 {
        GLYPH_H * self.scale()
    }

    /// Width of one advance cell, spacing included.
    pub const fn cell_width(self) -> u8 {
        ADVANCE * self.scale()
    }

    /// Width of the widest probe string this crate lays out in, in pixels.
    ///
    /// Templates reserve a box of this width and right-align the live value inside it, so a value
    /// changing from `9` to `10` cannot shift the digits to its left.
    pub const fn box_width(self, chars: usize) -> i16 {
        self.cell_width() as i16 * chars as i16
    }
}

/// The five column bytes of one glyph, LSB = top row.
type Glyph = [u8; 5];

const DEGREE_GLYPH: Glyph = [0x02, 0x05, 0x05, 0x02, 0x00];

/// ASCII 0x20 to 0x7E, five columns per glyph, column-major, bit 0 is the top pixel.
///
/// This is the classic 5x7 cell font. It is reproduced here rather than pulled from a font crate
/// because the firmware must not grow a dependency for 475 bytes of data, and because the tests
/// need the metrics to be a *known input* rather than whatever a library version ships.
#[rustfmt::skip]
const GLYPHS: [Glyph; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00], // ' '
    [0x00, 0x00, 0x5F, 0x00, 0x00], // '!'
    [0x00, 0x07, 0x00, 0x07, 0x00], // '"'
    [0x14, 0x7F, 0x14, 0x7F, 0x14], // '#'
    [0x24, 0x2A, 0x7F, 0x2A, 0x12], // '$'
    [0x23, 0x13, 0x08, 0x64, 0x62], // '%'
    [0x36, 0x49, 0x55, 0x22, 0x50], // '&'
    [0x00, 0x05, 0x03, 0x00, 0x00], // '\''
    [0x00, 0x1C, 0x22, 0x41, 0x00], // '('
    [0x00, 0x41, 0x22, 0x1C, 0x00], // ')'
    [0x14, 0x08, 0x3E, 0x08, 0x14], // '*'
    [0x08, 0x08, 0x3E, 0x08, 0x08], // '+'
    [0x00, 0x50, 0x30, 0x00, 0x00], // ','
    [0x08, 0x08, 0x08, 0x08, 0x08], // '-'
    [0x00, 0x60, 0x60, 0x00, 0x00], // '.'
    [0x20, 0x10, 0x08, 0x04, 0x02], // '/'
    [0x3E, 0x51, 0x49, 0x45, 0x3E], // '0'
    [0x00, 0x42, 0x7F, 0x40, 0x00], // '1'
    [0x42, 0x61, 0x51, 0x49, 0x46], // '2'
    [0x21, 0x41, 0x45, 0x4B, 0x31], // '3'
    [0x18, 0x14, 0x12, 0x7F, 0x10], // '4'
    [0x27, 0x45, 0x45, 0x45, 0x39], // '5'
    [0x3C, 0x4A, 0x49, 0x49, 0x30], // '6'
    [0x01, 0x71, 0x09, 0x05, 0x03], // '7'
    [0x36, 0x49, 0x49, 0x49, 0x36], // '8'
    [0x06, 0x49, 0x49, 0x29, 0x1E], // '9'
    [0x00, 0x36, 0x36, 0x00, 0x00], // ':'
    [0x00, 0x56, 0x36, 0x00, 0x00], // ';'
    [0x08, 0x14, 0x22, 0x41, 0x00], // '<'
    [0x14, 0x14, 0x14, 0x14, 0x14], // '='
    [0x00, 0x41, 0x22, 0x14, 0x08], // '>'
    [0x02, 0x01, 0x51, 0x09, 0x06], // '?'
    [0x32, 0x49, 0x79, 0x41, 0x3E], // '@'
    [0x7E, 0x11, 0x11, 0x11, 0x7E], // 'A'
    [0x7F, 0x49, 0x49, 0x49, 0x36], // 'B'
    [0x3E, 0x41, 0x41, 0x41, 0x22], // 'C'
    [0x7F, 0x41, 0x41, 0x22, 0x1C], // 'D'
    [0x7F, 0x49, 0x49, 0x49, 0x41], // 'E'
    [0x7F, 0x09, 0x09, 0x09, 0x01], // 'F'
    [0x3E, 0x41, 0x49, 0x49, 0x7A], // 'G'
    [0x7F, 0x08, 0x08, 0x08, 0x7F], // 'H'
    [0x00, 0x41, 0x7F, 0x41, 0x00], // 'I'
    [0x20, 0x40, 0x41, 0x3F, 0x01], // 'J'
    [0x7F, 0x08, 0x14, 0x22, 0x41], // 'K'
    [0x7F, 0x40, 0x40, 0x40, 0x40], // 'L'
    [0x7F, 0x02, 0x0C, 0x02, 0x7F], // 'M'
    [0x7F, 0x04, 0x08, 0x10, 0x7F], // 'N'
    [0x3E, 0x41, 0x41, 0x41, 0x3E], // 'O'
    [0x7F, 0x09, 0x09, 0x09, 0x06], // 'P'
    [0x3E, 0x41, 0x51, 0x21, 0x5E], // 'Q'
    [0x7F, 0x09, 0x19, 0x29, 0x46], // 'R'
    [0x46, 0x49, 0x49, 0x49, 0x31], // 'S'
    [0x01, 0x01, 0x7F, 0x01, 0x01], // 'T'
    [0x3F, 0x40, 0x40, 0x40, 0x3F], // 'U'
    [0x1F, 0x20, 0x40, 0x20, 0x1F], // 'V'
    [0x3F, 0x40, 0x38, 0x40, 0x3F], // 'W'
    [0x63, 0x14, 0x08, 0x14, 0x63], // 'X'
    [0x07, 0x08, 0x70, 0x08, 0x07], // 'Y'
    [0x61, 0x51, 0x49, 0x45, 0x43], // 'Z'
    [0x00, 0x7F, 0x41, 0x41, 0x00], // '['
    [0x02, 0x04, 0x08, 0x10, 0x20], // '\\'
    [0x00, 0x41, 0x41, 0x7F, 0x00], // ']'
    [0x04, 0x02, 0x01, 0x02, 0x04], // '^'
    [0x40, 0x40, 0x40, 0x40, 0x40], // '_'
    [0x00, 0x01, 0x02, 0x04, 0x00], // '`'
    [0x20, 0x54, 0x54, 0x54, 0x78], // 'a'
    [0x7F, 0x48, 0x44, 0x44, 0x38], // 'b'
    [0x38, 0x44, 0x44, 0x44, 0x20], // 'c'
    [0x38, 0x44, 0x44, 0x48, 0x7F], // 'd'
    [0x38, 0x54, 0x54, 0x54, 0x18], // 'e'
    [0x08, 0x7E, 0x09, 0x01, 0x02], // 'f'
    [0x0C, 0x52, 0x52, 0x52, 0x3E], // 'g'
    [0x7F, 0x08, 0x04, 0x04, 0x78], // 'h'
    [0x00, 0x44, 0x7D, 0x40, 0x00], // 'i'
    [0x20, 0x40, 0x44, 0x3D, 0x00], // 'j'
    [0x7F, 0x10, 0x28, 0x44, 0x00], // 'k'
    [0x00, 0x41, 0x7F, 0x41, 0x00], // 'l'
    [0x7C, 0x04, 0x18, 0x04, 0x78], // 'm'
    [0x7C, 0x08, 0x04, 0x04, 0x78], // 'n'
    [0x38, 0x44, 0x44, 0x44, 0x38], // 'o'
    [0x7C, 0x14, 0x14, 0x14, 0x08], // 'p'
    [0x08, 0x14, 0x14, 0x18, 0x7C], // 'q'
    [0x7C, 0x08, 0x04, 0x04, 0x08], // 'r'
    [0x48, 0x54, 0x54, 0x54, 0x20], // 's'
    [0x04, 0x3F, 0x44, 0x40, 0x20], // 't'
    [0x3C, 0x40, 0x40, 0x20, 0x7C], // 'u'
    [0x1C, 0x20, 0x40, 0x20, 0x1C], // 'v'
    [0x3C, 0x40, 0x30, 0x40, 0x3C], // 'w'
    [0x44, 0x28, 0x10, 0x28, 0x44], // 'x'
    [0x0C, 0x50, 0x50, 0x50, 0x3C], // 'y'
    [0x44, 0x64, 0x54, 0x4C, 0x44], // 'z'
    [0x00, 0x08, 0x36, 0x41, 0x00], // '{'
    [0x00, 0x00, 0x7F, 0x00, 0x00], // '|'
    [0x00, 0x41, 0x36, 0x08, 0x00], // '}'
    [0x08, 0x08, 0x2A, 0x1C, 0x08], // '~'
];

/// The columns of one character, or `None` if it is outside the table.
pub fn glyph(c: char) -> Option<Glyph> {
    if c == DEGREE {
        return Some(DEGREE_GLYPH);
    }
    let first = FIRST_CHAR as u32;
    let code = c as u32;
    if !(first..=LAST_CHAR as u32).contains(&code) {
        return None;
    }
    Some(GLYPHS[(code - first) as usize])
}

/// The width of a run of `chars` characters, for a caller that wants a `const`.
///
/// Split out from [`str_width`] because counting a `&str`'s characters is not a `const` operation,
/// and several layout constants want to be `const`.
pub const fn probe_width(chars: usize, font: Font) -> i16 {
    font.cell_width() as i16 * chars as i16
}

/// The width of `text` in pixels, which is `chars * cell_width`.
///
/// Monospaced by construction. A caller that wants a narrower string for a narrow box must
/// choose a shorter string, not a narrower font, because there is only one table.
pub fn str_width(text: &str, font: Font) -> i16 {
    probe_width(text.chars().count(), font)
}

/// Whether `text` fits in `box_w` at `font`.
pub fn fits(text: &str, font: Font, box_w: i16) -> bool {
    str_width(text, font) <= box_w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_printable_ascii_with_no_holes() {
        for code in 0x20u8..=0x7E {
            let c = code as char;
            assert!(glyph(c).is_some(), "missing glyph for {c:?}");
        }
        assert!(glyph('\n').is_none(), "a control character has no glyph");
        assert!(glyph(DEGREE).is_some(), "the degree sign is handled");
    }

    #[test]
    fn a_space_advances_but_draws_nothing() {
        let g = glyph(' ').unwrap();
        assert_eq!(g, [0, 0, 0, 0, 0]);
        assert_eq!(str_width(" ", Font::Small), Font::Small.cell_width() as i16);
    }

    #[test]
    fn a_glyph_fits_inside_its_own_cell() {
        // A column byte with bit 7 set would draw outside the seven-row box and break every row
        // band the layout tests compute.
        for code in 0x20u8..=0x7E {
            let g = glyph(code as char).unwrap();
            for (col, bits) in g.iter().enumerate() {
                assert_eq!(
                    bits & 0x80,
                    0,
                    "glyph {:?} column {col} uses a bit below the glyph box",
                    code as char
                );
            }
        }
    }

    #[test]
    fn the_metrics_are_exactly_what_the_row_map_assumes() {
        assert_eq!(Font::Small.height(), 7);
        assert_eq!(Font::Large.height(), 14);
        assert_eq!(Font::Small.cell_width(), 6);
        assert_eq!(Font::Large.cell_width(), 12);
        assert_eq!(str_width("100.0", Font::Small), 30);
        assert_eq!(str_width("100.0", Font::Large), 60);
    }

    #[test]
    fn a_wider_string_is_never_narrower() {
        let mut prev = 0;
        for n in 0..8 {
            let w = str_width(&"x".repeat(n), Font::Small);
            assert!(w >= prev, "width must be monotonic in length");
            prev = w;
        }
    }

    #[test]
    fn fits_is_the_inverse_of_the_width_comparison() {
        assert!(fits("99.9", Font::Small, 24));
        assert!(!fits("100.0", Font::Small, 24));
        assert!(fits("100.0", Font::Small, 30));
    }
}
