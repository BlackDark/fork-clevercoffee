//! The four operator switches and the water-tank float, as the firmware builds
//! them.
//!
//! Owner: **R4-01** (the wiring; the debounce arithmetic is `cc-domain`'s,
//! R3-02).
//!
//! # What this is
//!
//! [`SwitchBank`] owns five [`GpioIn`]s — power, brew, steam, hot water and the
//! tank float — reads them every control tick, and turns each debounced edge
//! into a [`cc_machine::Event`]. The reducer never sees a pin; it sees
//! `ButtonPressed` / `ButtonReleased`, which is the C++'s
//! `BrewHandler::processSwitchInput` contract
//! (`include/clevercoffee/handlers/BrewHandler.h:172`) expressed as a value.
//!
//! # The pin pull is `Floating` for the four operator switches, and that is not
//! an oversight
//!
//! GPIO34, 35, 36 and 39 are **input-only on the original ESP32 and have no
//! internal pull-up or pull-down at all** — the pad is a bare input. The C++
//! asks for `GPIOPin::IN_HARDWARE` (`src/hardware/HardwareManager.cpp:140,151,162,173`),
//! which is `pinMode(pin, INPUT)` (`GPIOPin.cpp:47-51`): floating, with the
//! board's external pull doing the work. [`GpioIn::pull_for`] would answer
//! `Pull::Down` for a normally-open switch, and ESP-IDF accepts that on GPIO34
//! without complaint while doing nothing — so this module uses
//! [`Pull::Floating`] for the operator switches and reserves
//! [`GpioIn::pull_for`] for the tank float, which is on GPIO23 and *does* have
//! an internal pull.
//!
//! Getting this backwards is a stuck-closed switch, not a wrong reading: a
//! floating input with no external pull wanders, and the debouncer will
//! eventually report a press. That is why the boot log prints each switch's
//! resting level after the first settling interval.
//!
//! # `initial_raw` is the *debouncer's* seed, not the pin's rest level
//!
//! `HardwareManager` passes `(mode == NORMALLY_OPEN) ? LOW : HIGH`
//! (`:139,150,161,172`) and `createWaterTankSensor` passes the opposite
//! `(mode == NORMALLY_OPEN) ? HIGH : LOW`
//! (`src/core/SystemInitializer.cpp:70`). They are not contradictory: the value
//! only decides whether the **first** read arms the 20 ms debounce, and
//! `Debounced::new` separately seeds the *state* to "not pressed" regardless
//! (`cc_domain::switch`, and `s2_the_c_tank_starts_full_and_the_switch_starts_open`).
//! Both are transcribed here rather than harmonised, because a machine whose
//! power switch boots "already on" is a different machine.

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

/// The pull for an operator switch: none, because the pin has none to give.
///
/// GPIO34/35/36/39 are input-only with no internal pull (`SOC_GPIO_PIN_COUNT`
/// aside, the original ESP32's datasheet lists no pull for the input-only bank),
/// and the C++ asks for `IN_HARDWARE`, which is a floating input. See the module
/// documentation.
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
    /// The C++ keeps one of these per handler (`BrewHandler::lastSwitchReading_`
    /// and its three siblings, `BrewHandler.h:172`). Here one small struct
    /// serves all five, and the reducer keeps its own copy in
    /// `Machine::switches` — the two are cross-checked, which is a test rather
    /// than a hope.
    previous: Levels,
    /// Whether the water-tank float is fitted at all.
    ///
    /// `hardware.sensors.watertank.enabled` is `false` by default, and the C++
    /// then has no float switch at all (`createWaterTankSensor` returns
    /// `nullptr`, `SystemInitializer.cpp:64-66`). A machine with no float must
    /// report **full**, or S4 would block the pump forever
    /// (`SensorCoordinator.h:260`'s "Assume full initially").
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
    /// the pull this board's wiring needs, which is the same thing
    /// `GPIOPin`'s constructor does.
    ///
    /// # Errors
    ///
    /// [`esp_idf_svc::sys::EspError`] from `PinDriver::input`. A switch that
    /// cannot be configured is a machine that cannot be operated by hand, which
    /// is worth failing the boot over — the C++ ignores the return of
    /// `pinMode` and reports a dead switch as a machine that does nothing.
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
        // The float is the one input whose pull matters: GPIO23 has one, and the
        // C++ asks for it (`SystemInitializer.cpp:71-74`).
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
    /// `hardware.switches.*.enabled` flags — all of which default to `false`,
    /// exactly as in the C++ (`Config.h:985,1004,1023,1042`).
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
    pub fn poll(&mut self, now: Millis) -> Vec<Event, MAX_EDGES_PER_TICK> {
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
            // `IOSwitch::longPressDetected()` is read **at the edge**, not after
            // the hold: `BrewHandler.h:199` consults it on the press that opens
            // `BACKFLUSH_IDLE` to choose manual flush over a backflush cycle.
            // So it is sampled here and carried in the event, because by the
            // time the reducer's per-tick work runs the flag has been cleared
            // by the release.
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
        // The order is power, brew, steam, hot water, tank: the C++'s
        // `initializeSwitches` order (`HardwareManager.cpp:130-175`), and the
        // read order is the order the boot log lists them in.
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
    /// One switch's level, so the edge loop reads like the C++'s
    /// `lastSwitchReading_[switch]`.
    const fn level(&self, switch: SwitchId) -> bool {
        match switch {
            SwitchId::Brew => self.brew,
            SwitchId::Steam => self.steam,
            SwitchId::Power => self.power,
            SwitchId::HotWater => self.hot_water,
        }
    }
}

/// What the machine believes about the water tank, given whether a float is
/// fitted and what it last read.
///
/// A machine with no float fitted reports **full**, which is the C++'s
/// `waterTankFull_ = true` ("Assume full initially",
/// `SensorCoordinator.h:260`) and the only safe answer: the alternative is a
/// machine whose pump is blocked by a sensor that does not exist.
///
/// ⚠ The fittedness test is an **exclusion, not a conjunction.** This
/// previously read `fitted && raw`, which returns `false` when no float is
/// fitted — the exact opposite of what the surrounding documentation, the field
/// documentation and the boot log all said. On hardware that put the machine
/// permanently in `WaterTankEmpty`; `should_pid_be_enabled` then cleared the
/// PID every tick, so **the heater could never come on at all.** It was
/// findable only because the boot log's own line ("when absent the machine
/// reports the tank FULL") printed directly above a `tank_full=false`.
///
/// The general form of the mistake: never gate a "assume the safe value"
/// fallback behind the presence of the thing it stands in for. When the sensor
/// is missing there is nothing to read, so the fallback *is* the answer.
///
/// It is a free function rather than a method so a host test can reach it:
/// [`SwitchBank`] owns five [`GpioIn`]s and cannot be built without pins, and
/// the bug it guards was a two-line decision that a type check cannot see.
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
/// `HardwareManager.cpp:139,150,161,172`:
/// `initialState = (mode == SwitchMode::NORMALLY_OPEN) ? LOW : HIGH` — the
/// **opposite** of the tank float's, and deliberately so; see the module
/// documentation. It is spelled out here rather than reusing
/// [`GpioIn::initial_raw_for`], which transcribes the float's line, because
/// reusing it would be a silent and load-bearing swap.
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
