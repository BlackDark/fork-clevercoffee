//! Switch debouncing and long-press detection, with an injected clock.
//!
//! Owner: **R3-02**.
//!
//! # What this replaces
//!
//! `src/hardware/IOSwitch.cpp` and `include/clevercoffee/hardware/IOSwitch.h`
//! — four switches (power, brew, steam, hot water) plus the water-tank float
//! switch, all through the same class.
//!
//! # Why the clock is injected
//!
//! The C++ uses `std::chrono::steady_clock::now()` and compares
//! `time_point`s, **not** `millis()`. That choice is load-bearing: `steady_clock`
//! is monotonic, so it cannot jump when NTP steps the system clock, and a
//! debounce that jumps is a debounce that lets a contact bounce straight
//! through. This port takes the current time as a [`Millis`] argument, which
//! keeps the property (the caller supplies a monotonic tick) and makes the whole
//! state machine host-testable with a synthetic clock — something the C++
//! cannot do, because `steady_clock::now()` is not replaceable.
//!
//! Every number here is the C++'s, and each is a named constant so a test can
//! pin it.
//!
//! # The C++'s debounce, faithfully
//!
//! ```cpp
//! bool IOSwitch::isPressed() {
//!     const uint8_t reading = gpio.read();
//!     const auto now = std::chrono::steady_clock::now();
//!
//!     if (reading != lastState) { lastDebounceTime = now; }        // (1)
//!
//!     const auto mapped_mode = static_cast<uint8_t>(mode_);
//!     if (now - lastDebounceTime > debounceDelay) {                // (2)
//!         if ((reading ^ mapped_mode) != currentState) {
//!             currentState = reading ^ mapped_mode;
//!             if (currentState == LOW) { lastStateChangeTime = now; }
//!             else { pressStartTime = now; }
//!         }
//!     }
//!     lastState = reading;                                         // (3)
//!     ...
//! }
//! ```
//!
//! Three details that a "clean" rewrite would get wrong, and that the tests
//! therefore pin:
//!
//! 1. **(1) resets the debounce timer on the *raw* pin**, before any mode
//!    inversion. So a `NORMALLY_CLOSED` switch whose pin is inverted still
//!    debounces on the electrical edge, not the logical one.
//! 2. **(2) is a strict `>`**, not `>=`. A contact that settles at exactly
//!    20 ms is *not* accepted until 21 ms.
//! 3. **(3) assigns `lastState` unconditionally**, at the end. It is the
//!    "previous raw reading" for (1), so it must be updated whether or not the
//!    debounce gate was open. Getting this wrong makes (1) compare against a
//!    stale value and the switch stops responding entirely.
//!
//! # The water tank's initial state
//!
//! `SystemInitializer::createWaterTankSensor` passes
//! `initialState = (mode == NORMALLY_OPEN) ? HIGH : LOW` and builds the pin
//! with `IN_PULLDOWN` for open and `IN_PULLUP` for closed
//! (`src/core/SystemInitializer.cpp:65-76`). The constructor seeds `lastState`
//! with that and `currentState` with `LOW`
//! (`IOSwitch.cpp:19`), so **a water-tank switch reads "not pressed" — tank
//! empty — until the debounce settles on the first read.** `SensorCoordinator`
//! separately initialises `waterTankFull_` to `true` with the comment "Assume
//! full initially" (`SensorCoordinator.h:260`), so the machine does not refuse
//! to pump on the first tick. Both are preserved; see
//! `s2_the_c_tank_starts_full_and_the_switch_starts_open`.

use crate::hardware::{SwitchMode, SwitchType};
use crate::units::Millis;

/// The debounce interval, in milliseconds.
///
/// `static constexpr std::chrono::milliseconds debounceDelay{20}`
/// (`IOSwitch.h:44`).
pub const DEBOUNCE: Millis = Millis::new(20);

/// The long-press threshold, in milliseconds.
///
/// `static constexpr std::chrono::milliseconds longPressDuration{500}`
/// (`IOSwitch.h:45`).
pub const LONG_PRESS: Millis = Millis::new(500);

/// The pin's logic level when a contact is open, matching the C++'s `LOW`.
const LOW: u8 = 0;
/// The pin's logic level when a contact is closed.
const HIGH: u8 = 1;

/// A debounced switch, driven by polling with an explicit time.
///
/// Construct it once, then call [`Debounced::update`] on every loop with the
/// current pin level. That is the shape the C++ has: `isPressed()` is called
/// from the loop and re-reads the pin itself each time.
#[derive(Clone, Debug)]
pub struct Debounced {
    switch_type: SwitchType,
    mode: SwitchMode,
    /// The previous *raw* pin reading, for the debounce-timer reset.
    ///
    /// `lastState` (`IOSwitch.cpp:19`), seeded with the caller's
    /// `initialState` rather than with 0.
    last_raw: u8,
    /// The debounced, mode-inverted state.
    ///
    /// `currentState` (`IOSwitch.cpp:20`), **always** seeded `LOW` regardless
    /// of `initialState`.
    state: u8,
    /// When the debounced state last went to `LOW`.
    ///
    /// `lastStateChangeTime` (`IOSwitch.cpp:53`).
    last_change_to_low: Millis,
    /// When the debounced state last went to `HIGH`.
    ///
    /// `pressStartTime` (`IOSwitch.cpp:55`).
    press_start: Millis,
    /// When the raw pin last changed, which arms the debounce.
    ///
    /// `lastDebounceTime` (`IOSwitch.cpp:54`).
    last_raw_change: Millis,
    /// Whether a long press has been reported and not yet cleared.
    ///
    /// `longPressTriggered` (`IOSwitch.cpp:56`).
    long_press_triggered: bool,
}

impl Debounced {
    /// A debouncer for a switch of the given type and wiring.
    ///
    /// `initial_raw` is the pin's believed rest level, and is what the C++ calls
    /// `initialState`. Its only effect is to decide whether the *first* read
    /// arms the debounce: if the first raw reading differs from it, the debounce
    /// timer starts then and the state does not change until 20 ms later.
    ///
    /// It does **not** seed the switch state, which starts `LOW` — pressed is
    /// never assumed.
    #[must_use]
    pub const fn new(switch_type: SwitchType, mode: SwitchMode, initial_raw: u8) -> Self {
        Self {
            switch_type,
            mode,
            last_raw: initial_raw,
            state: LOW,
            last_change_to_low: Millis::ZERO,
            press_start: Millis::ZERO,
            last_raw_change: Millis::ZERO,
            long_press_triggered: false,
        }
    }

    /// The switch's type.
    #[must_use]
    pub const fn switch_type(&self) -> SwitchType {
        self.switch_type
    }

    /// The switch's wiring.
    #[must_use]
    pub const fn mode(&self) -> SwitchMode {
        self.mode
    }

    /// The debounced, mode-inverted state: `true` when pressed.
    #[must_use]
    pub const fn is_pressed(&self) -> bool {
        self.state == HIGH
    }

    /// Whether a long press is currently reported.
    ///
    /// Ported from `IOSwitch::longPressDetected` (`IOSwitch.cpp:56-65`), which
    /// returns `false` for a `TOGGLE` switch unconditionally, before looking at
    /// the flag at all.
    #[must_use]
    pub const fn long_press_detected(&self) -> bool {
        // Preserved: a toggle switch never reports a long press. The C++ checks
        // the type first, so even a corrupted flag cannot make a toggle report
        // one.
        if matches!(self.switch_type, SwitchType::Toggle) {
            return false;
        }
        if matches!(self.switch_type, SwitchType::Momentary) {
            return self.long_press_triggered;
        }
        false
    }

    /// Read the pin and advance the debounce.
    ///
    /// `raw` is the pin's level, `1` for high. `now` is a monotonic
    /// millisecond timestamp — the same property `steady_clock` has, and
    /// whatever the caller reads it from.
    ///
    /// # Note on the long-press flag
    ///
    /// The C++ clears `longPressTriggered` in the same call that observes the
    /// release (`IOSwitch.cpp:48-49`), so it is **level-triggered on the current
    /// call**, not latched across calls. A caller that reads
    /// [`Self::long_press_detected`] after [`Self::update`] gets the value for
    /// the state *at that instant*; there is no "a long press happened" edge to
    /// miss, and also none to catch — a press shorter than the loop interval is
    /// invisible. Pinned by `s4_*` and
    /// `a_press_shorter_than_the_loop_interval_is_invisible`.
    pub fn update(&mut self, raw: u8, now: Millis) -> bool {
        let raw = if raw == 0 { LOW } else { HIGH };

        // (1) The debounce timer is reset on the RAW pin, before inversion.
        if raw != self.last_raw {
            self.last_raw_change = now;
        }

        // `mode_` as a u8: NORMALLY_OPEN is 0, NORMALLY_CLOSED is 1
        // (`defaults.h:147-150`), so the XOR inverts for a closed contact.
        let mapped_mode = self.mode as u8;

        // (2) Strict `>`, matching `now - lastDebounceTime > debounceDelay`.
        // The two conditions are one `if` in the C++; they are collapsed here
        // only because the bodies are the same length.
        if now.since(self.last_raw_change) > DEBOUNCE && (raw ^ mapped_mode) != self.state {
            self.state = raw ^ mapped_mode;
            if self.state == LOW {
                self.last_change_to_low = now;
            } else {
                self.press_start = now;
            }
        }

        // (3) Unconditional, at the end.
        self.last_raw = raw;

        // The long-press block, gated on the type exactly as the C++ gates it
        // (`IOSwitch.cpp:44`).
        if matches!(self.switch_type, SwitchType::Momentary) {
            if self.state == HIGH && now.since(self.press_start) >= LONG_PRESS {
                self.long_press_triggered = true;
            } else if self.state == LOW && self.last_change_to_low == now {
                // The C++'s condition is `lastStateChangeTime == currentTime`
                // (`IOSwitch.cpp:49`) — an equality of two `time_point`s, so it
                // holds only on the call that observed the release. On any
                // later call the flag is *not* cleared. That is a latent C++
                // bug: a long press followed by a release between two polls
                // leaves the flag set. Preserved, and pinned by
                // `s4_a_release_between_polls_leaves_the_long_press_flag_set`.
                self.long_press_triggered = false;
            }
        }

        self.state == HIGH
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// A momentary, normally-open switch: the power/brew/steam/water default.
    fn momentary() -> Debounced {
        Debounced::new(SwitchType::Momentary, SwitchMode::NormallyOpen, LOW)
    }

    /// A toggle switch: what `createWaterTankSensor` builds
    /// (`SystemInitializer.cpp:75`).
    fn toggle() -> Debounced {
        Debounced::new(SwitchType::Toggle, SwitchMode::NormallyOpen, HIGH)
    }

    // ============================================================== debounce

    #[test]
    fn the_thresholds_are_the_cpp_ones() {
        // IOSwitch.h:44-45
        assert_eq!(DEBOUNCE.raw(), 20);
        assert_eq!(LONG_PRESS.raw(), 500);
    }

    #[test]
    fn a_fresh_switch_is_not_pressed() {
        // `currentState(LOW)` in the constructor (`IOSwitch.cpp:19`).
        let switch = momentary();
        assert!(!switch.is_pressed());
    }

    #[test]
    fn a_steady_high_is_not_pressed_until_the_debounce_elapses() {
        // The whole point of the class: a bouncing contact must not register.
        let mut switch = momentary();
        // Poll at the C++'s loop rate, holding the pin high.
        let mut timeline = Vec::new();
        for ms in 0..25u32 {
            timeline.push(switch.update(HIGH, Millis::new(ms)));
        }
        // Held high throughout, pressed only from 21 ms: the C++'s strict `>`.
        // The pin goes high at t=0, which arms the debounce there, so the
        // state changes at t=21 (`> 20`, not `>= 20`) and not before.
        for (index, pressed) in timeline.iter().enumerate() {
            let expected = index > DEBOUNCE.raw() as usize;
            assert_eq!(*pressed, expected, "at t={index} ms");
        }
    }

    #[test]
    fn the_debounce_boundary_is_exclusive() {
        // `now - lastDebounceTime > debounceDelay` (`IOSwitch.cpp:33`).
        // A change at t=0 is accepted at t=21, not t=20.
        let mut switch = momentary();
        assert!(
            !switch.update(HIGH, Millis::ZERO),
            "the change arms the timer"
        );
        assert!(
            !switch.update(HIGH, Millis::new(20)),
            "20 ms is not > 20 ms"
        );
        assert!(switch.update(HIGH, Millis::new(21)), "21 ms is");
    }

    #[test]
    fn a_bouncing_contact_never_registers() {
        // A 5 ms-period bounce for 200 ms: the timer is reset every edge, so
        // the state never updates.
        let mut switch = momentary();
        let mut registered = 0;
        for ms in 0..200u32 {
            let raw = if (ms / 3) % 2 == 0 { HIGH } else { LOW };
            if switch.update(raw, Millis::new(ms)) {
                registered += 1;
            }
        }
        assert_eq!(registered, 0, "a 3 ms bounce must not register");
    }

    #[test]
    fn a_contact_that_settles_registers_after_exactly_one_interval() {
        // Bounce for 30 ms, then settle.
        let mut switch = momentary();
        let mut pressed_at = None;
        for ms in 0..200u32 {
            let raw = if ms < 30 && (ms / 3) % 2 == 0 {
                LOW
            } else {
                HIGH
            };
            if switch.update(raw, Millis::new(ms)) && pressed_at.is_none() {
                pressed_at = Some(ms);
            }
        }
        // The last LOW is at ms=26 (26/3 = 8, even), so the LOW->HIGH edge that
        // arms the timer is at ms=27, and the debounce expires strictly more
        // than 20 ms later: 27 + 21 = 48.
        assert_eq!(pressed_at, Some(48), "expected the first settled reading");
    }

    #[test]
    fn releasing_also_debounces() {
        let mut switch = momentary();
        let _ = switch.update(HIGH, Millis::ZERO);
        assert!(switch.update(HIGH, Millis::new(25)));
        assert!(switch.is_pressed());
        // The release is debounced too, and the debounce is armed by the
        // falling edge at t=30, so the state does not change until t=51.
        assert!(
            switch.update(LOW, Millis::new(30)),
            "still pressed at the edge"
        );
        assert!(
            switch.update(LOW, Millis::new(50)),
            "still pressed inside the debounce"
        );
        assert!(
            !switch.update(LOW, Millis::new(51)),
            "released once the debounce expires"
        );
        assert!(!switch.is_pressed());
    }

    #[test]
    fn the_debounce_timer_is_reset_by_the_raw_pin_not_the_inverted_one() {
        // (1) resets on `reading != lastState`, the raw value. For a
        // NORMALLY_CLOSED switch the *logical* edge is the opposite one, so a
        // port that debounced on the inverted value would accept a bounce on the
        // wrong edge.
        let mut switch = Debounced::new(SwitchType::Momentary, SwitchMode::NormallyClosed, LOW);
        // Pin low = contact closed = pressed, for a normally-closed switch.
        assert!(!switch.update(LOW, Millis::ZERO));
        // Pressed after the debounce.
        assert!(switch.update(LOW, Millis::new(25)));
        assert!(switch.is_pressed());
        // Now bounce the *other* way; the timer resets each time.
        for ms in 25..80u32 {
            let raw = if (ms / 2) % 2 == 0 { HIGH } else { LOW };
            let _ = switch.update(raw, Millis::new(ms));
        }
        assert!(switch.is_pressed(), "the bounce did not release it");
    }

    // ========================================================== mode inversion

    #[test]
    fn a_normally_closed_switch_inverts() {
        // `reading ^ mapped_mode` (`IOSwitch.cpp:31`): NORMALLY_CLOSED is 1, so
        // a low pin is "pressed".
        let mut switch = Debounced::new(SwitchType::Momentary, SwitchMode::NormallyClosed, HIGH);
        // Rest is high (contact open), so not pressed. The first read matches
        // `initial_raw`, so no debounce is armed and the state is HIGH-inverted
        // to LOW immediately.
        assert!(!switch.update(HIGH, Millis::ZERO));
        assert!(!switch.update(HIGH, Millis::new(25)));
        assert!(!switch.is_pressed());
        // Pulled low: the falling edge at t=30 arms the debounce, so the state
        // changes at t=51.
        assert!(
            !switch.update(LOW, Millis::new(30)),
            "still open at the edge"
        );
        assert!(
            !switch.update(LOW, Millis::new(50)),
            "still open inside the debounce"
        );
        assert!(switch.update(LOW, Millis::new(51)));
        assert!(switch.is_pressed());
    }

    #[test]
    fn a_normally_open_switch_does_not_invert() {
        let mut switch = Debounced::new(SwitchType::Momentary, SwitchMode::NormallyOpen, LOW);
        assert!(!switch.update(LOW, Millis::ZERO));
        assert!(!switch.update(LOW, Millis::new(25)));
        assert!(!switch.is_pressed());
        // The rising edge at t=30 arms the debounce; the state changes at t=51.
        assert!(
            !switch.update(HIGH, Millis::new(30)),
            "still open at the edge"
        );
        assert!(
            !switch.update(HIGH, Millis::new(50)),
            "still open inside the debounce"
        );
        assert!(switch.update(HIGH, Millis::new(51)));
        assert!(switch.is_pressed());
    }

    // ============================================================= long press

    #[test]
    fn a_momentary_switch_reports_a_long_press_at_500_ms() {
        let mut switch = momentary();
        let _ = switch.update(HIGH, Millis::ZERO);
        let mut reported_at = None;
        for ms in 0..800u32 {
            let _ = switch.update(HIGH, Millis::new(ms));
            if reported_at.is_none() && switch.long_press_detected() {
                reported_at = Some(ms);
            }
        }
        // `pressStartTime` is the call that set the state to HIGH, which is the
        // first one after the debounce, so 500 ms is measured from there.
        assert_eq!(reported_at, Some(521), "500 ms after the press registered");
    }

    #[test]
    fn a_short_press_never_reports_a_long_press() {
        let mut switch = momentary();
        let mut reported = false;
        for ms in 0..400u32 {
            let _ = switch.update(HIGH, Millis::new(ms));
            if switch.long_press_detected() {
                reported = true;
            }
            let _ = switch.update(LOW, Millis::new(ms + 100));
        }
        assert!(!reported, "400 ms is not a long press");
    }

    #[test]
    fn s4_the_long_press_flag_is_cleared_by_the_call_that_sees_the_release() {
        // PRESERVED C++ BEHAVIOUR, and it is a *reassuring* one.
        //
        // `IOSwitch.cpp:44-51` runs the long-press block after the state update,
        // and the clear condition is
        // `currentState == LOW && lastStateChangeTime == currentTime`. The
        // transition to LOW sets `lastStateChangeTime` on the *same* call, so
        // the equality holds on exactly the call that observed the release --
        // which means the flag cannot outlive the press.
        //
        // Worth pinning because the equality looks like it should be a `<=`: as
        // written it is doing a job a comparison would do badly. The flag is
        // therefore *level*-triggered. A caller gets "a long press is happening
        // right now" and not "a long press happened", so a press that starts
        // and ends between two polls is invisible. That is the C++'s contract
        // and it is preserved; the tests below pin both halves of it.
        let mut switch = momentary();
        for ms in 0..700u32 {
            let _ = switch.update(HIGH, Millis::new(ms));
        }
        assert!(
            switch.long_press_detected(),
            "held for 700 ms, the flag is set"
        );
        // The release: the falling edge arms the debounce, and the state change
        // lands on the call where it expires -- which is the call that clears.
        let _ = switch.update(LOW, Millis::new(700));
        assert!(
            switch.long_press_detected(),
            "inside the release debounce the flag is still set"
        );
        let _ = switch.update(LOW, Millis::new(721));
        assert!(
            !switch.long_press_detected(),
            "the flag clears on the call that observes the release"
        );
    }

    #[test]
    fn a_press_shorter_than_the_loop_interval_is_invisible() {
        // The other half of the C++'s contract, and the reason the previous
        // test matters: a long press that starts and ends between two polls is
        // never reported at all. A 400 ms press with the loop at 1 Hz, say.
        let mut switch = momentary();
        for ms in 0..100u32 {
            let _ = switch.update(HIGH, Millis::new(ms));
        }
        assert!(!switch.long_press_detected());
        // The pin is back low at the next poll.
        let _ = switch.update(LOW, Millis::new(1_000));
        let _ = switch.update(LOW, Millis::new(1_030));
        assert!(!switch.long_press_detected());
    }

    #[test]
    fn s3_a_toggle_switch_never_reports_a_long_press() {
        // PRESERVED C++ BEHAVIOUR, and required: the water-tank switch is a
        // TOGGLE (`SystemInitializer.cpp:75`), so it must not be able to raise a
        // long press. `longPressDetected` checks the type first
        // (`IOSwitch.cpp:57-59`) and `isPressed` gates the whole long-press
        // block on `type_ == MOMENTARY` (`IOSwitch.cpp:44`), so there are two
        // independent guards in the C++ and both are kept.
        let mut switch = toggle();
        for ms in 0..5_000u32 {
            let _ = switch.update(HIGH, Millis::new(ms));
            assert!(
                !switch.long_press_detected(),
                "a toggle reported a long press at {ms} ms"
            );
        }
        // Held for 5 s and still nothing.
        assert!(switch.is_pressed());
        assert!(!switch.long_press_detected());
    }

    #[test]
    fn a_toggle_switch_keeps_its_state_when_released() {
        // A toggle retains its position, so the C++'s debouncer cannot do
        // anything about it: the *contact* is what latches. What is preserved
        // is that the debouncer reports the contact's state without a long-press
        // side effect.
        let mut switch = toggle();
        let _ = switch.update(HIGH, Millis::new(0));
        assert!(switch.update(HIGH, Millis::new(25)));
        // "Released" as far as the pin is concerned: the falling edge at t=30
        // arms the debounce, so the state changes at t=51.
        assert!(switch.update(LOW, Millis::new(30)), "still on at the edge");
        assert!(
            switch.update(LOW, Millis::new(50)),
            "still on inside the debounce"
        );
        assert!(
            !switch.update(LOW, Millis::new(51)),
            "off once the debounce expires"
        );
        assert!(!switch.is_pressed());
        assert!(!switch.long_press_detected());
    }

    // =========================================================== the water tank

    #[test]
    fn s2_the_c_tank_starts_full_and_the_switch_starts_open() {
        // Two C++ facts that look contradictory and are not:
        //
        // * `SensorCoordinator.h:260` — `std::atomic<bool> waterTankFull_{true}`
        //   with the comment "Assume full initially". So the machine may pump on
        //   the very first tick.
        // * `IOSwitch.cpp:19` — `currentState(LOW)`, so the switch reports "not
        //   pressed", i.e. "no water", until its first settled read.
        //
        // Which means: on a cold boot with a full tank, the coordinator's
        // `updateWaterTank` is rate-limited to 200 ms
        // (`SensorCoordinator.h:272`) *and* gated on the debounce, so the tank
        // reads **empty** for the first ~220 ms. Both preserved.
        //
        // The switch's own construction, from `createWaterTankSensor`:
        //   initialState = (mode == NORMALLY_OPEN) ? HIGH : LOW
        let open_tank = Debounced::new(SwitchType::Toggle, SwitchMode::NormallyOpen, HIGH);
        assert!(!open_tank.is_pressed(), "the switch starts 'no water'");
        let closed_tank = Debounced::new(SwitchType::Toggle, SwitchMode::NormallyClosed, LOW);
        assert!(!closed_tank.is_pressed(), "and so does the other wiring");
    }

    #[test]
    fn the_water_tank_pin_pull_follows_the_wiring() {
        // `SystemInitializer.cpp:71-74`: normally open -> `IN_PULLDOWN`, normally
        // closed -> `IN_PULLUP`.
        //
        // The pull itself is a device concern (R3-02's `GpioIn`); what is pinned
        // here is the *level* that wiring implies, since that is the number the
        // debouncer is seeded with and the only one the port can see.
        //
        // Normally open with a pull-down: the contact is open at rest, the
        // pull-down holds the pin LOW, and water closes the contact to 3V3. So
        // the rest level is LOW and "water present" is HIGH.
        let mut tank = Debounced::new(SwitchType::Toggle, SwitchMode::NormallyOpen, LOW);
        assert!(!tank.update(LOW, Millis::ZERO));
        assert!(!tank.update(LOW, Millis::new(25)));
        assert!(!tank.is_pressed(), "no water");
        // The rising edge at t=30 arms the debounce; the state changes at t=51.
        assert!(!tank.update(HIGH, Millis::new(30)), "no water at the edge");
        assert!(
            !tank.update(HIGH, Millis::new(50)),
            "no water inside the debounce"
        );
        assert!(tank.update(HIGH, Millis::new(51)));
        assert!(tank.is_pressed(), "water present");
        assert!(!tank.long_press_detected());
    }

    // ================================================================ misc

    #[test]
    fn the_accessors_report_the_construction() {
        let switch = toggle();
        assert_eq!(switch.switch_type(), SwitchType::Toggle);
        assert_eq!(switch.mode(), SwitchMode::NormallyOpen);
    }

    #[test]
    fn any_pin_value_is_normalised_to_two_levels() {
        // The C++'s `gpio.read()` returns 0 or 1, and the XOR arithmetic assumes
        // it. A driver that returned anything else would corrupt `currentState`.
        let mut switch = momentary();
        for raw in [0u8, 1, 2, 255] {
            let _ = switch.update(raw, Millis::ZERO);
        }
        // Still coherent: with the pin bouncing on garbage the debounce never
        // expires, so nothing is registered.
        assert!(!switch.is_pressed());
    }
}
