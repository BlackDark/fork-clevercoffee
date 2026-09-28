//! Clever Coffee firmware — R1-01 workspace bring-up, plus the R1-07 heater
//! output.
//!
//! # What this binary is (and is not)
//!
//! This is the **R1-01 feasibility spike** extended by **R1-07**, not the
//! firmware. Its jobs are to prove that the toolchain builds, links, boots and
//! runs on this host for `xtensa-esp32-espidf`, to make the first image-size
//! measurement (07 §5), and — from R1-07 — to bring up the LEDC heater output
//! and hold it at duty 0.
//!
//! What it does, in order:
//!
//! 1. Initialise ESP-IDF and route `log` to UART0 at 115200 baud — the same
//!    stream and baud rate the C++ firmware uses.
//! 2. Configure the pump and water-valve pins — GPIO17 and GPIO27
//!    (`include/clevercoffee/hardware/pinmapping.h:39-40`) — as outputs and
//!    drive them **inactive**.
//! 3. **Attach GPIO2 to an LEDC channel at [`CARRIER_HZ`] / [`RESOLUTION`] with
//!    duty 0.** This replaces the C++'s 10 ms heater ISR (`isr.h:85-118`) with
//!    hardware PWM. The pin is de-energised from the moment the channel is
//!    configured and there is no code path here that raises the duty.
//! 4. **Read every actuator pin back and assert it is inactive.** This is the
//!    startup assertion of 04 §4 / R3-16 in its smallest possible form, done
//!    before anything else exists so a failure is unambiguous. For the heater the
//!    readback is the LEDC duty register, not the pin: a channel configured at
//!    duty 0 holds the pin at the idle level by construction.
//! 5. Move a `TWDTDriver` into a control task and feed it from there, so the
//!    watchdog subscriber is the control task and nothing else (04 §2, §3.4 —
//!    the same shape the recovered oracle used, 08 §3).
//! 6. Log a heartbeat every second — which is also the **supervisor heartbeat the
//!    heater's deadman consumes**, so the gate opens once the control task is
//!    provably alive and shuts again if it stops.
//!
//! # ⚠ Safety note about driving these three pins
//!
//! `inactive` is asserted as `Level::Low`, which is correct for the
//! `HIGH_TRIGGER` relays that are this board's default
//! (`hardware.relays.*.trigger_type`, default `HIGH_TRIGGER`; the oracle's boot
//! log says "active high", 08 §3). If this board were wired `LOW_TRIGGER`, `Low`
//! would **energise** the heater, the pump and the valve. The oracle refused
//! `LOW_TRIGGER` heater configurations outright for exactly that reason
//! (08 §4.1), and R2-06/R3-03 must reproduce that refusal. R0-01 has not yet
//! confirmed the relay board's trigger polarity in the field, so this is the one
//! thing to check before the boiler is ever energised.
//!
//! # ⚠ The heater output has never been energised
//!
//! Every call in this binary passes duty 0, and the gate
//! ([`cc_domain::heater::HeaterGate`]) refuses a non-zero duty until the
//! supervisor has beaten — which only the control task's heartbeat does, and it
//! passes 0 regardless. R1-07's hardware test (drive a **dummy load**, measure
//! with a scope) is **not** done: see the module docs of `cc-hal-esp32::heater`
//! and `docs/rust-migration/intentional-diffs.md` #5.
//!
//! There is deliberately **no** control loop, no state machine and no sensor
//! here. Those arrive at R2-08 and R3-xx.

use core::error::Error;

use cc_domain::ds18b20::{self as ds18b20_domain, Driver as Ds18b20Driver};
use cc_domain::onewire::{OneWireError, Rom};
use cc_domain::units::{Duty, Millis};
use cc_hal_esp32::heater::{HeaterOutput, LedcPwm, CARRIER_HZ, RESOLUTION};
use cc_hal_esp32::onewire::GpioOneWire;
use cc_hal_esp32::sensors::pins;
use core::fmt::Write as _;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{InputOutput, InputPin, Level, OutputPin, PinDriver, Pull};
use esp_idf_hal::ledc;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, TWDT};
use log::{error, info, warn};

type EspError = esp_idf_svc::sys::EspError;

/// The level that means "actuator de-energised" for a `HIGH_TRIGGER` relay.
const INACTIVE: Level = Level::Low;

/// Heartbeat period of the control task, and therefore the watchdog feed period.
const HEARTBEAT_MS: u32 = 1000;

/// How long the control task sleeps between iterations, in milliseconds.
///
/// 400 ms is the temperature sensor's cadence
/// (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS`, `constants/Timing.h:42`) and the
/// pressure sensor's 50 ms cadence divides into it exactly, so one tick is one
/// temperature sample and twenty pressure samples. The C++ runs its control loop
/// continuously and calls the coordinator on every iteration, which is what
/// makes the ABP2's `delay(10)` 20 % of the loop; here the loop's own sleep is
/// the only wait, and nothing inside it blocks.
const CONTROL_TICK_MS: u32 = 400;

/// Whether the LEDC heater output is brought up at boot.
///
/// **OFF, and it must stay off until R1-07 is fixed.** See the note on
/// [`HEATER_LEDC_DEFECT`].
const BRING_UP_HEATER_LEDC: bool = false;

/// 🔴 The heater cannot be driven by LEDC on this chip at 1 Hz.
///
/// Measured 2026-09-28 on the board, not inferred. The firmware panics at boot
/// with `Guru Meditation Error: Core 0 panic'ed (Interrupt wdt timeout on
/// CPU0)`, and the backtrace decodes to
/// `ledc_set_duty_and_update` -> `_ledc_update_duty` -> `ledc_ll_set_duty_start`
/// with `PC` inside that function.
///
/// The cause is in ESP-IDF's own HAL
/// (`components/hal/esp32/include/hal/ledc_ll.h:485-489`):
///
/// ```c
/// // wait until the last duty change took effect (duty_start bit will be
/// // self-cleared when duty update or fade is done)
/// // this is necessary on ESP32 only, otherwise, internal logic might mess up
/// while (hw->channel_group[speed_mode].channel[channel_num].conf1.duty_start);
/// ```
///
/// `duty_start` is cleared by the hardware at the next **timer period**, and the
/// spin is inside `portENTER_CRITICAL(&ledc_spinlock)` in
/// `ledc_set_duty_and_update` (`components/esp_driver_ledc/src/ledc.c:1603-1606`),
/// i.e. with interrupts masked. R1-07 chose a **1 Hz** carrier
/// ([`cc_hal_esp32::heater`] argues at length for exactly that, to minimise
/// contactor operations), so the spin is up to **one second** with interrupts
/// off. The original ESP32's interrupt watchdog is **300 ms**
/// (`components/esp_system/int_wdt.c`). Every duty write therefore trips it.
///
/// This is **not** a fault in the sensor work and **not** reachable only at
/// higher duties: `LedcPwm::new` writes duty 0, and `duty_start` self-clears at
/// the next period regardless of the duty value, so the very first write
/// panics.
///
/// Three things follow, and all three need a human decision:
///
/// 1. The "1 Hz or the contactor wears out" argument in [`cc_hal_esp32::heater`]
///    is correct as far as it goes and **incomplete**: it never checked what
///    ESP-IDF's own `ledc_ll_set_duty_start` does on this chip. Any carrier slow
///    enough to matter mechanically is also slow enough to trip a 300 ms
///    interrupt watchdog through that spin.
/// 2. The fix is not obvious. Raising the carrier to, say, 25 Hz would keep the
///    spin under 40 ms but reintroduces the contactor duty R1-07 set out to
///    avoid. Bypassing the spin needs a register write this HAL does not expose,
///    and therefore `unsafe` — which the workspace denies
///    (`[workspace.lints.rust] unsafe_code = "deny"`).
/// 3. Until it is fixed, GPIO2 is driven as a **plain inactive output**, exactly
///    like the pump and the valve. That is strictly safer than a PWM carrier
///    nobody has scoped, and it is what lets the rest of the bring-up — the
///    temperature probe, in particular — run at all.
#[allow(dead_code, reason = "read only while BRING_UP_HEATER_LEDC is false")]
const HEATER_LEDC_DEFECT: &str =
    "R1-07's 1 Hz LEDC carrier spins in ESP-IDF's ledc_ll_set_duty_start for up      to one period with interrupts masked, which exceeds the ESP32's 300 ms      interrupt watchdog. GPIO2 is held as a plain inactive output until R1-07      resolves it. See the firmware module docs.";

/// The DS18B20's ROM code on this machine, in the device's byte order.
///
/// Measured, not assumed: the recovered image's boot log reported
/// `DS18B20 at 0x41af78cdaa376928 (family 0x28)`, which is the ROM printed in
/// wire order (least-significant byte first) — see
/// `cc_domain::onewire`'s `the_logged_rom_is_printed_least_significant_byte_first`.
/// A mismatch is reported rather than tolerated, because a wrong ROM means the
/// driver is addressing a device that is not there.
const DS18B20_ROM: Rom = Rom([0x28, 0x69, 0x37, 0xAA, 0xCD, 0x78, 0xAF, 0x41]);

/// Stack size of the control task, from the priority table in 04 §2.
const CONTROL_STACK_BYTES: usize = 8 * 1024;

/// ESP-IDF version this binary was compiled against, as a coarse string. Logged
/// so a device is never diagnosed against the wrong IDF version: 08 records the
/// oracle was built with v5.5.5 and 05 §1 pins the same. `esp-idf-sys` emits
/// `esp_idf_version_at_least_X_Y_Z` cfgs, not a version string
/// (`esp-idf-sys-0.38.1/build/common.rs:239`), and the exact version is the
/// first line of the ESP-IDF boot banner anyway.
const IDF_VERSION: &str = if cfg!(esp_idf_version_at_least_6_0_0) {
    ">= 6.0.0 (UNEXPECTED: 05 pins v5.5.5)"
} else if cfg!(esp_idf_version_at_least_5_5_0) {
    "5.5.x"
} else if cfg!(esp_idf_version_at_least_5_1_0) {
    "5.1.x (UNEXPECTED: 05 pins v5.5.5)"
} else {
    "UNEXPECTED: older than 5.1"
};

/// Entry point. The `esp-idf-sys` `binstart` feature provides the C `main` that
/// calls this.
fn main() -> Result<(), Box<dyn Error>> {
    // Must run before `Peripherals::take()`: it applies the ESP-IDF linker
    // patches (`esp_idf_hal::sys::link_patches`).
    esp_idf_svc::sys::link_patches();

    // Routes the `log` crate into the ESP-IDF log system, i.e. UART0 at 115200.
    // The level comes from RUST_LOG and defaults to Info.
    esp_idf_svc::log::init_from_env();

    info!("Clever Coffee Rust firmware — R1-01 toolchain bring-up");
    info!("target: xtensa-esp32-espidf, ESP-IDF: {IDF_VERSION}");

    let peripherals = Peripherals::take()?;

    // The TWDT driver is *moved* into the control task so the subscription
    // belongs to the control task and to nothing else (04 §2: "Watchdog feed —
    // control task only").
    let twdt = peripherals.twdt;
    let ledc = peripherals.ledc;

    // The temperature probe. GPIO16 is `PIN_TEMPSENSOR` (`pinmapping.h:27`),
    // open-drain, and the driver bit-bangs it. This is a **read**: the pin is
    // never driven high (open drain), so nothing on this bus is energised.
    let mut temp_sensor = bring_up_temperature_sensor(peripherals.pins.gpio16)?;

    // 2. The two plain actuator pins to `inactive`, in `main`, before any task
    //    exists, so there is no window in which a task could observe them
    //    un-driven.
    let water_valve = drive_inactive(peripherals.pins.gpio17, "water valve")?;
    let pump = drive_inactive(peripherals.pins.gpio27, "pump")?;

    // 3. The heater. `BRING_UP_HEATER_LEDC` is false, so GPIO2 is driven as a
    //    plain inactive output — the same treatment as the pump and the valve,
    //    and strictly safer than a PWM carrier that trips the interrupt watchdog
    //    before its first duty write completes. See [`HEATER_LEDC_DEFECT`].
    let (heater, heater_description) = if BRING_UP_HEATER_LEDC {
        let output: HeaterOutput<LedcPwm<'_, _>> =
            LedcPwm::new(ledc.channel0, ledc.timer0, peripherals.pins.gpio2)
                .map(HeaterOutput::new_ledc)?;
        info!(
            "heater: LEDC carrier {CARRIER_HZ:?}, resolution {RESOLUTION:?}, \
             max_duty {}, gate closed until the supervisor beats",
            output.max_duty()
        );
        (Heater::Pwm(output), "LEDC duty 0")
    } else {
        error!("heater: {HEATER_LEDC_DEFECT}");
        // GPIO2 itself is driven inactive alongside the pump and the valve, so
        // the pin is at the safe level from before any of this runs.
        drive_inactive(peripherals.pins.gpio2, "heater")?;
        info!("heater: GPIO2 held low, gate still closed until the supervisor beats");
        (
            Heater::Inert(HeaterOutput::new(InertHeater::default(), 1)),
            "plain low",
        )
    };

    // 4. Read the pins back and assert. A failure here means an actuator is not
    //    in the state the machine considers safe, so nothing else may proceed.
    //
    //    The heater's readback is the transport's own view of what it drove:
    //    the LEDC duty register, or the GPIO level. Either is stronger than a
    //    bare assumption and needs no meter.
    let heater_inactive = match &heater {
        Heater::Pwm(output) => output.transport().hardware_duty() == 0,
        Heater::Inert(output) => output.transport().is_inactive(),
    };
    for (name, is_inactive) in [
        ("water valve", water_valve.is_low()),
        ("pump", pump.is_low()),
        ("heater", heater_inactive),
    ] {
        assert!(
            is_inactive,
            "startup readback failed: {name} is not inactive, refusing to continue"
        );
    }
    info!(
        "pin readback OK: heater=GPIO2 ({heater_description}) valve=GPIO17 \
         pump=GPIO27 all inactive"
    );

    // The plain drivers stay owned by `main` for the lifetime of the process.
    // In the real firmware they become `Actuators` (R3-03), the single owner of
    // the pump and the valve; nothing else may drive them.
    let _actuators = (water_valve, pump);

    // 5 + 6. The control task owns the watchdog subscription, the heartbeat, and
    //        the heater's deadman. It is also the *only* thing that can open the
    //        heater gate.
    let control = std::thread::Builder::new()
        .name("control".into())
        .stack_size(CONTROL_STACK_BYTES)
        .spawn(move || {
            if let Err(err) = control_task(twdt, heater, &mut temp_sensor) {
                error!("control task failed: {err}");
            }
        })?;

    // The control task is the only feed point, so `main` must not return while
    // it is alive. A panic inside it is fatal and is reported, not swallowed.
    control.join().map_err(|_| "control task panicked")?;

    Ok(())
}

/// Drives one actuator pin to the inactive level and returns the driver.
///
/// `input_output` rather than `output`, because the readback assertion needs to
/// observe the pin: in `esp-idf-hal` 0.47 `is_low()` is bounded on `InputMode`,
/// which `Output` does not implement (see `gpio.rs:362-380`).
fn drive_inactive<'d, P>(pin: P, name: &str) -> Result<PinDriver<'d, InputOutput>, EspError>
where
    P: InputPin + OutputPin + 'd,
{
    let mut driver = PinDriver::input_output(pin, Pull::Floating)?;
    driver.set_level(INACTIVE)?;
    info!("{name} driven inactive");
    Ok(driver)
}

/// Bring the 1-Wire bus up and read the DS18B20 once, so the boot log states
/// what is actually on the pin before the control loop starts.
///
/// A failure here is **not** fatal. The probe is a sensor: a machine with a
/// broken probe must still be able to run its state machine and report the
/// fault through S1, not refuse to boot. The C++ behaves the same way —
/// `TempSensorDallas` reports a failed read and `TempSensor::error_` is what
/// escalates it — so this matches it.
fn bring_up_temperature_sensor(
    pin: esp_idf_hal::gpio::Gpio16<'static>,
) -> Result<TemperatureSensor, EspError> {
    let mut bus = GpioOneWire::new(pin)?;
    let mut driver = Ds18b20Driver::new(DS18B20_ROM);

    // The resolution write is a one-time EEPROM cycle, so it happens here at
    // boot and never again (`DallasTemperature::setResolution` is called once
    // in the C++ constructor too, `TempSensorDallas.cpp:22`).
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

    Ok(TemperatureSensor { bus, driver })
}

/// The probe: a bit-banged bus plus the domain driver that decides what it
/// means.
struct TemperatureSensor {
    bus: GpioOneWire<'static>,
    driver: Ds18b20Driver,
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

impl Heater {
    /// Beat the deadman and drive a duty through whichever transport is fitted.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports. **The register holds the previous duty**
    /// on failure, so a caller must not conclude the heater is off.
    fn set_duty(&mut self, now: Millis, duty: Duty) -> Result<u32, EspError> {
        match self {
            Self::Pwm(output) => output.set_duty(now, duty),
            Self::Inert(output) => output.set_duty(now, duty),
        }
    }

    /// The deadman gate, so the supervisor task can beat into it.
    fn gate(&mut self) -> &mut cc_domain::heater::HeaterGate {
        match self {
            Self::Pwm(output) => output.gate(),
            Self::Inert(output) => output.gate(),
        }
    }

    /// The duty count the machine last asked for, before the gate.
    const fn requested_duty(&self) -> u32 {
        match self {
            Self::Pwm(output) => output.requested_duty(),
            Self::Inert(output) => output.requested_duty(),
        }
    }

    /// The transport's full-scale duty count.
    ///
    /// The inert transport is constructed with `max_duty = 1`, so "1 of 1" and
    /// the LEDC's "0 of 131072" both read as "nothing" in the log line below,
    /// which is the point: the line states the duty, not the resolution.
    const fn max_duty(&self) -> u32 {
        match self {
            Self::Pwm(output) => output.max_duty(),
            Self::Inert(output) => output.max_duty(),
        }
    }

    /// Why the output is being held at zero, or `None` if it is not.
    fn blocked_at(&self, now: Millis) -> Option<cc_domain::heater::GateBlock> {
        match self {
            Self::Pwm(output) => output.blocked_at(now),
            Self::Inert(output) => output.blocked_at(now),
        }
    }
}

/// Which transport the heater is driving.
///
/// Two arms because the choice is a *temporary* one, not a design one: the
/// LEDC arm is R1-07's and is expected to win back the slot once
/// [`HEATER_LEDC_DEFECT`] is resolved. Neither arm can raise a duty — the
/// inert one has no pin at all, and the LEDC one is behind a gate that is still
/// closed — so the union does not widen what the firmware can do to the boiler.
enum Heater {
    /// R1-07's LEDC carrier, when [`BRING_UP_HEATER_LEDC`] is set.
    Pwm(HeaterOutput<LedcPwm<'static, ledc::CHANNEL0<'static>>>),
    /// The stand-in used while it is not.
    Inert(HeaterOutput<InertHeater>),
}

/// A heater transport that drives nothing.
///
/// Used while [`BRING_UP_HEATER_LEDC`] is false. It implements
/// [`HeaterDuty`] faithfully — a non-zero duty is an error, not a write — so the
/// gate, the log line and the control loop are all exercised exactly as they
/// would be with hardware behind them, and the boiler stays cold.
#[derive(Clone, Copy, Default)]
struct InertHeater {
    /// The last duty fraction the gate let through, as `0.0..=1.0`.
    last_fraction: f32,
}

impl InertHeater {
    /// Whether nothing has been driven above zero.
    fn is_inactive(self) -> bool {
        self.last_fraction == 0.0
    }
}

impl cc_hal_esp32::heater::HeaterDuty for InertHeater {
    fn apply(&mut self, counts: u32, max_duty: u32) -> Result<(), EspError> {
        // Record what the gate let through, so a non-zero duty shows up in the
        // startup readback and the boot log rather than passing silently.
        #[allow(clippy::cast_precision_loss)]
        let fraction = if max_duty == 0 {
            0.0
        } else {
            counts as f32 / max_duty as f32
        };
        self.last_fraction = fraction;
        if counts != 0 {
            // A non-zero duty through this transport is a bug in the gate, and
            // the only correct response to a heater that must not be energised
            // is to refuse.
            return Err(EspError::from_infallible::<
                { esp_idf_svc::sys::ESP_ERR_INVALID_STATE },
            >());
        }
        Ok(())
    }
}

/// The control task: sole subscriber and sole feeder of the task watchdog
/// (04 §2, §3.4), and the only task that may open the heater gate.
///
/// It takes the heater by value, so the gate cannot be beaten from anywhere
/// else: there is exactly one holder of `&mut HeaterGate` in the program.
fn control_task(
    twdt: TWDT<'_>,
    mut heater: Heater,
    temp: &mut TemperatureSensor,
) -> Result<(), EspError> {
    // `TWDTConfig::new()` takes the timeout and the panic-on-trigger behaviour
    // from the ESP-IDF kconfig. R3-10 replaces this with explicit values, which
    // needs an `enumset` dependency to build the `EnumSet<Core>` of subscribed
    // idle tasks — deliberately not added in the spike.
    let config = TWDTConfig::new();
    info!(
        "control task: watchdog timeout {:?}, panic_on_trigger {}",
        config.duration, config.panic_on_trigger
    );

    let mut driver = TWDTDriver::new(twdt, &config)?;
    let mut watchdog = driver.watch_current_task()?;
    info!("control task: esp_task_wdt_add -> 0 (subscribed)");

    // Before the first beat the gate is closed, so this write is a no-op on the
    // hardware. It is made anyway so the log line below has something to report
    // and so the code path is exercised from the first iteration.
    let now = cc_domain::units::Millis::ZERO;
    let applied = heater.set_duty(now, Duty::new(0.0))?;
    info!("heater gate open? no — first beat not yet taken; duty {applied}");

    let mut tick: u32 = 0;
    loop {
        watchdog.feed()?;
        tick = tick.wrapping_add(1);

        // The supervisor heartbeat. This is what opens the deadman, and it is
        // deliberately the *same* beat as the watchdog feed: one thing that is
        // alive, one signal, rather than two that could disagree.
        let now = cc_domain::units::Millis::new(tick.wrapping_mul(HEARTBEAT_MS));
        heater.gate().heartbeat(now);

        // The heater command. Duty 0 in this binary: there is no PID here yet,
        // and R1-07's hardware test has not been run. The call is made anyway so
        // the gated path is the one that runs, and so the log line below is real.
        let applied = heater.set_duty(now, Duty::new(0.0))?;
        let requested = heater.requested_duty();
        info!(
            "control heartbeat {tick} — watchdog fed, duty {applied}/{requested} of {}, \
             gate {:?}",
            heater.max_duty(),
            heater.blocked_at(now).is_none()
        );

        // 5. The temperature read. Non-blocking by construction: the driver's
        //    conversion wait is a deadline the loop's own sleep covers, and the
        //    bus transactions are the only time spent here (~2 ms of bit-banging
        //    for a nine-byte scratchpad). The C++ does the same, and blocks for
        //    its 375 ms conversion inside the sensor's `getTempC`.
        match temp.driver.poll(&mut temp.bus, now) {
            Ok(ds18b20_domain::Poll::Reading(Ok(celsius))) => {
                info!(
                    "temperature: {celsius:.2} C (plausible: {})",
                    ds18b20_domain::is_plausible(celsius)
                );
            }
            Ok(ds18b20_domain::Poll::Reading(Err(fault))) => {
                // The C++'s `TempSensorDallas` logs and returns false for the
                // same faults; the count towards `error_` is the driver's.
                warn!("temperature: read failed: {fault}");
            }
            Ok(ds18b20_domain::Poll::Started | ds18b20_domain::Poll::Waiting) => {}
            Err(OneWireError::NoPresence) => {
                warn!("temperature: 1-Wire device stopped responding");
            }
            Err(OneWireError::Bus(err)) => error!("temperature: 1-Wire bus error {err}"),
        }

        FreeRtos::delay_ms(CONTROL_TICK_MS);
    }
}
