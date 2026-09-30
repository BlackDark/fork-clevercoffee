//! The control loop's shell: the reducer, the PID, and the clock.
//!
//! Owner: **R4-01**.
//!
//! # What this is
//!
//! 04 §3.1's `tick()`, written out. Four steps, in this order, and the order is
//! the C++'s (`LoopManager::update`, `src/core/LoopManager.cpp:90-253`):
//!
//! ```text
//! SENSE   the shell reads hardware into plain values          (step 2, C++)
//! DECIDE  fold events through cc_machine::reduce               (steps 3-5, C++)
//! ACT     cc_machine::apply -> cc_hal_esp32::Actuators         (steps 3-5, C++)
//! NOTIFY  publish telemetry to the slow consumers              (steps 6-8, C++)
//! ```
//!
//! Everything in DECIDE is a pure function in `cc-machine` with 420 host tests
//! behind it. Everything in ACT is [`cc_hal_esp32::Actuators`], which is the only
//! code in the firmware that can change an actuator pin. The shell's job is to
//! be the *only* thing that knows both exist.
//!
//! # Why the PID lives here and not in the reducer
//!
//! The C++'s `PID` is a global `PID_v1` object the state machine reaches through
//! `SystemContext`. `cc-domain`'s [`Controller`] is a bit-exact port of that
//! library and is a **value**, and `cc-machine` takes its output as
//! [`cc_machine::Event::PidOutput`] — a number the shell computed. So the
//! division is the one 04 §3.1 asks for: the reducer decides whether a duty may
//! *reach* the heater, and the shell decides what the duty *is*.
//!
//! `apply_pid_output`'s own documentation records the one visible consequence:
//! this port gates the emitted `SetHeaterDuty` where the C++ emits the value and
//! zeroes it microseconds later. Same machine state, strictly safer ordering.
//!
//! # The clock
//!
//! One clock, read once per tick, published to the reducer as
//! [`cc_machine::Event::Tick`] and to the actuator facade as
//! [`cc_hal_esp32::Actuators::set_now`]. The reducer never reads a clock
//! (04 §3.1) and the facade's methods take none, so this is the only place the
//! two can be made to agree.

use cc_config::Config;
use cc_domain::pid::{Controller, ControllerDirection, Mode, ProportionalOn};
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{reduce, Context, Event, Machine, Sensors};
use cc_safety::{SafetyConfig, SafetyState, Telemetry};
use log::{debug, info, warn};

/// The heater chopper's window, in milliseconds.
///
/// `ProcessState::windowSize_` (`include/clevercoffee/context/ProcessState.h:183`)
/// and `cc_config::Config::HEATER_WINDOW_MS`. The PID's output limits and its
/// sample time are both this number (`SystemInitializer.cpp:551-552`), so it is
/// named once here rather than spelled three times.
const WINDOW_MS: u32 = Config::HEATER_WINDOW_MS;

/// The control task's view of the machine: the reducer's state, the PID, and the
/// setpoint.
///
/// A struct rather than four locals because the tick is a list of steps and the
/// state those steps share is what makes the list a loop rather than a
/// re-initialisation. Every field is private and every mutation goes through a
/// method, so "what can change the machine state" has a short answer: the four
/// [`Self::feed`] methods and [`Self::tick`].
pub struct Control {
    /// `cc_machine::Machine` — the reducer's state.
    machine: Machine,
    /// The `PID_v1` port, as a value.
    pid: Controller,
    /// The `pid.enabled` value the reducer last decided, folded into the
    /// controller's `AUTOMATIC`/`MANUAL` mode.
    ///
    /// `ProcessController::setPIDEnabled` (`ProcessController.cpp:261-275`):
    /// enabling calls `SetMode(AUTOMATIC)`, which is the library's bumpless
    /// transfer. Doing it here rather than inside the reducer is what keeps the
    /// reducer free of the controller — and it is why the mode is applied
    /// *after* the tick, because the tick is what decides it.
    pid_mode_enabled: bool,
    /// The machine state the PID tunings were last chosen for.
    ///
    /// `ProcessController::updatePIDState`'s `lastMachineStatePid_`
    /// (`ProcessController.cpp:174`). The tunings change **only** on a state
    /// change, which is what stops the brew-detection gains from being applied
    /// in `PID_NORMAL` by a stale assignment.
    tuned_for: Option<MachineState>,
    /// The S1-S5 configuration, from `cc-config`'s `safety_view()`.
    safety: SafetyConfig,
    /// The active setpoint, °C.
    ///
    /// `brew.setpoint + brew.temp_offset` normally, `steam.setpoint` while steam
    /// mode is on — `ProcessController::updateSetpoint`
    /// (`ProcessController.cpp:235-244`). The offset is applied to the
    /// **setpoint**, not to the reading, which is the same arithmetic as the
    /// C++'s subtracting it from the reading
    /// (`ProcessController.cpp:138`) with the sign the parameter's help text
    /// describes ("offset added to the user-visible setpoint", `Config.h:808`).
    setpoint: f64,
}

impl Control {
    /// Bring the machine up, and say which state it starts in.
    ///
    /// `SystemInitializer::finalizeMachineState`
    /// (`src/core/SystemInitializer.cpp:604-641`), which reads the power switch
    /// *before* the state machine exists and hands it both an initial state and
    /// a runtime PID flag:
    ///
    /// | power switch | initial state | runtime PID |
    /// | --- | --- | --- |
    /// | momentary | `PID_NORMAL` | `true` |
    /// | toggle, ON | `PID_NORMAL` | `true` |
    /// | toggle, OFF | `PID_DISABLED` | `false` |
    /// | absent | `INIT` | `pid.enabled` |
    ///
    /// The effects are returned rather than applied here, because
    /// [`cc_hal_esp32::Actuators`] does not exist yet at this point in the boot
    /// sequence and the first effects are actuator writes.
    ///
    /// The PID is constructed with the C++'s exact call
    /// (`SystemInitializer.cpp:296-305`) followed by `initializePID`
    /// (`:536-566`): the same gains, the same 1000 ms sample time, the same
    /// `(0, 1000)` output limits, the same `(0, i_max)` integrator limits and
    /// the same EMA factor, in the same order. The order matters — `SetSampleTime`
    /// rescales `Ki` and `Kd` by the ratio of the periods, so setting the sample
    /// time before the tunings would give a different controller.
    #[must_use]
    pub fn boot(
        config: &Config,
        now: Millis,
        power_switch_pressed: Option<bool>,
    ) -> (Self, Vec<cc_machine::Effect>) {
        let (initial, runtime_pid) = initial_state(config, power_switch_pressed);
        let ctx = Context::new(config, celsius(config.effective_brew_setpoint()));
        let (machine, effects) = cc_machine::boot_in(initial, runtime_pid, now, &ctx);

        // `SystemInitializer::initializePID` (`SystemInitializer.cpp:536-566`).
        // `set_tunings` first, then `set_sample_time`: the latter rescales the
        // former by the period ratio, and the C++ relies on that.
        let (kp, ki, kd) = config.pid_tunings();
        let mut pid = Controller::new(
            now,
            kp,
            ki,
            kd,
            ProportionalOn::Error,
            ControllerDirection::Direct,
        );
        let _ = pid.set_sample_time(Millis::new(WINDOW_MS));
        let _ = pid.set_output_limits(0.0, f64::from(WINDOW_MS));
        let _ = pid.set_integrator_limits(0.0, config.pid.regular.i_max);
        pid.set_smoothing_factor(config.pid.ema_factor);

        let setpoint = effective_setpoint(config, false);
        // Read before the move: this is the controller's actual mode, which is
        // Manual at construction. See the field's comment for why the cache may
        // not be seeded from intent.
        let pid_mode_enabled = pid.in_automatic();
        let mut control = Self {
            machine,
            pid,
            // Seeded from the **controller**, not from what we intend the mode to
            // be. Those are not the same thing.
            //
            // `Controller::new` starts in Manual, and a cache seeded with
            // `runtime_pid` claimed the controller was already Automatic. The
            // first tick then saw `permitted == pid_mode_enabled`, never called
            // `set_mode(Automatic)`, and `compute()` returned `false` on every
            // tick: the machine sat in `PidNormal` with a live setpoint, a 7 K
            // error and a **permanently zero heater duty**. Found on hardware,
            // with a correct setpoint, a correct error sign and
            // `P=0.0 I=0.0 D=0.0`.
            //
            // `in_automatic()` is false at construction, so the first tick always
            // applies the mode whatever the configuration says.
            pid_mode_enabled,
            tuned_for: None,
            safety: safety_config(config),
            setpoint,
        };
        // The tunings are applied for the *initial* state, so the very first
        // `compute` in the C++ (which happens on the first
        // `updateProcessControl`, i.e. the first loop) uses the right gains.
        control.retune(config);
        info!(
            "control: booted in {initial:?} (runtime pid {runtime_pid}), \
             PID kp={kp:.3} ki={ki:.3} kd={kd:.3}, window {WINDOW_MS} ms, \
             setpoint {setpoint:.2} C"
        );
        (control, effects)
    }

    /// The machine's state, for `/api/status` and the transition log.
    #[must_use]
    pub const fn state(&self) -> MachineState {
        self.machine.state
    }

    /// The reducer's whole value, for the snapshot a side channel logs.
    #[must_use]
    pub const fn machine(&self) -> &Machine {
        &self.machine
    }

    /// The duty the PID last computed, in the C++'s 0-1000 units.
    ///
    /// This is the machine's belief, not a measurement of the pin: the pin is the
    /// 10 ms ISR's, and `cc_hal_esp32::Actuators::heater()` reports what the gate
    /// let through. `/api/status`'s `heaterPower` is `output / 10`, which is the
    /// C++'s own conversion (`WebServerManager.cpp:352`).
    #[must_use]
    pub fn pid_output(&self) -> f32 {
        self.machine.pid.output
    }

    /// The controller's error, integral and derivative terms, for the periodic
    /// log line.
    ///
    /// Read *after* a compute, so they describe the sample the duty came from.
    #[must_use]
    pub fn pid_terms(&self) -> (f64, f64, f64) {
        (
            self.pid.last_p_part(),
            self.pid.last_i_part(),
            self.pid.last_d_part(),
        )
    }

    /// The active setpoint, °C.
    #[must_use]
    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    /// Reset the shot counter — `POST /api/maintenance/reset-backflush-counter`.
    ///
    /// `MaintenanceCoordinator::resetSinceBackflush()`
    /// (`WebServerManager.cpp:528-537`). The counter lives in
    /// `Machine::shots_since_backflush` and the reducer only ever clears it as a
    /// *consequence* of entering `BACKFLUSH_FINISHED`
    /// (`Effect::ResetShotsSinceBackflush`), so an operator-initiated reset has no
    /// reducer event and this is the one place a shell may write a `Machine`
    /// field directly.
    ///
    /// It is deliberately the only such method. A second one would be the
    /// beginning of "the shell can reach into the state machine", which is the
    /// coupling 04 §3.1 exists to remove; one named operation the C++ also has is
    /// a port, and a general `&mut Machine` accessor is not.
    pub fn reset_shots_since_backflush(&mut self) {
        self.machine.shots_since_backflush = 0;
        info!("control: shots since backflush reset to 0");
    }

    /// Change the setpoint from outside — the web UI's `POST /api/setpoint`.
    ///
    /// # Panics
    ///
    /// Never. The value is a `f64` the C++ range-checks to `0..=150`
    /// (`WebServerManager.cpp:394`); this one is not re-checked because the only
    /// caller is the HTTP handler, which already did.
    pub fn set_setpoint(&mut self, celsius: f64) {
        self.setpoint = celsius;
    }

    /// The safety latch, as `cc-safety` last computed it.
    ///
    /// The facade's mirror of this is set from here, every tick, which is what
    /// makes the latch a single copy (see `cc_hal_esp32::actuators`'s module
    /// documentation on why the C++'s two copies are a bug).
    #[must_use]
    pub const fn safety_state(&self) -> SafetyState {
        self.machine.safety
    }

    /// One control period.
    ///
    /// `sensors` and `edges` are the shell's measurements; `now` is the clock.
    /// The returned effects are to be applied **in order**, front to back, and
    /// are the complete actuator history of the period.
    ///
    /// The event order is the C++'s and is load-bearing:
    ///
    /// 1. [`Event::SensorUpdated`] — C++ step 2, before anything reads a sensor.
    /// 2. [`Event::Safety`] — C++ `testEmergencyConditions`, which
    ///    `ProcessController.cpp:109-113` runs *before* the PID computes.
    /// 3. the switch edges — C++ step 3, so a brew press sets `brew_start` and
    ///    the [`Event::Tick`] in the same period consumes it.
    /// 4. [`Event::PidOutput`] — C++ `computePID` (`:112`).
    /// 5. [`Event::Tick`] — C++ steps 3-5: standby, the state machine, the
    ///    handler tail, the valve fail-safes and process control.
    ///
    /// [`Self::feed`] is public and is the same operation for one event, because
    /// the command queue is drained **before** the tick and a `POST /api/brew`
    /// must not wait for the next period to be acted on.
    pub fn tick(
        &mut self,
        config: &Config,
        sensors: Sensors,
        edges: &[Event],
        now: Millis,
    ) -> Vec<cc_machine::Effect> {
        let mut effects = Vec::new();

        // ---- 1. SENSE -> DECIDE: the sample -------------------------------
        self.feed(config, Event::SensorUpdated(sensors), &mut effects);

        // ---- 2. the safety verdict, before the PID sees the temperature ---
        // `cc_safety::reduce` is a pure function of the previous latch, the
        // telemetry, the configuration and the clock, and it is the *whole* of
        // S1-S5 (`cc-safety`'s module docs). Running it here rather than inside
        // the reducer is what `Event::Safety`'s documentation describes: S1 is
        // a three-reading debounce, so the counter is machine state and the
        // shell passes the outcome in whole rather than keeping a second copy.
        //
        // The state in the telemetry is the machine's *current* state, which is
        // the C++'s `MachineStateId` argument to `shouldPIDBeEnabled` and the
        // `Telemetry::state` field's reason for existing: S5's whitelist is a
        // function of the state, so the state is an input to the decision and not
        // part of the latch.
        let telemetry = Telemetry::new(
            sensors.temperature,
            sensors.water_tank_full,
            self.machine.state,
        );
        let outcome = cc_safety::reduce(&self.machine.safety, &telemetry, &self.safety, now);
        if let Some(reason) = outcome.verdict.reason {
            debug!("control: safety verdict refuses: {reason:?}");
        }
        self.feed(config, Event::Safety(outcome), &mut effects);

        // ---- 3. the switch edges -----------------------------------------
        for edge in edges {
            self.feed(config, *edge, &mut effects);
        }

        // ---- 4. the PID ---------------------------------------------------
        // The setpoint selection is `updateSetpoint(steamActive)`, and the
        // temperature is the reading the S1 check just accepted, so the PID and
        // the safety monitor see the same number — which the C++ also achieves
        // (`updateTemperature` runs before both).
        self.setpoint = effective_setpoint(config, self.machine.steam_mode);
        self.pid.input = f64::from(sensors.temperature.raw());
        self.pid.setpoint = self.setpoint;
        if self.pid.compute(now) {
            // The C++'s `pidOutput_` is a `double` and `Effect::SetHeaterDuty`
            // carries an `f32`, because the C++'s own consumer — the ISR's
            // `currentPidOutput` — is compared against a `u32` counter of 10 ms
            // steps, so the value that matters is 0..=1000 and every f32 holds
            // that exactly. The narrowing is therefore lossless over the whole
            // output range, which is the range `set_output_limits(0, window)`
            // enforces.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "the PID's output is clamped to 0..=1000 by \
                          set_output_limits(0, WINDOW_MS), and every integer in \
                          that range is exactly representable in f32"
            )]
            let duty = self.pid.output as f32;
            self.feed(config, Event::PidOutput(duty), &mut effects);
        }

        // ---- 5. the tick: standby, the state machine, the fail-safes -------
        self.feed(config, Event::Tick { now }, &mut effects);

        // ---- 6. post-tick: the PID mode and the tunings -------------------
        // Both are consequences of what the tick decided, so both are applied
        // after it. `set_mode(Automatic)` is the library's bumpless transfer and
        // is what `ProcessController::setPIDEnabled` does (`:261-275`).
        let permitted = self.machine.pid.mode_enabled;
        if permitted != self.pid_mode_enabled {
            self.pid.set_mode(if permitted {
                Mode::Automatic
            } else {
                Mode::Manual
            });
            self.pid_mode_enabled = permitted;
            info!(
                "control: PID mode {}",
                if permitted { "AUTOMATIC" } else { "MANUAL" }
            );
        }
        self.retune(config);

        effects
    }

    /// Fold one event through the reducer and append its effects.
    ///
    /// Public because the command queue is drained between ticks and each command
    /// is an event: 04 §3.2 requires a `POST /api/...` to become a `Command`
    /// delivered on a bounded queue, never a direct call into control state.
    pub fn feed(&mut self, config: &Config, event: Event, effects: &mut Vec<cc_machine::Effect>) {
        let ctx = Context::new(config, celsius(self.setpoint));
        let (machine, produced) = reduce(&self.machine, &ctx, event);
        self.machine = machine;
        effects.extend(produced);
    }

    /// Apply the tunings for the current state, if the state changed.
    ///
    /// `ProcessController::updatePIDState`'s switch
    /// (`ProcessController.cpp:174-198`), which keys on `lastMachineStatePid_`:
    /// the gains are chosen **on a state transition** and not before. The three
    /// arms are the C++'s three.
    ///
    /// A rejected tuning (`set_tunings` returns `false` on a negative gain) is
    /// logged and otherwise ignored, which is what the C++ does — `SetTunings`
    /// returns `void` there and the `if (Kp < 0 || ...) return;` swallows it.
    fn retune(&mut self, config: &Config) {
        let state = self.machine.state;
        if self.tuned_for == Some(state) {
            return;
        }
        self.tuned_for = Some(state);

        match state {
            // `STEAM_RUNNING`: P only. `ProcessController.cpp:180-183`.
            MachineState::SteamRunning => {
                let kp = config.pid.steam.kp;
                if !self.pid.set_tunings(kp, 0.0, 0.0, ProportionalOn::Error) {
                    warn!("control: steam PID tuning rejected (kp={kp})");
                    return;
                }
                info!("control: PID tunings for STEAM_RUNNING: p={kp:.3} i=0.000 d=0.000");
            }
            // The brew states: the brew-detection gains when `pid.bd.enabled`,
            // otherwise the regular ones. `ProcessController.cpp:184-192`.
            MachineState::BrewPreinfusion
            | MachineState::BrewPreinfusionPause
            | MachineState::BrewRunning
            | MachineState::BrewFinished => {
                if config.pid.bd.enabled {
                    let (kp, ki, kd) = config.brew_detection_tunings();
                    let _ = self.pid.set_tunings(kp, ki, kd, ProportionalOn::Error);
                    info!("control: PID tunings for {state:?} (brew detection): p={kp:.3} i={ki:.3} d={kd:.3}");
                } else {
                    self.apply_regular(config);
                }
            }
            // Everything else, including `PID_NORMAL` itself.
            // `ProcessController.cpp:193-198`'s `default:` arm.
            _ => self.apply_regular(config),
        }
    }

    /// The regular (non-brew-detection) tuning, `setPIDTunings` and all.
    ///
    /// `ProcessController::setPIDTunings` (`:203-218`): the derived gains, the
    /// integrator limit, and `P_ON_M` when `pid.use_ponm` is set.
    fn apply_regular(&mut self, config: &Config) {
        let (kp, ki, kd) = config.pid_tunings();
        let pon_m = if config.pid.use_ponm {
            ProportionalOn::Measurement
        } else {
            ProportionalOn::Error
        };
        if !self.pid.set_tunings(kp, ki, kd, pon_m) {
            warn!("control: regular PID tuning rejected (kp={kp} ki={ki} kd={kd})");
            return;
        }
        let _ = self
            .pid
            .set_integrator_limits(0.0, config.pid.regular.i_max);
        info!(
            "control: PID tunings p={kp:.3} i={ki:.3} d={kd:.3} ({} mode)",
            if config.pid.use_ponm {
                "P_ON_M"
            } else {
                "P_ON_E"
            }
        );
    }
}

/// `SystemInitializer::finalizeMachineState`'s four arms
/// (`src/core/SystemInitializer.cpp:604-641`).
///
/// `power_switch_pressed` is `None` when `hardware.switches.power.enabled` is
/// false, which is the **default** (`Config.h:1023`) — so an unconfigured
/// machine takes the last arm and follows `pid.enabled`.
fn initial_state(config: &Config, power_switch_pressed: Option<bool>) -> (MachineState, bool) {
    use cc_domain::hardware::SwitchType;
    if config.hardware.switches.power.enabled {
        match config.hardware.switches.power.r#type {
            SwitchType::Momentary => {
                info!("control: power switch is MOMENTARY — starting in PID_NORMAL");
                (MachineState::PidNormal, true)
            }
            // The toggle's *live* level decides, and `None` (no reading) is
            // treated as off, which is the C++'s answer too: `isPressed()` is
            // `currentState == HIGH` and `currentState` starts `LOW`
            // (`IOSwitch.cpp:19`), so a toggle reads "off" until the debounce
            // settles on it.
            SwitchType::Toggle => {
                if power_switch_pressed == Some(true) {
                    info!("control: power toggle is ON — starting in PID_NORMAL");
                    (MachineState::PidNormal, true)
                } else {
                    info!("control: power toggle is OFF — starting in PID_DISABLED");
                    (MachineState::PidDisabled, false)
                }
            }
        }
    } else {
        let enabled = config.pid.enabled;
        info!(
            "control: no power switch configured — following pid.enabled ({enabled}), \
             starting in {}",
            if enabled {
                "PID_NORMAL"
            } else {
                "PID_DISABLED"
            }
        );
        (
            if enabled {
                MachineState::PidNormal
            } else {
                MachineState::PidDisabled
            },
            enabled,
        )
    }
}

/// `ProcessController::updateSetpoint` (`ProcessController.cpp:235-244`) plus
/// `brew.temp_offset`.
///
/// The offset goes on the **setpoint**, not on the reading. The C++ subtracts it
/// from the reading (`ProcessController.cpp:138`) and adds nothing to the
/// setpoint, so `setpoint - (reading - offset)` and `(setpoint + offset) -
/// reading` are the same number; `cc-config` names the second form
/// (`Config::effective_brew_setpoint`) and its help text describes the parameter
/// the same way ("offset added to the user-visible setpoint", `Config.h:808`), so
/// that is the one used. `brew.temp_offset` defaults to `0.0`
/// (`defaults.h:19`), so on a default machine the two are indistinguishable.
fn effective_setpoint(config: &Config, steam_mode: bool) -> f64 {
    if steam_mode {
        config.steam.setpoint
    } else {
        config.effective_brew_setpoint()
    }
}

/// A temperature, narrowed from the configuration's `f64` to [`Celsius`]'s `f32`.
///
/// One place, so the justification is written once. `cc_config` uses `f64`
/// because the C++ uses `double`; `cc_domain::units::Celsius` is `f32` because
/// that is what the DS18B20's 0.0625 °C resolution needs. Over every range the
/// schema allows — `brew.setpoint` 20..=110, `brew.temp_offset` −15..=15,
/// `steam.setpoint` 100..=140, `safety.emergency_temp` 120..=180 — the loss is
/// under 1e-5 °C, four orders of magnitude below the probe's own resolution.
/// `cc_config::safety_view` narrows for exactly the same reason and says so.
fn celsius(degrees: f64) -> Celsius {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "every value that reaches here is a configured temperature in \
                  degrees Celsius within a range where f32 is exact to well \
                  under the probe's 0.0625 C resolution. See this function's \
                  documentation and cc_config::safety_view, which narrows the \
                  same way for the same reason"
    )]
    Celsius::new(degrees as f32)
}

/// `cc-config`'s `safety_view()` widened to `cc-safety`'s own struct.
///
/// A straight field-for-field copy; `cc_config::SafetyView` is typed in `Celsius`
/// precisely so that this join is a copy and not a conversion.
fn safety_config(config: &Config) -> SafetyConfig {
    let view = config.safety_view();
    SafetyConfig {
        emergency_temp: view.emergency_temp,
        emergency_hysteresis: view.emergency_hysteresis,
        steam_setpoint: view.steam_setpoint,
        heater_relay_trigger: view.heater_relay_trigger,
        temperature_sensor: view.temperature_sensor,
    }
}
