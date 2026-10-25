//! GPIO transport for [`cc_protocol::sensor::onewire::OneWireBus`].
//!
//! Use open drain: drive low or release, never high against another device.
//! The DS18B20 module supplies a 4.7 kΩ pull-up to 3V3; keep [`Pull::Floating`].
//! The internal ~45 kΩ pull is too weak for reliable rise times on long cables.
//! Protocol decisions and datasheet timing live in `cc-protocol`.

use cc_protocol::sensor::onewire::{self, OneWireBus};
use esp_idf_hal::delay::Ets;
use esp_idf_hal::gpio::{InputOutput, Level, OutputPin, PinDriver, Pull};
use esp_idf_hal::interrupt;
use esp_idf_svc::sys::EspError;

/// A bit-banged 1-Wire bus on one GPIO.
///
/// Open drain, so the firmware pulls the bus low or releases it and never
/// drives it high. The reset, the presence pulse and every bit are done with
/// interrupts disabled.
///
/// # Timing, and the 1 µs granularity
///
/// The plan's claim is that `esp_idf_hal`'s µs-granular delays are adequate for
/// 1-Wire's 3–65 µs slots, and it is — but the reason is worth stating
/// precisely, because the naive form of the claim is wrong.
///
/// `Ets::delay_ns(ns)` is `Ets::delay_us((ns + 999) / 1000)`
/// (`esp-idf-hal-0.47.0/src/delay.rs:250-252`), so it **rounds up to the next
/// whole microsecond**: `delay_ns(1500)` becomes `delay_us(2)`, a 33 % overshoot
/// on a 1.5 µs request. This port therefore never calls `delay_ns`. It calls
/// [`Ets::delay_us`] with the datasheet's integers, which need no conversion
/// and so incur no rounding at all.
///
/// What is left is `ets_delay_us`'s own behaviour: it is a calibrated busy-wait
/// that takes **at least** the requested time, and on the original ESP32 it is
/// accurate to roughly a microsecond including loop overhead. The 1-Wire
/// windows absorb that:
///
/// | quantity | datasheet | requested | headroom |
/// | --- | --- | --- | --- |
/// | write-1 `t_LOW` | 1–15 µs | 10 µs | 5 µs either side |
/// | write-0 `t_LOW` | 60–120 µs | 65 µs | 5 µs either side |
/// | read `t_LOW` | 1–15 µs | 3 µs | 12 µs |
/// | `t_SAMPLE` after the edge | 5–15 µs | 13 µs | **2 µs** |
/// | `t_RST` | ≥ 480 µs | 480 µs | 0 µs, at the limit |
///
/// The read sample point is the tight one: 3 + 10 = 13 µs against a 15 µs
/// ceiling. Both figures are the datasheet's, so a marginal setup is handled the
/// same way by any device on the bus.
/// `cc_protocol::sensor::onewire`'s
/// `read_the_sample_point_has_two_microseconds_of_headroom` pins the margin so
/// a future edit to either constant has to confront it.
pub struct GpioOneWire<'d> {
    pin: PinDriver<'d, InputOutput>,
}

impl<'d> GpioOneWire<'d> {
    /// Take a GPIO as a 1-Wire bus.
    ///
    /// The pin must be a real 1-Wire line with an external pull-up. It is
    /// configured open-drain with no internal pull — see the module docs.
    ///
    /// # Errors
    ///
    /// Whatever the peripheral reports if the pin cannot be configured as an
    /// open-drain input/output.
    pub fn new(pin: impl esp_idf_hal::gpio::InputPin + OutputPin + 'd) -> Result<Self, EspError> {
        // `PinDriver` is `Send` (`esp-idf-hal-0.47.0/src/gpio.rs:1170`), which
        // is what lets the bus move into the control task. The bus is *not*
        // `Sync`, correctly: two threads bit-banging one wire would interleave
        // their slots and produce garbage that fails the CRC.
        let pin = PinDriver::input_output_od(pin, Pull::Floating)?;
        Ok(Self { pin })
    }

    /// Drive a level with interrupts disabled, propagating the peripheral's
    /// own result.
    ///
    /// `interrupt::free` takes a closure and so cannot return a `Result` out of
    /// the critical section; the error is therefore captured in an `Option` and
    /// unwrapped here, outside it. Dropping it instead would be a swallowed
    /// peripheral failure, and this crate is the layer where a bus driver that
    /// silently stops driving is the bug you least want.
    fn drive_critical(&mut self, level: Level) -> Result<(), EspError> {
        let mut failure = None;
        interrupt::free(|| {
            if let Err(err) = self.pin.set_level(level) {
                failure = Some(err);
            }
        });
        match failure {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Pull the bus low and hold it for `micros`.
    ///
    /// Interrupts are off across the low pulse only, and the wait is *outside*
    /// the critical section: the pulse has to be atomic, the recovery time does
    /// not.
    ///
    /// # Errors
    ///
    /// Whatever the peripheral reports if the level write fails.
    fn pulse_low(&mut self, micros: u32) -> Result<(), EspError> {
        self.drive_critical(Level::Low)?;
        Ets::delay_us(micros);
        Ok(())
    }

    /// Release the bus, wait `micros`, then read the line.
    ///
    /// On an open-drain pin, writing `High` **releases** the line rather than
    /// driving it, which is what lets the sensor pull its presence pulse or its
    /// data bit.
    ///
    /// # Errors
    ///
    /// Whatever the peripheral reports if the level write fails.
    fn release_and_read(&mut self, micros: u32) -> Result<bool, EspError> {
        self.drive_critical(Level::High)?;
        Ets::delay_us(micros);
        Ok(self.pin.is_high())
    }
}

impl OneWireBus for GpioOneWire<'_> {
    type Error = EspError;

    /// One reset cycle on the bus: the reset pulse and the presence pulse.
    ///
    /// Pull low for 480 µs, release, wait 70 µs, sample for the presence pulse,
    /// then wait out the rest of the reset window.
    ///
    /// It first spins until the line reads high, in case a previous transaction
    /// left it low. That spin is the difference between a bus recovering from a
    /// truncated transaction and one needing a power cycle, and it costs at most
    /// 250 µs on a healthy bus.
    fn reset(&mut self) -> Result<bool, EspError> {
        const RETRIES: u32 = 125;
        for _ in 0..RETRIES {
            if self.pin.is_high() {
                break;
            }
            Ets::delay_us(2);
        }

        self.pulse_low(onewire::timing::RESET_LOW_US)?;
        let present = self.release_and_read(onewire::timing::RESET_PRESENCE_US)?;
        Ets::delay_us(onewire::timing::RESET_RECOVERY_US);
        // The device pulls the line low to say "here I am", so a low reading is
        // a presence pulse.
        Ok(!present)
    }

    fn write_bit(&mut self, bit: bool) -> Result<(), EspError> {
        let (low, high) = if bit {
            (
                onewire::timing::WRITE_ONE_LOW_US,
                onewire::timing::WRITE_ONE_HIGH_US,
            )
        } else {
            (
                onewire::timing::WRITE_ZERO_LOW_US,
                onewire::timing::WRITE_ZERO_HIGH_US,
            )
        };
        self.pulse_low(low)?;
        self.drive_critical(Level::High)?;
        Ets::delay_us(high);
        Ok(())
    }

    fn read_bit(&mut self) -> Result<bool, EspError> {
        // Pull low briefly to ask the device to speak, release, then sample
        // `t_SAMPLE` after the falling edge.
        self.pulse_low(onewire::timing::READ_LOW_US)?;
        let high = self.release_and_read(onewire::timing::READ_SAMPLE_US)?;
        Ets::delay_us(onewire::timing::READ_RECOVERY_US);
        Ok(high)
    }
}
