//! The traits every hardware implementation satisfies, and the recording fakes that stand in for
//! them on the host.
//!
//! This crate contains no hardware and no `unsafe`. It exists so that the rules in
//! [architecture.md](../../docs/rust-migration/architecture.md) can be enforced by the type
//! system rather than by convention:
//!
//! - Only one object owns an actuator. There is no second path to a relay, which is the Rust
//!   form of the C++ rule that relays must never be poked directly, and it is why defect D02
//!   (a shadow `heaterEnabled_` flag that disagreed with the hardware) cannot recur.
//! - The heater is a *command*, not a flag. [`Actuators::command`] is the only way to change
//!   what the machine is doing, and [`Actuators::force_off`] is always available to any caller,
//!   including the safety task and the panic handler.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use heapless::Vec;

/// What the machine is being told to do.
///
/// Deliberately not an enum of combinations: the three relays are independent, and the
/// pre-infusion pause needs a combination a one-variant-per-combination enum cannot hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ActuatorCommand {
    pub pump: bool,
    pub water_valve: bool,
    pub steam_valve: bool,
    /// Heater *permission*. The duty cycle itself is separate, because it is produced by the
    /// heater ISR on a hardware timer and must cross that boundary as an integer.
    pub heater_enabled: bool,
}

impl ActuatorCommand {
    pub const ALL_OFF: Self = Self {
        pump: false,
        water_valve: false,
        steam_valve: false,
        heater_enabled: false,
    };

    pub const fn is_all_off(self) -> bool {
        !self.pump && !self.water_valve && !self.steam_valve && !self.heater_enabled
    }

    /// Whether any water is moving. The interlock's single question.
    pub const fn flowing(self) -> bool {
        self.pump || self.water_valve || self.steam_valve
    }
}

/// A recorded actuator command. The host tests assert on these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ActuatorEvent {
    /// Milliseconds since the fake was created. A test asserting a sequence of commands does not
    /// need real time, but it does need to tell two identical commands apart.
    pub at_ms: u32,
    pub command: ActuatorCommand,
    /// What the caller said it was doing, for a failing test to read.
    pub reason: &'static str,
}

/// The sole owner of the actuator pins.
///
/// One object, no interior mutability that a second owner could reach, and every method takes
/// `&mut self`. Two tasks cannot both drive the relays, because they cannot both hold `&mut`.
pub trait Actuators {
    /// Applies a command. The implementation must be idempotent: applying the same command twice
    /// must not toggle anything.
    fn command(&mut self, command: ActuatorCommand, reason: &'static str);

    /// De-energises everything, whatever the last command was.
    ///
    /// Must be safe to call from any state, any number of times, and from a fault path. This is
    /// the C++ `HardwareManager` contract that D02 showed was inert, made impossible to get
    /// wrong: there is no shadow state that can say the heater is off when it is not.
    fn force_off(&mut self);

    /// Sets the heater duty cycle, 0 to `window`. Ignored when the heater is not enabled, so a
    /// stale duty cannot heat a machine whose state has turned the heater off.
    fn set_heater_duty(&mut self, duty_permille: u16);

    /// The command most recently applied, for tests and for the status endpoint.
    fn last_command(&self) -> ActuatorCommand;
}

/// A temperature reading, or why there isn't one.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TemperatureError {
    /// The bus is not answering, or no device answered on it.
    Disconnected,
    /// The device answered but the bytes did not check out.
    Corrupt,
    /// The value parsed but is not physically possible.
    OutOfRange,
    /// The conversion never completed.
    Timeout,
}

/// Reads a temperature in degrees Celsius.
pub trait TemperatureSensor {
    /// The device's 64-bit ROM address, for a log line and for a bus with several devices.
    fn address(&self) -> u64;

    /// Reads once. Must not block for longer than the conversion time, and must not return a
    /// value it is not confident in: the caller has no way to tell a guess from a measurement,
    /// which is what defect D03 was.
    fn read_celsius(&mut self) -> Result<f64, TemperatureError>;
}

/// A panel switch, sampled once per tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SwitchSample {
    /// True while the switch is held. Debounced, so this does not need its own debounce.
    pub pressed: bool,
    /// True once the press has lasted [`SwitchSample::LONG_PRESS`].
    pub long_press: bool,
}

impl SwitchSample {
    pub const fn released() -> Self {
        Self {
            pressed: false,
            long_press: false,
        }
    }
    pub const fn pressed() -> Self {
        Self {
            pressed: true,
            long_press: false,
        }
    }
}

/// Which physical switch a sample came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SwitchId {
    Power = 0,
    Brew = 1,
    Steam = 2,
    HotWater = 3,
}

/// Reads the debounced state of one switch.
///
/// A snapshot, not a query. The C++ `IOSwitch::isPressed()` mutated debounce state and was
/// called several times per loop, so later callers saw a stale reading (defect D37); a value
/// returned by value cannot have that problem.
pub trait Switch {
    fn id(&self) -> SwitchId;
    fn sample(&self) -> SwitchSample;
}

/// The 128x64 mono display.
pub trait Display {
    /// Pushes the framebuffer to the panel.
    fn flush(&mut self);

    /// The display's own clock, in milliseconds, for the auto-sleep timer.
    fn elapsed_ms(&self) -> u32;

    /// True while the panel is awake.
    fn is_awake(&self) -> bool;
}

/// A load-cell scale.
pub trait Scale {
    /// The weight in grams, or `None` if there is no reading yet or the reading is not credible.
    fn weight_g(&mut self) -> Option<f64>;

    /// Zeroes the scale.
    fn tare(&mut self) -> Result<(), ScaleError>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScaleError {
    /// The amplifier is not answering. The C++ `HX711Scale::init()` spun in an unbounded
    /// `while` loop, so a missing load cell hung the boot (defect D29).
    NoResponse,
    /// A second load cell is configured and is not answering.
    SecondCellMissing,
}

/// Persistent configuration storage.
pub trait Storage {
    /// Reads the whole config region into a fixed buffer, choosing the newer valid slot.
    fn read_config(&mut self, out: &mut [u8]) -> Result<usize, StorageError>;

    /// Writes the region, verifying the read-back before it reports success.
    fn write_config(&mut self, data: &[u8]) -> Result<(), StorageError>;

    /// Clears the region. The next boot reads compiled defaults.
    fn factory_reset(&mut self) -> Result<(), StorageError>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StorageError {
    /// Neither slot passed its CRC, and defaults were used.
    NoValidSlot,
    /// The write was verified and did not read back. The previous contents are intact.
    WriteVerifyFailed,
    /// Flash is not writable, which usually means it is write-protected.
    FlashReadOnly,
}

/// A channel the device can be provisioned over.
///
/// One trait, two implementations: UART0 on the original ESP32, which has no native USB, and
/// USB Serial/JTAG on the S3 and C6. The protocol above this trait is identical, so the choice of
/// transport is a build-time decision and not a code path.
pub trait ProvisioningTransport {
    /// Reads a line, without its terminator. `Ok(None)` means the port closed.
    fn read_line(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError>;

    /// Writes a line, appending the terminator the device expects.
    fn write_line(&mut self, line: &str) -> Result<(), TransportError>;

    /// Discards anything already buffered, so a provisioning session starts clean.
    fn drain(&mut self);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportError {
    /// The port is gone: unplugged, or a USB suspend.
    Disconnected,
    /// A write did not complete. Common on a full-speed CDC port with a large chunk.
    WriteFailed,
}

// ---------------------------------------------------------------------------
// Host fakes
// ---------------------------------------------------------------------------

/// How many events the recorder keeps. A brew cycle is about a dozen transitions, so 64 covers a
/// whole test with room, and it is a fixed size so the fake needs no allocator.
pub const RECORDER_CAPACITY: usize = 64;

/// An [`Actuators`] that records instead of driving pins.
///
/// This is the `mock-actuators` mode: on a device, building with it means the real GPIO outputs
/// are never constructed, so a bench run cannot energize anything. On the host it is what lets the
/// control tests assert the exact sequence of commands a scenario produces.
#[derive(Debug)]
pub struct RecordingActuators {
    now_ms: u32,
    last: ActuatorCommand,
    duty: u16,
    pub events: Vec<ActuatorEvent, RECORDER_CAPACITY>,
    /// Events dropped because the buffer was full. A test asserting on a truncated buffer would
    /// otherwise pass by accident, so the count is exposed and checked.
    pub dropped: u32,
}

impl Default for RecordingActuators {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingActuators {
    pub const fn new() -> Self {
        Self {
            now_ms: 0,
            last: ActuatorCommand::ALL_OFF,
            duty: 0,
            events: Vec::new(),
            dropped: 0,
        }
    }

    /// Advances the fake's clock, so events carry a plausible timestamp.
    pub fn advance(&mut self, ms: u32) {
        self.now_ms = self.now_ms.wrapping_add(ms);
    }

    pub fn duty(&self) -> u16 {
        self.duty
    }

    /// The recorded commands, oldest first. Empty if the buffer overflowed, in which case
    /// `dropped` is non-zero and a test must not use this.
    pub fn sequence(&self) -> &[ActuatorEvent] {
        &self.events
    }

    /// The commands only, which is what most assertions want.
    pub fn commands(&self) -> heapless::Vec<ActuatorCommand, RECORDER_CAPACITY> {
        let mut out = heapless::Vec::new();
        for e in &self.events {
            let _ = out.push(e.command);
        }
        out
    }

    /// Whether any recorded event had water moving, and any had the heater on.
    pub fn ever_flowed(&self) -> bool {
        self.events.iter().any(|e| e.command.flowing())
    }

    pub fn ever_heated(&self) -> bool {
        self.events.iter().any(|e| e.command.heater_enabled)
    }

    pub fn is_idle(&self) -> bool {
        self.last.is_all_off()
    }
}

impl Actuators for RecordingActuators {
    fn command(&mut self, command: ActuatorCommand, reason: &'static str) {
        // Idempotent: re-asserting the same command is a no-op, exactly as a real board must be,
        // because the control task re-asserts the current command every tick.
        if command == self.last {
            return;
        }
        self.last = command;
        if self
            .events
            .push(ActuatorEvent {
                at_ms: self.now_ms,
                command,
                reason,
            })
            .is_err()
        {
            self.dropped += 1;
        }
    }

    fn force_off(&mut self) {
        // Records every time, even when already off, because a caller reaching for force_off is
        // exactly the moment a test wants to see.
        self.last = ActuatorCommand::ALL_OFF;
        self.duty = 0;
        let _ = self.events.push(ActuatorEvent {
            at_ms: self.now_ms,
            command: ActuatorCommand::ALL_OFF,
            reason: "force_off",
        });
    }

    fn set_heater_duty(&mut self, duty_permille: u16) {
        // Clamped rather than trusted, because a caller computing a duty from a bad temperature
        // must not be able to command full power by passing 65535.
        self.duty = duty_permille.min(1000);
    }

    fn last_command(&self) -> ActuatorCommand {
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_idempotent() {
        let mut a = RecordingActuators::new();
        let on = ActuatorCommand {
            pump: true,
            ..ActuatorCommand::ALL_OFF
        };
        a.command(on, "brew");
        a.command(on, "brew");
        a.command(on, "brew");
        assert_eq!(
            a.sequence().len(),
            1,
            "re-asserting the same command must not record again"
        );
    }

    #[test]
    fn a_different_command_is_recorded() {
        let mut a = RecordingActuators::new();
        a.command(
            ActuatorCommand {
                pump: true,
                ..ActuatorCommand::ALL_OFF
            },
            "on",
        );
        a.advance(10);
        a.command(ActuatorCommand::ALL_OFF, "off");
        assert_eq!(a.sequence().len(), 2);
        assert_eq!(a.sequence()[0].reason, "on");
        assert_eq!(a.sequence()[1].at_ms, 10);
    }

    #[test]
    fn force_off_always_records_even_when_already_off() {
        let mut a = RecordingActuators::new();
        a.force_off();
        a.force_off();
        assert_eq!(a.sequence().len(), 2);
        assert!(a.is_idle());
    }

    #[test]
    fn force_off_clears_the_duty() {
        let mut a = RecordingActuators::new();
        a.command(
            ActuatorCommand {
                heater_enabled: true,
                ..ActuatorCommand::ALL_OFF
            },
            "heat",
        );
        a.set_heater_duty(1000);
        a.force_off();
        assert_eq!(
            a.duty(),
            0,
            "a force-off must not leave the heater commanded"
        );
    }

    #[test]
    fn the_duty_is_clamped_to_the_window() {
        let mut a = RecordingActuators::new();
        a.set_heater_duty(65535);
        assert_eq!(
            a.duty(),
            1000,
            "a caller must not be able to command more than full power"
        );
    }

    #[test]
    fn the_recorder_reports_its_own_overflow() {
        // A truncated buffer would make a sequence assertion pass by accident, so the drop count
        // is public and a test that uses `sequence` has to be able to see it.
        let mut a = RecordingActuators::new();
        for i in 0..RECORDER_CAPACITY + 5 {
            let on = if i % 2 == 0 {
                ActuatorCommand {
                    pump: true,
                    ..ActuatorCommand::ALL_OFF
                }
            } else {
                ActuatorCommand::ALL_OFF
            };
            a.command(on, "flood");
        }
        assert!(a.dropped > 0, "a full buffer must report the drops");
        assert_eq!(a.sequence().len(), RECORDER_CAPACITY);
    }

    #[test]
    fn flowing_and_all_off_agree_about_what_counts_as_water() {
        let valve_only = ActuatorCommand {
            water_valve: true,
            ..ActuatorCommand::ALL_OFF
        };
        assert!(valve_only.flowing());
        assert!(!valve_only.is_all_off());
        let heater_only = ActuatorCommand {
            heater_enabled: true,
            ..ActuatorCommand::ALL_OFF
        };
        assert!(!heater_only.flowing(), "the heater moves no water");
        assert!(!heater_only.is_all_off());
        assert!(ActuatorCommand::ALL_OFF.is_all_off());
        assert!(!ActuatorCommand::ALL_OFF.flowing());
    }
}
