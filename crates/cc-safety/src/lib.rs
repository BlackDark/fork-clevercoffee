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
//! | **S5** valve fail-safe | `include/clevercoffee/handlers/BrewHandler.h:105-122` | [`water_flow_allowed`] |
//!
//! Plus the two fail-closed configuration rules recovered from the previous
//! Rust firmware ([08 §4.1](../docs/rust-migration/08-recovered-oracle.md)):
//! [`validate_config`] and [`load_or_default`].
//!
//! # Semantics that are *not* the C++'s, and why
//!
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

use cc_domain::hardware::RelayTriggerType;
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
/// Deliberately tiny: it holds only the four values that can change whether an
/// actuator may be energised. Everything else in the 98-parameter schema is
/// irrelevant to a verdict, and putting it here would make the reducer's inputs
/// impossible to audit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafetyConfig {
    /// `safety.emergency_temp` — the over-temperature threshold.
    pub emergency_temp: Celsius,
    /// `safety.emergency_hysteresis` — how far below the threshold the machine
    /// must fall before the debounce counter resets.
    pub emergency_hysteresis: Celsius,
    /// `steam.setpoint` — needed only by [`validate_config`], and here because
    /// the emergency threshold must clear the steam setpoint or the machine
    /// would stop itself during normal steaming.
    pub steam_setpoint: Celsius,
    /// `hardware.relays.heater.trigger_type` — needed only by
    /// [`validate_config`]. See [`RelayTriggerType`]: a low-trigger heater relay
    /// cannot be made safe in firmware.
    pub heater_relay_trigger: RelayTriggerType,
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
            steam_setpoint: Celsius::new(120.0),
            heater_relay_trigger: RelayTriggerType::HighTrigger,
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
}

impl Telemetry {
    /// Build telemetry with a full tank in the given state.
    #[must_use]
    pub const fn new(temperature: Celsius, water_tank_full: bool, state: MachineState) -> Self {
        Self {
            temperature,
            water_tank_full,
            state,
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
}

impl SafetyState {
    /// The clear, unlatched state.
    pub const CLEAR: Self = Self {
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
}

/// The decision: may each actuator be energised right now?
///
/// Four independent permissions plus the latch. They are independent by
/// construction — an empty tank must not stop the heater (the boiler is
/// separate from the reservoir) and an emergency must stop everything — so they
/// are four booleans rather than one enum. See the `struct_excessive_bools`
/// allowance on the definition.
// The verdict is four independent permissions plus a latch, and the shape is
// fixed by the R2-05 brief and by how the applier reads it: each permission is
// written to a distinct actuator. An enum would either lose the "heater yes,
// pump no" combination or grow a combinatorial number of variants. The C++
// guards have the same shape — `emergencyMode_`, `waterTankEmpty_` and
// `valveState_` are separate fields consulted independently
// (`HardwareManager.cpp:278,325,354,398`).
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Verdict {
    /// May the heater be energised? `false` when the latch is set.
    pub may_heat: bool,
    /// May the pump be energised? `false` when the latch is set **or** the
    /// water tank is empty.
    pub may_pump: bool,
    /// May the water (three-way) valve be energised? `false` when the latch is
    /// set **or** the current state is not a water-flow state.
    pub may_open_water: bool,
    /// May the steam valve be energised? `false` when the latch is set.
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
    const ALL_REFUSED: Self = Self {
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
/// 5. **S4.** An empty tank blocks the pump only.
/// 6. **S5.** A state outside [`water_flow_allowed`] blocks the water valve.
///    Note this is the water (three-way) valve only. The steam valve is *not*
///    whitelist-gated in the C++ — `openSteamValve` checks only
///    `emergencyMode_` (`HardwareManager.cpp:397-400`) and no `steamSafetyShutdownCheck`
///    exists. That gap is preserved here for parity and called out in the crate
///    report; closing it is a deliberate, recorded change, not a port.
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
        state.high_reading_count = state.high_reading_count.saturating_add(1);
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

    // ---- S4 and S5: the per-actuator interlocks. ---------------------------
    let mut verdict = Verdict::ALL_PERMITTED;
    if !telemetry.water_tank_full {
        verdict.may_pump = false;
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
    /// The heater relay is configured `LOW_TRIGGER`.
    ///
    /// An ESP32 GPIO is high-impedance before `pinMode()` runs and while the
    /// chip resets, and a low-trigger relay board energises its coil when the
    /// input is low *or floating*. Firmware cannot prevent that: it loses
    /// control of the pin before its first instruction. The heater would be
    /// energised on every boot. This is a wiring property, not a code path, so
    /// no amount of firmware can make it safe.
    HeaterRelayLowTrigger,
}

/// Validate a configuration before it is run or stored.
///
/// Recovered from the previous Rust firmware ([08 §4.1](../docs/rust-migration/08-recovered-oracle.md)),
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
pub fn validate_config(cfg: &SafetyConfig) -> Result<(), ConfigViolation> {
    let steam_headroom = cfg.steam_setpoint.raw() + cfg.emergency_hysteresis.raw();
    if cfg.emergency_temp.raw() <= steam_headroom {
        return Err(ConfigViolation::EmergencyTempTooLowForSteam {
            emergency_temp: cfg.emergency_temp,
            steam_setpoint: cfg.steam_setpoint,
            emergency_hysteresis: cfg.emergency_hysteresis,
        });
    }

    if cfg.heater_relay_trigger == RelayTriggerType::LowTrigger {
        return Err(ConfigViolation::HeaterRelayLowTrigger);
    }

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
    /// This is the fail-closed rule from [08 §4.1](../docs/rust-migration/08-recovered-oracle.md):
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
