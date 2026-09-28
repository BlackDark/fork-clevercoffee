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
