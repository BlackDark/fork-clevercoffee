//! Reducer-owned memory, without interior mutability.
//!
//! [`Machine::error_since`] resets on every error-state entry; retaining a single
//! struct must not retain a previous entry's recovery deadline.

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};

use crate::event::Sensors;

/// The eleven action request flags.
///
/// They are a *group* on purpose: the drain operations run over the whole set -- "on state
/// entry, drain all action flags that cannot be acted upon" -- and both `clear_all` and
/// `clear_stale_stops` enumerate the list. One struct, two methods, and the compiler catches a
/// flag that forgets to be drained.
// Eleven booleans, because the two *group* operations that matter -- drain everything, drain
// only the stops -- are about the group. Modelling eleven independent states instead would
// invent structure the firmware does not have.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Requests {
    /// Start a brew.
    pub brew_start: bool,
    /// Stop the running brew.
    pub brew_stop: bool,
    /// Start steam.
    pub steam_start: bool,
    /// Stop steam.
    pub steam_stop: bool,
    /// Start a manual flush.
    pub manual_flush_start: bool,
    /// Stop the manual flush.
    pub manual_flush_stop: bool,
    /// Enter backflush mode.
    pub backflush_enter: bool,
    /// Start the next backflush cycle.
    pub backflush_cycle_start: bool,
    /// Stop the backflush cycle.
    pub backflush_stop: bool,
    /// Enter standby.
    pub standby: bool,
    /// Leave standby.
    pub normal_operation: bool,
}

/// One of the eleven request flags, as a value.
///
/// `Requests` is a struct of eleven `bool`s because the *drain* operations (`clear_all`,
/// `clear_stale_stops`) are about the group. Setting one flag individually is a different
/// concern and needs a different shape: not every flag re-arms the standby countdown, and
/// which ones do is a property of the individual flag rather than of the group. So the setters
/// take a `Request`, and [`Request::resets_standby_timer`] is the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Request {
    /// Start a brew.
    BrewStart,
    /// Stop the running brew.
    BrewStop,
    /// Start steam.
    SteamStart,
    /// Stop steam.
    SteamStop,
    /// Start a manual flush.
    ManualFlushStart,
    /// Stop the manual flush.
    ManualFlushStop,
    /// Enter backflush mode. The mode-on half of `POST /api/backflush`'s toggle.
    BackflushEnter,
    /// Start the next backflush cycle.
    BackflushCycleStart,
    /// Stop the backflush cycle.
    BackflushStop,
    /// Enter standby.
    Standby,
    /// Leave standby.
    NormalOperation,
}

impl Request {
    /// Every request, for the exhaustive `state x event` table.
    pub const ALL: [Request; 11] = [
        Request::BrewStart,
        Request::BrewStop,
        Request::SteamStart,
        Request::SteamStop,
        Request::ManualFlushStart,
        Request::ManualFlushStop,
        Request::BackflushEnter,
        Request::BackflushCycleStart,
        Request::BackflushStop,
        Request::Standby,
        Request::NormalOperation,
    ];

    /// Whether setting this flag to `true` also re-arms the standby countdown.
    ///
    /// | yes | no |
    /// | --- | --- |
    /// | [`Request::BackflushEnter`] | [`Request::BrewStop`] |
    /// | [`Request::BackflushCycleStart`] | [`Request::SteamStop`] |
    /// | [`Request::BackflushStop`] | [`Request::ManualFlushStop`] |
    /// | [`Request::BrewStart`] | [`Request::ManualFlushStart`] |
    /// | [`Request::SteamStart`] | [`Request::Standby`] |
    /// | [`Request::NormalOperation`] | |
    ///
    /// A start is an operator asking the machine to do something, so it counts as activity; a
    /// stop is usually the machine finishing by itself, so it does not. [`Request::Standby`]
    /// does not either -- asking the machine to sleep is not activity, and it would defeat the
    /// sleep if it were.
    ///
    /// [`Request::ManualFlushStart`] is the odd one out: it does not re-arm the countdown even
    /// though it starts something. That is harmless, because a manual flush is only reachable
    /// from backflush mode, where the backflush requests have already re-armed it.
    #[must_use]
    pub const fn resets_standby_timer(self) -> bool {
        match self {
            Self::BackflushEnter
            | Self::BackflushCycleStart
            | Self::BackflushStop
            | Self::BrewStart
            | Self::SteamStart
            | Self::NormalOperation => true,
            Self::BrewStop
            | Self::SteamStop
            | Self::ManualFlushStop
            | Self::ManualFlushStart
            | Self::Standby => false,
        }
    }
}

impl Requests {
    /// Read one flag.
    #[must_use]
    pub const fn get(&self, request: Request) -> bool {
        match request {
            Request::BrewStart => self.brew_start,
            Request::BrewStop => self.brew_stop,
            Request::SteamStart => self.steam_start,
            Request::SteamStop => self.steam_stop,
            Request::ManualFlushStart => self.manual_flush_start,
            Request::ManualFlushStop => self.manual_flush_stop,
            Request::BackflushEnter => self.backflush_enter,
            Request::BackflushCycleStart => self.backflush_cycle_start,
            Request::BackflushStop => self.backflush_stop,
            Request::Standby => self.standby,
            Request::NormalOperation => self.normal_operation,
        }
    }

    /// Write one flag.
    pub const fn set(&mut self, request: Request, value: bool) {
        match request {
            Request::BrewStart => self.brew_start = value,
            Request::BrewStop => self.brew_stop = value,
            Request::SteamStart => self.steam_start = value,
            Request::SteamStop => self.steam_stop = value,
            Request::ManualFlushStart => self.manual_flush_start = value,
            Request::ManualFlushStop => self.manual_flush_stop = value,
            Request::BackflushEnter => self.backflush_enter = value,
            Request::BackflushCycleStart => self.backflush_cycle_start = value,
            Request::BackflushStop => self.backflush_stop = value,
            Request::Standby => self.standby = value,
            Request::NormalOperation => self.normal_operation = value,
        }
    }

    /// Nothing requested.
    pub const CLEAR: Self = Self {
        brew_start: false,
        brew_stop: false,
        steam_start: false,
        steam_stop: false,
        manual_flush_start: false,
        manual_flush_stop: false,
        backflush_enter: false,
        backflush_cycle_start: false,
        backflush_stop: false,
        standby: false,
        normal_operation: false,
    };

    /// Discard stale action requests while preserving `standby`.
    ///
    /// PID-disabled states must not replay actions on enable. Sleep is different:
    /// clearing it before transition checks would make `/api/sleep` unreachable.
    pub fn clear_all(&mut self) {
        let standby = self.standby;
        *self = Self::CLEAR;
        self.standby = standby;
    }

    /// Drain the four stop flags only, keeping the start flags.
    ///
    /// A start flag is an intentional wake signal: `brew_start` arriving while the machine is
    /// asleep has to survive until the machine is awake to act on it, and a `STANDBY` transition
    /// must not swallow it. A stop flag names a machine that is running, so a stale one is
    /// always wrong.
    pub fn clear_stale_stops(&mut self) {
        self.brew_stop = false;
        self.steam_stop = false;
        self.manual_flush_stop = false;
        self.backflush_stop = false;
    }

    /// Whether any flag is set. Used by the exhaustive table's assertions.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.brew_start
            || self.brew_stop
            || self.steam_start
            || self.steam_stop
            || self.manual_flush_start
            || self.manual_flush_stop
            || self.backflush_enter
            || self.backflush_cycle_start
            || self.backflush_stop
            || self.standby
            || self.normal_operation
    }
}

/// The current level and long-press flag of each switch.
///
/// The levels are machine state rather than a lookup into a polling object, which is what lets
/// the PID state and the steam state read the water switch while they update.
// Four switch levels plus two latched long-press flags. The levels are not a state machine:
// any of the sixteen combinations is reachable.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwitchLevels {
    /// `hardware.switches.brew`. `true` is HIGH, i.e. "activated".
    pub brew: bool,
    /// `hardware.switches.steam`.
    pub steam: bool,
    /// `hardware.switches.power`.
    pub power: bool,
    /// `hardware.switches.hot_water`.
    pub hot_water: bool,
    /// Whether the brew switch reported a long press, sampled on the edge.
    ///
    /// Consulted only in `BACKFLUSH_IDLE`, to choose between "start a backflush cycle" and
    /// "start a manual flush". It is a property of the *press*, not of the level, so it is
    /// latched on the rising edge and never cleared by a release.
    pub brew_long_press: bool,
    /// Whether the power switch reported a long press, same latching.
    ///
    /// A reboot decision needs it *after* the press edge is gone, which is why
    /// [`crate::handlers::long_press_reboot`] runs on every tick rather than on the edge.
    pub power_long_press: bool,
}

impl SwitchLevels {
    /// Every switch released, no long press.
    pub const ALL_RELEASED: Self = Self {
        brew: false,
        steam: false,
        power: false,
        hot_water: false,
        brew_long_press: false,
        power_long_press: false,
    };

    /// The level of one switch.
    #[must_use]
    pub const fn level(&self, switch: crate::event::SwitchId) -> bool {
        match switch {
            crate::event::SwitchId::Brew => self.brew,
            crate::event::SwitchId::Steam => self.steam,
            crate::event::SwitchId::Power => self.power,
            crate::event::SwitchId::HotWater => self.hot_water,
        }
    }

    /// The level of one switch, as an assignment.
    pub fn set_level(&mut self, switch: crate::event::SwitchId, level: bool) {
        match switch {
            crate::event::SwitchId::Brew => self.brew = level,
            crate::event::SwitchId::Steam => self.steam = level,
            crate::event::SwitchId::Power => self.power = level,
            crate::event::SwitchId::HotWater => self.hot_water = level,
        }
    }
}

/// The brew timer: what the display shows and what `BREW_RUNNING` uses to stop by time.
///
/// The timer is written from four different states, each with a different base, so the base is
/// a function of the state and the elapsed time is added to it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrewProgress {
    /// Total elapsed brew time, including pre-infusion.
    pub elapsed_ms: f64,
    /// The target brew time; `0.0` means the shot does not end by time.
    pub target_ms: f64,
}

/// The two PID flags, which are not the same flag.
///
/// Getting this distinction wrong is what `test_state_flow_integration`'s
/// `EmergencyRecovery_RestoresPidFromConfig` exists for, so it gets its own struct with its own
/// documentation rather than two bare `bool`s in [`Machine`].
///
/// | field | set by | read by |
/// | --- | --- | --- |
/// | `runtime_enabled` | entry and exit of standby, `PID_DISABLED` and emergency stop | the global guard, the PID states |
/// | `mode_enabled` | process control, every iteration | the heater gate |
///
/// They are separate because they answer different questions. Emergency stop forces the
/// runtime flag off and its exit restores it from the configuration; the mode flag is
/// recomputed from the state and the sensors on every iteration, so the two can legitimately
/// disagree within one iteration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pid {
    /// The state machine's view: may the PID run at all?
    pub runtime_enabled: bool,
    /// The heater gate: has process control permitted the heater?
    pub mode_enabled: bool,
    /// Whether the `brew.pid_delay` window is holding the PID off.
    pub brew_disabled: bool,
    /// The last PID output, in milliseconds of on-time per 1000 ms window.
    pub output: f32,
}

impl Pid {
    /// The state a freshly-booted machine is in: everything off.
    pub const COLD: Self = Self {
        runtime_enabled: false,
        mode_enabled: false,
        brew_disabled: false,
        output: 0.0,
    };
}

/// Backflush mode and cycle counter.
///
/// The counter starts at **1**, not 0, and a cycle advance from
/// [`resolve_cycle_advance`](crate::backflush::resolve_cycle_advance)`(1, 5)` starts the next
/// cycle -- so the loop runs cycles 1 through 5 and the finished state is reached after the
/// fifth flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backflush {
    /// Whether backflush mode is on.
    pub on: bool,
    /// The current cycle, counting from 1.
    pub cycle: i32,
}

impl Default for Backflush {
    fn default() -> Self {
        Self {
            on: false,
            cycle: 1,
        }
    }
}

/// The standby countdown: how long until standby, and how long the panel stays lit after it.
///
/// # Why "started" is an `Option`
///
/// A sentinel start time of `0` is indistinguishable from the clock reading `0`, which is the
/// first millisecond after boot and every millisecond in a test harness -- so standby could
/// never fire in either. [`Option`] makes "started" unambiguous, and the difference can only
/// make standby work *earlier*, never later, so it is not a safety regression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StandbyTimer {
    /// When the countdown was last armed, or `None` if it never was.
    pub started_at: Option<Millis>,
    /// How long until standby, as last recomputed.
    pub remaining_ms: u32,
    /// How much longer the panel stays lit after standby begins.
    pub display_off_remaining_ms: u32,
    /// The once-a-second update gate.
    pub last_update: Option<Millis>,
}

impl StandbyTimer {
    /// Arm the countdown, and the display countdown with it, from the same instant.
    ///
    /// A zero `timeout_ms` is a no-op rather than an immediate standby, and it is how "standby
    /// is disabled" reaches this type:
    /// [`Context::standby_timeout_ms`](crate::Context::standby_timeout_ms) is `0` when
    /// `standby.enabled` is unset, so the disabled case is the caller's to express rather than
    /// a hidden configuration lookup here.
    ///
    /// Arming both countdowns from one instant is what makes the two deadlines `standby.time`
    /// and `standby.time + STANDBY_DISPLAY_OFF_MS`.
    pub fn reset(&mut self, now: Millis, timeout_ms: u32) {
        if timeout_ms == 0 {
            return;
        }
        self.remaining_ms = timeout_ms;
        self.display_off_remaining_ms = crate::timing::STANDBY_DISPLAY_OFF_MS;
        self.started_at = Some(now);
        self.last_update = Some(now);
    }

    /// "Standby now": expire the standby countdown and nothing else.
    ///
    /// The display countdown keeps its remaining time, so the panel stays lit for the rest of
    /// it instead of going dark at once: an operator who asks the machine to sleep expects the
    /// screen to go out in its own time.
    pub const fn expire_now(&mut self) {
        self.remaining_ms = 0;
    }

    /// Whether the panel should go dark: the standby countdown has run out **and** the display
    /// countdown has, **and** the timer was started.
    #[must_use]
    pub const fn should_turn_off_display(&self) -> bool {
        self.remaining_ms == 0 && self.display_off_remaining_ms == 0 && self.started_at.is_some()
    }

    /// Whether to enter standby: enabled **and** the countdown has run out **and** the timer
    /// was started.
    #[must_use]
    pub const fn should_enter(&self, enabled: bool) -> bool {
        enabled && self.remaining_ms == 0 && self.started_at.is_some()
    }
}

/// The whole machine, as a value.
// Four booleans among twenty-two fields. Nesting them into sub-structs would obscure exactly
// the cross-field invariants the guards read: the emergency check reads `safety`, and the
// heater gate reads `state`, the configuration and `pid.brew_disabled` together.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Machine {
    /// The current state.
    pub state: MachineState,
    /// Whether the machine has been initialised.
    ///
    /// The reducer ignores every event until this is true: an uninitialised machine must not
    /// act on a request that arrived before it had a state to act in.
    pub initialized: bool,
    /// The last clock reading the shell supplied via [`Event::Tick`](crate::Event::Tick).
    ///
    /// # Why it is not a clock
    ///
    /// The clock is read in a dozen places in the state logic, and a reading taken inside the
    /// reducer would make every timed decision untestable and the function impure. So the
    /// reading arrives in the event, and every timed decision is a comparison against it.
    pub now: Millis,
    /// When the current state was entered.
    pub entry_at: Millis,
    /// The latest [`Event::SensorUpdated`](crate::Event::SensorUpdated) sample.
    pub sensors: Sensors,
    /// Current switch levels.
    pub switches: SwitchLevels,
    /// The eleven action request flags.
    pub requests: Requests,
    /// The two PID flags and the last output.
    pub pid: Pid,
    /// The S1-S5 latch state, mirrored from `cc_safety`.
    pub safety: cc_safety::SafetyState,
    /// The last `cc-safety` verdict, used to gate the actuator effects.
    pub verdict: cc_safety::Verdict,
    /// Whether steam mode is active.
    pub steam_mode: bool,
    /// Whether this steam run is the first since the machine was last in standby.
    ///
    /// Set on entry to `STEAM_RUNNING` and cleared on its exit and on entry to `STANDBY`, so it
    /// names a run that starts from a cold boiler rather than from a boiler the operator has
    /// already steamed with.
    pub steam_first_on: bool,
    /// Backflush mode and cycle counter.
    pub backflush: Backflush,
    /// The brew timer.
    pub brew: BrewProgress,
    /// The standby countdown.
    pub standby: StandbyTimer,
    /// When the current error state was entered.
    ///
    /// A state object in a class hierarchy gets a fresh start time for free, because a
    /// transition constructs a new instance. Here [`crate::states::on_entry`] is the only
    /// writer, and the value is cleared on exit so a stale one can never be read.
    pub error_since: Option<Millis>,
    /// Shots since the last backflush.
    ///
    /// **The reducer is the only writer**, and that is a decision rather than an accident. The
    /// counter is touched from two directions -- a qualified brew finishing, and a backflush
    /// starting -- and both are state *entries*, which is where [`crate::states::on_entry`]
    /// runs. So the qualification rule is applied here, where the value lives, and
    /// [`Effect::RecordBrew`](crate::effect::Effect::RecordBrew) carries the answer rather than
    /// the inputs.
    ///
    /// The alternative -- an applier that owns the rule -- is what the firmware did first, and
    /// the rule was never applied by anyone: the counter stayed at 0 and `/api/status` reported
    /// `shotsSinceBackflush: 0` for the life of the firmware. A rule that has to be *wired up*
    /// can be forgotten; one that runs where its value lives cannot.
    pub shots_since_backflush: i32,
    /// Whether hot-water activity has been marked since the countdown was last re-armed.
    pub hot_water_activity: bool,
    /// When the machine started running, from which presses are ignored for
    /// [`timing::POWER_SWITCH_SETTLE_MS`](crate::timing::POWER_SWITCH_SETTLE_MS).
    pub boot_at: Option<Millis>,
    /// When the current long press began, if one is being tracked. `None` means the switch
    /// is not being held.
    pub power_press_started_at: Option<Millis>,
    /// When the brew pump was last started, against which
    /// [`timing::BREW_PUMP_TIMEOUT_MS`](crate::timing::BREW_PUMP_TIMEOUT_MS) is checked.
    pub brew_pump_started_at: Option<Millis>,
    /// When the hot-water pump was last started, against which
    /// [`timing::HOT_WATER_PUMP_TIMEOUT_MS`](crate::timing::HOT_WATER_PUMP_TIMEOUT_MS) is
    /// checked.
    pub hot_water_pump_started_at: Option<Millis>,
}

impl Machine {
    /// A machine that has not been initialised: `INIT`, no effects, no clock.
    ///
    /// The reducer has no null state, so "there is no machine yet" is `INIT` plus the
    /// [`initialized`](Self::initialized) flag, and every event is a no-op until
    /// [`boot`](crate::boot).
    #[must_use]
    pub const fn cold() -> Self {
        Self {
            state: MachineState::Init,
            initialized: false,
            now: Millis::ZERO,
            entry_at: Millis::ZERO,
            sensors: Sensors::healthy(),
            switches: SwitchLevels::ALL_RELEASED,
            requests: Requests::CLEAR,
            pid: Pid::COLD,
            safety: cc_safety::SafetyState::CLEAR,
            verdict: cc_safety::Verdict {
                may_heat: false,
                may_pump: false,
                may_open_water: false,
                may_open_steam: false,
                latched: false,
                reason: None,
            },
            steam_mode: false,
            steam_first_on: false,
            backflush: Backflush {
                on: false,
                cycle: 1,
            },
            brew: BrewProgress {
                elapsed_ms: 0.0,
                target_ms: 0.0,
            },
            standby: StandbyTimer {
                started_at: None,
                remaining_ms: 0,
                display_off_remaining_ms: 0,
                last_update: None,
            },
            error_since: None,
            shots_since_backflush: 0,
            hot_water_activity: false,
            boot_at: None,
            power_press_started_at: None,
            brew_pump_started_at: None,
            hot_water_pump_started_at: None,
        }
    }

    /// Milliseconds since the current state was entered.
    #[must_use]
    pub const fn state_elapsed_ms(&self) -> u32 {
        self.now.since(self.entry_at).raw()
    }

    /// Whether the current state has been active for at least `timeout_ms` -- `>=`, not `>`.
    #[must_use]
    pub const fn state_timeout_elapsed(&self, timeout_ms: u32) -> bool {
        self.state_elapsed_ms() >= timeout_ms
    }

    /// Whether the S1-S5 latch is set.
    #[must_use]
    pub const fn is_emergency_stop(&self) -> bool {
        self.safety.latched
    }

    /// Whether the PID may run, from the state machine's point of view.
    #[must_use]
    pub const fn is_pid_runtime_enabled(&self) -> bool {
        self.pid.runtime_enabled
    }

    /// Whether the water tank is full.
    #[must_use]
    pub const fn is_water_tank_full(&self) -> bool {
        self.sensors.water_tank_full
    }

    /// Whether any enabled sensor has reported a fault.
    #[must_use]
    pub const fn has_sensor_error(&self) -> bool {
        self.sensors.has_sensor_error()
    }

    /// Whether backflush mode is on.
    #[must_use]
    pub const fn is_backflush_mode_active(&self) -> bool {
        self.backflush.on
    }

    /// The current brew weight, in grams.
    #[must_use]
    pub const fn brew_weight(&self) -> f32 {
        self.sensors.brew_weight
    }

    /// The latest temperature reading.
    #[must_use]
    pub const fn temperature(&self) -> Celsius {
        self.sensors.temperature
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn a_cold_machine_is_uninitialised_and_in_init() {
        let m = Machine::cold();
        assert!(!m.initialized);
        assert_eq!(m.state, MachineState::Init);
        assert_eq!(m.now, Millis::ZERO);
        assert_eq!(m.entry_at, Millis::ZERO);
    }

    #[test]
    fn a_cold_machine_starts_with_backflush_cycle_one() {
        // Starting at 0 would make a five-cycle backflush run 0..=4: still five cycles, but
        // the counter reported to the display would be off by one throughout.
        assert_eq!(Machine::cold().backflush.cycle, 1);
        assert!(!Machine::cold().backflush.on);
    }

    /// Ten of the eleven flags drain; `standby` survives.
    ///
    /// The drain runs before the transition check, so clearing `standby` here would destroy a
    /// sleep request before anything could read it. It is the one flag that asks the machine to
    /// *go somewhere* rather than to start doing something, which is why the drain spares it.
    /// See [`Requests::clear_all`].
    #[test]
    fn clear_all_drains_the_action_flags_and_spares_standby() {
        let mut r = Requests {
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
        assert!(r.any());
        r.clear_all();

        // The ten action flags go.
        assert!(!r.brew_start);
        assert!(!r.brew_stop);
        assert!(!r.steam_start);
        assert!(!r.steam_stop);
        assert!(!r.manual_flush_start);
        assert!(!r.manual_flush_stop);
        assert!(!r.backflush_enter);
        assert!(!r.backflush_cycle_start);
        assert!(!r.backflush_stop);
        assert!(!r.normal_operation);

        // …and `standby` survives, or `PID_DISABLED` is a state the machine can
        // only leave on a timer.
        assert!(
            r.standby,
            "standby is spared, or POST /api/sleep is ignored while the PID is off"
        );
        assert!(r.any(), "a set standby request is still a set request");
    }

    #[test]
    fn clear_stale_stops_keeps_the_wake_triggers() {
        // ADR-0003: "Never drain wake-up signals".
        let mut r = Requests {
            brew_start: true,
            steam_start: true,
            backflush_enter: true,
            brew_stop: true,
            steam_stop: true,
            manual_flush_stop: true,
            backflush_stop: true,
            normal_operation: true,
            ..Requests::CLEAR
        };
        r.clear_stale_stops();
        assert!(
            r.brew_start,
            "brew start is a wake trigger and must survive"
        );
        assert!(
            r.steam_start,
            "steam start is a wake trigger and must survive"
        );
        assert!(r.backflush_enter);
        assert!(r.normal_operation);
        assert!(!r.brew_stop);
        assert!(!r.steam_stop);
        assert!(!r.manual_flush_stop);
        assert!(!r.backflush_stop);
    }

    #[test]
    fn clear_stale_stops_drains_exactly_the_four_stop_flags() {
        let mut r = Requests {
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
        r.clear_stale_stops();
        assert_eq!(
            r,
            Requests {
                brew_start: true,
                steam_start: true,
                manual_flush_start: true,
                backflush_enter: true,
                backflush_cycle_start: true,
                standby: true,
                normal_operation: true,
                ..Requests::CLEAR
            }
        );
    }

    #[test]
    fn state_timeout_uses_greater_or_equal() {
        // `>=`, so a state that has been active for exactly the timeout has timed out.
        let mut m = Machine::cold();
        m.now = Millis::new(3_000);
        m.entry_at = Millis::new(0);
        assert!(m.state_timeout_elapsed(3_000));
        assert!(!m.state_timeout_elapsed(3_001));
    }

    #[test]
    fn state_elapsed_wraps_across_the_u32_rollover() {
        // The subtraction wraps at `u32::MAX`: a state entered 100 ms before the rollover has
        // been active 100 ms, not four billion.
        let mut m = Machine::cold();
        m.entry_at = Millis::new(u32::MAX - 99);
        m.now = Millis::new(0);
        assert_eq!(m.state_elapsed_ms(), 100);
    }

    #[test]
    fn standby_only_fires_when_enabled_started_and_expired() {
        let mut t = StandbyTimer::default();
        assert!(!t.should_enter(true), "never started");
        t.started_at = Some(Millis::new(1));
        t.remaining_ms = 5_000;
        assert!(!t.should_enter(true), "not expired");
        t.expire_now();
        assert!(t.should_enter(true), "expired and started");
        assert!(!t.should_enter(false), "but standby is disabled");
    }

    #[test]
    fn standby_reset_is_a_no_op_when_disabled() {
        // A zero timeout means standby is disabled, and a disabled standby is not started by
        // a reset -- otherwise a disabled timeout would read as an immediate standby.
        let mut t = StandbyTimer::default();
        t.reset(Millis::new(1_000), 0);
        assert_eq!(t, StandbyTimer::default());
    }

    #[test]
    fn standby_reset_rearms_the_countdown() {
        let mut t = StandbyTimer {
            started_at: Some(Millis::new(1)),
            remaining_ms: 0,
            display_off_remaining_ms: 0,
            last_update: Some(Millis::new(1)),
        };
        t.reset(Millis::new(9_000), 60_000);
        assert_eq!(t.remaining_ms, 60_000);
        assert_eq!(t.started_at, Some(Millis::new(9_000)));
        assert!(!t.should_enter(true));
    }

    #[test]
    fn resetting_also_rearms_the_display_countdown() {
        // The same reset arms both countdowns, which is what keeps the panel lit for ten
        // minutes after standby begins.
        let mut t = StandbyTimer::default();
        t.reset(Millis::new(0), 60_000);
        assert_eq!(
            t.display_off_remaining_ms,
            crate::timing::STANDBY_DISPLAY_OFF_MS
        );
    }

    #[test]
    fn the_panel_only_goes_dark_once_both_countdowns_have_expired() {
        let mut t = StandbyTimer::default();
        t.reset(Millis::new(0), 60_000);
        t.expire_now();
        // Standby has begun, but the display countdown has not.
        assert!(t.should_enter(true));
        assert!(!t.should_turn_off_display());
        t.display_off_remaining_ms = 0;
        assert!(t.should_turn_off_display());
    }

    #[test]
    fn switch_levels_round_trip() {
        for switch in crate::event::SwitchId::ALL {
            let mut levels = SwitchLevels::ALL_RELEASED;
            assert!(!levels.level(switch));
            levels.set_level(switch, true);
            assert!(levels.level(switch));
            // Exactly one level changed.
            let set: Vec<bool> = crate::event::SwitchId::ALL
                .iter()
                .map(|s| levels.level(*s))
                .collect();
            assert_eq!(set.iter().filter(|v| **v).count(), 1, "{switch:?}");
        }
    }
}
