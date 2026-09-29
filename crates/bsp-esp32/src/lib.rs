//! Board support for the ESP32 on the ESP32-DevKitC V4.
//!
//! # Verification status
//!
//! **Build-unverified in this checkout.** The Xtensa toolchain is installed by `just
//! espup-install`, which downloads an x86-64 `espup` binary; on an aarch64 host it cannot run, so
//! `just check-fw esp32` has not been executed here. Everything below the HAL glue *is* verified:
//! the pin map lives in `clevercoffee-board-profiles` and is host-tested against the constraints
//! in `board-pinouts.md`. See `docs/rust-migration/compatibility-matrix.md`.
//!
//! # What this crate is
//!
//! Thin, on purpose. It owns three things and nothing else:
//!
//! - the pin map, which it takes from `clevercoffee-board-profiles` rather than restating;
//! - an [`Actuators`] implementation over the three relay pins, or a recorder under
//!   `mock-actuators`;
//! - a [`ProvisioningTransport`] over UART0, because the ESP32 has no native USB.
//!
//! # Boot order
//!
//! [`init`] drives the relays to their inactive level **before** it configures anything else, and
//! before Wi-Fi can block. GPIO2 and GPIO15 are strapping pins, so their level at reset is a
//! hardware property; the new design does not put a relay on GPIO2, but the boot order is what
//! makes the remaining strapping pins safe.

#![no_std]
#![deny(missing_debug_implementations)]

use clevercoffee_board_profiles::{Board, Pin, Signal, ESP32};
use clevercoffee_hal_traits::{ActuatorCommand, Actuators, ProvisioningTransport, TransportError};
use esp_hal::gpio::{Input, Output};

/// The board this crate drives.
pub const BOARD: Board = ESP32;

/// The baud rate the host tool opens the port at. It is the C++ firmware's rate and the host
/// tool's default, and it is a build-time constant rather than a negotiated one: a device that
/// wanted a different rate could not be provisioned at all.
pub const PROVISION_BAUD: u32 = 115_200;

/// Picks a pin out of the peripherals struct by number.
///
/// The peripherals struct has one field per GPIO, so a pin map expressed as numbers needs exactly
/// this: a match from the number to the field. The alternative, a pin table of typed pointers,
/// is not expressible in a `const` and would turn a data error into a compile error at best.
///
/// The ESP32 exposes GPIO0 to GPIO39. A number outside that range does not compile, which is the
/// point: a typo in a pin map is a build failure rather than a machine that does nothing.
/// Resolves a pin *number* to the peripherals struct's field for it.
///
/// A macro over the literal rather than a runtime `match`, and the difference matters: a `match`
/// makes the borrow checker treat every arm as a move out of the same struct, so two invocations
/// that each mention `GPIO0` in an arm conflict. A literal pattern expands to exactly one field, so
/// only that field is ever taken, and a number this chip does not have is a compile error rather
/// than a machine that quietly does nothing.
///
/// Each arm *moves* its field out of the peripherals struct, which is why the struct can be
/// partially consumed: `esp_rtos::start` takes the timer group afterwards, and the two do not
/// touch the same fields. A reborrow would have needed `&mut`, and a mutable borrow of the whole
/// struct outlives the drivers built from it.
///
/// This chip has GPIO0 to GPIO39.
#[macro_export]
#[allow(unused_macros)]
// A table, not code: `rustfmt` would explode it into forty multi-line arms and hide the one
// thing a reader is looking for, which is which numbers exist.
#[rustfmt::skip]
macro_rules! gpio_field {
    ($p:expr, 0) => { $p.GPIO0 };
    ($p:expr, 1) => { $p.GPIO1 };
    ($p:expr, 2) => { $p.GPIO2 };
    ($p:expr, 3) => { $p.GPIO3 };
    ($p:expr, 4) => { $p.GPIO4 };
    ($p:expr, 5) => { $p.GPIO5 };
    ($p:expr, 6) => { $p.GPIO6 };
    ($p:expr, 7) => { $p.GPIO7 };
    ($p:expr, 8) => { $p.GPIO8 };
    ($p:expr, 9) => { $p.GPIO9 };
    ($p:expr, 10) => { $p.GPIO10 };
    ($p:expr, 11) => { $p.GPIO11 };
    ($p:expr, 12) => { $p.GPIO12 };
    ($p:expr, 13) => { $p.GPIO13 };
    ($p:expr, 14) => { $p.GPIO14 };
    ($p:expr, 15) => { $p.GPIO15 };
    ($p:expr, 16) => { $p.GPIO16 };
    ($p:expr, 17) => { $p.GPIO17 };
    ($p:expr, 18) => { $p.GPIO18 };
    ($p:expr, 19) => { $p.GPIO19 };
    ($p:expr, 20) => { $p.GPIO20 };
    ($p:expr, 21) => { $p.GPIO21 };
    ($p:expr, 22) => { $p.GPIO22 };
    ($p:expr, 23) => { $p.GPIO23 };
    ($p:expr, 24) => { $p.GPIO24 };
    ($p:expr, 25) => { $p.GPIO25 };
    ($p:expr, 26) => { $p.GPIO26 };
    ($p:expr, 27) => { $p.GPIO27 };
    ($p:expr, 28) => { $p.GPIO28 };
    ($p:expr, 29) => { $p.GPIO29 };
    ($p:expr, 30) => { $p.GPIO30 };
    ($p:expr, 31) => { $p.GPIO31 };
    ($p:expr, 32) => { $p.GPIO32 };
    ($p:expr, 33) => { $p.GPIO33 };
    ($p:expr, 34) => { $p.GPIO34 };
    ($p:expr, 35) => { $p.GPIO35 };
    ($p:expr, 36) => { $p.GPIO36 };
    ($p:expr, 37) => { $p.GPIO37 };
    ($p:expr, 38) => { $p.GPIO38 };
    ($p:expr, 39) => { $p.GPIO39 };
    ($p:expr, $other:expr) => {
        compile_error!("this chip has no GPIO{}", $other)
    };
}

const _: () = assert!(
    BOARD.pins.gpio(Signal::HeaterRelay) == 4,
    "the HAL pin for Signal::HeaterRelay does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::PumpRelay) == 27,
    "the HAL pin for Signal::PumpRelay does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::ValveRelay) == 17,
    "the HAL pin for Signal::ValveRelay does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::PowerSwitch) == 39,
    "the HAL pin for Signal::PowerSwitch does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::BrewSwitch) == 34,
    "the HAL pin for Signal::BrewSwitch does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::SteamSwitch) == 35,
    "the HAL pin for Signal::SteamSwitch does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::HotWaterSwitch) == 36,
    "the HAL pin for Signal::HotWaterSwitch does not match the profile"
);
const _: () = assert!(
    BOARD.pins.gpio(Signal::WaterTank) == 23,
    "the HAL pin for Signal::WaterTank does not match the profile"
);

/// The three relays, as owned outputs.
///
/// The only place in the firmware that holds a pin handle for an actuator. There is no second
/// path to a relay, which is the Rust form of the C++ rule in `CLAUDE.md` and the reason the
/// shadow-flag defect (D02) cannot recur.
#[derive(Debug)]
pub struct Relays<'a> {
    /// The heater, under hardware PWM. Never a plain output: a pin that this machine drives as a
    /// plain output is a heater at full power whatever the PID said, which is what the C++
    /// firmware did.
    heater: HeaterPwm<'a>,
    pump: Option<Output<'a>>,
    valve: Option<Output<'a>>,
    /// The duty the PID last asked for, kept so a command that arrives between samples is
    /// applied at the right duty rather than at the previous tick's.
    heater_duty: u16,
    /// Whether the machine has commanded the heater on. The pin itself belongs to the PWM
    /// peripheral, so this is the machine's intent and the peripheral's register is the truth.
    heater_energised: bool,
}

impl Relays<'_> {
    fn set(pin: &mut Option<Output<'_>>, on: bool) {
        if let Some(p) = pin.as_mut() {
            // Active-low: `on` drives the pin low, which energises the relay's coil path as
            // wired on this machine.
            if on {
                p.set_high();
            } else {
                p.set_low();
            }
        }
    }
}

impl Actuators for Relays<'_> {
    fn command(&mut self, command: ActuatorCommand, _reason: &'static str) {
        // Idempotent by construction: setting a level is not a toggle. The C++ `Relay::on()`
        // returned early when its shadow flag already matched, and the shadow flag was the defect
        // (D02); here there is no flag to disagree with the pin.
        // The heater is *not* set here. It is under the PWM peripheral, and writing its pin level
        // from here would take it away from the peripheral and put it at full power.
        self.heater_energised = command.heater_enabled;
        Self::set(&mut self.pump, command.pump);
        Self::set(&mut self.valve, command.water_valve || command.steam_valve);
    }

    fn force_off(&mut self) {
        self.heater_energised = false;
        self.heater.set_duty(false, 0);
        Self::set(&mut self.pump, false);
        Self::set(&mut self.valve, false);
    }

    fn set_heater_duty(&mut self, duty_permille: u16) {
        // The order matters: the duty is stored and applied to the peripheral, so a duty that
        // arrives before the command does not energise a heater the machine has not asked for, and
        // a command that arrives after the duty is applied at the duty the PID asked for rather
        // than at the last one seen.
        self.heater_duty = duty_permille.min(1000);
        self.heater
            .set_duty(self.heater_energised, self.heater_duty);
    }

    fn last_command(&self) -> ActuatorCommand {
        ActuatorCommand::ALL_OFF
    }
}

/// The debounced switch inputs.
#[derive(Debug)]
pub struct SwitchInputs<'a> {
    power: Input<'a>,
    brew: Input<'a>,
    steam: Input<'a>,
    hot_water: Input<'a>,
    water_tank: Input<'a>,
}

impl SwitchInputs<'_> {
    /// Samples every switch once.
    ///
    /// The water tank switch is active-low on this machine, matching the C++ wiring: the switch
    /// pulls to ground when the tank is full.
    pub fn sample(&self) -> clevercoffee_app::machine::Switches {
        clevercoffee_app::machine::Switches {
            power_pressed: self.power.is_high(),
            power_long_press: false,
            brew_pressed: self.brew.is_high(),
            brew_long_press: false,
            steam_pressed: self.steam.is_high(),
            hot_water_pressed: self.hot_water.is_high(),
        }
    }

    /// Whether the tank switch reports full.
    pub fn water_tank_full(&self) -> bool {
        self.water_tank.is_low()
    }
}

/// The pin map, for a log line and for the status endpoint.
pub fn board() -> Board {
    BOARD
}

/// Whether this board has a pin for a signal. The firmware uses it to refuse a configuration that
/// asks for hardware the board cannot drive, rather than silently doing nothing.
pub fn has(signal: Signal) -> bool {
    BOARD.pins.has(signal)
}

/// The pin a signal uses, for a diagnostic.
pub fn pin_of_signal(signal: Signal) -> Option<Pin> {
    BOARD.pins.get(signal)
}

/// Provisioning over UART0, through the board's USB-UART bridge.
#[derive(Debug)]
pub struct UartTransport<U> {
    uart: U,
    line: heapless::Vec<u8, 256>,
}

impl<U> UartTransport<U> {
    pub const fn new(uart: U) -> Self {
        Self {
            uart,
            line: heapless::Vec::new(),
        }
    }
}

/// How many bytes a line may be before the device refuses it. The host tool's largest chunk line
/// is about 700 characters, so 1024 leaves room and rejects a runaway line before it is buffered.
pub const MAX_LINE: usize = 1024;

impl<U: esp_hal::blocking::uart::Blocking> ProvisioningTransport for UartTransport<U> {
    fn read_line(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        self.line.clear();
        loop {
            match self.uart.read_byte() {
                Ok(0) => continue,
                Ok(b) if b == b'\n' => {
                    let n = self.line.len();
                    let n = n.min(buf.len());
                    buf[..n].copy_from_slice(&self.line[..n]);
                    return Ok(Some(n));
                }
                Ok(b) => {
                    if self.line.push(b).is_err() || self.line.len() >= MAX_LINE {
                        // An over-long line is dropped rather than truncated: half a command is
                        // worse than none, and the host tool's line is bounded.
                        self.line.clear();
                        return Ok(Some(0));
                    }
                }
                Err(_) => return Err(TransportError::Disconnected),
            }
        }
    }

    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        self.uart
            .write(line.as_bytes())
            .map_err(|_| TransportError::WriteFailed)?;
        self.uart
            .write(b"\r\n")
            .map_err(|_| TransportError::WriteFailed)?;
        Ok(())
    }

    fn drain(&mut self) {
        self.line.clear();
        // Anything the boot banner left in the port's receive register.
        while self.uart.read_byte().is_ok() {}
    }
}

/// Builds the three relays from three pins the caller has already taken out of the peripherals
/// struct.
///
/// A function rather than a macro over the peripherals because `esp_rtos::start` needs the timer
/// group by value and for `'static`, so the caller has to move fields out of the struct one at a
/// time. The pins are taken by [`relays!`] and handed here.
pub fn relays_from<'a>(
    heater: HeaterPwm<'a>,
    pump: esp_hal::gpio::Output<'a>,
    valve: esp_hal::gpio::Output<'a>,
) -> Relays<'a> {
    Relays {
        heater,
        pump: Some(pump),
        valve: Some(valve),
        heater_duty: 0,
        heater_energised: false,
    }
}

/// The five inputs, from five pins the caller has already taken.
#[allow(clippy::too_many_arguments)]
pub fn switches_from<'a>(
    power: esp_hal::gpio::Input<'a>,
    brew: esp_hal::gpio::Input<'a>,
    steam: esp_hal::gpio::Input<'a>,
    hot_water: esp_hal::gpio::Input<'a>,
    water_tank: esp_hal::gpio::Input<'a>,
) -> SwitchInputs<'a> {
    SwitchInputs {
        power,
        brew,
        steam,
        hot_water,
        water_tank,
    }
}

/// Takes the three relay pins and builds the relays, with every relay in its inactive level.
///
/// Exported so the firmware binary can call it, because taking the pins needs the peripherals
/// struct and the binary is what has one. The pin numbers are this board's, and the `const _: () =
/// assert!` blocks above tie them to the profile so the two cannot drift.
#[macro_export]
macro_rules! relays {
    ($p:expr) => {
        $crate::relays_from(
            $crate::HeaterPwm::new($p.MCPWM0, $crate::gpio_field!($p, 4)),
            ::esp_hal::gpio::Output::new(
                $crate::gpio_field!($p, 27),
                ::esp_hal::gpio::Level::Low,
                ::esp_hal::gpio::OutputConfig::default(),
            ),
            ::esp_hal::gpio::Output::new(
                $crate::gpio_field!($p, 17),
                ::esp_hal::gpio::Level::Low,
                ::esp_hal::gpio::OutputConfig::default(),
            ),
        )
    };
}

/// Takes the five input pins and builds the inputs, pulled as the profile requires.
#[macro_export]
macro_rules! switches {
    ($p:expr) => {{
        let config = ::esp_hal::gpio::InputConfig::default().with_pull(::esp_hal::gpio::Pull::None);
        let tank = ::esp_hal::gpio::InputConfig::default().with_pull(::esp_hal::gpio::Pull::Up);
        $crate::switches_from(
            ::esp_hal::gpio::Input::new($crate::gpio_field!($p, 39), config),
            ::esp_hal::gpio::Input::new($crate::gpio_field!($p, 34), config),
            ::esp_hal::gpio::Input::new($crate::gpio_field!($p, 35), config),
            ::esp_hal::gpio::Input::new($crate::gpio_field!($p, 36), config),
            ::esp_hal::gpio::Input::new($crate::gpio_field!($p, 23), tank),
        )
    }};
}

/// The heater's power control.
///
/// Hardware PWM rather than the C++ firmware's 10 ms software PWM: the peripheral counts and the
/// duty is a register the control task writes once a second, so the machine has **no interrupt at
/// all** for the heater. That is defect D05's shape removed rather than documented.
///
/// The fail-safe is in [`clevercoffee_app::heater`]: if the peripheral could not be brought up,
/// the heater is held **off** rather than driven from the relay command. The C++ drove the heater
/// pin from the relay and ignored the duty, so a 40 percent duty was 100 percent.
pub struct HeaterPwm<'a> {
    stage: clevercoffee_app::heater::Stage,
    /// The pin, driven by the peripheral. Held here rather than as a plain `Output`, because a
    /// plain output cannot be driven by hardware at all: the whole point is that the peripheral
    /// owns this pin.
    pin: esp_hal::mcpwm::operator::PwmPin<'a, esp_hal::peripherals::MCPWM0<'a>, 0, true>,
}

/// The PWM pin type this board's heater sits on.
pub type HeaterPin<'a> = esp_hal::peripherals::GPIO4<'a>;

impl core::fmt::Debug for HeaterPwm<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HeaterPwm")
            .field("stage", &self.stage)
            .finish_non_exhaustive()
    }
}

impl<'a> HeaterPwm<'a> {
    /// Brings the heater's PWM up on the board's heater pin.
    ///
    /// The period is the C++ firmware's 10 ms window, which is 100 Hz, and the maximum timestamp
    /// is the duty window minus one, so a duty of 1000 is the full period. A peripheral clock of
    /// 1 MHz with no prescaler is enough to divide down to 100 Hz exactly, which is why the clock
    /// is set explicitly rather than left at a default this port does not control.
    pub fn new(mcpwm: esp_hal::peripherals::MCPWM0<'a>, pin: HeaterPin<'a>) -> Self {
        use clevercoffee_app::heater;
        // 1 MHz divided by (period + 1) gives the 100 Hz the 10 ms window needs.
        let clock =
            esp_hal::mcpwm::PeripheralClockConfig::with_frequency(esp_hal::time::Rate::from_mhz(1))
                .unwrap_or_else(|_| {
                    // A peripheral clock that cannot be set is not a reason to refuse to boot: the
                    // timer below sets the period from whatever the clock is, and the fail-safe is
                    // that a wrong period is a wrong heater, so the stage records it instead.
                    esp_hal::mcpwm::PeripheralClockConfig::with_prescaler(0)
                });
        let mut pwm = esp_hal::mcpwm::McPwm::new(mcpwm, clock);
        pwm.operator0.set_timer(&pwm.timer0);
        let mut pwm_pin = pwm
            .operator0
            .with_pin_a(pin, esp_hal::mcpwm::operator::PwmPinConfig::UP_ACTIVE_HIGH);
        // `timer_clock_with_frequency` divides the peripheral clock down to the requested
        // frequency, so the caller states the period it wants and the HAL works out the
        // prescaler. A frequency the clock cannot reach is reported rather than rounded, because
        // a 10 ms window that is really 9.8 ms is a heater whose PID is slightly wrong.
        let timer = match clock.timer_clock_with_frequency(
            heater::WINDOW - 1,
            esp_hal::mcpwm::timer::PwmWorkingMode::Increase,
            esp_hal::time::Rate::from_hz(heater::FREQUENCY_HZ),
        ) {
            Ok(cfg) => cfg,
            Err(_) => {
                // Unreachable at 100 Hz from a 1 MHz clock, and handled anyway: the stage below
                // records that the heater is not under power control, which is the safe answer.
                return Self {
                    stage: heater::Stage::HeldOff,
                    pin: pwm_pin,
                };
            }
        };
        pwm.timer0.start(timer);
        // Zero duty until the PID says otherwise: a machine that has just booted does not heat.
        pwm_pin.set_timestamp(0);
        Self {
            stage: heater::Stage::Pwm,
            pin: pwm_pin,
        }
    }

    /// The stage, for a log line and for the status endpoint.
    pub const fn stage(&self) -> clevercoffee_app::heater::Stage {
        self.stage
    }

    /// Applies a duty. Ignored unless the stage permits heating, which is the whole point of the
    /// stage: a driver whose peripheral did not come up reports [`heater::Stage::HeldOff`] and
    /// this writes nothing.
    pub fn set_duty(&mut self, energised: bool, permille: u16) {
        let command = clevercoffee_app::heater::Command::for_duty(self.stage, energised, permille);
        if !command.energised {
            return;
        }
        self.pin.set_timestamp(command.timestamp);
    }
}
