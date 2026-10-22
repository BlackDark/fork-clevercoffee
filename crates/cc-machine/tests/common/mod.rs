//! Reducer fixtures with an explicit clock and a machine booted into `PID_NORMAL`.
//!
//! Assert ordered effects: counting calls cannot distinguish a pulse from a
//! steady output. `elapse` sends ticks; `advance_clock` only moves fixture time.

#![allow(dead_code)] // Each suite uses a different subset of the fixture.

use cc_config::Config;
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{guards, reduce, states, Context, Effect, Event, Machine, Sensors, SwitchId};

/// A context over `config`.
///
/// A free function rather than a method so that a caller can hold the context
/// (which borrows `config`) at the same time as a `&mut Machine` — the two are
/// disjoint fields, and a method taking `&self` would borrow both.
#[must_use]
pub fn context_for(config: &Config) -> Context<'_> {
    // The setpoint is the brew setpoint plus its offset, which is what process control reports
    // in every state except steam. The state machine only passes it through, so the value is not
    // load-bearing for any assertion here.
    #[allow(clippy::cast_possible_truncation)]
    // Justification: `brew.setpoint` is a configured `f64` and `Celsius` is an `f32`; the fixture
    // is not testing the cast, it is supplying a plausible setpoint.
    let raw = (config.brew.setpoint + config.brew.temp_offset) as f32;
    Context::new(config, Celsius::new(raw))
}

/// A machine, a configuration, and a clock, as one mutable value.
pub struct Harness {
    /// The configuration, as the machine reads it. Every field is public so a test can set
    /// exactly what it needs and nothing else.
    pub config: Config,
    /// The machine under test.
    pub machine: Machine,
    /// The current clock reading.
    pub now: u32,
}

impl Harness {
    /// A machine in [`MachineState::PidNormal`] with the PID and every operator switch
    /// enabled, which is the state most suites want to start from.
    #[must_use]
    pub fn new() -> Self {
        let mut h = Self {
            config: Config::default(),
            machine: Machine::cold(),
            now: 0,
        };
        h.config.pid.enabled = true;
        h.config.hardware.switches.brew.enabled = true;
        h.config.hardware.switches.steam.enabled = true;
        h.config.hardware.switches.power.enabled = true;
        h.config.hardware.switches.hot_water.enabled = true;
        h.machine = cc_machine::boot_in(
            MachineState::PidNormal,
            true,
            Millis::new(0),
            &context_for(&h.config),
        )
        .0;
        h
    }

    /// A machine in an arbitrary state, without running that state's entry.
    #[must_use]
    pub fn in_state(state: MachineState) -> Self {
        let mut h = Self::new();
        h.enter_state_without_effects(state);
        h
    }

    /// Force the current state without running `on_entry`.
    ///
    /// The state and the entry time move together, so `state_elapsed_ms` reads 0 from the moment
    /// the test sets the state. A test that wants the entry effects calls [`Harness::on_entry`]
    /// itself.
    pub fn enter_state_without_effects(&mut self, state: MachineState) {
        self.machine.state = state;
        self.machine.entry_at = Millis::new(self.now);
    }

    /// A context over this harness's configuration. See [`context_for`] for the setpoint.
    #[must_use]
    pub fn ctx(&self) -> Context<'_> {
        context_for(&self.config)
    }

    /// Reduce one event.
    ///
    /// The reducer's own return type is [`cc_machine::Effects`], a
    /// fixed-capacity `heapless::Vec`, and the conversion to `Vec` happens here
    /// on purpose. Tests are not the hot path — they are allowed to use the
    /// allocator — and the helpers below need a list that is *not* bounded by
    /// `MAX_EFFECTS_PER_EVENT`: [`Harness::send_all`] concatenates the effects
    /// of several events, and [`transition_window`] slices one. Widening the
    /// firmware's ceiling to serve a test fixture is the wrong direction.
    pub fn send(&mut self, ev: Event) -> Vec<Effect> {
        let ctx = self.ctx();
        let (next, fx) = reduce(&self.machine, &ctx, ev);
        self.machine = next;
        fx.into_iter().collect()
    }

    /// Reduce several events in order and concatenate the effects.
    pub fn send_all(&mut self, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
        let mut out = Vec::new();
        for ev in events {
            out.extend(self.send(ev));
        }
        out
    }

    /// Advance the clock and run one control-loop iteration.
    pub fn tick(&mut self) -> Vec<Effect> {
        let now = self.now;
        self.send(Event::Tick {
            now: Millis::new(now),
        })
    }

    /// Advance the clock by `delta_ms` and run one control-loop iteration.
    pub fn elapse(&mut self, delta_ms: u32) -> Vec<Effect> {
        self.now = self.now.wrapping_add(delta_ms);
        self.tick()
    }

    /// Advance the clock **without** running a tick.
    ///
    /// `Event::Tick` is the only thing that moves `Machine::now`, so a test that calls
    /// `states::update` directly has to move it explicitly or every elapsed-time comparison reads
    /// 0.
    pub fn advance_clock(&mut self, delta_ms: u32) {
        self.now = self.now.wrapping_add(delta_ms);
        self.machine.now = cc_domain::units::Millis::new(self.now);
    }

    /// Advance the clock to `absolute_ms` and run one control-loop iteration.
    pub fn elapse_to(&mut self, absolute_ms: u32) -> Vec<Effect> {
        self.now = absolute_ms;
        self.tick()
    }

    /// Press a switch: the level goes high and the handler runs, in one event.
    pub fn press(&mut self, switch: SwitchId) -> Vec<Effect> {
        self.send(Event::ButtonPressed {
            switch,
            long_press: false,
        })
    }

    /// Press a switch with the hardware long-press flag set.
    pub fn long_press(&mut self, switch: SwitchId) -> Vec<Effect> {
        self.send(Event::ButtonPressed {
            switch,
            long_press: true,
        })
    }

    /// Release a switch: the level goes low and the handler runs.
    pub fn release(&mut self, switch: SwitchId) -> Vec<Effect> {
        self.send(Event::ButtonReleased { switch })
    }

    /// Run a state's entry hook and collect the effects.
    pub fn on_entry(&mut self, state: MachineState) -> Vec<Effect> {
        let ctx = context_for(&self.config);
        states::on_entry(state, &mut self.machine, &ctx)
            .into_iter()
            .collect()
    }

    /// Run a state's exit hook and collect the effects.
    pub fn on_exit(&mut self, state: MachineState) -> Vec<Effect> {
        let ctx = context_for(&self.config);
        states::on_exit(state, &mut self.machine, &ctx)
            .into_iter()
            .collect()
    }

    /// Run a state's update hook and collect the effects.
    pub fn update(&mut self, state: MachineState) -> Vec<Effect> {
        let ctx = context_for(&self.config);
        states::update(state, &mut self.machine, &ctx)
            .into_iter()
            .collect()
    }

    /// Ask a state for its next state, ignoring the global guards.
    pub fn check_specific(&mut self, state: MachineState) -> Option<MachineState> {
        let ctx = context_for(&self.config);
        states::check_specific(state, &mut self.machine, &ctx)
    }

    /// The guards *then* the specific rules, in the order the reducer applies them.
    pub fn check_transitions(&mut self, state: MachineState) -> Option<MachineState> {
        let guard = guards::global_guard(&self.machine);
        if let Some(target) = guards::guard_destination(guard) {
            return Some(target);
        }
        self.check_specific(state)
    }

    /// The destination a tick will actually move to: a self-transition is reported as `None`.
    pub fn next_state_this_tick(&mut self) -> Option<MachineState> {
        let current = self.machine.state;
        self.check_transitions(current).filter(|t| *t != current)
    }

    /// The PID state this harness's configuration selects.
    #[must_use]
    pub fn pid_state(&self) -> MachineState {
        states::pid_state(&context_for(&self.config))
    }

    /// The current state.
    #[must_use]
    pub fn state(&self) -> MachineState {
        self.machine.state
    }

    /// Whether a request flag is set.
    #[must_use]
    pub fn requested(&self, request: cc_machine::machine::Request) -> bool {
        self.machine.requests.get(request)
    }

    /// A healthy sensor sample, as `Sensors::healthy`.
    #[must_use]
    pub fn sensors() -> Sensors {
        Sensors::healthy()
    }

    /// A sensor sample at a given temperature.
    #[must_use]
    pub fn sensors_at(temperature: f32) -> Sensors {
        Sensors {
            temperature: Celsius::new(temperature),
            ..Sensors::healthy()
        }
    }
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

/// A configuration with automatic brewing, a 3 s pre-infusion, a 2 s pause, and a 25 s `by_time`
/// target so that `BREW_RUNNING` has something to stop on.
#[must_use]
pub fn automatic_brew_with_preinfusion() -> Config {
    let mut config = Config::default();
    config.pid.enabled = true;
    config.hardware.switches.brew.enabled = true;
    config.hardware.switches.steam.enabled = true;
    config.hardware.switches.power.enabled = true;
    config.hardware.switches.hot_water.enabled = true;
    config.brew.mode = cc_domain::process::BrewMode::Automatic;
    config.brew.pre_infusion.enabled = true;
    config.brew.pre_infusion.time = 3.0;
    config.brew.pre_infusion.pause = 2.0;
    config.brew.by_time.enabled = true;
    config.brew.by_time.target_time = 25.0;
    config
}

/// The number of effects in `fx` that are actuator writes.
#[must_use]
pub fn actuator_writes(fx: &[Effect]) -> usize {
    fx.iter().filter(|e| e.is_actuator_write()).count()
}

/// Whether `fx` contains `effect` at least once.
#[must_use]
pub fn has(fx: &[Effect], effect: Effect) -> bool {
    fx.contains(&effect)
}

/// How many times `effect` appears in `fx`.
#[must_use]
pub fn count(fx: &[Effect], effect: Effect) -> usize {
    fx.iter().filter(|e| **e == effect).count()
}

/// The index of the first occurrence of `effect`, or `None`.
///
/// Position is part of the contract: two effects emitted in one tick have a fixed order, and a
/// test that only checks membership cannot tell a pump that starts after a valve opens from one
/// that starts before.
#[must_use]
pub fn index_of(fx: &[Effect], effect: Effect) -> Option<usize> {
    fx.iter().position(|e| *e == effect)
}

/// The sub-slice of `fx` that the transition from `old` to `new` produced: from
/// the `ExitState(old)` marker up to the next `ExitState` marker (or the end of
/// the list).
///
/// That is `on_exit(old)` + `EnterState(new)` + `on_entry(new)`, in order. `EnterState(new)`
/// sits *before* the new state's entry effects, so the window has to run past it.
#[must_use]
pub fn transition_window(fx: &[Effect], old: MachineState, new: MachineState) -> Vec<Effect> {
    let start = index_of(fx, Effect::ExitState(old));
    let Some(start) = start else {
        return Vec::new();
    };
    if index_of(&fx[start..], Effect::EnterState(new)).is_none() {
        return Vec::new();
    }
    let end = fx[start + 1..]
        .iter()
        .position(|e| matches!(e, Effect::ExitState(_)))
        .map_or(fx.len(), |i| start + i);
    fx[start..end].to_vec()
}

/// The part of a [`transition_window`] **before** the new state's entry effects: the old state's
/// exit effects plus the `EnterState` marker.
#[must_use]
pub fn exit_half(win: &[Effect], new: MachineState) -> Vec<Effect> {
    let end = index_of(win, Effect::EnterState(new)).map_or(win.len(), |i| i + 1);
    win[..end.min(win.len())].to_vec()
}

/// The part of a [`transition_window`] **after** the `EnterState` marker: the new state's entry
/// effects.
#[must_use]
pub fn entry_half(win: &[Effect], new: MachineState) -> Vec<Effect> {
    match index_of(win, Effect::EnterState(new)) {
        Some(i) => win[i + 1..].to_vec(),
        None => Vec::new(),
    }
}

/// Exact comparison of a millisecond value.
///
/// `assert_eq!` on `f64` is denied by `clippy::float_cmp`. Every value compared
/// through this helper is an exact multiple of 1000 built from a small literal, so
/// the arithmetic is exact in binary floating point and a tolerance would hide a
/// real mistake rather than paper over one.
#[allow(clippy::float_cmp)]
// Justification: every value compared here is an exact multiple of 1000 derived
// from a small integer literal, so the comparison is exact by construction.
pub fn assert_ms(actual: f64, expected: f64) {
    assert_eq!(actual, expected, "millisecond value");
}

/// Exact comparison of a weight in grams.
#[allow(clippy::float_cmp)]
// Justification: as `assert_ms` — the weights compared are whole grams or tenths
// of a gram, both exact in `f32`.
pub fn assert_g(actual: f32, expected: f32) {
    assert_eq!(actual, expected, "weight in grams");
}
