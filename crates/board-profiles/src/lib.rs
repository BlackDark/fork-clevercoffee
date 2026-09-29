//! The three pin maps, as data.
//!
//! The pin maps are the part of the board support that can be wrong in a way nobody notices: a
//! number that is one out puts a relay on a flash pin, and the machine boots and then does nothing
//! interesting. So they live here, in a crate with no hardware dependency, where a host test can
//! check every map against the constraints in
//! [`docs/rust-migration/board-pinouts.md`](../../docs/rust-migration/board-pinouts.md): no two
//! signals share a pin, nothing lands on the flash bus, USB, UART0 or JTAG, and the C6 fits its
//! fourteen usable pins.
//!
//! The `bsp-*` crates turn these numbers into pins. That part is thin, and it is the part a host
//! test cannot honestly verify, which is why the numbers and the rules about them are here rather
//! than inside the HAL glue.
//!
//! # Why the ESP32 map differs from the C++ one
//!
//! Two changes, both forced by the evidence and both recorded in the pinout document: the heater
//! relay moved off GPIO2, which is a boot-mode strapping pin, and the steam LED moved off GPIO1,
//! which is UART0 TX. Everything else is kept where the C++ had it, because an existing machine
//! has wire soldered to those pins.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

/// A pin's electrical role. Not its direction: a pin's direction is a property of the signal, and
/// one pin can be an input on one board and a bidirectional bus on another.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A relay or a lamp. Driven, and de-energised before anything else at boot.
    Output,
    /// A panel switch or a tank sensor. Sampled, with a pull where the chip has one.
    Input,
    /// The 1-Wire bus: bidirectional, open-drain, with an external pull-up.
    Bidirectional,
}

/// One signal's pin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pin {
    pub gpio: u8,
    pub kind: Kind,
    /// True when the board must supply an external pull, because the chip has none for this pin.
    ///
    /// The four panel switches on the original ESP32 are on input-only pins 34 to 39, which have no
    /// internal pull at all. A machine wired to the C++ map therefore has external resistors, and
    /// the firmware must not pretend otherwise.
    pub needs_external_pull: bool,
}

impl Pin {
    pub const fn output(gpio: u8) -> Self {
        Self {
            gpio,
            kind: Kind::Output,
            needs_external_pull: false,
        }
    }

    pub const fn input(gpio: u8) -> Self {
        Self {
            gpio,
            kind: Kind::Input,
            needs_external_pull: false,
        }
    }

    /// An input on a pin the chip cannot pull, which needs a resistor on the board.
    pub const fn pulled_input(gpio: u8) -> Self {
        Self {
            gpio,
            kind: Kind::Input,
            needs_external_pull: true,
        }
    }

    pub const fn bus(gpio: u8) -> Self {
        Self {
            gpio,
            kind: Kind::Bidirectional,
            needs_external_pull: false,
        }
    }
}

/// Which optional hardware a board can drive.
///
/// The C6 has fourteen usable pins against the seventeen this project needs, so the user decided
/// (2026-09-29) that the three indicator LEDs and the second load cell are disabled there rather
/// than adding an expander. This is the compile-time capability the shared logic reads, so a board
/// without an LED does not get a pin map with a zero in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capabilities {
    pub status_led: bool,
    pub brew_led: bool,
    pub steam_led: bool,
    pub scale: bool,
    /// A second HX711 load cell. Only the ESP32 and the S3 have pins for it.
    pub scale_second_cell: bool,
    pub pressure_sensor: bool,
}

impl Capabilities {
    pub const fn full() -> Self {
        Self {
            status_led: true,
            brew_led: true,
            steam_led: true,
            scale: true,
            scale_second_cell: true,
            pressure_sensor: true,
        }
    }

    /// The C6's reduced set: everything but the LEDs and the second cell.
    pub const fn reduced() -> Self {
        Self {
            status_led: false,
            brew_led: false,
            steam_led: false,
            scale: true,
            scale_second_cell: false,
            pressure_sensor: true,
        }
    }
}

/// A whole board.
#[derive(Clone, Copy, Debug)]
pub struct Board {
    /// The cargo feature and the config value that select this board.
    pub id: BoardId,
    pub name: &'static str,
    pub pins: Pins,
    pub capabilities: Capabilities,
    /// The provisioning transport this board can use. The ESP32 has no native USB, so it is UART0
    /// only; the S3 and C6 have both, and the operator picks by plugging into the right port.
    pub uart_provisioning: bool,
    pub usb_provisioning: bool,
    pub flash_mb: u8,
    pub psram_mb: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BoardId {
    Esp32 = 0,
    Esp32s3 = 1,
    Esp32c6 = 2,
}

impl BoardId {
    pub const fn as_str(self) -> &'static str {
        match self {
            BoardId::Esp32 => "esp32",
            BoardId::Esp32s3 => "esp32s3",
            BoardId::Esp32c6 => "esp32c6",
        }
    }

    /// Parses the cargo feature name.
    pub const fn from_feature(feature: &str) -> Option<Self> {
        match feature.as_bytes() {
            b"board-esp32" => Some(BoardId::Esp32),
            b"board-esp32s3" => Some(BoardId::Esp32s3),
            b"board-esp32c6" => Some(BoardId::Esp32c6),
            _ => None,
        }
    }
}

/// Every signal this project uses, in one order so the three maps line up when read side by side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Signal {
    HeaterRelay = 0,
    PumpRelay,
    ValveRelay,
    StatusLed,
    BrewLed,
    SteamLed,
    PowerSwitch,
    BrewSwitch,
    SteamSwitch,
    HotWaterSwitch,
    WaterTank,
    OneWire,
    Hx711Data1,
    Hx711Data2,
    Hx711Clock,
    I2cSda,
    I2cScl,
}

impl Signal {
    /// Seventeen signals, which is the count the pin budget in `board-pinouts.md` section 2 is
    /// computed from. There is no steam-valve pin: the machine's steam state drives the heater
    /// only, and the domain's actuator table never opens a steam valve.
    pub const ALL: [Signal; 17] = [
        Signal::HeaterRelay,
        Signal::PumpRelay,
        Signal::ValveRelay,
        Signal::StatusLed,
        Signal::BrewLed,
        Signal::SteamLed,
        Signal::PowerSwitch,
        Signal::BrewSwitch,
        Signal::SteamSwitch,
        Signal::HotWaterSwitch,
        Signal::WaterTank,
        Signal::OneWire,
        Signal::Hx711Data1,
        Signal::Hx711Data2,
        Signal::Hx711Clock,
        Signal::I2cSda,
        Signal::I2cScl,
    ];

    /// The essentials: the signals without which the machine cannot brew.
    ///
    /// This is the list the C6 pin budget is computed against, so the budget test and the
    /// capability flags cannot disagree about what "essential" means.
    pub const ESSENTIAL: [Signal; 12] = [
        Signal::HeaterRelay,
        Signal::PumpRelay,
        Signal::ValveRelay,
        Signal::PowerSwitch,
        Signal::BrewSwitch,
        Signal::SteamSwitch,
        Signal::HotWaterSwitch,
        Signal::WaterTank,
        Signal::OneWire,
        Signal::Hx711Data1,
        Signal::Hx711Clock,
        Signal::I2cSda,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Signal::HeaterRelay => "heater_relay",
            Signal::PumpRelay => "pump_relay",
            Signal::ValveRelay => "valve_relay",
            Signal::StatusLed => "status_led",
            Signal::BrewLed => "brew_led",
            Signal::SteamLed => "steam_led",
            Signal::PowerSwitch => "power_switch",
            Signal::BrewSwitch => "brew_switch",
            Signal::SteamSwitch => "steam_switch",
            Signal::HotWaterSwitch => "hot_water_switch",
            Signal::WaterTank => "water_tank",
            Signal::OneWire => "one_wire",
            Signal::Hx711Data1 => "hx711_d1",
            Signal::Hx711Data2 => "hx711_d2",
            Signal::Hx711Clock => "hx711_clk",
            Signal::I2cSda => "i2c_sda",
            Signal::I2cScl => "i2c_scl",
        }
    }
}

/// A pin map. `None` for a signal the board cannot drive.
#[derive(Clone, Copy, Debug)]
pub struct Pins {
    map: [Option<Pin>; 17],
}

impl Pins {
    const fn new(entries: [(Signal, Option<Pin>); 17]) -> Self {
        let mut map = [None; 17];
        let mut i = 0;
        while i < entries.len() {
            map[entries[i].0 as usize] = entries[i].1;
            i += 1;
        }
        Self { map }
    }

    /// `const` so a board crate can assert at compile time that the pin it hard-codes for the HAL
    /// is the pin the profile says, which is the only way a hand-written macro arm and a data table
    /// can be kept from drifting apart.
    pub const fn get(&self, signal: Signal) -> Option<Pin> {
        self.map[signal as usize]
    }

    /// The GPIO number for a signal, for a `const` caller.
    ///
    /// Panics if the board does not drive the signal, which in a `const` context is a compile
    /// error. That is the right outcome: a board crate asking for a pin the board does not have is
    /// a mistake, and a silent zero would put a peripheral on a pin nothing is wired to.
    pub const fn gpio(&self, signal: Signal) -> u8 {
        match self.get(signal) {
            Some(pin) => pin.gpio,
            None => panic!("this board does not drive that signal"),
        }
    }

    /// Whether the board drives this signal at all.
    pub const fn has(&self, signal: Signal) -> bool {
        self.map[signal as usize].is_some()
    }

    /// Every signal this board drives, in declaration order.
    pub fn signals(&self) -> impl Iterator<Item = (Signal, Pin)> + '_ {
        Signal::ALL
            .into_iter()
            .filter_map(move |s| self.map[s as usize].map(|p| (s, p)))
    }

    /// The signals that must be driven for the machine to brew.
    pub fn essential(&self) -> impl Iterator<Item = (Signal, Pin)> + '_ {
        Signal::ESSENTIAL
            .into_iter()
            .filter_map(move |s| self.map[s as usize].map(|p| (s, p)))
    }

    /// How many distinct GPIOs the map uses.
    pub fn pin_count(&self) -> usize {
        let mut seen = heapless::FnvIndexSet::<u8, 32>::new();
        for (_, pin) in self.signals() {
            let _ = seen.insert(pin.gpio);
        }
        seen.len()
    }
}

/// ESP32-DevKitC V4, as settled in `board-pinouts.md` section 5.1.
pub const ESP32: Board = Board {
    id: BoardId::Esp32,
    name: "ESP32-DevKitC V4",
    pins: Pins::new([
        (Signal::HeaterRelay, Some(Pin::output(4))),
        (Signal::PumpRelay, Some(Pin::output(27))),
        (Signal::ValveRelay, Some(Pin::output(17))),
        (Signal::StatusLed, Some(Pin::output(26))),
        (Signal::BrewLed, Some(Pin::output(19))),
        (Signal::SteamLed, Some(Pin::output(21))),
        // Input-only pins, so the board must supply the pull. The C++ firmware relied on this too
        // and documented nothing, which is how a machine with no resistors gets a switch that reads
        // as permanently pressed.
        (Signal::PowerSwitch, Some(Pin::pulled_input(39))),
        (Signal::BrewSwitch, Some(Pin::pulled_input(34))),
        (Signal::SteamSwitch, Some(Pin::pulled_input(35))),
        (Signal::HotWaterSwitch, Some(Pin::pulled_input(36))),
        (Signal::WaterTank, Some(Pin::input(23))),
        (Signal::OneWire, Some(Pin::bus(16))),
        (Signal::Hx711Data1, Some(Pin::input(32))),
        (Signal::Hx711Data2, Some(Pin::input(25))),
        (Signal::Hx711Clock, Some(Pin::output(33))),
        (Signal::I2cSda, Some(Pin::bus(21))),
        (Signal::I2cScl, Some(Pin::bus(15))),
    ]),
    capabilities: Capabilities::full(),
    uart_provisioning: true,
    usb_provisioning: false,
    flash_mb: 4,
    psram_mb: 0,
};

/// ESP32-S3-DevKitC-1 v1.1, section 5.2.
pub const ESP32S3: Board = Board {
    id: BoardId::Esp32s3,
    name: "ESP32-S3-DevKitC-1 v1.1",
    pins: Pins::new([
        (Signal::HeaterRelay, Some(Pin::output(4))),
        (Signal::PumpRelay, Some(Pin::output(5))),
        (Signal::ValveRelay, Some(Pin::output(6))),
        (Signal::StatusLed, Some(Pin::output(21))),
        (Signal::BrewLed, Some(Pin::output(47))),
        (Signal::SteamLed, Some(Pin::output(48))),
        (Signal::PowerSwitch, Some(Pin::input(1))),
        (Signal::BrewSwitch, Some(Pin::input(2))),
        (Signal::SteamSwitch, Some(Pin::input(3))),
        (Signal::HotWaterSwitch, Some(Pin::input(10))),
        (Signal::WaterTank, Some(Pin::input(11))),
        (Signal::OneWire, Some(Pin::bus(12))),
        (Signal::Hx711Data1, Some(Pin::input(13))),
        (Signal::Hx711Data2, Some(Pin::input(14))),
        (Signal::Hx711Clock, Some(Pin::output(15))),
        (Signal::I2cSda, Some(Pin::bus(8))),
        (Signal::I2cScl, Some(Pin::bus(9))),
    ]),
    capabilities: Capabilities::full(),
    uart_provisioning: true,
    usb_provisioning: true,
    flash_mb: 8,
    psram_mb: 8,
};

/// ESP32-C6-DevKitC-1 v1.2, section 5.3. The three LEDs and the second load cell are absent.
pub const ESP32C6: Board = Board {
    id: BoardId::Esp32c6,
    name: "ESP32-C6-DevKitC-1 v1.2",
    pins: Pins::new([
        (Signal::HeaterRelay, Some(Pin::output(10))),
        (Signal::PumpRelay, Some(Pin::output(11))),
        (Signal::ValveRelay, Some(Pin::output(2))),
        // The three LEDs and the second load cell do not fit: fourteen usable pins against
        // seventeen signals. Disabled by capability rather than mapped to a wrong pin, which is the
        // user's decision of 2026-09-29.
        (Signal::StatusLed, None),
        (Signal::BrewLed, None),
        (Signal::SteamLed, None),
        (Signal::PowerSwitch, Some(Pin::input(0))),
        (Signal::BrewSwitch, Some(Pin::input(1))),
        (Signal::SteamSwitch, Some(Pin::input(3))),
        (Signal::HotWaterSwitch, Some(Pin::input(15))),
        (Signal::WaterTank, Some(Pin::input(4))),
        (Signal::OneWire, Some(Pin::bus(5))),
        (Signal::Hx711Data1, Some(Pin::input(6))),
        (Signal::Hx711Data2, None),
        (Signal::Hx711Clock, Some(Pin::output(7))),
        (Signal::I2cSda, Some(Pin::bus(9))),
        (Signal::I2cScl, Some(Pin::bus(16))),
    ]),
    capabilities: Capabilities::reduced(),
    uart_provisioning: false,
    usb_provisioning: true,
    flash_mb: 8,
    psram_mb: 0,
};

/// All three boards.
pub const BOARDS: [Board; 3] = [ESP32, ESP32S3, ESP32C6];

/// The board a feature name selects.
pub fn board_for(id: BoardId) -> Board {
    match id {
        BoardId::Esp32 => ESP32,
        BoardId::Esp32s3 => ESP32S3,
        BoardId::Esp32c6 => ESP32C6,
    }
}

/// The pins a chip cannot use, from `board-pinouts.md` section 4.4.
pub const fn unusable(id: BoardId) -> &'static [u8] {
    match id {
        // The SPI flash bus (D0 to CLK) and the USB bridge's UART0. GPIO0 and GPIO2 are strapping
        // pins, not unusable ones: they are legal once the pin is in its inactive state at reset,
        // which is why the boot order drives the relays off before anything else.
        BoardId::Esp32 => &[1, 3, 6, 7, 8, 9, 10, 11],
        // Native USB, UART0, JTAG, and the octal PSRAM and flash bus on the N8R8 module.
        BoardId::Esp32s3 => &[
            19, 20, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 39, 40, 41, 42, 43, 44, 45, 46,
        ],
        // The SDIO flash bus, native USB and the on-board RGB LED. GPIO16 and 17 are UART0 on the
        // bridge and are therefore given up for *provisioning*, but they remain legal GPIOs, which
        // is why the C6 uses 16 for I2C SCL.
        BoardId::Esp32c6 => &[8, 12, 13, 18, 19, 20, 21, 22, 23],
    }
}

/// How many GPIOs a chip exposes for this project, after the unusable ones.
pub const fn usable_pins(id: BoardId) -> usize {
    match id {
        // The DevKitC V4 breaks out 34 pins; the flash bus and UART0 take 11 of them.
        BoardId::Esp32 => 23,
        // Comfortably more than the seventeen this project needs.
        BoardId::Esp32s3 => 40,
        // Fourteen, per the vendor's own J1 and J3 tables. This is the number the C6 budget is
        // checked against, and it is the finding that changed the plan.
        BoardId::Esp32c6 => 14,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_boards_have_the_essentials() {
        for b in BOARDS {
            for signal in Signal::ESSENTIAL {
                assert!(
                    b.pins.has(signal),
                    "{} cannot drive {}",
                    b.name,
                    signal.name()
                );
            }
        }
    }

    #[test]
    fn no_signal_shares_a_pin_on_a_board_that_has_it() {
        // A duplicate is a wire that two peripherals fight over, and it is invisible in review
        // because each row looks right on its own.
        for b in BOARDS {
            let mut seen = heapless::FnvIndexSet::<u8, 32>::new();
            for (signal, pin) in b.pins.signals() {
                assert!(
                    seen.insert(pin.gpio).is_ok(),
                    "{}: {} and another signal share GPIO {}",
                    b.name,
                    signal.name(),
                    pin.gpio
                );
            }
        }
    }

    #[test]
    fn no_board_uses_a_pin_the_chip_cannot() {
        for b in BOARDS {
            for (signal, pin) in b.pins.signals() {
                assert!(
                    !unusable(b.id).contains(&pin.gpio),
                    "{}: {} is on GPIO {}, which the chip cannot use",
                    b.name,
                    signal.name(),
                    pin.gpio
                );
            }
        }
    }

    #[test]
    fn the_c6_fits_its_fourteen_usable_pins() {
        // The finding that changed the plan: seventeen signals, fourteen pins, so the LEDs and
        // the second load cell go. Asserted rather than described, so a future pin map that quietly
        // uses seventeen distinct pins fails here.
        let c6 = ESP32C6;
        assert!(
            c6.pins.pin_count() <= usable_pins(BoardId::Esp32c6),
            "the C6 map uses {} pins and the chip has {} usable",
            c6.pins.pin_count(),
            usable_pins(BoardId::Esp32c6)
        );
        assert!(!c6.capabilities.status_led);
        assert!(!c6.capabilities.brew_led);
        assert!(!c6.capabilities.steam_led);
        assert!(!c6.capabilities.scale_second_cell);
        assert!(c6.capabilities.scale, "a single-cell scale still fits");
        assert!(c6.capabilities.pressure_sensor);
    }

    #[test]
    fn the_esp32_and_s3_keep_their_optional_hardware() {
        for b in [ESP32, ESP32S3] {
            assert!(
                b.capabilities.status_led && b.capabilities.brew_led && b.capabilities.steam_led
            );
            assert!(b.capabilities.scale && b.capabilities.scale_second_cell);
        }
    }

    #[test]
    fn the_esp32_switches_are_the_input_only_ones_and_say_so() {
        // GPIO 34 to 39 on the ESP32 have no internal pull. A machine without external resistors
        // reads a switch as permanently pressed, and the firmware has to know that is possible.
        for signal in [
            Signal::PowerSwitch,
            Signal::BrewSwitch,
            Signal::SteamSwitch,
            Signal::HotWaterSwitch,
        ] {
            let pin = ESP32.pins.get(signal).expect("mapped");
            assert!(
                (34..=39).contains(&pin.gpio),
                "{} is not on an input-only pin",
                signal.name()
            );
            assert!(
                pin.needs_external_pull,
                "{} must declare the external pull it depends on",
                signal.name()
            );
        }
    }

    #[test]
    fn the_s3_and_c6_switches_do_not_claim_to_need_an_external_pull() {
        for b in [ESP32S3, ESP32C6] {
            for signal in [
                Signal::PowerSwitch,
                Signal::BrewSwitch,
                Signal::SteamSwitch,
                Signal::HotWaterSwitch,
            ] {
                let pin = b.pins.get(signal).expect("mapped");
                assert!(
                    !pin.needs_external_pull,
                    "{}: {} has an internal pull on this chip",
                    b.name,
                    signal.name()
                );
            }
        }
    }

    #[test]
    fn a_pin_the_chip_cannot_use_is_never_in_a_map() {
        // The list itself is worth asserting: an entry added to it that no map uses is a
        // documentation change, and one removed by accident is a pin that goes on the flash bus.
        assert!(unusable(BoardId::Esp32).contains(&6), "the ESP32 flash bus");
        assert!(
            unusable(BoardId::Esp32s3).contains(&19),
            "the S3 native USB pair"
        );
        assert!(
            unusable(BoardId::Esp32c6).contains(&12),
            "the C6 native USB pair"
        );
    }

    #[test]
    fn only_the_esp32_provisions_over_uart_and_only_it_lacks_usb() {
        // The decision record's whole point: the ESP32 has no native USB, and a USB CDC build for
        // it would compile and never receive a credential.
        // A const block, because these are constants: the point is that the property is checked
        // at compile time, not that a test notices it later.
        const {
            assert!(ESP32.uart_provisioning && !ESP32.usb_provisioning);
            assert!(ESP32S3.uart_provisioning && ESP32S3.usb_provisioning);
            assert!(!ESP32C6.uart_provisioning && ESP32C6.usb_provisioning);
        }
    }

    #[test]
    fn a_feature_name_selects_exactly_one_board() {
        assert_eq!(BoardId::from_feature("board-esp32"), Some(BoardId::Esp32));
        assert_eq!(
            BoardId::from_feature("board-esp32s3"),
            Some(BoardId::Esp32s3)
        );
        assert_eq!(
            BoardId::from_feature("board-esp32c6"),
            Some(BoardId::Esp32c6)
        );
        assert_eq!(BoardId::from_feature("mock-actuators"), None);
        assert_eq!(BoardId::from_feature("esp32"), None);
    }

    #[test]
    fn the_esp32_map_moved_off_the_two_pins_the_evidence_forbids() {
        // GPIO2 is a boot-mode strapping pin and GPIO1 is UART0 TX. Both were in the C++ map.
        assert_ne!(ESP32.pins.get(Signal::HeaterRelay).unwrap().gpio, 2);
        assert_ne!(ESP32.pins.get(Signal::SteamLed).unwrap().gpio, 1);
    }

    #[test]
    fn every_signal_name_is_unique() {
        let mut seen = heapless::FnvIndexSet::<&str, 32>::new();
        for s in Signal::ALL {
            assert!(seen.insert(s.name()).is_ok(), "{} appears twice", s.name());
        }
        assert_eq!(
            Signal::ALL.len(),
            17,
            "the pin budget counts seventeen signals"
        );
    }
}
