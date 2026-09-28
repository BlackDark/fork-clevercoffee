//! The machine's memory: everything the states need and the reducer owns.
//!
//! # Why one struct and not the C++'s seventeen objects
//!
//! The C++ spreads the equivalent state across `MachineStateContext` (five
//! booleans and eleven request flags, `MachineStateContext.h:783-808`),
//! `ProcessController` (brew timer, PID mode, output), `StandbyCoordinator` (a
//! three-field countdown), `MaintenanceCoordinator` (one int) and each state
//! object's own private members (`errorStartTime_`, which the C++ gets for free
//! because a transition constructs a **fresh instance** — `StateFactory.cpp:24`).
//!
//! That last point is the one that matters for a reducer. There is no instance
//! to be fresh, so "a new `errorStartTime_` on every entry" has to be modelled
//! explicitly. [`Machine::entry_error_since`] does that, and
//! `states::on_entry` is the only thing that writes it.
//!
//! # Immutability
//!
//! [`reduce`](crate::reduce) takes `&Machine` and returns a new one. Nothing
//! here has interior mutability, so "the reducer does not mutate its input" is
//! a property of the signature rather than a property a test has to verify —
//! and `tests/purity.rs` verifies it anyway, by clone-and-compare.

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};

use crate::event::Sensors;

/// The eleven action request flags.
///
/// `MachineStateContext.h:801-802`, verbatim:
///
/// ```cpp
/// bool requestBrewStart_ = false;
/// bool requestBrewStop_ = false;
/// bool requestSteamStart_ = false;
/// bool requestSteamStop_ = false;
/// bool requestManualFlushStart_ = false;
/// bool requestManualFlushStop_ = false;
/// bool requestEnterBackflush_ = false;
/// bool requestBackflushCycleStart_ = false;
/// bool requestBackflushStop_ = false;
/// bool requestStandby_ = false;
/// bool requestNormalOperation_ = false;
/// ```
///
/// They are a *group* on purpose. ADR-0003's "Flag lifecycle" section is about
/// the group as a unit — "on state entry, drain all action flags that cannot be
/// acted upon" — and the C++'s two drain helpers
/// (`clearAllActionRequests`, `clearStaleStopRequests`) both enumerate the whole
/// list. One struct, two methods, and the compiler catches a new flag that
/// forgets to be drained.
// Eleven booleans because the C++ has eleven booleans
// (`MachineStateContext.h:801-802`) and the two *group* operations ADR-0003
// cares about — drain everything, drain only the stops — are about the group.
// Modelling eleven independent states instead would be inventing structure the
// firmware does not have; the meaningful combinations are the ones
// `clear_all` and `clear_stale_stops` name.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Requests {
    /// `requestBrewStart_`.
    pub brew_start: bool,
    /// `requestBrewStop_`.
    pub brew_stop: bool,
    /// `requestSteamStart_`.
    pub steam_start: bool,
    /// `requestSteamStop_`.
    pub steam_stop: bool,
    /// `requestManualFlushStart_`.
    pub manual_flush_start: bool,
    /// `requestManualFlushStop_`.
    pub manual_flush_stop: bool,
    /// `requestEnterBackflush_`.
    pub backflush_enter: bool,
    /// `requestBackflushCycleStart_`.
    pub backflush_cycle_start: bool,
    /// `requestBackflushStop_`.
    pub backflush_stop: bool,
    /// `requestStandby_`.
    pub standby: bool,
    /// `requestNormalOperation_`.
    pub normal_operation: bool,
}

/// One of the eleven request flags, as a value.
///
/// `Requests` is a struct of eleven `bool`s because the *drain* operations
/// (`clear_all`, `clear_stale_stops`) are about the group. Setting one flag
/// individually is a different concern and needs a different shape: the C++'s
/// eleven setters do not all reset the standby timer, and which ones do is a
/// property of the individual flag, not of the group
/// (`MachineStateContext.cpp:217-267` for the seven that do,
/// `:483,509,536,587` for the four that do not). So the setters take a `Request`
/// and this enum is the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Request {
    /// `requestBrewStart_`.
    BrewStart,
    /// `requestBrewStop_`.
    BrewStop,
    /// `requestSteamStart_`.
    SteamStart,
    /// `requestSteamStop_`.
    SteamStop,
    /// `requestManualFlushStart_`.
    ManualFlushStart,
    /// `requestManualFlushStop_`.
    ManualFlushStop,
    /// `requestEnterBackflush_`.
    BackflushEnter,
    /// `requestBackflushCycleStart_`.
    BackflushCycleStart,
    /// `requestBackflushStop_`.
    BackflushStop,
    /// `requestStandby_`.
    Standby,
    /// `requestNormalOperation_`.
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
    /// # The C++ table, setter by setter
    ///
    /// | flag | resets? | line |
    /// | --- | --- | --- |
    /// | [`Request::BackflushEnter`] | yes | `MachineStateContext.cpp:217-222` |
    /// | [`Request::BackflushCycleStart`] | yes | `:224-229` |
    /// | [`Request::BackflushStop`] | yes | `:231-236` |
    /// | [`Request::BrewStart`] | yes | `:238-243` |
    /// | [`Request::SteamStart`] | yes | `:245-250` |
    /// | [`Request::NormalOperation`] | yes | `:254-260` |
    /// | [`Request::BrewStop`] | **no** | `:483` |
    /// | [`Request::SteamStop`] | **no** | `:509` |
    /// | [`Request::ManualFlushStop`] | **no** | `:536` |
    /// | [`Request::Standby`] | **no** | `:587` |
    /// | [`Request::ManualFlushStart`] | **no** | `:523` |
    ///
    /// The last one is a fourth "no" that the C++ does not document: the
    /// manual-flush start setter at `MachineStateContext.h:523` is a bare
    /// `requestManualFlushStart_ = requested;` with no standby reset, unlike the
    /// other four start setters. Manual flush is only reachable from backflush
    /// mode, where the backflush setters have already reset the timer, so it has
    /// never mattered.
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

    /// `clearAllActionRequests()` — every flag, no exceptions
    /// (`MachineStateContext.h:615-627`).
    ///
    /// # Preserved deliberately, see `09-cpp-findings.md` §11 / ADR-0003
    ///
    /// Used on entry to `PID_DISABLED` and in its `update` while PID stays off.
    /// This is S11's cure, and it has a cost: a request that arrived *while* the
    /// machine was already in `PID_DISABLED` is thrown away rather than honoured
    /// on the next enable. The C++ accepts that trade
    /// (`PidStates.cpp:118-127`); the reducer reproduces it exactly.
    pub fn clear_all(&mut self) {
        *self = Self::CLEAR;
    }

    /// `clearStaleStopRequests()` — the four stop flags only, start flags kept
    /// as wake triggers (`MachineStateContext.h:634-639`).
    ///
    /// ADR-0003: "**Never** drain wake-up signals: `STANDBY` preserves
    /// `brewStartRequested` and `steamStartRequested` as intentional wake
    /// triggers."
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
/// The C++ keeps these inside each handler (`BrewHandler::lastSwitchReading_`,
/// `SteamHandler::lastSwitchReading_`, `PowerHandler::lastPowerSwitchPressed_`)
/// and reads the *live* level through `Switch::isPressed()`. The reducer has no
/// handler objects, so the levels are machine state and the "live read" becomes
/// a lookup. That is also what lets `PidNormalState::update` and
/// `SteamRunningState::update` see the water switch, as they do in C++
/// (`PidStates.cpp:33-43`, `SteamStates.cpp:36-46`).
// Four switch levels plus two latched long-press flags. The levels are not a
// state machine: any of the sixteen combinations is reachable, and the C++
// holds them the same way (`BrewHandler::lastSwitchReading_`,
// `PowerHandler::lastPowerSwitchPressed_`).
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
    /// `Switch::longPressDetected()` for the brew switch, sampled on the edge.
    ///
    /// Only `BrewHandler` consults it (`BrewHandler.h:199`), and only in
    /// `BACKFLUSH_IDLE`, to choose between "start a backflush cycle" and "start
    /// a manual flush". It is a property of the *press*, not of the level, so it
    /// is latched on the rising edge and never cleared by a release.
    pub brew_long_press: bool,
    /// `Switch::longPressDetected()` for the power switch, same latching.
    ///
    /// `PowerHandler::checkForLongPressReboot` (`PowerHandler.h:144`) needs it to
    /// decide on a reboot, and it needs it *after* the press edge is gone —
    /// hence [`crate::handlers::long_press_reboot`] runs on every tick.
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

/// The brew timer the display and `BREW_RUNNING`'s own stop condition read.
///
/// `ProcessController::currBrewTime_` and `totalTargetBrewTime_`
/// (`ProcessController.cpp:301-312`). The C++ writes `currBrewTime_` from four
/// different states, each with a different base, so the base is a function of
/// the state and the elapsed time is added to it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrewProgress {
    /// `setCurrBrewTime(ms)` — total elapsed brew time including pre-infusion.
    pub elapsed_ms: f64,
    /// `setTotalTargetBrewTime(ms)`; `0.0` means the shot does not end by time.
    pub target_ms: f64,
}

/// The two PID flags, which are not the same flag.
///
/// Getting this distinction wrong is the bug ADR-0003 item 4 and
/// `test_state_flow_integration`'s `EmergencyRecovery_RestoresPidFromConfig`
/// were written for, so it gets its own struct with its own documentation
/// rather than two bare `bool`s in [`Machine`].
///
/// | | C++ | Set by | Read by |
/// | --- | --- | --- | --- |
/// | runtime | `SystemContext::isProcessPidEnabled()` | `setPidRuntimeState` (`MachineStateContext.cpp:275`) | the global guard in `BaseState.h:167`, `PidNormalState`, `PidDisabledState` |
/// | mode | `ProcessController::isPIDEnabled()`, i.e. `pidMode() == AUTOMATIC` | `setPIDEnabled` (`ProcessController.cpp:261`) | `shouldPIDBeEnabled` (`ProcessController.cpp:246`) — the heater gate |
///
/// The C++ keeps the second in a *different object* from the state machine, which
/// is how they drift: emergency stop forces `runtime` off, and only
/// `EmergencyStopState::onExitImpl` puts it back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pid {
    /// `SystemContext::isProcessPidEnabled()`. The state machine's view.
    pub runtime_enabled: bool,
    /// `ProcessController::isPIDEnabled()`. The heater's gate.
    pub mode_enabled: bool,
    /// `SystemContext::isProcessBrewPidDisabled()` — the brew-PID delay window
    /// (`ProcessController.cpp:410-424`).
    pub brew_disabled: bool,
    /// `ProcessController::pidOutput_`.
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
/// `MachineStateContext::backflushOn_` and `currBackflushCycles_`
/// (`MachineStateContext.h:787-788`). The counter starts at **1**, not 0, and
/// `resolveCycleAdvance(1, 5)` starts the next cycle — so the loop runs cycles
/// 1..=5 and `BackflushFinished` is reached after the fifth flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backflush {
    /// `backflushOn_`.
    pub on: bool,
    /// `currBackflushCycles_`.
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

/// Three fields in C++ plus a fourth that only affects the display
/// (`standbyModeRemainingTimeDisplayOffMillis_`); the display-off countdown is
/// not a state-machine concern and is not ported — see
/// [`DISPLAY_OFF_NOT_PORTED`].
///
/// # One intentional difference
///
/// The C++ uses `standbyModeStartTimeMillis_ != 0` as "the timer was started"
/// test (`StandbyCoordinator.h:120-121`) and stores `millis()` in it. At
/// `millis() == 0` — the first millisecond after boot, and every millisecond in
/// a test harness — the test is false, so standby can never fire. [`Option`]
/// makes "started" unambiguous and fixes that. This is the one place the port
/// does *not* preserve a C++ artefact, and it can only make standby work
/// *earlier*, never later, so it is not a safety regression.
pub const DISPLAY_OFF_NOT_PORTED: bool = true;

/// The standby countdown, ported from `StandbyCoordinator`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StandbyTimer {
    /// `standbyModeStartTimeMillis_ != 0`.
    pub started_at: Option<Millis>,
    /// `standbyModeRemainingTimeMillis_`.
    pub remaining_ms: u32,
    /// `lastStandbyTimeMillis_` — the once-a-second update gate.
    pub last_update: Option<Millis>,
}

impl StandbyTimer {
    /// `standbyCoordinator().reset()` (`StandbyCoordinator.h:86-93`).
    ///
    /// Resets only when standby is enabled — the C++'s own guard
    /// (`StandbyCoordinator.h:88`, `resetStandbyTimerIfNeeded`): the method
    /// reads the timeout out of `Config` itself, so disabling standby is a no-op
    /// rather than an immediate-standby.
    ///
    /// `timeout_ms` is [`Context::standby_timeout_ms`](crate::Context::standby_timeout_ms)
    /// when enabled and `0` when not, so the "disabled" case is expressed by
    /// the caller rather than hidden in a `Config` lookup this type cannot do.
    pub fn reset(&mut self, now: Millis, timeout_ms: u32) {
        if timeout_ms == 0 {
            return;
        }
        self.remaining_ms = timeout_ms;
        self.started_at = Some(now);
        self.last_update = Some(now);
    }

    /// `standbyCoordinator().setRemainingTimeMillis(0)`
    /// (`PowerHandler::powerOff`, `PowerHandler.h:172`) — "standby now".
    pub const fn expire_now(&mut self) {
        self.remaining_ms = 0;
    }

    /// `shouldEnterStandby()` (`StandbyCoordinator.h:115-122`): enabled **and**
    /// the countdown has run out **and** the timer was started.
    #[must_use]
    pub const fn should_enter(&self, enabled: bool) -> bool {
        enabled && self.remaining_ms == 0 && self.started_at.is_some()
    }
}

/// The whole machine, as a value.
// Four booleans among twenty-two fields. Each names the C++ field it replaces;
// the alternative — nesting them into sub-structs — would obscure exactly the
// cross-field invariants the guards read (`is_emergency_stop` reads `safety`
// *and* is documented as a separate flag in C++; `should_pid_be_enabled` reads
// `state`, config, and `pid.brew_disabled` together).
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Machine {
    /// `StateMachine::currentState_`.
    pub state: MachineState,
    /// `StateMachine::initialized_` (`StateMachine.cpp:23`).
    ///
    /// `StateMachine::update()` returns immediately when this is false
    /// (`StateMachine.cpp:72-75`). The reducer reproduces that: an
    /// uninitialised machine consumes every event and produces no effects.
    pub initialized: bool,
    /// The last clock reading the shell supplied via
    /// [`Event::Tick`](crate::Event::Tick).
    ///
    /// # Why it is not a clock
    ///
    /// `millis()` is read in eleven places in the C++'s state classes
    /// (`SensorErrorState.cpp:16,25,38`, `EepromErrorState.cpp:99,117`, and the
    /// entry times). A reducer that called it would be untestable and would not
    /// be a pure function. So the reading arrives in the event and every timed
    /// decision is a comparison against it.
    pub now: Millis,
    /// `MachineStateContext::stateEntryTime_` (`MachineStateContext.h:808`).
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
    /// `MachineStateContext::steamON_` (`MachineStateContext.h:785`).
    pub steam_mode: bool,
    /// `MachineStateContext::steamFirstON_` (`MachineStateContext.h:786`).
    pub steam_first_on: bool,
    /// Backflush mode and cycle counter.
    pub backflush: Backflush,
    /// The brew timer.
    pub brew: BrewProgress,
    /// The standby countdown.
    pub standby: StandbyTimer,
    /// `SensorErrorState::errorStartTime_` / `EepromErrorState::errorStartTime_`.
    ///
    /// The C++ gets a fresh value for free because a transition builds a new
    /// state object (`StateFactory.cpp:24`). Here, [`crate::states::on_entry`]
    /// is the only writer, and the value is cleared on exit so a stale one can
    /// never be read.
    pub error_since: Option<Millis>,
    /// `MaintenanceCoordinator::shotsSinceBackflush_`.
    pub shots_since_backflush: i32,
    /// `setHotWaterActivity` — "a hot-water switch edge happened". Resets the
    /// standby timer (`MachineStateContext.cpp:262-267`).
    pub hot_water_activity: bool,
    /// `PowerHandler::systemInitializedTime_` — when the power handler first
    /// ran, and from which presses are ignored for
    /// [`timing::POWER_SWITCH_SETTLE_MS`] (`PowerHandler.h:117`).
    pub boot_at: Option<Millis>,
    /// `PowerHandler::longPressStartTime_`, and the `trackingLongPress_` flag it
    /// is armed by. `None` means "not tracking", which is both `isRunning_` and
    /// `trackingLongPress_ == false` in the C++.
    pub power_press_started_at: Option<Millis>,
    /// `BrewHandler::brewStartTime_`. Recorded but **never compared** — see
    /// [`timing::PUMP_TIMEOUTS_NEVER_ARM`].
    pub brew_pump_started_at: Option<Millis>,
    /// `HotWaterHandler::pumpTimer_.startTime_`. Same: recorded, never used.
    pub hot_water_pump_started_at: Option<Millis>,
}

impl Machine {
    /// A machine that has not been initialised: `INIT`, no effects, no clock.
    ///
    /// This is `StateMachine`'s constructor state
    /// (`StateMachine.cpp:21-24`) — `currentState_` is `nullptr` and
    /// `update()` would refuse to run. The reducer has no null state, so it
    /// uses `INIT` plus the [`initialized`](Self::initialized) flag and makes
    /// every event a no-op until [`boot`](Self::boot).
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

    /// The machine's current state. Named so the reducer reads like the C++.
    #[must_use]
    pub const fn state_id(&self) -> MachineState {
        self.state
    }

    /// `context.getStateElapsedTimeMs()` (`MachineStateContext.cpp:447-450`).
    #[must_use]
    pub const fn state_elapsed_ms(&self) -> u32 {
        self.now.since(self.entry_at).raw()
    }

    /// `context.hasStateTimeoutElapsed(ms)` (`MachineStateContext.cpp:452-454`)
    /// — `>=`, not `>`.
    #[must_use]
    pub const fn state_timeout_elapsed(&self, timeout_ms: u32) -> bool {
        self.state_elapsed_ms() >= timeout_ms
    }

    /// `context.isEmergencyStop()` (`MachineStateContext.h:380-382`).
    #[must_use]
    pub const fn is_emergency_stop(&self) -> bool {
        self.safety.latched
    }

    /// `context.isPidRuntimeEnabled()` (`MachineStateContext.cpp:176-178`).
    #[must_use]
    pub const fn is_pid_runtime_enabled(&self) -> bool {
        self.pid.runtime_enabled
    }

    /// `context.isWaterTankFull()` (`MachineStateContext.cpp:114-116`).
    #[must_use]
    pub const fn is_water_tank_full(&self) -> bool {
        self.sensors.water_tank_full
    }

    /// `context.hasSensorError()` (`MachineStateContext.cpp:139-141`).
    #[must_use]
    pub const fn has_sensor_error(&self) -> bool {
        self.sensors.has_sensor_error()
    }

    /// `context.isBackflushModeActive()` (`MachineStateContext.h:422-424`).
    #[must_use]
    pub const fn is_backflush_mode_active(&self) -> bool {
        self.backflush.on
    }

    /// `context.getCurrentBrewWeight()` (`MachineStateContext.cpp:129-133`).
    #[must_use]
    pub const fn brew_weight(&self) -> f32 {
        self.sensors.brew_weight
    }

    /// `context.getCurrentTemperature()` (`MachineStateContext.cpp:106-108`).
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
        // `MachineStateContext.h:788`: `int currBackflushCycles_ = 1;`. Starting
        // at 0 would make `resolveCycleAdvance(0, 5)` run cycles 0..=4 — five
        // cycles, but the counter reported to the display would be wrong.
        assert_eq!(Machine::cold().backflush.cycle, 1);
        assert!(!Machine::cold().backflush.on);
    }

    #[test]
    fn clear_all_drains_all_eleven_flags() {
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
        assert_eq!(r, Requests::CLEAR);
        assert!(!r.any());
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
        // `MachineStateContext.cpp:454`: `return getStateElapsedTimeMs() >= timeoutMs;`
        let mut m = Machine::cold();
        m.now = Millis::new(3_000);
        m.entry_at = Millis::new(0);
        assert!(m.state_timeout_elapsed(3_000));
        assert!(!m.state_timeout_elapsed(3_001));
    }

    #[test]
    fn state_elapsed_wraps_across_the_u32_rollover() {
        // The C++ computes `now - entryTime` on a 32-bit `unsigned long` and
        // relies on the wrap. `Millis::since` does the same.
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
        // `StandbyCoordinator::reset()` starts with
        // `if (!standbyEnabled.get()) return;`, so a disabled standby is not
        // started by a reset.
        let mut t = StandbyTimer::default();
        t.reset(Millis::new(1_000), 0);
        assert_eq!(t, StandbyTimer::default());
    }

    #[test]
    fn standby_reset_rearms_the_countdown() {
        let mut t = StandbyTimer {
            started_at: Some(Millis::new(1)),
            remaining_ms: 0,
            last_update: Some(Millis::new(1)),
        };
        t.reset(Millis::new(9_000), 60_000);
        assert_eq!(t.remaining_ms, 60_000);
        assert_eq!(t.started_at, Some(Millis::new(9_000)));
        assert!(!t.should_enter(true));
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
