//! Debounced operator-switch and water-tank inputs.
//!
//! GPIO34/35/36/39 are input-only without internal pulls. Use [`Pull::Floating`]
//! and the board's external pulls; ESP-IDF silently accepts ineffective internal
//! pull requests. GPIO23's tank float can use [`GpioIn::pull_for`].
//!
//! Raw seeds arm debounce, not the initial logical state. Normally-open operator
//! switches start LOW, the float HIGH; both start logically unpressed. Do not
//! harmonize the seeds or a power switch may appear active at boot.

use cc_config::Config;
use cc_domain::hardware::{SwitchMode, SwitchType};
use cc_domain::units::Millis;
use cc_machine::{Event, SwitchId};
use esp_idf_hal::gpio::Pull;
use heapless::Vec;
use log::info;

use crate::pins;
use crate::sensors::GpioIn;

/// How many switch edges one tick can carry: one per operator switch.
///
/// The water-tank float is not a button, so it never contributes an edge. A
/// `heapless::Vec` at this capacity rather than `alloc::vec::Vec` because this
/// runs in the control tick, where an allocation is a heap operation on the
/// safety-critical path, and because four is not an estimate — it is the number
/// of switches, and [`SwitchBank::poll`] iterates exactly four of them, so the
/// capacity is sufficient by construction rather than by hope.
const MAX_EDGES_PER_TICK: usize = 4;

/// The edges one [`SwitchBank::poll`] can report.
pub type Edges = Vec<Event, MAX_EDGES_PER_TICK>;

/// The pull for an operator switch: none, because the pin has none to give.
///
/// GPIO34/35/36/39 are input-only with no internal pull (the original ESP32's
/// datasheet lists no pull for the input-only bank), so floating is what the
/// hardware offers. See the module documentation.
const OPERATOR_PULL: Pull = Pull::Floating;

/// Which switches exist on this machine.
///
/// A struct of five [`GpioIn`]s rather than an array because the four operator
/// switches and the tank float have different wiring rules and different
/// lifetimes, and a named field per switch is what makes a wiring mistake a
/// compile error rather than an index.
pub struct SwitchBank {
    /// `PIN_POWERSWITCH` (GPIO39). Standby and reboot.
    power: GpioIn<'static>,
    /// `PIN_BREWSWITCH` (GPIO34). Brew, backflush and manual flush.
    brew: GpioIn<'static>,
    /// `PIN_STEAMSWITCH` (GPIO35).
    steam: GpioIn<'static>,
    /// `PIN_WATERSWITCH` (GPIO36) — the hot-water **button**.
    hot_water: GpioIn<'static>,
    /// `PIN_WATERTANKSENSOR` (GPIO23) — the tank **float**. Not a button.
    water_tank: GpioIn<'static>,
    /// The previous debounced level of each, for edge detection.
    ///
    /// One small struct serves all five, and the reducer keeps its own copy in
    /// `Machine::switches` — the two are cross-checked, which is a test rather
    /// than a hope.
    previous: Levels,
    /// Whether the water-tank float is fitted at all.
    ///
    /// `hardware.sensors.watertank.enabled` is `false` by default, and a machine
    /// with no float must report **full**, or S4 would block the pump forever.
    /// Assuming full is the only safe answer when there is nothing to read.
    tank_fitted: bool,
}

/// The debounced level of each input, as read this tick.
///
/// Five booleans because the five inputs are five independent facts about five
/// independent pieces of wire, and every consumer wants exactly one of them:
/// the edge loop wants one, `/api/status` wants the float, and the boot log
/// wants all five. A two-variant enum per switch would be a state machine for
/// something that is a measurement.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Levels {
    /// The power switch.
    pub power: bool,
    /// The brew switch.
    pub brew: bool,
    /// The steam switch.
    pub steam: bool,
    /// The hot-water button.
    pub hot_water: bool,
    /// The tank float: "water detected".
    pub water_tank_full: bool,
}

impl SwitchBank {
    /// Build the five inputs from the configuration's switch parameters.
    ///
    /// The pins arrive already taken from `Peripherals`, because
    /// `Peripherals::take()` may only be called once and belongs to
    /// `cc-firmware`'s `bring_up`. Each is reconfigured here as an input with
    /// the pull this board's wiring needs.
    ///
    /// # Errors
    ///
    /// [`esp_idf_svc::sys::EspError`] from `PinDriver::input`. A switch that
    /// cannot be configured is a machine that cannot be operated by hand, which
    /// is worth failing the boot over.
    pub fn new(
        power: impl esp_idf_hal::gpio::InputPin + 'static,
        brew: impl esp_idf_hal::gpio::InputPin + 'static,
        steam: impl esp_idf_hal::gpio::InputPin + 'static,
        hot_water: impl esp_idf_hal::gpio::InputPin + 'static,
        water_tank: impl esp_idf_hal::gpio::InputPin + 'static,
        config: &Config,
    ) -> Result<Self, esp_idf_svc::sys::EspError> {
        let power = GpioIn::new(
            esp_idf_hal::gpio::PinDriver::input(power, OPERATOR_PULL)?,
            config.hardware.switches.power.r#type,
            config.hardware.switches.power.mode,
            operator_initial_raw(config.hardware.switches.power.mode),
        );
        let brew = GpioIn::new(
            esp_idf_hal::gpio::PinDriver::input(brew, OPERATOR_PULL)?,
            config.hardware.switches.brew.r#type,
            config.hardware.switches.brew.mode,
            operator_initial_raw(config.hardware.switches.brew.mode),
        );
        let steam = GpioIn::new(
            esp_idf_hal::gpio::PinDriver::input(steam, OPERATOR_PULL)?,
            config.hardware.switches.steam.r#type,
            config.hardware.switches.steam.mode,
            operator_initial_raw(config.hardware.switches.steam.mode),
        );
        let hot_water = GpioIn::new(
            esp_idf_hal::gpio::PinDriver::input(hot_water, OPERATOR_PULL)?,
            config.hardware.switches.hot_water.r#type,
            config.hardware.switches.hot_water.mode,
            operator_initial_raw(config.hardware.switches.hot_water.mode),
        );
        // The float is the one input whose pull matters: GPIO23 has one.
        let tank_mode = config.hardware.sensors.watertank.mode;
        let water_tank = GpioIn::new(
            esp_idf_hal::gpio::PinDriver::input(water_tank, GpioIn::pull_for(tank_mode))?,
            SwitchType::Toggle,
            tank_mode,
            GpioIn::initial_raw_for(tank_mode),
        );

        let bank = Self {
            power,
            brew,
            steam,
            hot_water,
            water_tank,
            previous: Levels::default(),
            tank_fitted: config.hardware.sensors.watertank.enabled,
        };
        bank.describe(config);
        Ok(bank)
    }

    /// The pin numbers, the configured type and mode, and whether the reducer
    /// will honour each switch.
    ///
    /// Printed once, at boot, because "the brew switch does nothing" is the
    /// single most likely field report and the reason is always one of the four
    /// `hardware.switches.*.enabled` flags — all of which default to `false`.
    fn describe(&self, config: &Config) {
        let row = |name: &str, pin: u8, enabled: bool, r#type: SwitchType, mode: SwitchMode| {
            info!(
                "switch {name}: GPIO{pin} type {type:?} mode {mode:?} enabled {enabled}{note}",
                type = r#type,
                note = if enabled { "" } else { " -- the reducer will IGNORE this switch" }
            );
        };
        row(
            "power",
            pins::POWER_SWITCH,
            config.hardware.switches.power.enabled,
            config.hardware.switches.power.r#type,
            config.hardware.switches.power.mode,
        );
        row(
            "brew",
            pins::BREW_SWITCH,
            config.hardware.switches.brew.enabled,
            config.hardware.switches.brew.r#type,
            config.hardware.switches.brew.mode,
        );
        row(
            "steam",
            pins::STEAM_SWITCH,
            config.hardware.switches.steam.enabled,
            config.hardware.switches.steam.r#type,
            config.hardware.switches.steam.mode,
        );
        row(
            "hot_water",
            pins::WATER_SWITCH,
            config.hardware.switches.hot_water.enabled,
            config.hardware.switches.hot_water.r#type,
            config.hardware.switches.hot_water.mode,
        );
        info!(
            "switch water_tank: GPIO{} toggle, enabled {fitted} -- when absent the machine \
             reports the tank FULL, or S4 would block the pump forever",
            pins::WATER_TANK_SENSOR,
            fitted = self.tank_fitted,
        );
    }

    /// Read every input and emit the edges.
    ///
    /// Called once per control tick, before the reducer runs. The returned
    /// `Vec` is at most four elements long (one per operator switch; the float
    /// is not a button) and is empty on a tick where nothing moved — which on a
    /// healthy machine is nearly every tick.
    ///
    /// The order is the [`SwitchId`] declaration order, so a tick in which two
    /// switches move produces the same event order every time and the reducer's
    /// output is reproducible.
    pub fn poll(&mut self, now: Millis) -> Edges {
        let levels = self.read(now);
        let mut events = Vec::new();
        for (switch, level) in [
            (SwitchId::Brew, levels.brew),
            (SwitchId::Steam, levels.steam),
            (SwitchId::Power, levels.power),
            (SwitchId::HotWater, levels.hot_water),
        ] {
            if self.previous.level(switch) == level {
                continue;
            }
            // The long press is read **at the edge**, not after the hold: the
            // press that opens `BACKFLUSH_IDLE` chooses manual flush over a
            // backflush cycle. So it is sampled here and carried in the event,
            // because by the time the reducer's per-tick work runs the flag has
            // been cleared by the release.
            let long_press = match switch {
                SwitchId::Brew => self.brew.long_press_detected(),
                SwitchId::Power => self.power.long_press_detected(),
                SwitchId::Steam | SwitchId::HotWater => false,
            };
            // `push` returns `Err` only when the vector is full, and it cannot
            // be: the loop below iterates exactly `SwitchId::ALL`, which is four
            // entries, which is `MAX_EDGES_PER_TICK`. A fifth switch would need
            // a fifth `GpioIn` field and a fifth loop entry, so the type would
            // have changed rather than the capacity being wrong.
            let outcome = events.push(if level {
                Event::ButtonPressed { switch, long_press }
            } else {
                Event::ButtonReleased { switch }
            });
            debug_assert!(outcome.is_ok(), "one edge per switch, four switches");
        }
        self.previous = levels;
        events
    }

    /// The water tank's reading, for `Sensors::water_tank_full`.
    #[must_use]
    pub const fn water_tank_full(&self) -> bool {
        tank_reading(self.tank_fitted, self.previous.water_tank_full)
    }

    /// The levels as of the last [`Self::poll`], for the boot log and
    /// `/api/status`.
    #[must_use]
    pub const fn levels(&self) -> Levels {
        self.previous
    }

    /// Read all five inputs, without touching the edge bookkeeping.
    fn read(&mut self, now: Millis) -> Levels {
        // The order is power, brew, steam, hot water, tank, which is the order
        // the boot log lists them in.
        let power = self.power.update(now);
        let brew = self.brew.update(now);
        let steam = self.steam.update(now);
        let hot_water = self.hot_water.update(now);
        // The float is read even when it is not fitted: its pin is already
        // configured, and reading it costs one `is_high()`. The *value* is
        // discarded when `tank_fitted` is false, in `water_tank_full`.
        let water_tank_full = self.water_tank.update(now);
        Levels {
            power,
            brew,
            steam,
            hot_water,
            water_tank_full,
        }
    }
}

impl Levels {
    /// One switch's level, so the edge loop reads as a lookup.
    const fn level(&self, switch: SwitchId) -> bool {
        match switch {
            SwitchId::Brew => self.brew,
            SwitchId::Steam => self.steam,
            SwitchId::Power => self.power,
            SwitchId::HotWater => self.hot_water,
        }
    }
}

/// Return the fitted float's reading, or full when no float exists.
///
/// Do not use `fitted && raw`: that would report an absent tank sensor as empty
/// and permanently block pumping and PID enable.
#[must_use]
pub const fn tank_reading(fitted: bool, raw: bool) -> bool {
    if fitted {
        raw
    } else {
        true
    }
}

/// The `initial_raw` seed for an operator switch.
///
/// A normally-open operator switch starts at `LOW` and a normally-closed one at
/// `HIGH` — the **opposite** of the tank float's, and deliberately so; see the
/// module documentation. It is spelled out here rather than reusing
/// [`GpioIn::initial_raw_for`], which is the float's rule, because reusing it
/// would be a silent and load-bearing swap.
#[must_use]
pub const fn operator_initial_raw(mode: SwitchMode) -> u8 {
    match mode {
        SwitchMode::NormallyOpen => 0,
        SwitchMode::NormallyClosed => 1,
    }
}

/// The on-target unit tests for the switch bank.
///
/// The debouncing itself needs real pins and is covered by driving them on
/// hardware; what is reachable from a host is the *decision* each reading feeds,
/// and [`tank_reading`] is where a real bug once lived.
#[cfg(any(test, feature = "device-tests"))]
pub mod tests {
    // A panic here is the on-target runner's reporting mechanism, not an
    // undocumented hazard: `cc-device-tests` is built around "a failed assert
    // resets the chip". None of this module ships.
    #![allow(clippy::missing_panics_doc)]

    use super::tank_reading;

    /// A machine with no float fitted reports the tank **full**.
    ///
    /// This is the regression test for a bug that shipped to hardware. The
    /// implementation read `fitted && raw`, so an absent float produced
    /// `false` — the opposite of the intent stated in the field documentation,
    /// in the method documentation, and in the boot log. The effect on the
    /// machine was that it sat in `WaterTankEmpty` forever,
    /// `should_pid_be_enabled` cleared the PID every tick, and **the heater
    /// could never turn on at all.**
    ///
    /// The default config is the trigger: `hardware.sensors.watertank.enabled`
    /// is `false` unless an operator changes it, so this is the out-of-the-box
    /// path, not an edge case.
    #[cfg_attr(test, test)]
    pub fn an_absent_float_reports_the_tank_full_rather_than_empty() {
        for raw in [false, true] {
            assert!(
                tank_reading(false, raw),
                "no float fitted, raw level {raw}: expected FULL, got EMPTY. \
                 An absent sensor must not block the pump (S4)."
            );
        }
    }

    /// A **fitted** float reports its own reading, either way.
    ///
    /// The other half of the same decision: the fallback must not leak into the
    /// fitted case, or a genuinely empty tank would be invisible and the pump
    /// would run dry.
    #[cfg_attr(test, test)]
    pub fn a_fitted_float_reports_its_own_reading() {
        assert!(
            tank_reading(true, true),
            "a fitted float reading high must report full"
        );
        assert!(
            !tank_reading(true, false),
            "a fitted float reading low must report empty, or the pump runs dry"
        );
    }
}
