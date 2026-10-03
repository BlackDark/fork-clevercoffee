//! Every branch of the safety paths S1-S5, one named test each, plus both
//! fail-closed configuration rules recovered from 08 §4.1.
//!
//! The S1/S3 assertions are ported case-for-case from
//! `test/test_emergency_stop_manager/test_main.cpp` (18 cases) and the S4
//! assertions from `test/test_hardware_water_tank/test_main.cpp` (5 cases).
//! Where the C++ test drives hardware through a pin spy, the Rust tests assert
//! on the verdict the applier is obliged to write, which is the same decision
//! observed one layer earlier.

use cc_domain::hardware::{RelayTriggerType, TemperatureSensorType};
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_safety::{
    can_clear, check_storable, load_or_default, reduce, steam_flow_allowed, validate_config,
    water_flow_allowed, ConfigOrigin, ConfigViolation, LoadedConfig, Outcome, Reason, SafetyConfig,
    SafetyState, Telemetry, Verdict, DEBOUNCE_COUNT, EMERGENCY_SAFE_TEMP_C,
};

/// The compiled-in defaults: `emergency_temp` 150, `hysteresis` 5, steam
/// setpoint 120 (`Config.h:813-829`, `defaults.h:20`).
fn cfg() -> SafetyConfig {
    SafetyConfig::default()
}

/// The same defaults with a different threshold, mirroring the C++ test's
/// `SetUp`.
fn cfg_with(emergency_temp: f32, hysteresis: f32) -> SafetyConfig {
    SafetyConfig {
        emergency_temp: Celsius::new(emergency_temp),
        emergency_hysteresis: Celsius::new(hysteresis),
        ..SafetyConfig::default()
    }
}

/// A **global** sample counter, so every `telemetry()` below is a new reading.
///
/// `reduce` counts *samples*, not invocations — see
/// `Telemetry::sample_seq` — so a test that models successive readings has to say
/// they are successive. This counter is the simplest way to say that without
/// threading a sequence through every call site.
static SAMPLE_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn telemetry(temp: f32) -> Telemetry {
    let seq = SAMPLE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Telemetry {
        sample_seq: seq,
        ..Telemetry::new(Celsius::new(temp), true, MachineState::PidNormal)
    }
}

/// The same reading delivered twice: the caller re-checked without a new
/// conversion, which is exactly what the control task does at 100 Hz against a
/// 2.5 Hz probe.
fn repeat_same_sample(telemetry: &Telemetry) -> Telemetry {
    *telemetry
}

fn reduce_temp(state: SafetyState, temp: f32, config: &SafetyConfig) -> Outcome {
    reduce(&state, &telemetry(temp), config, Millis::ZERO)
}

fn verdict_of(outcome: &Outcome) -> Verdict {
    outcome.verdict
}

// ===================================================================== S1 —
// Overtemp with debounce. Ported from test_emergency_stop_manager.

#[test]
fn s1_single_high_reading_does_not_trigger() {
    let config = cfg();
    let threshold = config.emergency_temp.raw();
    let out = reduce_temp(SafetyState::CLEAR, threshold + 1.0, &config);

    assert!(!verdict_of(&out).latched, "one reading must not trip");
    assert_eq!(out.state.high_reading_count, 1);
    // Parity note, and worth stating plainly: the C++ keeps heating while the
    // debounce counts, so at the production 400 ms sensor interval
    // (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS`) the machine can hold the
    // heater on through two further readings - 800 ms - at a temperature
    // already above the emergency threshold. That is
    // `EmergencyStopManager.cpp:36-50` behaving as designed: the debounce
    // exists to reject sensor noise. It is a real, deliberate exposure, and
    // narrowing it would be a recorded behaviour change, not a port.
    assert!(
        verdict_of(&out).may_heat,
        "the C++ does not block while counting"
    );
}

#[test]
fn s1_two_consecutive_high_readings_do_not_trigger() {
    let config = cfg();
    let threshold = config.emergency_temp.raw();
    let first = reduce_temp(SafetyState::CLEAR, threshold + 1.0, &config);
    assert_eq!(first.state.high_reading_count, 1);

    let out = reduce_temp(first.state, threshold + 1.0, &config);
    assert!(!verdict_of(&out).latched);
    assert_eq!(out.state.high_reading_count, 2);
}

#[test]
fn s1_three_consecutive_high_readings_trip() {
    let config = cfg();
    let threshold = config.emergency_temp.raw();
    let mut state = SafetyState::CLEAR;
    for expected in 1..DEBOUNCE_COUNT {
        let out = reduce_temp(state, threshold + 1.0, &config);
        assert!(
            !verdict_of(&out).latched,
            "reading {expected} must not trip"
        );
        assert_eq!(out.state.high_reading_count, expected);
        state = out.state;
    }
    let out = reduce_temp(state, threshold + 1.0, &config);
    assert!(
        verdict_of(&out).latched,
        "the third consecutive reading must trip"
    );
    assert_eq!(out.state.high_reading_count, DEBOUNCE_COUNT);
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::Overtemp {
            consecutive: DEBOUNCE_COUNT,
            ..
        })
    ));
}

#[test]
fn s1_counter_resets_below_the_hysteresis_threshold() {
    let config = cfg_with(145.0, 10.0);
    let mut state = reduce_temp(SafetyState::CLEAR, 146.0, &config).state;
    state = reduce_temp(state, 146.0, &config).state;
    assert_eq!(state.high_reading_count, 2);

    let out = reduce_temp(state, 145.0 - 10.0 - 1.0, &config);
    assert_eq!(
        out.state.high_reading_count, 0,
        "below threshold - hysteresis"
    );
    assert!(!verdict_of(&out).latched);
}

#[test]
fn s1_counter_is_maintained_inside_the_hysteresis_band() {
    let config = cfg_with(145.0, 10.0);
    let mut state = reduce_temp(SafetyState::CLEAR, 146.0, &config).state;
    state = reduce_temp(state, 146.0, &config).state;
    assert_eq!(state.high_reading_count, 2);

    // Halfway down the hysteresis band: neither counted nor reset.
    let out = reduce_temp(state, 145.0 - 5.0, &config);
    assert_eq!(
        out.state.high_reading_count, 2,
        "the band must hold the count"
    );
    assert!(!verdict_of(&out).latched);
}

#[test]
fn s1_an_interrupted_sequence_restarts_the_count() {
    let config = cfg_with(145.0, 10.0);
    let mut state = reduce_temp(SafetyState::CLEAR, 146.0, &config).state;
    state = reduce_temp(state, 146.0, &config).state;
    assert_eq!(state.high_reading_count, 2);

    state = reduce_temp(state, 145.0 - 10.0 - 1.0, &config).state;
    assert_eq!(state.high_reading_count, 0);

    let out = reduce_temp(state, 146.0, &config);
    assert_eq!(
        out.state.high_reading_count, 1,
        "the count starts again from 1"
    );
}

#[test]
fn s1_temperature_exactly_at_the_threshold_does_not_count() {
    let config = cfg();
    let out = reduce_temp(SafetyState::CLEAR, config.emergency_temp.raw(), &config);
    assert!(!verdict_of(&out).latched);
    assert_eq!(
        out.state.high_reading_count, 0,
        "the test is strictly greater than"
    );
}

#[test]
fn s1_temperature_just_above_the_threshold_counts() {
    let config = cfg();
    let out = reduce_temp(
        SafetyState::CLEAR,
        config.emergency_temp.raw() + 0.1,
        &config,
    );
    assert_eq!(out.state.high_reading_count, 1);
}

#[test]
fn s1_configured_threshold_changes_where_it_trips() {
    // Config.h:815 allows 120-180; the C++ test uses 130.
    let config = cfg_with(130.0, 5.0);
    let mut state = SafetyState::CLEAR;
    for _ in 0..2 {
        state = reduce_temp(state, 131.0, &config).state;
    }
    let out = reduce_temp(state, 131.0, &config);
    assert!(verdict_of(&out).latched, "131 must trip a 130 threshold");
}

#[test]
fn s1_configured_hysteresis_changes_where_it_resets() {
    let mut config = cfg_with(145.0, 10.0);
    let mut state = reduce_temp(SafetyState::CLEAR, 146.0, &config).state;
    state = reduce_temp(state, 146.0, &config).state;
    assert_eq!(state.high_reading_count, 2);

    // Widen the hysteresis to 20: 140 is now inside the band.
    config.emergency_hysteresis = Celsius::new(20.0);
    let out = reduce_temp(state, 140.0, &config);
    assert!(
        out.state.high_reading_count > 0,
        "140 must not reset the count when the hysteresis is 20"
    );
}

// ------------------------------------------- S1, invalid reading: no debounce.

#[test]
fn s1_an_implausibly_low_reading_trips_immediately() {
    let config = cfg();
    let out = reduce_temp(SafetyState::CLEAR, Celsius::MIN.raw() - 1.0, &config);
    assert!(
        verdict_of(&out).latched,
        "no debounce for an invalid reading"
    );
    assert!(!verdict_of(&out).may_heat);
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::InvalidReading { .. })
    ));
}

#[test]
fn s1_an_implausibly_high_reading_trips_immediately() {
    let config = cfg();
    let out = reduce_temp(SafetyState::CLEAR, Celsius::MAX.raw() + 1.0, &config);
    assert!(
        verdict_of(&out).latched,
        "no debounce for an invalid reading"
    );
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::InvalidReading { .. })
    ));
}

#[test]
fn s1_a_nan_reading_trips_immediately() {
    let config = cfg();
    let out = reduce_temp(SafetyState::CLEAR, f32::NAN, &config);
    assert!(
        verdict_of(&out).latched,
        "NaN is a faulted probe, not a temperature"
    );
    assert!(!verdict_of(&out).any_permitted());
}

#[test]
fn s1_the_tsic_fault_sentinels_trip_immediately() {
    // 01 §3: the TSIC-306 driver reports 221/222 for a faulted probe. If these
    // were plausible the machine would debounce a hard sensor fault.
    let config = cfg();
    for sentinel in [221.0f32, 222.0f32] {
        let out = reduce_temp(SafetyState::CLEAR, sentinel, &config);
        assert!(verdict_of(&out).latched, "sentinel {sentinel} must trip");
    }
}

#[test]
fn s1_the_boundary_of_the_valid_range_does_not_trip() {
    let config = cfg();
    for edge in [Celsius::MIN.raw(), Celsius::MAX.raw()] {
        let out = reduce_temp(SafetyState::CLEAR, edge, &config);
        assert!(!verdict_of(&out).latched, "edge {edge} is inside the range");
    }
}

// ===================================================================== S2 —
// The emergency latch. Ports the HardwareManager guard assertions.

/// A pre-latched state plus a reading that does **not** clear it.
///
/// The C++ clears the latch from inside `checkEmergencyConditions` whenever the
/// reading drops below `emergency_temp - hysteresis` *and* below 100 C
/// (`EmergencyStopManager.cpp:60-63`), so a latched state handed to `reduce`
/// together with a cool reading recovers immediately — which is S3, tested
/// separately. To isolate the latch rule (S2) the reading is placed inside the
/// hysteresis band, where the C++ takes neither branch and the latch survives.
fn latched_with_band_temperature() -> (SafetyState, Telemetry) {
    let config = cfg();
    let latched = SafetyState {
        last_sample_seq: None,
        latched: true,
        high_reading_count: DEBOUNCE_COUNT,
    };
    let inside_band = Telemetry::new(
        Celsius::new(config.emergency_temp.raw() - config.emergency_hysteresis.raw() / 2.0),
        true,
        MachineState::BrewRunning,
    );
    (latched, inside_band)
}

#[test]
fn s2_latch_blocks_the_heater() {
    let (latched, telemetry_in) = latched_with_band_temperature();
    let out = reduce(&latched, &telemetry_in, &cfg(), Millis::ZERO);
    assert!(
        !verdict_of(&out).may_heat,
        "enableHeater refuses in emergency mode"
    );
    assert!(verdict_of(&out).latched);
    assert_eq!(
        out.state, latched,
        "the latch and counter must survive untouched"
    );
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::EmergencyLatched)
    ));
}

#[test]
fn s2_latch_blocks_the_pump() {
    let (latched, telemetry_in) = latched_with_band_temperature();
    let out = reduce(&latched, &telemetry_in, &cfg(), Millis::ZERO);
    assert!(
        !verdict_of(&out).may_pump,
        "enablePump refuses in emergency mode"
    );
}

#[test]
fn s2_latch_blocks_the_water_valve() {
    let (latched, telemetry_in) = latched_with_band_temperature();
    let out = reduce(&latched, &telemetry_in, &cfg(), Millis::ZERO);
    assert!(
        !verdict_of(&out).may_open_water,
        "openWaterValve refuses in emergency mode"
    );
}

#[test]
fn s2_latch_blocks_the_steam_valve() {
    let (latched, telemetry_in) = latched_with_band_temperature();
    let out = reduce(&latched, &telemetry_in, &cfg(), Millis::ZERO);
    assert!(
        !verdict_of(&out).may_open_steam,
        "openSteamValve refuses in emergency mode"
    );
}

#[test]
fn s2_latch_refuses_everything_at_once() {
    let (latched, telemetry_in) = latched_with_band_temperature();
    let out = reduce(&latched, &telemetry_in, &cfg(), Millis::ZERO);
    assert!(!verdict_of(&out).any_permitted());
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::EmergencyLatched)
    ));
}

#[test]
fn s2_triggering_twice_changes_nothing() {
    let mut state = SafetyState::CLEAR;
    state.trigger();
    let after_first = state;
    state.trigger();
    assert_eq!(state, after_first, "triggerEmergency is idempotent");
    assert!(state.latched);
}

#[test]
fn s2_clearing_resets_the_counter_too() {
    let mut state = SafetyState {
        last_sample_seq: None,
        latched: true,
        high_reading_count: 3,
    };
    state.clear();
    assert!(!state.latched);
    assert_eq!(
        state.high_reading_count, 0,
        "clearEmergency zeroes the counter"
    );
}

#[test]
fn s2_clearing_when_not_latched_changes_nothing() {
    let mut state = SafetyState::CLEAR;
    state.clear();
    assert_eq!(state, SafetyState::CLEAR);
}

#[test]
fn s2_reset_forgets_everything() {
    let mut state = SafetyState {
        last_sample_seq: None,
        latched: true,
        high_reading_count: 2,
    };
    state.reset();
    assert_eq!(state, SafetyState::CLEAR);
}

// ===================================================================== S3 —
// Recovery. Ported from test_emergency_stop_manager.

#[test]
fn s3_emergency_is_clearable_when_the_reading_is_safe() {
    assert!(
        can_clear(Celsius::new(EMERGENCY_SAFE_TEMP_C.raw() - 1.0)),
        "below 100 C the machine may restart"
    );
}

#[test]
fn s3_exactly_the_safe_temperature_clears() {
    // EmergencyStopManager.cpp:83 uses `>`, so exactly 100.0 does clear.
    assert!(can_clear(EMERGENCY_SAFE_TEMP_C));
}

#[test]
fn s3_emergency_is_not_clearable_while_elevated() {
    assert!(
        !can_clear(Celsius::new(EMERGENCY_SAFE_TEMP_C.raw() + 1.0)),
        "above 100 C the machine stays stopped"
    );
}

#[test]
fn s3_emergency_is_not_clearable_from_an_invalid_reading() {
    assert!(!can_clear(Celsius::new(Celsius::MAX.raw() + 1.0)));
    assert!(!can_clear(Celsius::new(Celsius::MIN.raw() - 1.0)));
    assert!(!can_clear(Celsius::new(f32::NAN)));
}

#[test]
fn s3_emergency_clears_automatically_once_normalised() {
    let config = cfg_with(145.0, 10.0);
    let mut state = SafetyState::CLEAR;
    for _ in 0..DEBOUNCE_COUNT {
        state = reduce_temp(state, 146.0, &config).state;
    }
    assert!(state.latched, "three hot readings must latch");

    // Dropping below threshold - hysteresis resets the counter but does NOT
    // clear, because 134 C is still above the 100 C safe temperature.
    let out = reduce_temp(state, 145.0 - 10.0 - 1.0, &config);
    assert_eq!(out.state.high_reading_count, 0);
    assert!(out.state.latched, "134 C is not safe to restart at");

    state = out.state;
    let out = reduce_temp(state, 99.0, &config);
    assert!(!out.state.latched, "99 C is safe, so the latch must clear");
    assert_eq!(out.state.high_reading_count, 0);
    assert!(verdict_of(&out).may_heat, "and the heater may run again");
}

#[test]
fn s3_a_latched_machine_stays_latched_while_still_hot() {
    let config = cfg_with(145.0, 10.0);
    let state = SafetyState {
        last_sample_seq: None,
        latched: true,
        high_reading_count: 3,
    };
    // 120 C: below threshold - hysteresis, so the branch is taken, but above
    // the 100 C safe temperature, so the latch holds.
    let out = reduce_temp(state, 120.0, &config);
    assert!(out.state.latched);
    assert!(!verdict_of(&out).any_permitted());
}

// ===================================================================== S4 —
// Water tank. Ported from test_hardware_water_tank.

#[test]
fn s4_empty_tank_blocks_the_pump() {
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(20.0), false, MachineState::PidNormal),
        &cfg(),
        Millis::ZERO,
    );
    assert!(
        !verdict_of(&out).may_pump,
        "enablePump refuses on an empty tank"
    );
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::WaterTankEmpty)
    ));
}

#[test]
fn s4_empty_tank_does_not_block_the_heater() {
    // The boiler is a separate vessel from the reservoir. Stopping the heater
    // because the water tank is low would leave the user with cold coffee AND a
    // cold machine, and the C++ only gates the heater on
    // `keep_heater_on_empty` in the state machine, not here.
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(20.0), false, MachineState::PidNormal),
        &cfg(),
        Millis::ZERO,
    );
    assert!(verdict_of(&out).may_heat);
}

#[test]
fn div1_s4_empty_tank_blocks_the_water_valve_too() {
    // **Divergence from the C++, see `intentional-diffs.md` #3 / 09 §3.** The C++
    // gates only `enablePump` and `setPumpPressure` on `waterTankEmpty_`
    // (`HardwareManager.cpp:325-328,398-406`); `openWaterValve` checks only
    // `emergencyMode_`. The port refuses the valve as well.
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(20.0), false, MachineState::BrewRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert!(
        !verdict_of(&out).may_open_water,
        "an empty tank closes the valve"
    );
    assert!(!verdict_of(&out).may_pump);
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::WaterTankEmpty)
    ));

    // A full tank in the same state lifts both.
    let refilled = reduce(
        &out.state,
        &Telemetry::new(Celsius::new(20.0), true, MachineState::BrewRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert!(verdict_of(&refilled).may_open_water);
    assert!(verdict_of(&refilled).may_pump);

    // The *heater* is untouched: the boiler is a separate vessel.
    assert!(verdict_of(&out).may_heat);
}

#[test]
fn s4_pump_is_allowed_again_after_a_refill() {
    let empty = Telemetry::new(Celsius::new(20.0), false, MachineState::PidNormal);
    let full = Telemetry::new(Celsius::new(20.0), true, MachineState::PidNormal);
    let state = SafetyState::CLEAR;
    let after_empty = reduce(&state, &empty, &cfg(), Millis::ZERO);
    assert!(!verdict_of(&after_empty).may_pump);

    let after_refill = reduce(&after_empty.state, &full, &cfg(), Millis::ZERO);
    assert!(
        verdict_of(&after_refill).may_pump,
        "refilling re-enables the pump"
    );
    assert_eq!(
        after_refill.state, state,
        "the tank leaves no residue in the latch state"
    );
}

#[test]
fn s4_the_tank_reading_needs_no_edge_to_take_effect() {
    // The C++ kills a running pump inside `setWaterTankEmpty(true)`, so a missed
    // edge leaves the pump running. Here the verdict is level-triggered: two
    // consecutive empty readings behave exactly like one.
    let empty = Telemetry::new(Celsius::new(20.0), false, MachineState::PidNormal);
    let state = SafetyState::CLEAR;
    let first = reduce(&state, &empty, &cfg(), Millis::ZERO);
    let second = reduce(&first.state, &empty, &cfg(), Millis::ZERO);
    assert_eq!(first.verdict, second.verdict);
    assert!(!verdict_of(&second).may_pump);
}

// ===================================================================== S5 —
// The valve whitelist. Ports test_brew_handler and test_backflush_states.

#[test]
fn s5_the_three_brew_flow_states_may_flow_water() {
    for state in [
        MachineState::BrewPreinfusion,
        MachineState::BrewPreinfusionPause,
        MachineState::BrewRunning,
    ] {
        assert!(
            water_flow_allowed(state),
            "{} must be a water-flow state",
            state.name()
        );
    }
}

#[test]
fn s5_brew_finished_may_not_flow_water() {
    // `isBrewState(BREW_FINISHED)` is true, which is exactly why the C++ spells
    // out the exclusion (BrewHandler.h:110).
    assert!(MachineState::BrewFinished.is_brew_state());
    assert!(!water_flow_allowed(MachineState::BrewFinished));
}

#[test]
fn s5_manual_flush_may_flow_water() {
    assert!(water_flow_allowed(MachineState::ManualFlushRunning));
}

#[test]
fn s5_backflush_filling_and_flushing_may_flow_water() {
    assert!(water_flow_allowed(MachineState::BackflushFilling));
    assert!(water_flow_allowed(MachineState::BackflushFlushing));
}

#[test]
fn s5_backflush_idle_and_finished_may_not_flow_water() {
    assert!(!water_flow_allowed(MachineState::BackflushIdle));
    assert!(!water_flow_allowed(MachineState::BackflushFinished));
}

#[test]
fn s5_every_other_state_may_not_flow_water() {
    for state in [
        MachineState::Init,
        MachineState::PidNormal,
        MachineState::SteamRunning,
        MachineState::WaterTankEmpty,
        MachineState::EmergencyStop,
        MachineState::PidDisabled,
        MachineState::Standby,
        MachineState::SensorError,
        MachineState::EepromError,
    ] {
        assert!(
            !water_flow_allowed(state),
            "{} must not flow water",
            state.name()
        );
    }
}

#[test]
fn s5_a_non_water_flow_state_closes_the_water_valve_in_the_verdict() {
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(20.0), true, MachineState::SteamRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert!(
        !verdict_of(&out).may_open_water,
        "steam must not leave the water valve open"
    );
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::NotAWaterFlowState {
            state: MachineState::SteamRunning
        })
    ));
}

#[test]
fn s5_a_water_flow_state_permits_the_water_valve() {
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(20.0), true, MachineState::BrewRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert!(verdict_of(&out).may_open_water);
    // The tank is full and the state is a water-flow state, so the *only* thing
    // left to report is the steam whitelist (S5') — which BREW_RUNNING is
    // deliberately not on. See 09 §2: the steam valve is the same relay.
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::NotASteamState {
            state: MachineState::BrewRunning
        })
    ));
}

// ======================================================= S5' — steam whitelist —
//
// The C++ has NO steam-valve whitelist: `openSteamValve` checks only
// `emergencyMode_` (HardwareManager.cpp:397-400) and there is no
// `steamSafetyShutdownCheck` anywhere in the tree. The port adds one, because
// the steam valve is the *same physical relay* as the water valve
// (ValveState.h:8-11) and the port can reach it.
// See 09 §2 and `intentional-diffs.md` #2.

#[test]
fn div2_the_steam_valve_is_whitelist_gated() {
    // A water-flow state must NOT leave the steam valve permitted. In the C++
    // this verdict says `true`.
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(20.0), true, MachineState::BrewRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert!(
        !verdict_of(&out).may_open_steam,
        "divergence: the C++ does not gate the steam valve on the state; the port does"
    );
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::NotASteamState {
            state: MachineState::BrewRunning
        })
    ));
}

#[test]
fn div2_steam_running_is_the_only_state_that_may_flow_steam() {
    // The whitelist, asserted rather than described: exactly one state.
    let allowed: Vec<MachineState> = cc_domain::state::ALL
        .iter()
        .copied()
        .filter(|s| steam_flow_allowed(*s))
        .collect();
    assert_eq!(
        allowed,
        [MachineState::SteamRunning],
        "the steam whitelist is STEAM_RUNNING and nothing else — \
         SteamStates.cpp:16 is the only setSteamMode(true) in the tree"
    );

    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(120.0), true, MachineState::SteamRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert!(verdict_of(&out).may_open_steam);
    // STEAM_RUNNING is deliberately *not* on the water whitelist (S5), so the
    // water valve is closed while steam flows — the same relay, one permission.
    assert!(!verdict_of(&out).may_open_water);
    assert!(matches!(
        verdict_of(&out).reason,
        Some(Reason::NotAWaterFlowState {
            state: MachineState::SteamRunning
        })
    ));
}

#[test]
fn div2_the_two_whitelists_never_agree_on_a_state() {
    // The property that makes the single-state steam whitelist correct rather
    // than merely narrow: because the steam and water valves are one relay, a
    // state that were on both lists would re-open S5's hole from the other
    // side. There must be no such state.
    for state in cc_domain::state::ALL {
        assert!(
            !(water_flow_allowed(state) && steam_flow_allowed(state)),
            "{} is on both whitelists: the shared relay could be opened for water \
             in a state that the steam whitelist claims for steam",
            state.name()
        );
    }
}

// ======================================================= Configuration rules —

#[test]
fn config_the_compiled_defaults_are_safe() {
    assert_eq!(validate_config(&SafetyConfig::default()), Ok(()));
}

#[test]
fn config_emergency_temp_must_exceed_steam_setpoint_plus_hysteresis() {
    let cfg = SafetyConfig {
        emergency_temp: Celsius::new(140.0),
        steam_setpoint: Celsius::new(135.0),
        emergency_hysteresis: Celsius::new(10.0),
        ..SafetyConfig::default()
    };
    // 140 is not *greater* than 135 + 10, so the machine would trip itself
    // while steaming.
    assert_eq!(
        validate_config(&cfg),
        Err(ConfigViolation::EmergencyTempTooLowForSteam {
            emergency_temp: Celsius::new(140.0),
            steam_setpoint: Celsius::new(135.0),
            emergency_hysteresis: Celsius::new(10.0),
        })
    );
}

#[test]
fn config_equality_at_the_boundary_is_still_a_violation() {
    let cfg = SafetyConfig {
        emergency_temp: Celsius::new(130.0),
        steam_setpoint: Celsius::new(120.0),
        emergency_hysteresis: Celsius::new(10.0),
        ..SafetyConfig::default()
    };
    assert!(
        validate_config(&cfg).is_err(),
        "emergency_temp must *exceed* the sum"
    );
}

#[test]
fn config_one_degree_of_headroom_is_enough() {
    let cfg = SafetyConfig {
        emergency_temp: Celsius::new(130.1),
        steam_setpoint: Celsius::new(120.0),
        emergency_hysteresis: Celsius::new(10.0),
        ..SafetyConfig::default()
    };
    assert_eq!(validate_config(&cfg), Ok(()));
}

#[test]
fn config_the_highest_legal_steam_setpoint_still_validates_with_the_default_threshold() {
    // STEAM_SETPOINT_MAX is 140 (defaults.h:84) and the default emergency
    // threshold is 150, so the shipped defaults are 10 C clear of the worst
    // legal steam setpoint. This is the whole reason the defaults are safe.
    let cfg = SafetyConfig {
        steam_setpoint: Celsius::new(140.0),
        ..SafetyConfig::default()
    };
    assert_eq!(validate_config(&cfg), Ok(()));
}

#[test]
fn config_a_low_trigger_heater_relay_is_refused() {
    let cfg = SafetyConfig {
        heater_relay_trigger: RelayTriggerType::LowTrigger,
        ..SafetyConfig::default()
    };
    assert_eq!(
        validate_config(&cfg),
        Err(ConfigViolation::HeaterRelayLowTrigger),
        "an undriven GPIO at reset would energise the heater"
    );
}

#[test]
fn config_a_high_trigger_heater_relay_is_accepted() {
    let cfg = SafetyConfig {
        heater_relay_trigger: RelayTriggerType::HighTrigger,
        ..SafetyConfig::default()
    };
    assert_eq!(validate_config(&cfg), Ok(()));
}

// ==================== the temperature sensor type (R3-07 reversed R1-03)

// Both sensor types are supported, so there is nothing here to validate. The
// four tests that used to assert a `TSIC_306` refusal are replaced, not deleted:
// the behaviour they pinned was real and worth pinning — a firmware must not
// accept a sensor setting it cannot honour — and the thing that has changed is
// that there is no such setting any more.
//
// What is *not* given up is the underlying complaint. The C++ accepted
// `TSIC_306` and then read the 1-Wire bus anyway, silently substituting one
// probe for another. That is now prevented by construction rather than by
// refusing the configuration: `cc_domain::sensor` gives each sensor type its own
// `as_probe` into one shared `ProbeReading` vocabulary, the firmware selects the
// driver from the board, and a probe that does not answer is reported as
// `ProbeFault::NotConnected` with the configured sensor type in the log line.

#[test]
fn both_temperature_sensor_types_are_accepted() {
    // The reversal, as a test. `Hardware::TemperatureSensorType` has exactly two
    // values and neither is refused.
    for sensor in [
        TemperatureSensorType::Tsic306,
        TemperatureSensorType::DallasDs18b20,
    ] {
        let cfg = SafetyConfig {
            temperature_sensor: sensor,
            ..SafetyConfig::default()
        };
        assert_eq!(validate_config(&cfg), Ok(()), "{sensor:?} must be accepted");
        assert_eq!(
            check_storable(&cfg),
            Ok(&cfg),
            "{sensor:?} must be storable"
        );
    }
}

#[test]
fn the_compiled_in_default_is_the_cpps_tsic_306() {
    // `Config.h:1085-1092`:
    //
    //   EnumParamDef<Hardware::TemperatureSensorType> hardwareSensorsTemperatureType{
    //       "hardware.sensors.temperature.type",
    //       Hardware::TemperatureSensorType::TSIC_306, ...
    //
    // An earlier revision defaulted to `DALLAS_DS18B20` *and* refused
    // `TSIC_306`, on the reasoning that a default the validator rejects is a
    // machine that will not run its own defaults. That reasoning was correct
    // while the driver was missing; the driver exists (R3-07) and the C++'s
    // value is restored.
    let defaults = SafetyConfig::default();
    assert_eq!(defaults.temperature_sensor, TemperatureSensorType::Tsic306);
    assert_eq!(validate_config(&defaults), Ok(()));
    assert_eq!(check_storable(&defaults), Ok(&defaults));
}

#[test]
fn a_stored_tsic_306_config_is_loaded_not_discarded() {
    // The end-to-end consequence of the reversal: a stored `TSIC_306`
    // configuration is now a legitimate configuration, so it loads as
    // `ConfigOrigin::Stored` rather than being thrown away. Getting this wrong in
    // either direction is bad — a machine configured for a TSIC-306 that silently
    // falls back to defaults it was not configured for, or one that refuses to
    // run a setting it is entitled to use.
    let stored = SafetyConfig {
        temperature_sensor: TemperatureSensorType::Tsic306,
        ..SafetyConfig::default()
    };
    let loaded = load_or_default(Some(&stored));
    assert_eq!(loaded.config, stored);
    assert_eq!(loaded.origin, ConfigOrigin::Stored);
}

#[test]
fn a_stored_ds18b20_config_is_loaded_too() {
    // The other half: this machine's own probe, configured explicitly, is also
    // accepted and run as configured.
    let stored = SafetyConfig {
        temperature_sensor: TemperatureSensorType::DallasDs18b20,
        ..SafetyConfig::default()
    };
    let loaded = load_or_default(Some(&stored));
    assert_eq!(loaded.config, stored);
    assert_eq!(loaded.origin, ConfigOrigin::Stored);
}

#[test]
fn the_sensor_type_is_not_what_makes_a_config_unsafe() {
    // The rules that *remain*, restated together so the set is visible: the
    // cross-parameter emergency-temperature check and the `LOW_TRIGGER` heater
    // refusal. The sensor type is not a third one.
    let unsafe_steam = SafetyConfig {
        emergency_temp: Celsius::new(100.0),
        steam_setpoint: Celsius::new(120.0),
        ..SafetyConfig::default()
    };
    assert!(matches!(
        validate_config(&unsafe_steam),
        Err(ConfigViolation::EmergencyTempTooLowForSteam { .. })
    ));

    let unsafe_relay = SafetyConfig {
        heater_relay_trigger: RelayTriggerType::LowTrigger,
        ..SafetyConfig::default()
    };
    assert_eq!(
        validate_config(&unsafe_relay),
        Err(ConfigViolation::HeaterRelayLowTrigger)
    );

    // And the sensor type is genuinely orthogonal: varying it across every value
    // changes nothing about either verdict.
    for sensor in [
        TemperatureSensorType::Tsic306,
        TemperatureSensorType::DallasDs18b20,
    ] {
        let mut with_steam = unsafe_steam;
        with_steam.temperature_sensor = sensor;
        assert!(validate_config(&with_steam).is_err(), "{sensor:?}");
        let mut with_relay = unsafe_relay;
        with_relay.temperature_sensor = sensor;
        assert_eq!(
            validate_config(&with_relay),
            Err(ConfigViolation::HeaterRelayLowTrigger),
            "{sensor:?}"
        );
    }
}

#[test]
fn the_steam_rule_is_still_the_first_check() {
    // Both parameters wrong: the cross-parameter steam rule is reported, so the
    // diagnostic is stable and the ordering of the checks is pinned rather than
    // implied.
    let cfg = SafetyConfig {
        emergency_temp: Celsius::new(100.0),
        heater_relay_trigger: RelayTriggerType::LowTrigger,
        ..SafetyConfig::default()
    };
    assert!(matches!(
        validate_config(&cfg),
        Err(ConfigViolation::EmergencyTempTooLowForSteam { .. })
    ));
}

#[test]
fn config_the_steam_rule_is_checked_before_the_relay_rule() {
    // Both wrong: the first violation is reported. The order is fixed so the
    // diagnostic is stable.
    let cfg = SafetyConfig {
        emergency_temp: Celsius::new(100.0),
        heater_relay_trigger: RelayTriggerType::LowTrigger,
        ..SafetyConfig::default()
    };
    assert!(matches!(
        validate_config(&cfg),
        Err(ConfigViolation::EmergencyTempTooLowForSteam { .. })
    ));
}

// ------------------------------------------------- fail-closed on load/store —

#[test]
fn load_with_nothing_stored_uses_the_defaults() {
    let loaded: LoadedConfig = load_or_default(None);
    assert_eq!(loaded.config, SafetyConfig::default());
    assert_eq!(loaded.origin, ConfigOrigin::Defaults);
}

#[test]
fn load_with_a_valid_stored_config_keeps_it() {
    let stored = SafetyConfig {
        emergency_temp: Celsius::new(160.0),
        ..SafetyConfig::default()
    };
    let loaded = load_or_default(Some(&stored));
    assert_eq!(loaded.config, stored);
    assert_eq!(loaded.origin, ConfigOrigin::Stored);
}

#[test]
fn load_discards_an_unsafe_stored_config_and_falls_back_to_defaults() {
    let stored = SafetyConfig {
        steam_setpoint: Celsius::new(140.0),
        emergency_hysteresis: Celsius::new(15.0),
        ..SafetyConfig::default()
    };
    assert!(
        validate_config(&stored).is_err(),
        "the fixture must actually be unsafe"
    );

    let loaded = load_or_default(Some(&stored));
    assert_eq!(
        loaded.config,
        SafetyConfig::default(),
        "defaults, not a partially repaired configuration"
    );
    assert!(matches!(
        loaded.origin,
        ConfigOrigin::DiscardedUnsafe(ConfigViolation::EmergencyTempTooLowForSteam { .. })
    ));
}

#[test]
fn load_discards_a_low_trigger_heater_relay() {
    let stored = SafetyConfig {
        heater_relay_trigger: RelayTriggerType::LowTrigger,
        ..SafetyConfig::default()
    };
    let loaded = load_or_default(Some(&stored));
    assert_eq!(loaded.config, SafetyConfig::default());
    assert_eq!(
        loaded.origin,
        ConfigOrigin::DiscardedUnsafe(ConfigViolation::HeaterRelayLowTrigger)
    );
}

#[test]
fn store_refuses_an_unsafe_config() {
    let unsafe_cfg = SafetyConfig {
        heater_relay_trigger: RelayTriggerType::LowTrigger,
        ..SafetyConfig::default()
    };
    assert_eq!(
        check_storable(&unsafe_cfg),
        Err(ConfigViolation::HeaterRelayLowTrigger)
    );
    assert_eq!(
        check_storable(&SafetyConfig::default()),
        Ok(&SafetyConfig::default())
    );
}

// ================================================================== purity —

#[test]
fn reduce_is_a_pure_function_of_its_inputs() {
    let config = cfg();
    let telemetry_in = telemetry(60.0);
    let state = SafetyState {
        last_sample_seq: None,
        latched: false,
        high_reading_count: 2,
    };
    let first = reduce(&state, &telemetry_in, &config, Millis::new(1234));
    let second = reduce(&state, &telemetry_in, &config, Millis::new(1234));
    assert_eq!(first, second);
    assert_eq!(
        state.high_reading_count, 2,
        "the input state is not mutated"
    );
}

#[test]
fn a_healthy_brew_permits_everything() {
    // The one state that permits all four actuators does not exist: the water
    // whitelist and the steam whitelist are disjoint by design (the shared
    // relay). What is asserted here is the shape of a healthy, unlatched
    // verdict in a brew — the pump, the water valve and the heater are all
    // permitted, the steam valve is not, and the reason says which rule.
    let out = reduce(
        &SafetyState::CLEAR,
        &Telemetry::new(Celsius::new(94.5), true, MachineState::BrewRunning),
        &cfg(),
        Millis::ZERO,
    );
    assert_eq!(
        verdict_of(&out),
        Verdict {
            may_heat: true,
            may_pump: true,
            may_open_water: true,
            may_open_steam: false,
            latched: false,
            reason: Some(Reason::NotASteamState {
                state: MachineState::BrewRunning
            }),
        }
    );
}

#[test]
fn the_recovery_sequence_walks_the_whole_lifecycle() {
    // cold -> heating -> too hot -> latched -> cooling -> still too hot ->
    // safe -> recovered. One pass over the machine's worst day.
    let config = cfg_with(150.0, 5.0);
    let mut state = SafetyState::CLEAR;

    state = reduce_temp(state, 20.0, &config).state;
    assert!(!state.latched, "cold start is fine");
    state = reduce_temp(state, 100.0, &config).state;
    assert!(!state.latched, "brewing is fine");

    for _ in 0..DEBOUNCE_COUNT {
        let out = reduce_temp(state, 160.0, &config);
        state = out.state;
    }
    assert!(
        state.latched,
        "three readings at 160 C must latch a 150 C threshold"
    );

    let out = reduce_temp(state, 120.0, &config);
    assert!(
        out.state.latched,
        "120 C is below threshold - hysteresis but not safe"
    );
    state = out.state;

    let out = reduce_temp(state, 60.0, &config);
    assert!(!out.state.latched, "60 C is safe to restart");
    assert!(out.verdict.may_heat);
    assert_eq!(out.state, SafetyState::CLEAR);
}

#[test]
fn the_same_sample_delivered_repeatedly_does_not_advance_the_debounce() {
    // **The defect this field exists for.** The control task runs at 100 ms/10 ms
    // and calls `reduce` on every tick with whatever the last conversion produced.
    // The C++ increments the debounce once per *reading*, on a 400 ms cadence,
    // so `DEBOUNCE_COUNT = 3` is about 1.2 s of sustained overheat. Counting
    // invocations instead made the trip time 30 ms — a single probe spike latched
    // an emergency stop in the middle of a brew.
    let config = cfg_with(145.0, 10.0);
    let hot = telemetry(146.0);

    // One reading, looked at forty times, as a 100 Hz loop over a 2.5 Hz probe
    // would deliver it.
    let mut out = Outcome {
        state: SafetyState::CLEAR,
        verdict: Verdict::ALL_REFUSED,
    };
    for _ in 0..40 {
        out = reduce(&out.state, &repeat_same_sample(&hot), &config, Millis::ZERO);
    }
    assert_eq!(
        out.state.high_reading_count, 1,
        "one sample seen forty times is still one sample"
    );
    assert!(
        !out.verdict.latched,
        "a single over-threshold reading must not latch an emergency stop"
    );

    // Three *distinct* samples do latch, which is the C++'s behaviour.
    let mut out = Outcome {
        state: SafetyState::CLEAR,
        verdict: Verdict::ALL_REFUSED,
    };
    for _ in 0..3 {
        out = reduce(&out.state, &telemetry(146.0), &config, Millis::ZERO);
    }
    assert_eq!(out.state.high_reading_count, 3);
    assert!(
        out.verdict.latched,
        "three distinct over-threshold samples latch, as the C++ does"
    );
}

#[test]
fn clearing_the_latch_re_counts_the_sample_that_is_still_over_threshold() {
    // The trap in the other direction: if `clear` forgets which sample the
    // counter was on, the re-trigger that follows would be swallowed.
    let config = cfg_with(145.0, 10.0);
    let hot = telemetry(146.0);
    let out = reduce(&SafetyState::CLEAR, &hot, &config, Millis::ZERO);
    let mut state = out.state;
    state.clear();
    let again = reduce(&state, &repeat_same_sample(&hot), &config, Millis::ZERO);
    assert_eq!(
        again.state.high_reading_count, 1,
        "after a clear, the sample still over threshold counts again"
    );
}

// ============================================================ relay trigger type

/// The hazard is **not** heater-specific, and the port's refusal was.
///
/// `Relay::on()` branches on `triggerType` (`src/hardware/Relay.cpp:13-27`) and
/// `HardwareManager` wires all three parameters into it
/// (`HardwareManager.cpp:73,80,87`), so a stored configuration can carry a
/// low-trigger **pump** or **valve**. Such a relay energises while its pin
/// floats — before any firmware runs — so water would flow at every boot with
/// nothing in the loop to stop it. That is the same argument the recovered
/// oracle used for the heater, and it applies just as much to these two.
#[test]
fn a_low_trigger_relay_is_refused_for_every_relay() {
    for (what, cfg) in [
        (
            "heater",
            SafetyConfig {
                heater_relay_trigger: RelayTriggerType::LowTrigger,
                ..SafetyConfig::default()
            },
        ),
        (
            "pump",
            SafetyConfig {
                pump_relay_trigger: RelayTriggerType::LowTrigger,
                ..SafetyConfig::default()
            },
        ),
        (
            "valve",
            SafetyConfig {
                valve_relay_trigger: RelayTriggerType::LowTrigger,
                ..SafetyConfig::default()
            },
        ),
    ] {
        assert!(
            validate_config(&cfg).is_err(),
            "{what}: a LOW_TRIGGER relay energises while its pin floats, which is \\
             before any firmware runs — it must be refused for all three, not only \\
             the heater"
        );
    }
}

#[test]
fn high_trigger_relays_are_accepted_for_every_relay() {
    // The refusals above must not become a blanket refusal: the default
    // configuration has to boot.
    assert!(validate_config(&SafetyConfig::default()).is_ok());
}
