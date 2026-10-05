//! Which of the three status LEDs should be lit, as a pure function.
//!
//! Owner: **3.1** of
//! [`32-findings-2026-10-03.md`](../../../docs/history/review-2026-10-03.md)
//! — "`hardware.leds.*` (6 params) are writable and shown in the UI, but there
//! is no LED code anywhere".
//!
//! # Why this crate and not `cc-web` or `cc-machine`
//!
//! **The brief for 3.1 asked for `cc-web` (the home for pure logic after finding
//! 4.1) or `cc-machine`. `cc-display` is the correct third answer, and putting it
//! in either of the other two would have been a mistake.**
//!
//! The C++ does not put the LED rules in a machine file. `LoopManager::updateLEDs`
//! (`src/core/LoopManager.cpp:255-286`) delegates every decision to
//! `CleverCoffee::Display::` helpers in
//! `include/clevercoffee/display/displayHelpers.h:41-59`, and **cc-display is
//! already the host-tested port of that file** — its `helpers.rs` module doc
//! opens with "Port of `include/clevercoffee/display/displayHelpers.h`" and it
//! already carries two of the three primitives this needs:
//!
//! * [`crate::helpers::status_led_tolerance`] — `getStatusLedTolerance`, the
//!   steam-vs-blink-delta choice, with a test asserting each side.
//! * [`crate::helpers::is_near_setpoint_for_led`] — `isNearSetpointForStatusLed`,
//!   the **`<=`** comparator, tested against the display's strict `<` at the
//!   exact boundary value so the two cannot drift apart.
//! * [`crate::helpers::is_blink_phase_on`] — `isBlinkPhaseOn`, the `isrCounter <
//!   500` half-cycle the brew LED's manual-flush rule hangs off.
//!
//! Re-deriving any of those in `cc-machine` or `cc-web` would have created the
//! second copy of a tolerance rule that `helpers.rs`'s module doc explicitly
//! exists to prevent (ADR-0001 §3). This module is therefore **only the
//! composition** the C++ performs in `updateLEDs`, and it consumes the existing
//! primitives rather than restating them. The dependency edges are unchanged:
//! `cc-firmware` already depends on `cc-display`, and nothing new points at it.
//!
//! # What this is NOT
//!
//! This module decides *lit or not*. It does not touch a GPIO, does not know an
//! LED can be inverted, and does not know whether the pin is even wired. Those
//! belong to `cc_hal_esp32::leds`, which is the only code that drives a pin,
//! exactly as the C++ splits it: `LoopManager` asks, `StandardLED` writes
//! (`src/hardware/StandardLED.cpp:16-18`).
//!
//! # The `enabled` gate
//!
//! The C++ checks `hardware.leds.*.enabled` **twice**: once when constructing the
//! pin (`HardwareManager::initializeLEDs`, `:95-125` — a disabled LED has no
//! `StandardLED` object at all) and again in `updateLEDs` before dereferencing
//! it. That double check is an artefact of the C++ needing the pointer to be
//! non-null, not two decisions. [`LedOutput::from_state`](crate::leds::LedOutput::from_state)
//! models it once, by
//! reporting `false` for a disabled LED, and `cc_hal_esp32::leds` models the
//! other half by not configuring a pin it was not asked for. Together they are
//! the same rule applied once each side.

use cc_domain::state::MachineState;

use crate::helpers::{
    is_blink_phase_on, is_near_setpoint_for_led, is_steam_state, status_led_tolerance,
};
use crate::model::DisplayInput;

/// The three LED states the machine reports, as one value.
///
/// `true` means lit. Deliberately not a bitmask or a `heapless` set: there are
/// exactly three LEDs, they are named everywhere in the C++, and a named field
/// per LED is what makes `updateLEDs`'s three independent gates legible at the
/// call site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LedOutput {
    /// `PIN_STATUSLED` (GPIO26) — near setpoint, in an eligible state.
    pub status: bool,
    /// `PIN_BREWLED` (GPIO19) — brewing, or pumping water for a flush.
    pub brew: bool,
    /// `PIN_STEAMLED` — steaming.
    pub steam: bool,
}

impl LedOutput {
    /// The C++ `LoopManager::updateLEDs` (`LoopManager.cpp:255-286`), as a pure
    /// function of the machine state and the process temperature.
    ///
    /// `blinking_delta` is `config.display.blinking.delta`, read through
    /// [`crate::helpers::status_led_tolerance`], so the steam state's wider 5 °C
    /// tolerance and the display's blink delta stay the two numbers they already
    /// were rather than becoming two more. It is a bare `f64` and not a `Config`
    /// because this is evaluated every 10 ms control tick and it is the only
    /// configured field the rule reads — see [`crate::helpers::status_led_tolerance`].
    ///
    /// `input.isr_counter` is the C++'s `systemContext_.isrCounter()`, and it is
    /// consulted for exactly one thing: the brew LED's manual-flush and backflush
    /// exception, which the C++ applies only during the **first half** of the
    /// counter's 1 s cycle. That is the same half-cycle [`is_blink_phase_on`]
    /// defines for the display, and it is why the two are visually in phase. The
    /// status and steam LEDs do not read it at all — matching `updateLEDs`, where
    /// only the brew LED's branch mentions `isrCounter()`.
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
/// 1. `static_cast<int>(state) <= static_cast<int>(BACKFLUSH_FINISHED)`
///    (`LoopManager.cpp:262`). The C++ spells this as a raw numeric comparison
///    against the enum's discriminant, which is why it is spelled here as
///    `MachineState::BackflushFinished` — a `PartialOrd` on the same
///    `#[repr(u16)]` discriminants, so the comparison is identical and the intent
///    ("everything up to and including the backflush group") is readable.
///    The states it **excludes** are exactly the fault and idle states: tank
///    empty, emergency stop, PID disabled, standby, sensor error and EEPROM
///    error. That is deliberate in the C++ — the status LED is an "all is well"
///    indicator, so it must go dark when the machine is not.
/// 2. [`is_near_setpoint_for_led`] at [`status_led_tolerance`], which is the
///    **inclusive** `<=` the C++ uses for the LED (as opposed to the display's
///    strict `<`).
///
/// Note the status LED's tolerance in a **brew** state is `blinking_delta`, not
/// the 5 °C steam tolerance: `getStatusLedTolerance` only widens for
/// `isSteamState`, so during a brew this LED tracks the setpoint to a third of a
/// degree. That is the C++'s behaviour and it is preserved, not corrected.
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
/// `isBrewState(machineState)` first, and only if that is false the C++ consults
/// the blink phase (`LoopManager.cpp:270-278`):
///
/// ```cpp
/// bool brewLedOn = isBrewState(machineState);
/// if (!brewLedOn && systemContext_.isrCounter() < 500) {
///     if (machineState == MANUAL_FLUSH_RUNNING) {
///         brewLedOn = true;
///     } else if (isBackflushState(machineState) && machineState != BACKFLUSH_IDLE) {
///         brewLedOn = true;
///     }
/// }
/// ```
///
/// So the exception covers manual flush and the three *active* backflush states,
/// and never `BACKFLUSH_IDLE` — and it is gated on the counter's first half, so
/// the LED **blinks** during those states rather than staying lit. The C++ gates
/// the brew states themselves on nothing, so a brew is steady and a flush blinks.
///
/// `MachineState::BrewFinished` is included by `is_brew_state`, so the LED stays
/// lit through the post-brew timer. The states the C++ leaves out — `PID_NORMAL`,
/// `INIT`, `STEAM_RUNNING`, every fault state — read `false` here.
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

    /// The C++'s default `display.blinking.delta` (`defaults.h:109`), and the
    /// value every test that is not *about* the tolerance uses. Naming it keeps
    /// each case from repeating a literal that is a configuration default, not a
    /// parameter of the rule.
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
        // `state <= BACKFLUSH_FINISHED` is the whole of the eligibility rule, so
        // this list is exactly the C++'s excluded half (`LoopManager.cpp:262`).
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
        // No `isrCounter` gate on this branch — the tests pass the default
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
        // The C++'s exception, and it **blinks**: `isrCounter() < 500` gates it,
        // so `500..=999` goes dark. `BACKFLUSH_IDLE` is excluded outright, and
        // the four non-brew, non-flush states stay dark on both halves.
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
