//! The row map, and the two layout rules the C++ tree learned the hard way.
//!
//! Every Y in this crate is the **top of the glyph box** (`setFontPosTop()`, which is what
//! `ModernTemplate` documented in a comment at the top of its layout namespace), so a text row at
//! `y` with font `f` occupies `y .. y + f.height()`. The row map below is therefore checkable: two
//! rows overlap if and only if their bands intersect, and `tests::rows_do_not_overlap` checks that
//! mechanically for every template.
//!
//! The two rules, both from `CLAUDE.md` and both enforced by [`str_width`]-based boxes rather
//! than by eye:
//!
//! 1. A counting field gets a **fixed pixel width** reserved with the widest expected string, and
//!    the live value is drawn right-aligned inside it ([`Framebuffer::text_right_in_box`]). A
//!    value going from `9` to `10` therefore cannot shift the digits to its left.
//! 2. A bar and its label share a **vertical midline**: both are centred in the same row band
//!    ([`BarLabelCluster`]), so the label's optical centre and the bar's centre agree.

use crate::font::{self, Font};
use crate::framebuffer::{Framebuffer, HEIGHT, WIDTH};

/// The widest string any numeric field is allowed to show.
///
/// Five characters covers `100.0`, a three-digit temperature with one decimal, which is the widest
/// a sane setpoint reaches; the config schema caps the setpoint well below that.
pub const NUM_PROBE: &str = "100.0";
/// Seconds with a unit, for a brew timer: `999 s`.
pub const TIME_PROBE: &str = "999 s";
/// Grams with a unit: `999 g`.
pub const WEIGHT_PROBE: &str = "999 g";
/// Bar with a unit: `9.9 bar`.
pub const PRESSURE_PROBE: &str = "9.9 bar";

/// Gap between two rows, in pixels. The C++ templates used 2 px between the status bar and the
/// first content row.
pub const ROW_GAP: i16 = 2;

/// Where the status bar's separator line sits, below the status row.
pub const STATUS_ROW_Y: i16 = 0;
/// The separator, one pixel under the status row's glyph box.
pub const STATUS_SEPARATOR_Y: i16 = STATUS_ROW_Y + Font::Small.height() as i16 + 1;

/// The bar height used by every template's heater output bar.
pub const BAR_H: i16 = 4;

/// A bar and the label beside it, already placed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BarLabelCluster {
    pub bar_x: i16,
    pub bar_y: i16,
    pub bar_w: i16,
    pub bar_h: i16,
    pub label_x: i16,
    pub label_y: i16,
    /// Pixel width reserved for the label, so the value does not move as digits change.
    pub label_box_w: i16,
}

/// Places a bar and its label as one horizontal cluster, centred on the panel, with both
/// vertically centred in `row_y .. row_y + row_h`.
///
/// `label_probe` is the widest string the label will ever hold; the cluster is centred using that
/// width rather than the live one, so the bar does not slide left and right as a temperature
/// gains a digit.
pub fn bar_label_cluster(
    bar_w: i16,
    bar_h: i16,
    row_y: i16,
    row_h: i16,
    label_font: Font,
    gap: i16,
    label_probe: &str,
) -> BarLabelCluster {
    let label_box_w = font::str_width(label_probe, label_font);
    let label_h = label_font.height() as i16;
    let cluster_w = bar_w + gap + label_box_w;
    let cluster_x = ((WIDTH as i16 - cluster_w) / 2).max(0);
    // Both elements are aligned to the *same* vertical centre rather than each being centred
    // independently. Centring each independently is what the C++ `layoutBarLabelCluster` did and
    // it puts the two centrelines a pixel apart whenever the row height and the two heights have
    // different parities, which is most rows.
    let centre = row_y + row_h / 2;
    BarLabelCluster {
        bar_x: cluster_x,
        bar_y: centre - bar_h / 2,
        bar_w,
        bar_h,
        label_x: cluster_x + bar_w + gap,
        label_y: centre - label_h / 2,
        label_box_w,
    }
}

/// Where the label's glyph box sits relative to the bar's, so a test can compare midlines.
pub fn bar_midline(c: &BarLabelCluster) -> i16 {
    c.bar_y + c.bar_h / 2
}

/// The label's vertical centre.
pub fn label_midline(c: &BarLabelCluster, label_font: Font) -> i16 {
    c.label_y + label_font.height() as i16 / 2
}

/// The bottom strip every template anchors to, so a bar at the bottom of one screen is at the
/// bottom of the next and the panel does not appear to jump between templates.
pub fn bottom_row(font: Font) -> (i16, i16) {
    let h = font.height() as i16;
    (HEIGHT as i16 - h, h)
}

/// Maps a temperature to a fill width between a floor and the setpoint, the way the C++ Modern
/// template's `mapTempToBarWidth` did.
///
/// Below the floor, or with no setpoint, the bar is empty rather than a full negative width.
pub fn map_temp_to_bar_width(temp_c: f64, setpoint_c: f64, inner_w: i16, floor_c: i16) -> i16 {
    if setpoint_c <= floor_c as f64 || inner_w <= 0 {
        return 0;
    }
    let t = temp_c as i32;
    let lo = floor_c as i32;
    let hi = (setpoint_c as i32).max(lo);
    let clamped = t.clamp(lo, hi);
    // Integer map, so the width is reproducible and the test can assert an exact pixel count.
    ((clamped - lo) * inner_w as i32 / (hi - lo).max(1)) as i16
}

/// Draws the heater output bar with its frame, its fill and, when the machine is close enough,
/// the ready marker. Returns the cluster it used, so a test can check the midline.
pub fn draw_output_bar(
    fb: &mut Framebuffer,
    percent: u8,
    row_y: i16,
    row_h: i16,
    bar_w: i16,
) -> BarLabelCluster {
    let c = bar_label_cluster(bar_w, BAR_H, row_y, row_h, Font::Small, 3, "100%");
    fb.draw_frame(c.bar_x, c.bar_y, c.bar_w, c.bar_h);
    let inner = c.bar_w - 2;
    let fill = (percent.min(100) as i32 * inner as i32 / 100) as i16;
    if fill > 0 {
        fb.fill_rect(c.bar_x + 1, c.bar_y + 1, fill, c.bar_h - 2);
    }
    let mut buf = heapless::String::<8>::new();
    let _ = core::fmt::Write::write_fmt(&mut buf, format_args!("{}%", percent));
    fb.text_right_in_box(
        c.label_x,
        c.label_box_w,
        c.label_y,
        Font::Small,
        buf.as_str(),
    );
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cluster_centres_on_the_panel() {
        let c = bar_label_cluster(72, BAR_H, 54, 10, Font::Small, 3, "100%");
        let right = c.label_x + c.label_box_w;
        let left_margin = c.bar_x;
        let right_margin = WIDTH as i16 - right;
        assert!(
            (left_margin - right_margin).abs() <= 1,
            "the cluster must be centred: left {left_margin}, right {right_margin}, {c:?}"
        );
        assert!(c.bar_x >= 0 && right <= WIDTH as i16);
    }

    #[test]
    fn a_bar_and_its_label_share_a_vertical_midline() {
        for row_y in [0, 10, 54] {
            for row_h in [8, 10, 12] {
                let c = bar_label_cluster(72, BAR_H, row_y, row_h, Font::Small, 3, "100%");
                assert_eq!(
                    bar_midline(&c),
                    label_midline(&c, Font::Small),
                    "bar and label midlines differ for row_y={row_y} row_h={row_h}: {c:?}"
                );
            }
        }
    }

    #[test]
    fn a_value_gaining_a_digit_keeps_its_right_edge() {
        // The reason the label is measured with the probe and the value is right-aligned inside
        // that box: the cluster does not move, and the value's right edge does not either.
        let c = bar_label_cluster(72, BAR_H, 54, 10, Font::Small, 3, "100%");
        let mut right_edges = heapless::Vec::<i16, 3>::new();
        for value in ["9%", "10%", "100%"] {
            let mut fb = crate::framebuffer::Framebuffer::new();
            fb.text_right_in_box(c.label_x, c.label_box_w, c.label_y, Font::Small, value);
            assert_eq!(fb.clipped(), 0, "{value} overflows the reserved label box");
            let right = (c.label_x..c.label_x + c.label_box_w).rev().find(|x| {
                (0..font::GLYPH_H as i16).any(|dy| fb.get(*x as u16, (c.label_y + dy) as u16))
            });
            let _ = right_edges.push(right.expect("the value is drawn"));
        }
        assert!(
            right_edges.iter().all(|x| *x == right_edges[0]),
            "the right edge moved as the digit count changed: {right_edges:?}"
        );
    }

    #[test]
    fn the_bottom_row_ends_on_the_last_pixel() {
        let (y, h) = bottom_row(Font::Small);
        assert_eq!(y + h, HEIGHT as i16);
        let (y, h) = bottom_row(Font::Large);
        assert_eq!(y + h, HEIGHT as i16);
    }

    #[test]
    fn temperature_maps_to_a_bounded_width() {
        assert_eq!(
            map_temp_to_bar_width(20.0, 0.0, 70, 20),
            0,
            "no setpoint, no bar"
        );
        assert_eq!(map_temp_to_bar_width(20.0, 90.0, 70, 20), 0, "at the floor");
        assert_eq!(
            map_temp_to_bar_width(90.0, 90.0, 70, 20),
            70,
            "at the setpoint"
        );
        assert_eq!(
            map_temp_to_bar_width(500.0, 90.0, 70, 20),
            70,
            "clamped above"
        );
        let mid = map_temp_to_bar_width(55.0, 90.0, 70, 20);
        assert!((30..=40).contains(&mid), "half way is about half: {mid}");
    }

    #[test]
    fn the_status_separator_sits_below_the_status_row() {
        assert!(STATUS_SEPARATOR_Y > STATUS_ROW_Y + Font::Small.height() as i16 - 1);
        assert!(STATUS_SEPARATOR_Y < HEIGHT as i16);
    }

    #[test]
    fn an_output_bar_stays_on_the_panel() {
        let mut fb = Framebuffer::new();
        let c = draw_output_bar(&mut fb, 100, 54, 10, 72);
        assert!(c.bar_x >= 0 && c.bar_x + c.bar_w <= WIDTH as i16);
        assert_eq!(fb.clipped(), 0);
    }
}
