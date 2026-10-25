//! Pure actuator permissions from safety state, telemetry, configuration, and time.
//!
//! S1–S3 handle overheat, latching, and recovery. S4 blocks pump and water valve
//! continuously while the tank is empty. S5/S5' whitelist water and steam states;
//! both valves share a physical relay. Checks belong at every actuator call.
//!
//! Machine state arrives in telemetry; only latch history belongs in [`SafetyState`].
//! [`validate_config`] and [`load_or_default`] reject unsafe settings on write and load.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use cc_domain::hardware::{RelayTriggerType, TemperatureSensorType};
use cc_domain::process::BrewMode;
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};

/// The number of consecutive above-threshold readings that trip emergency stop.
///
/// Three readings is about 1.2 s of sustained overheat at the 400 ms sensor
/// interval the firmware samples on.
pub const DEBOUNCE_COUNT: u8 = 3;

/// The temperature below which an emergency stop may be cleared.
///
/// 100 °C: water at atmospheric pressure, and a boiler that cool is not
/// steaming.
pub const EMERGENCY_SAFE_TEMP_C: Celsius = Celsius::new(100.0);

/// The configuration inputs the safety reducer needs.
///
/// Deliberately tiny: only the values that can change whether an actuator may be
/// energised. Everything else in the parameter schema is irrelevant to a verdict,
/// and carrying it here would make the reducer's inputs impossible to audit.
///
/// What the struct must not become is a mirror of `cc_config::Config`: the shape
/// to preserve is "every field is one somebody had to think about", not "every
/// field is small".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafetyConfig {
    /// `safety.emergency_temp` — the over-temperature threshold.
    pub emergency_temp: Celsius,
    /// `safety.emergency_hysteresis` — how far below the threshold the machine
    /// must fall before the debounce counter resets.
    pub emergency_hysteresis: Celsius,
    /// `brew.mode` — needed only by [`validate_config`], and for one reason: an
    /// **automatic** brew is supposed to end by itself, so a configuration that
    /// leaves it with no reachable stop condition is a machine holding a pump
    /// and an open valve with nothing to end it. In `MANUAL_BREW` the operator
    /// ends the shot with the brew switch, which is a stop condition no
    /// configuration can remove.
    pub brew_mode: BrewMode,
    /// `brew.by_time.enabled` — needed only by [`validate_config`], and it is
    /// half of the question the brew-stop rule asks: is there a stop condition
    /// that does not need a scale?
    pub brew_by_time_enabled: bool,
    /// `brew.by_weight.enabled` — the other half. This is the one that needs a
    /// scale to mean anything, which is why [`Self::scale_fitted`] is here.
    pub brew_by_weight_enabled: bool,
    /// `hardware.sensors.scale.enabled` — whether this machine has a scale.
    ///
    /// A configuration value rather than a live probe, which is the point:
    /// [`validate_config`] must be a pure function of configuration, and the
    /// value it needs has to be readable before the driver has produced a
    /// single sample. What the *driver* then does — starts, faults, answers —
    /// is reported through `Sensors::has_scale_error` and the weight itself,
    /// and neither reaches this struct.
    pub scale_fitted: bool,
    /// `steam.setpoint` — needed only by [`validate_config`], and here because
    /// the emergency threshold must clear the steam setpoint or the machine
    /// would stop itself during normal steaming.
    pub steam_setpoint: Celsius,
    /// The temperature the PID is actually told to hold in brew mode —
    /// `brew.setpoint + brew.temp_offset`, not the raw `brew.setpoint`.
    ///
    /// Needed only by [`validate_config`], for the same reason
    /// [`Self::steam_setpoint`] is, and the offset is in here for the same
    /// reason it is in `cc_config::Config::effective_brew_setpoint`: the
    /// threshold has to clear the temperature the boiler is *driven to*, and
    /// the offset is part of that number. `brew.temp_offset` is bounded 0..=20,
    /// which is enough to push a legal `brew.setpoint` past a legal
    /// `safety.emergency_temp`.
    pub effective_brew_setpoint: Celsius,
    /// `hardware.relays.heater.trigger_type` — needed only by
    /// [`validate_config`]. See [`RelayTriggerType`]: a low-trigger heater relay
    /// cannot be made safe in firmware.
    pub heater_relay_trigger: RelayTriggerType,
    /// `hardware.relays.pump.trigger_type` — needed only by
    /// [`validate_config`], for the same reason: a low-trigger pump relay runs
    /// water from power-on.
    pub pump_relay_trigger: RelayTriggerType,
    /// `hardware.relays.valve.trigger_type` — likewise.
    pub valve_relay_trigger: RelayTriggerType,
    /// `hardware.sensors.temperature.type`.
    ///
    /// **Not used by [`validate_config`].** It is here because a configuration's
    /// sensor type is part of the configuration and the firmware has to carry it
    /// somewhere, and because the `cc-config` schema and this struct are the two
    /// ends of the same key.
    ///
    /// Both types have a driver (`cc_protocol::sensor::ds18b20` and
    /// `cc_protocol::sensor::tsic306`), so no value of this field can be refused
    /// and there is nothing to validate.
    pub temperature_sensor: TemperatureSensorType,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            emergency_temp: Celsius::new(150.0),
            emergency_hysteresis: Celsius::new(5.0),
            // The defaults must pass `validate_config` themselves: a default
            // configuration its own validator refuses is a machine that will not
            // run, so no brew-stop rule is enabled here.
            //
            // `scale_fitted: false` is also the honest answer for the machine
            // this firmware was built for, whose scale pins are unconnected. It
            // is the *conservative* direction: with no scale the by-weight stop
            // cannot fire, so the rule below can only refuse more, never less.
            brew_mode: BrewMode::Manual,
            brew_by_time_enabled: false,
            brew_by_weight_enabled: false,
            scale_fitted: false,
            steam_setpoint: Celsius::new(120.0),
            effective_brew_setpoint: Celsius::new(95.0),
            heater_relay_trigger: RelayTriggerType::HighTrigger,
            pump_relay_trigger: RelayTriggerType::HighTrigger,
            valve_relay_trigger: RelayTriggerType::HighTrigger,
            // `TSIC_306`, the schema default. Both types have a driver, so
            // either value is honoured.
            //
            // **What this does not mean:** that a TSIC-306 is fitted to this
            // machine. The probe on the board is a DS18B20, family 0x28, ROM
            // `286937aacd78af41`. A machine built with this default and a
            // DS18B20 fitted reports `NOT_CONNECTED` and says so, because the two
            // drivers fail differently and visibly. `cc-firmware` therefore
            // selects the driver from the *board*, not from this default; see its
            // `PROBE` constant.
            temperature_sensor: TemperatureSensorType::Tsic306,
        }
    }
}

/// Everything the reducer observes about the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Telemetry {
    /// The latest temperature reading.
    ///
    /// This is `f32` and is *not* pre-validated: whether a reading is
    /// believable is part of the decision (S1), not an input to it. A reading
    /// outside [`Celsius::MIN`]..=[`Celsius::MAX`], or `NaN`, is treated exactly
    /// as a disconnected probe: emergency stop, immediately.
    pub temperature: Celsius,
    /// Whether the water tank is full. `false` blocks the pump (S4).
    pub water_tank_full: bool,
    /// The machine's current state, which S5's whitelist is a function of.
    pub state: MachineState,
    /// A counter that changes **once per temperature sample**, not once per pass.
    ///
    /// # Why this field exists
    ///
    /// S1's debounce counts **samples**, because [`DEBOUNCE_COUNT`] readings is
    /// meant to be about 1.2 s of sustained overheat. [`reduce`] runs once per
    /// control tick, every 10 ms, with whatever the last conversion produced, so
    /// without a sequence number the *same* reading would be counted about forty
    /// times and the debounce would trip in 30 ms instead. A probe spike — a
    /// 1-Wire CRC retry, a flash write — would then latch an emergency stop
    /// mid-brew.
    ///
    /// The caller sets this to the driver's own sample counter, and [`reduce`]
    /// advances the debounce **only when it changes**. The latching check and the
    /// hysteresis are unaffected: they are properties of the reading, not of how
    /// often we look.
    pub sample_seq: u32,
}

impl Telemetry {
    /// Build telemetry with a full tank in the given state.
    #[must_use]
    pub const fn new(temperature: Celsius, water_tank_full: bool, state: MachineState) -> Self {
        Self {
            temperature,
            water_tank_full,
            state,
            sample_seq: 0,
        }
    }
}

/// The reducer's only memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SafetyState {
    /// Whether emergency stop is latched.
    pub latched: bool,
    /// Consecutive above-threshold readings so far.
    pub high_reading_count: u8,
    /// The `sample_seq` the counter was last advanced on, so a repeat of the
    /// same sample does not advance it.
    pub last_sample_seq: Option<u32>,
}

impl SafetyState {
    /// The clear, unlatched state.
    pub const CLEAR: Self = Self {
        last_sample_seq: None,
        latched: false,
        high_reading_count: 0,
    };

    /// Whether a reading is currently above `safety.emergency_temp`.
    ///
    /// Strictly greater, so a reading *exactly* at the threshold does not count.
    #[must_use]
    pub fn is_over_threshold(self, temperature: Celsius, emergency_temp: Celsius) -> bool {
        temperature.raw() > emergency_temp.raw()
    }

    /// Whether a reading is far enough below the threshold to reset the
    /// debounce counter, i.e. below `emergency_temp - emergency_hysteresis`.
    #[must_use]
    pub fn is_below_hysteresis(
        self,
        temperature: Celsius,
        emergency_temp: Celsius,
        hysteresis: Celsius,
    ) -> bool {
        temperature.raw() < emergency_temp.raw() - hysteresis.raw()
    }

    /// Latch emergency stop. Idempotent, and deliberately does *not* touch the
    /// debounce counter, so the operator can see how many readings had
    /// accumulated when the trip happened.
    pub fn trigger(&mut self) {
        self.latched = true;
    }

    /// Clear emergency stop, and zero the debounce counter with it.
    pub fn clear(&mut self) {
        self.latched = false;
        self.high_reading_count = 0;
        // And forget which sample the counter was on, or the next pass would
        // see an unchanged `sample_seq` and refuse to re-count the sample that
        // is still over threshold.
        self.last_sample_seq = None;
    }

    /// Forget everything.
    pub const fn reset(&mut self) {
        *self = Self::CLEAR;
    }
}

/// Why an actuator was refused.
///
/// `None` in [`Verdict::reason`] means "nothing is wrong"; the rules are
/// evaluated in the order below, so `reason` names the *first* reason that
/// applied, which is the one worth logging.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reason {
    /// S2/S3: the emergency latch is set.
    EmergencyLatched,
    /// S1: three or more consecutive readings above `emergency_temp`.
    Overtemp {
        /// How many consecutive above-threshold readings have been seen.
        consecutive: u8,
        /// The threshold that was crossed.
        threshold: Celsius,
    },
    /// S1: the reading itself is not physically possible, so it is a
    /// disconnected or faulted probe rather than a real temperature.
    InvalidReading {
        /// The reading.
        reading: Celsius,
        /// The plausible range, `[low, high]`.
        low: Celsius,
        /// See [`Reason::InvalidReading`].
        high: Celsius,
    },
    /// S4: the water tank is empty.
    WaterTankEmpty,
    /// S5: the current state is not one in which water may flow.
    NotAWaterFlowState {
        /// The state that was refused.
        state: MachineState,
    },
    /// S5': the current state is not one in which steam may flow.
    ///
    /// **A deliberate divergence**: the steam valve is the same physical relay as
    /// the water valve, so it gets a whitelist. See [`steam_flow_allowed`].
    NotASteamState {
        /// The state that was refused.
        state: MachineState,
    },
}

/// The decision: may each actuator be energised right now?
///
/// Four independent permissions plus the latch. They are independent by
/// construction — an empty tank must not stop the heater (the boiler is separate
/// from the reservoir) and an emergency must stop everything — so they are four
/// booleans rather than one enum. Each permission is written to a distinct
/// actuator.
#[expect(
    clippy::struct_excessive_bools,
    reason = "an enum would either lose the \"heater yes, pump no\" combination or \
              grow a combinatorial number of variants: an empty tank blocks the \
              pump and the water valve but must not block the heater, and a steam \
              whitelist refusal must not block the water valve"
)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Verdict {
    /// May the heater be energised? `false` when the latch is set.
    pub may_heat: bool,
    /// May the pump be energised? `false` when the latch is set **or** the
    /// water tank is empty.
    pub may_pump: bool,
    /// May the water (three-way) valve be energised? `false` when the latch is
    /// set, when the water tank is empty, **or** when the current state is not a
    /// water-flow state.
    ///
    /// The tank condition is a **deliberate divergence**: an empty reservoir must
    /// not open the water valve either.
    pub may_open_water: bool,
    /// May the steam valve be energised? `false` when the latch is set **or**
    /// the current state is not a steam-flow state.
    ///
    /// The state condition is a **deliberate divergence**: the steam and water
    /// valves are the *same physical relay*, so an ungated steam open would
    /// re-open the hole S5 exists to close.
    pub may_open_steam: bool,
    /// Whether the emergency latch is set after this reduce.
    pub latched: bool,
    /// The first rule that refused something, for logging and diagnostics.
    pub reason: Option<Reason>,
}

impl Verdict {
    /// Everything permitted, nothing latched. The normal, healthy case.
    const ALL_PERMITTED: Self = Self {
        may_heat: true,
        may_pump: true,
        may_open_water: true,
        may_open_steam: true,
        latched: false,
        reason: None,
    };

    /// Everything refused because the emergency latch is set: the shutdown state
    /// of every actuator, expressed as a permission set.
    pub const ALL_REFUSED: Self = Self {
        may_heat: false,
        may_pump: false,
        may_open_water: false,
        may_open_steam: false,
        latched: true,
        reason: Some(Reason::EmergencyLatched),
    };

    /// Whether *any* actuator may be energised. Useful for a single "the
    /// machine is doing nothing" log line and for the applier's fast path.
    #[must_use]
    pub const fn any_permitted(self) -> bool {
        self.may_heat || self.may_pump || self.may_open_water || self.may_open_steam
    }
}

/// The result of one reduce: the new latch state and the verdict.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    /// The state to carry into the next call.
    pub state: SafetyState,
    /// The decision the applier must obey.
    pub verdict: Verdict,
}

/// S5: may water flow in this state?
///
/// A `match` with **no `_` arm**, on purpose. Adding a `MachineState` variant
/// makes this function fail to compile until someone has decided whether the new
/// state flows water, which is what keeps the whitelist auditable.
///
/// The whitelist is the set of states in which water is drawn:
///
/// * the brew states except `BrewFinished`
/// * `ManualFlushRunning`
/// * the backflush states except `BackflushIdle` and `BackflushFinished`
#[must_use]
pub const fn water_flow_allowed(state: MachineState) -> bool {
    match state {
        MachineState::BrewPreinfusion
        | MachineState::BrewPreinfusionPause
        | MachineState::BrewRunning
        | MachineState::ManualFlushRunning
        | MachineState::BackflushFilling
        | MachineState::BackflushFlushing => true,
        MachineState::Init
        | MachineState::PidNormal
        | MachineState::BrewFinished
        | MachineState::SteamRunning
        | MachineState::BackflushIdle
        | MachineState::BackflushFinished
        | MachineState::WaterTankEmpty
        | MachineState::EmergencyStop
        | MachineState::PidDisabled
        | MachineState::Standby
        | MachineState::SensorError
        | MachineState::EepromError => false,
    }
}

/// S5': may steam flow in this state?
///
/// `SteamRunning`, and only `SteamRunning`, which is what
/// [`MachineState::is_steam_state`] reports. A `match` with **no `_` arm**, for
/// the same reason as [`water_flow_allowed`]: a new state must be classified
/// before it compiles.
///
/// # Why this exists
///
/// Steam and water share one physical relay, so an ungated steam open energises
/// the very solenoid S5 exists to keep closed. The steam valve is reachable
/// through `Effect::OpenSteamValve`, and this whitelist is what
/// `Actuators::may_open_steam` consults before allowing it. `may_open_water`
/// excludes `SteamRunning` for the same reason, from the other side.
///
/// # The whitelist
///
/// The set of states in which steam is drawn is `SteamRunning` alone.
/// `SteamRunning` is the only state that turns steam mode on, and leaving it
/// turns the flag off again; every other state either never touches the flag or
/// clears it. Water injection while steaming happens **inside** `SteamRunning`
/// rather than in a state of its own.
///
/// A wider whitelist would be actively wrong: because the relay is shared,
/// listing a water-flow state as a steam state would make `may_open_steam` agree
/// with `may_open_water` and quietly re-open S5's hole from the other side.
#[must_use]
pub const fn steam_flow_allowed(state: MachineState) -> bool {
    match state {
        MachineState::SteamRunning => true,
        MachineState::Init
        | MachineState::PidNormal
        | MachineState::BrewPreinfusion
        | MachineState::BrewPreinfusionPause
        | MachineState::BrewRunning
        | MachineState::BrewFinished
        | MachineState::ManualFlushRunning
        | MachineState::BackflushIdle
        | MachineState::BackflushFilling
        | MachineState::BackflushFlushing
        | MachineState::BackflushFinished
        | MachineState::WaterTankEmpty
        | MachineState::EmergencyStop
        | MachineState::PidDisabled
        | MachineState::Standby
        | MachineState::SensorError
        | MachineState::EepromError => false,
    }
}

/// The whole safety decision, as a pure function.
///
/// Runs once per control tick, before the PID computes and before any state
/// transition. `now` is accepted for interface symmetry with the other reducers
/// and for the deadman timeout that is not implemented yet; nothing in S1-S5' is
/// time-based, so it does not participate in the decision.
///
/// # The rules, in evaluation order
///
/// 1. **S1, invalid reading.** A reading outside
///    [`Celsius::MIN`]..=[`Celsius::MAX`] (or `NaN`) latches immediately, with
///    **no debounce**: a probe reading 222 °C is reporting a fault, not a
///    temperature, and waiting three readings to act on it protects nothing.
/// 2. **S1, over-temperature with debounce.** Above `emergency_temp` the counter
///    increments; at [`DEBOUNCE_COUNT`] it latches. A reading exactly at the
///    threshold does not count.
/// 3. **S1 reset / S3 clear.** Below `emergency_temp - emergency_hysteresis`
///    the counter resets *and*, if latched, the latch clears — but only if the
///    reading is valid **and** at or below [`EMERGENCY_SAFE_TEMP_C`]. Between
///    those two temperatures the counter is left alone, which is the hysteresis
///    that stops the machine oscillating around the threshold.
/// 4. **S2.** If latched, nothing may be energised.
/// 5. **S4.** An empty tank blocks the pump and the water valve. The boiler is
///    fed from the reservoir, so an empty tank means the pump is running dry;
///    leaving the valve open is not what makes that safe, it is just as much of a
///    mistake.
/// 6. **S5.** A state outside [`water_flow_allowed`] blocks the water valve.
/// 7. **S5'.** A state outside [`steam_flow_allowed`] blocks the steam valve,
///    which is the same relay as the water valve.
#[must_use]
pub fn reduce(
    prev: &SafetyState,
    telemetry: &Telemetry,
    cfg: &SafetyConfig,
    now: Millis,
) -> Outcome {
    let _ = now; // S1-S5 are all instantaneous. See the doc comment.

    let mut state = *prev;

    // ---- S1 step 1: an implausible reading trips with no debounce. --------
    let temperature = telemetry.temperature;
    if !temperature.is_valid() {
        state.trigger();
        return Outcome {
            state,
            verdict: Verdict {
                reason: Some(Reason::InvalidReading {
                    reading: temperature,
                    low: Celsius::MIN,
                    high: Celsius::MAX,
                }),
                ..Verdict::ALL_REFUSED
            },
        };
    }

    // ---- S1 step 2: debounced over-temperature. ----------------------------
    if state.is_over_threshold(temperature, cfg.emergency_temp) {
        // **Only on a new sample.** See `Telemetry::sample_seq`.
        if state.last_sample_seq != Some(telemetry.sample_seq) {
            state.high_reading_count = state.high_reading_count.saturating_add(1);
            state.last_sample_seq = Some(telemetry.sample_seq);
        }
        if state.high_reading_count >= DEBOUNCE_COUNT {
            state.trigger();
            return Outcome {
                state,
                verdict: Verdict {
                    reason: Some(Reason::Overtemp {
                        consecutive: state.high_reading_count,
                        threshold: cfg.emergency_temp,
                    }),
                    ..Verdict::ALL_REFUSED
                },
            };
        }
    } else if state.is_below_hysteresis(temperature, cfg.emergency_temp, cfg.emergency_hysteresis) {
        // ---- S1 step 3: reset, and S3: maybe clear. -------------------------
        state.high_reading_count = 0;
        if state.latched && can_clear(temperature) {
            state.clear();
        }
    }
    // Anything else leaves the counter alone: that is the hysteresis band.

    // ---- S2: the latch. ----------------------------------------------------
    if state.latched {
        return Outcome {
            state,
            verdict: Verdict::ALL_REFUSED,
        };
    }

    // ---- S4, S5 and S5': the per-actuator interlocks. ----------------------
    let mut verdict = Verdict::ALL_PERMITTED;
    if !telemetry.water_tank_full {
        verdict.may_pump = false;
        // A deliberate divergence: an empty reservoir must not open the water
        // valve either.
        verdict.may_open_water = false;
        verdict.reason = Some(Reason::WaterTankEmpty);
    }
    if !water_flow_allowed(telemetry.state) {
        verdict.may_open_water = false;
        if verdict.reason.is_none() {
            verdict.reason = Some(Reason::NotAWaterFlowState {
                state: telemetry.state,
            });
        }
    }
    if !steam_flow_allowed(telemetry.state) {
        verdict.may_open_steam = false;
        if verdict.reason.is_none() {
            verdict.reason = Some(Reason::NotASteamState {
                state: telemetry.state,
            });
        }
    }

    Outcome { state, verdict }
}

/// S3: may a latched emergency stop be cleared?
///
/// The reading must be plausible **and** at or below
/// [`EMERGENCY_SAFE_TEMP_C`], so exactly 100.0 °C clears: 100 °C is water at
/// atmospheric pressure, and the machine will not be steaming.
#[must_use]
pub fn can_clear(temperature: Celsius) -> bool {
    temperature.is_valid() && temperature.raw() <= EMERGENCY_SAFE_TEMP_C.raw()
}

/// Why a configuration is unsafe to run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConfigViolation {
    /// `safety.emergency_temp` is not above `steam.setpoint +
    /// safety.emergency_hysteresis`.
    ///
    /// If the emergency threshold sits at or below the steam setpoint, the
    /// machine trips emergency stop during **normal** steaming: the steam PID
    /// holds `steam.setpoint`, three readings above `emergency_temp` latch the
    /// machine, and it will not restart until the boiler drops below 100 °C.
    /// The user has then configured a machine that cannot be steamed.
    EmergencyTempTooLowForSteam {
        /// The configured emergency threshold.
        emergency_temp: Celsius,
        /// The configured steam setpoint.
        steam_setpoint: Celsius,
        /// The configured hysteresis.
        emergency_hysteresis: Celsius,
    },
    /// `safety.emergency_temp` is not above the brew setpoint (plus the offset
    /// the PID adds to it) plus `safety.emergency_hysteresis`.
    ///
    /// The steam pair has the same shape and the same asymmetry that makes this
    /// one the worse of the two: S1's test is **strictly greater**
    /// ([`SafetyState::is_over_threshold`]), so a setpoint sitting *at* the
    /// emergency threshold means the boiler is driven there and held, and the
    /// three-reading debounce can never count a breach.
    ///
    /// The schema bounds alone keep the defaults far enough apart to be safe, so
    /// the live hazard is a legal pair written by a path that skipped them. The
    /// check lives here, at the validator, so that every write path is covered,
    /// not just the one where the hole was found.
    EmergencyTempTooLowForBrew {
        /// The configured emergency threshold.
        emergency_temp: Celsius,
        /// The temperature the PID is told to hold in brew mode, offset
        /// included.
        brew_setpoint: Celsius,
        /// The configured hysteresis.
        emergency_hysteresis: Celsius,
    },
    /// The heater relay is configured `LOW_TRIGGER`.
    ///
    /// An ESP32 GPIO is high-impedance before the pin is configured as an output and
    /// resets, and a low-trigger relay board energises its coil when the input is
    /// low *or floating*. Firmware cannot prevent that: it loses control of the
    /// pin before its first instruction, so the heater would be energised on
    /// every boot. This is a wiring property rather than a code path, and no
    /// amount of firmware can make it safe.
    HeaterRelayLowTrigger,
    /// A `LOW_TRIGGER` pump relay: the pump runs at every boot, before any
    /// firmware exists to stop it.
    ///
    /// `actuators::Polarity` can drive such a relay, but a configuration carrying
    /// one is refused at boot rather than obeyed, because the pin floats before
    /// firmware runs and "honoured" would mean "energised from power-on".
    PumpRelayLowTrigger,
    /// A `LOW_TRIGGER` valve relay: water at every boot.
    ValveRelayLowTrigger,
    /// An automatic brew whose only stop condition is a weight, on a machine
    /// with no scale.
    ///
    /// **A pump with an open water valve and nothing to stop it.** An automatic
    /// shot has two stop conditions: `brew.by_time`, whose target is zero unless
    /// `by_time.enabled`, and `brew.by_weight`, which compares the scale reading
    /// against `target_weight`. With `by_time` off and nothing to weigh with,
    /// neither arm can fire, and what is left is the brew switch or the
    /// 300-second pump watchdog (`timing::BREW_PUMP_TIMEOUT_MS`) of water.
    ///
    /// The configuration is **legal in every field**: both booleans default to
    /// `false`, and `target_weight` is bounded `0 ..= 500` and defaults to 36. No
    /// single-parameter range check can catch this, because it takes two settings
    /// read together, which is exactly what this rule is for.
    ///
    /// It bites only while the scale driver works: with no driver the by-weight
    /// arm is dead anyway and the same configuration is merely a 300-second shot.
    BrewByWeightWithNoScale,
}

impl ConfigViolation {
    /// Keys this violation implicates. Repair reverts these. HTTP 400 names them.
    ///
    /// The headroom rules name `safety.emergency_temp`, not the setpoint:
    /// raising the threshold cannot fail the rule. That is not always enough, as
    /// `steam.setpoint` 140 plus `safety.emergency_hysteresis` 15 exceeds the
    /// 150 default, so `cc_config::repair_unsafe` falls back to
    /// `REPAIR_ESCALATION_KEYS`.
    #[must_use]
    pub const fn implicated_keys(self) -> &'static [&'static str] {
        match self {
            Self::EmergencyTempTooLowForSteam { .. } | Self::EmergencyTempTooLowForBrew { .. } => {
                &["safety.emergency_temp"]
            }
            Self::HeaterRelayLowTrigger => &["hardware.relays.heater.trigger_type"],
            Self::PumpRelayLowTrigger => &["hardware.relays.pump.trigger_type"],
            Self::ValveRelayLowTrigger => &["hardware.relays.valve.trigger_type"],
            Self::BrewByWeightWithNoScale => &["brew.by_weight.enabled"],
        }
    }
}

/// Validate cross-field safety constraints before applying or storing settings.
///
/// Individual ranges cannot prevent steam/hysteresis or brew thresholds from
/// overlapping the emergency threshold. S1 uses strict `>`, so a PID target at
/// the threshold could hold there without advancing the overheat debounce.
///
/// # Errors
///
/// Returns the first [`ConfigViolation`], including weight mode without a scale.
pub fn validate_config(cfg: &SafetyConfig) -> Result<(), ConfigViolation> {
    let steam_headroom = cfg.steam_setpoint.raw() + cfg.emergency_hysteresis.raw();
    // A bench build compiles the two headroom rules out so the over-temperature
    // trip can be reached with a hand-warmed DS18B20 on the bench. The relay and
    // brew-target rules below are unaffected; see
    // [`cc_domain::BENCH_UNSAFE_TEMPERATURES`].
    if !cc_domain::BENCH_UNSAFE_TEMPERATURES {
        if cfg.emergency_temp.raw() <= steam_headroom {
            return Err(ConfigViolation::EmergencyTempTooLowForSteam {
                emergency_temp: cfg.emergency_temp,
                steam_setpoint: cfg.steam_setpoint,
                emergency_hysteresis: cfg.emergency_hysteresis,
            });
        }

        // The brew pair, immediately after the steam one so the two read as the
        // rule they are: the threshold must clear *both* setpoints the PID can
        // be told to hold, plus the hysteresis, by a strictly positive margin.
        let brew_headroom = cfg.effective_brew_setpoint.raw() + cfg.emergency_hysteresis.raw();
        if cfg.emergency_temp.raw() <= brew_headroom {
            return Err(ConfigViolation::EmergencyTempTooLowForBrew {
                emergency_temp: cfg.emergency_temp,
                brew_setpoint: cfg.effective_brew_setpoint,
                emergency_hysteresis: cfg.emergency_hysteresis,
            });
        }
    }

    // **Every relay, not just the heater.** A low-trigger relay energises
    // whenever its pin floats, which is *before* any firmware runs, so a
    // low-trigger pump or valve relay runs water at boot with no firmware in the
    // loop to stop it. Nothing about that hazard is heater-specific, and every
    // trigger type is storable.
    if cfg.heater_relay_trigger == RelayTriggerType::LowTrigger {
        return Err(ConfigViolation::HeaterRelayLowTrigger);
    }
    if cfg.pump_relay_trigger == RelayTriggerType::LowTrigger {
        return Err(ConfigViolation::PumpRelayLowTrigger);
    }
    if cfg.valve_relay_trigger == RelayTriggerType::LowTrigger {
        return Err(ConfigViolation::ValveRelayLowTrigger);
    }

    // **An automatic brew has to have somewhere to stop.** Placed after the
    // relays because, unlike them, this one is about what the machine will *do*
    // rather than about a wiring hazard firmware cannot undo.
    //
    // Neither stop arm can fire: by-time is off, so its target is zero, and
    // by-weight has nothing to weigh. What is left is the brew switch and the
    // 300-second pump watchdog.
    //
    // `brew.by_weight.target_weight > 0.0` is deliberately **not** part of this
    // test. The field is bounded `0 ..= 500` and defaults to 36, and "brew by
    // weight, to zero grams" is not a configuration an operator reaches by
    // accident; enabling by-weight on a machine that cannot weigh is refused
    // either way.
    if !cfg.scale_fitted
        && cfg.brew_mode == BrewMode::Automatic
        && !cfg.brew_by_time_enabled
        && cfg.brew_by_weight_enabled
    {
        return Err(ConfigViolation::BrewByWeightWithNoScale);
    }

    // `cfg.temperature_sensor` is deliberately not checked: both types have a
    // driver, so there is no value of this field the firmware cannot honour.
    let _ = cfg.temperature_sensor;

    Ok(())
}

/// Where a [`SafetyConfig`] came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConfigOrigin {
    /// Nothing was stored, so the compiled-in defaults are in use.
    Defaults,
    /// A stored configuration was loaded and passed validation.
    Stored,
    /// A stored configuration was found, failed [`validate_config`], and was
    /// **discarded**. The defaults are in use instead.
    ///
    /// Refusing to run it is the fail-closed direction: the alternative is a
    /// configuration that trips emergency stop on every steam shot, or that
    /// energises the heater at reset, and neither is acceptable on a machine that
    /// heats water under pressure.
    DiscardedUnsafe(ConfigViolation),
}

/// A validated configuration plus its provenance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadedConfig {
    /// The configuration to run.
    pub config: SafetyConfig,
    /// Where it came from, and whether anything was thrown away.
    pub origin: ConfigOrigin,
}

/// Load a stored [`SafetyConfig`], falling back to the defaults if it is unsafe.
///
/// `stored` is `None` when the store held no configuration at all — first boot,
/// or after a factory reset. That is not a failure: the defaults are used and
/// [`ConfigOrigin::Defaults`] is reported.
///
/// A stored configuration that fails [`validate_config`] is **discarded** and the
/// defaults are used. It is *not* repaired field by field, because a partially
/// repaired safety configuration is indistinguishable from a correct one and
/// nobody will ever audit it again.
#[must_use]
pub fn load_or_default(stored: Option<&SafetyConfig>) -> LoadedConfig {
    let defaults = SafetyConfig::default();
    let Some(cfg) = stored else {
        return LoadedConfig {
            config: defaults,
            origin: ConfigOrigin::Defaults,
        };
    };
    match validate_config(cfg) {
        Ok(()) => LoadedConfig {
            config: *cfg,
            origin: ConfigOrigin::Stored,
        },
        Err(violation) => LoadedConfig {
            config: defaults,
            origin: ConfigOrigin::DiscardedUnsafe(violation),
        },
    }
}

/// Refuse to persist a configuration that is unsafe to run.
///
/// The mirror of [`load_or_default`]: the store is the other place an unsafe
/// configuration can enter the system, and refusing at the boundary is cheaper
/// than discovering it on the next boot.
///
/// # Errors
///
/// Returns the [`ConfigViolation`] that makes the configuration unsafe, or the
/// configuration itself on success so the caller can pass it straight to the
/// store.
pub fn check_storable(cfg: &SafetyConfig) -> Result<&SafetyConfig, ConfigViolation> {
    validate_config(cfg).map(|()| cfg)
}
