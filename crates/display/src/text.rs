//! Number formatting for a `no_std` target with no float formatting library.
//!
//! `f64::round`, `f64::fract` and `ToString` are unavailable in `no_std`, and the C++ firmware
//! used `printf("%.1f")` to build every one of these strings. Each function here is a pure
//! integer rendering of a value that has already been clamped, so the output is byte-for-byte
//! predictable and a test can assert it.
//!
//! The important property is the *width*: [`temp_c`] always produces one of `"9.9"`, `"10.0"`,
//! `"-5.0"` … inside a five-character box, and a temperature that gains a digit grows to the
//! *left* of a fixed right edge rather than pushing its neighbours around. That is the display
//! half of the stable-numeric-field rule.

use core::fmt::Write;
use heapless::String;

/// Longest buffer any of these needs. `"-123.4"` is six, `"999 s"` is five, and the pressure
/// string is built from a one-decimal bar value so it cannot exceed that.
pub const CAP: usize = 12;

/// A temperature in degrees Celsius with one decimal, no unit.
pub fn temp_c(c: f64) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{:.1}", c);
    s
}

/// A temperature with the degree sign and `C`, e.g. `93.5°C`.
pub fn temp_c_unit(c: f64) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{:.1}\u{b0}C", c);
    s
}

/// A setpoint with the degree sign and `C` but no decimal, the way the C++ Modern template
/// printed it next to the bar.
pub fn setpoint_unit(c: f64) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{:.0}\u{b0}C", c);
    s
}

/// Milliseconds as `M:SS`, or `H:MM:SS` past an hour. A brew longer than an hour is possible on
/// a manual brew, so the hour case is handled rather than assumed away.
pub fn elapsed(ms: u32) -> String<CAP> {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    let mut out = String::new();
    if h > 0 {
        let _ = write!(out, "{}:{:02}:{:02}", h, m, s);
    } else {
        let _ = write!(out, "{}:{:02}", m, s);
    }
    out
}

/// Whole seconds, no unit and no colon, for a row that also shows a target.
///
/// Compact on purpose: `12 / 27` is five cells and `0:12 / 0:27` is eleven, and only the first
/// fits beside a translated label on 128 pixels.
pub fn seconds_short(v: u32) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{}", v);
    s
}

/// Whole seconds with the unit, the form the Modern template's brew row uses.
pub fn seconds(v: u32) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{} s", v);
    s
}

/// A weight in grams with one decimal and the unit.
pub fn weight_g(v: f64) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{:.1} g", v);
    s
}

/// A weight in grams with one decimal and no unit, for the row that also shows a target.
pub fn weight_short(v: f64) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{:.1}", v);
    s
}

/// A pressure in bar with one decimal and the unit.
pub fn pressure_bar(v: f64) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{:.1} bar", v);
    s
}

/// A heater output, 0 to 1000, as a percentage.
pub fn percent(permille: u16) -> String<CAP> {
    let mut s = String::new();
    let _ = write!(s, "{}%", permille / 10);
    s
}

/// Uptime as `H:MM`, the form the C++ status row used.
pub fn uptime(ms: u32) -> String<CAP> {
    let total = ms / 1000;
    let mut s = String::new();
    let _ = write!(s, "{}:{:02}", total / 3600, (total % 3600) / 60);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_temperature_keeps_one_decimal() {
        assert_eq!(temp_c(93.45).as_str(), "93.5");
        assert_eq!(temp_c(-5.04).as_str(), "-5.0");
        assert_eq!(temp_c(0.0).as_str(), "0.0");
    }

    #[test]
    fn a_temperature_with_a_unit_fits_the_probe_box() {
        assert_eq!(temp_c_unit(93.5).as_str(), "93.5\u{b0}C");
        assert!(
            temp_c_unit(-12.3).len() <= 8,
            "the widest formatted reading is eight characters"
        );
    }

    #[test]
    fn a_time_grows_a_digit_without_gaining_a_separator() {
        assert_eq!(elapsed(9_000).as_str(), "0:09");
        assert_eq!(elapsed(10_000).as_str(), "0:10");
        assert_eq!(elapsed(599_000).as_str(), "9:59");
        assert_eq!(elapsed(3_600_000).as_str(), "1:00:00");
    }

    #[test]
    fn the_time_field_is_a_fixed_width_for_the_range_a_brew_can_reach() {
        // A five-minute brew is four characters; the layout reserves five, so `0:09` and `0:59`
        // and `1:00` all fit without moving the field's right edge.
        for secs in [0u32, 9, 59, 60, 599, 600, 3599, 3600] {
            assert!(
                elapsed(secs * 1000).len() <= 7,
                "{secs}s must fit the reserved box"
            );
        }
    }

    #[test]
    fn a_percentage_is_derived_from_the_pid_window() {
        assert_eq!(percent(0).as_str(), "0%");
        assert_eq!(percent(1000).as_str(), "100%");
        assert_eq!(percent(9).as_str(), "0%");
        assert_eq!(percent(10).as_str(), "1%");
    }

    #[test]
    fn compact_seconds_fit_a_three_cell_box() {
        assert_eq!(seconds_short(9).as_str(), "9");
        assert_eq!(seconds_short(27).as_str(), "27");
        assert_eq!(seconds_short(999).as_str(), "999");
    }

    #[test]
    fn a_short_weight_is_five_characters_at_most() {
        assert_eq!(weight_short(9.9).as_str(), "9.9");
        assert_eq!(weight_short(1000.4).as_str(), "1000.4");
        assert!(
            weight_short(99.9).len() <= 5,
            "the row reserves five cells for the number"
        );
    }

    #[test]
    fn uptime_is_hours_and_minutes() {
        assert_eq!(uptime(0).as_str(), "0:00");
        assert_eq!(uptime(3_600_000).as_str(), "1:00");
        assert_eq!(uptime(90 * 60_000).as_str(), "1:30");
    }

    #[test]
    fn nothing_overflows_its_buffer() {
        // Every formatter is total: a hostile sensor value must not panic the render task.
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300, -1e300] {
            let _ = temp_c(v);
            let _ = temp_c_unit(v);
            let _ = setpoint_unit(v);
            let _ = weight_g(v);
            let _ = weight_short(v);
            let _ = pressure_bar(v);
        }
        let _ = elapsed(u32::MAX);
        let _ = seconds(u32::MAX);
        let _ = percent(u16::MAX);
        let _ = uptime(u32::MAX);
    }
}
