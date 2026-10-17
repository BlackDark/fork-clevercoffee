//! Which of the three status LEDs should be lit, as a pure function.
//!
//! # Why this crate and not `cc-web` or `cc-machine`
//!
//! `cc-display` is the right home because the three primitives this needs are
//! already here, each with its own test:
//!
//! * [`crate::helpers::status_led_tolerance`] — the steam-versus-blink delta
//!   choice, with a test asserting each side.
//! * [`crate::helpers::is_near_setpoint_for_led`] — the **`<=`** comparator,
//!   tested against the display's strict `<` at the exact boundary value so the
//!   two cannot drift apart.
//! * [`crate::helpers::is_blink_phase_on`] — the half-cycle the brew LED's
//!   manual-flush rule hangs off.
//!
//! Re-deriving any of those in `cc-machine` or `cc-web` would create a second
//! copy of a tolerance rule that `helpers.rs` exists to keep single. This module
//! is therefore **only the composition**: it consumes the existing primitives
//! rather than restating them, and the dependency edges are unchanged —
//! `cc-firmware` already depends on `cc-display`, and nothing new points at it.
//!
//! # What this is NOT
//!
//! This module decides *lit or not*. It does not touch a GPIO, does not know an
//! LED can be inverted, and does not know whether the pin is even wired. Those
//! belong to `cc_hal_esp32::leds`, the only code that drives a pin: this module
//! asks, and that one writes.
//!
//! # The `enabled` gate
//!
//! [`LedOutput::from_state`](crate::leds::LedOutput::from_state) reports `false`
//! for a disabled LED, and `cc_hal_esp32::leds` does not configure a pin it was
//! not asked for. Together those are the rule "a disabled LED stays dark",
//! applied once on each side.

use cc_domain::state::MachineState;

use crate::helpers::{
    is_blink_phase_on, is_near_setpoint_for_led, is_steam_state, status_led_tolerance,
};
use crate::model::DisplayInput;

/// The three LED states the machine reports, as one value.
///
/// `true` means lit. Deliberately not a bitmask or a fixed-size set: there are
/// exactly three LEDs, each with its own independent gate, and a named field per
/// LED is what makes the three gates legible at the call site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LedOutput {
    /// `PIN_STATUSLED` — near setpoint, in an eligible state.
    pub status: bool,
    /// `PIN_BREWLED` — brewing, or pumping water for a flush.
    pub brew: bool,
    /// `PIN_STEAMLED` — steaming.
    pub steam: bool,
}

impl LedOutput {
    /// The whole LED rule, as a pure function of the machine state, the process
    /// temperature and the blink phase.
    ///
    /// `blinking_delta` is `config.display.blinking.delta`, read through
    /// [`crate::helpers::status_led_tolerance`], so the steam state's wider 5 °C
    /// tolerance and the display's blink delta stay the two numbers they already
    /// were rather than becoming two more. It is a bare `f64` and not a `Config`
    /// because this runs every 10 ms control tick and it is the only configured
    /// field the rule reads.
    ///
    /// `input.isr_counter` is consulted for exactly one thing: the brew LED's
    /// manual-flush and backflush exception, which applies only during the
    /// **first half** of the counter's 1 s cycle. That is the same half-cycle
    /// [`is_blink_phase_on`] defines for the display, which is why the two are
    /// visually in phase. The status and steam LEDs do not read it at all.
    #[must_use]
    pub fn from_state(input: &DisplayInput, blinking_delta: f64) -> Self {
        Self {
            status: status_led_should_light(input, blinking_delta),
            brew: brew_led_should_light(input),
            steam: is_steam_state(input.state),
        }
    }
}

/// The status LED's gate.
///
/// Two conditions, **and**ed, and the order matters only for readability:
///
/// 1. `state` is at or below [`MachineState::BackflushFinished`]. Ordering the
///    enum that way is deliberate: the states above it are exactly the fault and
///    idle states — tank empty, emergency stop, PID disabled, standby, sensor
///    error and EEPROM error — so the LED goes dark whenever the machine is not
///    well. `MachineState` is `#[repr(u16)]` with its variants in that order, so
///    the comparison is a discriminant comparison and the intent ("everything up
///    to and including the backflush group") stays readable.
/// 2. [`is_near_setpoint_for_led`] at [`status_led_tolerance`], which is the
///    **inclusive** `<=` the LED uses, where the display's blink uses a strict
///    `<`.
///
/// The status LED's tolerance in a **brew** state is `blinking_delta`, not the
/// 5 °C steam tolerance: the tolerance only widens for a steam state, so during a
/// brew this LED tracks the setpoint to a third of a degree.
fn status_led_should_light(input: &DisplayInput, blinking_delta: f64) -> bool {
    let eligible_state = input.state <= MachineState::BackflushFinished;
    let near_setpoint = is_near_setpoint_for_led(
        input.temperature,
        input.setpoint,
        status_led_tolerance(input.state, blinking_delta),
    );
    eligible_state && near_setpoint
}

/// The brew LED's gate.
///
/// A brew state lights it steadily; anything else has to earn a blink:
///
/// 1. [`MachineState::is_brew_state`] alone is enough, so the LED stays lit
///    through the post-brew timer (`BrewFinished` is a brew state).
/// 2. Otherwise the blink phase gates an exception for a manual flush and for the
///    three *active* backflush states, never `BackflushIdle`. Because the
///    exception is gated on the counter's first half, the LED **blinks** during
///    those states rather than staying lit.
///
/// So a brew is steady and a flush blinks. `PidNormal`, the init state, the steam
/// state and every fault state read `false`, on both halves.
fn brew_led_should_light(input: &DisplayInput) -> bool {
    if input.state.is_brew_state() {
        return true;
    }
    if !is_blink_phase_on(input) {
        return false;
    }
    input.state.is_manual_flush_state()
        || (input.state.is_backflush_state() && input.state != MachineState::BackflushIdle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default of `config.display.blinking.delta`, and the value every test
    /// that is not *about* the tolerance uses. Naming it keeps each case from
    /// repeating a literal that is a configuration default, not a parameter of the
    /// rule.
    const DELTA: f64 = 0.3;

    /// A readout that is near setpoint and not in any special state, so a test
    /// that asserts `!led.brew` is asserting the state rule and not the
    /// temperature.
    fn idle_at_setpoint() -> DisplayInput {
        DisplayInput {
            state: MachineState::PidNormal,
            temperature: 95.0,
            setpoint: 95.0,
            ..DisplayInput::default()
        }
    }

    #[test]
    fn the_status_led_is_lit_at_setpoint_in_pid_normal() {
        let led = LedOutput::from_state(&idle_at_setpoint(), DELTA);
        assert!(
            led.status,
            "on setpoint, PID_NORMAL must light the status LED"
        );
    }

    #[test]
    fn the_status_led_goes_dark_in_every_state_after_backflush_finished() {
        // The eligibility rule is the whole of the rule, so this list is exactly
        // the excluded half.
        for state in [
            MachineState::WaterTankEmpty,
            MachineState::EmergencyStop,
            MachineState::PidDisabled,
            MachineState::Standby,
            MachineState::SensorError,
            MachineState::EepromError,
        ] {
            let input = DisplayInput {
                state,
                ..idle_at_setpoint()
            };
            assert!(
                !LedOutput::from_state(&input, DELTA).status,
                "{state:?} is after BACKFLUSH_FINISHED, so the LED must be dark"
            );
        }
    }

    #[test]
    fn the_status_led_stays_available_through_the_whole_backflush_group() {
        // The boundary is inclusive on purpose: `BACKFLUSH_FINISHED` itself is
        // `<= BACKFLUSH_FINISHED`, so the last backflush state still lights it.
        for state in [
            MachineState::BackflushIdle,
            MachineState::BackflushFilling,
            MachineState::BackflushFlushing,
            MachineState::BackflushFinished,
        ] {
            let input = DisplayInput {
                state,
                ..idle_at_setpoint()
            };
            assert!(
                LedOutput::from_state(&input, DELTA).status,
                "{state:?} is within BACKFLUSH_FINISHED"
            );
        }
    }

    #[test]
    fn the_status_led_uses_the_blink_delta_outside_steam() {
        // 0.3 °C is `display.blinking.delta`, and it is **not** the 5 °C steam
        // tolerance: `getStatusLedTolerance` only widens for the steam state.
        let input = DisplayInput {
            state: MachineState::PidNormal,
            temperature: 95.5,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        assert!(
            !LedOutput::from_state(&input, DELTA).status,
            "0.5 C out is past the 0.3 C blink delta"
        );
        let nearly = DisplayInput {
            temperature: 95.2,
            ..input
        };
        assert!(
            LedOutput::from_state(&nearly, DELTA).status,
            "0.2 C out is inside the blink delta"
        );
    }

    #[test]
    fn the_status_led_widens_to_five_degrees_while_steaming() {
        // `getStatusLedTolerance` returns `TEMP_TOLERANCE_STEAM_C` for the steam
        // state, and the steam LED lights in the same state — so while steaming
        // both LEDs are on and the status one is not chasing a third of a degree.
        let input = DisplayInput {
            state: MachineState::SteamRunning,
            temperature: 98.0,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        let led = LedOutput::from_state(&input, DELTA);
        assert!(led.status, "3 C out is inside the 5 C steam tolerance");
        assert!(led.steam, "and the steam LED is on in the same state");
    }

    #[test]
    fn the_status_led_boundary_is_inclusive() {
        // `isNearSetpointForStatusLed` is `<=`, unlike the display's strict `<`.
        // A `delta` of 0.25 is used rather than the configured 0.3 because
        // `95.0 - 0.3` is not representable and the comparison would be
        // measuring rounding; `helpers.rs`'s boundary test explains why at
        // length.
        let delta = 0.25;
        let at = DisplayInput {
            state: MachineState::PidNormal,
            temperature: 94.75,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        assert!(
            LedOutput::from_state(&at, delta).status,
            "exactly at the tolerance counts as near for the LED"
        );
    }

    #[test]
    fn every_brew_state_lights_the_brew_led_steadily() {
        // No ISR-counter gate on this branch — the tests pass the default
        // counter, and the next test shows the flush exception is the only one
        // that reads it.
        for state in [
            MachineState::BrewPreinfusion,
            MachineState::BrewPreinfusionPause,
            MachineState::BrewRunning,
            MachineState::BrewFinished,
        ] {
            for counter in [0, 499, 500, 999] {
                let input = DisplayInput {
                    state,
                    isr_counter: counter,
                    ..idle_at_setpoint()
                };
                assert!(
                    LedOutput::from_state(&input, DELTA).brew,
                    "{state:?} at counter {counter} must be lit: brewing is steady"
                );
            }
        }
    }

    #[test]
    fn manual_flush_and_active_backflush_light_the_brew_led_only_on_the_first_half() {
        // The exception **blinks**: the blink phase gates it, so the second half
        // of the cycle goes dark. `BackflushIdle` is excluded outright, and the
        // four non-brew, non-flush states stay dark on both halves.
        for (state, lit_on_first_half) in [
            (MachineState::ManualFlushRunning, true),
            (MachineState::BackflushFilling, true),
            (MachineState::BackflushFlushing, true),
            (MachineState::BackflushFinished, true),
            (MachineState::BackflushIdle, false),
            (MachineState::PidNormal, false),
            (MachineState::SteamRunning, false),
            (MachineState::Init, false),
            (MachineState::Standby, false),
        ] {
            for (counter, first_half) in [(0u32, true), (499, true), (500, false), (999, false)] {
                let input = DisplayInput {
                    state,
                    isr_counter: counter,
                    ..idle_at_setpoint()
                };
                assert_eq!(
                    LedOutput::from_state(&input, DELTA).brew,
                    lit_on_first_half && first_half,
                    "{state:?} at counter {counter}"
                );
            }
        }
    }

    #[test]
    fn only_the_steam_state_lights_the_steam_led() {
        for state in cc_domain::state::ALL {
            let input = DisplayInput {
                state,
                ..idle_at_setpoint()
            };
            assert_eq!(
                LedOutput::from_state(&input, DELTA).steam,
                state == MachineState::SteamRunning,
                "{state:?}"
            );
        }
    }

    #[test]
    fn the_three_leds_are_independent_of_each_others_conditions() {
        // One case per LED that only its own gate can turn on, so a future edit
        // that collapses two branches into one shows up here.
        let steaming = DisplayInput {
            state: MachineState::SteamRunning,
            temperature: 95.0,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        let led = LedOutput::from_state(&steaming, DELTA);
        assert!(led.steam && led.status && !led.brew);

        let brewing = DisplayInput {
            state: MachineState::BrewRunning,
            temperature: 95.0,
            setpoint: 95.0,
            isr_counter: 600,
            ..DisplayInput::default()
        };
        let led = LedOutput::from_state(&brewing, DELTA);
        assert!(
            led.brew && led.status && !led.steam,
            "counter 600 proves the brew LED ignores it"
        );
    }
}
