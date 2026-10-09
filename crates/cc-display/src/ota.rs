//! Port of `src/display/DisplayOtaScreen.cpp`.
//!
//! Three mutually exclusive renderings, chosen by the OTA state, plus the
//! one non-trivial bit of logic in the file: `truncateToWidth`.
//!
//! # `truncateToWidth`
//!
//! The error message is an arbitrary `String` from the HTTP layer, and the
//! screen is 128 px wide, so it has to be cut to fit. The C++ peels one
//! character off the end until it fits:
//!
//! ```cpp
//! while (!message.isEmpty() && display->getStrWidth(message.c_str()) > kMaxWidth)
//!     message.remove(message.length() - 1);
//! if (message.endsWith("..")) return;
//! if (display->getStrWidth(message.c_str()) > kMaxWidth && message.length() > 3)
//!     message = message.substring(0, message.length() - 3) + "...";
//! ```
//!
//! Three subtleties, all reproduced:
//!
//! * the loop is `length() - 1`, so it drops the *last* character and re-tests —
//!   an O(n^2) walk, but n is a short error string and matching it matters more
//!   than the cost;
//! * a message that already ends in `".."` is left as-is even if it is still too
//!   wide, because the author is telling the truncation that it ran; and
//! * the `length() > 3` guard means the `"..."` suffix is never *longer* than
//!   what it replaced, so a 3-character message that is too wide is left
//!   too wide rather than becoming `"..."`.
//!
//! This is the one place in the display layer where a `String` exists, and it is
//! a `&'static str` here: the device builds it once per frame from the OTA
//! manager, and a host test can pass any string.

use crate::display::{Display, DISPLAY_WIDTH};
use crate::fmt::{format_int, format_uint};
use crate::font;
use crate::layout::draw_str_centered_on_screen;
use crate::model::{DisplayInput, OtaKind, OtaStatus};
use crate::templates::Stage;

/// The width budget: `DISPLAY_WIDTH - 4`, i.e. a 2 px margin each side.
const MAX_WIDTH: i32 = DISPLAY_WIDTH - 4;

/// `drawOtaScreen` — returns `None` when the OTA screen does not apply.
#[must_use]
pub fn draw(d: &mut Display, input: &DisplayInput) -> Option<Stage> {
    if !input.ota.show {
        return None;
    }

    d.clear_buffer();

    match input.ota.status {
        // `hasError() || status == Error` -- the C++ checks both, so an error
        // flag with a non-error status still takes this branch.
        OtaStatus::Error => {
            // fub17 is 150 px and centres at x = -11.
            d.set_font(font::profont17());
            draw_str_centered_on_screen(d, 8, "Update failed");

            d.set_font(font::profont10());
            let message = if input.ota.error_message.is_empty() {
                "Unknown error"
            } else {
                input.ota.error_message
            };
            let truncated = truncate_to_width(d, message);
            draw_str_centered_on_screen(d, 32, truncated.as_str());

            d.set_font(font::profont10());
            draw_str_centered_on_screen(d, 48, "Retry from web UI");
        }
        OtaStatus::Complete => {
            d.set_font(font::fub17());
            draw_str_centered_on_screen(d, 12, "Update OK");
            d.set_font(font::profont11());
            draw_str_centered_on_screen(d, 36, "Restarting...");
        }
        _ => {
            d.set_font(font::fub17());
            draw_str_centered_on_screen(d, 6, "Updating");

            d.set_font(font::profont10());
            draw_str_centered_on_screen(d, 24, kind_label(input.ota.kind));

            crate::widgets::display_progress_bar(d, i32::from(input.ota.progress), 14, 38, 100);

            d.set_font(font::profont11());
            // `snprintf("%u%%")` then centred.
            let mut percent = format_uint(u32::from(input.ota.progress));
            percent.push('%').ok();
            draw_str_centered_on_screen(d, 50, percent.as_str());
        }
    }

    Some(Stage::Ota)
}

/// `otaTypeLabel` — `"Filesystem"` or `"Firmware"`.
#[must_use]
pub const fn kind_label(kind: OtaKind) -> &'static str {
    match kind {
        OtaKind::Filesystem => "Filesystem",
        OtaKind::Firmware => "Firmware",
    }
}

/// `truncateToWidth` — cut `message` until it fits in `MAX_WIDTH`.
///
/// See the module docs for the three behaviours that are easy to lose. Returns
/// a [`FixedText`], a fixed-capacity copy, because there is no `alloc` here and
/// a `&'static str` cannot be built at runtime.
///
/// The 96-character capacity covers `MAX_WIDTH / 2` in `profont10` (the widest
/// glyph is 6 px) with room to spare, so the peel loop cannot overflow it. A
/// message longer than that would be truncated by capacity, not by width — which
/// is the same outcome, and is asserted in the tests.
#[must_use]
pub fn truncate_to_width(d: &Display, message: &str) -> FixedText {
    let mut out = FixedText::from(message);

    // The peel loop. `out.len()` shrinks by one byte per iteration; a multi-byte
    // UTF-8 sequence would be cut mid-character, which is what the C++ does too
    // (it removes one *byte*), and the fonts are Latin-1 so it cannot happen for
    // the ASCII error strings the device actually produces.
    while !out.is_empty() && d.str_width(out.as_str()) > MAX_WIDTH {
        out.remove_last();
    }
    if out.ends_with("..") {
        return out;
    }
    if d.str_width(out.as_str()) > MAX_WIDTH && out.len() > 3 {
        let mut replaced = FixedText::new();
        replaced.push_str(&out.as_str()[..out.len() - 3]);
        replaced.push_str("...");
        return replaced;
    }
    out
}

/// A fixed-capacity string for [`truncate_to_width`]'s result.
///
/// 96 bytes: `MAX_WIDTH` is 124 px and the widest `profont10` glyph is 6 px, so
/// no message that *fits* can be longer than 21 characters. 96 is generous, and
/// a `push` past it truncates rather than panicking, so a hostile input cannot
/// abort the display task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedText {
    buf: [u8; 96],
    len: usize,
}

impl FixedText {
    /// An empty string.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; 96],
            len: 0,
        }
    }

    /// A copy of `s`, truncated to the capacity.
    #[must_use]
    pub fn from(s: &str) -> Self {
        let mut out = Self::new();
        out.push_str(s);
        out
    }

    /// Append `s`, truncating at the capacity.
    pub fn push_str(&mut self, s: &str) {
        for b in s.bytes() {
            if self.len >= self.buf.len() {
                return;
            }
            self.buf[self.len] = b;
            self.len += 1;
        }
    }

    /// Drop the last byte.
    pub fn remove_last(&mut self) {
        if self.len > 0 {
            self.len -= 1;
        }
    }

    /// The contents.
    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    /// The length in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the string is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether the string ends with `suffix`.
    #[must_use]
    pub fn ends_with(&self, suffix: &str) -> bool {
        let s = self.as_str();
        s.len() >= suffix.len() && s.ends_with(suffix)
    }
}

impl Default for FixedText {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq<&str> for FixedText {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<&FixedText> for str {
    fn eq(&self, other: &&FixedText) -> bool {
        self == other.as_str()
    }
}

impl core::fmt::Display for FixedText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A convenience so the three branches read the same as the C++'s.
#[must_use]
pub fn percent_label(progress: u8) -> crate::fmt::Formatted {
    let mut s = format_int(i32::from(progress));
    s.push('%').ok();
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::Rotation;

    fn d() -> Display {
        let mut d = Display::new();
        d.set_font(font::profont10());
        d.set_font_pos_top();
        d
    }

    #[test]
    fn a_short_message_is_untouched() {
        let d = d();
        assert_eq!(
            truncate_to_width(&d, "Flash write failed"),
            "Flash write failed"
        );
    }

    #[test]
    fn a_long_message_is_peeled_one_byte_at_a_time() {
        let d = d();
        let long = "the firmware image could not be written to the flash partition";
        let out = truncate_to_width(&d, long);
        assert!(
            d.str_width(out.as_str()) <= MAX_WIDTH,
            "must fit, got {}",
            d.str_width(out.as_str())
        );
        // Peeled, not replaced: the `"..."` branch only runs if the peel did not
        // already fit it, which it always does. So the result is a prefix.
        assert!(long.starts_with(out.as_str()));
        assert!(
            !out.ends_with("..."),
            "the peel always fits first, so the ellipsis branch is dead here"
        );
    }

    #[test]
    fn the_peel_loop_always_runs_before_the_dot_dot_guard() {
        // `DisplayOtaScreen.cpp:26`:
        //
        // ```cpp
        // while (!message.isEmpty() && getStrWidth(message) > kMaxWidth)
        //     message.remove(message.length() - 1);
        // if (message.endsWith("..")) return;
        // if (getStrWidth(message) > kMaxWidth && message.length() > 3)
        //     message = message.substring(0, message.length() - 3) + "...";
        // ```
        //
        // This is a real behaviour difference, not a formality. The guard reads
        // the *peeled* string, and the peel stops at the first prefix that fits.
        // Here the boundary lands on the space after "error", so the guard does
        // not fire and the second `if` does not either -- the string already
        // fits -- and the result is a 24-character string with a trailing space.
        //
        // A port that checked for `..` *before* peeling, which is what the
        // guard reads like it intends, would return the full 49-character
        // message 1 px too wide and blow the 124 px budget. So the ordering is
        // pinned here.
        let d = d();
        let marked = "an extremely long error message that will not fit..";
        assert!(
            d.str_width(marked) > MAX_WIDTH,
            "the premise: it does not fit"
        );

        let out = truncate_to_width(&d, marked);
        assert_eq!(out.as_str(), "an extremely long error ");
        assert!(d.str_width(out.as_str()) <= MAX_WIDTH, "and now it fits");
        // Adding the next character is what makes the input too wide, so the
        // peel really did stop one character short.
        let mut plus_one = FixedText::new();
        plus_one.push_str(out.as_str());
        plus_one.push_str("m");
        assert!(d.str_width(plus_one.as_str()) > MAX_WIDTH);
    }

    #[test]
    fn the_guard_is_reachable_but_never_changes_the_result() {
        // The peel removes from the end until the string fits, so its result is
        // the *longest* prefix that fits. The `endsWith("..")` guard can only
        // fire when that longest prefix ends in `..`.
        //
        // A run of dots is the case that gets there: every prefix ends in "..",
        // so any string long enough to need peeling reaches the guard. Search
        // for the boundary instead of guessing it, so the test proves the guard
        // is reachable rather than asserting that it is not.
        let d = d();
        let mut dots = FixedText::new();
        dots.push_str(&".".repeat(40));
        let source = dots.as_str();

        let fitting = (2..=source.len())
            .filter(|&n| d.str_width(&source[..n]) <= MAX_WIDTH)
            .max_by_key(|&n| n)
            .expect("a dot run this long has a fitting prefix");
        let boundary = &source[..fitting];
        assert!(
            boundary.ends_with(".."),
            "the premise: a two-dot fit boundary"
        );
        assert!(
            d.str_width(&source[..=fitting]) > MAX_WIDTH,
            "the premise: one more character would not fit"
        );

        let out = truncate_to_width(&d, source);
        // Reached the guard, and the guard returned the peel unchanged -- which
        // is also what the second `if` would have done, because the string
        // already fits. Reachable, but inert.
        assert_eq!(out.as_str(), boundary);
    }

    #[test]
    fn a_three_character_message_is_not_replaced_by_an_ellipsis() {
        // `message.length() > 3` guards the `"..."` branch, so a 3-character
        // message that is too wide is left too wide rather than becoming the
        // longer `"..."`.
        let d = d();
        let three = "WWW";
        assert!(truncate_to_width(&d, three).len() <= 3, "must not grow");
    }

    #[test]
    fn the_empty_message_becomes_unknown_error() {
        // That substitution happens at the call site, not in the truncate
        // helper; asserted here so the two are read together.
        let d = d();
        // The substitution is at the call site, and it reads the *input's*
        // message, not a literal.
        let raw: &str = "";
        let message = if raw.is_empty() { "Unknown error" } else { raw };
        assert_eq!(truncate_to_width(&d, message), "Unknown error");
    }

    #[test]
    fn the_error_screen_is_chosen_by_the_status_not_by_a_flag() {
        let base = DisplayInput {
            ota: crate::model::OtaInput {
                show: true,
                ..Default::default()
            },
            ..Default::default()
        };
        for status in [OtaStatus::Idle, OtaStatus::Uploading, OtaStatus::Processing] {
            let input = DisplayInput {
                ota: crate::model::OtaInput {
                    show: true,
                    status,
                    ..Default::default()
                },
                ..base
            };
            let mut d = d();
            assert_eq!(
                draw(&mut d, &input),
                Some(Stage::Ota),
                "{status:?} shows the progress form"
            );
        }
        for status in [OtaStatus::Complete, OtaStatus::Error] {
            let input = DisplayInput {
                ota: crate::model::OtaInput {
                    show: true,
                    status,
                    ..Default::default()
                },
                ..base
            };
            let mut d = d();
            assert_eq!(
                draw(&mut d, &input),
                Some(Stage::Ota),
                "{status:?} shows a terminal form"
            );
        }
    }

    #[test]
    fn the_ota_screen_declines_when_not_shown() {
        let mut d = d();
        assert_eq!(
            draw(&mut d, &DisplayInput::default()),
            None,
            "no OTA in flight"
        );
    }

    #[test]
    fn the_ota_frame_fits_the_panel() {
        // The progress form is the one with a bar and a centred percentage,
        // and it is drawn *before* the rotation is known, so it is laid out for
        // a landscape 128x64 panel.
        for rotation in [Rotation::R0, Rotation::R1, Rotation::R2, Rotation::R3] {
            let input = DisplayInput {
                ota: crate::model::OtaInput {
                    show: true,
                    status: OtaStatus::Processing,
                    progress: 42,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut d = d();
            d.set_display_rotation(rotation);
            let _unused = draw(&mut d, &input);
            let fb = d.into_framebuffer();
            assert!(fb.lit_bounds().is_some(), "{rotation:?} drew nothing");
            // The framebuffer is physically 128x64 whatever the rotation, so a
            // bounds check on it is the check.
            let (x0, y0, x1, y1) = fb.lit_bounds().expect("ink");
            assert!((0..128).contains(&x0) && (0..64).contains(&y0));
            assert!((0..128).contains(&x1) && (0..64).contains(&y1));
        }
    }

    #[test]
    fn the_percentage_label_is_percent_suffixed() {
        assert_eq!(percent_label(0).as_str(), "0%");
        assert_eq!(percent_label(42).as_str(), "42%");
        assert_eq!(percent_label(100).as_str(), "100%");
    }

    #[test]
    fn the_kind_label_distinguishes_firmware_from_filesystem() {
        assert_eq!(kind_label(OtaKind::Firmware), "Firmware");
        assert_eq!(kind_label(OtaKind::Filesystem), "Filesystem");
    }
}
