//! The one place that decides whether an actuator may be energised.
//!
//! # What this crate is
//!
//! A pure reducer: `(previous safety state, telemetry, configuration, now) ->
//! (new safety state, verdict)`. It performs no I/O, allocates nothing, and has
//! exactly one dependency (`cc-domain`, for units and the state enum). That is
//! the whole point of keeping it separate (04 §6): a reviewer can read this
//! crate in one sitting and be certain nothing else influences the verdict.
//!
//! # The safety paths it implements
//!
//! | Path | C++ source | Rust |
//! | --- | --- | --- |
//! | **S1** overtemp | `src/control/EmergencyStopManager.cpp:17-69` | [`reduce`] |
//! | **S2** emergency latch | `src/hardware/HardwareManager.cpp:278-321,353,398,443` | [`Verdict`] permissions |
//! | **S3** emergency recovery | `EmergencyStopManager.cpp:71-90` | [`reduce`] |
//! | **S4** water tank empty | `HardwareManager.cpp:325-328,546-561` | [`Verdict::may_pump`] |
//! | **S5** water-valve fail-safe | `include/clevercoffee/handlers/BrewHandler.h:105-122` | [`water_flow_allowed`] |
//! | **S5'** steam-valve fail-safe | **absent in the C++** — see [`steam_flow_allowed`] | [`steam_flow_allowed`] |
//!
//! Plus the two fail-closed configuration rules recovered from the previous
//! Rust firmware ([08 §4.1](../docs/history/recovered-oracle.md)):
//! [`validate_config`] and [`load_or_default`].
//!
//! # Semantics that are *not* the C++'s, and why
//!
//! These are deliberate, human-approved divergences. Every one is a line in
//! [`intentional-diffs.md`](../../docs/history/divergences.md)
//! and is pinned by a `div_`-prefixed test, so a parity harness that reports a
//! diff there knows it is expected.
//!
//! * **S4 gates the water valve as well as the pump.** The C++ checks
//!   `waterTankEmpty_` in `enablePump` and `setPumpPressure` only
//!   (`HardwareManager.cpp:325-328,398-406`); `openWaterValve` does not check it
//!   ([09 §3](../docs/history/cpp-findings.md)). Emptying the tank and
//!   then entering a brew state opened the water valve against a dry reservoir.
//!   Gating it costs nothing — the S5 whitelist is consulted in the same breath —
//!   and removes a way to be wrong.
//! * **S5' is new.** The C++ has no steam-valve whitelist at all
//!   ([09 §2](../docs/history/cpp-findings.md)); see
//!   [`steam_flow_allowed`] for the derivation and for why it is not merely
//!   theoretical.
//! * **S4 is edge-free here.** The C++ kills a running pump inside
//!   `setWaterTankEmpty(true)` — an *event* on the empty→empty edge
//!   (`HardwareManager.cpp:546-561`). Here the verdict simply says
//!   `may_pump == false` while the tank is empty, and the applier drives the
//!   pump to that. The observable behaviour is identical, and it is *strictly*
//!   safer: the C++ version depends on the transition being observed exactly
//!   once, so a missed edge leaves a pump running against a dry tank.
//! * **Every call is checked, not every call site.** The C++ checks
//!   `emergencyMode_` and `waterTankEmpty_` inside `HardwareManager`'s
//!   methods, which every caller happens to use. Here the check is the
//!   verdict, and `04 §4` makes the applier the only code that can reach an
//!   actuator at all.
//! * **`Telemetry` carries the machine state.** S5 is a function of the current
//!   state, and the state is an input to the decision, not to the safety latch,
//!   so it belongs in the telemetry rather than in [`SafetyState`].

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use cc_domain::hardware::{RelayTriggerType, TemperatureSensorType};
use cc_domain::process::BrewMode;
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};

/// The number of consecutive above-threshold readings that trip emergency stop.
///
/// `EmergencyStopManager::DEBOUNCE_COUNT = 3` (`EmergencyStopManager.h:106`).
/// With the production 400 ms sensor interval (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS`)
/// that is roughly 1.2 s of sustained overheat.
pub const DEBOUNCE_COUNT: u8 = 3;

/// The temperature below which an emergency stop may be cleared.
///
/// `Temperature::EMERGENCY_SAFE_TEMP_C = 100.0`
/// (`include/clevercoffee/constants/Temperature.h:11`), used at
/// `EmergencyStopManager.cpp:83`.
pub const EMERGENCY_SAFE_TEMP_C: Celsius = Celsius::new(100.0);

/// The configuration inputs the safety reducer needs.
///
/// Deliberately tiny: it holds only the values that can change whether an
/// actuator may be energised, and nothing else. Everything else in the
/// 98-parameter schema is irrelevant to a verdict, and putting it here would
/// make the reducer's inputs impossible to audit.
///
/// The set has grown twice past "four", both times by exactly the fields a new
/// [`validate_config`] rule needed and nothing else: [`Self::effective_brew_setpoint`]
/// and the brew-stop trio below. What the struct is *not* allowed to become is a
/// mirror of `cc_config::Config` — the shape to preserve is "every field is one
/// somebody had to think about", not "every field is small".
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
    /// **The C++'s own definition of a fitted scale**, not this port's: it is
    /// the guard on every scale command (`WebServerManager.cpp:540,563`) and the
    /// third argument of `recordBrewIfQualified` (`BrewStates.cpp:311`), which
    /// is [`crate::validate_config`]'s subject as much as the reducer's is.
    ///
    /// It is a configuration value rather than a live probe, which is the point:
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
    /// the offset is part of that number. `brew.temp_offset` is bounded 0..=20
    /// (`defaults.h:84-85`), which is enough to push a legal `brew.setpoint`
    /// past a legal `safety.emergency_temp`.
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
    /// **Not used by [`validate_config`] any more**, and that is a deliberate
    /// reversal. It is still here because a configuration's sensor type is part
    /// of the configuration and the firmware has to carry it somewhere, and
    /// because the `cc-config` schema and this struct are the two ends of the
    /// same key.
    ///
    /// Both sensor types are now implemented — see
    /// `cc_protocol::sensor::ds18b20` and `cc_protocol::sensor::tsic306` — so
    /// there is no `ConfigViolation` left for either of them and nothing to
    /// validate. An earlier revision carried an
    /// `UnsupportedTemperatureSensor` variant that refused `TSIC_306` on the
    /// grounds that no Rust driver existed; R3-07 added the driver, and the
    /// variant has been deleted rather than left behind as a dead arm.
    pub temperature_sensor: TemperatureSensorType,
}

impl Default for SafetyConfig {
    /// The C++ compiled-in defaults: `Config.h:813-829` sets
    /// `emergencyTemp` to 150.0 with range 120-180, and `emergencyStopHysteresis`
    /// to 5.0 with range 1-15.
    ///
    /// NOTE the divergence from `Temperature::EMERGENCY_THRESHOLD_C = 145.0`
    /// (`Temperature.h:6`): that constant is *not* what the running firmware
    /// uses. `EmergencyStopManager.cpp:18` reads `config_.emergencyStopTemp`, and
    /// that parameter's default is 150.0. The 145.0 constant is dead. See the
    /// crate report — this is finding 2.
    fn default() -> Self {
        Self {
            emergency_temp: Celsius::new(150.0),
            emergency_hysteresis: Celsius::new(5.0),
            // `BrewMode::Manual`, `brewByTimeEnabled = false` and
            // `brewByWeightEnabled = false`: `Config.h:236` and `defaults.h`
            // give every one of them as false/Manual, so the shipped defaults
            // carry no brew-stop rule of their own — a default configuration
            // that its own validator refuses is a machine that will not run.
            //
            // `scale_fitted: false` is the C++ default too
            // (`hardwareSensorsScaleEnabled`, `Config.h`) and is also the honest
            // answer for the machine this firmware was built for: GPIO32/25/33
            // are unconnected (09 §23). It is the *conservative* direction here,
            // because with no scale the by-weight stop cannot fire and the rule
            // below can only refuse more, never less.
            brew_mode: BrewMode::Manual,
            brew_by_time_enabled: false,
            brew_by_weight_enabled: false,
            scale_fitted: false,
            steam_setpoint: Celsius::new(120.0),
            effective_brew_setpoint: Celsius::new(95.0),
            heater_relay_trigger: RelayTriggerType::HighTrigger,
            pump_relay_trigger: RelayTriggerType::HighTrigger,
            valve_relay_trigger: RelayTriggerType::HighTrigger,
            // `TSIC_306`, matching `Config.h:1085-1092`:
            //
            //   EnumParamDef<Hardware::TemperatureSensorType> hardwareSensorsTemperatureType{
            //       "hardware.sensors.temperature.type",
            //       Hardware::TemperatureSensorType::TSIC_306, ...
            //
            // An earlier revision defaulted this to `DALLAS_DS18B20` and made
            // `validate_config` reject `TSIC_306`, so that a default which the
            // validator refused could never be the machine's own configuration.
            // That reasoning was sound *given a missing driver*; the driver now
            // exists (`cc_protocol::sensor::tsic306`, R3-07) and both types are
            // supported, so the parity default is restored.
            //
            // **What this does not mean:** that a TSIC-306 is fitted to this
            // machine. It is not — the probe on the board is a DS18B20 (family
            // 0x28, ROM `286937aacd78af41`, measured 2026-09-28). A machine
            // built with this default and a DS18B20 fitted reports
            // `NOT_CONNECTED`/no-presence and says so, because the two drivers
            // fail differently and visibly. `cc-firmware` therefore selects the
            // driver from the *board*, not from this default; see its
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
    /// S1's debounce counts **samples**: the C++ increments
    /// `emergencyTempReadingCount_` once per `updateTemperature()`, which the
    /// coordinator calls on its 400 ms sensor cadence, so `DEBOUNCE_COUNT = 3` is
    /// about **1.2 s** of sustained overheat. This port calls [`reduce`] once
    /// per **control tick** — every 10 ms — with whatever the last conversion
    /// produced, so without a sequence number the *same* reading is counted
    /// about forty times and the debounce trips in **30 ms**.
    ///
    /// A probe spike (a 1-Wire CRC retry, a flash write, someone moving the
    /// probe on a boiler at 155 °C) would then latch an emergency stop the C++
    /// rides out mid-brew. That is the most user-visible divergence in the safety
    /// path, and it is invisible in a test that feeds one reading per call.
    ///
    /// So the caller sets this to the driver's own sample counter and
    /// [`reduce`] advances the debounce **only when it changes**. The latching
    /// check and the hysteresis are unaffected: they are properties of the
    /// reading, not of how often we look.
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
    /// Whether emergency stop is latched. Mirrors `emergencyActive_`.
    pub latched: bool,
    /// Consecutive above-threshold readings so far. Mirrors
    /// `emergencyTempReadingCount_`.
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
    /// Strictly greater: a reading *exactly* at the threshold does not count,
    /// which is what `EmergencyStopManager.cpp:36` does.
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

    /// Latch emergency stop. Mirrors `triggerEmergency()`
    /// (`EmergencyStopManager.cpp:92-97`): idempotent, and deliberately does
    /// *not* touch the debounce counter, so the operator can see how many
    /// readings had accumulated when the trip happened.
    pub fn trigger(&mut self) {
        self.latched = true;
    }

    /// Clear emergency stop. Mirrors `clearEmergency()`
    /// (`EmergencyStopManager.cpp:99-105`), which also zeroes the counter.
    pub fn clear(&mut self) {
        self.latched = false;
        self.high_reading_count = 0;
        // And forget which sample the counter was on, or the next pass would
        // see an unchanged `sample_seq` and refuse to re-count the sample that
        // is still over threshold.
        self.last_sample_seq = None;
    }

    /// Forget everything. Mirrors `reset()` (`EmergencyStopManager.cpp:107-110`).
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
    /// **No C++ equivalent** — `openSteamValve` checks only `emergencyMode_`
    /// ([09 §2](../../docs/history/cpp-findings.md)). See
    /// [`steam_flow_allowed`].
    NotASteamState {
        /// The state that was refused.
        state: MachineState,
    },
}

/// The decision: may each actuator be energised right now?
///
/// Four independent permissions plus the latch. They are independent by
/// construction — an empty tank must not stop the heater (the boiler is
/// separate from the reservoir) and an emergency must stop everything — so they
/// are four booleans rather than one enum. Each permission is written to a
/// distinct actuator, and the shape is fixed by the R2-05 brief as much as by
/// the C++'s guards. The enum argument, and the `struct_excessive_bools`
/// allowance it earns, are on the definition.
#[allow(
    clippy::struct_excessive_bools,
    reason = "an enum would either lose the \"heater yes, pump no\" combination or \
              grow a combinatorial number of variants. The C++ guards have the \
              same shape — `emergencyMode_`, `waterTankEmpty_` and `valveState_` \
              are separate fields consulted independently \
              (`HardwareManager.cpp:278,325,354,398`)"
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
    /// The tank condition is a **deliberate divergence** ([09 §3](../../docs/history/cpp-findings.md)):
    /// the C++'s `openWaterValve` checks only `emergencyMode_`.
    pub may_open_water: bool,
    /// May the steam valve be energised? `false` when the latch is set **or**
    /// the current state is not a steam-flow state.
    ///
    /// The state condition is a **deliberate divergence**
    /// ([09 §2](../../docs/history/cpp-findings.md)): the C++'s
    /// `openSteamValve` checks only `emergencyMode_`, and the steam and water
    /// valves are the *same physical relay*.
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

    /// Everything refused because the emergency latch is set. This is the
    /// emergency-shutdown actuator state of `HardwareManager::disableAllHardware`
    /// (`HardwareManager.cpp:528-542`), expressed as a permission set.
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
/// A `match` with **no `_` arm**, on purpose. Adding a 19th `MachineState`
/// variant makes this function fail to compile until someone has decided
/// whether the new state flows water. That is the entire mechanism: the C++
///
/// ```cpp
/// const bool waterFlowActive = (isBrewState(state) && state != BREW_FINISHED) ||
///                              isManualFlushState(state) ||
///                              (isBackflushState(state) && state != BACKFLUSH_IDLE &&
///                               state != BACKFLUSH_FINISHED);
/// ```
///
/// (`BrewHandler.h:109-112`) hides the decision inside predicates, so a new
/// state silently inherits "no water" without anyone noticing. Here it is a
/// compile error (04 §4, skill §4).
///
/// The whitelist, transcribed from the C++ expression above:
///
/// * brew states except `BREW_FINISHED` — 31, 32, 33
/// * `MANUAL_FLUSH_RUNNING` — 36
/// * backflush states except `BACKFLUSH_IDLE` and `BACKFLUSH_FINISHED` — 61, 62
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
/// A `match` with **no `_` arm**, for the same reason as
/// [`water_flow_allowed`]: a new state must be classified before it compiles.
/// There is no C++ expression to transcribe here, so the whitelist is derived —
/// and the derivation is the argument, so it is written out.
///
/// # The C++ has no steam whitelist at all
///
/// ```cpp
/// void HardwareManager::openSteamValve() noexcept {
///     if (emergencyMode_) {
///         LOG(WARNING, "Cannot open steam valve - emergency mode active");
///         return;
///     }
///     ...
/// ```
/// (`src/hardware/HardwareManager.cpp:397-400`)
///
/// `emergencyMode_` is the *only* check. There is no `steamSafetyShutdownCheck`
/// anywhere in the tree — `BrewHandler::valveSafetyShutdownCheck`
/// (`BrewHandler.h:105-122`) is the water valve's, and it names only the water
/// relay. The same three lines, minus the whitelist, at
/// `MachineStateContext.cpp:556-558`.
///
/// # Why that is a real gap and not a theoretical one
///
/// Steam and water share **one relay**:
///
/// > "Steam and water valves share the same physical relay. This enum tracks
/// > which valve(s) should be open, ensuring correct relay control."
///
/// — `include/clevercoffee/hardware/ValveState.h:8-11`.
///
/// The pin map confirms it: one valve relay, GPIO17
/// (`include/clevercoffee/hardware/pinmapping.h:39`). So an ungated
/// `openSteamValve()` does not open some other solenoid — **it energises the
/// very relay that S5 spends its whole existence keeping closed.** The C++ is
/// protected only by the accident that nothing calls it: `rg -n openSteamValve`
/// over `src/` and `include/` finds the definition, the pass-through, the
/// interface declaration and nothing else. A port that gives the state machine
/// the ability to express it — which this one does, via
/// `Effect::OpenSteamValve` — inherits the gap with none of the accident.
///
/// # The derivation
///
/// The whitelist is *the set of states in which steam is drawn*. From the C++:
///
/// 1. `SteamRunningState::onEntryImpl` is the **only** place in the tree that
///    turns steam mode on: `context.setSteamMode(true)`
///    (`src/state/states/SteamStates.cpp:16`). Every other state either never
///    touches it or clears it — `SteamRunningState::onExitImpl` (`:21`),
///    `SystemStates.cpp:17`.
///
/// 2. Steam mode is what the process controller keys off to select the steam
///    setpoint, and nothing else: `updateSetpoint(isSteamModeActive())`
///    (`src/control/ProcessController.cpp:119-120`, `:235-244`). So the
///    *only* state in which the machine is holding the boiler at
///    `steam.setpoint` is `STEAM_RUNNING`.
///
/// 3. Water injection during steam — the second place the C++ moves water while
///    steaming — happens **inside** `STEAM_RUNNING`, not in a state of its own:
///    `SteamRunningState::update` drives the pump from the water switch
///    (`SteamStates.cpp:36-46`). `MachineStateId` has no injection state; the
///    eleven state ids in `MachineStateIds.h` include exactly one steam state
///    (`STEAM_RUNNING = 51`).
///
/// 4. `WebServerManager.cpp:444-445` can flip `isSteamModeActive()` directly
///    over HTTP without changing the state. That is a debug surface, and it is
///    the case that makes a *mode-based* gate wrong: the steam mode would be on
///    in `PID_NORMAL`, where the setpoint has not changed and no steam can be
///    drawn. A state-based whitelist ignores it, correctly.
///
/// Therefore: **`STEAM_RUNNING`, and only `STEAM_RUNNING`.** Every other state —
/// including `PID_NORMAL`, `BREW_RUNNING`, `BACKFLUSH_FILLING` and `STANDBY` —
/// must hold the steam valve closed.
///
/// The single-state whitelist is not a stub left for later. A wider one would be
/// actively wrong: because the relay is shared, listing a water-flow state as a
/// steam state would make `may_open_steam` agree with `may_open_water` and
/// quietly re-open S5's hole from the other side.
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
/// transition — the same order `LoopManager::update()` uses
/// (`src/core/LoopManager.cpp:617-620`, emergency check then PID compute at step
/// 7). `now` is accepted for interface symmetry with the other reducers and for
/// the deadman timeout that R2-09b will add; nothing in S1-S5 is time-based, so
/// it does not participate in the decision today.
///
/// # The rules, in evaluation order
///
/// 1. **S1, invalid reading.** A reading outside
///    [`Celsius::MIN`]..=[`Celsius::MAX`] (or `NaN`) latches immediately, with
///    **no debounce** — `EmergencyStopManager.cpp:23-33` says so in a comment
///    ("no debouncing for safety") and it is right: a probe reading 222 °C is
///    reporting a fault, not a temperature.
/// 2. **S1, overtemp with debounce.** Above `emergency_temp` the counter
///    increments; at [`DEBOUNCE_COUNT`] it latches. A reading exactly at the
///    threshold does not count.
/// 3. **S1 reset / S3 clear.** Below `emergency_temp - emergency_hysteresis`
///    the counter resets *and*, if latched, the latch clears — but only if the
///    reading is valid **and** at or below [`EMERGENCY_SAFE_TEMP_C`]. Between
///    those two temperatures the counter is left alone, which is the hysteresis
///    that stops the machine oscillating around the threshold.
/// 4. **S2.** If latched, nothing may be energised.
/// 5. **S4.** An empty tank blocks the pump and — deliberately, unlike the C++
///    — the water valve. The boiler is fed from the reservoir, so an empty tank
///    means the pump is running dry; leaving the valve open is not what makes
///    that safe, it is just as much of a mistake.
/// 6. **S5.** A state outside [`water_flow_allowed`] blocks the water valve.
/// 7. **S5'.** A state outside [`steam_flow_allowed`] blocks the steam valve.
///    This check does not exist in the C++ at all
///    ([09 §2](../../docs/history/cpp-findings.md)); it is added here
///    because the steam valve is the same relay as the water valve and the port
///    can reach it. See [`steam_flow_allowed`] for the derivation.
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
        // Deliberate divergence from the C++ (09 §3): `openWaterValve` does not
        // check `waterTankEmpty_` there.
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
/// Port of `EmergencyStopManager::isEmergencyCleared`
/// (`EmergencyStopManager.cpp:71-90`): the reading must be plausible **and** at
/// or below [`EMERGENCY_SAFE_TEMP_C`]. The comparison is `>`, so exactly
/// 100.0 °C does clear — 100 °C is water at atmospheric pressure, and the
/// machine will not be steaming.
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
    /// `safety.emergency_temp` defaults to 150 (`Config.h:813-829`) and
    /// `brew.setpoint` is bounded 20..=110 by the schema
    /// (`Config.h:795-802`, `defaults.h:81-82`), so the schema bounds alone
    /// keep the defaults 40 °C apart — which is why the C++ is not exposed to
    /// this through `/api/parameters`. It *was* exposed here, because
    /// `POST /api/setpoint` filtered to `0..=150` instead of the schema's range
    /// and wrote the result straight to the store without passing
    /// `cc_config::assign::parse` (`WebServerManager.cpp:391-402` reads
    /// `0..=150` and then range-checks nothing; `Config.h:795` is what would
    /// have, and it was bypassed). The check lives here so that **every** write
    /// path is covered, not just the one that was found.
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
    /// An ESP32 GPIO is high-impedance before `pinMode()` runs and while the
    /// chip resets, and a low-trigger relay board energises its coil when the
    /// input is low *or floating*. Firmware cannot prevent that: it loses
    /// control of the pin before its first instruction. The heater would be
    /// energised on every boot. This is a wiring property, not a code path, so
    /// no amount of firmware can make it safe.
    /// A `LOW_TRIGGER` heater relay. See [`Self::PumpRelayLowTrigger`] for why
    /// the same hazard applies to the other two.
    HeaterRelayLowTrigger,
    /// A `LOW_TRIGGER` pump relay: the pump runs at every boot, before any
    /// firmware exists to stop it.
    ///
    /// The C++ honours this setting (`Relay.cpp:13-27`), and so does this port —
    /// `actuators::Polarity` exists for it — but a configuration carrying one is
    /// refused at boot rather than obeyed, because the pin floats before
    /// firmware runs and "honoured" would mean "energised from power-on".
    PumpRelayLowTrigger,
    /// A `LOW_TRIGGER` valve relay: water at every boot.
    ValveRelayLowTrigger,
    /// An automatic brew whose only stop condition is a weight, on a machine
    /// with no scale.
    ///
    /// **A pump with an open water valve and nothing to stop it.**
    /// `BrewRunningState::checkSpecificTransitions` (`BrewStates.cpp:266-303`)
    /// ends an automatic shot on exactly two things: `brew.by_time`, whose
    /// target comes from `initTotalTargetBrewTime` (`:53-62`) and is **zero**
    /// unless `brew.by_time.enabled`, and `brew.by_weight`, which compares
    /// `getCurrentBrewWeight()` against `brew.by_weight.target_weight`. With
    /// `by_time` off and no scale to weigh with, neither arm can ever fire and
    /// the only thing left is the brew switch or `BREW_PUMP_TIMEOUT_MS` — 300
    /// seconds (`timing::BREW_PUMP_TIMEOUT_MS`) of water.
    ///
    /// The configuration is **legal in every field**. `brew.mode` is an enum
    /// with two values, both offered; `by_time.enabled` and `by_weight.enabled`
    /// are independent booleans that both default to `false`; `target_weight`
    /// is bounded `0 ..= 500` (`defaults.h`) and defaults to 36. So no
    /// single-parameter range check anywhere can catch this — it takes two
    /// settings read together, which is exactly what this function is for.
    ///
    /// **The C++ cannot catch it either, and does not need to be told twice.**
    /// `BrewStates.cpp:283-299` has the identical arm and the identical hole.
    /// What differs is that in the C++ the weight is *never produced at all*
    /// (09 §23: `HardwareContext::setScale` has no caller, so `getBrewWeight()`
    /// returns 0 forever), so the by-weight arm is dead there unconditionally —
    /// an operator who enabled it got a 300-second shot, which is the same
    /// defect with a larger blast radius. Here the driver exists
    /// (`cc_hal_esp32::Sampler`), so the arm is live and the weight is threaded
    /// into it; that is precisely what makes the *unreachable* case the one
    /// worth refusing. Inherited, not introduced — see
    /// [09 §23](../docs/history/cpp-findings.md).
    BrewByWeightWithNoScale,
}

impl ConfigViolation {
    /// Keys this violation implicates. Repair reverts these. HTTP 400 names them.
    ///
    /// Headroom rules name `safety.emergency_temp`, not the setpoint: raising
    /// the threshold cannot fail the rule. Not always enough: `steam.setpoint`
    /// 140 plus `safety.emergency_hysteresis` 15 exceeds the 150 default.
    /// `cc_config::repair_unsafe` then uses `REPAIR_ESCALATION_KEYS`. Finding #12.
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

/// Validate a configuration before it is run or stored.
///
/// Recovered from the previous Rust firmware ([08 §4.1](../docs/history/recovered-oracle.md)),
/// which is the only implementation known to have had this check. The C++ has
/// neither rule: it validates each parameter in isolation
/// (`Config.h:isValid`) and never looks at two parameters at once, so a
/// perfectly legal `steam.setpoint = 140` together with
/// `safety.emergency_temp = 120` is accepted, and the machine then refuses to
/// steam.
///
/// # Errors
///
/// Returns the first [`ConfigViolation`] found, or `Ok(())` if the
/// configuration is safe to run.
///
/// # Note on the thresholds
///
/// Both parameters are individually range-checked by `cc-config` before this
/// is called, so the interesting case is not an out-of-range value but a
/// *legal* pair that is unsafe in combination: `emergency_temp` in 120-180 and
/// `steam_setpoint` in 100-140 overlap across 120-140, and
/// `emergency_hysteresis` adds up to 15 more degrees on top.
///
/// The brew pair is checked for the same reason and with the same asymmetry.
/// S1's over-temperature test is **strictly greater**
/// ([`SafetyState::is_over_threshold`]), so the failure mode of a brew setpoint
/// at or above the threshold is not a machine that trips — it is a machine the
/// PID drives *to* the threshold and holds there, with a debounce that can
/// never count a breach. That is why the check belongs here rather than at the
/// write path: `POST /api/setpoint` once filtered to `0..=150` and persisted the
/// result without going through `cc_config::assign::parse`, and a rule enforced
/// only where the bug was found is a rule waiting for the next writer.
///
/// The brew-stop rule ([`ConfigViolation::BrewByWeightWithNoScale`]) is here for
/// the same reason and one more: it is a **four-way** conjunction in which no
/// term is out of range on its own, so there is no single-parameter range check
/// anywhere in the firmware that could substitute for it.
pub fn validate_config(cfg: &SafetyConfig) -> Result<(), ConfigViolation> {
    let steam_headroom = cfg.steam_setpoint.raw() + cfg.emergency_hysteresis.raw();
    // A bench build compiles the two headroom rules out so the over-temp trip
    // in `operations/runbook.md` §13.1 can be reached with a hand-warmed
    // DS18B20. Everything below this point still applies: the relay rules, the
    // brew-target rule, and everything after them. See
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

    // **Every relay, not just the heater.** The recovered oracle refused
    // `LOW_TRIGGER` for the heater because such a relay energises whenever its
    // pin floats, which is *before* any firmware runs — so a low-trigger pump or
    // valve relay runs water at boot, with no firmware in the loop to stop it.
    // Nothing about that hazard is heater-specific, and the C++ wires all three
    // trigger types into `Relay::on()` (`HardwareManager.cpp:73,80,87`), so a
    // stored configuration can carry any of them.
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
    // rather than about what cannot be made safe in firmware at all.
    //
    // The rule is the conjunction of the two arms of
    // `BrewRunningState::checkSpecificTransitions` (`BrewStates.cpp:283-299`),
    // negated: the by-time arm cannot fire because `by_time` is off (and
    // `initTotalTargetBrewTime`, `:53-62`, returns 0.0 in that case, so
    // `machine.brew.target_ms` is 0 and the arm's own `target > 0.0` test fails
    // too), and the by-weight arm cannot fire because there is nothing to weigh.
    // What is left is `requests.brew_stop` — the switch — and the 300-second
    // pump watchdog.
    //
    // `brew.by_weight.target_weight > 0.0` is deliberately **not** part of this
    // test. It is part of the arm's, so a target of 0 makes that arm dead too,
    // but the schema bounds the field at `0 ..= 500` and defaults it to 36, and
    // "brew by weight, to zero grams" is not a thing an operator configures by
    // accident in a way the target check would usefully catch. Enabling
    // by-weight on a machine that cannot weigh is refused either way.
    if !cfg.scale_fitted
        && cfg.brew_mode == BrewMode::Automatic
        && !cfg.brew_by_time_enabled
        && cfg.brew_by_weight_enabled
    {
        return Err(ConfigViolation::BrewByWeightWithNoScale);
    }

    // `cfg.temperature_sensor` is deliberately not checked. Both types have a
    // driver (R1-03 for the DS18B20, R3-07 for the TSIC-306), so there is no
    // value of this field the firmware cannot honour. The
    // `UnsupportedTemperatureSensor` refusal that used to be here is gone with
    // the variant itself; the reasoning is in `intentional-diffs.md`.
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
    /// This is the fail-closed rule from [08 §4.1](../docs/history/recovered-oracle.md):
    /// refuse to store an unsafe configuration, and refuse to run one that was
    /// stored by an older or corrupted image. The alternative — running a
    /// configuration that trips emergency stop on every steam shot, or that
    /// energises the heater at reset — is not an option for a machine that
    /// heats water to 120 °C under pressure.
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
/// than discovering it on the next boot. The recovered firmware logged
/// `"refusing to store an unsafe configuration"` (08 §4.1).
///
/// # Errors
///
/// Returns the [`ConfigViolation`] that makes the configuration unsafe, or the
/// configuration itself on success so the caller can pass it straight to the
/// store.
pub fn check_storable(cfg: &SafetyConfig) -> Result<&SafetyConfig, ConfigViolation> {
    validate_config(cfg).map(|()| cfg)
}
