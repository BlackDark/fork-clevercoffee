//! The recorded divergences this crate can demonstrate, one test each.
//!
//! [`docs/history/divergences.md`](../../docs/history/divergences.md) is the record, and each
//! section there names its pin in this file. The point of the file is that a deliberate difference
//! from the machine this firmware replaces cannot be read as a bug and quietly reverted: the
//! behaviour is asserted here and the ledger says why it is the way it is.
//!
//! # The other half: what was deliberately *not* changed
//!
//! Some behaviours that look like defects were left alone on purpose, and those tests are the more
//! valuable half of this file, because "we left this alone" is the claim most likely to be
//! reversed by someone who finds it surprising. Each one says so in its own doc comment.
//!
//! The divergences:
//!
//! * §1 the pump watchdogs: [`div1_the_pump_timeouts_are_armed`],
//!   [`div1_the_watchdogs_arm_only_while_the_pump_is_commanded_on`],
//!   [`div1_the_watchdogs_re_arm_after_a_release`].
//! * [`div2_the_steam_valve_is_whitelist_gated`] -- §2 the steam valve.
//! * [`div3_the_water_valve_is_tank_gated`] -- §3 the water valve and the tank.
//! * [`a_sleep_request_is_honoured_with_the_pid_disabled`],
//!   [`the_sleep_request_is_consumed_and_not_repeated`],
//!   [`waking_from_standby_still_works_with_the_pid_disabled`] -- §16 sleep with the PID off.
//! * [`div39_backflush_fill_and_flush_reassert_their_hardware`] -- §39 fill and flush.
//!
//! And the behaviours left alone, which the ledger lists under its preserved ones:
//!
//! * [`the_emergency_debounce_needs_three_readings`] -- three readings above the threshold.
//! * [`the_live_emergency_thresholds_are_the_defaults`] -- 150 °C to trip, 100 °C to recover.
//! * [`the_sensor_error_recovery_delay_is_measured_from_entry`] -- a persistent fault recovers at
//!   once.
//! * [`the_water_switch_does_not_wake_the_machine_from_standby`] -- the water switch only extends
//!   the timer.
//! * [`the_power_off_happens_before_the_standby_request`] -- one tick of pump after the shutdown.

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{
    reduce, steam_flow_allowed, water_flow_allowed, Command, Effect, Event, PumpWatchdog, Request,
    Sensors, SwitchId,
};
use common::Harness;

// ---------------------------------------------------------------------------
// §2 — the steam valve is whitelist-gated
// ---------------------------------------------------------------------------

/// The steam valve is opened only in `STEAM_RUNNING`, and asserted closed everywhere else.
///
/// [`divergences.md` §2](../../docs/history/divergences.md#d02). Three things are asserted, and
/// all three matter:
///
/// 1. **The whitelist is `STEAM_RUNNING` and nothing else.** `cc_safety::steam_flow_allowed` is a
///    `match` with no wildcard arm, so adding a 19th state is a compile error until someone
///    classifies it.
/// 2. **The reducer honours it**, over every state and every event: the valve is never *opened*
///    outside `STEAM_RUNNING`, and is *closed* in every other state. The close is the mirror of
///    S5 for the other valve, and it is what stops a new state from reopening the gap.
/// 3. **The two whitelists are disjoint**, because the steam and water valves are the same
///    physical relay, so a state on both lists could open the shared relay for water while the
///    steam list believed steam was flowing.
#[test]
fn div2_the_steam_valve_is_whitelist_gated() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = common::context_for(&config);

    for state in cc_domain::state::ALL {
        let mut machine = machine_in(state);
        for ev in all_events() {
            let (next, fx) = reduce(&machine, &ctx, ev);
            if steam_flow_allowed(state) {
                // Inside the whitelist the valve is left alone rather than opened: nothing in
                // this firmware asks for steam to be drawn yet (see the module note on
                // `Effect::OpenSteamValve`).
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
// §3 — the water valve is gated on the water tank
// ---------------------------------------------------------------------------

/// An empty tank refuses the pump **and** the water valve.
///
/// [`divergences.md` §3](../../docs/history/divergences.md#d03). Refusing the valve as well as the
/// pump is the divergence: the tank feeds the pump, so a valve left open over an empty tank draws
/// air into the group head and runs the pump dry.
///
/// The heater is deliberately *not* gated on the tank. The boiler is a separate vessel, and
/// `hardware.sensors.watertank.keep_heater_on_empty` is a real configuration the machine has to
/// honour.
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
        "S4 refuses the water valve as well as the pump"
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
// S1's debounce: three readings, then the latch
// ---------------------------------------------------------------------------

/// `cc_safety::DEBOUNCE_COUNT` consecutive readings are needed to latch, and the latch does not
/// set on the first or on the second.
///
/// The consequence is a real exposure rather than a style choice: at the production 400 ms sensor
/// interval the heater stays permitted for up to about 800 ms while the probe already reads above
/// the emergency temperature. Tripping on the first reading is the safer behaviour and is not
/// implemented here, so the ledger records this as an open item rather than as a settled decision:
/// [`divergences.md`](../../docs/history/divergences.md), the preserved-behaviours table.
///
/// The debounce counts *distinct* samples, which is what `Telemetry::sample_seq` exists to express.
/// A test that re-used one telemetry value would model one reading seen repeatedly, and the control
/// task samples far faster than the probe.
///
/// An implausible reading is the exception: anything outside the plausible range latches at once,
/// because there is no benign explanation for it.
#[test]
fn the_emergency_debounce_needs_three_readings() {
    let cfg = cc_safety::SafetyConfig::default();
    // Each iteration is a **distinct sample**, which is what the debounce counts and what
    // `Telemetry::sample_seq` exists to express. Re-using one telemetry value would model one
    // reading seen repeatedly, and would latch an emergency stop in milliseconds.
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
            "reading {reading} of {} does not latch: the debounce needs three",
            cc_safety::DEBOUNCE_COUNT
        );
        assert!(
            outcome.verdict.may_heat,
            "the heater is still permitted on reading {reading}: two readings do not latch"
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
// The live emergency thresholds
// ---------------------------------------------------------------------------

/// 150 °C trips and 100 °C clears, and both are the configured defaults rather than constants the
/// machine ignores.
///
/// The trip threshold is `safety.emergency_temp`, so a configuration that never sets it trips
/// where the shipped machine does; the recovery threshold is `cc_safety::EMERGENCY_SAFE_TEMP_C`.
/// The two are far apart on purpose: a latch that cleared at the trip threshold would chatter on a
/// probe sitting near it.
#[test]
fn the_live_emergency_thresholds_are_the_defaults() {
    assert_eq!(
        cc_safety::SafetyConfig::default().emergency_temp,
        Celsius::new(150.0),
        "the default trip threshold is the configured one"
    );
    assert_eq!(
        cc_safety::EMERGENCY_SAFE_TEMP_C,
        Celsius::new(100.0),
        "the recovery threshold is the live 100 °C constant"
    );

    // A 149 °C reading is below the default threshold, so nothing may latch.
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
// §1 — both pump watchdogs are armed
// ---------------------------------------------------------------------------

/// Both pump watchdogs are armed, and a trip is announced before it acts.
///
/// [`divergences.md` §1](../../docs/history/divergences.md#d01). The clock starts on the tick that
/// commands the pump **on**, not on the tick a state is entered, and it is cleared when the pump
/// is commanded off. Hold the water switch and the pump stops after sixty seconds; hold a brew and
/// it stops after five minutes. An unbounded pump run on a machine with a 2 kW boiler is the
/// failure this prevents.
///
/// The two announcement strings are verbatim: they are the ledger's `matchers` for this divergence,
/// so the parity harness compares against them.
#[test]
fn div1_the_pump_timeouts_are_armed() {
    // The two deadlines, pinned because they are the safety contract.
    assert_eq!(cc_machine::timing::BREW_PUMP_TIMEOUT_MS, 300_000);
    assert_eq!(cc_machine::timing::HOT_WATER_PUMP_TIMEOUT_MS, 60_000);

    // ---- the brew watchdog -------------------------------------------------
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.brew.mode = cc_domain::process::BrewMode::Manual;

    // The activating edge: this tick's update commands the pump on, and that is where the clock
    // starts.
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
        "the deadline test is strict, so the trip lands on the first tick past it"
    );

    // Announced before it acts, in the same tick and after the pump writes. What matters is that
    // the trip is *visible* and that it is emitted at all, rather than being a silent hardware
    // change.
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
        "verbatim: the ledger's matcher for §1"
    );
    assert!(!fx.contains(&Effect::DisablePump), "{fx:?}");

    // The action is a brew-stop **request**, consumed by the next tick's transition rather than
    // this one, so the pump is written exactly once per tick.
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

    // The announcement precedes the stop. The tick's own `EnablePump` comes first -- the water
    // switch is still held, so `PID_NORMAL`'s update asks for the pump again -- which is why this
    // asserts the *order of the two watchdog effects* rather than their absolute index.
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
        "verbatim: the ledger's matcher for §1"
    );
}

/// The arming rule follows the pump command: not "any brew state", and not "any state with the
/// water switch held".
///
/// Asserted against `arm_pump_watchdogs` **directly** rather than through a tick, because a tick
/// also *transitions*: `BREW_PREINFUSION_PAUSE` moves to `BREW_RUNNING` inside the same tick (the
/// pause is time-based) and would therefore be armed for the wrong reason.
#[test]
fn div1_the_watchdogs_arm_only_while_the_pump_is_commanded_on() {
    for (state, hot_water, brew_armed, hot_water_armed) in [
        // The two brew states whose update commands the pump on.
        (MachineState::BrewPreinfusion, false, true, false),
        (MachineState::BrewRunning, false, true, false),
        // A pause is a brew state, but its update commands the pump *off* to hold puck pressure,
        // so a pause is not pump run time and must not run the clock.
        (MachineState::BrewPreinfusionPause, false, false, false),
        (MachineState::BrewFinished, false, false, false),
        // Neither watchdog covers these two, deliberately: each is bounded by its own timeout.
        (MachineState::ManualFlushRunning, false, false, false),
        (MachineState::BackflushFilling, false, false, false),
        // The water switch means hot water only in PID_NORMAL.
        (MachineState::PidNormal, true, false, true),
        (MachineState::PidNormal, false, false, false),
        // In STEAM_RUNNING the same switch means water *injection*, not hot water, so it must not
        // start that clock.
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
// A persistent sensor error recovers as soon as the fault clears
// ---------------------------------------------------------------------------

/// The recovery delay is measured from **entry** to `SENSOR_ERROR`, not from the moment the fault
/// clears.
///
/// The faulted guard answers with the state the machine is already in, and a self-transition is
/// discarded, so the entry time is never re-stamped while the probe stays faulted. A fault that
/// persists for an hour and then clears therefore recovers on the next tick instead of waiting the
/// recovery delay out.
///
/// The recovery is still safe: it happens only once the error is genuinely clear, and the guard is
/// re-checked on every tick. It is the *delay* that does not apply, not the check.
#[test]
fn the_sensor_error_recovery_delay_is_measured_from_entry() {
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
            "the entry time is not re-stamped while the fault persists"
        );
    }

    // The error clears: recovery is immediate, not five seconds later.
    let _ = h.send(Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(10 * 60_000);
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "the delay is measured from entry, not from the clear"
    );
}

// ---------------------------------------------------------------------------
// §39 — fill and flush re-assert their hardware
// ---------------------------------------------------------------------------

/// [`divergences.md` §39](../../docs/history/divergences.md#d39).
#[test]
fn div39_backflush_fill_and_flush_reassert_their_hardware() {
    let mut fill = Harness::in_state(MachineState::BackflushFilling);
    fill.config.backflush.cycles = 5;
    fill.config.backflush.fill_time = 5.0;
    fill.machine.backflush.on = true;
    fill.config.brew.pid_delay = 0.0;

    let update = fill.update(MachineState::BackflushFilling);
    assert!(common::has(&update, Effect::EnablePump), "{update:?}");
    assert!(common::has(&update, Effect::OpenWaterValve), "{update:?}");

    let fx = fill.elapse(1_000);
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
    assert_eq!(common::count(&fx, Effect::CloseWaterValve), 0, "{fx:?}");

    let mut flush = Harness::in_state(MachineState::BackflushFlushing);
    flush.config.backflush.cycles = 5;
    flush.config.backflush.flush_time = 10.0;
    flush.machine.backflush.on = true;

    let update = flush.update(MachineState::BackflushFlushing);
    assert!(common::has(&update, Effect::DisablePump), "{update:?}");
    assert!(common::has(&update, Effect::CloseWaterValve), "{update:?}");

    let fx = flush.tick();
    assert_eq!(common::count(&fx, Effect::CloseWaterValve), 1, "{fx:?}");
    assert!(common::has(&fx, Effect::DisablePump), "{fx:?}");
}

// ---------------------------------------------------------------------------
// The water switch does not wake the machine from standby
// ---------------------------------------------------------------------------

/// The water switch is activity for the standby timer and nothing else.
///
/// It pushes the countdown out -- `Effect::ResetStandbyTimer` -- and does not wake the machine, so
/// a user who walks up to a sleeping machine, holds the water switch for a cup of hot water and
/// lets go gets a machine that is still asleep with a freshly extended timer. Only the brew and
/// steam switches wake it, and the last part of the test shows that.
///
/// Kept as it is: [`divergences.md`](../../docs/history/divergences.md) lists it under the
/// preserved behaviours, where "do not fix these" is the instruction.
#[test]
fn the_water_switch_does_not_wake_the_machine_from_standby() {
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
        "the water switch extends the countdown without waking the machine"
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
// The power-off shutdown precedes the standby request by one tick
// ---------------------------------------------------------------------------

/// The hardware is shut down first, and the standby request is consumed on the next tick.
///
/// For that one tick the machine is in `PID_NORMAL` with the hardware already off, and
/// `PID_NORMAL`'s update commands the pump on again while the water switch is held. Powering off
/// mid-dispense therefore restarts the pump for one control period: narrow, sub-millisecond in
/// practice, and real.
///
/// Kept as it is: [`divergences.md`](../../docs/history/divergences.md) lists it under the
/// preserved behaviours, where "do not fix these" is the instruction.
#[test]
fn the_power_off_happens_before_the_standby_request() {
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
        "PID_NORMAL's update restarts the pump between the shutdown and the transition: {fx:?}"
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

/// A sleep request is honoured while the PID is off.
///
/// [`divergences.md` §16](../../docs/history/divergences.md#d16). Found on hardware: `POST
/// /api/sleep` answered `202 {"accepted":true}`, `control: command Sleep` reached the log, and the
/// state never left `PID_DISABLED`. Since `pid.enabled` defaults to `false`, that was the
/// out-of-the-box configuration, so the machine could not be put to sleep through its own web
/// interface at all.
///
/// `PID_DISABLED` therefore honours the request, as the sibling state one line away already did.
/// Sleep itself needs no heat: putting a cold machine to sleep is a legitimate request.
#[test]
fn a_sleep_request_is_honoured_with_the_pid_disabled() {
    let mut h = Harness::in_state(MachineState::PidDisabled);
    // `in_state` sets the *state*; the runtime flag is separate, and leaving it
    // true would let `PidDisabled`'s first arm hand straight back to
    // `PidNormal` before the standby check is ever reached.
    h.machine.pid.runtime_enabled = false;
    assert_eq!(h.state(), MachineState::PidDisabled, "the precondition");
    assert!(!h.machine.pid.runtime_enabled, "the PID is off");

    // The command sets the request; the tick that follows is where the state's rules see it. A
    // request is never acted on where it is set.
    h.send(Event::Command(Command::Standby));
    h.tick();

    assert_eq!(
        h.state(),
        MachineState::Standby,
        "a sleep request is honoured with the PID off"
    );
}

/// The request is **consumed**, so a later tick does not re-enter standby with nothing having
/// asked it to.
#[test]
fn the_sleep_request_is_consumed_and_not_repeated() {
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
/// The other half of §16: making sleep work with the PID off must not make standby a one-way door.
#[test]
fn waking_from_standby_still_works_with_the_pid_disabled() {
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
        "a wake request is honoured from standby"
    );
}
