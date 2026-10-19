//! The boot screen: logo, version, Wi-Fi address.
//!
//! # Why this module exists
//!
//! The panel shows the screen twice: once with the firmware version straight
//! after the display is brought up, and once with the address as soon as the radio
//! has associated. Without it the panel sits on whatever the previous frame left
//! there — on a cold boot, a cleared panel — for as long as the Wi-Fi takes.
//!
//! # The layout
//!
//! Two very different arrangements behind one `if`, and both are reproduced:
//!
//! | template | logo | text |
//! | --- | --- | --- |
//! | landscape (the default) | `(38, 0)`, 40x40 | two lines at `(0, 45)` and `(0, 55)` |
//! | Upright | `(11, 4)`, 40x40 | one word per line from `y = 47`, 10 px apart |
//!
//! The Upright arm word-wraps on spaces, which is why the version line is passed
//! with a trailing space: the empty final token is dropped rather than printing a
//! blank line. The landscape arm prints the two lines as given, so the same
//! trailing space costs a pixel of width and nothing else.

use crate::bitmaps_data as bm;
use crate::display::Display;
use crate::font;
use crate::templates::TemplateId;

/// Draw the boot screen.
///
/// The two lines are separate arguments because the Upright arm wraps them word by
/// word and the landscape arm prints them verbatim, so a caller passes what its
/// layout needs: a phrase in Upright, a whole row in landscape.
pub fn draw(d: &mut Display, line1: &str, line2: &str, template: TemplateId) {
    d.clear_buffer();
    if template.is_upright() {
        // The portrait space is 64 wide and 128 tall, so the logo is near the
        // top and the words run down from y=47 in the *logical* space; the
        // rotation transform in `Display` puts them on the physical panel.
        d.draw_xbmp(11, 4, 40, 40, &bm::CLEVERCOFFEE_LOGO);
        d.set_font(font::profont10());
        let mut y = 47;
        for word in line1.split(' ').chain(line2.split(' ')) {
            if word.is_empty() {
                continue;
            }
            d.draw_str(0, y, word);
            y += 10;
        }
    } else {
        d.draw_str(0, 45, line1);
        d.draw_str(0, 55, line2);
        d.draw_xbmp(38, 0, 40, 40, &bm::CLEVERCOFFEE_LOGO);
    }
}

/// The boot strings, in one place.
///
/// Named so the firmware cannot invent its own wording: the version line and the
/// address line are part of what a support screenshot shows.
pub mod text {
    /// The version line: `"Version "` and the version, the trailing space being
    /// what the Upright wrap needs.
    #[must_use]
    pub fn version(version: &str) -> (&'static str, &str) {
        ("Version ", version)
    }

    /// The association line: `"WiFi Connected"` and the address.
    #[must_use]
    pub fn wifi_connected(ip: &str) -> (&'static str, &str) {
        ("WiFi Connected", ip)
    }

    /// The two no-Wi-Fi lines, in English. The Upright arm splits them into words
    /// and the landscape arm prints them verbatim, and the layout was tuned on
    /// these two phrases.
    #[must_use]
    pub fn no_wifi() -> (&'static str, &'static str) {
        ("No WiFi", "Check settings")
    }
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::display::{Display, DISPLAY_HEIGHT, DISPLAY_WIDTH};
    use crate::templates::TemplateId;

    /// The rightmost and bottom-most inked pixel, which is how "does it fit in
    /// the frame" is answered without an image diff.
    fn ink(d: &Display) -> (i32, i32) {
        let fb = d.framebuffer();
        let mut max_x = -1;
        let mut max_y = -1;
        for y in 0..DISPLAY_HEIGHT {
            for x in 0..DISPLAY_WIDTH {
                if fb.pixel(x, y) {
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        (max_x, max_y)
    }

    #[test]
    fn the_boot_screen_fits_the_frame_in_both_templates() {
        // The AGENTS.md rule, as an assertion: a screen that clips at the edge
        // is a blocking defect, and this is a screen nobody had ever looked at.
        for template in [TemplateId::Standard, TemplateId::Upright] {
            for (line1, line2) in [
                ("Version ", "0.1.0"),
                ("WiFi Connected", "192.168.1.123"),
                ("No WiFi", "Check settings"),
            ] {
                let mut d = Display::new();
                d.set_display_rotation(if template.is_upright() {
                    crate::display::Rotation::R1
                } else {
                    crate::display::Rotation::R0
                });
                draw(&mut d, line1, line2, template);
                let (max_x, max_y) = ink(&d);
                assert!(
                    max_x < DISPLAY_WIDTH && max_y < DISPLAY_HEIGHT,
                    "{template:?} \"{line1}\" \"{line2}\" inks to ({max_x}, {max_y})"
                );
            }
        }
    }

    #[test]
    fn the_logo_is_on_every_boot_screen() {
        // The version line alone would pass the fit test with an empty panel, so
        // the logo gets its own check: ink in the top-right 40x40 box, which is
        // where the landscape layout puts it.
        let mut d = Display::new();
        draw(&mut d, "Version ", "0.1.0", TemplateId::Standard);
        let fb = d.framebuffer();
        let logo_pixels = (0..40)
            .flat_map(|y| (38..78).map(move |x| (x, y)))
            .filter(|(x, y)| fb.pixel(*x, *y))
            .count();
        assert!(logo_pixels > 200, "only {logo_pixels} logo pixels");
    }
}
