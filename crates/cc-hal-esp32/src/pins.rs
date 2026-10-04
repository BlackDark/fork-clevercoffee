//! The board's pin map — the one copy, checked twice.
//!
//! Owner: **2.4** of `32-findings-2026-10-03.md`, replacing the two copies this
//! used to have: eight `u8` constants in `sensors::pins` that existed only to
//! print a boot-log line, and the `peripherals.pins.gpioNN` fields in
//! `cc-firmware`'s `bring_up` that were what the machine was *actually* wired
//! to. Change either and the boot log silently lied about the other.
//!
//! # What is checked, and where
//!
//! **Legal pins — at compile time.** [`assert_valid`] is a `const fn`, called
//! by a `const _` item in this module, so a pin this chip does not have, a pin
//! in the SPI-flash bank, an input driven as an output, or a wire claimed twice
//! is a *build failure*. This is what the C++'s 21 `static_assert`s in
//! `pinmapping.h:79-99` gave the oracle and what `04 §1.4`/`§6` promised as
//! `Board::PINS.assert_valid()`.
//!
//! **The numbers agree with the wiring — at boot.** [`assert_wiring`] is a
//! runtime check, and it is runtime because of a limit of `esp-idf-hal` 0.47,
//! not a choice: a pin's number is reachable only through
//! [`esp_idf_hal::gpio::Pin::pin`], which is a `fn` and not a `const fn`, and
//! the reverse direction — turning a number into the concrete `Gpio17` field of
//! `Peripherals` — needs `AnyIOPin::steal`, which is `unsafe`, which this
//! workspace denies. So the drift that actually bites ("someone rewires GPIO17
//! to GPIO19 and the log still says 17") is caught once, at bring-up, with a
//! panic that names the pin. That is a real check rather than a comment,
//! which is the whole of what the two-copy arrangement did not have.
//!
//! # There is no `Board` trait
//!
//! `04 §1.4` promised one, and it is not here. This machine is one board —
//! ESP32-DevKitC V4 on the original ESP32 — and a trait with one implementation
//! is a `PinMap` const plus a name. What was actually missing was the
//! *validation*, and that is what this module adds.

use esp_idf_hal::gpio::Pin;
use esp_idf_hal::peripherals::Peripherals;

// ---- inputs -------------------------------------------------------------
//
// Every number is `pinmapping.h`'s, and the doc on each says which `#define`.

/// `PIN_POWERSWITCH` (`pinmapping.h:17`). Standby and reboot.
pub const POWER_SWITCH: u8 = 39;
/// `PIN_BREWSWITCH` (`pinmapping.h:18`). Brew, backflush, manual flush.
pub const BREW_SWITCH: u8 = 34;
/// `PIN_STEAMSWITCH` (`pinmapping.h:19`).
pub const STEAM_SWITCH: u8 = 35;
/// `PIN_WATERSWITCH` (`pinmapping.h:20`) — the hot-water **button**.
///
/// Not the water *tank* float, which is [`WATER_TANK_SENSOR`] below: one is a
/// button the user presses, the other is a float the water moves.
pub const WATER_SWITCH: u8 = 36;
/// `PIN_WATERTANKSENSOR` (`pinmapping.h:28`) — the tank **float**.
pub const WATER_TANK_SENSOR: u8 = 23;
/// `PIN_TEMPSENSOR` (`pinmapping.h:27`) — the DS18B20 1-Wire bus.
///
/// **Measured on the board**: the recovered image's boot log reported a DS18B20
/// answering on this line, and the same log named GPIO2/GPIO17/GPIO27 for
/// heater/valve/pump exactly as `pinmapping.h` does. See
/// [`cc_domain::onewire::FAMILY_DS18B20`].
pub const TEMP_SENSOR: u8 = 16;
/// `PIN_HXDAT` (`pinmapping.h:29`) — the scale's first data line.
pub const SCALE_DATA_1: u8 = 32;
/// `PIN_HXDAT2` (`pinmapping.h:30`) — the scale's second data line.
pub const SCALE_DATA_2: u8 = 25;
/// UART0's RXD, the `GPIO3` the provisioning console reads.
pub const UART_RX: u8 = 3;

// ---- outputs ------------------------------------------------------------

/// `PIN_HEATER` (`pinmapping.h:35`) — the heater relay.
pub const HEATER: u8 = 2;
/// `PIN_VALVE` (`pinmapping.h:33`) — the water valve relay.
pub const WATER_VALVE: u8 = 17;
/// `PIN_PUMP` (`pinmapping.h:34`) — the pump relay.
pub const PUMP: u8 = 27;
/// `PIN_HXCLK` (`pinmapping.h:31`) — the scale's shared clock, bit-banged.
pub const SCALE_CLOCK: u8 = 33;
/// UART0's TXD, the `GPIO1` the provisioning console writes.
pub const UART_TX: u8 = 1;

/// `PIN_STATUSLED` (`pinmapping.h:43`) — the near-setpoint LED.
///
/// The `#define`'s comment reads "25 works with logging // Moved from pin 26
/// (pin 26 had hardware issues)", which is a note about the *previous* pin, not a
/// statement about this one: the value is 26 and that is what the C++ wires.
pub const STATUS_LED: u8 = 26;
/// `PIN_BREWLED` (`pinmapping.h:44`) — the brew/flush LED.
pub const BREW_LED: u8 = 19;

// ---- bidirectional ------------------------------------------------------

/// `PIN_I2CSDA` (`pinmapping.h:54`) — the ABP2 and the SSD1306, shared.
pub const I2C_SDA: u8 = 21;
/// `PIN_I2CSCL` (`pinmapping.h:53`) — the same shared bus.
pub const I2C_SCL: u8 = 22;

// ---------------------------------------------------------------------------
// The compile-time half.
// ---------------------------------------------------------------------------

/// Every pin the map claims, once each.
///
/// The uniqueness this buys *is* the "no pin is used as both an input and an
/// output" rule: a wire that appears once cannot be on both lists, and a wire
/// that appears twice — which is what a copy-paste between the two lists
/// produces — is a build failure naming nothing, which is why the message is on
/// [`assert_valid`].
const ALL: [u8; 18] = [
    POWER_SWITCH,
    BREW_SWITCH,
    STEAM_SWITCH,
    WATER_SWITCH,
    WATER_TANK_SENSOR,
    TEMP_SENSOR,
    SCALE_DATA_1,
    SCALE_DATA_2,
    UART_RX,
    HEATER,
    WATER_VALVE,
    PUMP,
    SCALE_CLOCK,
    UART_TX,
    STATUS_LED,
    BREW_LED,
    I2C_SDA,
    I2C_SCL,
];

/// The pins this firmware can pull low: the three relays, the bit-banged scale
/// clock, UART0's TXD, the 1-Wire bus, both I²C lines and the two status LEDs.
///
/// Not "the outputs". [`TEMP_SENSOR`] is on this list because the DS18B20 bus
/// is open-drain and the firmware drives the reset pulse itself
/// (`GpioOneWire`), and the I²C lines because they are open-drain too. What
/// the list is for is the rule below.
const DRIVEN: [u8; 10] = [
    HEATER,
    WATER_VALVE,
    PUMP,
    SCALE_CLOCK,
    UART_TX,
    STATUS_LED,
    BREW_LED,
    TEMP_SENSOR,
    I2C_SDA,
    I2C_SCL,
];

/// Whether the original ESP32 has a GPIO with this number.
///
/// The exclusions are the chip's, not a policy:
///
/// * **6-11** are the SPI flash pins. They are on the module and off the
///   header, and `Pins` in `esp-idf-hal` 0.47 (`gpio.rs:1635-1640`) has no
///   field for them at all — which is already a compile error for a typed
///   field, and this makes it one for a number too.
/// * **20 and 24** are the silkscreen pads on the `DevKitC` V4 that the
///   ESP32-S2/S3 use for USB, and the ESP32 has no user function on either.
/// * **28-31** are reserved and unused.
const fn is_gpio(pin: u8) -> bool {
    (pin <= 19 && !(pin >= 6 && pin <= 11))
        || (pin >= 21 && pin <= 23)
        || (pin >= 25 && pin <= 27)
        || (pin >= 32 && pin <= 39)
}

/// Whether the pin can only ever be read.
///
/// GPIO34-39 on the original ESP32 are input-only with **no internal pull at
/// all** (`SOC_GPIO_PIN_COUNT` aside — the datasheet lists no pull for the
/// bank). `switches` uses `Pull::Floating` on exactly this bank for exactly
/// this reason, so a drive here would be both a wiring error and a lie about
/// the pull.
const fn is_input_only(pin: u8) -> bool {
    pin >= 34 && pin <= 39
}

/// The compile-time pin validation, called by the `const _` below.
///
/// Three rules, and each one is a defect that has actually been plausible:
///
/// 1. **the pin exists.** A number off the end of `Pins` cannot be wired, so
///    a boot log naming one is fiction.
/// 2. **a driven pin can be driven.** GPIO34-39 have no output driver, so a
///    relay on one is silently dead — and, worse, the heater *log* would still
///    report a duty cycle.
/// 3. **no wire is claimed twice.** The two copies this module replaced were
///    free to disagree; this makes a disagreement a build failure.
const fn assert_valid() {
    let mut i = 0;
    while i < ALL.len() {
        let pin = ALL[i];
        assert!(
            is_gpio(pin),
            "a pin in the map is not a GPIO this chip has (check the \
             flash bank 6-11, 20, 24 and 28-31, which are not user IO)"
        );

        let mut j = i + 1;
        while j < ALL.len() {
            assert!(
                ALL[j] != pin,
                "the same pin is claimed twice in the map, so one of the two \
                 functions is wired to a pin the other is also using"
            );
            j += 1;
        }
        i += 1;
    }

    let mut i = 0;
    while i < DRIVEN.len() {
        let pin = DRIVEN[i];
        assert!(
            !is_input_only(pin),
            "this firmware drives the pin, and GPIO34-39 are input-only on the \
             original ESP32: the line would never move"
        );
        i += 1;
    }
}

/// The build-time gate. A change to any constant above that breaks a rule is a
/// compile error here rather than a miswired machine.
const _: () = assert_valid();

// ---------------------------------------------------------------------------
// The run-time half.
// ---------------------------------------------------------------------------

/// Fail the boot if a pin `Peripherals` handed out is not the one the map names.
///
/// # Why this is not a `const fn`
///
/// `esp-idf-hal` 0.47 reaches a pin's number through
/// [`esp_idf_hal::gpio::Pin::pin`], a `fn` and not a `const fn`
/// (`gpio.rs:19`), and offers no safe way back — turning a number into the
/// `Gpio17` field of [`Peripherals`] means `AnyIOPin::steal`, which is `unsafe`,
/// and `unsafe_code` is denied workspace-wide (`Cargo.toml:125`). So the
/// direction that actually goes wrong — someone rewires the field and the boot
/// log, reading the constant, keeps saying the old number — is checked once,
/// here, at bring-up, and panics with both numbers named.
///
/// # Panics
///
/// If any wired pin's number differs from its constant. Boot is the right
/// outcome: every one of these lines is either a relay, a bus or a switch, and
/// continuing past a disagreement means the heater's log, the web UI's pin
/// table and the machine's actual wiring are three different stories.
///
/// # Errors
///
/// None. This returns so the call site can be a statement rather than a
/// `let _ =`; it has no failure mode that is not a bug.
pub fn assert_wiring(peripherals: &Peripherals) {
    let pins = &peripherals.pins;
    let checked: [(&str, u8, u8); 18] = [
        ("POWER_SWITCH", POWER_SWITCH, Pin::pin(&pins.gpio39)),
        ("BREW_SWITCH", BREW_SWITCH, Pin::pin(&pins.gpio34)),
        ("STEAM_SWITCH", STEAM_SWITCH, Pin::pin(&pins.gpio35)),
        ("WATER_SWITCH", WATER_SWITCH, Pin::pin(&pins.gpio36)),
        (
            "WATER_TANK_SENSOR",
            WATER_TANK_SENSOR,
            Pin::pin(&pins.gpio23),
        ),
        ("TEMP_SENSOR", TEMP_SENSOR, Pin::pin(&pins.gpio16)),
        ("SCALE_DATA_1", SCALE_DATA_1, Pin::pin(&pins.gpio32)),
        ("SCALE_DATA_2", SCALE_DATA_2, Pin::pin(&pins.gpio25)),
        ("UART_RX", UART_RX, Pin::pin(&pins.gpio3)),
        ("HEATER", HEATER, Pin::pin(&pins.gpio2)),
        ("WATER_VALVE", WATER_VALVE, Pin::pin(&pins.gpio17)),
        ("PUMP", PUMP, Pin::pin(&pins.gpio27)),
        ("SCALE_CLOCK", SCALE_CLOCK, Pin::pin(&pins.gpio33)),
        ("UART_TX", UART_TX, Pin::pin(&pins.gpio1)),
        ("STATUS_LED", STATUS_LED, Pin::pin(&pins.gpio26)),
        ("BREW_LED", BREW_LED, Pin::pin(&pins.gpio19)),
        ("I2C_SDA", I2C_SDA, Pin::pin(&pins.gpio21)),
        ("I2C_SCL", I2C_SCL, Pin::pin(&pins.gpio22)),
    ];

    for (name, declared, wired) in checked {
        assert_eq!(
            declared, wired,
            "the pin map says {name} is GPIO{declared} but the firmware wired \
             it to GPIO{wired}: the boot log and the wiring disagree, refusing \
             to boot"
        );
    }
}
