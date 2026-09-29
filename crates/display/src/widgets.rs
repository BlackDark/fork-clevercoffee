//! The shared drawing pieces, in one place so the six templates cannot drift apart.
//!
//! The C++ tree had the same pieces spread over `DisplayWidgets.h`, `displayHelpers.h` and each
//! template's own `render*Display()`, and the copies had diverged: `BACKFLUSH_FLUSHING`
//! re-asserted the valve where `BACKFLUSH_FILLING` did not, and the numeric columns were placed
//! with hard-coded offsets in one template and with `getStrWidth` in another. Here every piece
//! is a function that takes the framebuffer and a [`ScreenModel`], and every column offset is a
//! constant in [`crate::layout`] or a computed box width.

use crate::font::{self, Font, DEGREE};
use crate::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use crate::layout::{self, ROW_GAP, TIME_PROBE};
use crate::model::ScreenModel;
use crate::text;

/// The temperature block's geometry, shared by every template that draws one.
pub mod temp_block {
    use super::font;
    use super::Font;
    use crate::layout::NUM_PROBE;

    /// Width of the label column. Six small cells: the longest translated label in the tables is
    /// `Druck:` at five plus a colon, and `Press:` is six, so six cells hold every language
    /// without a per-language offset.
    pub const LABEL_W: i16 = 36;
    /// Width of the numeric box: `100.0`.
    pub const VALUE_W: i16 = font::probe_width(NUM_PROBE.len(), Font::Small);
    /// Width of the `°C` unit column.
    pub const UNIT_W: i16 = 2 * Font::Small.cell_width() as i16;
    /// Height of one temperature row, which is the small font's glyph box.
    pub const ROW_H: i16 = Font::Small.height() as i16;
    /// Gap between the current and the setpoint row.
    pub const ROW_GAP_PX: i16 = 3;
}

/// Draws `label` then a right-aligned value then the unit, all inside `x .. x + width`.
///
/// The value box is [`temp_block::VALUE_W`] wide whatever the value is, which is the whole point:
/// `9.0` and `10.0` and `100.0` all end at the same pixel.
pub fn temperature_row(fb: &mut Framebuffer, x: i16, y: i16, label: &str, value: Option<f64>) {
    fb.text(x, y, Font::Small, label);
    let value_x = x + temp_block::LABEL_W;
    let unit_x = value_x + temp_block::VALUE_W;
    match value {
        Some(v) => {
            let s = text::temp_c(v);
            fb.text_right_in_box(value_x, temp_block::VALUE_W, y, Font::Small, s.as_str());
            fb.text(unit_x, y, Font::Small, "\u{b0}C");
        }
        None => {
            // A missing reading is drawn as dashes in the same box. It is never drawn as a
            // number, because a number here is indistinguishable from a measurement.
            fb.text_right_in_box(value_x, temp_block::VALUE_W, y, Font::Small, "----");
            fb.text(unit_x, y, Font::Small, "\u{b0}C");
        }
    }
}

/// The status bar: a separator, then the network indicators and the uptime.
pub fn status_bar(fb: &mut Framebuffer, m: &ScreenModel, y: i16) {
    let s = m.language.strings();
    fb.hline(0, y, WIDTH as i16 / 2);
    if !m.net.offline {
        // Two indicators at fixed cells rather than at a computed offset, so a connected and a
        // disconnected machine differ by one glyph and nothing moves.
        fb.text(4, 0, Font::Small, if m.net.wifi { "W" } else { "-" });
        fb.text(11, 0, Font::Small, if m.net.mqtt { "M" } else { "-" });
    } else {
        fb.text(4, 0, Font::Small, s.offline);
    }
    // Uptime right-aligned in a seven-cell box at the right edge, so a machine that has been up
    // for a hundred hours does not slide the indicator to its left.
    const UPTIME_BOX_W: i16 = 7 * Font::Small.cell_width() as i16;
    let up = text::uptime(m.uptime_ms);
    fb.text_right_in_box(
        panel_right() - UPTIME_BOX_W,
        UPTIME_BOX_W,
        0,
        Font::Small,
        up.as_str(),
    );
}

/// The thermometer outline with a fill, the C++ `displayThermometerOutline` shape.
pub fn thermometer(fb: &mut Framebuffer, m: &ScreenModel, x: i16, y: i16, w: i16, h: i16) {
    fb.draw_frame(x, y, w, h);
    let inner_h = h - 2;
    let temp = m.temperature_c.unwrap_or(0.0);
    // Map 0..100 C onto the tube. A sensor at 200 C, which the TSIC can report before the
    // firmware's range check rejects it, must not produce a negative width.
    let clamped = temp.clamp(0.0, 100.0) as i32;
    let fill = (clamped * inner_h as i32 / 100) as i16;
    if fill > 0 {
        fb.fill_rect(x + 1, y + 1 + (inner_h - fill), w - 2, fill);
    }
}

/// A labelled horizontal bar, used for the heater output on most templates.
pub fn output_bar(
    fb: &mut Framebuffer,
    m: &ScreenModel,
    y: i16,
    h: i16,
    w: i16,
) -> layout::BarLabelCluster {
    let percent = (m.pid.output_permille / 10).min(100) as u8;
    layout::draw_output_bar(fb, percent, y, h, w)
}

/// The output bar anchored to the bottom of the panel.
///
/// Every template's bar sits here rather than at a hand-picked Y, so the bar is in the same place
/// whichever template is selected, and so a small font's seven-pixel label row cannot run off the
/// bottom of a 64-pixel panel. `w` is clamped so the bar plus its `100%` label always fit.
pub fn bottom_output_bar(fb: &mut Framebuffer, m: &ScreenModel, w: i16) -> layout::BarLabelCluster {
    let (y, h) = layout::bottom_row(Font::Small);
    let max_w = WIDTH as i16 - 2 * (3 + 4 * Font::Small.cell_width() as i16);
    widgets_output(fb, m, y, h, w.min(max_w))
}

fn widgets_output(
    fb: &mut Framebuffer,
    m: &ScreenModel,
    y: i16,
    h: i16,
    w: i16,
) -> layout::BarLabelCluster {
    output_bar(fb, m, y, h, w)
}

/// A brew timer row.
///
/// Two shapes again, for the same reason as the weight row: a translated label, a `m:ss` clock and
/// a target do not fit in 128 pixels. With a target the row shows compact seconds (`12 / 27`) in
/// three fixed cells each; without one it shows `m:ss` in five. Either way the numbers are
/// right-aligned in a box, so the field does not move as the count grows.
pub fn brew_row(fb: &mut Framebuffer, m: &ScreenModel, x: i16, y: i16) {
    if !m.features.brew_switch {
        return;
    }
    let s = m.language.strings();
    let label = if matches!(m.state, clevercoffee_domain::State::ManualFlushRunning) {
        s.manual_flush
    } else {
        s.brew
    };
    let label_w = font::str_width(label, Font::Small);
    fb.text(x, y, Font::Small, label);
    let vx = x + label_w + font::str_width(" ", Font::Small);
    match m.brew_target_ms {
        Some(total) if !m.brew_by_weight => {
            let cell = 3 * Font::Small.cell_width() as i16;
            let now = text::seconds_short(m.brew_elapsed_ms / 1000);
            fb.text_right_in_box(vx, cell, y, Font::Small, now.as_str());
            fb.text(vx + cell, y, Font::Small, "/");
            let target = text::seconds_short(total / 1000);
            fb.text_right_in_box(
                vx + cell + Font::Small.cell_width() as i16,
                cell,
                y,
                Font::Small,
                target.as_str(),
            );
        }
        _ => {
            let box_w = font::str_width(TIME_PROBE, Font::Small);
            let time = text::elapsed(m.brew_elapsed_ms);
            fb.text_right_in_box(vx, box_w, y, Font::Small, time.as_str());
        }
    }
}

/// A weight row.
///
/// Two shapes, because the label, a value, a unit and a target do not fit on 128 pixels in one
/// row at this font: with a target the unit is dropped (the label already says what it is) and
/// the two numbers share the row; without one, the unit is shown. Both shapes reserve a fixed box
/// per number, so a weight going from `9.9 g` to `10.0 g` does not move the target beside it.
pub fn weight_row(fb: &mut Framebuffer, m: &ScreenModel, x: i16, y: i16) {
    let s = m.language.strings();
    let label_w = font::str_width(s.weight, Font::Small);
    fb.text(x, y, Font::Small, s.weight);
    let vx = x + label_w + font::str_width(" ", Font::Small);
    // Five cells hold `999.9`, which is a kilogram-scale machine's whole range.
    let num_w = 5 * Font::Small.cell_width() as i16;
    match m.weight_g {
        Some(w) => {
            let short = text::weight_short(w);
            fb.text_right_in_box(vx, num_w, y, Font::Small, short.as_str());
            if let Some(target) = m.target_weight_g {
                let tx = vx + num_w + Font::Small.cell_width() as i16;
                // A longer translation, or a row that starts further right, leaves no room for
                // the target. The weight still reads correctly without it, so the target is
                // dropped rather than drawn off the panel.
                if tx + num_w <= crate::framebuffer::WIDTH as i16 {
                    fb.text(vx + num_w, y, Font::Small, "/");
                    let t = text::weight_short(target);
                    fb.text_right_in_box(tx, num_w, y, Font::Small, t.as_str());
                }
            } else {
                fb.text(vx + num_w + 1, y, Font::Small, "g");
            }
        }
        None => {
            let placeholder = if m.scale_fault { s.scale_fault } else { "----" };
            fb.text_right_in_box(vx, num_w, y, Font::Small, placeholder);
        }
    }
}

/// A pressure row, or nothing when no pressure sensor is configured.
pub fn pressure_row(fb: &mut Framebuffer, m: &ScreenModel, x: i16, y: i16) {
    if !m.features.pressure {
        return;
    }
    let s = m.language.strings();
    fb.text(x, y, Font::Small, s.pressure);
    let vx = x + font::str_width(s.pressure, Font::Small) + font::str_width(" ", Font::Small);
    let box_w = font::str_width(crate::layout::PRESSURE_PROBE, Font::Small);
    match m.pressure_bar {
        Some(p) => {
            let txt = text::pressure_bar(p);
            fb.text_right_in_box(vx, box_w, y, Font::Small, txt.as_str());
        }
        None => fb.text_right_in_box(vx, box_w, y, Font::Small, "----"),
    }
}

/// The PID row: `10.0|100.0|10.0  42%`, the C++ `displayPIDInfo` row.
pub fn pid_row(fb: &mut Framebuffer, m: &ScreenModel, x: i16, y: i16) {
    let ratio = if m.pid.ki != 0.0 {
        m.pid.kp / m.pid.ki
    } else {
        0.0
    };
    let derivative = if m.pid.kp != 0.0 {
        m.pid.kd / m.pid.kp
    } else {
        0.0
    };
    let mut gains = heapless::String::<24>::new();
    use core::fmt::Write;
    let _ = write!(gains, "{:.0}|{:.0}|{:.0}", m.pid.kp, ratio, derivative);
    fb.text(x, y, Font::Small, gains.as_str());
}

/// A centred full-screen message, used by the fault and standby screens.
pub fn centred_message(fb: &mut Framebuffer, line1: &str, line2: &str) {
    fb.text_centered(22, Font::Small, line1);
    if !line2.is_empty() {
        fb.text_centered(36, Font::Small, line2);
    }
}

/// The status separator plus content start, so a template does not re-derive it.
pub const fn content_top() -> i16 {
    layout::STATUS_SEPARATOR_Y + ROW_GAP
}

/// The bottom of the drawable area, used by templates that anchor to the panel edge.
pub const fn panel_bottom() -> i16 {
    HEIGHT as i16
}

/// The right edge of the panel, used by templates that anchor to it.
pub const fn panel_right() -> i16 {
    WIDTH as i16
}

/// The degree sign, re-exported so a template building a unit string does not import the font
/// module for one character.
pub const DEGREE_SIGN: char = DEGREE;
