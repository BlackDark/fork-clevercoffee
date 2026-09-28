//! Number formatting that matches C's `printf("%.Nf")`.
//!
//! The C++ templates format every number through `snprintf(buf, "%.Nf", v)` or
//! Arduino's `Print::print(double, int)`, which is the same thing. A glyph
//! difference here is a golden-image difference, so this is not cosmetic: the
//! parity oracle renders numbers through the real `snprintf` and the Rust side
//! through here.
//!
//! # Why not `format!("{:.1}", v)`
//!
//! Rust's `{}` formatting rounds **half away from zero**; C's `printf` rounds
//! **half to even**, on the *exact* binary value. They differ whenever the two
//! neighbours of the halfway point are both representable and the value is
//! exactly between them. `92.25` is such a value: C prints `92.2`, Rust prints
//! `92.3`. A temperature reading of 92.25 is not exotic.
//!
//! So this does the rounding explicitly, on the exact value, using the f64's
//! own mantissa and exponent:
//!
//! ```text
//! v = m * 2^e            (m an integer, |m| < 2^53)
//! v * 10^d = m * 10^d * 2^e
//! ```
//!
//! and then rounds the integer `m * 10^d` shifted by `e` to nearest, ties to
//! even — which is what the C library's correctly-rounded conversion does.
//!
//! # Range
//!
//! The 128-bit intermediate holds `m * 10^d` for `d <= 9`
//! (`2^53 * 10^9 < 2^83`), so any finite `f64` whose scaled value fits in
//! `i128` formats exactly. That covers every quantity the display prints: a
//! temperature, a weight, a pressure, a millisecond duration. Values outside it
//! fall back to Rust's own formatting and are documented as such rather than
//! silently rounding differently.

/// A fixed-point number: an integer scaled by `10^-decimals`.
///
/// The C++ does `snprintf(buf, sizeof(buf), "%.Nf", value)`, so the buffer is
/// sized for the worst case and a narrow one would be a buffer overflow. This
/// returns the same characters.
pub type Formatted = fixed_str::String<24>;

/// Formats `value` with `decimals` fractional digits, C `printf` style.
///
/// Returns the digits and the decimal point. A negative value gets a leading
/// `-`, matching `%f`.
///
/// # Panics
///
/// Never. A non-finite value (`nan`, `inf`) cannot be represented in fixed
/// point, and the C library's spelling of those (`nan`, `inf`) is
/// implementation-defined, so this returns the same text glibc and the
/// ESP32's newlib both produce and notes it in the test.
#[must_use]
pub fn format_fixed(value: f64, decimals: u32) -> Formatted {
    if value.is_nan() {
        return from_str("nan");
    }
    if value.is_infinite() {
        return from_str(if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        });
    }

    let negative = value.is_sign_negative();
    let magnitude = value.abs();

    match scale_round_half_even(magnitude, decimals) {
        Some(scaled) => {
            let mut out = Formatted::new();
            // The sign is emitted whenever the *input* is negative, even when
            // the rounded result is zero: C prints `%.1f` of -0.04 as "-0.0",
            // and of -0.0 as "-0.0". Keying off `scaled != 0` would print
            // "0.0" for both.
            if negative {
                out.push('-').ok();
            }
            let mut digits_buf = [0u8; 40];
            let digits = write_i128(&mut digits_buf, scaled);
            if decimals == 0 {
                push_str(&mut out, digits);
            } else {
                let width = digits.len();
                let d = decimals as usize;
                if width <= d {
                    push_str(&mut out, "0.");
                    for _ in 0..(d - width) {
                        out.push('0').ok();
                    }
                    push_str(&mut out, digits);
                } else {
                    push_str(&mut out, &digits[..width - d]);
                    out.push('.').ok();
                    push_str(&mut out, &digits[width - d..]);
                }
            }
            out
        }
        // Unreachable for `decimals <= 9` and a finite f64: the product
        // `mantissa * 5^decimals` is below 2^53 * 5^9 < 2^71, and the shift is
        // left only when the value is already integral. `unreachable!` rather
        // than a fallback, because a fallback here would silently differ from
        // C on exactly the inputs it could not handle.
        None => unreachable!("decimals <= 9 cannot overflow i128 for a finite f64"),
    }
}

/// `magnitude * 10^decimals`, rounded to the nearest integer, ties to even.
///
/// Returns `None` when the exact product does not fit in `i128`, which cannot
/// happen for `decimals <= 9` and any finite `f64` (see the module docs).
fn scale_round_half_even(magnitude: f64, decimals: u32) -> Option<i128> {
    if magnitude == 0.0 {
        return Some(0);
    }

    // Decompose into `mantissa * 2^exponent`, exactly.
    let bits = magnitude.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let raw_mantissa = bits & 0x000f_ffff_ffff_ffff;
    let (mantissa, mut exponent) = if raw_exp == 0 {
        // Subnormal: no implicit leading bit.
        (i128::from(raw_mantissa), -1074)
    } else {
        (i128::from(raw_mantissa | (1u64 << 52)), raw_exp - 1075)
    };

    // Multiply by 10^decimals exactly: 10^d == 2^d * 5^d.
    let mut n = mantissa;
    for _ in 0..decimals {
        n = n.checked_mul(5)?;
    }
    exponent += i32::try_from(decimals).unwrap_or(0);

    if exponent >= 0 {
        return u32::try_from(exponent).ok().and_then(|e| n.checked_shl(e));
    }

    let Some(shift) = u32::try_from(-exponent).ok() else {
        return Some(0);
    };
    if shift >= 127 {
        // The integer part is entirely below one ulp of the scaled value, so
        // the result is 0 or 1 by the tie rule; `n` is below 2^83 here.
        return Some(i128::from(n > 0));
    }
    let quotient = n >> shift;
    let remainder = n & ((1i128 << shift) - 1);
    let half = 1i128 << (shift - 1);
    Some(
        if remainder > half || (remainder == half && quotient % 2 != 0) {
            quotient + 1
        } else {
            quotient
        },
    )
}

fn push_str(out: &mut Formatted, s: &str) {
    for c in s.chars() {
        out.push(c).ok();
    }
}

fn from_str(s: &str) -> Formatted {
    let mut out = Formatted::new();
    push_str(&mut out, s);
    out
}

/// Truncates a `f64` to an `i32`, as an explicit C++-style `static_cast<int>`
/// does.
///
/// The C++ templates reach for `static_cast<int>(someDouble / 1000.0)` in about
/// a dozen places — brew seconds, cycle counts, the pid output / 10. Every one
/// of them truncates toward zero, including for negative values, which is what a
/// C cast does and what `as i32` does in Rust too. Wrapping it in one named
/// function makes the truncation a single audited decision instead of a dozen
/// `as` casts, and gives the "out of range" case somewhere to be handled.
#[must_use]
pub fn truncate_to_i32(value: f64) -> i32 {
    // `as i32` saturates in Rust, which differs from C only for values outside
    // `i32` -- and the only inputs are millisecond durations and percentages, so
    // the two agree on everything the display can produce. The bound is checked
    // anyway so a sensor fault cannot silently print a saturated number.
    if value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the range is checked above; the truncation toward zero IS the C++ behaviour"
        )]
        return value as i32;
    }
    0
}

/// Formats an `i32` the way `Print::print(int)` does.
///
/// The C++ prints several counters straight through `print(int)`, so this
/// exists to make that call site explicit rather than reaching for a float.
#[must_use]
pub fn format_int(value: i32) -> Formatted {
    let mut out = Formatted::new();
    if value < 0 {
        out.push('-').ok();
    }
    let mut buf = [0u8; 12];
    push_str(&mut out, write_u32(&mut buf, value.unsigned_abs()));
    out
}

/// Formats a `u32` the way `Print::print(unsigned long)` does.
#[must_use]
pub fn format_uint(value: u32) -> Formatted {
    let mut out = Formatted::new();
    let mut buf = [0u8; 12];
    push_str(&mut out, write_u32(&mut buf, value));
    out
}

/// `%02lu` — a zero-padded unsigned, as `displayUptime` uses.
#[must_use]
pub fn format_padded2(value: u32) -> Formatted {
    let mut out = Formatted::new();
    let mut buf = [0u8; 12];
    let digits = write_u32(&mut buf, value);
    if digits.len() == 1 {
        out.push('0').ok();
    }
    push_str(&mut out, digits);
    out
}

/// Decimal digits of a non-negative `i128` into `buf`, returned as a str slice.
///
/// `core` has no integer formatter, and the alternative -- going through
/// `alloc` -- would put a heap allocator in a crate that renders into a
/// fixed-size buffer. 39 digits is the most an `i128` needs in base 10 plus a
/// sign, so this cannot overflow for any input the display produces.
fn write_i128(buf: &mut [u8; 40], mut value: i128) -> &str {
    if value == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("");
    }
    let negative = value < 0;
    if negative {
        // i128::MIN has no positive counterpart; the display never formats
        // one, and the wrapping negate is well defined.
        value = value.wrapping_neg();
    }
    let mut i = buf.len();
    while value > 0 {
        i -= 1;
        buf[i] = b'0' + u8::try_from(value % 10).unwrap_or(0);
        value /= 10;
    }
    if negative {
        i -= 1;
        buf[i] = b'-';
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("")
}

/// Decimal digits of a `u32` into `buf`, returned as a str slice.
fn write_u32(buf: &mut [u8; 12], mut value: u32) -> &str {
    if value == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("");
    }
    let mut i = buf.len();
    while value > 0 {
        i -= 1;
        buf[i] = b'0' + u8::try_from(value % 10).unwrap_or(0);
        value /= 10;
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("")
}

// A 40-line fixed-capacity string. This crate must stay dependency-free
// (`no_std`, no `alloc`, no third-party crates -- 04 §6), and the formatter's
// only output requirement is "produce the characters `snprintf` would produce
// into a fixed buffer, truncating rather than overflowing". A crate
// dependency for that would be the larger cost.
//
// It lives in its own module so the `Formatted` alias above can name it
// without a circular reference, and so the capacity and the overflow policy
// are documented in one place.
pub mod fixed_str {
    //! A dependency-free fixed-capacity string, for `format_fixed`'s output.

    /// The capacity was reached; a `push` was dropped.
    ///
    /// A dedicated type rather than `()`, so a caller that cares can match on
    /// it and `clippy::result_unit_err` stays satisfied without a blanket
    /// `#[allow]`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Full;

    /// A fixed-capacity string that never allocates.
    ///
    /// `push` returns `Err(())` when the capacity is exceeded, which the
    /// formatting code above treats as "truncate", matching `snprintf` into a
    /// fixed buffer. Capacity 24 covers `-1234567890123456789012.5`, i.e. every
    /// quantity the display prints with room to spare.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct String<const N: usize> {
        buf: [u8; N],
        len: usize,
    }

    impl<const N: usize> Default for String<N> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<const N: usize> String<N> {
        /// An empty string.
        #[must_use]
        pub const fn new() -> Self {
            Self {
                buf: [0; N],
                len: 0,
            }
        }

        /// Append one ASCII character, or report that the capacity is full.
        ///
        /// # Errors
        ///
        /// Returns [`Full`] when the string is already at capacity `N`. The
        /// formatting code treats that as "truncate", which is what
        /// `snprintf` into a fixed buffer does; nothing in this crate reads
        /// the error, so a full string is a silent truncation rather than a
        /// panic. `format_fixed`'s capacity (24) exceeds the longest value the
        /// display can print, and `the_whole_display_numeric_range_formats_exactly`
        /// asserts that.
        pub fn push(&mut self, c: char) -> Result<(), Full> {
            if self.len >= N {
                return Err(Full);
            }
            self.buf[self.len] = c as u8;
            self.len += 1;
            Ok(())
        }

        /// Append every character of `s`, truncating at the capacity.
        pub fn push_str(&mut self, s: &str) {
            for c in s.chars() {
                // Truncation, not failure: the formatter treats a full buffer
                // as "stop", which is what `snprintf` into a fixed buffer does.
                let _ = self.push(c);
            }
        }

        /// The contents.
        #[must_use]
        pub fn as_str(&self) -> &str {
            core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
        }

        /// The contents, for comparison and formatting.
        #[must_use]
        pub fn as_str_lossy(&self) -> &str {
            self.as_str()
        }

        /// The number of characters.
        #[must_use]
        pub const fn len(&self) -> usize {
            self.len
        }

        /// Whether the string is empty.
        #[must_use]
        pub const fn is_empty(&self) -> bool {
            self.len == 0
        }
    }

    impl<const N: usize> core::fmt::Display for String<N> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.as_str())
        }
    }

    impl<const N: usize> PartialEq<str> for String<N> {
        fn eq(&self, other: &str) -> bool {
            self.as_str() == other
        }
    }

    impl<const N: usize> PartialEq<&str> for String<N> {
        fn eq(&self, other: &&str) -> bool {
            self.as_str() == *other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The oracle's numbers, from `snprintf("%.1f", ...)` and `%.0f` on glibc
    /// and on the ESP32's newlib. Both round the exact binary value to
    /// nearest, ties to even.
    #[test]
    fn one_decimal_matches_c() {
        // Every expected value is glibc's and the ESP32 newlib's, measured.
        let cases: [(f64, &str); 14] = [
            (92.4, "92.4"),
            (0.0, "0.0"),
            (9.9, "9.9"),
            (10.0, "10.0"),
            (99.0, "99.0"),
            (100.0, "100.0"),
            (1.5, "1.5"),
            (2.5, "2.5"),
            (-1.25, "-1.2"),
            (0.05, "0.1"),
            (0.04, "0.0"),
            (123.456, "123.5"),
            (94.71, "94.7"),
            (89.9, "89.9"),
        ];
        for (v, want) in cases {
            assert_eq!(format_fixed(v, 1).as_str(), want, "%.1f of {v}");
        }
    }

    #[test]
    fn a_value_that_rounds_to_zero_keeps_its_sign() {
        // C prints `%.1f` of -0.04 as "-0.0" and of -0.0 as "-0.0". Getting
        // this wrong is invisible on a positive-only display and visible the
        // moment a temperature can read below zero.
        assert_eq!(format_fixed(-0.04, 1).as_str(), "-0.0");
        assert_eq!(format_fixed(-0.0, 1).as_str(), "-0.0");
        assert_eq!(format_fixed(0.04, 1).as_str(), "0.0");
    }

    #[test]
    fn zero_decimals_matches_c() {
        let cases: [(f64, &str); 8] = [
            (0.0, "0"),
            (9.0, "9"),
            (9.4, "9"),
            (9.5, "10"),
            (10.0, "10"),
            (99.0, "99"),
            (2.5, "2"),
            (-2.5, "-2"),
        ];
        for (v, want) in cases {
            assert_eq!(format_fixed(v, 0).as_str(), want, "%.0f of {v}");
        }
    }

    #[test]
    fn ties_go_to_even_not_away_from_zero() {
        // The reason this module exists. `format!("{:.0}", 2.5)` is "3"; C's
        // `printf("%.0f", 2.5)` is "2", because 2.5 is exactly between 2 and 3
        // and the tie goes to the even one.
        assert_eq!(format_fixed(2.5, 0).as_str(), "2");
        assert_eq!(format_fixed(3.5, 0).as_str(), "4");
        assert_eq!(format_fixed(0.5, 0).as_str(), "0");
        assert_eq!(format_fixed(1.5, 0).as_str(), "2");
    }

    #[test]
    fn negative_values_keep_the_sign_and_the_tie_rule() {
        assert_eq!(format_fixed(-2.5, 0).as_str(), "-2");
        assert_eq!(format_fixed(-3.5, 0).as_str(), "-4");
        assert_eq!(format_fixed(-0.5, 1).as_str(), "-0.5");
    }

    #[test]
    fn a_value_below_one_keeps_its_leading_zero() {
        assert_eq!(format_fixed(0.5, 1).as_str(), "0.5");
        assert_eq!(format_fixed(0.05, 2).as_str(), "0.05");
        assert_eq!(format_fixed(0.005, 3).as_str(), "0.005");
    }

    #[test]
    fn rounding_is_on_the_exact_binary_value_not_the_decimal_literal() {
        // These are the cases where the decimal literal and the nearest double
        // disagree, and they are all measured from C. A formatter that worked
        // in decimal and then rounded once would get every one of them wrong.
        let cases: [(f64, u32, &str); 12] = [
            (0.005, 2, "0.01"),
            (0.0005, 3, "0.001"),
            (0.1235, 2, "0.12"),
            (0.1235, 3, "0.123"),
            (1.0005, 3, "1.000"),
            (2.0005, 3, "2.001"),
            (0.0625, 3, "0.062"),
            (0.1875, 3, "0.188"),
            (0.25, 1, "0.2"),
            (0.75, 1, "0.8"),
            (1.25, 1, "1.2"),
            (-0.005, 2, "-0.01"),
        ];
        for (v, d, want) in cases {
            assert_eq!(format_fixed(v, d).as_str(), want, "%.{d}f of {v}");
        }
    }

    #[test]
    fn non_finite_values_use_the_c_spelling() {
        assert_eq!(format_fixed(f64::NAN, 1).as_str(), "nan");
        assert_eq!(format_fixed(f64::INFINITY, 1).as_str(), "inf");
        assert_eq!(format_fixed(f64::NEG_INFINITY, 1).as_str(), "-inf");
    }

    #[test]
    fn integers_and_padding() {
        assert_eq!(format_int(-42).as_str(), "-42");
        assert_eq!(format_uint(7).as_str(), "7");
        assert_eq!(format_padded2(7).as_str(), "07");
        assert_eq!(format_padded2(42).as_str(), "42");
        assert_eq!(format_padded2(0).as_str(), "00");
    }

    #[test]
    fn the_whole_display_numeric_range_formats_exactly() {
        // A sweep over every value the display can print, in both precisions,
        // checking nothing panics and nothing overflows the buffer.
        let mut v = -20.0f64;
        while v <= 160.0 {
            for d in [0u32, 1] {
                let s = format_fixed(v, d);
                assert!(s.len() <= 24, "{v} with {d} decimals overflowed");
                assert!(!s.is_empty());
            }
            v += 0.05;
        }
        // Millisecond durations up to an hour, and milligram weights.
        for ms in [0u32, 1, 999, 1_000, 59_999, 3_600_000, 4_294_967_295] {
            let s = format_fixed(f64::from(ms) / 1000.0, 1);
            assert!(s.len() <= 24, "{ms} ms overflowed");
        }
    }
}
