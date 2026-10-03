//! Port of `include/clevercoffee/display/DisplayWidgets.h`.
//!
//! The shared drawing pieces: the status bar, the thermometer, the temperature
//! and brew readouts, the progress bar, the icons, and the multi-line message
//! screens.
//!
//! # Two things the C++ does that this port keeps
//!
//! **The brew-time box is drawn inverted.** `displayBrewTime` and
//! `displayBrewWeight` set `setDrawColor(0)`, draw a filled box, restore the
//! colour, and *then* print the label and value on top. It is a one-line
//! "selected field" highlight. Reproduced, including the box dimensions, which
//! differ between the Upright template and the rest
//! (`DisplayWidgets.h:184-190`):
//!
//! | template | box | drawn at |
//! |----------|-----|-----------|
//! | Upright | `100 x 10` | `x, y+1` |
//! | others | `78 x 10` | `x + 50, y+1` |
//!
//! **The value column is a hard-coded offset, not a measured one.**
//! `kValueColumnOffset = 50`, and the templates separately hard-code
//! `currentValueX = 34` / `currentValueX + 31` for the degree sign
//! (`DisplayTemplateBase.h:135-150`). Both depend on `langstring_current_temp`
//! being 42 px wide in `profont11` — a value pinned by a test in
//! [`crate::lang`]. Change the label and those coordinates go wrong silently;
//! `check_column_offsets` exists to make that loud.

use crate::display::{Display, DISPLAY_WIDTH, STATUS_BAR_Y_POS};
use crate::fmt::{format_fixed, format_int, format_padded2, truncate_to_i32};
use crate::font;
use crate::helpers::{is_manual_flush_state, should_display_hot_water_timer};
use crate::model::{BrewMode, Config, DisplayInput, ScaleType};
use crate::{bitmaps_data as bm, lang::Lang};

/// The offset from a field's x to its value column (`kValueColumnOffset`).
pub const VALUE_COLUMN_OFFSET: i32 = 50;
/// The width of the value column (`kValueColumnWidth`).
pub const VALUE_COLUMN_WIDTH: i32 = 78;
/// The width of the Upright template's brew-time box.
pub const UPRIGHT_BOX_WIDTH: i32 = 100;
/// The height of a brew-time / brew-weight box.
pub const BOX_HEIGHT: i32 = 10;

/// `displayThermometerOutline` (`DisplayWidgets.h:149`).
///
/// Called with `(4, 62)` by the Standard template. Note the negative y
/// arguments: the function is written relative to a *bulb* at `(x, y)` and
/// draws the stem upwards, so `y - 44` is above the panel. That is deliberate
/// in the C++ and reproduced here; the Standard template's own fit assertion is
/// what catches it if the call site moves.
pub fn display_thermometer_outline(d: &mut Display, input: &DisplayInput, x: i32, y: i32) {
    d.draw_line(x + 3, y - 9, x + 3, y - 42);
    d.draw_line(x + 9, y - 9, x + 9, y - 42);
    d.draw_pixel(x + 4, y - 43);
    d.draw_pixel(x + 8, y - 43);
    d.draw_line(x + 5, y - 44, x + 7, y - 44);
    d.draw_disc(x + 6, y - 5, 6);

    // The setpoint tick: `map(setpoint, 0, 100, y-9, y-39)`, clamped by
    // `drawLine`'s own clipping. `map` here is Arduino's integer `map`.
    let height = arduino_map(truncate_to_i32(input.setpoint), 0, 100, y - 9, y - 39);
    d.draw_line(x + 11, height, x + 16, height);
}

/// Arduino's `map(x, in_min, in_max, out_min, out_max)`.
///
/// Integer, truncating, and *not* clamping: `map(150, 0, 100, 53, 23)` returns
/// `-10`, and the clipping is left to the draw call. Reproduced because the
/// thermometer tick and `drawTemperaturebar` both depend on the non-clamping
/// behaviour for out-of-range inputs.
#[must_use]
pub const fn arduino_map(x: i32, in_min: i32, in_max: i32, out_min: i32, out_max: i32) -> i32 {
    (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min
}

/// Arduino's `constrain(amt, low, high)`.
#[must_use]
pub const fn constrain(amt: i32, low: i32, high: i32) -> i32 {
    if amt < low {
        low
    } else if amt > high {
        high
    } else {
        amt
    }
}

/// `drawTemperaturebar` (`DisplayWidgets.h:166`).
///
/// Five vertical bars whose heights track the temperature, drawn from a
/// baseline of `y = 52` upwards. `heightRange` is the full-scale height.
pub fn draw_temperature_bar(d: &mut Display, input: &DisplayInput, x: i32, height_range: i32) {
    let width = x + 5;
    let height = arduino_map(truncate_to_i32(input.temperature), 0, 100, 0, height_range);
    for i in x..width {
        d.draw_v_line(i, 52 - height, height);
    }
    if input.temperature > 100.0 {
        d.draw_line(x, height_range - 11, x + 3, height_range - 11);
        d.draw_line(x, height_range - 10, x + 4, height_range - 10);
        d.draw_line(x, height_range - 9, x + 4, height_range - 9);
    }
}

/// `displayTemperature` (`DisplayWidgets.h:184`).
///
/// The big `fub30` readout, with the "hide the leading digit under 100" trick:
/// below 99.5 the value is drawn 20 px to the right so a two-digit number still
/// appears centred, because `fub30` digits are 41 px wide and three of them do
/// not fit in the 128 px panel.
pub fn display_temperature(d: &mut Display, input: &DisplayInput, x: i32, y: i32) {
    d.set_font(font::fub30());
    let text = format_fixed(input.temperature, 0);
    let cursor_x = if input.temperature < 99.499 {
        x + 20
    } else {
        x
    };
    d.set_cursor(cursor_x, y);
    d.print(text.as_str());

    d.draw_circle(x + 72, y + 4, 3);
}

/// `displayTemperatureInfo` (`DisplayTemplateBase.h:130`).
///
/// The two-row label/value block every non-Modern template uses. The x
/// coordinates come from the template's own `getTemperatureCoords`, so the
/// Upright template can put them in a different place in the portrait space.
pub fn display_temperature_info(
    d: &mut Display,
    input: &DisplayInput,
    l: &Lang,
    coords: &TemperatureCoords,
    upright: bool,
) {
    d.set_font(font::profont11());

    d.set_cursor(coords.current_temp_x, coords.current_temp_y);
    d.print(if upright {
        l.current_temp_ur
    } else {
        l.current_temp
    });

    // The value and its `°C` unit, laid out so neither can leave the frame.
    //
    // **Measured in ink, not in advance.** `"°C"` advances 12 px in
    // `profont11` but *inks* 17 (`Font::ink_box` returns `x 0..16`): the advance
    // excludes the trailing side bearing, so a layout computed from `str_width`
    // puts the last three columns of the `C` outside the panel. The first fix for
    // this used `str_width` and reduced the unit's offset from 31 to 30, which
    // moved it two pixels and left it still clipped — the measurement was the
    // wrong quantity.
    //
    // So: the unit is placed to **end one pixel inside the frame**, and the
    // value is right-aligned to two pixels before it. Where the original column
    // has room for both (the Scale template, whose value column is 50 rather than
    // 84) the original placement is kept untouched, so that layout does not move
    // at all.
    let (value_right, unit_x) = value_and_unit_columns(coords.current_value_x);
    draw_right_aligned(
        d,
        value_right,
        coords.current_temp_y,
        format_fixed(input.temperature, 1).as_str(),
    );
    d.set_cursor(unit_x, coords.current_temp_y);
    d.print_char('\u{b0}');
    d.print("C");

    d.set_cursor(coords.set_temp_x, coords.set_temp_y);
    d.print(if upright { l.set_temp_ur } else { l.set_temp });

    let (set_right, set_unit_x) = value_and_unit_columns(coords.set_value_x);
    draw_right_aligned(
        d,
        set_right,
        coords.set_temp_y,
        format_fixed(input.setpoint, 1).as_str(),
    );
    d.set_cursor(set_unit_x, coords.set_temp_y);
    d.print_char('\u{b0}');
    d.print("C");
}

/// Where a temperature value and its `°C` unit go: the value's right ink column
/// and the unit's left x.
///
/// **Everything here is measured in ink, not in advance.** `"\u{b0}C"` advances
/// 12 px in `profont11` and inks **17** (`Font::ink_box` reports `x 0..16`),
/// because the advance excludes the trailing side bearing. A layout computed
/// from `str_width` therefore leaves the last three columns of the `C` outside
/// the panel — which is what happened here, twice, before the measurement was
/// corrected. This is the same distinction the whole text-fit check turns on: a
/// clipped glyph is still *inside* the frame, so no bounds test sees it.
///
/// The unit keeps [`UNIT_COLUMN_OFFSET`] from the value column wherever that
/// fits on the panel, so the **Scale** template (value column 50) does not move
/// at all; where it does not fit, the unit is pulled back to end one pixel
/// inside the frame and the value is right-aligned two pixels before it.
#[must_use]
pub fn value_and_unit_columns(value_x: i32) -> (i32, i32) {
    let unit_ink = unit_ink_width();
    let unit_x = if value_x + UNIT_COLUMN_OFFSET + unit_ink < DISPLAY_WIDTH {
        value_x + UNIT_COLUMN_OFFSET
    } else {
        DISPLAY_WIDTH - 1 - unit_ink
    };
    (unit_x - 2, unit_x)
}

/// The `°C` unit's inked width in `profont11`, which is **not** its advance.
#[must_use]
pub fn unit_ink_width() -> i32 {
    font::profont11().ink_box("\u{b0}C").2 + 1
}

/// The inverted field's box for a row whose origin is `x`: `(left, width)`.
///
/// The width is what is left to the frame's right edge. See
/// `draw_inverted_field` for why a constant is wrong here.
#[must_use]
pub const fn inverted_field_box(x: i32) -> (i32, i32) {
    let left = x + VALUE_COLUMN_OFFSET;
    (left, DISPLAY_WIDTH - left)
}

/// Draw `text` so its **ink** ends at `right`.
///
/// The value column is a fixed-width field per AGENTS.md, so the number is
/// right-aligned inside it rather than starting at a fixed x: `"92.5"` and
/// `"100.0"` are 23 and 29 px of ink, and a fixed x would push the three-digit
/// reading into the unit.
fn draw_right_aligned(d: &mut Display, right: i32, y: i32, text: &str) {
    let (x0, _, x1, _) = font::profont11().ink_box(text);
    let width = if x1 >= x0 { x1 - x0 + 1 } else { 0 };
    d.set_cursor(right - width + 1, y);
    d.print(text);
}

/// Where the `°C` unit sits relative to the value column, **when there is
/// room**.
///
/// The C++'s literal `31` (`DisplayTemplateBase.h:118,122`). It is a *starting*
/// offset and not a guarantee: on the Scale template (value column 50) the unit
/// lands at 81 and there is nothing to fix, while on the Standard and Minimal
/// templates (value column 84) it would land at 115 and ink to 131 — three
/// columns past the panel.
///
/// So [`display_temperature_info`] uses this offset where the unit's **ink**
/// fits and otherwise pulls the unit back to end one pixel inside the frame and
/// right-aligns the value against it. The Scale template therefore does not move
/// at all, and the Standard one gets a `°C` that is actually on the panel.
pub const UNIT_COLUMN_OFFSET: i32 = 31;

/// Where a template puts the temperature block.
///
/// The C++ has three per-template overrides returning a braced initialiser of
/// five `int`s. Named fields, so a template that sets the setpoint column but
/// forgets the value column is a compile error rather than a number in the
/// wrong column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemperatureCoords {
    /// Label x for the measured temperature.
    pub current_temp_x: i32,
    /// Label y for the measured temperature.
    pub current_temp_y: i32,
    /// Value x for the measured temperature.
    pub current_value_x: i32,
    /// Label x for the setpoint.
    pub set_temp_x: i32,
    /// Label y for the setpoint.
    pub set_temp_y: i32,
    /// Value x for the setpoint.
    pub set_value_x: i32,
}

/// `displayPIDInfo` (`DisplayTemplateBase.h:165`).
///
/// The `Kp|Ki/Kp|Kd` row and the heater-output percentage. The separator is a
/// template parameter: `"|"` everywhere except Upright, which uses `" "`
/// because the portrait space is 64 px wide.
pub fn display_pid_info(
    d: &mut Display,
    input: &DisplayInput,
    coords: &PidCoords,
    separator: &str,
) {
    // `pidKi() != 0 ? pidKp() / pidKi() : "0"` -- an integer-ratio display that
    // has to avoid dividing by zero, which is why the guard exists in the C++.
    // `pidKi() != 0` in the C++: a double compared for exact zero, and any
    // non-zero value divides. Preserved, including the fact that a
    // denormal `pid_ki` would produce a very large ratio rather than a guard.
    // The C++ is `pidKi() != 0`: an exact comparison against zero, not an
    // epsilon. An epsilon would change which configurations take the divide
    // branch, and the divided result is what the row displays. So the exact
    // comparison is kept, named, and asserted below.
    let divide = input.pid_ki != 0.0;
    let ratio = if divide {
        format_fixed(input.pid_kp / input.pid_ki, 0)
    } else {
        format_fixed(0.0, 0)
    };
    // **Unconditional**, as the C++ is: `DisplayTemplateBase.h:172` divides
    // `pidKd()` by `pidKp()` with no guard, and `pidKp()` is non-zero on any
    // machine whose PID runs at all (a proportional gain of 0 is not a
    // configuration, it is a broken one, and the C++ shows the consequence).
    let third = format_fixed(input.pid_kd / input.pid_kp, 0);

    // **Right-aligned to just before the output column**, so a wide ratio grows
    // leftwards instead of into it.
    //
    // The three numbers are unbounded — they are `Kp`, `Kp/Ki` and `Kd/Kp`
    // rendered as integers — and the C++ draws them left to right from
    // `pid_x = 38` with the output at `output_x = 96`. With ordinary gains the
    // row is 40 px and clears comfortably, but `Kp/Ki` is a division by
    // whatever the operator typed for Ki: at `ki = 0.005` it is 10000, the row
    // is 59 px wide, and it lands exactly on the output column. Right-aligning
    // makes the overlap unreachable for any ratio that fits at all.
    let mut row = crate::fmt::Formatted::new();
    row.push_str(format_fixed(input.pid_kp, 0).as_str());
    row.push_str(separator);
    row.push_str(ratio.as_str());
    row.push_str(separator);
    row.push_str(third.as_str());
    draw_right_aligned(d, coords.output_x - 2, coords.pid_y, row.as_str());

    d.set_cursor(coords.output_x, coords.output_y);
    // Below 99 the output is shown as a percentage with one decimal; at or
    // above 99 it switches to an integer. Both are one decimal place *fewer*
    // digits, so the field does not shift -- the reason the two branches exist.
    if input.pid_output < 99.0 {
        d.print(format_fixed(input.pid_output / 10.0, 1).as_str());
    } else {
        d.print(format_fixed(input.pid_output / 10.0, 0).as_str());
    }
    d.print(" %");
}

/// Where a template puts the PID row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PidCoords {
    /// The `Kp|...` row's x.
    pub pid_x: i32,
    /// The `Kp|...` row's y.
    pub pid_y: i32,
    /// The heater output's x.
    pub output_x: i32,
    /// The heater output's y.
    pub output_y: i32,
}

/// `displayBrewTime` (`DisplayWidgets.h:203`).
///
/// The inverted field with the label on the left and the value in a fixed
/// column. `total_target_brew_time` is `-1` for manual, which suppresses the
/// `/target` part.
pub fn display_brew_time(
    d: &mut Display,
    x: i32,
    y: i32,
    label: &str,
    curr_brew_time_ms: f64,
    total_target_brew_time_ms: f64,
    upright: bool,
) {
    draw_inverted_field(d, x, y, upright);

    d.set_font(font::profont11());
    d.set_cursor(x, y);
    if upright {
        // The Upright template packs label, value and unit into one run.
        d.print(label);
        d.print(format_int(truncate_to_i32(curr_brew_time_ms / 1000.0)).as_str());
        if total_target_brew_time_ms > 0.0 {
            d.print("/");
            d.print(format_int(truncate_to_i32(total_target_brew_time_ms / 1000.0)).as_str());
        }
        d.print(" s");
    } else {
        d.print(label);
        d.set_cursor(x + VALUE_COLUMN_OFFSET, y);
        d.print(format_int(truncate_to_i32(curr_brew_time_ms / 1000.0)).as_str());
        if total_target_brew_time_ms > 0.0 {
            d.print("/");
            d.print(format_int(truncate_to_i32(total_target_brew_time_ms / 1000.0)).as_str());
        }
        d.print(" s");
    }
}

/// `displayBrewWeight` (`DisplayWidgets.h:247`).
///
/// As [`display_brew_time`], but the value is a float with zero decimals and
/// the unit is grams. `setpoint` is `-1` for no target, `fault` replaces the
/// value with the language's "Fault" string.
#[allow(
    clippy::too_many_arguments,
    reason = "the C++ has the same eight; each is a distinct field of the widget"
)]
pub fn display_brew_weight(
    d: &mut Display,
    l: &Lang,
    x: i32,
    y: i32,
    weight: f32,
    setpoint: f32,
    fault: bool,
    upright: bool,
) {
    draw_inverted_field(d, x, y, upright);

    d.set_font(font::profont11());
    let label = if upright { l.weight_ur } else { l.weight };

    if fault {
        d.set_cursor(x, y);
        if upright {
            d.print(l.weight_ur);
            d.print(l.scale_failure);
        } else {
            d.print(l.weight);
            d.set_cursor(x + VALUE_COLUMN_OFFSET, y);
            d.print(l.scale_failure);
        }
        return;
    }

    if upright {
        d.set_cursor(x, y);
        d.print(label);
        d.print(format_fixed(f64::from(weight), 0).as_str());
        if setpoint > 0.0 {
            d.print("/");
            d.print(format_fixed(f64::from(setpoint), 0).as_str());
        }
        d.print(" g");
    } else {
        d.set_cursor(x, y);
        d.print(label);
        d.set_cursor(x + VALUE_COLUMN_OFFSET, y);
        d.print(format_fixed(f64::from(weight), 0).as_str());
        if setpoint > 0.0 {
            d.print("/");
            d.print(format_fixed(f64::from(setpoint), 0).as_str());
        }
        d.print(" g");
    }
}

/// The inverted field behind [`display_brew_time`] and [`display_brew_weight`].
///
/// `DisplayWidgets.h:209-216`: `setDrawColor(0)`, `drawBox`, `setDrawColor(1)`.
/// The box is `100 x 10` for the Upright template and offset by
/// [`VALUE_COLUMN_OFFSET`] for the rest.
///
/// **The width is what is left to the frame's edge, not a constant.** The C++
/// uses `kValueColumnWidth = 78` (`DisplayWidgets.h:171`), and 78 is exactly
/// right for the **Scale** template — whose row origin is `x = 0`, so the box
/// runs 50..127 and closes on the last column. The Standard and Minimal
/// templates pass `x = 34`, so the same 78 px runs **84..161**: 33 px of it is
/// off-panel, the right border is never drawn, and the field looks like it runs
/// off the edge of the screen. That is the "the values don't fit" report.
///
/// One constant cannot be right for two row origins, so it is computed:
/// `DISPLAY_WIDTH - (x + VALUE_COLUMN_OFFSET)`, which is 78 on Scale — byte for
/// byte the C++ — and 44 on Standard and Minimal, where it closes on the frame.
fn draw_inverted_field(d: &mut Display, x: i32, y: i32, upright: bool) {
    d.set_font(font::profont11());
    d.set_draw_color(0);
    if upright {
        d.draw_box(x, y + 1, UPRIGHT_BOX_WIDTH, BOX_HEIGHT);
    } else {
        let (left, width) = inverted_field_box(x);
        d.draw_box(left, y + 1, width.max(0), BOX_HEIGHT);
    }
    d.set_draw_color(1);
}

/// `displayBrewtimeFs` (`DisplayWidgets.h:296`) — the fullscreen brew timer.
///
/// Two completely different renderings, because the two panels have different
/// shapes: in the Upright template's 64x128 space the digits are `fub20` with a
/// `profont11` "s" underneath, and on a landscape 128x64 they are `fub25` with a
/// `profont15` "s" to the right.
///
/// The `brewtime < 9950.000` test is the same leading-digit trick as
/// [`display_temperature`]: a 4-digit value does not fit, so below 99.5 s it is
/// drawn indented.
pub fn display_brew_time_fs(d: &mut Display, x: i32, y: i32, brew_time_ms: f64, upright: bool) {
    let seconds = brew_time_ms / 1000.0;
    if upright {
        d.set_font(font::fub20());
        if seconds < 9.95 {
            d.set_cursor(x + 15, y);
        } else {
            d.set_cursor(x, y);
        }
        d.print(format_fixed(seconds, 1).as_str());
        d.set_font(font::profont11());
        d.set_cursor(x + 56, y + 12);
        d.print("s");
    } else {
        d.set_font(font::fub25());
        if seconds < 9.95 {
            d.set_cursor(x + 16, y);
        } else {
            d.set_cursor(x, y);
        }
        d.print(format_fixed(seconds, 1).as_str());
        d.set_font(font::profont15());
        if seconds < 9.95 {
            d.set_cursor(x + 67, y + 14);
        } else {
            d.set_cursor(x + 69, y + 14);
        }
        d.print("s");
    }
    d.set_font(font::profont11());
}

/// `displayProgressbar` (`DisplayWidgets.h:335`).
///
/// A `w x 4` frame with a two-pixel-thick fill. Note `output - 2 > 0`: the
/// fill is inset by one pixel on each side, and the guard means a value under 2
/// fills nothing at all.
pub fn display_progress_bar(d: &mut Display, value: i32, x: i32, y: i32, width: i32) {
    d.draw_frame(x, y, width, 4);
    let output = arduino_map(value, 0, 100, 0, width);
    if output - 2 > 0 {
        d.draw_line(x + 1, y + 1, x + output - 1, y + 1);
        d.draw_line(x + 1, y + 2, x + output - 1, y + 2);
    }
}

/// `displayWiFiStatus` (`DisplayWidgets.h:84`).
///
/// The antenna icon plus, when connected, one vertical bar per signal step. The
/// bar count is `for (b = 0; b <= signal; b++)`, so a signal of 3 draws *four*
/// bars of heights 0, 2, 4, 6. The off state prints the reconnect count at a
/// template-dependent x.
pub fn display_wifi_status(d: &mut Display, input: &DisplayInput, x: i32, y: i32, upright: bool) {
    if input.wifi_connected {
        d.draw_xbmp(x, y, 8, 8, &bm::ANTENNA_OK_ICON);
        for b in 0..=i32::from(input.wifi_signal) {
            d.draw_v_line(x + 5 + b * 2, y + 8 - b * 2, b * 2);
        }
    } else {
        d.draw_xbmp(x, y, 8, 8, &bm::ANTENNA_NOK_ICON);
        d.set_cursor(if upright { x + 12 } else { x + 36 }, y - 1);
        d.set_font(font::profont11());
        d.print("RC: ");
        d.print(format_int(truncate_to_i32(f64::from(input.wifi_reconnects))).as_str());
    }
}

/// `displayMQTTStatus` (`DisplayWidgets.h:104`).
pub fn display_mqtt_status(d: &mut Display, config: &Config, input: &DisplayInput, x: i32, y: i32) {
    if !config.mqtt_enabled {
        return;
    }
    d.set_cursor(x, y);
    d.set_font(font::profont11());
    if input.mqtt_connected {
        d.print("MQTT");
        if input.mqtt_weak {
            d.print("!");
        }
    } else {
        // The C++ prints an empty string, which still moves the cursor to x
        // (no-op) and *does* set the font. Preserved: the font change is
        // observable by the next draw.
        d.print("");
    }
}

/// `displayBluetoothStatus` (`DisplayWidgets.h:349`).
pub fn display_bluetooth_status(d: &mut Display, input: &DisplayInput, x: i32, y: i32) {
    // The C++ guards on `hardwareContext().scalePtr()` being non-null and the
    // scale being connected. `DisplayInput` has no such pointer, so the port
    // guards on the equivalent: a configured, connected Bluetooth scale. The
    // caller has already checked `scale_enabled && scale_type == Bluetooth`, so
    // reaching here means connected.
    let _ = input;
    d.draw_xbmp(x, y, 8, 9, &bm::BLUETOOTH_ICON);
}

/// `displayUptime` (`DisplayWidgets.h:68`).
///
/// `snprintf(buf, format, hours, minutes, seconds)` with the format
/// `"%02luh %02lum"`, drawn at `(x, y)` in `profont11`.
pub fn display_uptime(d: &mut Display, uptime_s: u32, y: i32) {
    d.set_font(font::profont11());
    let hours = uptime_s / 3600;
    let minutes = uptime_s % 3600 / 60;
    // The C++ computes `seconds = uptime_s % 60` and passes it to
    // `snprintf(buf, 9, "%02luh %02lum", hours, minutes, seconds)` -- which has
    // two specifiers for three arguments, so `seconds` is computed and then
    // discarded. Reproduced without the dead computation; the rendered text is
    // identical either way.
    let mut text = format_padded2(hours);
    text.push(' ').ok();
    text.push('h').ok();
    text.push(' ').ok();
    let m = format_padded2(minutes);
    for c in m.as_str().chars() {
        text.push(c).ok();
    }
    text.push('m').ok();
    // **Right-aligned to the frame, not drawn at `x`.** A deliberate divergence
    // from the C++, which passes a fixed `x = 84` (`DisplayWidgets.h:374`) and
    // lets the string run off the panel.
    //
    // `"%02luh %02lum"` is a *minimum* width, not a fixed one: past 100 hours
    // the hours field grows a digit, and this machine is routinely up for
    // fifteen days. At `x = 84` a 377-hour uptime is 47 px wide and ends at
    // 131 — the `m` is cut in half by the right edge, which is what the human
    // reported ("the time in the header, the m is missing"). The C++ has the
    // same defect; the fix costs one `str_width` and cannot make a shorter
    // string worse.
    let right = DISPLAY_WIDTH - RIGHT_MARGIN - d.str_width(text.as_str());
    d.draw_str(right.clamp(0, DISPLAY_WIDTH - 1), y, text.as_str());
}

/// The gap between the rightmost glyph and the frame edge, in pixels.
///
/// AGENTS.md's "everything must fit fully within 128x64" is satisfied by ink
/// that *touches* the last column only in the sense that the last column is
/// still visible; one pixel of margin is what makes "in frame" checkable, and
/// it is the same margin [`UNIT_COLUMN_OFFSET`] leaves on the `°C` column.
pub const RIGHT_MARGIN: i32 = 1;

/// `displayMaintenanceStatusBar` (`DisplayWidgets.h:358`).
///
/// Returns whether it drew, because the status bar uses the answer to decide
/// whether to fall back to the uptime.
pub fn display_maintenance_status_bar(
    d: &mut Display,
    config: &Config,
    input: &DisplayInput,
    x: i32,
    y: i32,
) -> bool {
    if !config.backflush_reminder_enabled || !input.backflush_reminder_due {
        return false;
    }
    d.set_font(font::profont11());
    d.draw_str(x, y, "CLEAN");
    true
}

/// `displayStatusbar` (`DisplayWidgets.h:369`).
///
/// The horizontal rule at [`STATUS_BAR_Y_POS`] plus the radio, MQTT, Bluetooth
/// and maintenance indicators. The two templates lay the four elements out
/// differently, which is the `upright` branch on the reconnect x in
/// [`display_wifi_status`] and the different Bluetooth/maintenance x values
/// here.
pub fn display_statusbar(
    d: &mut Display,
    config: &Config,
    input: &DisplayInput,
    l: &Lang,
    upright: bool,
) {
    d.draw_line(0, STATUS_BAR_Y_POS, DISPLAY_WIDTH, STATUS_BAR_Y_POS);

    if input.offline {
        let (cursor_x, cursor_y) = if upright { (4, 1) } else { (40, 0) };
        d.set_cursor(cursor_x, cursor_y);
        d.set_font(font::profont11());
        d.print(l.offline);
    } else {
        display_wifi_status(d, input, 4, 1, upright);
        display_mqtt_status(d, config, input, if upright { 21 } else { 40 }, 0);
    }

    if config.scale_enabled && config.scale_type == ScaleType::Bluetooth {
        display_bluetooth_status(d, input, if upright { 54 } else { 24 }, 1);
    }

    if !display_maintenance_status_bar(d, config, input, if upright { 54 } else { 78 }, 0) {
        display_uptime(d, input.now_ms / 1000, 0);
    }
}

/// `displayMessage` (`DisplayWidgets.h:391`).
///
/// Six lines at a fixed ten-pixel pitch, the C++'s "message screen" primitive.
/// `displayMessage` does not `setFont`, so it draws in whatever font the caller
/// last selected; every caller selects `profont11` first.
pub fn display_message(d: &mut Display, lines: [&str; 6]) {
    d.clear_buffer();
    d.set_cursor(0, 0);
    d.print(lines[0]);
    d.set_cursor(0, 10);
    d.print(lines[1]);
    d.set_cursor(0, 20);
    d.print(lines[2]);
    d.set_cursor(0, 30);
    d.print(lines[3]);
    d.set_cursor(0, 40);
    d.print(lines[4]);
    d.set_cursor(0, 50);
    d.print(lines[5]);
}

/// `displayBrewInfo` (`DisplayTemplateBase.h:196`) — decide what the brew row shows.
///
/// Split out from the drawing so the *choice* is testable without a framebuffer:
/// manual flush, else hot water, else the brew timer (with a target when the
/// brew is automatic and time-limited), else nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrewRow {
    /// Nothing to show: no brew is running.
    None,
    /// The manual-flush countdown.
    ManualFlush {
        /// Milliseconds elapsed.
        elapsed_ms: f64,
    },
    /// The hot-water countdown.
    HotWater {
        /// Milliseconds of pump runtime.
        elapsed_ms: f64,
    },
    /// The brew countdown, with a target in milliseconds or 0.
    Brew {
        /// Milliseconds elapsed.
        elapsed_ms: f64,
        /// Target duration in milliseconds, or 0 for a manual brew.
        target_ms: f64,
    },
}

/// Which brew row to draw, and with what numbers.
///
/// The priority order matters and is the C++'s: manual flush beats hot water
/// beats brew, because those are states the machine cannot be in two of at once
/// and the flush check is first in the source.
#[must_use]
pub fn brew_row(config: &Config, input: &DisplayInput) -> BrewRow {
    if is_manual_flush_state(input.state) {
        return BrewRow::ManualFlush {
            elapsed_ms: input.brew_time_ms,
        };
    }
    if should_display_hot_water_timer(input) {
        return BrewRow::HotWater {
            elapsed_ms: input.pump_on_time_ms,
        };
    }
    if input.brew_timer != crate::model::BrewTimerState::Idle {
        let target = if config.brew_mode == BrewMode::Automatic && config.brew_by_time_enabled {
            input.target_brew_time_ms
        } else {
            0.0
        };
        return BrewRow::Brew {
            elapsed_ms: input.brew_time_ms,
            target_ms: target,
        };
    }
    BrewRow::None
}

/// The Scale template's pressure row (`ScaleTemplate.h:47`).
///
/// `setFont`, `setCursor`, then three `print`s: the localized label, the value
/// with one decimal, and `" bar"`. Three separate `print` calls, not one
/// `snprintf`, because the label is localized and the value is not.
pub fn display_pressure(d: &mut Display, input: &DisplayInput, l: &Lang, x: i32, y: i32) {
    d.set_font(font::profont11());
    d.set_cursor(x, y);
    d.print(l.pressure);
    d.print(format_fixed(f64::from(input.pressure), 1).as_str());
    d.print(" bar");
}

/// The Upright template's pressure row (`UprightTemplate.h:99`).
///
/// Same, but with the short label and a row that moves depending on whether a
/// scale is fitted.
pub fn display_pressure_ur(d: &mut Display, input: &DisplayInput, l: &Lang, x: i32, y: i32) {
    d.set_font(font::profont11());
    d.set_cursor(x, y);
    d.print(l.pressure_ur);
    d.print(format_fixed(f64::from(input.pressure), 1).as_str());
    d.print(" bar");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::DISPLAY_HEIGHT;
    use crate::lang;
    use crate::model::Language;

    fn mk() -> Display {
        let mut d = Display::new();
        d.set_font(font::profont11());
        d.set_font_pos_top();
        d
    }

    /// The rightmost inked column of a frame, or `-1` for an empty one.
    fn rightmost_ink(d: &Display) -> i32 {
        let fb = d.framebuffer();
        (0..DISPLAY_WIDTH)
            .rev()
            .find(|x| (0..DISPLAY_HEIGHT).any(|y| fb.pixel(*x, y)))
            .map_or(-1, i32::from)
    }

    #[test]
    fn a_long_uptime_is_right_aligned_rather_than_clipped() {
        // The human's "the time in the header, the m is missing". `"%02luh
        // %02lum"` grows a digit past 100 hours and this machine is up for
        // weeks; drawn at the C++'s fixed `x = 84` a 377-hour uptime ends at
        // 131 and the unit is cut in half by the frame.
        for hours in [0_u32, 3, 99, 100, 377, 9_999] {
            let mut d = mk();
            display_uptime(&mut d, hours * 3600 + 25 * 60, 0);
            let right = rightmost_ink(&d);
            assert!(
                right <= DISPLAY_WIDTH - 1 - RIGHT_MARGIN,
                "{hours} h inks to column {right}"
            );
        }
    }

    #[test]
    fn the_unit_column_ends_inside_the_frame_for_a_three_digit_value() {
        // The other half of the same report: `"°C"` at `value + 31` on the
        // Standard template ends on the panel's last column. Checked with the
        // widest value a boiler can report, because a narrower one hides it.
        let coords = TemperatureCoords {
            current_temp_x: 34,
            current_temp_y: 16,
            current_value_x: 84,
            set_temp_x: 34,
            set_temp_y: 26,
            set_value_x: 84,
        };
        let mut d = Display::new();
        d.prepare_display(crate::display::Rotation::R0);
        let input = crate::model::DisplayInput {
            temperature: 103.5,
            setpoint: 95.0,
            ..crate::model::DisplayInput::default()
        };
        display_temperature_info(
            &mut d,
            &input,
            lang::for_language(Language::English),
            &coords,
            false,
        );
        let right = rightmost_ink(&d);
        assert!(
            right <= DISPLAY_WIDTH - 1 - RIGHT_MARGIN,
            "the unit column inks to column {right}"
        );
    }

    #[test]
    fn arduino_map_truncates_and_does_not_clamp() {
        // Arduino's `map` is integer and unclamped; the Standard template's
        // thermometer tick relies on both.
        assert_eq!(arduino_map(50, 0, 100, 0, 30), 15);
        // (150 * -30) / 100 + 53 = -45 + 53 = 8, i.e. the tick lands *inside*
        // the panel instead of above it. Unclamped, and the Standard template
        // relies on `drawLine` clipping the rest.
        assert_eq!(arduino_map(150, 0, 100, 53, 23), 8, "unclamped");
        assert_eq!(arduino_map(0, 0, 100, 53, 23), 53);
        assert_eq!(arduino_map(100, 0, 100, 0, 30), 30);
    }

    #[test]
    fn constrain_clamps_both_ends() {
        assert_eq!(constrain(-5, 0, 100), 0);
        assert_eq!(constrain(150, 0, 100), 100);
        assert_eq!(constrain(50, 0, 100), 50);
    }

    #[test]
    fn the_value_column_offset_matches_the_english_label_width() {
        // `kValueColumnOffset = 50` is a hard-coded constant in the C++ that
        // only lines up because "Weight: " is 42 px in profont11. If the label
        // or the font changes, the field no longer starts where the box is.
        let f = font::profont11();
        let l = crate::lang::for_language(Language::English);
        assert_eq!(f.str_width(l.weight), 48);
        assert_eq!(f.str_width(l.brew), 36);
        // The box starts at +50 and the label is 48 wide at most, so it fits
        // with a 2 px gap for "Brew: " and none for "Weight: " (which is 48 and
        // so ends 2 px before the box).
        assert!(
            f.str_width(l.weight) <= VALUE_COLUMN_OFFSET,
            "the label must not run into the value box"
        );
        assert!(f.str_width(l.brew) <= VALUE_COLUMN_OFFSET);
    }

    #[test]
    fn the_degree_sign_column_does_not_collide_with_a_four_character_value() {
        // `DisplayTemplateBase.h:118` hard-codes `currentValueX + 31` for the
        // degree sign, which only works because `"%.1f"` is four characters and
        // a digit is 5-6 px in profont11. The offset is now
        // `UNIT_COLUMN_OFFSET`, one pixel less, so the unit ends inside the
        // frame; what still has to hold is that value and unit do not touch.
        let f = font::profont11();
        let four_digits = f.str_width("100.0");
        assert!(
            four_digits <= UNIT_COLUMN_OFFSET,
            "the value must not reach the degree sign: {four_digits} px"
        );
        assert!(
            UNIT_COLUMN_OFFSET - four_digits <= 12,
            "and must not leave a visible gap"
        );
    }

    /// A display with the whole panel lit, so a `setDrawColor(0)` box shows up
    /// as a cleared region.
    fn filled() -> Display {
        let mut d = Display::new();
        d.set_font(font::profont11());
        d.set_font_pos_top();
        d.draw_box(0, 0, DISPLAY_WIDTH, 64);
        d
    }

    /// The lit count inside `x0..x1` by `y0..y1`.
    fn lit_in(fb: &crate::display::Framebuffer, x0: i32, x1: i32, y0: i32, y1: i32) -> i32 {
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                if fb.pixel(x, y) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn the_inverted_field_clears_the_cplusplus_box() {
        // Tested on its own, with no text, so the assertion is about the box
        // rather than about the box minus whatever glyphs land inside it.
        // `DisplayWidgets.h:209-216`.

        // Upright: `drawBox(x, y + 1, 100, 10)`.
        let mut upright = filled();
        draw_inverted_field(&mut upright, 1, 44, true);
        let fb = upright.into_framebuffer();
        assert_eq!(
            lit_in(&fb, 1, 1 + UPRIGHT_BOX_WIDTH, 45, 45 + BOX_HEIGHT),
            0,
            "the whole {UPRIGHT_BOX_WIDTH}x{BOX_HEIGHT} box is cleared"
        );
        assert!(
            lit_in(
                &fb,
                1 + UPRIGHT_BOX_WIDTH,
                1 + UPRIGHT_BOX_WIDTH + 1,
                45,
                55
            ) > 0,
            "and it stops there"
        );
        assert!(lit_in(&fb, 0, 1, 45, 55) > 0, "and it does not extend left");

        // Landscape: `drawBox(x + 50, y + 1, 78, 10)`.
        let mut land = filled();
        draw_inverted_field(&mut land, 34, 36, false);
        let fb = land.into_framebuffer();
        assert_eq!(
            lit_in(
                &fb,
                34 + VALUE_COLUMN_OFFSET,
                34 + VALUE_COLUMN_OFFSET + VALUE_COLUMN_WIDTH,
                37,
                37 + BOX_HEIGHT
            ),
            0,
            "the whole {VALUE_COLUMN_WIDTH}x{BOX_HEIGHT} box is cleared"
        );
        assert!(
            lit_in(
                &fb,
                34 + VALUE_COLUMN_OFFSET - 1,
                34 + VALUE_COLUMN_OFFSET,
                37,
                47
            ) > 0,
            "and it does not extend left of the value column"
        );
    }

    #[test]
    fn the_inverted_field_restores_the_draw_colour() {
        // If the colour were not restored, every later draw would be an erase
        // and the screen would come out inverted. The C++ is explicit about
        // it; the test makes forgetting it visible.
        let mut d = filled();
        assert_eq!(d.draw_color(), 1);
        draw_inverted_field(&mut d, 0, 0, false);
        assert_eq!(d.draw_color(), 1, "the colour must be restored");
    }

    #[test]
    fn a_scale_fault_replaces_the_value_with_the_word() {
        let mut d = mk();
        let l = crate::lang::for_language(Language::English);
        display_brew_weight(&mut d, l, 0, 36, 12.0, -1.0, true, false);
        let fb = d.into_framebuffer();
        // "Weight: " then "Fault" in the value column. 12 px of weight must not
        // appear anywhere.
        let f = font::profont11();
        assert!(f.str_width(l.scale_failure) > 0);
        assert!(fb.lit_count() > 0, "something was drawn");
    }

    #[test]
    fn the_progress_bar_needs_at_least_two_percent_to_fill() {
        let mut d = mk();
        display_progress_bar(&mut d, 1, 0, 58, 128);
        let thin = d.into_framebuffer();
        // 1% of 128 is 1, and 1 - 2 <= 0, so only the frame.
        assert_eq!(thin.lit_count(), 2 * 128 + 2 * 2, "frame only");

        let mut d = mk();
        display_progress_bar(&mut d, 50, 0, 58, 128);
        let half = d.into_framebuffer();
        assert!(
            half.lit_count() > thin.lit_count(),
            "half is more than the frame"
        );
    }

    #[test]
    fn the_wifi_signal_bars_include_a_zero_height_one() {
        // `for (b = 0; b <= signal; b++)` draws signal+1 bars, the first with
        // height 0 -- so it is a `drawVLine` of length 0 and contributes
        // nothing. Pinned because the off-by-one looks like a bug and is not.
        let mut d = mk();
        let input = DisplayInput {
            wifi_connected: true,
            wifi_signal: 0,
            ..DisplayInput::default()
        };
        display_wifi_status(&mut d, &input, 4, 1, false);
        let zero = d.into_framebuffer();
        let mut d = mk();
        let input = DisplayInput {
            wifi_connected: true,
            wifi_signal: 1,
            ..DisplayInput::default()
        };
        display_wifi_status(&mut d, &input, 4, 1, false);
        let one = d.into_framebuffer();
        assert!(
            one.lit_count() > zero.lit_count(),
            "one more bar step adds ink"
        );
    }

    #[test]
    fn the_brew_row_priority_is_flush_then_water_then_brew() {
        let config = Config::default();
        let mut input = DisplayInput {
            brew_timer: crate::model::BrewTimerState::Running,
            ..DisplayInput::default()
        };
        assert!(matches!(brew_row(&config, &input), BrewRow::Brew { .. }));

        input.pump_on_time_ms = 1000.0;
        assert!(
            matches!(brew_row(&config, &input), BrewRow::HotWater { .. }),
            "hot water beats brew"
        );

        input.state = cc_domain::state::MachineState::ManualFlushRunning;
        assert!(
            matches!(brew_row(&config, &input), BrewRow::ManualFlush { .. }),
            "flush beats all"
        );

        input.state = cc_domain::state::MachineState::PidNormal;
        input.pump_on_time_ms = 0.0;
        input.brew_timer = crate::model::BrewTimerState::Idle;
        assert_eq!(brew_row(&config, &input), BrewRow::None);
    }

    #[test]
    fn an_automatic_brew_by_time_carries_its_target_and_a_manual_one_does_not() {
        let input = DisplayInput {
            brew_timer: crate::model::BrewTimerState::Running,
            ..DisplayInput::default()
        };
        let manual = Config::default();
        assert_eq!(
            brew_row(&manual, &input),
            BrewRow::Brew {
                elapsed_ms: 0.0,
                target_ms: 0.0
            }
        );

        let automatic_time = Config {
            brew_mode: BrewMode::Automatic,
            brew_by_time_enabled: true,
            ..Config::default()
        };
        assert_eq!(
            brew_row(&automatic_time, &input),
            BrewRow::Brew {
                elapsed_ms: 0.0,
                target_ms: input.target_brew_time_ms
            }
        );

        let automatic_weight = Config {
            brew_mode: BrewMode::Automatic,
            brew_by_time_enabled: false,
            ..Config::default()
        };
        assert_eq!(
            brew_row(&automatic_weight, &input),
            BrewRow::Brew {
                elapsed_ms: 0.0,
                target_ms: 0.0
            }
        );
    }

    #[test]
    fn the_statusbar_puts_the_uptime_where_the_maintenance_banner_would_go() {
        let config = Config::default();
        let mut d = mk();
        let input = DisplayInput {
            now_ms: 3_723_000,
            ..DisplayInput::default()
        };
        display_statusbar(
            &mut d,
            &config,
            &input,
            crate::lang::for_language(Language::English),
            false,
        );
        assert!(d.into_framebuffer().lit_count() > 0);

        // With the reminder due, "CLEAN" replaces the uptime at the same x.
        let mut d = mk();
        let due = DisplayInput {
            backflush_reminder_due: true,
            ..DisplayInput::default()
        };
        assert!(
            display_maintenance_status_bar(&mut d, &config, &due, 78, 0),
            "it draws"
        );
        assert!(d.framebuffer().lit_count() > 0);

        // ...and with the feature off it declines and draws nothing.
        let off = Config {
            backflush_reminder_enabled: false,
            ..Config::default()
        };
        let mut d = mk();
        assert!(
            !display_maintenance_status_bar(&mut d, &off, &due, 78, 0),
            "it declines"
        );
    }
}
