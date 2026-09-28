//! Clever Coffee firmware — R1-01 workspace bring-up.
//!
//! # What this binary is (and is not)
//!
//! This is the **R1-01 feasibility spike**, not the firmware. Its only jobs are
//! to prove that the toolchain builds, links, boots and runs on this host for
//! `xtensa-esp32-espidf`, and to make the first image-size measurement (07 §5:
//! "Phase 1 measures the minimal image. That number is the reference every later
//! phase is compared against, and it is the one that tells us whether the whole
//! migration fits at all").
//!
//! What it does, in order:
//!
//! 1. Initialise ESP-IDF and route `log` to UART0 at 115200 baud — the same
//!    stream and baud rate the C++ firmware uses.
//! 2. Configure the three actuator pins — heater GPIO2, water valve GPIO17,
//!    pump GPIO27 (`include/clevercoffee/hardware/pinmapping.h:38-40`) — as
//!    outputs and drive them **inactive**.
//! 3. **Read each pin level back and assert it is inactive.** This is the
//!    startup assertion of 04 §4 / R3-16 in its smallest possible form, done
//!    before anything else exists so a failure is unambiguous.
//! 4. Move a `TWDTDriver` into a control task and feed it from there, so the
//!    watchdog subscriber is the control task and nothing else (04 §2, §3.4 —
//!    the same shape the recovered oracle used, 08 §3).
//! 5. Log a heartbeat every second, so `just mon` has something to show.
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
//! There is deliberately **no** control loop, no state machine and no sensor
//! here. Those arrive at R2-08 and R3-xx.

use core::error::Error;

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{InputOutput, InputPin, Level, OutputPin, PinDriver, Pull};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, TWDT};
use log::{error, info};

type EspError = esp_idf_svc::sys::EspError;

/// The level that means "actuator de-energised" for a `HIGH_TRIGGER` relay.
const INACTIVE: Level = Level::Low;

/// Heartbeat period of the control task, and therefore the watchdog feed period.
const HEARTBEAT_MS: u32 = 1000;

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

    // 2. Actuator pins to `inactive`, in `main`, before any task exists, so
    //    there is no window in which a task could observe them un-driven.
    let heater = drive_inactive(peripherals.pins.gpio2, "heater")?;
    let water_valve = drive_inactive(peripherals.pins.gpio17, "water valve")?;
    let pump = drive_inactive(peripherals.pins.gpio27, "pump")?;

    // 3. Read the pins back and assert. A failure here means a pin is not in the
    //    state the machine considers safe, so nothing else may proceed.
    for (name, is_inactive) in [
        ("heater", heater.is_low()),
        ("water valve", water_valve.is_low()),
        ("pump", pump.is_low()),
    ] {
        assert!(
            is_inactive,
            "startup readback failed: {name} is HIGH, expected inactive (LOW)"
        );
    }
    info!("pin readback OK: heater=GPIO2 valve=GPIO17 pump=GPIO27 all inactive (LOW)");

    // The three drivers stay owned by `main` for the lifetime of the process.
    // In the real firmware they become `Actuators` (R3-03), the single owner of
    // the pump and the valve; nothing else may drive them.
    let _actuators = (heater, water_valve, pump);

    // 4 + 5. The control task owns the watchdog subscription and the heartbeat.
    let control = std::thread::Builder::new()
        .name("control".into())
        .stack_size(CONTROL_STACK_BYTES)
        .spawn(move || {
            if let Err(err) = control_task(twdt) {
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

/// The control task: sole subscriber and sole feeder of the task watchdog
/// (04 §2, §3.4).
fn control_task(twdt: TWDT<'_>) -> Result<(), EspError> {
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

    let mut tick: u32 = 0;
    loop {
        watchdog.feed()?;
        tick = tick.wrapping_add(1);
        info!("control heartbeat {tick} — actuators inactive, watchdog fed");
        FreeRtos::delay_ms(HEARTBEAT_MS);
    }
}
