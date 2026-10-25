//! Compare glyph metrics with frozen U8g2 measurements before trusting goldens.
//!
//! Changed fonts require remeasurement; the original probe is not in this tree.

mod support;

use cc_display::font::{self, Font};
use support::glyph_widths::{GLYPH_WIDTHS, PRINTABLE};
use support::string_widths;

/// The ten embedded fonts, in the order the measurement table uses.
fn all() -> [Font; 10] {
    [
        font::profont10(),
        font::profont11(),
        font::profont12(),
        font::profont15(),
        font::profont17(),
        font::profont22(),
        font::fub17(),
        font::fub20(),
        font::fub25(),
        font::fub30(),
    ]
}

#[test]
fn every_glyph_advance_matches_u8g2() {
    let fonts = all();
    let mut failures = Vec::new();

    for (font, (name, expected)) in fonts.iter().zip(GLYPH_WIDTHS) {
        for (c, want) in PRINTABLE.iter().zip(expected) {
            let got = font.str_width(&c.to_string());
            if got != want {
                failures.push(format!(
                    "{name} U+{:04X} '{c}': got {got}, u8g2 {want}",
                    u32::from(*c)
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} glyph(s) differ from U8g2:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn multi_glyph_string_widths_match_u8g2() {
    // This is the test that covers the two width-loop quirks. A per-glyph table
    // cannot: the last-glyph ink substitution and the balanced first-glyph
    // x-offset both only appear once a string has more than one glyph, and
    // both change where a right-aligned or centred label lands.
    //
    // The repeated-glyph rows matter most. "iiiiiiiiii" and "!!!!!!!!!!" are
    // all-inset glyphs, so a build without the balanced term is
    // (glyphs x x_offset) pixels short on them -- a systematic rightward drift
    // on exactly the narrow labels the display leans on.
    let fonts = all();
    let mut failures = Vec::new();

    for (i, font) in fonts.iter().enumerate() {
        let name = string_widths::FONTS[i];
        for (text, want) in string_widths::STRINGS.iter().zip(string_widths::WIDTHS[i]) {
            let &want = &want;
            let got = font.str_width(text);
            if got != want {
                failures.push(format!("{name} {text:?}: got {got}, u8g2 {want}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} string(s) differ from U8g2:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_utf8_width_entry_point_agrees_on_latin1_text() {
    // The firmware's strings are ASCII plus one Latin-1 byte, so U8g2's two
    // decoders agree on every real input. A caller that measures with one and
    // draws with the other would still disagree the moment that changes.
    let fonts = all();
    let mut failures = Vec::new();

    for (i, font) in fonts.iter().enumerate() {
        let name = string_widths::FONTS[i];
        for (text, want) in string_widths::STRINGS
            .iter()
            .zip(string_widths::UTF8_WIDTHS[i])
        {
            let &want = &want;
            let got = font.str_width_utf8(text);
            if got != want {
                failures.push(format!("{name} {text:?}: got {got}, u8g2 {want}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} string(s) differ from U8g2:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
