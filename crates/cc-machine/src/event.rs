//! What can happen to the machine, and what the outside world can ask for.
//!
//! An [`Event`] is the *only* way anything enters the reducer. There is no other door: no
//! handle, no singleton, no global configuration accessor. That is the mechanism behind the
//! rule that adding a feature means adding an `Event` variant and a `reduce` arm.
//!
//! Every variant is `Copy` and fixed-size, because cross-task messages travel over
//! `hal::task::queue::Queue<Command, 32>` and that queue is bounded by `T: Copy`. A `String`,
//! a `Vec` or a `Box` in [`Command`] would make the event type unusable across the task
//! boundary, and the compiler refuses it rather than a comment warning about it.
use cc_domain::units::{Celsius, Millis};

/// A sampled sensor reading.
///
/// What the state machine actually reads, and nothing else. Note what is **not** here:
/// pressure and the raw scale weight. The state machine only logs those, so carrying them
/// would enlarge the event for no decision's sake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sensors {
    /// Latest temperature, already offset by `brew.temp_offset`.
    pub temperature: Celsius,
    /// Whether the water tank is full.
    pub water_tank_full: bool,
    /// Whether the temperature probe has failed.
    pub has_temperature_error: bool,
    /// Whether the scale has failed.
    pub has_scale_error: bool,
    /// The current brew weight, in grams.
    pub brew_weight: f32,
    /// How many temperature samples this carries — the probe's own conversion
    /// counter, so a consumer can tell a **new reading** from the same reading
    /// looked at again.
    ///
    /// It exists for S1's debounce. `cc_safety::reduce` is called once per
    /// control tick (10 ms) but the probe converts at 2.5 Hz, so without this the
    /// over-temperature debounce counted one reading forty times and latched on a
    /// spike. The counter is threaded through `Sensors` rather than taken from the
    /// shell separately so that it cannot get out of step with the reading it
    /// describes.
    pub sample_seq: u32,
}

impl Sensors {
    /// A healthy sample: 25 °C, tank full, no faults, no weight.
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            sample_seq: 0,
            temperature: Celsius::new(25.0),
            water_tank_full: true,
            has_temperature_error: false,
            has_scale_error: false,
            brew_weight: 0.0,
        }
    }

    /// Whether any enabled sensor has an error: the temperature probe **or** the scale.
    ///
    /// No scale driver is built, so the second term is always false. It is kept so that
    /// re-adding a scale cannot silently change a guard.
    #[must_use]
    pub const fn has_sensor_error(&self) -> bool {
        self.has_temperature_error || self.has_scale_error
    }
}

/// A sampled switch edge.
///
/// The switch layer polls its pins and compares each reading with the previous one, so the
/// edge *is* the event and the level is a consequence of it. A sample that matches the stored
/// level therefore produces no [`Event`] at all.
///
/// The reducers handle a repeated edge defensively anyway (see `handlers::brew_switch`):
/// [`Machine::switches`](crate::Machine) is public, so a caller *can* construct a bogus edge,
/// and a safety-relevant path must be total rather than trusting its input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A new sensor sample.
    ///
    /// Fed in before anything reads a sensor, so the safety verdict, the guards and the state
    /// machine all react to the same sample in the same tick.
    SensorUpdated(Sensors),

    /// The switch went from released to pressed.
    ///
    /// `long_press` reports the switch layer's long-press flag, which tells a short press
    /// from a long one: in `BACKFLUSH_IDLE` the first starts a backflush cycle and the second
    /// starts a manual flush.
    ButtonPressed {
        /// Which physical switch moved.
        switch: SwitchId,
        /// The hardware long-press flag at the moment of the edge.
        long_press: bool,
    },

    /// The switch went from pressed to released.
    ButtonReleased {
        /// Which physical switch moved.
        switch: SwitchId,
    },

    /// A request from outside the switch layer: the web UI, MQTT, or the power handler's
    /// derived actions.
    ///
    /// These arrive on a bounded queue and are folded into the machine's request flags there.
    /// Nothing outside the control task reaches control state directly.
    Command(Command),

    /// One control-loop iteration, with the clock reading.
    ///
    /// This is the only event that advances [`Machine::now`](crate::Machine::now).
    /// The reducer never reads a clock itself.
    ///
    /// # Why the reading and not an elapsed delta
    ///
    /// An elapsed delta makes the reducer's timeouts depend on the *sum* of every delta the
    /// shell chose to send, so a shell bug silently changes a safety timeout and no test can
    /// see it. Passing the absolute reading makes every timeout a comparison against
    /// [`Machine::entry_at`](crate::Machine::entry_at), which is directly assertable, and it
    /// makes the `u32` `millis()` rollover of `Millis::since` explicit. The shell is the only
    /// clock.
    Tick {
        /// The current `millis()`-equivalent reading.
        now: Millis,
    },

    /// A fresh PID output, in milliseconds of on-time per 1000 ms window.
    ///
    /// The reducer does **not** compute the PID: `cc-domain` does, and its vectors are pinned
    /// against the reference implementation. The reducer decides whether that output is
    /// *permitted to reach the heater*, which is a state-machine question.
    PidOutput(f32),

    /// A safety outcome from `cc-safety`, folded in whole.
    ///
    /// This is how the S1-S5 latch enters the machine: the shell runs
    /// `cc_safety::reduce(&machine.safety, ...)` once per tick and hands the result over as
    /// an event, before the PID output is applied.
    ///
    /// # Why the whole `Outcome` and not just the `Verdict`
    ///
    /// S1 is a **three-reading debounce**, so `SafetyState::high_reading_count` is part of the
    /// machine's memory and has to survive between ticks. Passing only the verdict would force
    /// the shell to keep a second copy of the counter, and two copies of one latch are a
    /// standing invitation to update them out of step. Here there is one copy, in
    /// [`Machine::safety`](crate::Machine::safety).
    Safety(cc_safety::Outcome),
}

/// Which physical switch produced an edge: the four the machine has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwitchId {
    /// `hardware.switches.brew` — also the backflush and manual-flush switch.
    Brew,
    /// `hardware.switches.steam`.
    Steam,
    /// `hardware.switches.power` — standby and reboot.
    Power,
    /// `hardware.switches.hot_water` — hot water, and steam water injection.
    ///
    /// Named for the switch, not the function: the same switch is the hot-water
    /// switch in `PID_NORMAL` and the boiler water-injection switch in
    /// `STEAM_RUNNING`.
    HotWater,
}

impl SwitchId {
    /// Every switch, for the exhaustive `state x event` table.
    pub const ALL: [SwitchId; 4] = [
        SwitchId::Brew,
        SwitchId::Steam,
        SwitchId::Power,
        SwitchId::HotWater,
    ];

    /// A short, stable name for logs and test output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Brew => "brew",
            Self::Steam => "steam",
            Self::Power => "power",
            Self::HotWater => "hot_water",
        }
    }
}

/// A request from outside the switch layer.
///
/// Each variant is one request the machine can be asked to honour, plus, where the flag has a
/// matching persisted setting, the value that changes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Start a brew: the web `brew/start` path.
    BrewStart,
    /// Stop the running brew.
    BrewStop,
    /// Start steam.
    SteamStart,
    /// Stop steam.
    SteamStop,
    /// Start a manual flush.
    ManualFlushStart,
    /// Stop the manual flush.
    ManualFlushStop,
    /// Ask for backflush mode. Half of the backflush-mode toggle.
    ///
    /// The *cycle* logic behind the toggle -- whether it actually enables backflush mode, and
    /// whether it clears the pending requests -- is
    /// [`crate::backflush::apply_backflush_mode`], a pure function of (`was_active`,
    /// `requested`, `configured_cycles`). This command only sets the request flag that
    /// function reads.
    BackflushEnter,
    /// Set backflush mode to this value: the entry point `POST /api/backflush` calls, with
    /// `!backflush_mode()`.
    ///
    /// **This is the command the toggle needs and did not have.** With only a mode-on command
    /// and a cycle-stop command, turning backflush mode *off* had no way to clear
    /// `backflush.on`: the firmware fed the cycle-stop, which leaves the mode flag set.
    /// Measured on a bench ESP32 -- four consecutive toggles, including the explicit `?on=0`,
    /// all answered `{"backflushOn":true}` and the machine stayed in `BACKFLUSH_IDLE`.
    ///
    /// One command carrying the value is also the shape the decision function wants:
    /// [`crate::backflush::apply_backflush_mode`] is already a pure function of
    /// `(was_active, requested, cycles)`.
    SetBackflushMode(bool),
    /// Start the next backflush cycle.
    BackflushCycleStart,
    /// Stop the backflush cycle.
    BackflushStop,
    /// Enter standby: the power-off path.
    Standby,
    /// Leave standby: the power-on path.
    NormalOperation,
    /// Set the PID's enabled state: persists `pid.enabled` **and** sets the runtime flag.
    ///
    /// One command rather than two, because splitting them would let a caller persist a
    /// preference without activating it, or activate a PID the configuration does not enable
    /// -- drift that a state's exit would then have to undo.
    SetUserPidEnabled(bool),
    /// Reboot the machine: show the message, shut the hardware down safely, and restart.
    Reboot,
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn every_switch_has_a_distinct_name() {
        let names: Vec<&str> = SwitchId::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names, ["brew", "steam", "power", "hot_water"]);
    }

    #[test]
    fn a_healthy_sample_is_valid_and_has_no_faults() {
        let sample = Sensors::healthy();
        assert!(sample.temperature.is_valid());
        assert!(sample.water_tank_full);
        assert!(!sample.has_sensor_error());
    }

    #[test]
    fn a_scale_fault_alone_counts_as_a_sensor_error() {
        // The two fault flags are ORed, so either one alone reports a sensor error.
        let sample = Sensors {
            has_scale_error: true,
            ..Sensors::healthy()
        };
        assert!(sample.has_sensor_error());
    }

    #[test]
    fn a_temperature_fault_alone_counts_as_a_sensor_error() {
        let sample = Sensors {
            has_temperature_error: true,
            ..Sensors::healthy()
        };
        assert!(sample.has_sensor_error());
    }
}
