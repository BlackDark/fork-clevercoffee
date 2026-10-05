//! What can happen to the machine, and what the outside world can ask for.
//!
//! An [`Event`] is the *only* way anything enters the reducer. There is no
//! other door: no handle, no singleton, no `Config::getInstance()`. That is the
//! mechanism behind 04 §3.1's "adding a feature means adding an `Event` variant
//! and a `reduce` arm".
//!
//! Every variant is `Copy` and fixed-size. 04 §3.2 requires cross-task messages
//! to be `hal::task::queue::Queue<Command, 32>`-compatible, and that queue is
//! bounded by `T: Copy` — so a `String`, a `Vec` or a `Box` in [`Command`] would
//! make the event type unusable across the task boundary. The compiler enforces
//! that here rather than in a comment.

use cc_domain::units::{Celsius, Millis};

/// A sampled sensor reading.
///
/// Mirrors what `SensorCoordinator` exposes and what the state machine actually
/// reads. Note what is **not** here: pressure and the raw scale weight. The
/// state machine only logs those (`BrewStates.cpp:100-103`), so carrying them
/// would make the event bigger for no decision's sake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sensors {
    /// Latest temperature, already offset by `brew.temp_offset` the way
    /// `ProcessController::updateTemperature` does
    /// (`ProcessController.cpp:131-141`).
    pub temperature: Celsius,
    /// `SensorCoordinator::isWaterTankFull()` (`SensorCoordinator.h:180`).
    pub water_tank_full: bool,
    /// `SensorCoordinator::hasTemperatureSensorError()`.
    pub has_temperature_error: bool,
    /// `SensorCoordinator::hasScaleSensorError()`.
    pub has_scale_error: bool,
    /// `SensorCoordinator::getBrewWeight()`, grams.
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

    /// `SensorCoordinator::hasSensorError()` — "any *enabled* sensor has an
    /// error" (`SensorCoordinator.h:190-192`), which is the temperature probe
    /// **or** the scale. The scale is gone after R2-07, so on the Rust machine
    /// the second term is always false; it is kept so the name and the C++
    /// agree and so re-adding a sensor does not silently change a guard.
    #[must_use]
    pub const fn has_sensor_error(&self) -> bool {
        self.has_temperature_error || self.has_scale_error
    }
}

/// A sampled switch edge.
///
/// The C++ handlers poll `Switch::isPressed()` every loop and compare with the
/// previous reading (`BrewHandler.h:172`, `SteamHandler.h:103`). The edge *is*
/// the event; the level is a consequence of it. So there is no "level changed
/// to the same value" event, and a sample that matches the stored level
/// produces no [`Event`] at all — exactly the C++'s
/// `if (reading != lastSwitchReading_)`.
///
/// The reducers handle a repeated edge defensively anyway (see
/// `handlers::brew_switch`): [`Machine::switches`](crate::Machine) is public, so
/// a caller *can* construct a bogus edge, and a safety-relevant path must be
/// total rather than trusting its input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A new sensor sample.
    ///
    /// The C++ does this in `LoopManager` step 2, *before* the state machine
    /// (`LoopManager.cpp:135-138`), so the guards react to the same sample in
    /// the same tick. Delivering it as an event preserves that.
    SensorUpdated(Sensors),

    /// The switch went from released to pressed.
    ///
    /// `long_press` is `Switch::longPressDetected()` (`hardware/Switch.h:28`),
    /// which `BrewHandler` consults to tell a short press (start a backflush
    /// cycle) from a long press (start a manual flush) in `BACKFLUSH_IDLE`
    /// (`BrewHandler.h:198-200`).
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

    /// A request from outside the switch layer: the web UI, MQTT, or the power
    /// handler's derived actions.
    ///
    /// In the C++ these are the `context->setXxxRequested(true)` calls made
    /// from `PowerHandler`, the HTTP handlers and `MQTTManager`. They arrive on a
    /// bounded queue and are folded here, never called directly into control
    /// state (04 §3.2).
    Command(Command),

    /// One control-loop iteration, with the clock reading.
    ///
    /// This is the only event that advances [`Machine::now`](crate::Machine::now).
    /// The reducer never reads a clock itself.
    ///
    /// # Why the reading and not an elapsed delta
    ///
    /// The brief sketches `Tick { elapsed }`. An elapsed delta makes the
    /// reducer's timeouts depend on the *sum* of every delta the shell chose to
    /// send, so a shell bug silently changes a safety timeout and no test can
    /// see it. Passing the absolute reading makes every timeout a comparison
    /// against [`Machine::entry_at`](crate::Machine::entry_at), which is
    /// directly assertable, and it makes the `u32` `millis()` rollover of
    /// `Millis::since` explicit. The shell is the only clock (04 §3.1).
    Tick {
        /// The current `millis()`-equivalent reading.
        now: Millis,
    },

    /// A fresh PID output, in the C++'s 0-1000 duty units.
    ///
    /// The reducer does **not** compute the PID (`cc-domain` does, and it is
    /// bit-exact with the Arduino library). It decides whether that output is
    /// *permitted to reach the heater*, which is a state-machine question.
    PidOutput(f32),

    /// A safety outcome from `cc-safety`, folded in whole.
    ///
    /// This is how the S1-S5 latch enters the machine: the shell runs
    /// `cc_safety::reduce(&machine.safety, ...)` once per tick and hands the
    /// result over as an event, exactly as `LoopManager` step 5 runs
    /// `ProcessController::testEmergencyConditions` before the PID computes
    /// (`ProcessController.cpp:109-113`).
    ///
    /// # Why the whole `Outcome` and not just the `Verdict`
    ///
    /// The brief sketches `Safety(SafetyVerdict)`. A verdict alone is not enough:
    /// S1 is a **three-reading debounce** (`EmergencyStopManager.cpp:46`), so
    /// `SafetyState::high_reading_count` is part of the machine's memory and has
    /// to survive between ticks. Passing only the verdict would force the shell
    /// to keep a second copy of the counter, and the C++ already has that exact
    /// bug shape — `EmergencyStopManager::emergencyActive_` and
    /// `MachineStateContext::emergencyStop_` are two copies of one latch that
    /// have to be updated in step (`ProcessController.cpp:336`,
    /// `EmergencyStopState.cpp:59-63`). Here there is one copy, in
    /// [`Machine::safety`](crate::Machine::safety).
    Safety(cc_safety::Outcome),
}

/// Which physical switch produced an edge.
///
/// The four the C++ has: `HardwareManager::getBrewSwitch`,
/// `getSteamSwitch`, `getHotWaterSwitch`, `getPowerSwitch`
/// (`MachineStateContext.cpp:44-58`).
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
    /// `STEAM_RUNNING` (`PidStates.cpp:33`, `SteamStates.cpp:36`).
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
/// Each variant is the C++'s `context->setXxxRequested(true)`, plus, where the
/// C++ has one, the `setXxx` that also changes a persistent flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `setBrewStartRequested(true)` — the web `brew/start` path.
    BrewStart,
    /// `setBrewStopRequested(true)`.
    BrewStop,
    /// `setSteamStartRequested(true)`.
    SteamStart,
    /// `setSteamStopRequested(true)`.
    SteamStop,
    /// `setManualFlushStartRequested(true)`.
    ManualFlushStart,
    /// `setManualFlushStopRequested(true)`.
    ManualFlushStop,
    /// `setBackflushEnterRequested(true)` — the backflush-mode toggle.
    ///
    /// The *cycle* logic behind this — whether the toggle actually enables
    /// backflush mode, and whether it clears the pending requests — is
    /// [`crate::backflush::apply_backflush_mode`], a pure function of
    /// (`was_active`, `requested`, `configured_cycles`) ported from
    /// `backflush/BackflushModeLogic.h:26-34`. The command is only the request
    /// flag that function sets.
    BackflushEnter,
    /// `setBackflushMode(bool)` (`MachineStateContext.cpp:354-381`) — the C++'s
    /// own entry point, which `POST /api/backflush` calls with
    /// `!backflushMode()` (`WebServerManager.cpp:489-491`).
    ///
    /// **This is the command the toggle needs and did not have.** There was a
    /// `BackflushEnter` (mode on) and a `BackflushStop` (stop the running
    /// cycle), so turning backflush mode *off* had no command that clears
    /// `backflush.on`: the firmware fed `BackflushStop`, which leaves the mode
    /// flag set. Measured on a bench ESP32 — four consecutive toggles, including
    /// the explicit `?on=0`, all answered `{"backflushOn":true}` and the
    /// machine stayed in `BACKFLUSH_IDLE`.
    ///
    /// One command carrying the value is also the faithful shape: the C++
    /// computes the new state in the web handler and passes it down, and the
    /// decision function ([`crate::backflush::apply_backflush_mode`]) is already
    /// a pure function of `(was_active, requested, cycles)`.
    SetBackflushMode(bool),
    /// `setBackflushCycleStartRequested(true)`.
    BackflushCycleStart,
    /// `setBackflushStopRequested(true)`.
    BackflushStop,
    /// `setStandbyRequested(true)` — the power-off path
    /// (`PowerHandler::powerOff`, `PowerHandler.h:170`).
    Standby,
    /// `setNormalOperationRequested(true)` — the power-on path
    /// (`PowerHandler::powerOn`, `PowerHandler.h:156`).
    NormalOperation,
    /// `setUserPidEnabled(bool)` (`SystemUtils.h:34-40`): persists
    /// `pid.enabled` **and** sets the runtime flag.
    ///
    /// The C++ has these as two separate calls, and splitting them here would
    /// let a caller persist a preference without activating it, or activate a
    /// PID that config does not enable — the exact drift that
    /// `EmergencyStopState::onExitImpl` exists to undo
    /// (`EmergencyStopState.cpp:29-30`). One command, both effects.
    SetUserPidEnabled(bool),
    /// A reboot request. `PowerHandler::triggerSystemReboot`
    /// (`PowerHandler.h:177-192`) shows the message, shuts down safely, and
    /// restarts.
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
        // `SensorCoordinator.h:190-192` ORs the temperature and scale flags.
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
