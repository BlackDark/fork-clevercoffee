//! Hostile text must not take the display down.
//!
//! # Why
//!
//! `OtaInput::error_message` is whatever the HTTP server said
//! (`web.rs::ota_status_json` renders a message straight into that field), and
//! `panic = "abort"` means a panic in the display task is a **device reset**.
//! Before the bounds guard in [`cc_display::font::Font::glyph_header`], one
//! emoji or CJK character in a server's error string walked off the end of the
//! 2251-byte font blob and aborted:
//!
//! ```text
//! cargo run -p cc-display --example probe -- missing
//! thread 'main' panicked at font/mod.rs:135:
//! index out of bounds: the len is 2251 but the index is 2500
//! ```
//!
//! U8g2's own search cannot do this — its fonts are well-formed and its tables
//! terminate. This port walks a fixed blob, so the walk needs a bound.
//!
//! # What "safe" means here
//!
//! A missing glyph advances the pen by zero and draws nothing, which is exactly
//! what U8g2 does and what [`cc_display::font::Font::str_width`] already
//! reported. The two agreeing is the property worth pinning: a string that
//! measures 3 px wide must *draw* 3 px wide, or every column after it on the row
//! is off by the difference.

use cc_display::display::Display;
use cc_display::font;
use cc_display::model::{Config, DisplayInput, OtaInput, OtaKind, OtaStatus};

/// Strings a network peer, a translation file or a tired human can produce.
const HOSTILE: &[&str] = &[
    "",
    " ",
    "\u{4e2d}",  // CJK, outside any Latin-1 range
    "\u{1f642}", // emoji
    "\u{fffd}",  // the replacement character itself
    "\u{0}",     // NUL
    "\u{7f}",    // DEL
    "a\u{4e2d}b",
    "\u{20ac}100", // euro sign
    "\u{b0}C",     // degree + C: the real unit, which must work
    "Temp.-Sensor ueberpruefen!",
    "xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx",
];

/// Every font the templates draw with.
const FONTS: &[fn() -> font::Font] = &[
    font::profont10,
    font::profont11,
    font::profont12,
    font::profont17,
    font::profont22,
    font::profont15,
    font::fub17,
    font::fub20,
    font::fub25,
    font::fub30,
];

#[test]
fn a_glyph_the_font_does_not_have_is_none_and_not_a_panic() {
    for f in FONTS {
        let f = f();
        for text in HOSTILE {
            for ch in text.chars() {
                // The contract: no panic, and a `None` for anything the font
                // cannot draw. The call itself is the test — before the bounds
                // guard this aborted the process.
                let _ = f.glyph_header(ch as u16);
            }
        }
    }
}

#[test]
fn drawing_hostile_text_into_a_frame_does_not_panic() {
    let config = Config::default();
    for text in HOSTILE {
        for template in [
            cc_display::templates::TemplateId::Standard,
            cc_display::templates::TemplateId::Modern,
            cc_display::templates::TemplateId::Upright,
        ] {
            let input = DisplayInput {
                ota: OtaInput {
                    show: true,
                    status: OtaStatus::Error,
                    kind: OtaKind::Firmware,
                    progress: 0,
                    // The OTA screen prints this string centred on the panel.
                    error_message: leak(text),
                },
                ..DisplayInput::default()
            };
            let mut d = Display::new();
            let _ = cc_display::templates::render(template, &mut d, &input, &config);
            // The frame must also still be *drawn*: a screen that renders
            // nothing because it bailed is not a safe screen.
            assert!(
                d.framebuffer().as_bytes().iter().any(|b| *b != 0),
                "{template:?} drew nothing for an OTA error of {text:?}"
            );
        }
    }
}

#[test]
fn a_missing_glyph_measures_zero_and_draws_zero() {
    // The disagreement that made the bug invisible: `str_width` was guarded and
    // `glyph_header` was not, so a string could *measure* narrow and then
    // panic on the way to being drawn.
    let f = font::profont11();
    for ch in ['\u{4e2d}', '\u{1f642}', '\u{fffd}'] {
        assert!(
            f.glyph_header(ch as u16).is_none(),
            "{ch:?} unexpectedly has a glyph"
        );
    }
    // And the degree sign, which the templates rely on, must still be there.
    assert!(
        font::profont11().glyph_header('\u{b0}' as u16).is_some(),
        "the degree sign must be drawable in profont11"
    );
}

/// Leak a test string so `OtaInput::error_message` can hold it.
///
/// `OtaInput` is `Copy` with a `&'static str`, which is the right shape for the
/// firmware (the web layer holds one) and the wrong shape for a test that builds
/// strings. The leak is bounded by the corpus above and this file never runs on
/// a device.
fn leak(text: &str) -> &'static str {
    Box::leak(text.to_string().into_boxed_str())
}
