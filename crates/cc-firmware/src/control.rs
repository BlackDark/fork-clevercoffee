//! The control loop's shell: the reducer, the PID, and the clock.
//!
//! # What this is
//!
//! The tick, written out. Four steps, in this order:
//!
//! ```text
//! SENSE   the shell reads hardware into plain values
//! DECIDE  fold events through cc_machine::reduce
//! ACT     cc_machine::apply -> cc_hal_esp32::Actuators
//! NOTIFY  publish telemetry to the slow consumers
//! ```
//!
//! Everything in DECIDE is a pure function in `cc-machine`, with a host test suite
//! behind it. Everything in ACT is [`cc_hal_esp32::Actuators`], which is the only
//! code in the firmware that can change an actuator pin. The shell's job is to
//! be the *only* thing that knows both exist.
//!
//! # Why the PID lives here and not in the reducer
//!
//! `cc-domain`'s [`Controller`] is a **value**, and `cc-machine` takes its output
//! as [`cc_machine::Event::PidOutput`] — a number the shell computed. So the
//! division is deliberate: the reducer decides whether a duty may *reach* the
//! heater, and the shell decides what the duty *is*.
//!
//! `apply_pid_output`'s own documentation records the one visible consequence:
//! this firmware gates the emitted `SetHeaterDuty` rather than emitting the value
//! and zeroing it microseconds later. Same machine state, strictly safer ordering.
//!
//! # The clock
//!
//! One clock, read once per tick, published to the reducer as
//! [`cc_machine::Event::Tick`] and to the actuator facade as
//! [`cc_hal_esp32::Actuators::set_now`]. The reducer never reads a clock and the
//! facade's methods take none, so this is the only place the two can be made to
//! agree.

use cc_config::Config;
use cc_domain::pid::{Controller, ControllerDirection, Mode, ProportionalOn};
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{reduce, Context, Event, Machine, Sensors};
use cc_safety::{SafetyConfig, SafetyState, Telemetry};
use log::{debug, info, warn};

/// The heater chopper's window, in milliseconds.
///
/// `ProcessState::windowSize_`
/// and `cc_config::Config::HEATER_WINDOW_MS`. The PID's output limits and its
/// sample time are both this number, so it is
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
    /// The PID controller, as a value.
    pid: Controller,
    /// The `pid.enabled` value the reducer last decided, folded into the
    /// controller's `AUTOMATIC`/`MANUAL` mode.
    ///
    /// Enabling calls `SetMode(AUTOMATIC)`, which is the bumpless transfer: the
    /// integrator keeps its value and the output does not jump. Doing it here rather
    /// than inside the reducer is what keeps the reducer free of the controller — and
    /// it is why the mode is applied *after* the tick, because the tick is what decides
    /// it.
    pid_mode_enabled: bool,
    /// The machine state the PID tunings were last chosen for.
    ///
    /// The tunings change **only** on a state change, which is what stops the
    /// brew-detection gains from being applied in `PID_NORMAL` by a stale assignment.
    ///
    /// **Except** after a tuning is written at runtime — see
    /// [`Self::retune_now`], which clears this so the next tick re-chooses.
    tuned_for: Option<MachineState>,
    /// The S1-S5 configuration, from `cc-config`'s `safety_view()`.
    safety: SafetyConfig,
    /// The active setpoint, °C.
    ///
    /// `brew.setpoint + brew.temp_offset` normally, `steam.setpoint` while steam mode
    /// is on. The offset is applied to the **setpoint**, not to the reading, which is
    /// equivalent arithmetic with the sign the parameter's help text describes: "offset
    /// added to the user-visible setpoint".
    setpoint: f64,
}

impl Control {
    /// Bring the machine up, and say which state it starts in.
    ///
    /// The power switch is read *before* the state machine exists and decides both the
    /// initial state and the runtime PID flag:
    ///
    /// | power switch | initial state | runtime PID |
    /// | --- | --- | --- |
    /// | momentary | `PID_NORMAL` | `true` |
    /// | toggle, ON | `PID_NORMAL` | `true` |
    /// | toggle, OFF | `PID_DISABLED` | `false` |
    /// | absent | `INIT` | `pid.enabled` |
    ///
    /// `shots_since_backflush` is an **argument** rather than something restored
    /// afterwards, because the counter is part of the state the machine boots *into*:
    /// the NVS read happens before the machine exists, in `main`. Passing it in also
    /// keeps the "the shell may write exactly one `Machine` field" rule on
    /// [`Self::reset_shots_since_backflush`] honest — the counter arrives as boot state
    /// rather than as a second setter.
    ///
    /// The effects are returned rather than applied here, because
    /// [`cc_hal_esp32::Actuators`] does not exist yet at this point in the boot
    /// sequence and the first effects are actuator writes.
    ///
    /// The PID is constructed with the state's gains, a 1000 ms sample time, `(0,
    /// 1000)` output limits, `(0, i_max)` integrator limits and an EMA factor — in that
    /// order. **The order matters**: `SetSampleTime` rescales `Ki` and `Kd` by the
    /// ratio of the periods, so setting the sample time before the tunings would give a
    /// different controller.
    #[must_use]
    pub fn boot(
        config: &Config,
        now: Millis,
        power_switch_pressed: Option<bool>,
        shots_since_backflush: i32,
    ) -> (Self, cc_machine::Effects) {
        let (initial, runtime_pid) = initial_state(config, power_switch_pressed);
        let ctx = Context::new(config, celsius(config.effective_brew_setpoint()));
        let (mut machine, effects) = cc_machine::boot_in(initial, runtime_pid, now, &ctx);
        // The stored count is part of what this machine is born holding. A negative
        // value cannot come from the stored form (four bytes of a count that starts at
        // 0), but `0` is applied for it anyway so that nothing downstream has to reason
        // about a counter below its floor.
        machine.shots_since_backflush = shots_since_backflush.max(0);

        // `set_tunings` first, then `set_sample_time`: the latter rescales the former
        // by the period ratio, so the reverse order gives different gains.
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
        // `SetIntegratorLimits(0, i_max)`, guarded on a non-zero `i_max` because the
        // controller refuses a window whose `min >= max`. The guard is not a workaround:
        // `Config::pid_tunings` reads `i_max == 0` as `Ki = 0`, which pins the
        // integrator to zero, so there is no window left to set and the `bool` cannot be
        // false for any value that reaches it.
        if config.pid.regular.i_max > 0.0 {
            let _ = pid.set_integrator_limits(0.0, config.pid.regular.i_max);
        }
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
        // The tunings are applied for the *initial* state, so the first `compute` of
        // the run uses the right gains.
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

    /// The duty the PID last computed, in 0-1000 units (promille).
    ///
    /// This is the machine's belief, not a measurement of the pin: the pin is the
    /// 10 ms ISR's, and `cc_hal_esp32::Actuators::heater()` reports what the gate let
    /// through. `/api/status`'s `heaterPower` is `output / 10`, i.e. per cent.
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

    /// The **active gains**, `(kp, ki, kd)`, as the operator entered them.
    ///
    /// This is what the display's PID row is labelled with, and it is deliberately
    /// *not* [`Self::pid_terms`]: those are the controller's last P, I and D
    /// contributions, the I among them being an accumulator that grows without bound.
    /// Feeding the row the terms produced `4444|81|0 - 100%`, which is not a number an
    /// operator can act on.
    #[must_use]
    pub fn pid_gains(&self) -> (f64, f64, f64) {
        self.pid.gains()
    }

    /// The active setpoint, °C.
    #[must_use]
    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    /// Reset the shot counter — `POST /api/maintenance/reset-backflush-counter`.
    ///
    /// The counter lives in `Machine::shots_since_backflush` and the reducer only ever
    /// clears it as a *consequence* of entering `BACKFLUSH_FINISHED`
    /// (`Effect::ResetShotsSinceBackflush`), so an operator-initiated reset has no
    /// reducer event and this is the one place a shell may write a `Machine` field
    /// directly.
    ///
    /// **The caller must also persist it.** This only clears the value; the NVS
    /// write is `cc_hal_esp32::FirmwareSide::on_reset_shots_since_backflush`,
    /// which the reducer's `Effect::ResetShotsSinceBackflush` goes through too —
    /// so a completed backflush and this route end up writing the same key
    /// through the same code, rather than being two behaviours that have to be
    /// kept in step.
    ///
    /// It is deliberately the only such method. A second one would be the beginning of
    /// "the shell can reach into the state machine", which is the coupling the shell
    /// exists to prevent: one named operation is a deliberate hole, and a general
    /// `&mut Machine` accessor is not.
    pub fn reset_shots_since_backflush(&mut self) {
        self.machine.shots_since_backflush = 0;
        info!("control: shots since backflush reset to 0");
    }

    /// Change the setpoint from outside — the web UI's `POST /api/setpoint`.
    ///
    /// # Panics
    ///
    /// Never. The value is a `f64` the handler has already range-checked
    /// through `cc_config::assign::parse` against the schema's `brew.setpoint`
    /// bounds (`web::parse_setpoint`); this one does not re-check, because the
    /// only caller is the HTTP handler and there is one bound, not two.
    pub fn set_setpoint(&mut self, celsius: f64) {
        self.setpoint = celsius;
    }

    /// The safety latch, as `cc-safety` last computed it.
    ///
    /// The facade's mirror of this is set from here, every tick, which is what makes
    /// the latch a single copy (see `cc_hal_esp32::actuators`'s module documentation on
    /// why two copies are a bug).
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
    /// The event order is load-bearing:
    ///
    /// 1. [`Event::SensorUpdated`] — before anything reads a sensor.
    /// 2. [`Event::Safety`] — the safety verdict is computed *before* the PID.
    /// 3. the switch edges — so a brew press sets `brew_start` and the [`Event::Tick`]
    ///    in the same period consumes it.
    /// 4. [`Event::PidOutput`].
    /// 5. [`Event::Tick`] — standby, the state machine, the handler tail, the valve
    ///    fail-safes and process control.
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
    ) -> cc_machine::Effects {
        let mut effects = cc_machine::Effects::new();

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
        // The state in the telemetry is the machine's *current* state, which is the
        // `Telemetry::state` field's reason for existing: S5's whitelist is a function of
        // the state, so the state is an input to the decision and not part of the latch.
        //
        // **`sample_seq` is the probe's own conversion counter**, not a tick counter.
        // S1's debounce is a count of *readings*, and this tick runs ten times a second
        // against a probe that converts every 400 ms. Without the sequence number one
        // reading is counted forty times and the debounce trips in 30 ms: a probe spike
        // latches an emergency stop in the middle of a brew. See
        // `cc_safety::Telemetry::sample_seq`.
        let telemetry = Telemetry {
            sample_seq: sensors.sample_seq,
            ..Telemetry::new(
                sensors.temperature,
                sensors.water_tank_full,
                self.machine.state,
            )
        };
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
        // The setpoint selection is for the steam state, and the temperature is the
        // reading the safety check just accepted, so the PID and the safety monitor see
        // the same number.
        self.setpoint = effective_setpoint(config, self.machine.steam_mode);
        self.pid.input = f64::from(sensors.temperature.raw());
        self.pid.setpoint = self.setpoint;
        if self.pid.compute(now) {
            // `Effect::SetHeaterDuty` carries an `f32` rather than the controller's
            // `f64`, because the ISR compares it against a `u32` counter of 10 ms steps:
            // the value that matters is 0..=1000 and every `f32` holds that exactly, so
            // the narrowing is lossless over the whole range
            // `set_output_limits(0, window)` enforces.
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
        // Both are consequences of what the tick decided, so both are applied after it.
        // `set_mode(Automatic)` is the bumpless transfer: the integrator keeps its value
        // and the output does not jump.
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
    /// Public because the command queue is drained between ticks and each command is an
    /// event: a `POST /api/...` becomes a `Command` delivered on a bounded queue, never
    /// a direct call into control state.
    pub fn feed(&mut self, config: &Config, event: Event, effects: &mut cc_machine::Effects) {
        let ctx = Context::new(config, celsius(self.setpoint));
        let (machine, produced) = reduce(&self.machine, &ctx, event);
        self.machine = machine;
        effects.extend(&produced);
    }

    /// Re-choose the gains on the next tick, without waiting for a state change.
    ///
    /// **A deliberate divergence**, recorded as §34 of
    /// `docs/history/divergences.md`. Gating the retune on the last *state* would store
    /// and report back a `pid.regular.kp` written over HTTP while the running PID kept
    /// the old gains until the machine happened to change state. Measured on a bench
    /// ESP32: `POST /api/parameters pid.regular.kp=62` was accepted, persisted across a
    /// reboot and reported by the `GET`, and `heaterPower` stayed at the old tuning's
    /// 21.8 % until the PID was cycled off and on — at which point it jumped to 100 % at
    /// a 65 K error.
    ///
    /// Gating on a state change is defensible (a tuning is a preference, not an event)
    /// and this keeps that rule intact — the same gains are still chosen per state, brew
    /// detection still never leaks into `PID_NORMAL`. The only change is that a write
    /// takes effect on the next tick instead of on the next transition.
    ///
    /// Called by the control task after a staged parameter write, so a tuning
    /// change is one control period old rather than one state change old.
    pub fn retune_now(&mut self) {
        self.tuned_for = None;
    }

    /// Apply the tunings for the current state, if the state changed.
    ///
    /// The gains are chosen **on a state transition** and not before, which is what the
    /// field this keys on is for.
    ///
    /// A rejected tuning (`set_tunings` returns `false` on a negative gain) is logged
    /// and otherwise ignored: keeping the old gains is the controller's own behaviour,
    /// and the log is the only place that rejection becomes visible.
    fn retune(&mut self, config: &Config) {
        let state = self.machine.state;
        if self.tuned_for == Some(state) {
            return;
        }
        self.tuned_for = Some(state);

        match state {
            // Steam running: P only, so the boiler does not overshoot on the way up.
            MachineState::SteamRunning => {
                let kp = config.pid.steam.kp;
                if !self.pid.set_tunings(kp, 0.0, 0.0, ProportionalOn::Error) {
                    warn!("control: steam PID tuning rejected (kp={kp})");
                    return;
                }
                info!("control: PID tunings for STEAM_RUNNING: p={kp:.3} i=0.000 d=0.000");
            }
            // The brew states: the brew-detection gains when `pid.bd.enabled`, otherwise
            // the regular ones.
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
            // Everything else, including `PID_NORMAL` itself: the regular gains.
            _ => self.apply_regular(config),
        }
    }

    /// The regular (non-brew-detection) tuning, `setPIDTunings` and all.
    ///
    /// Applies the derived gains, the integrator limit, and `P_ON_M` when
    /// `pid.use_ponm` is set.
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
        // `setPidIntegratorLimits(0, aggIMax_)`.
        // See the boot path's comment: `i_max == 0` is "no integral action" and
        // is already `Ki == 0`, so the call is skipped rather than made and its
        // rejection discarded.
        if config.pid.regular.i_max > 0.0 {
            let _ = self
                .pid
                .set_integrator_limits(0.0, config.pid.regular.i_max);
        }
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

/// The four arms of the boot decision.
///
/// `power_switch_pressed` is `None` when `hardware.switches.power.enabled` is false,
/// which is the **default** — so an unconfigured machine takes the last arm and follows
/// `pid.enabled`.
fn initial_state(config: &Config, power_switch_pressed: Option<bool>) -> (MachineState, bool) {
    use cc_domain::hardware::SwitchType;
    if config.hardware.switches.power.enabled {
        match config.hardware.switches.power.r#type {
            SwitchType::Momentary => {
                info!("control: power switch is MOMENTARY — starting in PID_NORMAL");
                (MachineState::PidNormal, true)
            }
            // The toggle's *live* level decides, and `None` (no reading) counts as off: a
            // toggle switch reads "off" until the debounce settles on it.
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

/// The active setpoint, including `brew.temp_offset`.
///
/// The offset goes on the **setpoint**, not on the reading: `setpoint - (reading -
/// offset)` and `(setpoint + offset) - reading` are the same number, and `cc-config`
/// names the second form (`Config::effective_brew_setpoint`). Its help text describes
/// the parameter the same way — "offset added to the user-visible setpoint" — and
/// `brew.temp_offset` defaults to `0.0`, so on a default machine the two forms are
/// indistinguishable.
pub(crate) fn effective_setpoint(config: &Config, steam_mode: bool) -> f64 {
    if steam_mode {
        config.steam.setpoint
    } else {
        config.effective_brew_setpoint()
    }
}

/// A temperature, narrowed from the configuration's `f64` to [`Celsius`]'s `f32`.
///
/// One place, so the justification is written once. `cc_config` uses `f64` because the
/// schema's ranges are written that way; `cc_domain::units::Celsius` is `f32` because
/// that is what the DS18B20's 0.0625 °C resolution needs. Over every range the schema
/// allows — `brew.setpoint` 20..=110, `brew.temp_offset` 0..=20, `steam.setpoint`
/// 100..=140, `safety.emergency_temp` 120..=180 — the loss is under 1e-5 °C, four
/// orders of magnitude below the probe's own resolution. `cc_config::safety_view`
/// narrows for exactly the same reason and says so.
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
///
/// `pub(crate)` rather than private because three call sites need it and the join
/// is the thing that must not be written twice: `cc_config::Config::safety_view`
/// names exactly the five fields, and a second spelling of that list is a second
/// answer to "which parameters can make this machine unsafe".
/// The single place a [`SafetyConfig`] is built from a [`Config`].
///
/// One owner, deliberately: the boot path (`network::bring_up_config`) and this
/// module's tick both need it, and two field-by-field copies drifted once —
/// see the call site's note. If a field is added to `SafetyConfig`, this is the
/// only function that has to learn about it.
pub(crate) fn safety_config(config: &Config) -> SafetyConfig {
    let view = config.safety_view();
    SafetyConfig {
        emergency_temp: view.emergency_temp,
        emergency_hysteresis: view.emergency_hysteresis,
        steam_setpoint: view.steam_setpoint,
        effective_brew_setpoint: view.effective_brew_setpoint,
        brew_mode: view.brew_mode,
        brew_by_time_enabled: view.brew_by_time_enabled,
        brew_by_weight_enabled: view.brew_by_weight_enabled,
        scale_fitted: view.scale_fitted,
        heater_relay_trigger: view.heater_relay_trigger,
        pump_relay_trigger: view.pump_relay_trigger,
        valve_relay_trigger: view.valve_relay_trigger,
        temperature_sensor: view.temperature_sensor,
    }
}
