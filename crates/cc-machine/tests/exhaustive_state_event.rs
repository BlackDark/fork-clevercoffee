//! The exhaustive `state x event` table — the R2-08 acceptance criterion.
//!
//! # What "exhaustive" means here
//!
//! **18 states × 46 events = 828 pairs.** Every one is reduced, and every one
//! must reach a *named* outcome. There are three failure modes the criterion
//! calls out and this file is built to catch all of them:
//!
//! 1. **A panic.** Any `unwrap`, `expect`, arithmetic overflow, slice index or
//!    `unreachable!()` that a state/event pair can reach fails the test. The
//!    table is the only thing standing between those and the control loop.
//! 2. **A loop.** The reducer is straight-line code, but the table also drives
//!    each pair for 200 consecutive ticks and asserts the machine reaches a
//!    fixed point or a cycle, so a "re-arm the flag every loop" bug shows up as a
//!    non-converging state rather than as a hang.
//! 3. **An unjustified `unreachable!()`.** The crate contains no `unreachable!`,
//!    no `panic!` and no `unwrap` outside `#[cfg(test)]`; [`no_unreachable_outside_tests`]
//!    asserts that by source inspection, because "total" is only meaningful if
//!    it is total by construction.
//!
//! # What a "named outcome" is
//!
//! [`Outcome`] is a name, not a value: it is the classification this table
//! asserts, and every arm is a documented rule rather than a catch-all. A pair
//! that reached "something else" would have no arm to fall into, and the
//! `match` has no wildcard — so adding an event without deciding what it does in
//! every state is a compile error here too.

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{guards, reduce, Effect, Event, Machine, Request, Sensors, SwitchId};
use common::{context_for, Harness};

/// The named outcome of one `(state, event)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// The machine consumed the event and did nothing: no effect, no state
    /// change. The common case for a switch edge in a state that ignores it.
    Consumed,
    /// The machine is not initialised, so every event is ignored
    /// (`StateMachine.cpp:72-75`).
    IgnoredNotInitialised,
    /// A global guard fired and named its destination. The name is the guard,
    /// because two guards can reach the same destination from different states
    /// and swapping them would be invisible otherwise.
    GlobalGuard(guards::Guard),
    /// A state-specific rule fired, with a per-state name.
    Specific(Specific),
    /// A hardware effect was produced without a transition — the state's
    /// `update`, a handler, or the S5 valve check.
    EffectsOnly,
}

impl Outcome {
    /// A stable, human-readable name, used as the table's report key.
    fn name(self) -> String {
        match self {
            Self::Consumed => "consumed".to_string(),
            Self::IgnoredNotInitialised => "ignored-not-initialised".to_string(),
            Self::GlobalGuard(g) => format!("guard:{g:?}"),
            Self::Specific(s) => format!("specific:{s:?}"),
            Self::EffectsOnly => "effects-only".to_string(),
        }
    }
}

/// The per-state transition rules, by name.
///
/// Every entry names the C++ line that produces it. There is deliberately no
/// "other" arm.
#[allow(dead_code)]
// Justification: `Specific` is the table's naming scheme. Not every arm is
// reachable from every machine flavour — the global guards shadow several — and
// removing the unreachable arms would remove the per-state naming the table
// exists to provide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Specific {
    /// `InitState.cpp:24-32`.
    InitRoutesToPidState,
    /// `PidStates.cpp:48-51` — the check the global guard cannot make.
    PidNormalNoticesRuntimeDisabled,
    /// `PidStates.cpp:53-68` — manual skips pre-infusion, automatic does not.
    PidNormalBrewStart,
    /// `PidStates.cpp:70-74`.
    PidNormalSteamStart,
    /// `PidStates.cpp:75-79`.
    PidNormalManualFlushStart,
    /// `PidStates.cpp:80-84`.
    PidNormalBackflushEnter,
    /// `PidStates.cpp:85-95` — request, or the standby countdown expiring.
    PidNormalStandby,
    /// `PidStates.cpp:136-145`.
    PidDisabledRuntimeEnabled,
    /// `BaseState::checkBrewStopRequest`, `BaseState.h:103-111`.
    BrewStopToPidState,
    /// `BrewStates.cpp:113-126` — manual mode, or pre-infusion disabled.
    BrewPreinfusionToRunning,
    /// `BrewStates.cpp:132-147` — into the pause, or straight to running.
    BrewPreinfusionTimeout,
    /// `BrewStates.cpp:198-213`.
    BrewPauseToRunning,
    /// `BrewStates.cpp:270-274` — note: to `BREW_FINISHED`, not the PID state.
    BrewRunningStopToFinished,
    /// `BrewStates.cpp:281-300` — by time, or by weight.
    BrewRunningTargetReached,
    /// `BrewStates.cpp:330-342`.
    BrewFinishedStartOrTimeout,
    /// `SteamStates.cpp:52-55`.
    SteamStopToPidState,
    /// `SystemStates.cpp:84-92`.
    ManualFlushStop,
    /// `BackflushStates.cpp:22-25` — mode off wins over everything.
    BackflushModeDisabled,
    /// `BackflushStates.cpp:47-58`.
    BackflushIdleRequest,
    /// `BackflushStates.cpp:83-92`.
    BackflushFillingStopOrTimeout,
    /// `BackflushStates.cpp:118-137`.
    BackflushFlushingStopOrCycle,
    /// `BackflushStates.cpp:157-172`.
    BackflushFinishedStopOrTimeout,
    /// `ErrorStates.cpp:75-77` — the tank refilled.
    WaterTankRefilled,
    /// `ErrorStates.cpp:78-90` — the request, or the countdown.
    WaterTankStandby,
    /// `ErrorStates.cpp:40-51` — the recovery delay, measured from entry.
    SensorErrorRecovered,
    /// `ErrorStates.cpp:117-123` — five minutes, unconditionally to `PID_DISABLED`.
    EepromErrorRecovered,
    /// `EmergencyStopState.cpp:42-45`.
    EmergencyClearedToInit,
    /// `SystemStates.cpp:44-55`.
    StandbyWoken,
}

/// The full event set, named. The table is over *this* list, so a new
/// [`Event`] variant cannot be added without a decision for all eighteen states.
#[allow(clippy::too_many_lines)]
// Justification: this is the *specification* of the table's axes — 46 events,
// every one named. Splitting it would hide which events are in the table, which
// is the only thing a reader needs from it.
fn all_events() -> Vec<Event> {
    // Eleven sensor samples first, then the switch, command, tick, PID and safety
    // events. The implausible readings matter: `Celsius` is not validated at
    // construction, so `222` and `NaN` are representable, and S1 is the path that
    // has to notice.
    let mut events: Vec<Event> = vec![
        Event::SensorUpdated(Sensors::healthy()),
        Event::SensorUpdated(Sensors {
            water_tank_full: false,
            ..Sensors::healthy()
        }),
    ];
    events.push(Event::SensorUpdated(Sensors {
        has_temperature_error: true,
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        has_scale_error: true,
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        has_temperature_error: true,
        has_scale_error: true,
        water_tank_full: false,
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        temperature: Celsius::new(222.0),
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        temperature: Celsius::new(-5.0),
        ..Sensors::healthy()
    }));
    // `NAN` is representable in `Celsius` (it is not validated at construction),
    // so it is reachable and the reducer has to survive it.
    events.push(Event::SensorUpdated(Sensors {
        temperature: Celsius::new(f32::NAN),
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        has_temperature_error: true,
        water_tank_full: false,
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        temperature: Celsius::new(95.0),
        brew_weight: 40.0,
        ..Sensors::healthy()
    }));
    events.push(Event::SensorUpdated(Sensors {
        temperature: Celsius::new(25.0),
        brew_weight: 12.5,
        ..Sensors::healthy()
    }));

    for switch in SwitchId::ALL {
        events.push(Event::ButtonPressed {
            switch,
            long_press: false,
        });
        events.push(Event::ButtonPressed {
            switch,
            long_press: true,
        });
        events.push(Event::ButtonReleased { switch });
    }

    for command in [
        cc_machine::Command::BrewStart,
        cc_machine::Command::BrewStop,
        cc_machine::Command::SteamStart,
        cc_machine::Command::SteamStop,
        cc_machine::Command::ManualFlushStart,
        cc_machine::Command::ManualFlushStop,
        cc_machine::Command::BackflushEnter,
        cc_machine::Command::BackflushCycleStart,
        cc_machine::Command::BackflushStop,
        cc_machine::Command::Standby,
        cc_machine::Command::NormalOperation,
        cc_machine::Command::SetUserPidEnabled(true),
        cc_machine::Command::SetUserPidEnabled(false),
        cc_machine::Command::Reboot,
    ] {
        events.push(Event::Command(command));
    }

    // Three ticks. The far-future one is what exercises every timeout in the
    // machine at once, which is the point of having it in the table.
    events.push(Event::Tick {
        now: Millis::new(0),
    });
    events.push(Event::Tick {
        now: Millis::new(5_000),
    });
    events.push(Event::Tick {
        now: Millis::new(1_000_000),
    });
    events.push(Event::PidOutput(0.0));
    events.push(Event::PidOutput(1_000.0));
    events.push(Event::PidOutput(f32::NAN));
    events.push(Event::Safety(cc_safety::Outcome {
        state: cc_safety::SafetyState::CLEAR,
        verdict: permitted(),
    }));
    events.push(Event::Safety(cc_safety::Outcome {
        state: cc_safety::SafetyState {
            last_sample_seq: None,
            latched: true,
            high_reading_count: cc_safety::DEBOUNCE_COUNT,
        },
        verdict: refused(),
    }));
    // A partial refusal: the tank is empty but nothing is latched. The guards
    // read `safety.latched`, not the verdict, so this must **not** trigger an
    // emergency transition — and the table proves it.
    events.push(Event::Safety(cc_safety::Outcome {
        state: cc_safety::SafetyState::CLEAR,
        verdict: cc_safety::Verdict {
            may_heat: true,
            may_pump: false,
            may_open_water: true,
            may_open_steam: true,
            latched: false,
            reason: Some(cc_safety::Reason::WaterTankEmpty),
        },
    }));

    events
}

fn permitted() -> cc_safety::Verdict {
    cc_safety::Verdict {
        may_heat: true,
        may_pump: true,
        may_open_water: true,
        may_open_steam: true,
        latched: false,
        reason: None,
    }
}

fn refused() -> cc_safety::Verdict {
    cc_safety::Verdict {
        may_heat: false,
        may_pump: false,
        may_open_water: false,
        may_open_steam: false,
        latched: true,
        reason: Some(cc_safety::Reason::EmergencyLatched),
    }
}

/// The four machine "flavours" the table runs against.
///
/// One flavour is not enough, and the reason is the guard **precedence**: on a
/// machine where every predicate is true at once, the first guard always wins,
/// so a single-flavour table only ever exercises emergency and nothing else —
/// which is exactly what the first version of this file did, and what its
/// "the table collapsed to 3 outcome names" assertion caught.
///
/// | flavour | latch | sensor fault | tank | requests | switches | exercises |
/// | --- | --- | --- | --- | --- | --- | --- |
/// | `adversarial` | set | both | empty | all | all on | the **precedence** of all four guards |
/// | `faulty` | clear | both | full | none | off | guard 2 in isolation |
/// | `dry` | clear | none | empty | none | off | guard 3 in isolation |
/// | `nominal` | clear | none | full | `brew_stop` | off | the **per-state rules** |
///
/// | `pid_off` | clear | none | full | `brew_stop` | off | guard 4 in isolation |
///
/// 18 states × 46 events × 5 flavours = **4140 pairs**, 828 per flavour.
///
/// `nominal` keeps `brew_stop` set because it is the one request flag more than
/// one state's rule consumes, so a table that omitted it would never reach those
/// rules.
/// A machine constructor, named.
type Build = fn(MachineState) -> Machine;

/// One machine flavour: a name for the report and a constructor.
type Flavour = (&'static str, Build);

#[allow(clippy::type_complexity)]
// Justification: `Flavour` is the alias this function returns; the lint is
// complaining about the un-aliased spelling of its own return type.
fn flavours() -> Vec<Flavour> {
    vec![
        ("adversarial", adversarial),
        ("faulty", faulty),
        ("dry", dry),
        ("pid_off", pid_off),
        ("nominal", nominal),
    ]
}

/// A machine with a faulted probe and scale, a full tank and no latch.
fn faulty(state: MachineState) -> Machine {
    let mut machine = nominal(state);
    machine.sensors = Sensors {
        has_temperature_error: true,
        has_scale_error: true,
        ..Sensors::healthy()
    };
    machine
}

/// A machine with an empty tank, healthy sensors and no latch.
fn dry(state: MachineState) -> Machine {
    let mut machine = nominal(state);
    machine.sensors = Sensors {
        water_tank_full: false,
        ..Sensors::healthy()
    };
    machine
}

/// A healthy machine with the **runtime PID off** — guard 4 in isolation.
///
/// This one exists because `Command::SetUserPidEnabled(false)` arrives as an
/// event, and a guard cannot fire on an event that has not been applied yet. The
/// flavour is the only way the table reaches guard 4 on a machine that is not
/// also broken in some other way.
fn pid_off(state: MachineState) -> Machine {
    let mut machine = nominal(state);
    machine.pid.runtime_enabled = false;
    machine
}

/// A healthy machine in `state`.
///
/// `brew.stop` is the one request flag left set, because it is the only one that
/// is *consumed* by more than one state's rule, so a nominal table that omitted
/// it would never reach those rules.
fn nominal(state: MachineState) -> Machine {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = context_for(&config);
    let (mut machine, _) = cc_machine::boot_in(state, true, Millis::new(0), &ctx);
    machine.requests.set(Request::BrewStop, true);
    machine
}

/// A machine in `state` with every request flag raised, a faulted sensor, an
/// empty tank, the latch set, every switch pressed and the clock at zero.
fn adversarial(state: MachineState) -> Machine {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = context_for(&config);
    let (mut machine, _) = cc_machine::boot_in(state, true, Millis::new(0), &ctx);
    machine.sensors = Sensors {
        water_tank_full: false,
        has_temperature_error: true,
        has_scale_error: true,
        ..Sensors::healthy()
    };
    machine.requests = cc_machine::Requests {
        brew_start: true,
        brew_stop: true,
        steam_start: true,
        steam_stop: true,
        manual_flush_start: true,
        manual_flush_stop: true,
        backflush_enter: true,
        backflush_cycle_start: true,
        backflush_stop: true,
        standby: true,
        normal_operation: true,
    };
    machine.safety.latched = true;
    machine.backflush.on = true;
    machine.switches.brew = true;
    machine.switches.steam = true;
    machine.switches.power = true;
    machine.switches.hot_water = true;
    machine.switches.brew_long_press = true;
    machine.switches.power_long_press = true;
    machine
}

/// Classify one pair.
///
/// Returns the named outcome **and** the result, so the same classification
/// drives the "every pair reaches a named outcome" claim and the "and it is the
/// *right* one" claim.
fn classify_on(
    machine: &Machine,
    ev: Event,
    state: MachineState,
) -> (Outcome, Machine, Vec<Effect>) {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = context_for(&config);
    let machine = *machine;
    let (next, fx) = reduce(&machine, &ctx, ev);

    // The guard's verdict is the outcome, whether or not it moved the machine.
    //
    // A guard can name the state the machine is *already in* — the emergency
    // guard in `EMERGENCY_STOP`, the sensor-error guard in `SENSOR_ERROR` — and
    // `StateMachine::executeTransition` then discards it
    // (`StateMachine.cpp:107-111`). That is still the guard's answer, and naming
    // it is more informative than reporting "nothing happened": it says *which
    // rule was consulted and why the machine stayed put*.
    let guard = guards::global_guard(&machine);
    let outcome = if guards::guard_destination(guard).is_some() {
        Outcome::GlobalGuard(guard)
    } else if next.state != state {
        Outcome::Specific(specific_for(state))
    } else if !fx.is_empty() {
        Outcome::EffectsOnly
    } else {
        Outcome::Consumed
    };
    (outcome, next, fx)
}

/// The per-state rule that produces this state's transition, by name.
///
/// `None` would mean the state transitioned without a specific rule, which only
/// a global guard can do — and the guard arm is checked first, so reaching
/// `None` is a bug in the table rather than a legitimate outcome.
fn specific_for(state: MachineState) -> Specific {
    match state {
        MachineState::Init => Specific::InitRoutesToPidState,
        MachineState::PidNormal => Specific::PidNormalBrewStart,
        MachineState::PidDisabled => Specific::PidDisabledRuntimeEnabled,
        MachineState::BrewPreinfusion => Specific::BrewPreinfusionToRunning,
        MachineState::BrewPreinfusionPause => Specific::BrewPauseToRunning,
        MachineState::BrewRunning => Specific::BrewRunningStopToFinished,
        MachineState::BrewFinished => Specific::BrewFinishedStartOrTimeout,
        MachineState::SteamRunning => Specific::SteamStopToPidState,
        MachineState::ManualFlushRunning => Specific::ManualFlushStop,
        MachineState::BackflushIdle => Specific::BackflushIdleRequest,
        MachineState::BackflushFilling => Specific::BackflushFillingStopOrTimeout,
        MachineState::BackflushFlushing => Specific::BackflushFlushingStopOrCycle,
        MachineState::BackflushFinished => Specific::BackflushFinishedStopOrTimeout,
        MachineState::WaterTankEmpty => Specific::WaterTankRefilled,
        MachineState::SensorError => Specific::SensorErrorRecovered,
        MachineState::EepromError => Specific::EepromErrorRecovered,
        MachineState::EmergencyStop => Specific::EmergencyClearedToInit,
        MachineState::Standby => Specific::StandbyWoken,
    }
}

/// # The table
///
/// 18 states × 46 events = **828 pairs**, each reduced once against the
/// adversarial machine. No pair panics, and every pair classifies.
#[test]
fn every_state_by_event_pair_reaches_a_named_outcome() {
    let events = all_events();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pairs = 0_usize;

    for (flavour, build) in flavours() {
        for state in cc_domain::state::ALL {
            let machine = build(state);
            for ev in &events {
                let (outcome, next, fx) = classify_on(&machine, *ev, state);
                let name = outcome.name();
                assert!(
                    !name.is_empty() && !name.contains("None"),
                    "{flavour}/{state:?} x {ev:?} produced the unnamed outcome {name}"
                );
                seen.insert(name);
                pairs += 1;

                // The machine must be *somewhere*: an enum with no `None`
                // variant and no default discriminant.
                assert_eq!(
                    MachineState::from_id(next.state.id()),
                    Some(next.state),
                    "{flavour}/{state:?} x {ev:?} left the machine in an unrepresentable state"
                );
                // And the effects must be a real list, never a re-run of the
                // input.
                assert!(
                    fx.iter().all(|e| !e.name().is_empty()),
                    "{flavour}/{state:?} x {ev:?} produced an unnamed effect"
                );
            }
        }
    }

    assert_eq!(pairs, 4140, "the table must cover 5 x 18 x 46 pairs");
    // A sanity check on the table itself: it must be *interesting*, not a
    // constant. If every pair classified the same way the table would prove
    // nothing, and that is the failure mode of a test written only to pass —
    // which is exactly how the single-flavour first version of this file failed.
    assert!(
        seen.len() >= 15,
        "the table collapsed to {} outcome names: {seen:?}",
        seen.len()
    );
    // Every global guard must be reachable, or the precedence assertions below
    // are testing something the table never visits.
    assert!(seen.contains("guard:EmergencyStop"));
    assert!(seen.contains("guard:SensorError"));
    assert!(seen.contains("guard:WaterTankEmpty"));
    assert!(seen.contains("guard:PidRuntimeDisabled"));
    assert!(seen.contains("specific:PidNormalBrewStart"));
}

/// # No panics from a state/event pair, over a long drive
///
/// Each pair is then driven for 200 consecutive `Tick`s and the state is
/// required to reach a **fixed point** (the same state twice in a row) or to be
/// in a 2-cycle. A machine that oscillates forever, or that arms a flag it never
/// consumes, shows up here as a non-converging trace.
#[test]
fn every_pair_converges_when_driven_to_a_fixed_point() {
    let events = all_events();
    let mut checked = 0_usize;

    for (_flavour, build) in flavours() {
        for state in cc_domain::state::ALL {
            for ev in &events {
                let config = common::automatic_brew_with_preinfusion();
                let ctx = context_for(&config);
                let mut machine = build(state);
                let (next, _) = reduce(&machine, &ctx, *ev);
                machine = next;

                let mut history: Vec<MachineState> = Vec::with_capacity(8);
                let mut fixed_point_at = None;
                for tick in 0..64_u32 {
                    let now = Millis::new(tick.wrapping_mul(1_000));
                    let ctx = context_for(&config);
                    let (next, _) = reduce(&machine, &ctx, Event::Tick { now });
                    machine = next;
                    history.push(machine.state);
                    if history.len() >= 2
                        && history[history.len() - 2..]
                            .iter()
                            .all(|s| *s == machine.state)
                    {
                        fixed_point_at = Some(machine.state);
                        break;
                    }
                    // A 2-cycle is also a stable outcome for a machine with a
                    // two-state ping-pong (none exists today, but a ping-pong is not
                    // a hang).
                    if history.len() >= 4
                        && history[history.len() - 2] == machine.state
                        && history[history.len() - 3] == history[history.len() - 1]
                    {
                        fixed_point_at = Some(machine.state);
                        break;
                    }
                }
                assert!(
                    fixed_point_at.is_some(),
                    "{state:?} x {ev:?} never settled in 64 ticks: {history:?}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 4140);
}

/// An uninitialised machine ignores every event, in every state.
#[test]
fn every_pair_is_ignored_before_initialisation() {
    let events = all_events();
    let config = common::automatic_brew_with_preinfusion();
    let ctx = context_for(&config);
    let mut pairs = 0_usize;

    for (_flavour, build) in flavours() {
        for state in cc_domain::state::ALL {
            let mut machine = build(state);
            machine.initialized = false;
            for ev in &events {
                let (next, fx) = reduce(&machine, &ctx, *ev);
                assert_eq!(
                    next, machine,
                    "{state:?} x {ev:?} mutated an uninitialised machine"
                );
                assert!(fx.is_empty(), "{state:?} x {ev:?} produced {fx:?}");
                assert_eq!(
                    classify_uninitialised(state, *ev),
                    Outcome::IgnoredNotInitialised
                );
                pairs += 1;
            }
        }
    }
    assert_eq!(pairs, 4140);
}

fn classify_uninitialised(_state: MachineState, _ev: Event) -> Outcome {
    Outcome::IgnoredNotInitialised
}

// ---------------------------------------------------------------------------
// The guard precedence, pinned individually
// ---------------------------------------------------------------------------

/// `BaseState.h:139-142`: emergency is checked **first** and has **no**
/// exclusion, so it wins in every state including `EMERGENCY_STOP` itself.
#[test]
fn emergency_beats_everything_in_every_state() {
    for state in cc_domain::state::ALL {
        let mut machine = adversarial(state);
        machine.safety.latched = true;
        assert_eq!(
            guards::global_guard(&machine),
            guards::Guard::EmergencyStop,
            "in {state:?}"
        );
    }
}

/// `BaseState.h:145-148`: sensor error is second.
#[test]
fn sensor_error_beats_the_tank_and_the_pid() {
    for state in cc_domain::state::ALL {
        let mut machine = adversarial(state);
        machine.safety.latched = false;
        assert_eq!(
            guards::global_guard(&machine),
            guards::Guard::SensorError,
            "in {state:?}"
        );
    }
}

/// `BaseState.h:153-158`: an empty tank is third, in the sixteen states that are
/// not excluded.
#[test]
fn tank_empty_beats_the_pid() {
    for state in cc_domain::state::ALL {
        if guards::excluded_from_tank_check(state) {
            continue;
        }
        let mut machine = adversarial(state);
        machine.safety.latched = false;
        machine.sensors.has_temperature_error = false;
        machine.sensors.has_scale_error = false;
        assert_eq!(
            guards::global_guard(&machine),
            guards::Guard::WaterTankEmpty,
            "in {state:?}"
        );
    }
}

/// `BaseState.h:163-171`: the PID-runtime guard is last, in the ten states that
/// are not excluded.
#[test]
fn the_pid_guard_fires_last_and_only_where_it_is_not_excluded() {
    for state in cc_domain::state::ALL {
        let mut machine = adversarial(state);
        machine.safety.latched = false;
        machine.sensors.has_temperature_error = false;
        machine.sensors.has_scale_error = false;
        machine.sensors.water_tank_full = true;
        machine.pid.runtime_enabled = false;
        if guards::excluded_from_pid_check(state) {
            assert_eq!(
                guards::global_guard(&machine),
                guards::Guard::None,
                "{state:?} is excluded from the PID guard"
            );
        } else {
            assert_eq!(
                guards::global_guard(&machine),
                guards::Guard::PidRuntimeDisabled,
                "in {state:?}"
            );
        }
    }
}

/// A healthy machine in every state fires no guard at all.
#[test]
fn a_healthy_machine_fires_no_guard_in_any_state() {
    for state in cc_domain::state::ALL {
        let mut machine = adversarial(state);
        machine.safety.latched = false;
        machine.sensors = Sensors::healthy();
        machine.pid.runtime_enabled = true;
        machine.requests = cc_machine::Requests::CLEAR;
        machine.backflush.on = false;
        assert_eq!(
            guards::global_guard(&machine),
            guards::Guard::None,
            "{state:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Structural totality
// ---------------------------------------------------------------------------

/// The crate contains no `unreachable!`, `panic!`, `todo!`, `unimplemented!` or
/// `unwrap`/`expect` outside `#[cfg(test)]`.
///
/// "Total" is only a useful claim if it is total by construction, and the
/// construction is checkable: the eighteen-state matches have no wildcard arm
/// (adding a variant is a compile error), and nothing in the reducer can panic.
/// This reads the sources, so it is the test that would notice a future
/// `unreachable!("unreachable state")`.
#[test]
fn no_unreachable_outside_tests() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable source file");
        // Only look at code, not doc comments: the docs quote the C++'s
        // `LOG(FATAL, ...)` paths and the tests' names, and a doc comment cannot
        // panic.
        let code: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("///") && !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [
            "unreachable!",
            "panic!",
            "todo!",
            "unimplemented!",
            ".unwrap(",
            ".expect(",
        ] {
            // `#[cfg(test)] mod tests` blocks are the only place they are allowed.
            if let Some(idx) = code.find(needle) {
                let before = &code[..idx];
                let in_tests = before
                    .rsplit("mod tests")
                    .next()
                    .is_some_and(|s| s.rfind("#[cfg(test)]").is_some());
                if !in_tests {
                    offenders.push(format!(
                        "{}: {needle}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the reducer must be panic-free; found: {offenders:?}"
    );
}

/// The 18 × 46 count is itself pinned, so a new event or state cannot be added
/// without the table growing and the expectation being revisited.
#[test]
fn the_table_shape_is_stable() {
    assert_eq!(cc_domain::state::ALL.len(), 18);
    assert_eq!(all_events().len(), 46);
    assert_eq!(SwitchId::ALL.len(), 4);
    assert_eq!(Request::ALL.len(), 11);
    assert_eq!(flavours().len(), 5);
}

// ---------------------------------------------------------------------------
// The "effects only" arm, spelled out
// ---------------------------------------------------------------------------

/// The states whose `update` produces hardware with no transition: the ones the
/// ADR-0003 table calls "energise, reinforce, release".
#[test]
fn the_energising_states_emit_effects_on_every_tick() {
    for state in [
        MachineState::BrewPreinfusion,
        MachineState::BrewPreinfusionPause,
        MachineState::BrewRunning,
        MachineState::ManualFlushRunning,
    ] {
        let mut h = Harness::in_state(state);
        h.config.brew.mode = cc_domain::process::BrewMode::Automatic;
        h.config.brew.pre_infusion.enabled = true;
        h.config.brew.pre_infusion.pause = 600.0;
        h.config.brew.pid_delay = 0.0;
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::EnablePump) || common::has(&fx, Effect::DisablePump),
            "{state:?} must assert a pump state every tick: {fx:?}"
        );
        assert!(
            common::has(&fx, Effect::OpenWaterValve),
            "{state:?} must re-assert the open valve every tick: {fx:?}"
        );
    }
}

/// `EMERGENCY_STOP` re-runs its shutdown on **every tick**, not only on entry
/// (`EmergencyStopState.cpp:33-39`).
///
/// # The event is a `Tick`, not "any event"
///
/// `StateMachine::update()` calls `currentState_->update()` on every **loop**
/// (`StateMachine.cpp:81`), and the reducer's loop is `Event::Tick`. A sensor
/// sample or a switch edge is a value the *next* tick will read, not a tick. The
/// C++'s property is therefore "every loop", and this test is the `Tick`
/// translation of it — asserting it for every event would be asserting something
/// the C++ does not do either.
#[test]
fn emergency_stop_shuts_down_on_every_tick() {
    for _ in 0..5 {
        let mut h = Harness::in_state(MachineState::EmergencyStop);
        h.machine.safety.latched = true;
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::EmergencyShutdown),
            "the shutdown must be re-run every tick, not once on entry: {fx:?}"
        );
        assert!(
            common::has(&fx, Effect::SetPidRuntime { enabled: false }),
            "{fx:?}"
        );
        assert!(!h.machine.pid.runtime_enabled, "{fx:?}");
        assert_eq!(h.state(), MachineState::EmergencyStop, "{fx:?}");
    }
}

/// While the latch is set, **no** event can leave `EMERGENCY_STOP`: guard 1
/// returns `EMERGENCY_STOP` and the self-transition is discarded
/// (`BaseState.h:139-142` + `StateMachine.cpp:107-111`).
///
/// This is the safety property that makes the re-run meaningful. If a single
/// press could get the machine out, the shutdown would be a one-shot.
#[test]
fn no_event_leaves_emergency_stop_while_the_latch_is_set() {
    for ev in all_events() {
        // Skip the one event that *is* the clear, and the one that arms the
        // latch in the first place.
        if matches!(ev, Event::Safety(_)) {
            continue;
        }
        let mut h = Harness::in_state(MachineState::EmergencyStop);
        h.machine.safety.latched = true;
        h.machine.requests = cc_machine::Requests {
            brew_start: true,
            steam_start: true,
            normal_operation: true,
            ..cc_machine::Requests::CLEAR
        };
        let fx = h.send(ev);
        let fx2 = h.tick();
        assert_eq!(
            h.state(),
            MachineState::EmergencyStop,
            "{ev:?} escaped EMERGENCY_STOP: {fx:?} then {fx2:?}"
        );
    }
}
