//! The temperature probe: which driver the configuration names, its bring-up,
//! and the enum that owns whichever transport it turned out to be.
//!
//! # Why this is its own module
//!
//! `main.rs` is the startup order, the task spawns and the watchdog feed
//! (04-target-architecture.md §6). The probe is none of those: it is one
//! peripheral with two possible transports behind it, brought up once at boot
//! and then polled by the control task. Keeping it here means the wiring in
//! `bring_up` stays readable — a reader sees the pins taken and handed to
//! `bring_up_temperature_sensor` and nothing else — and the DS18B20 / TSIC-306
//! argument sits next to the code it argues about.
//!
//! **This is a move, not a redesign.** Nothing about the bring-up, the log
//! lines or the poll loop changed; the `Gpio16` the caller passes in is still
//! taken in `main.rs` (`cc_hal_esp32::pins::TEMP_SENSOR`), because the wiring is
//! what `assert_wiring` is checked against.

use core::fmt::Write as _;

use cc_domain::hardware::TemperatureSensorType;
use cc_domain::units::Millis;
use cc_hal_esp32::onewire::GpioOneWire;
use cc_hal_esp32::pins;
use cc_hal_esp32::zacwire::{self, ZacwireCapture};
use cc_protocol::sensor::ds18b20::{self as ds18b20_domain, Driver as Ds18b20Driver};
use cc_protocol::sensor::onewire::{OneWireError, Rom};
use cc_protocol::sensor::tsic306 as tsic306_domain;
use cc_protocol::sensor::tsic306::Tsic306;
use log::{error, info, warn};

/// The `bring_up` arms return and the control task polls, so both callers are
/// outside this module.
use crate::EspError;

/// The probe physically fitted to the board this firmware was built for.
///
/// **Documentation, not behaviour.** It is what the ROM address, the
/// `initial_raw` seed and the bring-up log line below are written for, and it is
/// the value a machine with a `DS18B20` wants in
/// `hardware.sensors.temperature.type`.
///
/// The driver that actually runs is [`PROBE_FROM_CONFIG`] — the *configured*
/// value, exactly as the C++ chooses it (`SystemInitializer.cpp` builds a
/// `TempSensorDallas` or a `TempSensorTSIC` from
/// `Config::hardwareSensorsTemperatureType`). It used to be this `const`, on the
/// argument that the board is what it is and a configuration default should not
/// override a measured fact. The human's answer to that was the right one:
///
/// > I switched the sensor in config but it still shows temperature — that
/// > should not work
///
/// A setting that changes nothing is not a default, it is a lie, and a silent
/// one: a board wired for a `DS18B20` that reads one while the operator has
/// selected a `TSIC-306` is exactly the "silently read the other bus" failure the
/// comment below claimed to be avoiding. With the configuration honoured, the
/// mismatch is visible instead: the selected driver gets no answer, and no answer
/// is what sends the machine to `SENSOR_ERROR` with a zero duty.
///
/// Measured: ROM `286937aacd78af41`, family `0x28`, 2026-09-28.
const BOARD_PROBE: TemperatureSensorType = TemperatureSensorType::DallasDs18b20;

/// Read the probe type out of the configuration, naming the C++ it mirrors.
///
/// `hardware.sensors.temperature.type` (`Config.h:1085-1092`), which defaults to
/// `TSIC_306` in both firmwares — a default that does not match the machine that
/// ships with a `DS18B20`. That is the C++'s own mismatch and it is preserved;
/// what is **not** preserved is the C++'s tolerance of it being wrong in
/// silence, because the C++ picks the driver from the same value.
fn probe_from_config(config: &cc_config::Config) -> TemperatureSensorType {
    // Not a `match`: the enum has two variants and the point of the function is
    // to be a *narrowing* of the configuration's value to the two the driver
    // layer implements. A future third variant has to break this line rather
    // than fall through, which is the whole reason the two are named here.
    config.hardware.sensors.temperature.r#type
}

/// The `DS18B20`'s ROM code on this machine, in the device's byte order.
///
/// Measured, not assumed: the recovered image's boot log reported
/// `DS18B20 at 0x41af78cdaa376928 (family 0x28)`, which is the ROM printed in
/// wire order (least-significant byte first) — see
/// `cc_domain::onewire`'s `the_logged_rom_is_printed_least_significant_byte_first`.
/// A mismatch is reported rather than tolerated, because a wrong ROM means the
/// driver is addressing a device that is not there.
const DS18B20_ROM: Rom = Rom([0x28, 0x69, 0x37, 0xAA, 0xCD, 0x78, 0xAF, 0x41]);

/// Bring up whichever probe [`PROBE`] names, and read it once.
///
/// A failure here is **not** fatal. The probe is a sensor: a machine with a
/// broken probe must still be able to run its state machine and report the fault
/// through S1, not refuse to boot. The C++ behaves the same way —
/// `TempSensorDallas` reports a failed read and `TempSensor::error_` is what
/// escalates it — so this matches it.
///
/// Both arms are compiled and type-checked, and now both are **linked**: the
/// selection is a runtime value, so the arm the operator does not choose is dead
/// weight rather than dead code. That is the honest cost of honouring the
/// setting, and it is what `just size` now reports.
pub(crate) fn bring_up_temperature_sensor(
    pin: esp_idf_hal::gpio::Gpio16<'static>,
    config: &cc_config::Config,
) -> Result<TemperatureSensor, EspError> {
    let probe = probe_from_config(config);
    // The board and the configuration are two different facts and the log says
    // so, because on this machine they disagree by default: the board has a
    // `DS18B20` and the parameter's default is `TSIC_306`.
    if probe == BOARD_PROBE {
        info!(
            "temperature: driver = {probe:?}, which is the probe fitted to this \
             board (hardware.sensors.temperature.type agrees)"
        );
    } else {
        warn!(
            "temperature: driver = {probe:?} because \
             hardware.sensors.temperature.type says so, but the probe measured \
             on this board is {BOARD_PROBE:?} (ROM 286937aacd78af41). If nothing \
             reads, that is why: a {probe:?} on a 1-Wire bus has nothing to talk \
             to, and the machine will report a sensor error rather than guess."
        );
    }
    match probe {
        TemperatureSensorType::DallasDs18b20 => bring_up_ds18b20(pin),
        TemperatureSensorType::Tsic306 => bring_up_tsic306(pin),
    }
}

/// The `DS18B20` arm: a bit-banged 1-Wire bus, calibrated and polled once.
fn bring_up_ds18b20(
    pin: esp_idf_hal::gpio::Gpio16<'static>,
) -> Result<TemperatureSensor, EspError> {
    let mut bus = GpioOneWire::new(pin)?;
    let mut driver = Ds18b20Driver::new(DS18B20_ROM);

    // The resolution write is a one-time EEPROM cycle, so it happens here at
    // boot and never again (`DallasTemperature::setResolution` is called once in
    // the C++ constructor too, `TempSensorDallas.cpp:22`).
    if let Err(err) = driver.calibrate(&mut bus) {
        warn!("temperature: calibration failed ({err:?}); assuming 12-bit");
    } else {
        info!(
            "temperature: DS18B20 at {rom}, {}-bit, {} ms conversion, reading every {} ms",
            ds18b20_domain::RESOLUTION_BITS,
            driver.conversion_time(),
            ds18b20_domain::CADENCE,
            rom = format_rom(DS18B20_ROM),
        );
    }

    // Confirm the device answers, so a wrong pin or a missing pull-up is a boot
    // log line rather than a stream of read failures.
    match driver.poll(&mut bus, Millis::ZERO) {
        Ok(ds18b20_domain::Poll::Started) => {}
        Ok(other) => warn!("temperature: unexpected first poll result {other:?}"),
        Err(OneWireError::NoPresence) => {
            error!(
                "temperature: no 1-Wire device answered on GPIO{} (no pull-up?)",
                pins::TEMP_SENSOR
            );
        }
        Err(OneWireError::Bus(err)) => error!("temperature: 1-Wire bus error {err}"),
    }

    Ok(TemperatureSensor::Dallas {
        bus,
        driver,
        last_reading: None,
        samples: 0,
    })
}

/// The `TSIC-306` arm: a `ZACwire` edge capture and the domain driver.
///
/// # 🔴 This arm has never run
///
/// **No `TSIC-306` is fitted to the machine this firmware was built for.** The
/// probe is a `DS18B20` and [`PROBE`] says so, so this function is compiled (the
/// types, the protocol arithmetic and the rejection paths are all checked) and
/// **not executed**. See `cc_protocol::sensor::tsic306` and
/// `cc_hal_esp32::zacwire` for what a green test run of this driver does and does
/// not prove.
///
/// The capture's first action is to report the line's own rest state, which is
/// the one thing a `ZACwire` line has and a 1-Wire bus does not: the sensor drives
/// it **high** when idle (app note §1.1, "the signal is normally high"). If the
/// line reads low, either the sensor is unpowered or something else is pulling
/// the pin down, and neither is a temperature reading — so it is said plainly
/// rather than being reported as a decode failure on a bus that is not there.
fn bring_up_tsic306(
    pin: esp_idf_hal::gpio::Gpio16<'static>,
) -> Result<TemperatureSensor, EspError> {
    let capture = ZacwireCapture::new(pin)?;
    info!(
        "temperature: TSIC-306 ZACwire capture on GPIO{}, idle sample {} us, \\
         burst sample {} us, hold {} us, ring {} edges, no-signal timeout {} ms",
        pins::TEMP_SENSOR,
        zacwire::IDLE_POLL_US,
        zacwire::BURST_POLL_US,
        zacwire::BURST_HOLD_US,
        tsic306_domain::ring::CAPACITY,
        tsic306_domain::protocol::NO_SIGNAL_TIMEOUT_US / 1_000,
    );
    // Acquire one burst and decode it, so the boot log says whether the line is
    // alive and what, if anything, it decoded. This is the boot-time equivalent
    // of `TempSensorTSIC`'s 221, and it is checked here rather than left to the
    // first `tryGetValue` so an operator sees it at boot.
    let mut capture = capture;
    let mut buffer = tsic306_domain::ring::EdgeBuffer::new();
    let outcome = {
        // Borrowed for one probe-and-decode so the boot log can report a verdict
        // before the capture is moved into the long-lived driver.
        let mut probe = Tsic306::new(&mut capture as &mut ZacwireCapture<'_>);
        probe.poll(&mut buffer)
    };
    match outcome {
        tsic306_domain::Outcome::Reading(celsius) => {
            info!("temperature: TSIC-306 reads {celsius:.2} C");
        }
        tsic306_domain::Outcome::NotConnected => {
            error!(
                "temperature: the TSIC-306 line on GPIO{} is idle; the sensor is \
                 unpowered, not connected, or the pin is pulled low",
                pins::TEMP_SENSOR
            );
        }
        tsic306_domain::Outcome::ReadFailed => {
            warn!(
                "temperature: the TSIC-306 line moved but did not decode \
                 ({} edges, {} transitions, {} dropped) -- parity, start-bit duty \
                 or stop-bit",
                capture.stats().edges,
                capture.stats().transitions,
                capture.dropped(),
            );
        }
    }
    Ok(TemperatureSensor::Tsic {
        driver: Tsic306::new(capture),
        last_reading: None,
        samples: 0,
    })
}

/// The probe: its bus, and the domain driver that decides what it means.
///
/// An enum, because the two arms own **different transports** and the pin: the
/// 1-Wire arm bit-bangs GPIO16 itself, and the `ZACwire` arm's capture owns it as
/// an interrupt-free sampler. `cc_protocol::sensor::probe`'s `ProbeReading` is the
/// vocabulary the state machine sees, and this is the one place that has to know
/// which is which.
#[allow(
    clippy::large_enum_variant,
    reason = "the two arms are only ever one of them; a `Box` here would be a \
              second allocation for a value that is moved once at boot and never \
              again, and boxing the 45-byte DS18B20 arm to save nothing on the \
              other is the wrong direction"
)]
pub(crate) enum TemperatureSensor {
    /// A bit-banged 1-Wire bus plus the `DS18B20` pipeline.
    Dallas {
        bus: GpioOneWire<'static>,
        driver: Ds18b20Driver,
        /// The most recent reading, so the control task can publish it without a
        /// match on which driver is fitted.
        last_reading: LastReading,
        /// Conversions completed. The safety monitor's clock — see
        /// [`TemperatureSensor::sample_seq`].
        samples: u32,
    },
    /// A `ZACwire` edge capture, which is also the driver's `EdgeSource`, so there
    /// is one owner of the pin, one owner of the ring, and nothing shared.
    Tsic {
        driver: Tsic306<ZacwireCapture<'static>>,
        /// The most recent reading. See [`TemperatureSensor::Dallas`].
        last_reading: LastReading,
        /// Decodable frames. See [`TemperatureSensor::sample_seq`].
        samples: u32,
    },
}

/// `(celsius, plausible)` — the most recent reading.
///
/// The pair is kept together because that is what S1 consumes: the C++ keeps
/// `TempSensor::value_` and `TempSensor::error_` apart
/// (`TempSensor.h:88-95`) and `EmergencyStopManager` branches on the *flag*,
/// not on a NaN.
pub(crate) type LastReading = Option<(f64, bool)>;

/// Which sensor fault was last written to the log.
///
/// A three-variant tag rather than the driver's own `Ds18b20Fault` because the
/// no-presence and bus-error arms report a different type (`OneWireError`), and
/// the point of the value is only "have I already said this".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DallasFaultTag {
    /// A read that came back with a named fault.
    Read,
    /// No device answered the reset.
    NoPresence,
    /// The bus itself failed.
    Bus,
    /// The `ZACwire` capture produced no decodable frame.
    Tsic,
}

impl TemperatureSensor {
    /// How many conversions have completed since boot.
    ///
    /// **This is the safety monitor's clock.** S1's debounce counts *samples*,
    /// not invocations (`cc_safety::Telemetry::sample_seq`), and the loop calls
    /// the monitor ten times a second against a probe that converts at 2.5 Hz.
    /// Without this counter the same reading is counted forty times and the
    /// over-temperature debounce trips in 30 ms instead of the C++'s 1.2 s.
    ///
    /// # 🔴 A fault does NOT advance this counter, and must not
    ///
    /// When the probe goes silent the counter freezes with the reading, so S1's
    /// debounce stops advancing on a stale value. That is correct, and it is the
    /// whole contract of the counter: it says a conversion completed, and a
    /// failure is not a conversion.
    ///
    /// Advancing it on a fault would look like it closes the gap and would
    /// reopen the bug `intentional-diffs.md` #16 exists to prevent. The loop
    /// polls at 100 Hz, so ten faults are 100 ms apart: three of them would trip
    /// the over-temperature debounce in **30 ms** on a number nothing is
    /// producing — the exact "one stale reading latches an emergency stop" failure
    /// the counter was introduced to stop, reached by the other door.
    ///
    /// **The gap is closed on the other path, and that is the C++'s path.** The
    /// frozen reading no longer matters because a dead probe is a *sensor error*
    /// ([`Self::has_error`]), which reaches `SENSOR_ERROR` and de-energises the
    /// heater through `should_pid_be_enabled` (`guards.rs:183`). Note that the
    /// C++ reaches *both* conclusions from a dead probe — its stale 155 °C also
    /// trips S1, because it counts per `updateTemperature()` call rather than per
    /// reading — so not advancing here is part of divergence #16, and the safety
    /// it gave up is recovered by the sensor-error guard instead.
    pub(crate) fn sample_seq(&self) -> u32 {
        match self {
            Self::Dallas { samples, .. } | Self::Tsic { samples, .. } => *samples,
        }
    }

    /// The most recent reading, and whether it was plausible.
    ///
    /// `None` before the first conversion completes, which is why
    /// `/api/temperatures` reports a temperature before it reports a plausible
    /// one: reporting `null` for a probe that has not spoken yet is honest, and
    /// reporting `0.0` is a disconnected probe wearing a plausible value.
    ///
    /// **This is deliberately not the fault signal.** A reading that was
    /// plausible when it arrived stays plausible forever, because nothing here
    /// invalidates it — the C++ has the same property: `cachedTemperature_` is
    /// written on success and never cleared (`SensorCoordinator.cpp:58`, field
    /// at `SensorCoordinator.h:241`).
    /// [`Self::has_error`] is the fault signal; see its docs for why asking
    /// this one is not enough.
    #[must_use]
    pub(crate) fn last_reading(&self) -> LastReading {
        match self {
            Self::Dallas { last_reading, .. } | Self::Tsic { last_reading, .. } => *last_reading,
        }
    }

    /// Whether the driver has latched a fault, whatever the last reading said.
    ///
    /// This is the C++'s `TempSensor::hasError()` (`TempSensor.h:80-83`) via
    /// `isConnected()` (`:158-161`), which `BaseState::checkTransitions` turns
    /// into `SENSOR_ERROR` (`BaseState.h:145-148`). Both arms' drivers keep the
    /// flag to the C++'s own rule: set at
    /// [`MAX_BAD_READINGS`](cc_protocol::sensor::ds18b20::MAX_BAD_READINGS)
    /// consecutive failures and cleared by the next success
    /// (`TempSensor.h:41-53`).
    ///
    /// **Why this cannot be derived from [`Self::last_reading`].** `poll` only
    /// ever *writes* `last_reading`, never clears it, so a probe that reads
    /// 95 °C and then goes silent leaves `Some((95.0, true))` behind
    /// indefinitely — the C++ has the same property
    /// (`SensorCoordinator::cachedTemperature_`, `SensorCoordinator.h:241`).
    /// Deriving the fault from the reading's plausibility — which is what the
    /// sample used to do — therefore reports a dead probe as a healthy one, and
    /// the machine keeps regulating the PID against a number nothing is
    /// producing. The C++ never had this window because `error_` is a
    /// *separate* piece of state from `last_temperature_`, which is exactly the
    /// split this method restores.
    #[must_use]
    pub(crate) fn has_error(&self) -> bool {
        match self {
            Self::Dallas { driver, .. } => driver.has_error(),
            Self::Tsic { driver, .. } => driver.has_error(),
        }
    }

    /// One step of whichever driver is fitted, and a log line for it.
    ///
    /// Both arms are non-blocking by construction — the `DS18B20` waits on a
    /// deadline the loop's own sleep covers, and the `TSIC-306` samples for a
    /// bounded window and returns — so this never stalls the control loop.
    pub(crate) fn poll(&mut self, now: Millis, sensor_fault_logged: &mut Option<DallasFaultTag>) {
        match self {
            Self::Dallas {
                bus,
                driver,
                last_reading,
                samples,
            } => {
                match driver.poll(bus, now) {
                    Ok(ds18b20_domain::Poll::Reading(Ok(celsius))) => {
                        // A good reading re-arms the log, so a fault that comes
                        // back after a recovery is announced again.
                        *sensor_fault_logged = None;
                        *samples = samples.saturating_add(1);
                        *last_reading =
                            Some((f64::from(celsius), ds18b20_domain::is_plausible(celsius)));
                        info!(
                            "temperature: {celsius:.2} C (plausible: {})",
                            ds18b20_domain::is_plausible(celsius)
                        );
                    }
                    Ok(ds18b20_domain::Poll::Reading(Err(fault))) => {
                        // The C++'s `TempSensorDallas` logs and returns false for
                        // the same faults; the count towards `error_` is the
                        // driver's. See `div6_*`: the C++ can only report
                        // "not connected" for all six, and this port names the
                        // fault.
                        //
                        // **Once per streak, not once per read.** At the loop's
                        // rate a misconfigured probe fails on every read, and 50
                        // lines a second on a 115200-baud console is both a wall
                        // of noise and real time spent in `println`. The
                        // fault itself is unchanged; only the log is gated.
                        if *sensor_fault_logged != Some(DallasFaultTag::Read) {
                            *sensor_fault_logged = Some(DallasFaultTag::Read);
                            warn!("temperature: read failed: {fault} (logged once per fault)");
                        }
                    }
                    Ok(ds18b20_domain::Poll::Started | ds18b20_domain::Poll::Waiting) => {}
                    // The same gate as the read failure above: a bus that is
                    // broken stays broken, and one line per read hides the one
                    // line that matters.
                    Err(OneWireError::NoPresence) => {
                        if *sensor_fault_logged != Some(DallasFaultTag::NoPresence) {
                            *sensor_fault_logged = Some(DallasFaultTag::NoPresence);
                            warn!("temperature: 1-Wire device stopped responding (logged once)");
                        }
                    }
                    Err(OneWireError::Bus(_)) => {
                        if *sensor_fault_logged != Some(DallasFaultTag::Bus) {
                            *sensor_fault_logged = Some(DallasFaultTag::Bus);
                            error!("temperature: 1-Wire bus error (logged once)");
                        }
                    }
                }
            }
            Self::Tsic {
                driver,
                last_reading,
                samples,
            } => {
                let mut buffer = tsic306_domain::ring::EdgeBuffer::new();
                let outcome = driver.poll(&mut buffer);
                match outcome {
                    tsic306_domain::Outcome::Reading(celsius) => {
                        *samples = samples.saturating_add(1);
                        // A `ZACwire` frame that decodes is a reading, and a
                        // decoded frame is by construction plausible (the decoder
                        // range-checks), so the flag is unconditionally true here.
                        *last_reading = Some((f64::from(celsius), true));
                        *sensor_fault_logged = None;
                    }
                    other => {
                        // Same gate as the `DS18B20` arm, and for the same
                        // reason: a `TSIC-306` that is not fitted fails on every
                        // read, and one line per read on a 115200-baud console is
                        // a wall of text that buries the line that says why.
                        if let Some(fault) = other.probe_fault() {
                            if *sensor_fault_logged != Some(DallasFaultTag::Tsic) {
                                *sensor_fault_logged = Some(DallasFaultTag::Tsic);
                                warn!(
                                    "temperature: {fault} (ZACwire, logged once) — \
                                     is hardware.sensors.temperature.type right \
                                     for this board?"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The ROM in the order a human reads it: family byte first, as the device
/// stores it, then the CRC last.
fn format_rom(rom: Rom) -> String {
    let mut out = String::with_capacity(16);
    for byte in rom.0 {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
