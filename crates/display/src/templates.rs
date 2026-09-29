//! The six templates, and the pipeline that decides which screen a state gets.
//!
//! The pipeline is the C++ `DisplayTemplateBase::printScreen` order, unchanged, because the order
//! is behaviour: an OTA screen wins over everything (defect D01), offline mode wins over the
//! normal screen, and a fault state wins over a normal screen. The C++ version set
//! `displayBufferReady = false` in each of those branches to skip the flush; here returning from
//! [`render`] is the whole of it, because a board flushes what the framebuffer says is dirty.

use clevercoffee_domain::State;

use crate::font::{self, Font};
use crate::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use crate::layout::{self, NUM_PROBE};
use crate::model::{ScreenModel, Template};
use crate::text;
use crate::widgets::{self, temp_block};

/// Which screen a state resolved to. Public so a test can assert the pipeline's decisions
/// without parsing pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// The template's own normal screen.
    Normal,
    /// The OTA progress screen, which owns the panel.
    Ota,
    /// Offline mode, which owns the panel.
    Offline,
    /// A fullscreen fault message.
    Fault(State),
    /// The sensor-fault screen, which carries a live reading when there is one.
    SensorError,
    /// The backflush prompt.
    BackflushPrompt,
    /// The upright template's idle screen, drawn by [`render_upright`].
    UprightIdle,
}

/// Decides which screen a model gets, without drawing anything.
pub fn screen_for(m: &ScreenModel) -> Screen {
    if m.ota.is_some() {
        return Screen::Ota;
    }
    if m.net.offline {
        return Screen::Offline;
    }
    match m.state {
        State::SensorError => Screen::SensorError,
        State::EmergencyStop | State::WaterTankEmpty | State::EepromError => Screen::Fault(m.state),
        State::BackflushIdle => Screen::BackflushPrompt,
        _ => {
            if m.template == Template::Upright {
                Screen::UprightIdle
            } else {
                Screen::Normal
            }
        }
    }
}

/// Renders `m` into `fb`. The framebuffer is cleared first, as every C++ `render*Display` did.
pub fn render(fb: &mut Framebuffer, m: &ScreenModel) {
    fb.clear();
    match screen_for(m) {
        Screen::Ota => render_ota(fb, m),
        Screen::Offline => render_offline(fb, m),
        Screen::SensorError => render_sensor_error(fb, m),
        Screen::Fault(state) => render_fault(fb, m, state),
        Screen::BackflushPrompt => render_backflush_prompt(fb, m),
        Screen::UprightIdle => render_upright(fb, m),
        Screen::Normal => match m.template {
            Template::Modern => render_modern(fb, m),
            Template::Standard => render_standard(fb, m),
            Template::Minimal => render_minimal(fb, m),
            Template::Scale => render_scale(fb, m),
            Template::TemperatureOnly => render_temperature_only(fb, m),
            Template::Upright => render_upright(fb, m),
        },
    }
}

// ---------------------------------------------------------------------------
// Fullscreen screens
// ---------------------------------------------------------------------------

fn render_ota(fb: &mut Framebuffer, m: &ScreenModel) {
    let ota = m.ota.expect("screen_for only returns Ota when ota is set");
    fb.text_centered(8, Font::Small, "OTA UPDATE");
    let bar_w = 100;
    let x = (WIDTH as i16 - bar_w) / 2;
    let y = 28;
    fb.draw_frame(x, y, bar_w, layout::BAR_H);
    let fill = (ota.percent.min(100) as i32 * (bar_w - 2) as i32 / 100) as i16;
    if fill > 0 {
        fb.fill_rect(x + 1, y + 1, fill, layout::BAR_H - 2);
    }
    let pct = text::percent(ota.percent as u16 * 10);
    fb.text_centered(24, Font::Small, pct.as_str());
}

fn render_offline(fb: &mut Framebuffer, m: &ScreenModel) {
    let s = m.language.strings();
    fb.text_centered(24, Font::Small, s.offline);
}

fn render_sensor_error(fb: &mut Framebuffer, m: &ScreenModel) {
    let s = m.language.strings();
    fb.text_centered(8, Font::Small, s.sensor_error_1);
    // The reading is drawn even here, in its own fixed box, because a user diagnosing a flaky
    // sensor needs to see what the last good conversion said next to the fault.
    widgets::temperature_row(fb, 0, 26, s.current_temp, m.temperature_c);
    fb.text_centered(40, Font::Small, s.sensor_error_2);
}

fn render_fault(fb: &mut Framebuffer, m: &ScreenModel, state: State) {
    let s = m.language.strings();
    let (line1, line2) = match state {
        State::EmergencyStop => (s.emergency_stop, ""),
        State::WaterTankEmpty => (s.tank_empty, ""),
        State::EepromError => (s.eeprom_error, ""),
        // `screen_for` only produces a fault screen for these three; anything else would be a
        // bug rather than a state, and showing a wrong message would be worse than none.
        _ => (s.emergency_stop, ""),
    };
    widgets::centred_message(fb, line1, line2);
}

fn render_backflush_prompt(fb: &mut Framebuffer, m: &ScreenModel) {
    let s = m.language.strings();
    fb.text_centered(16, Font::Small, s.backflush_start_1);
    fb.text_centered(28, Font::Small, s.backflush_start_2);
    let c = layout::draw_output_bar(fb, 0, HEIGHT as i16 - 8, 8, 72);
    let _ = c;
}

// ---------------------------------------------------------------------------
// The upright template
// ---------------------------------------------------------------------------

/// The vertical layout for an upright panel.
///
/// Row map, in glyph-box tops: status bar 0, temperature 14, setpoint 24, heat bar 32, large
/// status word 40, weight 52, pressure 58, output bar 60. Every row is inside 0..64 and the
/// bands do not touch, which `tests::rows_do_not_overlap` checks.
fn render_upright(fb: &mut Framebuffer, m: &ScreenModel) {
    let s = m.language.strings();
    fb.hline(
        0,
        layout::STATUS_ROW_Y + Font::Small.height() as i16,
        WIDTH as i16 / 2,
    );
    if !m.net.offline {
        fb.text(4, 0, Font::Small, if m.net.wifi { "W" } else { "-" });
    } else {
        fb.text(4, 0, Font::Small, s.offline);
    }

    // The value block starts after the label column, so `T: 93.5` and `T: 103.5` end at the same
    // pixel.
    widgets::temperature_row(fb, 1, 14, s.current_temp, m.temperature_c);
    widgets::temperature_row(fb, 1, 24, s.set_temp, Some(m.setpoint_c));

    // The heat bar, drawn as the C++ upright template drew it: a full-width frame with a fill.
    fb.draw_frame(0, 34, WIDTH as i16, layout::BAR_H);
    let inner = WIDTH as i16 - 2;
    let fill = ((m.pid.output_permille.min(1000) as i32 * inner as i32) / 1000) as i16;
    if fill > 0 {
        fb.fill_rect(1, 35, fill, layout::BAR_H - 2);
    }

    // The large word, centred, at the large font's height.
    let word = m.upright_status();
    fb.text_centered(40, Font::Large, word);

    // One sensor row, not two. The heat bar above already shows the heater output, and a weight
    // row and a pressure row do not both fit under a fourteen-pixel status word on a 64-row
    // panel: 40 + 14 is 54, and two seven-pixel rows from 55 would need 69. Which one is shown
    // follows the C++ upright template, which also picked a single Y from the fitted features.
    if m.features.scale {
        widgets::weight_row(fb, m, 1, 55);
    } else if m.features.pressure {
        widgets::pressure_row(fb, m, 1, 55);
    }
}

// ---------------------------------------------------------------------------
// The modern template
// ---------------------------------------------------------------------------

/// The modern template: a large temperature, a status row, and a temperature-to-setpoint bar.
///
/// Row map: status bar 0, large temperature 12, status row 38, bottom bar row 54. The bottom row
/// is anchored to `HEIGHT - font height`, so it is at the same place as every other template's.
fn render_modern(fb: &mut Framebuffer, m: &ScreenModel) {
    widgets::status_bar(fb, m, layout::STATUS_SEPARATOR_Y);

    if m.shows_brew_timer() {
        render_modern_brew(fb, m);
    } else {
        render_modern_idle(fb, m);
    }
}

fn render_modern_idle(fb: &mut Framebuffer, m: &ScreenModel) {
    // Large temperature, centred as one composite block with its unit, exactly as the C++
    // `drawLargeTemperature` did: digits right-aligned in a `100.0` box, then the unit.
    let digits_w = font::str_width(NUM_PROBE, Font::Large);
    let unit_w = font::str_width("\u{b0}C", Font::Large);
    let gap = 2;
    let total = digits_w + gap + unit_w;
    let start_x = (WIDTH as i16 - total) / 2;
    let big_y = widgets::content_top() + 2;
    let value = m.temperature_c;
    match value {
        Some(v) => {
            let s = text::temp_c(v);
            fb.text_right_in_box(start_x, digits_w, big_y, Font::Large, s.as_str());
        }
        None => fb.text_right_in_box(start_x, digits_w, big_y, Font::Large, "----"),
    }
    // The unit shares the row and is vertically centred against the taller digits, which is the
    // C++ `unitY` calculation.
    let unit_y = big_y + (Font::Large.height() as i16 - Font::Small.height() as i16) / 2;
    fb.text(start_x + digits_w + gap, unit_y, Font::Small, "\u{b0}C");

    // Status row: an icon and a word, centred as a cluster.
    let (icon, word) = if m.is_ready() {
        (Icon::Ready, m.language.strings().upright_ready)
    } else {
        (Icon::Heating, m.language.strings().upright_waiting)
    };
    let icon_w = 16;
    let word_w = font::str_width(word, Font::Small);
    let cluster_x = (WIDTH as i16 - (icon_w + 4 + word_w)) / 2;
    let icon_y = 38;
    icon.draw(fb, cluster_x, icon_y);
    let text_y = icon_y + (icon_w - Font::Small.height() as i16) / 2;
    fb.text(cluster_x + icon_w + 4, text_y, Font::Small, word);

    render_modern_bottom_bar(fb, m);
}

fn render_modern_brew(fb: &mut Framebuffer, m: &ScreenModel) {
    widgets::status_bar(fb, m, layout::STATUS_SEPARATOR_Y);

    // Phase row, only while a brew is in one of its pre-infusion phases.
    let main_y = widgets::content_top();
    let s = m.language.strings();
    match m.state {
        State::BrewPreinfusion => {
            fb.text_centered(main_y, Font::Small, s.brew);
        }
        State::BrewPreinfusionPause => {
            fb.text_centered(main_y, Font::Small, s.set_temp);
        }
        _ => {}
    }

    // The time and temperature block, centred as one composite with fixed boxes.
    let time_w = font::str_width(layout::TIME_PROBE, Font::Small);
    let sep_w = font::str_width(" - ", Font::Small);
    let temp_w = font::str_width(NUM_PROBE, Font::Small);
    let block_x = (WIDTH as i16 - (time_w + sep_w + temp_w)) / 2;
    let y = main_y + 12;
    let t = text::elapsed(m.brew_elapsed_ms);
    fb.text_right_in_box(block_x, time_w, y, Font::Small, t.as_str());
    fb.text(block_x + time_w, y, Font::Small, " - ");
    match m.temperature_c {
        Some(v) => {
            let tv = text::temp_c(v);
            fb.text_right_in_box(
                block_x + time_w + sep_w,
                temp_w,
                y,
                Font::Small,
                tv.as_str(),
            );
        }
        None => fb.text_right_in_box(block_x + time_w + sep_w, temp_w, y, Font::Small, "----"),
    }

    // Weight and pressure share the footer row when both are fitted, each in its own fixed box.
    let footer_y = 43;
    if m.features.scale {
        let w = text::weight_g(m.weight_g.unwrap_or(0.0));
        fb.text(2, footer_y, Font::Small, w.as_str());
    }
    if m.features.pressure {
        let p = text::pressure_bar(m.pressure_bar.unwrap_or(0.0));
        fb.text_right_in_box(
            WIDTH as i16 - font::str_width(p.as_str(), Font::Small) - 2,
            font::str_width(p.as_str(), Font::Small),
            footer_y,
            Font::Small,
            p.as_str(),
        );
    }

    render_modern_bottom_bar(fb, m);
}

/// The temperature-to-setpoint bar with its setpoint label, the C++
/// `drawTemperatureToSetpointBar`, anchored to the bottom of the panel.
fn render_modern_bottom_bar(fb: &mut Framebuffer, m: &ScreenModel) {
    let (row_y, row_h) = layout::bottom_row(Font::Small);
    let probe = "\u{b0}C";
    let c = layout::bar_label_cluster(
        72,
        layout::BAR_H,
        row_y,
        row_h,
        Font::Small,
        3,
        "100\u{b0}C",
    );
    fb.draw_frame(c.bar_x, c.bar_y, c.bar_w, c.bar_h);
    let inner = c.bar_w - 2;
    let fill =
        layout::map_temp_to_bar_width(m.temperature_c.unwrap_or(0.0), m.setpoint_c, inner, 20);
    if fill > 0 {
        fb.fill_rect(c.bar_x + 1, c.bar_y + 1, fill, layout::BAR_H - 2);
    }
    let ready = m.setpoint_c - m.ready_threshold_c;
    if ready > 20.0 {
        let ready_x = layout::map_temp_to_bar_width(ready, m.setpoint_c, inner, 20);
        fb.vline(c.bar_x + 1 + ready_x, c.bar_y - 1, layout::BAR_H + 2);
    }
    let label = text::setpoint_unit(m.setpoint_c);
    fb.text_right_in_box(
        c.label_x,
        c.label_box_w,
        c.label_y,
        Font::Small,
        label.as_str(),
    );
    let _ = probe;
}

// ---------------------------------------------------------------------------
// The classic templates
// ---------------------------------------------------------------------------

/// Row map shared by Standard and Minimal: status 0, current temperature 16, setpoint 26, brew
/// 36, PID 47, output bar 59.
fn render_classic(fb: &mut Framebuffer, m: &ScreenModel, thermometer: bool) {
    let s = m.language.strings();
    widgets::status_bar(fb, m, layout::STATUS_SEPARATOR_Y);
    if thermometer {
        widgets::thermometer(fb, m, 2, widgets::content_top(), 20, 46);
    }
    let x = if thermometer { 26 } else { 2 };
    widgets::temperature_row(fb, x, 16, s.current_temp, m.temperature_c);
    widgets::temperature_row(fb, x, 26, s.set_temp, Some(m.setpoint_c));
    widgets::brew_row(fb, m, x, 36);
    if m.shows_brew_timer() && m.features.scale {
        widgets::weight_row(fb, m, x, 44);
    }
    widgets::pid_row(fb, m, x, 47);
    widgets::bottom_output_bar(fb, m, 96);
}

fn render_standard(fb: &mut Framebuffer, m: &ScreenModel) {
    render_classic(fb, m, true);
}

fn render_minimal(fb: &mut Framebuffer, m: &ScreenModel) {
    render_classic(fb, m, false);
}

/// Row map: status 0, temperature 16, setpoint 26, weight 36, pressure 46, output bar 58.
fn render_scale(fb: &mut Framebuffer, m: &ScreenModel) {
    let s = m.language.strings();
    widgets::status_bar(fb, m, layout::STATUS_SEPARATOR_Y);
    widgets::temperature_row(fb, 0, 16, s.current_temp, m.temperature_c);
    widgets::temperature_row(fb, 0, 26, s.set_temp, Some(m.setpoint_c));
    if m.features.brew_switch {
        widgets::brew_row(fb, m, 0, 34);
    }
    if m.features.scale {
        widgets::weight_row(fb, m, 0, 42);
    }
    widgets::pressure_row(fb, m, 0, 48);
    widgets::bottom_output_bar(fb, m, 96);
}

/// Row map: the large temperature at 8, the setpoint centred at 50, the output bar at 58.
fn render_temperature_only(fb: &mut Framebuffer, m: &ScreenModel) {
    let digits_w = font::str_width(NUM_PROBE, Font::Large);
    match m.temperature_c {
        Some(v) => {
            let s = text::temp_c(v);
            fb.text_centered(8, Font::Large, s.as_str());
        }
        None => fb.text_centered(8, Font::Large, "----"),
    }
    let target = format_target(m);
    fb.text_centered(48, Font::Small, target.as_str());
    let _ = digits_w;
    widgets::bottom_output_bar(fb, m, 96);
}

fn format_target(m: &ScreenModel) -> heapless::String<16> {
    use core::fmt::Write;
    let mut s = heapless::String::new();
    let _ = write!(s, "Target {:.0}\u{b0}C", m.setpoint_c);
    s
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

/// The two status icons the modern template uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// A flame, drawn when the machine is still heating.
    Heating,
    /// A cup, drawn when the machine is at temperature.
    Ready,
}

impl Icon {
    /// The icon's box, 16 by 16.
    pub const SIZE: i16 = 16;

    pub fn draw(&self, fb: &mut Framebuffer, x: i16, y: i16) {
        match self {
            Icon::Heating => {
                fb.fill_triangle_down(x + 1, y, 14, 12);
                fb.fill_rect(x + 5, y + 12, 6, 4);
            }
            Icon::Ready => {
                fb.draw_frame(x + 2, y + 2, 10, 9);
                fb.hline(x + 12, y + 3, 3);
                fb.hline(x + 12, y + 7, 3);
                fb.hline(x + 3, y + 13, 9);
            }
        }
    }
}

/// The width a temperature block occupies, so a template can check it fits beside a graphic.
pub fn temperature_block_width() -> i16 {
    temp_block::LABEL_W + temp_block::VALUE_W + temp_block::UNIT_W
}
