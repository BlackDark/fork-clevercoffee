//! Parity pins for [`09-cpp-findings.md`](../../docs/rust-migration/09-cpp-findings.md)
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
//! Closing a finding is a **deliberate divergence**, recorded in
//! `intentional-diffs.md` by R1-08/R4-09, with its own test. None of these are
//! closed here.
//!
//! # Index
//!
//! | test | finding | applies? |
//! | --- | --- | --- |
//! | [`s2_the_steam_valve_is_not_whitelist_gated`] | §2 the steam valve has no safety whitelist | **yes** |
//! | [`s3_the_water_valve_is_not_gated_on_an_empty_tank`] | §3 the water valve is not tank-gated | **yes** |
//! | [`s4_the_emergency_debounce_keeps_the_heater_on`] | §4 S1's debounce keeps heating | **yes** |
//! | [`s5_the_emergency_threshold_constant_is_dead`] | §5 two dead "145 °C" constants | **yes** |
//! | [`s6_the_anti_windup_dead_band_can_freeze_the_integrator`] | §6 anti-windup gate | no (PID, R2-04) |
//! | [`s7_the_shipped_pid_gains_look_like_bang_bang`] | §7 gains look like on/off | no (PID, R2-04) |
//! | [`s8_config_validation_is_per_parameter_only`] | §8 no cross-parameter validation | no (`cc-safety`, R2-05) |
//! | [`s1_the_pid_sample_time_is_integer_divided_by_1000`] | §1 integer division by zero | no (PID, R2-04) |
//! | [`s11_the_pump_timeouts_are_never_armed`] | **new** — the pump watchdogs are dead | **yes** |
//! | [`s12_the_sensor_error_recovery_clock_is_never_reset`] | **new** — `ErrorStates.cpp:49` is unreachable | **yes** |
//! | [`s13_backflush_filling_never_re_asserts_its_hardware`] | **new** — violates ADR-0003's own rule | **yes** |
//! | [`s14_the_water_switch_does_not_wake_the_machine_from_standby`] | **new** — `hasUserActivity()` is a stub | **yes** |
//! | [`s15_the_power_off_happens_before_the_standby_request`] | **new** — a one-tick window | **yes** |

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{reduce, water_flow_allowed, Command, Effect, Event, Request, Sensors, SwitchId};
use common::Harness;

// ---------------------------------------------------------------------------
// §2 — the steam valve has no safety whitelist
// ---------------------------------------------------------------------------

/// **Preserved deliberately, see 09 §2.** The water valve is gated by
/// `cc_safety::water_flow_allowed`; the steam valve is gated by nothing.
///
/// `HardwareManager::openSteamValve` checks only `emergencyMode_`
/// (`HardwareManager.cpp:397-400`) and there is no `steamSafetyShutdownCheck`
/// anywhere in the tree. The steam valve is driven by the steam PID in
/// `ProcessController`, not by any state — so in the C++ the state machine cannot
/// reach it at all.
///
/// The port can reach it, because `Effect::OpenSteamValve` exists. What is pinned
/// is that **nothing emits it**: over every state and every event, the steam
/// valve is never opened or closed by the state machine. If a future state starts
/// emitting it, this fails — which is the point, because adding a steam-valve
/// whitelist is a deliberate decision and not a side effect.
#[test]
fn s2_the_steam_valve_is_not_whitelist_gated() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = common::context_for(&config);

    for state in cc_domain::state::ALL {
        let mut machine = machine_in(state);
        for ev in all_events() {
            let (next, fx) = reduce(&machine, &ctx, ev);
            assert_eq!(
                common::count(&fx, Effect::OpenSteamValve),
                0,
                "{state:?} x {ev:?} opened the steam valve"
            );
            assert_eq!(
                common::count(&fx, Effect::CloseSteamValve),
                0,
                "{state:?} x {ev:?} closed the steam valve"
            );
            machine = next;
        }
    }

    // And the asymmetry is explicit: 18 states, six on the water whitelist, and
    // the steam valve has no whitelist at all — not even a subset.
    let water: Vec<MachineState> = cc_domain::state::ALL
        .iter()
        .copied()
        .filter(|s| water_flow_allowed(*s))
        .collect();
    assert_eq!(water.len(), 6);
    assert!(
        MachineState::SteamRunning.is_steam_state()
            && !water_flow_allowed(MachineState::SteamRunning),
        "the state that is *about* steam is not on the water whitelist"
    );
}

// ---------------------------------------------------------------------------
// §3 — the water valve is not gated on an empty tank
// ---------------------------------------------------------------------------

/// **Preserved deliberately, see 09 §3.** Only `enablePump` and `setPumpPressure`
/// check `waterTankEmpty_` (`HardwareManager.cpp:325-328, 398-406`).
/// `openWaterValve` does not.
///
/// So the C++ will open the water valve with a dry tank, and the pump — the part
/// that would actually move water — is refused. The observable consequence in the
/// port is that a `WATER_TANK_EMPTY` machine still gets `CloseWaterValve` from the
/// S5 check (so the valve is not *left* open) but nothing refuses an
/// `OpenWaterValve`.
#[test]
fn s3_the_water_valve_is_not_gated_on_an_empty_tank() {
    // `cc-safety`'s verdict refuses the pump and nothing else.
    let verdict = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(Celsius::new(25.0), false, MachineState::BrewRunning),
        &cc_safety::SafetyConfig::default(),
        Millis::new(0),
    );
    assert!(!verdict.verdict.may_pump, "S4 refuses the pump");
    assert!(
        verdict.verdict.may_open_water,
        "preserved: S4 does NOT refuse the water valve — see 09 §3"
    );

    // And the verdict's `may_open_water` is governed only by the state whitelist.
    let in_water_state = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(Celsius::new(25.0), false, MachineState::BrewRunning),
        &cc_safety::SafetyConfig::default(),
        Millis::new(0),
    );
    assert!(in_water_state.verdict.may_open_water);
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
    let telemetry = cc_safety::Telemetry::new(Celsius::new(151.0), true, MachineState::PidNormal);

    let mut safety = cc_safety::SafetyState::CLEAR;
    for reading in 1..cc_safety::DEBOUNCE_COUNT {
        let outcome = cc_safety::reduce(&safety, &telemetry, &cfg, Millis::new(0));
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

    let outcome = cc_safety::reduce(&safety, &telemetry, &cfg, Millis::new(0));
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
// §11 (new) — the pump watchdogs are never armed
// ---------------------------------------------------------------------------

/// **Preserved deliberately; new finding, see `timing::PUMP_TIMEOUTS_NEVER_ARM`.**
///
/// `BrewHandler::checkPumpTimeout` (`BrewHandler.h:254-262`) and
/// `HotWaterHandler::checkPumpTimeout` (`HotWaterHandler.h:114-122`) are the only
/// run-time bound on how long the pump may run. Both are inert: `PumpTimer`
/// initialises `isRunning_ = false` (`PumpTimer.h:14`) and **nothing calls
/// `start()`**, so `isExpired()` returns `false` unconditionally.
///
/// In the C++ that means: hold the water switch forever and the pump runs
/// forever. There is no timeout.
///
/// The port keeps the check and makes it *reachable* (the reducer has an explicit
/// `brew_pump_started_at` where the C++ has a timer nobody starts), so the port
/// is strictly safer here. What is pinned is that the check exists, that the brew
/// one requests a stop rather than transitioning, and that an unarmed timer never
/// fires.
#[test]
fn s11_the_pump_timeouts_are_never_armed() {
    // The constants survive, with the C++'s values.
    assert_eq!(cc_machine::timing::BREW_PUMP_TIMEOUT_MS, 300_000);
    assert_eq!(cc_machine::timing::HOT_WATER_PUMP_TIMEOUT_MS, 60_000);
    // A `const` block rather than a runtime assert: the finding is a fact
    // about the C++, and the compiler already knows its value.
    const { assert!(cc_machine::timing::PUMP_TIMEOUTS_NEVER_ARM) };

    // An unarmed timer never fires: the C++'s
    // `if (!isRunning_ || startTime_ == 0) return false;` (`PumpTimer.h:24`).
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.machine.brew_pump_started_at = None;
    h.machine.hot_water_pump_started_at = None;
    h.machine.switches.hot_water = true;
    h.config.brew.mode = cc_domain::process::BrewMode::Manual;
    let fx = h.elapse(10 * 60_000);
    assert!(
        !h.requested(Request::BrewStop),
        "an unarmed pump timer must not request a brew stop: {fx:?}"
    );

    // Armed, the brew watchdog requests a stop — and does **not** transition in
    // the same tick, because `BrewHandler.h:259` sets a flag that
    // `checkTransitions` consumes on the *next* loop.
    let mut h2 = Harness::in_state(MachineState::BrewRunning);
    h2.config.brew.mode = cc_domain::process::BrewMode::Manual;
    h2.machine.brew_pump_started_at = Some(Millis::new(0));
    let fx = h2.elapse(300_001);
    assert!(h2.requested(Request::BrewStop), "{fx:?}");
    assert_eq!(
        h2.state(),
        MachineState::BrewRunning,
        "the request must not transition in the same tick"
    );
    let fx = h2.tick();
    assert_eq!(h2.state(), MachineState::BrewFinished, "{fx:?}");

    // Armed, the hot-water watchdog stops the pump.
    let mut h3 = Harness::in_state(MachineState::PidNormal);
    h3.machine.hot_water_pump_started_at = Some(Millis::new(0));
    h3.machine.switches.hot_water = true;
    let fx = h3.elapse(60_001);
    assert!(common::has(&fx, Effect::DisablePump), "{fx:?}");
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
