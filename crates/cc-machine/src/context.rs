//! What the reducer is told, as a pure value.
//!
//! [`Context`] is the *read-only world* the C++ reaches through
//! `MachineStateContext`. It holds only the values that can change the
//! transition decision, so the set is deliberately small: pull anything in here
//! that does not change a transition's outcome and the reducer stops being
//! auditable.
//!
//! # What is **not** here, and why
//!
//! * **Sensor values.** They are [`Event::SensorUpdated`] payloads, not context.
//!   The C++ reads them out of `SensorCoordinator` inside `checkTransitions`,
//!   which makes the transition decision depend on *when* it was read relative
//!   to the sensor update. Here they are an explicit event, so every decision
//!   is a decision about a specific sample.
//! * **The clock.** The reducer never reads one. [`Event::Tick`] carries the
//!   reading (see `lib.rs`).
//! * **The current state.** That is [`Machine::state`], not the context — it is
//!   the thing being reduced, not the context it is reduced in.
//! * **The safety verdict.** That is [`Event::Safety`], because S1 is
//!   stateful (a three-reading debounce) and the latch has to be part of the
//!   machine's memory.

use cc_config::Config;
use cc_domain::units::Celsius;

/// Everything the state machine may read but not change.
///
/// Borrowed rather than owned: the configuration is a large value and the
/// reducer only ever reads from it, and copying 98 fields on every one of the
/// ~50 ticks per second would be the dominant cost of the control loop.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The whole typed configuration (`cc-config`).
    ///
    /// The state machine reads a small subset of it — `pid.enabled`,
    /// `brew.mode`, `brew.pre_infusion.*`, `brew.by_time.*`, `brew.by_weight.*`,
    /// `brew.pid_delay`, `backflush.*`, `standby.*`,
    /// `hardware.switches.*.type`, `hardware.switches.*.enabled`,
    /// `hardware.sensors.watertank.keep_heater_on_empty` — and the helpers
    /// below name each of those reads so a reviewer can check the mapping
    /// without grepping.
    pub config: &'a Config,
    /// The active PID setpoint in degrees Celsius.
    ///
    /// The C++ computes this in `ProcessController::updateSetpoint`, which
    /// chooses `steam.setpoint` while `isSteamModeActive()` and
    /// `brew.setpoint + brew.temp_offset` otherwise. Setpoint *selection* is
    /// process control, not a transition, so the shell decides it and hands the
    /// result in. The state machine only needs it to report the duty it wants.
    pub setpoint: Celsius,
}

impl<'a> Context<'a> {
    /// A context over `config` with the given setpoint.
    #[must_use]
    pub const fn new(config: &'a Config, setpoint: Celsius) -> Self {
        Self { config, setpoint }
    }

    // -- pid ---------------------------------------------------------------

    /// `context.isPidConfigEnabled()` — the *user's saved preference*,
    /// `Config::getInstance().pidEnabled.get()` (`MachineStateContext.cpp:181`).
    ///
    /// Distinct from the runtime flag the state machine actually reads
    /// ([`Machine::pid`](crate::Machine)::`runtime_enabled`): this one is the
    /// source of truth that runtime state is restored *from* on exit from
    /// standby and emergency stop.
    #[must_use]
    pub fn pid_config_enabled(&self) -> bool {
        self.config.pid.enabled
    }

    // -- brew --------------------------------------------------------------

    /// `context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW`
    /// (`BrewStates.cpp:54,113,198,226,277`).
    #[must_use]
    pub fn brew_is_automatic(&self) -> bool {
        self.config.brew.mode == cc_domain::process::BrewMode::Automatic
    }

    /// `calculatePreinfusionBaseTimeMs` (`BrewStates.cpp:24-37`), in
    /// milliseconds: pre-infusion plus pause, or zero when pre-infusion is
    /// off.
    #[must_use]
    pub fn preinfusion_base_ms(&self) -> f64 {
        preinfusion_base_ms(self.config)
    }

    /// `initTotalTargetBrewTime` (`BrewStates.cpp:53-62`): the total target brew
    /// time in milliseconds, or `0.0` unless brewing automatically *by time*.
    #[must_use]
    pub fn total_target_brew_ms(&self) -> f64 {
        if self.brew_is_automatic() && self.config.brew.by_time.enabled {
            self.preinfusion_base_ms() + (self.config.brew.by_time.target_time * 1000.0)
        } else {
            0.0
        }
    }

    /// `brew.pid_delay` in milliseconds
    /// (`ProcessController::handleBrewPIDDelay`, `ProcessController.cpp:469`).
    #[must_use]
    pub fn brew_pid_delay_ms(&self) -> f64 {
        self.config.brew.pid_delay * 1000.0
    }

    // -- backflush ---------------------------------------------------------

    /// `context.getBackflushFillTimeMs()` (`MachineStateContext.cpp:433`).
    #[must_use]
    pub fn backflush_fill_ms(&self) -> u32 {
        secs_to_ms(self.config.backflush.fill_time)
    }

    /// `context.getBackflushFlushTimeMs()` (`MachineStateContext.cpp:437`).
    #[must_use]
    pub fn backflush_flush_ms(&self) -> u32 {
        secs_to_ms(self.config.backflush.flush_time)
    }

    /// `context.getBackflushCycles()` (`MachineStateContext.cpp:441`).
    #[must_use]
    pub const fn backflush_cycles(&self) -> i32 {
        self.config.backflush.cycles
    }

    // -- standby -----------------------------------------------------------

    /// `StandbyCoordinator::getStandbyTimeoutMillis` (`StandbyCoordinator.h:155`)
    /// — `standby.time` minutes to milliseconds.
    #[must_use]
    pub fn standby_timeout_ms(&self) -> u32 {
        secs_to_ms(self.config.standby.time * 60.0)
    }

    // -- hardware ----------------------------------------------------------

    /// `hardware.sensors.watertank.keep_heater_on_empty`
    /// (`ProcessController::shouldPIDBeEnabled`, `ProcessController.cpp:247-248`).
    #[must_use]
    pub const fn keep_heater_on_empty(&self) -> bool {
        self.config.hardware.sensors.watertank.keep_heater_on_empty
    }

    /// `config_.hardwareSwitchesBrewEnabled.get()` (`BrewHandler.h:126`).
    #[must_use]
    pub const fn brew_switch_enabled(&self) -> bool {
        self.config.hardware.switches.brew.enabled
    }

    /// `config_.hardwareSwitchesBrewType.get()` (`BrewHandler.h:91`).
    #[must_use]
    pub const fn brew_switch_type(&self) -> cc_domain::hardware::SwitchType {
        self.config.hardware.switches.brew.r#type
    }

    /// `config_.hardwareSwitchesSteamEnabled.get()` (`SteamHandler.h:87`).
    #[must_use]
    pub const fn steam_switch_enabled(&self) -> bool {
        self.config.hardware.switches.steam.enabled
    }

    /// `config_.hardwareSwitchesSteamType.get()` (`SteamHandler.h:82`).
    #[must_use]
    pub const fn steam_switch_type(&self) -> cc_domain::hardware::SwitchType {
        self.config.hardware.switches.steam.r#type
    }

    /// `config_.hardwareSwitchesPowerEnabled.get()` (`PowerHandler.h:50`).
    #[must_use]
    pub const fn power_switch_enabled(&self) -> bool {
        self.config.hardware.switches.power.enabled
    }

    /// `config_.hardwareSwitchesPowerType.get()` (`PowerHandler.h:63`).
    #[must_use]
    pub const fn power_switch_type(&self) -> cc_domain::hardware::SwitchType {
        self.config.hardware.switches.power.r#type
    }

    /// `config_.hardwareSwitchesHotWaterEnabled.get()`
    /// (`HotWaterHandler.h:48`).
    #[must_use]
    pub const fn hot_water_switch_enabled(&self) -> bool {
        self.config.hardware.switches.hot_water.enabled
    }
}

/// The file-scope form of [`Context::preinfusion_base_ms`], for the
/// standalone helper the parity tests call directly.
fn preinfusion_base_ms(config: &Config) -> f64 {
    let mut time_ms = 0.0;
    if config.brew.pre_infusion.enabled {
        time_ms += config.brew.pre_infusion.time * 1000.0;
        if config.brew.pre_infusion.pause > 0.0 {
            time_ms += config.brew.pre_infusion.pause * 1000.0;
        }
    }
    time_ms
}

/// The C++'s `(unsigned long)(seconds * 1000)` truncation, in one place.
///
/// `static_cast<unsigned long>(config.backflushFillTime.get() * 1000)` and the
/// same for the pre-infusion and pause times. A negative or absurd value
/// truncates to `0`, which makes the timeout fire immediately — the same
/// behaviour the C++ cast produces, and the reason a range-checked
/// `Config` is not optional here.
// The C++ is `static_cast<unsigned long>(seconds * 1000)`. The truncation and
// the sign loss are both the C++'s behaviour and are the point: a range-checked
// `Config` never produces either, and an unchecked one should saturate towards
// "already elapsed" rather than panic.
#[allow(clippy::cast_possible_truncation)]
// Justification: reproduces `static_cast<unsigned long>(seconds * 1000)`
// exactly, including the truncation.
#[allow(clippy::cast_sign_loss)]
// Justification: `secs_to_ms` checks `seconds > 0.0` before calling this, so the
// value cast is known to be non-negative.
fn saturating_millis(seconds: f64) -> u32 {
    (seconds * 1000.0) as u32
}

fn secs_to_ms(seconds: f64) -> u32 {
    if seconds <= 0.0 {
        return 0;
    }
    saturating_millis(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_config::Config;

    /// Exact comparison of a millisecond value.
    ///
    /// `assert_eq!` on `f64` is denied by `clippy::float_cmp`. These values come
    /// from `small_literal * 1000.0`, which is exact in binary floating point, so
    /// an exact comparison is the *right* one here and a tolerance would hide a
    /// genuine arithmetic mistake.
    #[allow(clippy::float_cmp)]
    // Justification: every value compared here is an exact multiple of 1000 from
    // a small literal, so there is no rounding to tolerate.
    fn assert_ms(actual: f64, expected: f64) {
        assert_eq!(actual, expected);
    }

    #[test]
    fn preinfusion_base_is_zero_when_disabled() {
        // `BrewStates.cpp:27-28`: the whole calculation is inside
        // `if (preinfusionEnabled)`.
        let config = Config {
            brew: cc_config::config::Brew {
                pre_infusion: cc_config::config::BrewPreInfusion {
                    enabled: false,
                    time: 3.0,
                    pause: 2.0,
                },
                ..cc_config::config::Brew::default()
            },
            ..Config::default()
        };
        let ctx = Context::new(&config, Celsius::new(95.0));
        assert_ms(ctx.preinfusion_base_ms(), 0.0);
        assert_ms(ctx.total_target_brew_ms(), 0.0);
    }

    #[test]
    fn preinfusion_base_is_time_plus_pause() {
        let config = Config {
            brew: cc_config::config::Brew {
                pre_infusion: cc_config::config::BrewPreInfusion {
                    enabled: true,
                    time: 3.0,
                    pause: 2.0,
                },
                ..cc_config::config::Brew::default()
            },
            ..Config::default()
        };
        let ctx = Context::new(&config, Celsius::new(95.0));
        assert_ms(ctx.preinfusion_base_ms(), 5000.0);
    }

    #[test]
    fn preinfusion_base_ignores_a_zero_pause() {
        // `BrewStates.cpp:31`: `if (pauseTimeSec > 0.0)`. A zero pause adds
        // nothing, which is also why `BREW_PREINFUSION_PAUSE` is skipped
        // entirely rather than entered for 0 ms.
        let config = Config {
            brew: cc_config::config::Brew {
                pre_infusion: cc_config::config::BrewPreInfusion {
                    enabled: true,
                    time: 2.0,
                    pause: 0.0,
                },
                ..cc_config::config::Brew::default()
            },
            ..Config::default()
        };
        let ctx = Context::new(&config, Celsius::new(95.0));
        assert_ms(ctx.preinfusion_base_ms(), 2000.0);
    }

    #[test]
    fn total_target_is_zero_unless_automatic_and_by_time() {
        let base = cc_config::config::Brew {
            pre_infusion: cc_config::config::BrewPreInfusion {
                enabled: true,
                time: 3.0,
                pause: 2.0,
            },
            by_time: cc_config::config::BrewByTime {
                enabled: true,
                target_time: 25.0,
            },
            ..cc_config::config::Brew::default()
        };

        // Manual brew by time: no target, so BrewRunning never self-terminates.
        let manual = Config {
            brew: base.clone(),
            ..Config::default()
        };
        assert_ms(
            Context::new(&manual, Celsius::new(95.0)).total_target_brew_ms(),
            0.0,
        );

        // Automatic but not by time: same.
        let not_by_time = Config {
            brew: cc_config::config::Brew {
                mode: cc_domain::process::BrewMode::Automatic,
                by_time: cc_config::config::BrewByTime {
                    enabled: false,
                    target_time: 25.0,
                },
                ..base.clone()
            },
            ..Config::default()
        };
        assert_ms(
            Context::new(&not_by_time, Celsius::new(95.0)).total_target_brew_ms(),
            0.0,
        );

        // Automatic and by time: 5000 + 25000.
        let both = Config {
            brew: cc_config::config::Brew {
                mode: cc_domain::process::BrewMode::Automatic,
                ..base
            },
            ..Config::default()
        };
        assert_ms(
            Context::new(&both, Celsius::new(95.0)).total_target_brew_ms(),
            30_000.0,
        );
    }

    #[test]
    fn backflush_times_are_seconds_to_truncated_milliseconds() {
        let config = Config::default();
        let ctx = Context::new(&config, Celsius::new(95.0));
        // Defaults are 5 s fill / 10 s flush (`config.rs` `Backflush::default`).
        assert_eq!(ctx.backflush_fill_ms(), 5_000);
        assert_eq!(ctx.backflush_flush_ms(), 10_000);
    }

    #[test]
    fn standby_timeout_is_minutes_to_milliseconds() {
        let config = Config::default();
        let ctx = Context::new(&config, Celsius::new(95.0));
        // Default `standby.time` is 35 minutes.
        assert_eq!(ctx.standby_timeout_ms(), 35 * 60 * 1000);
    }

    #[test]
    fn a_negative_duration_truncates_to_zero() {
        // The C++ cast of a negative double to `unsigned long` is undefined, but
        // in practice yields 0, and 0 means "the timeout has already elapsed".
        assert_eq!(secs_to_ms(-1.0), 0);
        assert_eq!(secs_to_ms(0.0), 0);
        assert_eq!(secs_to_ms(1.5), 1500);
    }
}
