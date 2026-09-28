//! The ABP2 pressure sensor and the debounced switches, over ESP-IDF.
//!
//! Owner: **R3-05** (pressure) and **R3-02** (switches).
//!
//! # What is here and what is not
//!
//! The decisions — when to read, the conversion arithmetic, the debounce state
//! machine — are in [`cc_domain::abp2`] and [`cc_domain::switch`] and are
//! host-tested there. This file supplies the two things those need from the
//! outside world: an I²C bus, and a pin.
//!
//! # I²C: the count the HAL does not give you
//!
//! `cc_domain::abp2::I2cBus::read` returns how many bytes were actually read,
//! because that is the check the C++ does not make. `hal::i2c`'s own `read`
//! does **not** return a count — it takes a `&mut [u8]` and returns
//! `Result<(), EspError>`
//! (`esp-idf-hal-0.47.0/src/i2c.rs:295-312`) — so [`Abp2I2c`] counts what it
//! got itself. On ESP-IDF a short read surfaces as an `EspError` from
//! `i2c_master_cmd_begin` rather than as a short buffer, so the count here is
//! the buffer length on success and the error is propagated on failure; the
//! short-read branch is therefore a defensive assertion rather than the main
//! path. It is kept because it costs nothing and it is the branch that matters
//! on a bus where a device can NACK mid-transfer.

use cc_domain::abp2::{self, I2cBus};
use cc_domain::hardware::{SwitchMode, SwitchType};
use cc_domain::switch::Debounced;
use cc_domain::units::Millis;
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::gpio::{Input, PinDriver, Pull};
use esp_idf_hal::i2c::I2cDriver;
use esp_idf_hal::i2c::I2C0;

use esp_idf_hal::units::Hertz;
use esp_idf_svc::sys::EspError;

/// The I²C clock the firmware runs the shared bus at.
///
/// The bus carries the OLED and the ABP2 (`pinmapping.h:53-54`). 400 kHz is the
/// ABP2's maximum and the SSD1306's comfortable maximum, so it is the one rate
/// both tolerate. Nothing in the C++ sets a rate explicitly — Arduino's
/// `Wire.begin()` defaults to 100 kHz — so this is a **change** from the C++,
/// and a deliberate one: the pressure read is 7 bytes and a faster bus is
/// strictly less bus time for a shared peripheral. It is recorded in
/// `intentional-diffs.md`.
pub const I2C_HZ: Hertz = Hertz(400_000);

/// The SDA pin handed to [`Abp2I2c::new`].
///
/// `AnyIOPin` rather than the concrete `Gpio21`, so the type does not fix the
/// pin number here: that is [`pins::I2C_SDA`]'s job, and the board layer
/// (R3-01) is what turns a number into a peripheral.
pub type SdaPin = esp_idf_hal::gpio::AnyIOPin<'static>;
/// The SCL pin handed to [`Abp2I2c::new`]. See [`SdaPin`].
pub type SclPin = esp_idf_hal::gpio::AnyIOPin<'static>;

/// The I²C bus, as [`cc_domain::abp2::I2cBus`] needs it.
///
/// A thin newtype over [`I2cDriver`]. The driver is a single owner of the
/// peripheral; `04 §7` makes the bus a shared, mutex-guarded resource, and
/// because this crate is `no_std` the mutex is R3-12's problem. What matters
/// here is that **every transaction is bounded**: three bytes out, seven in,
/// and a driver-level timeout, so a wedged device cannot hold the bus.
pub struct Abp2I2c<'d> {
    bus: I2cDriver<'d>,
}

impl<'d> Abp2I2c<'d> {
    /// Build the I²C driver with the clock and pull-ups this crate's
    /// [`I2C_HZ`] calls for, and wrap it.
    ///
    /// The peripheral is **passed in**, not taken here: `Peripherals::take()`
    /// may only be called once per process and belongs to `cc-firmware`'s
    /// `main`, which is also where the pin readback assertion happens. Taking it
    /// here would mean two places that believe they own the chip's peripherals.
    ///
    /// # Errors
    ///
    /// Whatever `i2c_new` or the configuration reports.
    pub fn new(peripheral: I2C0<'d>, sda: SdaPin, scl: SclPin) -> Result<Self, EspError> {
        // `I2cDriver::new` takes the peripheral and *then* the two pins
        // (`esp-idf-hal-0.47.0/src/i2c.rs:248-254`); it rejects a clock above
        // 1 MHz itself, so `I2C_HZ` cannot be out of range by accident.
        let config = esp_idf_hal::i2c::config::Config::new()
            .baudrate(I2C_HZ)
            .sda_enable_pullup(true)
            .scl_enable_pullup(true);
        let bus = I2cDriver::new(peripheral, sda, scl, &config)?;
        Ok(Self { bus })
    }

    /// Wrap an already-constructed I²C driver.
    ///
    /// The constructor for a caller that wants a different `Config` — a bus with
    /// external pull-ups and no internal ones, say, which is what R3-09's OLED
    /// may prefer. Sharing one driver between both devices is the point of
    /// `04 §7`'s "`I2cBus` — a mutex-guarded shared bus".
    #[must_use]
    pub const fn from_driver(bus: I2cDriver<'d>) -> Self {
        Self { bus }
    }
}

impl I2cBus for Abp2I2c<'_> {
    type Error = EspError;

    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        // A NAK on the address arrives as an `EspErrInvalidState` from
        // `i2c_master_cmd_begin`, which is the check the C++ omits by never
        // reading `stat`.
        self.bus.write(address, bytes, BLOCK)
    }

    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        self.bus.read(address, buffer, BLOCK)?;
        // `hal::i2c::read` fills the whole buffer or fails, so on success the
        // count is the length asked for. See the module docs.
        Ok(buffer.len())
    }
}

/// The ABP2, wired to a bus.
///
/// A newtype over [`cc_domain::abp2::Driver`] so the type says which sensor it
/// is at every call site; the domain driver underneath is the whole decision.
pub struct Abp2Pressure<'d> {
    bus: Abp2I2c<'d>,
    driver: abp2::Driver,
}

impl<'d> Abp2Pressure<'d> {
    /// Attach a sensor to a bus.
    #[must_use]
    pub const fn new(bus: Abp2I2c<'d>) -> Self {
        Self {
            bus,
            driver: abp2::Driver::new(),
        }
    }

    /// Advance the read. Never blocks.
    ///
    /// # Errors
    ///
    /// [`abp2::ReadError`], where the C++ silently produced a sample from stale
    /// bytes.
    pub fn poll(&mut self, now: Millis) -> Result<abp2::Poll, abp2::ReadError> {
        self.driver.poll(&mut self.bus, now)
    }

    /// The most recent decoded sample, if any.
    #[must_use]
    pub const fn last_sample(&self) -> Option<abp2::Sample> {
        self.driver.last_sample()
    }
}

/// A debounced switch on one GPIO.
///
/// A newtype over [`Debounced`], plus the pin. The pull is chosen by the caller
/// because it is a **wiring** decision, not a logic one — see
/// [`Pull`] and the note on the water tank below.
pub struct GpioIn<'d> {
    pin: PinDriver<'d, Input>,
    debounced: Debounced,
}

impl<'d> GpioIn<'d> {
    /// Take a GPIO as a debounced switch.
    ///
    /// `initial_raw` is the pin's rest level, and is what arms (or does not
    /// arm) the debounce on the first read. See [`Debounced::new`].
    #[must_use]
    pub const fn new(
        pin: PinDriver<'d, Input>,
        switch_type: SwitchType,
        mode: SwitchMode,
        initial_raw: u8,
    ) -> Self {
        Self {
            pin,
            debounced: Debounced::new(switch_type, mode, initial_raw),
        }
    }

    /// The pull-up/pull-down a switch of this wiring needs.
    ///
    /// `createWaterTankSensor` uses `IN_PULLDOWN` for a normally-open switch and
    /// `IN_PULLUP` for a normally-closed one
    /// (`src/core/SystemInitializer.cpp:71-74`). That choice is not arbitrary:
    /// a float switch is a pair of contacts, and the internal pull decides what
    /// an *unconnected* or *dry* line reads. For the water tank that is
    /// safety-relevant — see the note below.
    #[must_use]
    pub const fn pull_for(mode: SwitchMode) -> Pull {
        match mode {
            // Normally open: the contact pulls the line up when made, so the
            // rest state must be pulled *down*.
            SwitchMode::NormallyOpen => Pull::Down,
            // Normally closed: the rest state is the contact made, so pull up.
            SwitchMode::NormallyClosed => Pull::Up,
        }
    }

    /// The rest level a switch of this wiring idles at.
    ///
    /// `createWaterTankSensor` passes
    /// `initialState = (mode == NORMALLY_OPEN) ? HIGH : LOW`
    /// (`SystemInitializer.cpp:70`). The two look contradictory for a
    /// normally-open switch with a pull-down, and they are: `initialState` is
    /// the level the *debouncer* starts from, not the level the pin idles at.
    /// It only decides whether the first read arms the debounce.
    #[must_use]
    pub const fn initial_raw_for(mode: SwitchMode) -> u8 {
        match mode {
            SwitchMode::NormallyOpen => 1,
            SwitchMode::NormallyClosed => 0,
        }
    }

    /// Read the pin and advance the debounce.
    ///
    /// # Note on the water tank
    ///
    /// The tank switch is a `TOGGLE` (`SystemInitializer.cpp:75`) whose
    /// `isPressed()` means "water detected". The pull choice above is what makes
    /// a *broken* float switch fail towards empty rather than towards full: a
    /// normally-open tank switch with a pull-down reads low — no water — if the
    /// float is stuck up or the wire is cut, so S4 blocks the pump. Getting this
    /// backwards would let the pump run against a dry reservoir, so it is stated
    /// here and pinned in `cc_domain::switch`'s water-tank tests.
    pub fn update(&mut self, now: Millis) -> bool {
        let raw = u8::from(self.pin.is_high());
        self.debounced.update(raw, now)
    }

    /// Whether a long press is reported.
    ///
    /// Always `false` for a `TOGGLE`, per the C++ (`IOSwitch.cpp:57-59`).
    #[must_use]
    pub const fn long_press_detected(&self) -> bool {
        self.debounced.long_press_detected()
    }

    /// The debounced state, without re-reading the pin.
    #[must_use]
    pub const fn is_pressed(&self) -> bool {
        self.debounced.is_pressed()
    }
}

/// The four operator switches and the water-tank float, as the firmware builds
/// them.
///
/// The pin numbers are `pinmapping.h:17-20` and `:28`. Keeping them in one
/// `const` block is the point of `04 §4`'s compile-time pin assertion: a pin
/// that does not exist on the chip is a compile error, not a silent miswiring,
/// which is what the C++'s 21 `static_assert`s in `pinmapping.h:57-101` achieve
/// and what `Board::PINS.assert_valid()` (R3-01) will do for the whole map.
pub mod pins {
    /// `PIN_POWERSWITCH` (`pinmapping.h:17`).
    pub const POWER_SWITCH: u8 = 39;
    /// `PIN_BREWSWITCH` (`pinmapping.h:18`).
    pub const BREW_SWITCH: u8 = 34;
    /// `PIN_STEAMSWITCH` (`pinmapping.h:19`).
    pub const STEAM_SWITCH: u8 = 35;
    /// `PIN_WATERSWITCH` — the hot-water momentary switch (`pinmapping.h:20`).
    ///
    /// Not to be confused with the water *tank* float below: one is a button
    /// the user presses, the other is a float the water moves.
    pub const WATER_SWITCH: u8 = 36;
    /// `PIN_WATERTANKSENSOR` — the tank float switch (`pinmapping.h:28`).
    pub const WATER_TANK_SENSOR: u8 = 23;
    /// `PIN_TEMPSENSOR` — the DS18B20 1-Wire bus (`pinmapping.h:27`).
    ///
    /// **Measured on the board**: the recovered image's boot log reported a
    /// DS18B20 answering on this line, and the same log named GPIO2/GPIO17/GPIO27
    /// for heater/valve/pump exactly as `pinmapping.h` does, which is what makes
    /// the inference solid. See
    /// [`cc_domain::onewire::FAMILY_DS18B20`].
    pub const TEMP_SENSOR: u8 = 16;
    /// `PIN_I2CSDA` (`pinmapping.h:54`).
    pub const I2C_SDA: u8 = 21;
    /// `PIN_I2CSCL` (`pinmapping.h:53`).
    pub const I2C_SCL: u8 = 22;
}
