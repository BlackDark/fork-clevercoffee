//! `format_fixed` against C's `snprintf("%.*f")`, over 80 000 values.
//!
//! # Why this is a data file and not a table in the test
//!
//! The interesting failures are the *rare* ones: a value whose two nearest
//! `f64` neighbours straddle a decimal halfway point, where ties-to-even and
//! ties-away-from-zero disagree. A hand-written table contains the cases the
//! author thought of; `fmt_oracle.txt` contains 80 000 values from a fixed
//! PRNG, every one of which C itself formatted. A regression that breaks the
//! tie rule, the exponent handling or the sign therefore shows up as a diff
//! against a file rather than as a subtly wrong glyph on one screen.
//!
//! Each line is `<16 hex digits of the IEEE-754 pattern> <decimals> <C's
//! rendering>`. The Rust side formats the *exact* double C formatted, so the
//! two cannot disagree about which number they meant.
//!
//! # Regenerating
//!
//! `tools/fmt_oracle.c` is the C program that produced the file. It is not
//! built by the test — the file is the artefact, and the C program is kept so
//! the artefact can be re-derived if the rounding rule is ever in question.
//!
//! ```text
//! cc -O0 -o /tmp/fmt_oracle crates/cc-display/tools/fmt_oracle.c
//! /tmp/fmt_oracle > crates/cc-display/tests/fmt_oracle.txt
//! ```

use cc_display::fmt::format_fixed;

const ORACLE: &str = include_str!("fmt_oracle.txt");

#[test]
fn format_fixed_matches_c_over_eighty_thousand_values() {
    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for (lineno, line) in ORACLE.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.splitn(3, ' ');
        let (Some(bits_hex), Some(decimals), Some(expected)) =
            (fields.next(), fields.next(), fields.next())
        else {
            panic!("fmt_oracle.txt line {} is malformed: {line:?}", lineno + 1);
        };
        let bits = u64::from_str_radix(bits_hex, 16).expect("hex bit pattern");
        let decimals: u32 = decimals.parse().expect("decimal count");
        let value = f64::from_bits(bits);

        let got = format_fixed(value, decimals);
        checked += 1;
        if got.as_str() != expected {
            mismatches.push(format!(
                "line {}: bits={bits_hex} (value {value:?}) %.{decimals}f -- C says {expected:?}, cc-display says {:?}",
                lineno + 1,
                got.as_str()
            ));
            if mismatches.len() >= 20 {
                break;
            }
        }
    }

    assert_eq!(checked, 80_000, "the oracle file should hold 80 000 cases");
    assert!(
        mismatches.is_empty(),
        "{} of {checked} values differ from C:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

// -- The degree sign ----------------------------------------------------------

// Four fields build their unit string through `fmt::Formatted::push`: the
// TemperatureOnly `Target  %.1f °C` line, Modern's idle `°C`, Modern's
// bottom-row setpoint label and Modern's brew `°C`. `push` stored `c as u8`, so
// `push('\u{b0}')` wrote a lone `0xB0`, which is not valid UTF-8 — `as_str()`
// then returned the **empty string** for the whole field, not just for the
// degree sign. All four fields drew nothing.
//
// These tests assert the observable outcome: the degree glyph is actually inked
// on the rendered panel, in the font each field draws with. A test on a private
// helper would still pass if the field were never drawn at all — which is
// exactly what happened.

use cc_display::display::{Display, Framebuffer};
use cc_display::font::{self, Font};
use cc_display::model::{BrewTimerState, Config, DisplayInput};
use cc_display::templates::{self, TemplateId};

/// The lit pixels of one `°C` unit -- the glyphs U8g2's
/// `print(static_cast<char>(176))` followed by `'C'` put on the panel -- as
/// `(dx, dy)` offsets from the unit's top-left corner.
///
/// The **unit**, not the bare degree sign: a small ring of pixels on its own is
/// not a fingerprint, and a panel full of digits has plenty of those. A `°C`
/// pair at the font's own advance is, and all four fields end in one.
fn degree_c_pattern(make_font: fn() -> Font) -> Vec<(i32, i32)> {
    let mut d = Display::new();
    d.set_font(make_font());
    d.set_cursor(0, 20);
    d.print_char('\u{b0}');
    d.print_char('C');

    let lit: Vec<(i32, i32)> = d.into_framebuffer().lit_pixels().collect();
    assert!(!lit.is_empty(), "the font has no degree sign glyph");
    let min_x = lit.iter().map(|p| p.0).min().unwrap_or(0);
    let min_y = lit.iter().map(|p| p.1).min().unwrap_or(0);
    lit.into_iter()
        .map(|(x, y)| (x - min_x, y - min_y))
        .collect()
}

/// Whether `pattern` appears anywhere in `frame`.
///
/// Translation-only: the templates place these fields by measured width, so the
/// unit's own coordinates are not part of the contract. The whole 128x64 panel
/// is searched, so a field that is drawn *anywhere* counts -- which is the claim,
/// and the one that was false.
fn contains(frame: &Framebuffer, pattern: &[(i32, i32)]) -> bool {
    let (span_x, span_y) = pattern
        .iter()
        .fold((0, 0), |(mx, my), &(dx, dy)| (mx.max(dx), my.max(dy)));
    for y in 0..(64 - span_y) {
        for x in 0..(128 - span_x) {
            if pattern.iter().all(|&(dx, dy)| frame.pixel(x + dx, y + dy)) {
                return true;
            }
        }
    }
    false
}

/// A machine mid-brew on a setpoint, which is what every field needs to draw.
fn input(brew_timer: BrewTimerState) -> DisplayInput {
    DisplayInput {
        temperature: 92.5,
        setpoint: 93.0,
        pid_output: 420.0,
        brew_time_ms: 12_500.0,
        target_brew_time_ms: 30_000.0,
        brew_timer,
        brew_active: brew_timer == BrewTimerState::Running,
        now_ms: 987_654,
        ..DisplayInput::default()
    }
}

/// Render one template and report whether `pattern` is on the panel.
fn draws(template: TemplateId, input: &DisplayInput, pattern: &[(i32, i32)]) -> bool {
    let mut d = Display::new();
    d.set_display_rotation(Config::default().rotation());
    templates::render(template, &mut d, input, &Config::default());
    contains(&d.into_framebuffer(), pattern)
}

#[test]
fn the_temperature_only_target_line_prints_its_degree_sign() {
    let config = Config::default();
    let input = input(BrewTimerState::Idle);
    let mut d = Display::new();
    d.set_display_rotation(config.rotation());
    templates::render(TemplateId::TemperatureOnly, &mut d, &input, &config);
    assert!(
        contains(&d.into_framebuffer(), &degree_c_pattern(font::profont11)),
        "the TemperatureOnly 'Target 93.0 °C' line drew no degree sign at all"
    );
}

#[test]
fn the_modern_idle_unit_prints_its_degree_sign() {
    assert!(
        draws(
            TemplateId::Modern,
            &input(BrewTimerState::Idle),
            &degree_c_pattern(font::profont17)
        ),
        "Modern's idle temperature unit drew no degree sign"
    );
}

#[test]
fn the_modern_setpoint_label_prints_its_degree_sign() {
    assert!(
        draws(
            TemplateId::Modern,
            &input(BrewTimerState::Idle),
            &degree_c_pattern(font::profont10)
        ),
        "Modern's bottom-row setpoint label drew no degree sign"
    );
}

#[test]
fn the_modern_brew_temperature_prints_its_degree_sign() {
    assert!(
        draws(
            TemplateId::Modern,
            &input(BrewTimerState::Running),
            &degree_c_pattern(font::profont17)
        ),
        "Modern's brew temperature field drew no degree sign"
    );
}
