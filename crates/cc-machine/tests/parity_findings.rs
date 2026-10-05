//! Parity pins for [`09-cpp-findings.md`](../../docs/history/cpp-findings.md)
//! — every finding that touches the state machine, plus the new ones found while
//! porting it.
//!
//! # The rule
//!
//! A C++ bug found during the port is **preserved**, not fixed, and pinned by a
//! test whose name starts `s<N>_`, matching the numbering in doc 09. Fixing one
//! silently would make the port's diff against the C++ non-empty in a way nobody
//! reviewed, and a parity harness that reports "zero unexplained diffs" would
//! then be reporting a lie.
//!
//! # …unless it is closed on purpose
//!
//! A finding that has been **closed** gets a `div<N>_` test instead, naming the
//! behaviour the port now has. A `s<N>_` test that has been replaced must be
//! deleted, not left alongside, or the pair would disagree.
//!
//! The closed ones, all in
//! [`intentional-diffs.md`](../../docs/history/divergences.md):
//!
//! | closed finding | test |
//! | --- | --- |
//! | 09 §11 the pump watchdogs are dead | [`div1_the_pump_timeouts_are_armed`], [`div1_the_watchdogs_arm_only_while_the_pump_is_commanded_on`], [`div1_the_watchdogs_re_arm_after_a_release`] |
//! | 09 §2 the steam valve has no whitelist | [`div2_the_steam_valve_is_whitelist_gated`] |
//! | 09 §3 the water valve is not tank-gated | [`div3_the_water_valve_is_tank_gated`] |
//!
//! 09 §1 (the PID's integer division) is closed too, but in `cc-domain` rather
//! than here: `cc_domain::pid_parity::scenario_d_the_cpp_goes_nan_and_this_port_does_not`.
//!
//! # Index
//!
//! | test | finding | applies? |
//! | --- | --- | --- |
//! | [`div2_the_steam_valve_is_whitelist_gated`] | §2 the steam valve has no safety whitelist | **closed** |
//! | [`div3_the_water_valve_is_tank_gated`] | §3 the water valve is not tank-gated | **closed** |
//! | [`s4_the_emergency_debounce_keeps_the_heater_on`] | §4 S1's debounce keeps heating | yes |
//! | [`s5_the_emergency_threshold_constant_is_dead`] | §5 two dead "145 °C" constants | yes |
//! | [`s6_the_anti_windup_dead_band_can_freeze_the_integrator`] | §6 anti-windup gate | no (PID, R2-04) |
//! | [`s7_the_shipped_pid_gains_look_like_bang_bang`] | §7 gains look like on/off | no (PID, R2-04) |
//! | [`s8_config_validation_is_per_parameter_only`] | §8 no cross-parameter validation | no (`cc-safety`, R2-05) |
//! | [`s1_the_pid_sample_time_is_integer_divided_by_1000`] | §1 integer division by zero | **fixed** in `cc-domain`, R1-07 |
//! | [`div1_the_pump_timeouts_are_armed`] | **new** — the pump watchdogs are dead | **closed** |
//! | [`s12_the_sensor_error_recovery_clock_is_never_reset`] | **new** — `ErrorStates.cpp:49` is unreachable | yes |
//! | [`s13_backflush_filling_never_re_asserts_its_hardware`] | **new** — violates ADR-0003's own rule | yes |
//! | [`s14_the_water_switch_does_not_wake_the_machine_from_standby`] | **new** — `hasUserActivity()` is a stub | yes |
//! | [`s15_the_power_off_happens_before_the_standby_request`] | **new** — a one-tick window | yes |

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{
    reduce, steam_flow_allowed, water_flow_allowed, Command, Effect, Event, PumpWatchdog, Request,
    Sensors, SwitchId,
};
use common::Harness;

// ---------------------------------------------------------------------------
// §2 — the steam valve had no safety whitelist. CLOSED.
// ---------------------------------------------------------------------------

/// **Divergence, see [`intentional-diffs.md`](../../docs/history/divergences.md)
/// #2 and 09 §2.** This replaces `s2_the_steam_valve_is_not_whitelist_gated`,
/// which pinned the C++ behaviour.
///
/// Three things are asserted, and all three matter:
///
/// 1. **The whitelist exists and is `STEAM_RUNNING` and nothing else.**
///    `cc_safety::steam_flow_allowed` is a `match` with no wildcard arm, so
///    adding a 19th state is a compile error until someone classifies it.
/// 2. **The reducer honours it.** Over every state and every event, the steam
///    valve is never *opened* outside `STEAM_RUNNING`, and is *closed* in every
///    other state — the `steamValveSafetyShutdownCheck` this port adds, which is
///    the mirror of S5 and the only reason the gap cannot be reopened by a new
///    state.
/// 3. **The two whitelists are disjoint**, because the steam and water valves
///    are the same physical relay (`ValveState.h:8-11`).
#[test]
fn div2_the_steam_valve_is_whitelist_gated() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = common::context_for(&config);

    for state in cc_domain::state::ALL {
        let mut machine = machine_in(state);
        for ev in all_events() {
            let (next, fx) = reduce(&machine, &ctx, ev);
            if steam_flow_allowed(state) {
                // STEAM_RUNNING: the port leaves the valve alone rather than
                // opening it, because nothing in the C++ ever asked for steam
                // to be drawn (see the module note on `Effect::OpenSteamValve`).
                assert_eq!(
                    common::count(&fx, Effect::OpenSteamValve),
                    0,
                    "{state:?} x {ev:?}: no state may open the steam valve yet"
                );
                assert_eq!(
                    common::count(&fx, Effect::CloseSteamValve),
                    0,
                    "{state:?} x {ev:?} closed the steam valve inside the whitelist"
                );
            } else {
                assert_eq!(
                    common::count(&fx, Effect::OpenSteamValve),
                    0,
                    "{state:?} x {ev:?} opened the steam valve outside the whitelist"
                );
            }
            machine = next;
        }
    }

    // The fail-safe close: every tick in a non-steam state asserts the valve
    // closed, exactly as S5 does for the water valve.
    for state in cc_domain::state::ALL {
        if steam_flow_allowed(state) {
            continue;
        }
        let mut h = Harness::in_state(state);
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::CloseSteamValve),
            "{state:?} must assert the steam valve closed: {fx:?}"
        );
    }

    // The whitelist itself: one state, and it is the one that means "steam".
    let steam: Vec<MachineState> = cc_domain::state::ALL
        .iter()
        .copied()
        .filter(|s| steam_flow_allowed(*s))
        .collect();
    assert_eq!(steam, [MachineState::SteamRunning]);

    // The two lists are disjoint, because the two "valves" are one relay.
    let water: Vec<MachineState> = cc_domain::state::ALL
        .iter()
        .copied()
        .filter(|s| water_flow_allowed(*s))
        .collect();
    assert_eq!(water.len(), 6, "S5's whitelist is unchanged by this fix");
    assert!(
        water.iter().all(|s| !steam_flow_allowed(*s)),
        "no state may be on both whitelists: the shared relay would be openable \
         for water in a state the steam list claims"
    );
}

// ---------------------------------------------------------------------------
// §3 — the water valve was not gated on an empty tank. CLOSED.
// ---------------------------------------------------------------------------

/// **Divergence, see [`intentional-diffs.md`](../../docs/history/divergences.md)
/// #3 and 09 §3.** This replaces `s3_the_water_valve_is_not_gated_on_an_empty_tank`,
/// which pinned the C++ behaviour.
///
/// The C++ checks `waterTankEmpty_` in `enablePump` and `setPumpPressure`
/// (`HardwareManager.cpp:325-328,398-406`) and **not** in `openWaterValve`, so an
/// empty tank in a water-flow state still permits the valve. The port refuses
/// it.
///
/// The heater is deliberately *not* gated on the tank: the boiler is a separate
/// vessel, and `hardware.sensors.watertank.keep_heater_on_empty` is a real
/// configuration the machine must honour.
#[test]
fn div3_the_water_valve_is_tank_gated() {
    // `cc-safety`'s verdict now refuses the pump *and* the valve.
    let verdict = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(Celsius::new(25.0), false, MachineState::BrewRunning),
        &cc_safety::SafetyConfig::default(),
        Millis::new(0),
    );
    assert!(!verdict.verdict.may_pump, "S4 refuses the pump");
    assert!(
        !verdict.verdict.may_open_water,
        "divergence: S4 also refuses the water valve — see 09 §3"
    );
    assert!(verdict.verdict.may_heat, "the boiler is a separate vessel");

    // The S5 whitelist still governs it independently: a full tank in a
    // non-water-flow state is still refused.
    let not_a_water_state = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(Celsius::new(25.0), true, MachineState::Standby),
        &cc_safety::SafetyConfig::default(),
        Millis::new(0),
    );
    assert!(!not_a_water_state.verdict.may_open_water);
    assert!(not_a_water_state.verdict.may_pump);
    assert!(matches!(
        not_a_water_state.verdict.reason,
        Some(cc_safety::Reason::NotAWaterFlowState {
            state: MachineState::Standby
        })
    ));

    // And a refilled tank in a brew state permits it again, so the gate is not
    // a one-way door.
    let refilled = cc_safety::reduce(
        &verdict.state,
        &cc_safety::Telemetry::new(Celsius::new(25.0), true, MachineState::BrewRunning),
        &cc_safety::SafetyConfig::default(),
        Millis::new(0),
    );
    assert!(refilled.verdict.may_open_water);
    assert!(refilled.verdict.may_pump);
}

// ---------------------------------------------------------------------------
// §4 — the emergency debounce keeps the heater on
// ---------------------------------------------------------------------------

/// **Preserved deliberately, see 09 §4.** S1 needs three consecutive readings
/// above `safety.emergency_temp`, so at the production 400 ms sensor interval the
/// heater stays energised for up to ~800 ms while already above the emergency
/// temperature.
///
/// The port keeps the debounce exactly: `cc_safety::DEBOUNCE_COUNT == 3`, and the
/// latch does not set on the first or second above-threshold reading. The
/// recommendation in doc 09 (trip immediately on a reading above the threshold,
/// use the debounce only for recovery) is **not** implemented here — that would be
/// a deliberate divergence for R4-09's `intentional-diffs.md`.
///
/// The two readings that *do* trip immediately are the implausible ones, and that
/// is preserved too: `EmergencyStopManager.cpp:25-33` has no debounce for a
/// reading outside `[0, 200]`.
#[test]
fn s4_the_emergency_debounce_keeps_the_heater_on() {
    let cfg = cc_safety::SafetyConfig::default();
    // Each iteration is a **distinct sample**, which is what the C++'s debounce
    // counts and what `Telemetry::sample_seq` exists to express. A test that
    // re-used one telemetry value would now model one reading seen repeatedly —
    // the defect fixed on 2026-10-01, where the control task's 100 Hz calls
    // against a 2.5 Hz probe latched an emergency stop in 30 ms.
    let mut seq = 0_u32;
    let mut telemetry = || {
        seq += 1;
        cc_safety::Telemetry {
            sample_seq: seq,
            ..cc_safety::Telemetry::new(Celsius::new(151.0), true, MachineState::PidNormal)
        }
    };

    let mut safety = cc_safety::SafetyState::CLEAR;
    for reading in 1..cc_safety::DEBOUNCE_COUNT {
        let outcome = cc_safety::reduce(&safety, &telemetry(), &cfg, Millis::new(0));
        assert!(
            !outcome.verdict.latched,
            "preserved: reading {reading} of {} does not latch — see 09 §4",
            cc_safety::DEBOUNCE_COUNT
        );
        assert!(
            outcome.verdict.may_heat,
            "preserved: the heater is still permitted on reading {reading} — \
             this is the exposure doc 09 §4 describes"
        );
        safety = outcome.state;
    }

    let outcome = cc_safety::reduce(&safety, &telemetry(), &cfg, Millis::new(0));
    assert!(outcome.verdict.latched, "the third reading latches");
    assert!(!outcome.verdict.may_heat);

    // An implausible reading trips with no debounce at all.
    let implausible = cc_safety::Telemetry::new(Celsius::new(222.0), true, MachineState::PidNormal);
    let outcome = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &implausible,
        &cfg,
        Millis::new(0),
    );
    assert!(
        outcome.verdict.latched,
        "an implausible reading latches at once"
    );
    assert!(matches!(
        outcome.verdict.reason,
        Some(cc_safety::Reason::InvalidReading { .. })
    ));
}

// ---------------------------------------------------------------------------
// §5 — the two dead "145 °C" constants
// ---------------------------------------------------------------------------

/// **Preserved deliberately, see 09 §5.** `Temperature::EMERGENCY_THRESHOLD_C =
/// 145.0` and `EMERGENCY_RESET_THRESHOLD_C = 120.0`
/// (`constants/Temperature.h:6-7`) are read **nowhere**. The live value is
/// `config_.emergencyStopTemp`, default 150.0 (`Config.h:813`).
///
/// The port's threshold is 150.0, matching the running firmware, not the dead
/// constant. 100.0 is `EMERGENCY_SAFE_TEMP_C` (`Temperature.h:11`), which *is*
/// live and *is* the recovery threshold.
#[test]
fn s5_the_emergency_threshold_constant_is_dead() {
    assert_eq!(
        cc_safety::SafetyConfig::default().emergency_temp,
        Celsius::new(150.0),
        "the live default is 150 °C, not the dead 145 °C constant (09 §5)"
    );
    assert_eq!(
        cc_safety::EMERGENCY_SAFE_TEMP_C,
        Celsius::new(100.0),
        "the recovery threshold is the live 100 °C constant"
    );

    // A 149 °C reading is above the dead 145 °C constant and below the live
    // threshold, and must not latch.
    let cfg = cc_safety::SafetyConfig::default();
    let mut safety = cc_safety::SafetyState::CLEAR;
    let telemetry = cc_safety::Telemetry::new(Celsius::new(149.0), true, MachineState::PidNormal);
    for _ in 0..10 {
        let outcome = cc_safety::reduce(&safety, &telemetry, &cfg, Millis::new(0));
        assert!(!outcome.verdict.latched, "149 °C must not latch at 150 °C");
        safety = outcome.state;
    }
}

// ---------------------------------------------------------------------------
// §11 (new) — the pump watchdogs were never armed. CLOSED.
// ---------------------------------------------------------------------------

/// **Divergence, see [`intentional-diffs.md`](../../docs/history/divergences.md)
/// #1 and 09 §11.** This replaces `s11_the_pump_timeouts_are_never_armed`, which
/// pinned the C++ behaviour.
///
/// In the C++ both watchdogs are dead: `PumpTimer` initialises
/// `isRunning_ = false` (`PumpTimer.h:14`) and **nothing calls `start()`**, so
/// `isExpired()` returns `false` unconditionally and the two `logError` lines
/// are unreachable. Hold the water switch forever and the pump runs forever.
///
/// Here both are **armed on the activating edge** and a trip is **announced**
/// before it acts. The C++'s constants and its two actions are unchanged.
#[test]
fn div1_the_pump_timeouts_are_armed() {
    // The constants survive, with the C++'s values.
    assert_eq!(cc_machine::timing::BREW_PUMP_TIMEOUT_MS, 300_000);
    assert_eq!(cc_machine::timing::HOT_WATER_PUMP_TIMEOUT_MS, 60_000);

    // ---- the brew watchdog -------------------------------------------------
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.brew.mode = cc_domain::process::BrewMode::Manual;

    // The activating edge: `BrewRunningState::update` pushes `EnablePump`
    // (`BrewStates.cpp:245`), and that is where the clock starts.
    let fx = h.elapse(10);
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
    assert_eq!(
        h.machine.brew_pump_started_at,
        Some(Millis::new(10)),
        "the watchdog arms on the pump-on edge"
    );

    // Hold the brew open, one second at a time, and find the trip.
    let deadline = cc_machine::timing::BREW_PUMP_TIMEOUT_MS;
    let mut elapsed = 10;
    let mut trip = None;
    while elapsed < deadline + 5_000 {
        elapsed += 1_000;
        let fx = h.elapse(1_000);
        if common::has(
            &fx,
            Effect::PumpTimeoutFired {
                watchdog: PumpWatchdog::Brew,
            },
        ) {
            trip = Some((elapsed, fx));
            break;
        }
    }
    let (at, fx) = trip.expect("the brew watchdog never fired");
    assert!(at > deadline, "it fired after the deadline, at {at} ms");
    assert_eq!(
        at,
        deadline + 10 + 1_000,
        "`isExpired` is a strict `>`, so the first tick strictly past the deadline"
    );

    // Announced before it acts. `pump_timeouts` runs after `states::update`
    // (`LoopManager` step 4-tail), so the pump and valve writes for this tick
    // precede it; what matters is that the trip is *visible*, and that it is
    // emitted at all rather than being a silent hardware change.
    assert!(
        common::has(
            &fx,
            Effect::PumpTimeoutFired {
                watchdog: PumpWatchdog::Brew
            }
        ),
        "{fx:?}"
    );
    assert_eq!(
        PumpWatchdog::Brew.message(),
        "Pump timeout - stopping for safety",
        "BrewHandler.h:256, verbatim"
    );
    assert!(!fx.contains(&Effect::DisablePump), "{fx:?}");

    // And the C++'s action, unchanged: a brew-stop **request**, consumed by the
    // *next* `checkTransitions` (`BrewHandler.h:259`).
    assert!(h.requested(Request::BrewStop), "{fx:?}");
    assert_eq!(
        h.state(),
        MachineState::BrewRunning,
        "the request must not transition in the same tick"
    );
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewFinished, "{fx:?}");

    // Leaving the pump-on state disarms, so a second brew gets the full five
    // minutes rather than inheriting the first one's elapsed time.
    assert_eq!(h.machine.brew_pump_started_at, None);

    // ---- the hot-water watchdog -------------------------------------------
    let mut g = Harness::in_state(MachineState::PidNormal);
    let _ = g.press(SwitchId::HotWater);
    let fx = g.elapse(10);
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
    assert_eq!(
        g.machine.hot_water_pump_started_at,
        Some(Millis::new(10)),
        "the hot-water watchdog arms when the water switch is held in PID_NORMAL"
    );

    let deadline = cc_machine::timing::HOT_WATER_PUMP_TIMEOUT_MS;
    let mut elapsed = 10;
    let mut trip = None;
    while elapsed < deadline + 5_000 {
        elapsed += 1_000;
        let fx = g.elapse(1_000);
        if common::has(
            &fx,
            Effect::PumpTimeoutFired {
                watchdog: PumpWatchdog::HotWater,
            },
        ) {
            trip = Some((elapsed, fx));
            break;
        }
    }
    let (at, fx) = trip.expect("the hot-water watchdog never fired");
    assert!(at > deadline, "it fired after the deadline, at {at} ms");

    // `HotWaterHandler.h:116-120`: log, then `context.disablePump()`. The
    // tick's own `EnablePump` (from `PidNormalState::update`, which sees the
    // switch still held) precedes both — which is the point of asserting the
    // *order of the two watchdog effects*, not their absolute index.
    let announced = common::index_of(
        &fx,
        Effect::PumpTimeoutFired {
            watchdog: PumpWatchdog::HotWater,
        },
    );
    let stopped = common::index_of(&fx, Effect::DisablePump);
    assert_eq!(announced, Some(1), "{fx:?}");
    assert_eq!(stopped, Some(2), "log first, then stop the pump: {fx:?}");
    assert_eq!(
        PumpWatchdog::HotWater.message(),
        "Hot water pump timeout - stopping for safety",
        "HotWaterHandler.h:117, verbatim (and recovered from the previous Rust \
         firmware, 08 §4.2)"
    );
}

/// The arming rule is not "any brew state" and not "any state with the switch
/// held". Asserted against `arm_pump_watchdogs` **directly** rather than through
/// a tick, because a tick also *transitions*: `BREW_PREINFUSION_PAUSE` moves to
/// `BREW_RUNNING` inside the same tick (the pause is time-based) and would
/// therefore be armed for the wrong reason.
#[test]
fn div1_the_watchdogs_arm_only_while_the_pump_is_commanded_on() {
    for (state, hot_water, brew_armed, hot_water_armed) in [
        // The two brew states whose `update()` pushes `EnablePump`
        // (`BrewStates.cpp:70,245`).
        (MachineState::BrewPreinfusion, false, true, false),
        (MachineState::BrewRunning, false, true, false),
        // A pause is a brew state the C++'s own `isBrewActive()` accepts, but
        // its `update()` pushes `DisablePump` (`BrewStates.cpp:172`), so a pause
        // is not pump run time and must not run the clock.
        (MachineState::BrewPreinfusionPause, false, false, false),
        (MachineState::BrewFinished, false, false, false),
        // Not covered by either C++ timer, and deliberately not extended here.
        (MachineState::ManualFlushRunning, false, false, false),
        (MachineState::BackflushFilling, false, false, false),
        // The water switch means hot water only in PID_NORMAL.
        (MachineState::PidNormal, true, false, true),
        (MachineState::PidNormal, false, false, false),
        // In STEAM_RUNNING the same switch means water *injection*, not hot
        // water (`SteamStates.cpp:36-46`), so it must not start that clock.
        (MachineState::SteamRunning, true, false, false),
    ] {
        let (brew, hot) = arming_result(state, hot_water);
        assert_eq!(
            brew.is_some(),
            brew_armed,
            "{state:?} (water switch {hot_water}): brew watchdog arming"
        );
        assert_eq!(
            hot.is_some(),
            hot_water_armed,
            "{state:?} (water switch {hot_water}): hot-water watchdog arming"
        );
    }
}

/// One call of `arm_pump_watchdogs` on a machine in `state` at t = 10 ms.
fn arming_result(state: MachineState, hot_water: bool) -> (Option<Millis>, Option<Millis>) {
    let mut h = Harness::in_state(state);
    h.machine.switches.hot_water = hot_water;
    h.advance_clock(10);
    cc_machine::handlers::arm_pump_watchdogs(&mut h.machine);
    (
        h.machine.brew_pump_started_at,
        h.machine.hot_water_pump_started_at,
    )
}

#[test]
fn div1_the_watchdogs_re_arm_after_a_release() {
    // Releasing the water switch stops the clock, so a later hold gets the full
    // sixty seconds rather than inheriting the first one's elapsed time.
    let mut r = Harness::in_state(MachineState::PidNormal);
    let _ = r.press(SwitchId::HotWater);
    let _ = r.elapse(10);
    assert!(r.machine.hot_water_pump_started_at.is_some());
    let _ = r.release(SwitchId::HotWater);
    let _ = r.elapse(10);
    assert_eq!(
        r.machine.hot_water_pump_started_at, None,
        "releasing the switch stops the clock"
    );
    let _ = r.press(SwitchId::HotWater);
    let _ = r.elapse(10);
    assert_eq!(
        r.machine.hot_water_pump_started_at,
        Some(Millis::new(30)),
        "and the next hold starts a fresh sixty seconds"
    );
}

// ---------------------------------------------------------------------------
// §12 (new) — the sensor-error recovery clock is never reset
// ---------------------------------------------------------------------------

/// **Preserved deliberately; new finding.**
///
/// `SensorErrorState::checkSpecificTransitions` says it will "keep resetting the
/// clock so recovery delay is measured from when the error actually clears"
/// (`ErrorStates.cpp:47-50`). It does not, and cannot:
///
/// `BaseState::checkTransitions` tests `hasSensorError()` with **no exclusion**
/// (`BaseState.h:145-148`), so while the probe is faulted the guard returns
/// `SENSOR_ERROR` — a discarded self-transition (`StateMachine.cpp:107-111`) —
/// and `checkSpecificTransitions` is never called. The `errorStartTime_ =
/// millis()` line is unreachable.
///
/// The delay is therefore measured from **entry**. A probe fault that persists
/// for an hour and then clears does not wait five seconds; the machine recovers
/// on the next loop.
///
/// The recovery is still safe — it only happens once the error is genuinely
/// clear, and the guard re-checks every loop — but it is not what the comment
/// says, and `test_sensor_error_state` cannot see it because its stubs always
/// report the error clear.
#[test]
fn s12_the_sensor_error_recovery_clock_is_never_reset() {
    let mut h = Harness::in_state(MachineState::SensorError);
    h.config.pid.enabled = true;
    h.config.hardware.sensors.watertank.enabled = true;
    h.now = 0;
    h.machine.now = Millis::new(0);
    h.machine.pid.runtime_enabled = true;
    let _ = h.on_entry(MachineState::SensorError);
    let entry = h.machine.error_since;
    assert!(entry.is_some());

    // Ten minutes with the probe still faulted. The clock must not move.
    for minute in 1..=10_u32 {
        let _ = h.send(Event::SensorUpdated(Sensors {
            has_temperature_error: true,
            ..Sensors::healthy()
        }));
        let _ = h.elapse_to(minute * 60_000);
        assert_eq!(h.state(), MachineState::SensorError, "minute {minute}");
        assert_eq!(
            h.machine.error_since, entry,
            "preserved: ErrorStates.cpp:49 never runs, so the clock never moves"
        );
    }

    // The error clears: recovery is immediate, not five seconds later.
    let _ = h.send(Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(10 * 60_000);
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "preserved: the delay is measured from entry, not from the clear"
    );
}

// ---------------------------------------------------------------------------
// §13 (new) — BACKFLUSH_FILLING never re-asserts its hardware
// ---------------------------------------------------------------------------

/// **Preserved deliberately; new finding, and the most serious one here.
///
/// ADR-0003's contract is three lines per energising state: enable in
/// `onEntryImpl`, **reinforce in `update`**, disable in `onExitImpl`. The contract
/// is stated as a bug fix in ADR-0003's context ("`valveSafetyShutdownCheck()`
/// only excluded brew states…"), and `test_backflush_states` is the suite the
/// coverage map assigns to S5 for the backflush.
///
/// `BackflushFillingState::update` (`BackflushStates.cpp:71-76`) only logs. It
/// never calls `enablePump()` or `openWaterValve()`.
///
/// Why it is a gap and not merely an omission: `BACKFLUSH_FILLING` **is** on the
/// S5 whitelist, so `valveSafetyShutdownCheck` does *not* close its valve, and the
/// pump is only ever blocked by the tank interlock. So the fill phase's pump and
/// valve are set once on entry and then left to the hardware manager for an
/// arbitrary number of loops, with no layer re-asserting them. `BREW_PREINFUSION`,
/// `BREW_PREINFUSION_PAUSE`, `BREW_RUNNING` and `MANUAL_FLUSH_RUNNING` all
/// re-assert every loop.
#[test]
fn s13_backflush_filling_never_re_asserts_its_hardware() {
    let mut h = Harness::in_state(MachineState::BackflushFilling);
    h.config.backflush.cycles = 5;
    h.config.backflush.fill_time = 5.0;
    h.machine.backflush.on = true;
    h.config.brew.pid_delay = 0.0;

    // Entry energises, as ADR-0003 requires.
    let entry = h.on_entry(MachineState::BackflushFilling);
    assert!(common::has(&entry, Effect::EnablePump), "{entry:?}");
    assert!(common::has(&entry, Effect::OpenWaterValve), "{entry:?}");

    // `update` does not. This is the finding.
    let update = h.update(MachineState::BackflushFilling);
    assert_eq!(
        common::count(&update, Effect::EnablePump),
        0,
        "preserved: BackflushFillingState::update does not re-assert the pump \
         (BackflushStates.cpp:71-76) — see 09 §13"
    );
    assert_eq!(
        common::count(&update, Effect::OpenWaterValve),
        0,
        "{update:?}"
    );

    // And a whole tick, with the S5 whitelist satisfied, therefore produces no
    // pump or valve effect at all.
    let fx = h.elapse(1_000);
    assert_eq!(
        common::count(&fx, Effect::EnablePump),
        0,
        "and a full tick does not either: {fx:?}"
    );
    assert_eq!(common::count(&fx, Effect::CloseWaterValve), 0, "{fx:?}");

    // The contrast, so the test cannot pass by accident: the four other
    // energising states DO re-assert.
    for state in [
        MachineState::BrewPreinfusion,
        MachineState::BrewPreinfusionPause,
        MachineState::BrewRunning,
        MachineState::ManualFlushRunning,
    ] {
        let mut h2 = Harness::in_state(state);
        h2.config.brew.mode = cc_domain::process::BrewMode::Automatic;
        h2.config.brew.pre_infusion.enabled = true;
        h2.config.brew.pre_infusion.pause = 600.0;
        h2.config.brew.pid_delay = 0.0;
        let fx = h2.update(state);
        assert!(
            common::has(&fx, Effect::OpenWaterValve),
            "{state:?} must re-assert its valve: {fx:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// §14 (new) — the water switch does not wake the machine from standby
// ---------------------------------------------------------------------------

/// **Preserved deliberately; new finding.**
///
/// `StandbyState::checkSpecificTransitions` exits standby on
/// `isBrewStartRequested() || isSteamStartRequested() || hasUserActivity() ||
/// shouldExitStandby()` (`SystemStates.cpp:50-51`). The last two are **stubs that
/// return false**:
///
/// ```cpp
/// bool MachineStateContext::hasUserActivity() const {
///     // This is a simplified implementation...
///     return false; // TODO: Implement proper user activity detection
/// }
/// ```
/// (`MachineStateContext.cpp:419-423`)
///
/// So only the brew and steam switches can wake the machine. The water switch
/// cannot — it resets the standby countdown
/// (`MachineStateContext.cpp:262-267`) and does nothing else. A user who walks up
/// to a sleeping machine, holds the water switch for a cup of hot water, and lets
/// go gets a machine that is still asleep and whose standby timer has just been
/// pushed out.
#[test]
fn s14_the_water_switch_does_not_wake_the_machine_from_standby() {
    let mut h = Harness::in_state(MachineState::Standby);
    h.config.pid.enabled = true;
    h.config.standby.enabled = true;
    h.config.standby.time = 35.0;
    h.machine.pid.runtime_enabled = false;
    h.machine.standby.started_at = Some(Millis::new(0));
    h.machine.standby.remaining_ms = 1_000;

    let fx = h.press(SwitchId::HotWater);
    assert!(h.machine.hot_water_activity, "{fx:?}");
    assert!(common::has(&fx, Effect::ResetStandbyTimer), "{fx:?}");

    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "preserved: the water switch does not wake the machine \
         (MachineStateContext.cpp:419-423) — see 09 §14"
    );
    assert_eq!(common::count(&fx, Effect::EnablePump), 0, "{fx:?}");

    // The brew and steam switches, by contrast, do wake it.
    for switch in [SwitchId::Brew, SwitchId::Steam] {
        let mut h2 = Harness::in_state(MachineState::Standby);
        h2.config.pid.enabled = true;
        h2.machine.pid.runtime_enabled = false;
        let _ = h2.press(switch);
        let fx = h2.tick();
        assert_eq!(h2.state(), MachineState::PidNormal, "{switch:?}: {fx:?}");
    }
}

// ---------------------------------------------------------------------------
// §15 (new) — the power-off shutdown precedes the standby request
// ---------------------------------------------------------------------------

/// **Preserved deliberately; new finding.**
///
/// `PowerHandler::powerOff` (`PowerHandler.h:163-175`):
///
/// ```cpp
/// if (state != STANDBY) {
///     processController->performSafeShutdown();
///     context->setStandbyRequested(true);
///     standbyCoordinator().setRemainingTimeMillis(0);
/// }
/// ```
///
/// The hardware is shut down, and *then* the standby request is set — but the
/// request is not consumed until the next `checkTransitions`, i.e. the next loop.
/// For that one loop the machine is in `PID_NORMAL` with the hardware already
/// off, and `PidNormalState::update` will `enablePump()` if the water switch is
/// held (`PidStates.cpp:36-38`).
///
/// So powering off while dispensing hot water re-enables the pump for one loop
/// after the "safe shutdown". Narrow — one control-loop iteration, sub-millisecond
/// in practice — and real.
#[test]
fn s15_the_power_off_happens_before_the_standby_request() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = cc_domain::hardware::SwitchType::Momentary;
    let _ = h.tick();
    h.advance_clock(cc_machine::timing::POWER_SWITCH_SETTLE_MS + 1_000);
    let _ = h.press(SwitchId::HotWater);

    let fx = h.press(SwitchId::Power);
    assert!(common::has(&fx, Effect::SafeHardwareShutdown), "{fx:?}");
    assert!(
        h.requested(Request::Standby),
        "the request is set immediately"
    );
    assert_eq!(h.state(), MachineState::PidNormal, "but not yet consumed");

    // The window: in PID_NORMAL, with the hardware already shut down, the
    // update re-enables the pump.
    let fx = h.tick();
    assert!(
        common::has(&fx, Effect::EnablePump),
        "preserved: PID_NORMAL's update re-enables the pump in the one loop \
         between the shutdown and the transition — see 09 §15: {fx:?}"
    );
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "and the machine then sleeps: {fx:?}"
    );
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// A machine in `state`, booted with the PID runtime on.
fn machine_in(state: MachineState) -> cc_machine::Machine {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = common::context_for(&config);
    cc_machine::boot_in(state, true, Millis::new(0), &ctx).0
}

/// A representative event sample for the "nothing ever opens the steam valve"
/// sweep.
fn all_events() -> Vec<Event> {
    let mut events = vec![
        Event::SensorUpdated(Sensors::healthy()),
        Event::PidOutput(500.0),
        Event::Tick {
            now: Millis::new(0),
        },
        Event::Tick {
            now: Millis::new(1_000_000),
        },
    ];
    for switch in SwitchId::ALL {
        events.push(Event::ButtonPressed {
            switch,
            long_press: false,
        });
        events.push(Event::ButtonReleased { switch });
    }
    for command in [
        Command::BrewStart,
        Command::SteamStart,
        Command::ManualFlushStart,
        Command::BackflushEnter,
        Command::Standby,
        Command::NormalOperation,
        Command::SetUserPidEnabled(true),
        Command::Reboot,
    ] {
        events.push(Event::Command(command));
    }
    events
}

/// **09 §25, closed:** a request to sleep is honoured with the PID disabled.
///
/// `isStandbyRequested()` is checked by exactly two states in the C++
/// (`PidNormalState`, `PidStates.cpp:85`, and `EepromErrorState`,
/// `ErrorStates.cpp:78`). `PidDisabledState::checkSpecificTransitions`
/// (`PidStates.cpp:135-148`) checks only the standby **timer**, so in the C++ a
/// request to sleep is silently dropped whenever the PID happens to be off.
///
/// Found on hardware by the human driving the web UI: `POST /api/sleep`
/// answered `202 {"accepted":true}`, `control: command Sleep` appeared in the
/// log — so the request reached the machine — and the state never left
/// `PID_DISABLED`. Since `pid.enabled` defaults to `false`, that is the
/// out-of-the-box configuration, and the machine could not be put to sleep
/// through its own web interface at all.
///
/// Closed 2026-09-30 by the human's decision: `PidDisabled` honours the request,
/// which is what its sibling state one line away already does.
#[test]
fn div13_a_sleep_request_is_honoured_with_the_pid_disabled() {
    let mut h = Harness::in_state(MachineState::PidDisabled);
    // `in_state` sets the *state*; the runtime flag is separate, and leaving it
    // true would let `PidDisabled`'s first arm hand straight back to
    // `PidNormal` before the standby check is ever reached.
    h.machine.pid.runtime_enabled = false;
    assert_eq!(h.state(), MachineState::PidDisabled, "the precondition");
    assert!(!h.machine.pid.runtime_enabled, "the PID is off");

    // The command sets the request; the **next tick** is where the state
    // machine checks it — which is the C++'s arrangement too (a request is
    // polled by `checkSpecificTransitions`, never acted on where it is set).
    h.send(Event::Command(Command::Standby));
    h.tick();

    assert_eq!(
        h.state(),
        MachineState::Standby,
        "divergence: a sleep request must be honoured with the PID off — \
         the C++ drops it here (09 §25)"
    );
}

/// The request is **consumed**, so a later tick does not re-enter standby.
///
/// `MachineStateContext::setStandbyRequested(false)` — every C++ handler that
/// reads a request also clears it, and that is what stops a stale flag from
/// driving the machine somewhere it was never asked to go.
#[test]
fn div13_the_sleep_request_is_consumed_and_not_repeated() {
    let mut h = Harness::in_state(MachineState::PidDisabled);
    h.machine.pid.runtime_enabled = false;
    h.send(Event::Command(Command::Standby));
    h.tick();
    assert_eq!(h.state(), MachineState::Standby);
    assert!(
        !h.requested(Request::Standby),
        "the request must be cleared on the transition that consumed it, or a \\
         later tick re-enters standby with nothing having asked it to"
    );
}

/// Waking from standby still works, and does not need the PID.
///
/// The other half: closing §25 must not make standby a one-way door.
#[test]
fn div13_waking_from_standby_still_works_with_the_pid_disabled() {
    let mut h = Harness::in_state(MachineState::PidDisabled);
    h.machine.pid.runtime_enabled = false;
    h.send(Event::Command(Command::Standby));
    h.tick();
    assert_eq!(h.state(), MachineState::Standby);

    h.send(Event::Command(Command::NormalOperation));
    h.tick();

    assert_ne!(
        h.state(),
        MachineState::Standby,
        "divergence: a wake request must be honoured from standby (09 §25)"
    );
}
