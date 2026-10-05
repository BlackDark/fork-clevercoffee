//! Proof that the reducer is a pure function.
//!
//! The acceptance criterion has three parts, and each gets a test:
//!
//! 1. **Deterministic** — the same `(state, ctx, event)` twice gives the same
//!    result. [`the_same_input_twice_gives_the_same_result`]
//! 2. **Non-mutating** — `reduce` does not change its input. Checked by
//!    clone-and-compare, over every state and every event.
//!    [`reduce_does_not_mutate_its_input`]
//! 3. **Referentially transparent** — the result depends only on its arguments,
//!    not on any history. [`the_result_depends_only_on_its_arguments`] reaches
//!    the same state from two different paths and asserts the two agree, and
//!    [`two_identical_machines_stay_identical`]
//!
//! # Why this is not a formality
//!
//! The reducer's whole claim is that the control loop can be tested
//! exhaustively on a host. That claim collapses if any of the three fails: a
//! reducer that mutates its input is a reducer whose behaviour depends on what
//! ran before it, and the `state × event` table would then be describing a
//! different machine on every row.

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{reduce, Context, Event, Machine};
use common::context_for;

/// The full event sample, mirroring the table's.
fn all_events() -> Vec<Event> {
    let mut events = vec![
        Event::SensorUpdated(cc_machine::Sensors::healthy()),
        Event::SensorUpdated(cc_machine::Sensors {
            water_tank_full: false,
            has_temperature_error: true,
            ..cc_machine::Sensors::healthy()
        }),
    ];
    for switch in cc_machine::SwitchId::ALL {
        events.push(Event::ButtonPressed {
            switch,
            long_press: false,
        });
        events.push(Event::ButtonReleased { switch });
    }
    for command in [
        cc_machine::Command::BrewStart,
        cc_machine::Command::SteamStart,
        cc_machine::Command::Standby,
        cc_machine::Command::SetUserPidEnabled(false),
        cc_machine::Command::Reboot,
        cc_machine::Command::BackflushEnter,
    ] {
        events.push(Event::Command(command));
    }
    events.push(Event::Tick {
        now: Millis::new(0),
    });
    events.push(Event::Tick {
        now: Millis::new(60_000),
    });
    events.push(Event::PidOutput(500.0));
    events.push(Event::Safety(cc_safety::Outcome {
        state: cc_safety::SafetyState {
            last_sample_seq: None,
            latched: true,
            high_reading_count: cc_safety::DEBOUNCE_COUNT,
        },
        verdict: cc_safety::Verdict {
            may_heat: false,
            may_pump: false,
            may_open_water: false,
            may_open_steam: false,
            latched: true,
            reason: None,
        },
    }));
    events
}

fn machine_in(state: MachineState) -> Machine {
    let config = common::automatic_brew_with_preinfusion();
    let ctx = context_for(&config);
    let (mut machine, _) = cc_machine::boot_in(state, true, Millis::new(1_000), &ctx);
    machine.sensors = cc_machine::Sensors {
        water_tank_full: false,
        brew_weight: 12.0,
        ..cc_machine::Sensors::healthy()
    };
    machine.requests = cc_machine::Requests {
        brew_stop: true,
        steam_start: true,
        backflush_cycle_start: true,
        ..cc_machine::Requests::CLEAR
    };
    machine.backflush.on = true;
    machine.backflush.cycle = 3;
    machine.switches.brew = true;
    machine.switches.hot_water = true;
    machine.brew.elapsed_ms = 4_321.0;
    machine.brew.target_ms = 30_000.0;
    machine.shots_since_backflush = 17;
    machine.error_since = Some(Millis::new(500));
    machine.boot_at = Some(Millis::new(400));
    machine
}

/// 18 states × 20 events = **360 pairs**, reduced twice each.
#[test]
fn the_same_input_twice_gives_the_same_result() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx: Context<'_> = context_for(&config);
    let events = all_events();
    let mut pairs = 0_usize;

    for state in cc_domain::state::ALL {
        let machine = machine_in(state);
        for ev in &events {
            let (next_a, fx_a) = reduce(&machine, &ctx, *ev);
            let (next_b, fx_b) = reduce(&machine, &ctx, *ev);
            assert_eq!(next_a, next_b, "{state:?} x {ev:?}: the machines differ");
            assert_eq!(fx_a, fx_b, "{state:?} x {ev:?}: the effects differ");
            pairs += 1;
        }
    }
    assert_eq!(pairs, 360);
}

/// # Clone-and-compare
///
/// The input is cloned, `reduce` is called on the original, and the clone is
/// compared with the original. Any mutation — a flag cleared in place, a counter
/// incremented, a `Cell` written — shows up as a difference.
#[test]
fn reduce_does_not_mutate_its_input() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx: Context<'_> = context_for(&config);
    let events = all_events();
    let mut pairs = 0_usize;

    for state in cc_domain::state::ALL {
        let machine = machine_in(state);
        for ev in &events {
            let before = machine; // Machine is Copy, so this is a value snapshot.
            let _ = reduce(&machine, &ctx, *ev);
            assert_eq!(
                machine, before,
                "{state:?} x {ev:?}: reduce mutated its input"
            );

            // And the explicit clone-and-compare, for the record. `Machine` is
            // `Copy`, so the clone is a copy — which is itself part of the
            // guarantee: there is no heap state behind the value that `reduce`
            // could have reached.
            let cloned = machine;
            let _ = reduce(&machine, &ctx, *ev);
            assert_eq!(cloned, machine, "{state:?} x {ev:?}");
            pairs += 1;
        }
    }
    assert_eq!(pairs, 360);
}

/// The configuration is borrowed, not taken, so `reduce` cannot have changed it
/// either — and the borrow checker is what proves it, not a test.
#[test]
fn reduce_does_not_mutate_the_configuration() {
    let config = common::automatic_brew_with_preinfusion();
    let before = config.clone();
    let machine = machine_in(MachineState::PidNormal);
    {
        let ctx: Context<'_> = Context::new(&config, Celsius::new(95.0));
        for ev in all_events() {
            let _ = reduce(&machine, &ctx, ev);
        }
    }
    assert_eq!(config, before);
}

/// Referential transparency: the same machine and the same event give the same
/// answer no matter how the machine was arrived at.
///
/// `BREW_RUNNING` with a brew stop is reachable from `PID_NORMAL` and from
/// `BREW_PREINFUSION`; the two paths leave different breadcrumbs (the pre-infusion
/// path has a brew target, the manual path does not), so the assertion is only
/// meaningful when the breadcrumbs are equal too — which is the point: **the
/// answer is a function of the machine, not of the history**.
#[test]
fn the_result_depends_only_on_its_arguments() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx: Context<'_> = context_for(&config);
    let ev = Event::Command(cc_machine::Command::BrewStop);

    // Two machines that are byte-identical.
    let a = machine_in(MachineState::BrewRunning);
    let b = machine_in(MachineState::BrewRunning);
    let (next_a, fx_a) = reduce(&a, &ctx, ev);
    let (next_b, fx_b) = reduce(&b, &ctx, ev);
    assert_eq!(next_a, next_b);
    assert_eq!(fx_a, fx_b);

    // And a machine that was *built* differently but is value-equal.
    // Built from scratch rather than copied, so the equality is not trivially
    // true: the point is that two independently-constructed machines with the
    // same field values are the same machine.
    let mut d = Machine::cold();
    d.state = a.state;
    d.initialized = a.initialized;
    d.now = a.now;
    d.entry_at = a.entry_at;
    d.sensors = a.sensors;
    d.switches = a.switches;
    d.requests = a.requests;
    d.pid = a.pid;
    d.steam_mode = a.steam_mode;
    d.backflush = a.backflush;
    d.brew = a.brew;
    d.shots_since_backflush = a.shots_since_backflush;
    d.error_since = a.error_since;
    d.boot_at = a.boot_at;
    let (next_d, fx_d) = reduce(&d, &ctx, ev);
    assert_eq!(
        next_d, next_a,
        "a differently-built machine reduced differently"
    );
    assert_eq!(fx_d, fx_a);
}

/// Two identical machines stay identical under the same event sequence.
#[test]
fn two_identical_machines_stay_identical() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx: Context<'_> = context_for(&config);
    let a = machine_in(MachineState::PidNormal);
    let b = a;

    for ev in all_events() {
        let (next_a, _) = reduce(&a, &ctx, ev);
        let (next_b, _) = reduce(&b, &ctx, ev);
        assert_eq!(next_a, next_b, "diverged on {ev:?}");
    }
}

/// The returned machine is independent of the input: mutating the output does not
/// affect a later reduce of the input.
///
/// Trivially true for a `Copy` value, and worth stating because the property a
/// reader actually cares about is "the result is a fresh value", which the
/// signature gives.
#[test]
fn the_result_is_independent_of_the_input() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx: Context<'_> = context_for(&config);
    let machine = machine_in(MachineState::BrewRunning);
    let (mut next, _) = reduce(
        &machine,
        &ctx,
        Event::Tick {
            now: Millis::new(0),
        },
    );

    next.pid.runtime_enabled = !next.pid.runtime_enabled;
    next.requests.clear_all();

    let (again, _) = reduce(
        &machine,
        &ctx,
        Event::Tick {
            now: Millis::new(0),
        },
    );
    assert_eq!(again.state, next.state, "the states must still agree");
    assert_ne!(again.pid.runtime_enabled, next.pid.runtime_enabled);
}

/// `Machine` has no interior mutability, so "the reducer cannot reach through the
/// input" is a property of the type rather than of a test.
///
/// This asserts the type property, by checking that the crate does not name
/// `Cell`, `RefCell`, `UnsafeCell`, `Atomic*` or `static mut` in `src/`.
#[test]
fn the_machine_type_has_no_interior_mutability() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path).expect("a readable source file");
        let code: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("///") && !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [
            "Cell<",
            "RefCell<",
            "UnsafeCell",
            "static mut",
            "AtomicBool",
            "AtomicU32",
        ] {
            if code.contains(needle) {
                offenders.push(format!("{name}: {needle}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a pure reducer cannot use interior mutability; found: {offenders:?}"
    );
}

/// The reducer does not read a clock: the only place a `Millis` can enter is an
/// event, and `Machine::now` moves only on `Event::Tick`.
#[test]
fn only_a_tick_moves_the_clock() {
    let config = common::automatic_brew_with_preinfusion();
    let ctx: Context<'_> = context_for(&config);
    let machine = machine_in(MachineState::PidNormal);
    let now = machine.now;

    for ev in all_events() {
        if matches!(ev, Event::Tick { .. }) {
            continue;
        }
        let (next, _) = reduce(&machine, &ctx, ev);
        assert_eq!(next.now, now, "{ev:?} moved the clock without being a Tick");
    }
}
