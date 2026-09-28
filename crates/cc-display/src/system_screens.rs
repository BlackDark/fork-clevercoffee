//! Port of `include/clevercoffee/display/DisplaySystemScreens.h`.
//!
//! The shared screens every template (except those whose policy opts out) can
//! be preempted by. The C++ is one long `if` chain, in this order:
//!
//! 1. the fullscreen heating logo, if the policy allows it;
//! 2. the PID-disabled screen, if `displayPidOffLogo == 1`;
//! 3. the standby screen, if `displayPidOffLogo == 1`;
//! 4. the steam screen;
//! 5. the water-tank-empty screen;
//! 6. the backflush screen;
//! 7. the emergency-stop screen;
//! 8. the temperature-sensor-error screen;
//! 9. the EEPROM-error screen.
//!
//! Two details in that chain are load-bearing and easy to lose:
//!
//! * **The heating logo checks the policy and the PID state, the PID-off logo
//!   checks only the config.** So a machine in `PID_NORMAL` with
//!   `displayPidOffLogo == 1` shows neither: the heating logo is state-gated to
//!   `PID_NORMAL` and the PID-off logo to `PID_DISABLED`. The chain is
//!   state-partitioned, and a reordering shows two screens or none.
//! * **The emergency screen's `isrCounter() < 500` guard.** For the first 500 ms
//!   after the ISR starts it additionally draws `PID STOPPED` at the top. So the
//!   emergency screen has two renderings, and a golden captured at the wrong
//!   `isr_counter` is not a regression.

use cc_domain::state::MachineState;

use crate::bitmaps_data as bm;
use crate::display::Display;
use crate::fmt::{format_fixed, format_int};
use crate::font;
use crate::helpers::{
    current_display_state, is_backflush_state, is_heating_logo_condition_met, is_steam_state,
};
use crate::model::{Config, DisplayInput};
use crate::templates::{SystemScreenId, TemplateId, TemplatePolicy};
use crate::widgets;

/// `drawSystemScreen` — returns `None` when no shared screen applies.
///
/// The body is one long `if` chain in the C++, and it stays one chain here
/// rather than becoming a `match`: the chain is *ordered*, and two of the arms
/// test the same config flag against different states, so a `match` on the state
/// would have to invent a "no screen" and a "several arms apply" case to stay
/// faithful. The 100-line lint is waived with that reason rather than by
/// splitting the function, because a split would hide the order.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the ordered if-chain IS the C++ control flow; see the doc comment"
)]
pub fn draw(
    d: &mut Display,
    input: &DisplayInput,
    config: &Config,
    template: TemplateId,
    policy: TemplatePolicy,
) -> Option<SystemScreenId> {
    let l = crate::lang::for_language(config.language);
    let upright = template.is_upright();
    let state = current_display_state(input);

    // 1. The fullscreen heating logo.
    if policy.shared_heating_logo && is_heating_logo_condition_met(input, config) {
        d.clear_buffer();
        widgets::display_statusbar(d, config, input, l, upright);
        d.draw_xbmp(0, 20, 40, 40, &bm::HEATING_LOGO);
        d.set_font(font::fub25());
        d.set_cursor(50, 30);
        d.print(format_fixed(input.temperature, 1).as_str());
        d.draw_circle(122, 32, 3);
        return Some(SystemScreenId::Heating);
    }

    // 2. The PID-off screen.
    if config.pid_off_logo == 1 && state == MachineState::PidDisabled {
        d.clear_buffer();
        d.draw_xbmp(38, 0, 52, 53, &bm::OFF_LOGO);
        d.set_cursor(0, 55);
        d.set_font(font::profont10());
        d.print("PID is disabled manually");
        return Some(SystemScreenId::PidDisabled);
    }

    // 3. The standby screen. Same config gate, different state, and a different
    //    layout for the portrait template: the logo goes at the *bottom* of the
    //    128-tall space and the caption below it.
    if config.pid_off_logo == 1 && state == MachineState::Standby {
        d.clear_buffer();
        if upright {
            d.draw_xbmp(6, 50, 52, 53, &bm::OFF_LOGO);
            d.set_cursor(1, 110);
        } else {
            d.draw_xbmp(38, 0, 52, 53, &bm::OFF_LOGO);
            d.set_cursor(36, 55);
        }
        d.set_font(font::profont10());
        d.print("Standby mode");
        return Some(SystemScreenId::Standby);
    }

    // 4. The steam screen. Note `drawXBMP(-1, 12, ...)`: the logo is drawn one
    //    pixel off the left edge, so its rightmost column is clipped. That is
    //    the C++ and it is reproduced, because "fixing" it would move a column
    //    and desynchronise the oracle.
    if is_steam_state(state) {
        d.clear_buffer();
        d.draw_xbmp(-1, 12, 40, 40, &bm::STEAM_LOGO);
        widgets::display_temperature(d, input, 48, 16);
        return Some(SystemScreenId::Steam);
    }

    // 5. The water-tank-empty screen: the logo only, no text.
    if state == MachineState::WaterTankEmpty {
        d.clear_buffer();
        if upright {
            d.draw_xbmp(8, 50, 47, 64, &bm::WATER_TANK_EMPTY_LOGO);
        } else {
            d.draw_xbmp(45, 0, 47, 64, &bm::WATER_TANK_EMPTY_LOGO);
        }
        d.set_font(font::profont11());
        return Some(SystemScreenId::WaterTankEmpty);
    }

    // 6. The backflush screen. The title is `fub17` at y=10; the body differs per
    //    sub-state: the two idle-ish states show prompts, and everything else
    //    shows the cycle counter.
    if is_backflush_state(state) {
        d.clear_buffer();
        d.set_font(font::fub17());
        d.set_cursor(2, 10);
        d.print("Backflush");

        match state {
            MachineState::BackflushIdle | MachineState::BackflushFinished => {
                // y=37 is always the prompt ("Press brew switch"); y=50 is the
                // consequence ("to start..." / "to finish...").
                d.set_font(font::profont12());
                d.set_cursor(4, 37);
                d.print(l.backflush_press);
                d.set_cursor(4, 50);
                d.print(if state == MachineState::BackflushIdle {
                    l.backflush_start
                } else {
                    l.backflush_finish
                });
            }
            _ => {
                d.set_font(font::fub17());
                d.set_cursor(42, 42);
                d.print(format_int(i32::from(input.backflush_cycle_count)).as_str());
                d.print("/");
                d.print(format_int(i32::from(config.backflush_cycles)).as_str());
            }
        }
        return Some(SystemScreenId::Backflush);
    }

    // 7. The emergency-stop screen: both readings, the thermometer, and -- for
    //    the first 500 ms of ISR time -- a "PID STOPPED" banner.
    if state == MachineState::EmergencyStop {
        d.clear_buffer();
        d.set_font(font::profont11());
        d.set_cursor(32, 24);
        d.print(l.current_temp);
        d.print(format_fixed(input.temperature, 1).as_str());
        d.print(" ");
        d.print_char('\u{b0}');
        d.print("C");
        d.set_cursor(32, 34);
        d.print(l.set_temp);
        d.print(format_fixed(input.setpoint, 1).as_str());
        d.print(" ");
        d.print_char('\u{b0}');
        d.print("C");

        widgets::display_thermometer_outline(d, input, 4, 58);

        if input.isr_counter < 500 {
            widgets::draw_temperature_bar(d, input, 8, 30);
            d.set_cursor(32, 4);
            d.print("PID STOPPED");
        }
        return Some(SystemScreenId::EmergencyStop);
    }

    // 8. The temperature-sensor-error screen. Landscape packs the reading into
    //    line 2; portrait spends five lines on the message. The C++ has a
    //    `snprintf("%.1f")` between them, which is why the landscape variant
    //    takes `tempBuffer` as a *pre-built string*.
    if state == MachineState::SensorError {
        let temp = format_fixed(input.temperature, 1);
        d.clear_buffer();
        d.set_font(font::profont11());
        if upright {
            widgets::display_message(
                d,
                [
                    l.error_tsensor[0],
                    l.error_tsensor[1],
                    temp.as_str(),
                    l.error_tsensor[2],
                    l.error_tsensor[3],
                    l.error_tsensor[4],
                ],
            );
        } else {
            widgets::display_message(
                d,
                [
                    l.error_tsensor[0],
                    temp.as_str(),
                    l.error_tsensor[1],
                    "",
                    "",
                    "",
                ],
            );
        }
        return Some(SystemScreenId::SensorError);
    }

    // 9. The EEPROM-error screen. The C++ has a *six*-argument
    //    `displayMessage` call with a trailing comma after `"EEPROM Error,
    //    please set Values"`, which does not compile in C++ -- so this screen has
    //    never been reached. Ported as a one-line message, which is what the
    //    author clearly intended. Recorded here because it is the one place the
    //    C++ is known-broken.
    if state == MachineState::EepromError {
        d.clear_buffer();
        d.set_font(font::profont11());
        widgets::display_message(d, ["EEPROM Error, please set Values", "", "", "", "", ""]);
        return Some(SystemScreenId::EepromError);
    }

    None
}
