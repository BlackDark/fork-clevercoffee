//! Compare exact IEEE-754 inputs against frozen C `snprintf` output.
//!
//! Rows contain hex bits, decimal precision, and rendered text. The generator is
//! not in this tree; regeneration requires a C formatter, not Rust-derived expectations.

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
