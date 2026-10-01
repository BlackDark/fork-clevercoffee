//! The six templates' `renderNormalDisplay`.
//!
//! Each is a faithful transcription of its C++ counterpart. The two that are
//! not obvious at a glance — Upright, because its coordinates live in the
//! rotated space, and Modern, because every field has a reserved width — carry
//! the most comment.

use crate::bitmaps_data as bm;
use crate::display::Display;
use crate::fmt::{format_fixed, format_int, truncate_to_i32};
use crate::font;
use crate::helpers::{is_manual_flush_state, is_near_setpoint_with_config};
use crate::lang::Lang;
use crate::model::{BrewMode, Config, DisplayInput};
use crate::templates::{modern_layout as ml, TemplateId};
use crate::widgets::{self, BrewRow, PidCoords, TemperatureCoords};

/// `StandardTemplate::renderNormalDisplay`.
///
/// The default layout, and the one with the most in it. Note the order: the
/// thermometer outline is drawn *before* the temperature bar, so the bar paints
/// over it, and the bar is gated on the blink phase so the whole bar blinks at
/// setpoint.
pub fn standard(d: &mut Display, input: &DisplayInput, config: &Config) {
    let l = crate::lang::for_language(config.language);
    d.clear_buffer();

    widgets::display_statusbar(d, config, input, l, false);
    widgets::display_temperature_info(d, input, l, &standard_temp_coords(), false);

    widgets::display_thermometer_outline(d, input, 4, 62);
    if !is_near_setpoint_with_config(input.temperature, input.setpoint, config)
        || crate::helpers::is_blink_phase_on(input)
    {
        widgets::draw_temperature_bar(d, input, 8, 30);
    }

    draw_brew_info(d, input, config, l, false, 34, 36);
    widgets::display_pid_info(d, input, &standard_pid_coords(), "|");

    widgets::display_progress_bar(d, truncate_to_i32(input.pid_output / 10.0), 30, 60, 98);
}

/// `MinimalTemplate::renderNormalDisplay`.
pub fn minimal(d: &mut Display, input: &DisplayInput, config: &Config) {
    let l = crate::lang::for_language(config.language);
    d.clear_buffer();

    widgets::display_statusbar(d, config, input, l, false);
    widgets::display_temperature_info(d, input, l, &standard_temp_coords(), false);
    draw_brew_info(d, input, config, l, false, 34, 36);
    widgets::display_progress_bar(d, truncate_to_i32(input.pid_output / 10.0), 0, 58, 128);
}

/// `TemperatureOnlyTemplate::renderNormalDisplay`.
///
/// The `Target  %.1f °C` line is the only place in the display layer that uses
/// `setCursor` + `print` of a `snprintf` result together, and the only one with a
/// two-space gap in its label. Transcribed as written.
pub fn temperature_only(d: &mut Display, input: &DisplayInput, _config: &Config) {
    d.clear_buffer();

    widgets::display_temperature(d, input, 0, 8);

    d.set_font(font::profont11());
    let mut target = crate::fmt::Formatted::new();
    target.push_str("Target  ");
    target.push_str(format_fixed(input.setpoint, 1).as_str());
    target.push(' ').ok();
    target.push('\u{b0}').ok();
    target.push_str("C");
    let target_w = d.str_width(target.as_str());
    d.set_cursor((128 - target_w) / 2, 50);
    d.print(target.as_str());

    widgets::display_progress_bar(d, truncate_to_i32(input.pid_output / 10.0), 0, 58, 128);
}

/// `ScaleTemplate::renderNormalDisplay`.
///
/// Standard plus two conditional rows. Note the different y values from the
/// Standard template: the temperature block at y=16, the brew at y=26, the
/// weight and the pressure — a nine-pixel pitch, see [`SCALE_ROW_TEMP`] —
/// against Standard's 16/36/47.
pub fn scale(d: &mut Display, input: &DisplayInput, config: &Config) {
    let l = crate::lang::for_language(config.language);
    d.clear_buffer();

    widgets::display_statusbar(d, config, input, l, false);
    widgets::display_temperature_info(d, input, l, &scale_temp_coords(), false);

    if config.brew_switch_enabled {
        draw_brew_info(d, input, config, l, false, 0, SCALE_ROW_BREW);
    }

    if config.scale_enabled {
        if crate::templates::should_display_brew_timer(input.brew_timer) {
            // `-1` is the C++'s "no target" sentinel for a `float` parameter.
            // The config value is a `f64`; narrowing to `f32` is what the C++'s
            // implicit `double -> float` conversion at the call site did.
            let target = if config.brew_mode == BrewMode::Automatic && config.brew_by_weight_enabled
            {
                narrow_f32(config.brew_by_weight_target)
            } else {
                -1.0_f32
            };
            widgets::display_brew_weight(
                d,
                l,
                0,
                SCALE_ROW_WEIGHT,
                input.brew_weight,
                target,
                input.scale_fault,
                false,
            );
        } else {
            widgets::display_brew_weight(
                d,
                l,
                0,
                SCALE_ROW_WEIGHT,
                input.weight,
                -1.0,
                input.scale_fault,
                false,
            );
        }
    }

    if config.pressure_enabled {
        widgets::display_pressure(d, input, l, 0, SCALE_ROW_PRESSURE);
    }

    widgets::display_progress_bar(d, truncate_to_i32(input.pid_output / 10.0), 0, 60, 128);
}

/// `UprightTemplate::renderNormalDisplay`.
///
/// **This renders in the R1/R3 portrait space, where the panel is 64 wide and
/// 128 tall.** Every coordinate below is in that space, and the rotation
/// transform in [`Display`] puts it into the physical 128x64 buffer. The y values
/// — 124, 110, 100 — are above 64 and are *correct*: in portrait space y runs to
/// 128. Anyone reading this against the panel size will think it is a bug.
///
/// The row order top-to-bottom in the *logical* space is: temperature (y=14),
/// heat bar (y=124), main status (y=55/60/65 depending on sensors), brew (y=34),
/// sensors (y=44/54), status bar (y=12). Note that y increases *upward* on the
/// physical panel, because R1 rotates 90 degrees.
pub fn upright(d: &mut Display, input: &DisplayInput, config: &Config) {
    let l = crate::lang::for_language(config.language);
    d.clear_buffer();

    widgets::display_temperature_info(d, input, l, &upright_temp_coords(), true);
    upright_heat_bar(d, input);
    upright_main_status(d, input, config);
    draw_brew_info(d, input, config, l, true, 1, 34);
    upright_sensor_info(d, input, config, l);
    upright_status_bar(d, input, config, l);
}

/// `UprightTemplate::displayHeatBar`.
///
/// A 64x4 frame at y=124 — the very bottom of the portrait space — with two
/// one-pixel lines inside it whose length is `pidOutput / 16.13`. That divisor is
/// not `100`: it is `(1000 - pidOutput) / 100 * 1.613`-shaped nonsense that
/// happens to make a 1000-unit output fill the bar. Reproduced verbatim, with a
/// test, because "correcting" it to 100 would visibly shorten the bar.
fn upright_heat_bar(d: &mut Display, input: &DisplayInput) {
    d.draw_frame(0, 124, 64, 4);
    let reach = truncate_to_i32(input.pid_output / 16.13) + 1;
    d.draw_line(1, 125, reach, 125);
    d.draw_line(1, 126, reach, 126);
}

/// `UprightTemplate::displayMainStatus`.
///
/// The big `fub20` status word, and a y position that depends on which sensors
/// are fitted — two rows have to fit below it. The branch is
/// `both ? 65 : (either ? 60 : 55)`, transcribed.
fn upright_main_status(d: &mut Display, input: &DisplayInput, config: &Config) {
    let scale = config.scale_enabled;
    let pressure = config.pressure_enabled;
    let y = if scale && pressure {
        65
    } else if scale || pressure {
        60
    } else {
        55
    };
    d.set_cursor(1, y);
    d.set_font(font::fub20());

    if is_manual_flush_state(input.state) {
        d.print("FLUSH");
    } else if crate::templates::should_display_brew_timer(input.brew_timer) {
        d.print("BREW");
    } else if config.backflush_reminder_enabled && input.backflush_reminder_due {
        d.print("CLEAN");
    } else if is_near_setpoint_with_config(input.temperature, input.setpoint, config)
        && crate::helpers::is_blink_phase_on(input)
    {
        d.print("OK");
    } else {
        d.print("WAIT");
    }
}

/// `UprightTemplate::displaySensorInfo`.
///
/// The weight row and the pressure row, whose y depends on whether a scale is
/// fitted so they do not collide. The weight shows the *brew* weight while a
/// brew timer is showing and the plain weight otherwise.
fn upright_sensor_info(d: &mut Display, input: &DisplayInput, config: &Config, l: &Lang) {
    if config.scale_enabled {
        if crate::templates::should_display_brew_timer(input.brew_timer) {
            let target = if config.brew_mode == BrewMode::Automatic && config.brew_by_weight_enabled
            {
                narrow_f32(config.brew_by_weight_target)
            } else {
                -1.0_f32
            };
            widgets::display_brew_weight(
                d,
                l,
                1,
                44,
                input.brew_weight,
                target,
                input.scale_fault,
                true,
            );
        } else {
            widgets::display_brew_weight(d, l, 1, 44, input.weight, -1.0, input.scale_fault, true);
        }
    }

    if config.pressure_enabled {
        let y = if config.scale_enabled { 54 } else { 44 };
        widgets::display_pressure_ur(d, input, l, 1, y);
    }
}

/// `UprightTemplate::displayStatusBar`.
///
/// The rule spans `0 .. OLED_WIDTH / 2` — half the *physical* width, 64 px,
/// which is the *full* width of the portrait space. Correct, and easy to mistake
/// for a bug: `OLED_WIDTH` is the constant 128, not the logical width.
fn upright_status_bar(d: &mut Display, input: &DisplayInput, config: &Config, l: &Lang) {
    d.draw_line(
        0,
        crate::display::STATUS_BAR_Y_POS,
        128 / 2,
        crate::display::STATUS_BAR_Y_POS,
    );
    if input.offline {
        d.set_cursor(4, 1);
        d.set_font(font::profont11());
        d.print(l.offline);
    } else {
        widgets::display_wifi_status(d, input, 4, 2, true);
        widgets::display_mqtt_status(d, config, input, 21, 0);
    }

    if config.scale_enabled && config.scale_type == crate::model::ScaleType::Bluetooth {
        widgets::display_bluetooth_status(d, input, 54, 1);
    }

    widgets::display_maintenance_status_bar(d, config, input, 54, 0);
}

/// `ModernTemplate::renderNormalDisplay`.
///
/// The dispatch is on three flags, in this order: manual flush, post-brew,
/// brewing, else idle. Note the flush check comes first and uses the *machine
/// state*, not the brew timer.
pub fn modern(d: &mut Display, input: &DisplayInput, config: &Config) {
    let l = crate::lang::for_language(config.language);
    let flushing = is_manual_flush_state(input.state);
    let screen = ml::screen_for(
        flushing,
        crate::fullscreen::is_post_brew(input.brew_timer),
        crate::templates::should_display_brew_timer(input.brew_timer),
    );

    match screen {
        ml::Screen::Brew { phase, flushing } => modern_brew(d, input, config, phase, flushing),
        ml::Screen::PostBrew => modern_post_brew(d, input),
        ml::Screen::Idle => modern_idle(d, input, config, l),
    }
}

/// `ModernTemplate::drawIdleScreen`.
fn modern_idle(d: &mut Display, input: &DisplayInput, config: &Config, l: &Lang) {
    d.clear_buffer();
    widgets::display_statusbar(d, config, input, l, false);
    modern_large_temperature(d, input);
    modern_idle_status_row(d, input, config);
    modern_setpoint_bar(d, input);
}

/// `ModernTemplateLayout::drawLargeTemperature`.
///
/// The rule-3 exemplar: the digits go in a box as wide as `getStrWidth("100.0")`
/// and are **right-aligned** inside it, and the `°C` unit is positioned relative
/// to that box — so a one-digit temperature does not move the unit, and a
/// three-digit one does not push it right.
fn modern_large_temperature(d: &mut Display, input: &DisplayInput) {
    let f20 = font::fub20();
    let f17 = font::profont17();

    let temp = format_fixed(input.temperature, 1);
    let mut unit = crate::fmt::Formatted::new();
    unit.push('\u{b0}').ok();
    unit.push_str("C");

    d.set_font(f20);
    let digits_box_w = f20.str_width("100.0");

    d.set_font(f17);
    let unit_w = f17.str_width(unit.as_str());
    let unit_y = ml::K_IDLE_TEMP_Y + (ml::K_FONT_HEIGHT_FUB20 - ml::K_FONT_HEIGHT_PROFONT17) / 2;
    let total_w = digits_box_w + ml::K_ROW_GAP + unit_w;
    let start_x = (128 - total_w) / 2;

    d.set_font(f20);
    crate::layout::draw_str_right_in_box(
        d,
        start_x,
        digits_box_w,
        ml::K_IDLE_TEMP_Y,
        temp.as_str(),
    );

    d.set_font(f17);
    d.draw_str(
        start_x + digits_box_w + ml::K_ROW_GAP,
        unit_y,
        unit.as_str(),
    );
}

/// `ModernTemplateLayout::drawTemperatureToSetpointBar`.
///
/// The bottom row: a `72 x 4` bar with a fill, a "ready" tick, and the setpoint
/// label. The label is **right-aligned in a box as wide as `getStrWidth("100°C")`**
/// — the same rule-3 mechanism as the temperature above.
fn modern_setpoint_bar(d: &mut Display, input: &DisplayInput) {
    let f10 = font::profont10();
    if input.setpoint <= f64::from(ml::K_TEMP_MAP_MIN) {
        return;
    }

    d.set_font(f10);
    let layout = crate::layout::layout_bar_label_cluster(
        d,
        crate::layout::BarBox {
            w: ml::K_BOTTOM_BAR_W,
            h: ml::K_BOTTOM_BAR_H,
        },
        crate::layout::LabelBox {
            row_top_y: ml::K_BOTTOM_ROW_Y,
            row_h: ml::K_BOTTOM_ROW_H,
            font_h: ml::K_FONT_HEIGHT_PROFONT10,
            gap: ml::K_BOTTOM_BAR_LABEL_GAP,
        },
        ml::K_SETPOINT_PROBE,
    );

    d.draw_frame(
        layout.bar_x,
        layout.bar_y,
        ml::K_BOTTOM_BAR_W,
        ml::K_BOTTOM_BAR_H,
    );

    let inner_w = ml::K_BOTTOM_BAR_W - 2;
    let fill_w = ml::map_temp_to_bar_width(input.temperature, input.setpoint, inner_w);
    if fill_w > 0 {
        d.draw_box(
            layout.bar_x + 1,
            layout.bar_y + 1,
            fill_w,
            ml::K_BOTTOM_BAR_H - 2,
        );
    }

    // The "ready" tick, five degrees below setpoint. It is drawn one row above
    // and below the bar (`bar_y - 1`, `bar_y + bar_h + 1`), which is why it is
    // the one element in the Modern layout that is *taller* than the row it
    // belongs to -- and therefore the one the fit assertion has to allow for.
    let ready_temp = input.setpoint - f64::from(crate::helpers::HEATING_LOGO_THRESHOLD_C);
    if ready_temp > f64::from(ml::K_TEMP_MAP_MIN) {
        let ready_x = ml::map_temp_to_bar_width(ready_temp, input.setpoint, inner_w);
        d.draw_v_line(
            layout.bar_x + 1 + ready_x,
            layout.bar_y - 1,
            ml::K_BOTTOM_BAR_H + 2,
        );
    }

    let mut target = crate::fmt::Formatted::new();
    target.push_str(format_fixed(input.setpoint, 0).as_str());
    target.push('\u{b0}').ok();
    target.push_str("C");
    let probe_w = f10.str_width(ml::K_SETPOINT_PROBE);
    crate::layout::draw_str_right_in_box(
        d,
        layout.label_x,
        probe_w,
        layout.label_y,
        target.as_str(),
    );
}

/// `ModernTemplate::drawIdleStatusRow`.
///
/// The 16x16 HEATING/READY icon plus its label, centred as a unit, and blanked
/// during the off half of the blink when the temperature is at setpoint.
fn modern_idle_status_row(d: &mut Display, input: &DisplayInput, config: &Config) {
    let temp_c = input.temperature;
    let setpoint_c = input.setpoint;
    let is_ready = ml::is_ready_for_brew(temp_c, setpoint_c);
    let at_setpoint = is_near_setpoint_with_config(temp_c, setpoint_c, config);
    let blink_off = is_ready && at_setpoint && !crate::helpers::is_blink_phase_on(input);

    let status: &str = if is_manual_flush_state(input.state) {
        "FLUSHING"
    } else if is_ready {
        "READY"
    } else {
        "HEATING"
    };

    if blink_off {
        return;
    }

    d.set_font(font::profont11());
    let y = ml::K_IDLE_STATUS_ROW_Y;
    let text_y = y + (ml::K_IDLE_ICON_SIZE - ml::K_FONT_HEIGHT_PROFONT11) / 2;

    if is_manual_flush_state(input.state) {
        let text_w = d.str_width(status);
        d.draw_str((128 - text_w) / 2, text_y, status);
        return;
    }

    let text_w = d.str_width(status);
    let total_w = ml::K_IDLE_ICON_SIZE + ml::K_IDLE_ICON_GAP + text_w;
    let x = (128 - total_w) / 2;

    if is_ready {
        modern_ready_icon(d, x, y);
    } else {
        modern_heating_icon(d, x, y);
    }
    d.draw_str(
        x + ml::K_IDLE_ICON_SIZE + ml::K_IDLE_ICON_GAP,
        text_y,
        status,
    );
}

/// `ModernTemplateLayout::drawHeatingIcon` — a flame.
fn modern_heating_icon(d: &mut Display, x: i32, y: i32) {
    d.draw_triangle(x + 8, y, x + 3, y + 12, x + 13, y + 12);
    d.draw_box(x + 5, y + 12, 6, 3);
    d.draw_pixel(x + 8, y + 4);
}

/// `ModernTemplateLayout::drawReadyIcon` — a cup.
fn modern_ready_icon(d: &mut Display, x: i32, y: i32) {
    d.draw_frame(x + 3, y + 2, 9, 8);
    d.draw_line(x + 12, y + 3, x + 15, y + 3);
    d.draw_line(x + 12, y + 7, x + 15, y + 7);
    d.draw_line(x + 4, y + 14, x + 11, y + 14);
}

/// `ModernTemplate::drawBrewScreen`.
fn modern_brew(
    d: &mut Display,
    input: &DisplayInput,
    config: &Config,
    phase: ml::BrewPhase,
    flushing: bool,
) {
    d.clear_buffer();
    modern_phase_indicator(d, phase, flushing);
    modern_brew_main_readout(d, input);
    if !flushing {
        modern_brew_progress_bar(d, input, config);
    }
    modern_brew_footer(d, input, config);
}

/// `ModernTemplate::drawPhaseIndicator`.
///
/// Three labels at fixed x, separated by `>`, with the active one inverted.
/// The `>` is drawn at `xs[i] - 8`, i.e. 8 px to the left of the next label, and
/// only for `i > 0`.
fn modern_phase_indicator(d: &mut Display, phase: ml::BrewPhase, flushing: bool) {
    d.set_font(font::profont10());
    if flushing {
        d.draw_str(2, 1, "FLUSHING");
    } else {
        for i in 0..3 {
            if i > 0 {
                d.draw_str(ml::BrewPhase::XS[i] - 8, 1, ">");
            }
            if i == phase.index() {
                let w = d.str_width(ml::BrewPhase::LABELS[i]);
                d.draw_box(ml::BrewPhase::XS[i] - 1, 0, w + 2, 10);
                d.set_draw_color(0);
                d.draw_str(ml::BrewPhase::XS[i], 1, ml::BrewPhase::LABELS[i]);
                d.set_draw_color(1);
            } else {
                d.draw_str(ml::BrewPhase::XS[i], 1, ml::BrewPhase::LABELS[i]);
            }
        }
    }
    d.draw_line(0, 12, 128, 12);
}

/// `ModernTemplate::drawBrewMainReadout`.
///
/// Three fixed-width fields — `999 s`, `" - "`, `100.0°C` — whose *combined*
/// block is centred once, and each of which is right-aligned inside its own box.
/// This is the rule-3 mechanism at its clearest: the block never moves, and the
/// digits inside each field never move.
fn modern_brew_main_readout(d: &mut Display, input: &DisplayInput) {
    let f17 = font::profont17();
    let time_box_w = f17.str_width("999 s");
    let sep_w = f17.str_width(" - ");
    let temp_box_w = f17.str_width(ml::K_DEGREE_C_PROBE);
    let block_w = time_box_w + sep_w + temp_box_w;
    let block_x = (128 - block_w) / 2;
    let y = ml::K_BREW_MAIN_Y;

    let mut time = crate::fmt::Formatted::new();
    time.push_str(format_int(truncate_to_i32(input.brew_time_ms / 1000.0)).as_str());
    time.push(' ').ok();
    time.push('s').ok();

    let mut temp = crate::fmt::Formatted::new();
    temp.push_str(format_fixed(input.temperature, 1).as_str());
    temp.push('\u{b0}').ok();
    temp.push_str("C");

    d.set_font(f17);
    crate::layout::draw_str_right_in_box(d, block_x, time_box_w, y, time.as_str());
    d.draw_str(block_x + time_box_w, y, " - ");
    crate::layout::draw_str_right_in_box(
        d,
        block_x + time_box_w + sep_w,
        temp_box_w,
        y,
        temp.as_str(),
    );
}

/// `ModernTemplate::drawBrewProgressBar`.
///
/// An 88 px bar — wider than the idle bar's 72, because it also has to make room
/// for its label — with the target label right-aligned in a box as wide as the
/// wider of the two target probes. With no target the label is omitted and the
/// bar is centred on its own.
fn modern_brew_progress_bar(d: &mut Display, input: &DisplayInput, config: &Config) {
    let f10 = font::profont10();
    let mut fill_width = 0;
    let mut has_target = false;
    let mut target_label = crate::fmt::Formatted::new();
    let mut label_probe = ml::K_BREW_TARGET_TIME_PROBE;

    if config.brew_mode == BrewMode::Automatic && config.brew_by_weight_enabled {
        let target = config.brew_by_weight_target;
        if target > 0.0 {
            let percent = crate::widgets::constrain(
                truncate_to_i32(f64::from(input.brew_weight) / target * 100.0),
                0,
                100,
            );
            fill_width = crate::widgets::constrain(
                crate::widgets::arduino_map(percent, 0, 100, 0, ml::K_BREW_BAR_W - 2),
                0,
                ml::K_BREW_BAR_W - 2,
            );
            let _ = target_label.push('/');
            target_label.push(' ').ok();
            target_label.push_str(format_fixed(target, 0).as_str());
            target_label.push('g').ok();
            has_target = true;
            label_probe = ml::K_BREW_TARGET_WEIGHT_PROBE;
        }
    }

    if !has_target && config.brew_mode == BrewMode::Automatic && config.brew_by_time_enabled {
        let target = input.target_brew_time_ms;
        if target > 0.0 {
            let percent = crate::widgets::constrain(
                truncate_to_i32(input.brew_time_ms / target * 100.0),
                0,
                100,
            );
            fill_width = crate::widgets::constrain(
                crate::widgets::arduino_map(percent, 0, 100, 0, ml::K_BREW_BAR_W - 2),
                0,
                ml::K_BREW_BAR_W - 2,
            );
            let _ = target_label.push('/');
            target_label.push(' ').ok();
            target_label.push_str(format_int(truncate_to_i32(target / 1000.0)).as_str());
            target_label.push('s').ok();
            has_target = true;
        }
    }

    d.set_font(f10);
    let layout = if has_target {
        crate::layout::layout_bar_label_cluster(
            d,
            crate::layout::BarBox {
                w: ml::K_BREW_BAR_W,
                h: ml::K_BOTTOM_BAR_H,
            },
            crate::layout::LabelBox {
                row_top_y: ml::K_BOTTOM_ROW_Y,
                row_h: ml::K_BOTTOM_ROW_H,
                font_h: ml::K_FONT_HEIGHT_PROFONT10,
                gap: ml::K_BOTTOM_BAR_LABEL_GAP,
            },
            label_probe,
        )
    } else {
        // No label: the bar is centred on its own, and there is no label to
        // right-align, so `label_x` is meaningless and never read.
        crate::layout::BarLabelCluster {
            bar_x: (128 - ml::K_BREW_BAR_W) / 2,
            bar_y: ml::K_BOTTOM_ROW_Y + (ml::K_BOTTOM_ROW_H - ml::K_BOTTOM_BAR_H) / 2,
            label_x: 0,
            label_y: 0,
        }
    };

    d.draw_frame(
        layout.bar_x,
        layout.bar_y,
        ml::K_BREW_BAR_W,
        ml::K_BOTTOM_BAR_H,
    );
    if has_target && fill_width > 0 {
        d.draw_box(
            layout.bar_x + 1,
            layout.bar_y + 1,
            fill_width,
            ml::K_BOTTOM_BAR_H - 2,
        );
    }
    if has_target {
        let probe_w = f10.str_width(label_probe);
        crate::layout::draw_str_right_in_box(
            d,
            layout.label_x,
            probe_w,
            layout.label_y,
            target_label.as_str(),
        );
    }
}

/// `ModernTemplate::drawBrewFooter`.
///
/// Weight and pressure on one line, at a fixed x and pitch. The x advances by
/// the *rendered* width of what was just drawn, so with neither sensor enabled
/// the line is empty.
fn modern_brew_footer(d: &mut Display, input: &DisplayInput, config: &Config) {
    d.set_font(font::profont10());
    let mut x_pos = 4;

    if config.scale_enabled {
        let mut buf = crate::fmt::Formatted::new();
        if config.brew_mode == BrewMode::Automatic
            && config.brew_by_weight_enabled
            && config.brew_by_weight_target > 0.0
        {
            buf.push_str(format_fixed(f64::from(input.brew_weight), 1).as_str());
            buf.push('g').ok();
            buf.push('/').ok();
            buf.push_str(format_fixed(config.brew_by_weight_target, 0).as_str());
            buf.push('g').ok();
        } else {
            buf.push_str(format_fixed(f64::from(input.brew_weight), 1).as_str());
            buf.push(' ').ok();
            buf.push('g').ok();
        }
        d.draw_str(x_pos, ml::K_BREW_FOOTER_Y, buf.as_str());
        x_pos += d.str_width(buf.as_str()) + 10;
    }

    if config.pressure_enabled {
        let mut buf = crate::fmt::Formatted::new();
        buf.push_str(format_fixed(f64::from(input.pressure), 1).as_str());
        buf.push(' ').ok();
        buf.push_str("bar");
        d.draw_str(x_pos, ml::K_BREW_FOOTER_Y, buf.as_str());
    }
}

/// `ModernTemplate::drawPostBrewScreen`.
///
/// The 40x40 cup at y=2, then the brew time as `SS.s` right-aligned in a
/// `999.9`-wide box, centred as a block with its `" s"` unit. The block is
/// centred once — `startX` — and the digits are right-aligned inside it, so
/// `9.8 s` and `99.8 s` produce the same block position and the same unit x.
fn modern_post_brew(d: &mut Display, input: &DisplayInput) {
    d.clear_buffer();

    let cup_x = (128 - ml::BREW_CUP_LOGO_W) / 2;
    d.draw_xbmp(
        cup_x,
        ml::BREW_CUP_Y,
        ml::BREW_CUP_LOGO_W,
        ml::BREW_CUP_LOGO_H,
        &bm::BREW_CUP_LOGO,
    );

    let brew_time_ms = truncate_to_i32(input.brew_time_ms);
    let mut time = crate::fmt::Formatted::new();
    time.push_str(format_int(brew_time_ms / 1000).as_str());
    time.push('.').ok();
    time.push_str(format_int((brew_time_ms % 1000) / 100).as_str());

    let f17 = font::profont17();
    d.set_font(f17);
    let time_y = ml::BREW_CUP_Y + ml::BREW_CUP_LOGO_H + 4;
    let digits_w = f17.str_width("999.9");
    let unit_w = f17.str_width(" s");
    let total_w = digits_w + unit_w;
    let start_x = (128 - total_w) / 2;

    crate::layout::draw_str_right_in_box(d, start_x, digits_w, time_y, time.as_str());
    d.draw_str(start_x + digits_w, time_y, " s");
}

/// `displayBrewInfo` for the five non-Modern templates.
///
/// Drawn at `(base_x, base_y)` with the label from `l`; the row itself is chosen
/// by [`widgets::brew_row`]. `upright` selects the field layout, not the row.
fn draw_brew_info(
    d: &mut Display,
    input: &DisplayInput,
    config: &Config,
    l: &Lang,
    upright: bool,
    base_x: i32,
    base_y: i32,
) {
    if !config.brew_switch_enabled {
        return;
    }
    match widgets::brew_row(config, input) {
        BrewRow::None => {}
        BrewRow::ManualFlush { elapsed_ms } => {
            widgets::display_brew_time(
                d,
                base_x,
                base_y,
                l.manual_flush,
                elapsed_ms,
                -1.0,
                upright,
            );
        }
        BrewRow::HotWater { elapsed_ms } => {
            widgets::display_brew_time(d, base_x, base_y, l.hot_water, elapsed_ms, -1.0, upright);
        }
        BrewRow::Brew {
            elapsed_ms,
            target_ms,
        } => {
            widgets::display_brew_time(d, base_x, base_y, l.brew, elapsed_ms, target_ms, upright);
        }
    }
}

/// Narrow a `f64` config value to the `f32` the weight widget takes.
///
/// The C++ passes a `double` config value into a `float` parameter, which
/// narrows implicitly. The weight is at most a few hundred grams, so the
/// narrowing is exact for every value the device can hold, and the range is
/// checked so a corrupt config cannot produce a negative target.
fn narrow_f32(value: f64) -> f32 {
    if value.is_finite() && value >= f64::from(f32::MIN) && value <= f64::from(f32::MAX) {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the range is checked; a weight in grams is far inside f32"
        )]
        return value as f32;
    }
    -1.0
}

/// `StandardTemplate::getTemperatureCoords` / Minimal's (identical).
fn standard_temp_coords() -> TemperatureCoords {
    TemperatureCoords {
        current_temp_x: 34,
        current_temp_y: 16,
        current_value_x: 84,
        set_temp_x: 34,
        set_temp_y: 26,
        set_value_x: 84,
    }
}

/// `StandardTemplate::getPIDCoords` / Minimal's.
fn standard_pid_coords() -> PidCoords {
    PidCoords {
        pid_x: 38,
        pid_y: 47,
        output_x: 96,
        output_y: 47,
    }
}

/// `ScaleTemplate::getTemperatureCoords`. Same x, but the value column is 50
/// rather than 84 — the value is at the right of a 128 px panel instead of
/// past it.
fn scale_temp_coords() -> TemperatureCoords {
    TemperatureCoords {
        current_temp_x: 0,
        current_temp_y: SCALE_ROW_TEMP,
        current_value_x: 50,
        set_temp_x: 0,
        set_temp_y: SCALE_ROW_SET,
        set_value_x: 50,
    }
}

// The Scale template's five content rows.
//
// **Re-pitched from 16/26/26/36/46.** The C++ puts the setpoint row and the brew
// row both at `y = 26` (`ScaleTemplate.h:24,59-61`), and the brew row's inverted
// field is `78 x 10` at `(x + 50, y + 1)` (`DisplayWidgets.h:170-171`) — so it
// erases the setpoint's label, value and `°C`. The collision is not hidden by a
// flag: `display.fullscreen_brew_timer` defaults to **false** in both firmwares, so
// on a Scale-template machine the setpoint simply disappears for the whole
// duration of a brew.
//
// A ten-pixel pitch cannot hold five rows above the progress bar at y=60: the last
// row's ink would end at 58-61 and touch it. A **nine**-pixel pitch fits, and the
// rows start at 13 so the last ends at 58, two clear of the bar.
//
// The cost is real and is the point of recording it: the C++'s row positions
// move, so `scale*.ppm` changes and the screen looks different. What does *not*
// change is that nothing is clipped and nothing is erased.
const SCALE_ROW_TEMP: i32 = 13;
const SCALE_ROW_SET: i32 = 22;
const SCALE_ROW_BREW: i32 = 31;
const SCALE_ROW_WEIGHT: i32 = 40;
const SCALE_ROW_PRESSURE: i32 = 49;

/// `UprightTemplate::getTemperatureCoords`. The value column is `base_x + 50`
/// against `base_x = 1`, i.e. 51.
fn upright_temp_coords() -> TemperatureCoords {
    TemperatureCoords {
        current_temp_x: 1,
        current_temp_y: 14,
        current_value_x: 51,
        set_temp_x: 1,
        set_temp_y: 24,
        set_value_x: 51,
    }
}

/// Draw one of the six templates' normal layouts.
pub fn dispatch(template: TemplateId, d: &mut Display, input: &DisplayInput, config: &Config) {
    match template {
        TemplateId::Standard => standard(d, input, config),
        TemplateId::Minimal => minimal(d, input, config),
        TemplateId::TemperatureOnly => temperature_only(d, input, config),
        TemplateId::Scale => scale(d, input, config),
        TemplateId::Upright => upright(d, input, config),
        TemplateId::Modern => modern(d, input, config),
    }
}
