//! Clever Coffee firmware — R1-01 workspace bring-up, plus the R1-07 heater
//! output.
//!
//! # What this binary is (and is not)
//!
//! This is the **R1-01 feasibility spike** extended by **R1-07**, not the
//! firmware. Its jobs are to prove that the toolchain builds, links, boots and
//! runs on this host for `xtensa-esp32-espidf`, to make the first image-size
//! measurement (07 §5), and — from R1-07 — to bring up the `LEDC` heater output
//! and hold it at duty 0.
//!
//! What it does, in order:
//!
//! 1. Initialise ESP-IDF and route `log` to UART0 at 115200 baud — the same
//!    stream and baud rate the C++ firmware uses.
//! 2. Configure the pump and water-valve pins — GPIO17 and GPIO27
//!    (`include/clevercoffee/hardware/pinmapping.h:39-40`) — as outputs and
//!    drive them **inactive**.
//! 3. **Attach GPIO2 to an `LEDC` channel at [`CARRIER_HZ`] / [`RESOLUTION`] with
//!    duty 0.** This replaces the C++'s 10 ms heater ISR (`isr.h:85-118`) with
//!    hardware PWM. The pin is de-energised from the moment the channel is
//!    configured and there is no code path here that raises the duty.
//! 4. **Read every actuator pin back and assert it is inactive.** This is the
//!    startup assertion of 04 §4 / R3-16 in its smallest possible form, done
//!    before anything else exists so a failure is unambiguous. For the heater the
//!    readback is the `LEDC` duty register, not the pin: a channel configured at
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

mod network;

use core::error::Error;
use std::sync::Arc;

use cc_domain::hardware::TemperatureSensorType;
use cc_domain::sensor::ds18b20::{self as ds18b20_domain, Driver as Ds18b20Driver};
use cc_domain::sensor::onewire::{OneWireError, Rom};
use cc_domain::sensor::tsic306 as tsic306_domain;
use cc_domain::sensor::tsic306::Tsic306;
use cc_domain::units::{Duty, Millis};
use cc_hal_esp32::heater::{HeaterOutput, TimerIsrPwm};
use cc_hal_esp32::onewire::GpioOneWire;
use cc_hal_esp32::sensors::pins;
use cc_hal_esp32::time::now_ms;
use cc_hal_esp32::zacwire::{self, ZacwireCapture};
use core::fmt::Write as _;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{InputOutput, InputPin, Level, OutputPin, PinDriver, Pull};
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

/// Which temperature probe this build expects on GPIO16.
///
/// # The configuration default is `TSIC_306`; this is not that
///
/// `hardware.sensors.temperature.type` defaults to `TSIC_306` — the C++'s value
/// at `Config.h:1085-1092`, restored in `cc-config` and `cc-safety` when the
/// `TSIC-306` driver landed (R3-07). **The probe physically fitted to the attached
/// machine is a `DS18B20`** (family `0x28`, ROM `286937aacd78af41`, measured
/// 2026-09-28), and the C++ itself has the same mismatch: its default is a
/// `TSIC-306` and the machine ships with a `DS18B20`, which is why
/// `TempSensorDallas` and `TempSensorTSIC` both take `PIN_TEMPSENSOR` and the
/// firmware picks between them.
///
/// So the driver is selected from the **board**, not from the configuration, and
/// the configuration's value is logged alongside it. That is the honest
/// arrangement: the C++'s default is preserved where the C++ keeps it, and the
/// board's actual probe is not overridden by it. The failure mode is visible
/// either way — a machine configured for a sensor that is not fitted reports
/// `not connected` and names the sensor it asked for, which is the thing the C++
/// got wrong when it silently read the other bus.
///
/// The TSIC branch is **compiled and type-checked on every build** and is
/// dead-code-eliminated when this is `DallasDs18b20`, so the `ZACwire` driver's
/// flash cost is unmeasured until a `TSIC-306` board is selected. That is stated
/// rather than glossed: `just size` measures the `DS18B20` image.
const PROBE: TemperatureSensorType = TemperatureSensorType::DallasDs18b20;

/// Whether the `LEDC` heater output is brought up at boot.
///
/// **OFF, permanently, on this chip.** See [`HEATER_LEDC_DEFECT`]. The heater is
/// driven by a 10 ms `GPTimer` ISR ([`TimerIsrPwm`]), which is what the C++ and the
/// lost firmware both used.
const BRING_UP_HEATER_LEDC: bool = false;

// Stated at compile time so the finding cannot be quietly reversed by flipping a
// `const` and rebuilding: the original ESP32's `ledc_ll_set_duty_start` spins
// inside `portENTER_CRITICAL` for up to one carrier period, and no carrier that
// is slow enough for the contactor is fast enough for the 300 ms interrupt
// watchdog. See [`HEATER_LEDC_DEFECT`] and 09 §17.
const _: () = assert!(
    !BRING_UP_HEATER_LEDC,
    "LEDC at a low carrier trips the ESP32's interrupt watchdog; use the 10 ms \
     GPTimer ISR (see 09-cpp-findings.md section 17)"
);

/// 🔴 The heater cannot be driven by `LEDC` on this chip at 1 Hz.
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
/// Three things follow, and the third is what this firmware does.
///
/// 1. The "1 Hz or the contactor wears out" argument in [`cc_hal_esp32::heater`]
///    is correct as far as it goes and **incomplete**: it never checked what
///    ESP-IDF's own `ledc_ll_set_duty_start` does on this chip. Any carrier slow
///    enough to matter mechanically is also slow enough to trip a 300 ms
///    interrupt watchdog through that spin.
/// 2. The fix is not available at a higher carrier either. Raising it to, say,
///    25 Hz keeps the spin under 40 ms but reintroduces the contactor duty R1-07
///    set out to avoid, and bypassing the spin needs a register write this HAL
///    does not expose, and therefore `unsafe` — which the workspace denies
///    (`[workspace.lints.rust] unsafe_code = "deny"`).
/// 3. **So `LEDC` is not used, and the heater is chopped by a 10 ms `GPTimer` ISR**
///    ([`TimerIsrPwm`]) — which is what the C++ does (`isr.h:85-118`) and what the
///    lost firmware did (08 §3: *"heater interrupt running on GPIO2 (active high),
///    1000 ms window"*). The cost is 100 contactor operations per second instead
///    of two, which is what this machine has always done. A chip that panics at
///    boot is a worse trade than contactor wear.
///
/// `LedcPwm` stays in `cc-hal-esp32`, unbrought-up, behind the same
/// [`HeaterDuty`] seam, for a target whose chip does not have the spin. The spin
/// is unique to the original ESP32: every other `ledc_ll.h` in this tree
/// (`esp32c2`, `esp32c3`, `esp32c5`, and the s3/h2/p4 equivalents) has the loop
/// removed.
const HEATER_LEDC_DEFECT: &str =
    "R1-07's 1 Hz LEDC carrier spins in ESP-IDF's ledc_ll_set_duty_start for up \
      to one period with interrupts masked, which exceeds the ESP32's 300 ms \
      interrupt watchdog. The spin is unique to this chip -- every other \
      ledc_ll.h in the tree has it removed -- so there is no carrier that is both \
      slow enough for the contactor and fast enough for the watchdog. The heater \
      is driven by the C++'s own 10 ms GPTimer ISR instead. LedcPwm stays in \
      cc-hal-esp32 behind the HeaterDuty seam, unbrought-up, for a target whose \
      chip does not have the spin. See 09-cpp-findings.md section 17.";

/// The `DS18B20`'s ROM code on this machine, in the device's byte order.
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

/// `MachineState::PidNormal`'s discriminant, for `/api/status`.
///
/// **This firmware has no state machine wired in yet** — R2-08's reducer is a
/// separate crate and its handlers are not connected to this bring-up binary — so
/// the reported state is a constant. It is a named constant rather than a
/// literal so the number a browser sees traces to
/// `cc_domain::state::MachineState` and not to a magic number, and so the one
/// line to change when the reducer is connected is findable.
const MACHINE_STATE_PID_NORMAL: i32 = cc_domain::state::MachineState::PidNormal as i32;

/// The SSE event cadence, in milliseconds.
///
/// `WebServerManager::tempEventInterval_` as driven from
/// `LoopManager::updateWebsite`. One second, and the reason the `new_temps`
/// frame is small: it goes out per connected client per interval.
const SSE_INTERVAL_MS: u32 = 1_000;

/// How often the heap is logged, in milliseconds.
///
/// Sixty seconds. The heap moves on a scale of minutes under normal load and on
/// a scale of milliseconds under an OOM — and the OOM case is the one that
/// crashes before the next line would be printed, so this interval is for the
/// operator's benefit, not the machine's.
const HEAP_LOG_INTERVAL_MS: u32 = 60_000;

/// The radio's maintenance cadence.
///
/// `cc_hal_esp32::wifi::MONITOR_PERIOD_MS` and the C++'s
/// `checkAndMaintainConnection` interval (`CleverCoffeeWiFiManager.cpp:105`).
/// Named here rather than imported because the control task owns the poll and a
/// firmware that drifted between the two would drift silently.
const WIFI_POLL_MS: u32 = cc_hal_esp32::wifi::MONITOR_PERIOD_MS;

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
///
/// It does almost nothing: it hands the whole startup sequence to
/// [`bring_up`] on a thread whose stack is [`BRING_UP_STACK_BYTES`], and joins
/// it. The reason is in that constant; the short version is that `app_main`
/// runs on ESP-IDF's main task with a `CONFIG_ESP_MAIN_TASK_STACK_SIZE`-byte
/// stack (3584 in this build) and the startup sequence needs more than three
/// times that, so running it here overflows into DRAM and the symptom is an
/// allocator assert inside ESP-IDF's `tlsf` that names nothing useful.
fn main() -> Result<(), Box<dyn Error>> {
    // Must run before anything else, and before the thread: it applies the
    // ESP-IDF linker patches (`esp_idf_hal::sys::link_patches`) and brings up
    // the log sink, and both are process-wide. Their own stack frames are a few
    // dozen bytes, which is what makes them safe to leave on the small stack.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::init_from_env();
    info!("Clever Coffee Rust firmware — R1-01 toolchain bring-up");
    info!("target: xtensa-esp32-espidf, ESP-IDF: {IDF_VERSION}");

    // `Box<dyn Error>` is not `Send`, so the thread's return type is the
    // message rather than the error value, and the message is carried back as
    // an `io::Error` because that is how a `String` becomes a `Box<dyn Error>`.
    // The chain is flattened here, at the one place a boot failure is reported;
    // everything downstream of it is a single line on the console anyway.
    let outcome = std::thread::Builder::new()
        .name("bring-up".into())
        .stack_size(BRING_UP_STACK_BYTES)
        .spawn(|| bring_up().map_err(|err| std::io::Error::other(format!("{err}"))))
        .map_err(|err| -> Box<dyn Error> { err.into() })?
        .join()
        .map_err(|_| -> Box<dyn Error> { "the bring-up task panicked".into() })?;
    outcome.map_err(|err| -> Box<dyn Error> { err.into() })
}

/// The stack the startup sequence runs on, in bytes.
///
/// **Derived from the disassembly, not guessed.** The `diagnostic` profile is
/// byte-for-byte the same codegen as `release` (see the `justfile`), so
/// `xtensa-esp32-elf-objdump --dwarf=frames` on it gives the exact
/// `DW_CFA_def_cfa_offset` of every frame. The deepest chain in the startup
/// sequence, and its frames:
///
/// | frame | bytes |
/// | --- | --- |
/// | `firmware::bring_up` | 2400 |
/// | `network::bring_up_config` | 1584 |
/// | `BlobConfigStore::<EspNvsBlob>::{load,save}`, which inlines the whole `Config` serde | 3088 |
/// | five levels of `Deserialize` into the nested `Config` | ~450 each |
/// | `f64::from_str` (grisu), the leaf of every float parameter | 1712 |
///
/// which is about 11 KB. This constant is 16 KB, so the deepest chain has
/// roughly 40 % headroom.
///
/// The cost is 16 KB of the 154 KB DRAM heap, permanently: `FreeRTOS` takes a
/// task's stack from the same pool everything else comes from. That is 10 % of
/// the heap spent on not crashing, and the alternative — making the decode
/// shallower — is a change to `cc-config`'s serde shape that would have to be
/// redone for every future nesting level.
///
/// **How to re-derive it after a change:** re-run the `DWARF` dump above, add
/// the frames on the deepest path, and round up. A frame that grows should be
/// noticed here; the alternative is finding out from a `tlsf` assert whose
/// backtrace does not pass through this function.
const BRING_UP_STACK_BYTES: usize = 16 * 1024;

/// The startup sequence, 04 §4, in order.
#[allow(
    clippy::too_many_lines,
    reason = "this IS the startup sequence 04 §4 specifies, in order, and it \
              is read as a list. Splitting it would hide the ordering, which is \
              the one property that matters: the actuators are driven inactive \
              before the sensor, the sensor before the network, and the network \
              before any task exists."
)]
fn bring_up() -> Result<(), Box<dyn Error>> {
    let peripherals = Peripherals::take()?;

    // The TWDT driver is *moved* into the control task so the subscription
    // belongs to the control task and to nothing else (04 §2: "Watchdog feed —
    // control task only").
    let twdt = peripherals.twdt;
    // The LEDC peripheral is deliberately left unused. `LedcPwm` — the only
    // thing in this firmware that would want it — is unbrought-up because of
    // `HEATER_LEDC_DEFECT`, and *taking* the peripheral is how the previous build
    // ended up one step away from calling `ledc_set_duty_and_update`. Not taking
    // it at all means no future edit can reach a duty write by accident.
    //
    // `Peripherals` is `#[non_exhaustive]` and must be taken whole, so the field
    // is dropped with it. The comment is the record; the type is 4 bytes.
    let _ = peripherals.ledc;

    // The temperature probe. GPIO16 is `PIN_TEMPSENSOR` (`pinmapping.h:27`) and
    // whichever driver [`PROBE`] names is built on it. This is a **read**: the
    // 1-Wire bus is open-drain and the ZACwire line is an input, so nothing on
    // this pin is ever energised.
    let mut temp_sensor = bring_up_temperature_sensor(peripherals.pins.gpio16)?;

    // 2. The two plain actuator pins to `inactive`, in `main`, before any task
    //    exists, so there is no window in which a task could observe them
    //    un-driven.
    let water_valve = drive_inactive(peripherals.pins.gpio17, "water valve")?;
    let pump = drive_inactive(peripherals.pins.gpio27, "pump")?;

    // 3. The heater pin, driven **inactive and read back** before anything else
    //    touches it. This is the only moment at which the heater pin can be read
    //    back as a pin: the 10 ms ISR takes ownership of it immediately after,
    //    and from then on the firmware's view of the pin is the ISR's record of
    //    what it drove.
    let heater_pin = drive_inactive(peripherals.pins.gpio2, "heater")?;
    assert!(
        heater_pin.is_low(),
        "startup readback failed: heater is not inactive, refusing to continue"
    );

    // 4. The heater transport: the C++'s 10 ms GPTimer ISR.
    //
    //    `BRING_UP_HEATER_LEDC` is `false` and the `const` assert above makes
    //    that a **compile-time** fact rather than a runtime branch: there is no
    //    LEDC construction site in this binary, so there is no path on which a
    //    duty write can reach `ledc_ll_set_duty_start` and its watchdog-eating
    //    spin. The alternative is not gone — `LedcPwm` is in `cc-hal-esp32`
    //    behind the same `HeaterDuty` seam — it is just not wired up.
    info!("heater: {HEATER_LEDC_DEFECT}");
    // The pin is already inactive and has been read back. `TimerIsrPwm` takes
    // it, configures a 10 ms GPTimer, subscribes the ISR, and starts the timer —
    // with the chopper **disarmed**, so the timer ticks and the pin does not move
    // until the control task opens the gate.
    let transport = TimerIsrPwm::new(heater_pin)?;
    let heater_description = "10 ms GPTimer ISR, disarmed";
    info!(
        "heater: {heater_description}, duty {} ms, window {} ms, counter {}, \
         gate closed until the supervisor beats",
        transport.chopper().duty_ms(),
        transport.chopper().window_ms(),
        transport.chopper().counter_ms(),
    );
    // `max_duty = 1`: the ISR transport works in the C++'s own milliseconds and
    // `HeaterDuty::apply` converts the count back through
    // `cc_domain::heater::CHOSEN_MAX_DUTY`. A `max_duty` of 1 means the control
    // task's `set_duty` is a pure pass-through of the gate's decision, with no
    // second quantisation.
    let heater = HeaterOutput::new(transport, 1);

    // 5. Read the actuator pins back and assert. A failure here means an actuator
    //    is not in the state the machine considers safe, so nothing else may
    //    proceed.
    let heater_inactive = !heater.transport().is_high();
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

    // ---- R3: storage and the network tier ------------------------------
    //
    // 7. The configuration store, loaded through the fail-closed rule. This is
    //    before the Wi-Fi bring-up because the SSID and the hostname it needs
    //    come out of it, and because the boot log's line about a discarded
    //    configuration is the first thing an operator with a misbehaving machine
    //    needs to see.
    //
    //    Destructured rather than used as a struct: `store` moves into the
    //    control task (the only writer — `ConfigStore::load`/`save` take
    //    `&mut self`, and one owner beats a lock), `nvs_description` is the one
    //    thing the HTTP server is given about NVS, and `origin` is printed here
    //    and never needed again.
    let network::Booted {
        config,
        origin,
        store,
        nvs_description,
    } = network::bring_up_config()?;
    info!(
        "nvs: {nvs_description} — the C++ firmware's `config` namespace is a \
         different key space and is ignored by design (R3-08, decided 2026-09-28)"
    );
    info!("nvs: the boot decision was `{origin:?}`");

    // 8. The shared HTTP state and the network→control command queue.
    let net = Arc::new(network::Network::new());
    let commands = Arc::new(cc_hal_esp32::task::CommandQueue::new());

    // 9. Wi-Fi. Brought up only when a network is configured: the netif is
    //    created with the hostname on it (the ordering `WiFiStaConnect.h` exists
    //    to protect) and the monitor takes it from there.
    // Held for the life of `main` so the netif is never torn down while the httpd
    // task is serving `/api/status`.
    // 8b. The network stack, **before** the radio and before the HTTP server and
    //     unconditionally. `esp_netif_init` and `esp_event_loop_create_default`
    //     are once-per-process, and everything that opens a socket needs them —
    //     including the HTTP server, which does not care whether a radio
    //     exists. An unprovisioned machine must still serve `/api/status` and a
    //     telnet console, or there is no way to find out why it is unprovisioned.
    let sys_loop = cc_hal_esp32::wifi::init_stack()?;
    info!("netif: lwIP and the default event loop are up");

    let wifi = if config.is_wifi_provisioned() {
        Some(bring_up_wifi(peripherals.modem, &config, &sys_loop)?)
    } else {
        info!("wifi: no SSID configured — the UART provisioning task will run");
        None
    };

    // 10. The HTTP server. `EspHttpServer` is neither `Send` nor `Sync`, so it
    //     lives in this frame — and `main` blocks on `control.join()` for the
    //     life of the process, so its `Drop` (which stops the httpd task) never
    //     runs. See `network::start_http`.
    let _http = network::start_http(&net, &config, &nvs_description, Arc::clone(&commands))?;

    // 11b. The UART provisioning task, **only** when there is no SSID (04 §3.2:
    //     "The provisioning task is only spawned when no valid credentials
    //     exist, and it exits after success. It is never a permanent task").
    //     `Config::is_wifi_provisioned` is that same predicate, so the rule is one
    //     named function rather than two spellings of "the SSID is empty".
    //
    //     The handoff is shared with the control task, which is what does the
    //     writing: the store moved into that task in step 7, and a credential
    //     cannot be stored by a task that does not hold the store.
    let handoff = network::Handoff::new();
    if config.is_wifi_provisioned() {
        info!("wifi: a credential is stored — the provisioning task is not started");
    } else {
        start_provisioning(
            peripherals.uart0,
            peripherals.pins.gpio1,
            peripherals.pins.gpio3,
            handoff.clone(),
        );
    }

    // 11. MQTT, only when a broker is configured. `cc_config::Mqtt::default` has
    //     `enabled = false` and an empty broker, so an unprovisioned machine
    //     does not spend 4 KB of task stack and 2 KB of buffers on a client with
    //     nowhere to connect — the C++'s `MQTTManager.cpp:79-83` does the same.
    let mqtt_configured = cc_hal_esp32::mqtt::is_configured(&config);
    let mqtt_connected = if mqtt_configured {
        match cc_hal_esp32::mqtt::Client::new(&config) {
            Ok(client) => {
                info!("mqtt: {}", client.describe());
                // `Client::new` returns before the TCP connect completes -- it
                // is asynchronous on the client's own task -- so this is `false`
                // on a first boot and is not evidence of a fault. The
                // `ever_connected` flag is what a later check would read.
                client.ever_connected()
            }
            Err(err) => {
                warn!("mqtt: the client did not start: {err:?}");
                false
            }
        }
    } else {
        info!("mqtt: not configured (mqtt.enabled is false or mqtt.broker is empty)");
        false
    };

    // 5 + 6. The control task owns the watchdog subscription, the heartbeat, and
    //        the heater's deadman. It is also the *only* thing that can open the
    //        heater gate, and — since step 7 moved the store here — the only
    //        thing that can write the configuration.
    let control = std::thread::Builder::new()
        .name("control".into())
        .stack_size(CONTROL_STACK_BYTES)
        .spawn({
            let net = Arc::clone(&net);
            let commands = Arc::clone(&commands);
            move || {
                if let Err(err) = control_task(
                    twdt,
                    heater,
                    &mut temp_sensor,
                    &net,
                    &commands,
                    config.brew.setpoint,
                    mqtt_configured,
                    mqtt_connected,
                    store,
                    &handoff,
                    // The radio moves into the control task rather than staying
                    // in this frame. It is `Send` (`EspWifi` is, and `Monitor`
                    // and `String` are), and the control task is the only task
                    // with a watchdog subscription, so the 1 s
                    // `checkAndMaintainConnection` poll and the `/api/status`
                    // radio fields (`network::publish_radio`) both belong to the
                    // task whose stalls are already fatal. Before this, the
                    // radio was polled nowhere and `/api/status` reported
                    // `wifiAssociated: false` on a machine that was associated.
                    wifi,
                ) {
                    error!("control task failed: {err}");
                }
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

/// Bring up whichever probe [`PROBE`] names, and read it once.
///
/// A failure here is **not** fatal. The probe is a sensor: a machine with a
/// broken probe must still be able to run its state machine and report the fault
/// through S1, not refuse to boot. The C++ behaves the same way —
/// `TempSensorDallas` reports a failed read and `TempSensor::error_` is what
/// escalates it — so this matches it.
///
/// Both arms are compiled and type-checked; the one [`PROBE`] does not name is
/// dead-code-eliminated by the optimiser, because the comparison is on a `const`.
fn bring_up_temperature_sensor(
    pin: esp_idf_hal::gpio::Gpio16<'static>,
) -> Result<TemperatureSensor, EspError> {
    info!(
        "temperature: configuration default is TSIC_306 (Config.h:1085-1092), \\
         this board's probe is {PROBE:?}"
    );
    match PROBE {
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
    })
}

/// The `TSIC-306` arm: a `ZACwire` edge capture and the domain driver.
///
/// # 🔴 This arm has never run
///
/// **No `TSIC-306` is fitted to the machine this firmware was built for.** The
/// probe is a `DS18B20` and [`PROBE`] says so, so this function is compiled (the
/// types, the protocol arithmetic and the rejection paths are all checked) and
/// **not executed**. See `cc_domain::sensor::tsic306` and
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
    })
}

/// The probe: its bus, and the domain driver that decides what it means.
///
/// An enum, because the two arms own **different transports** and the pin: the
/// 1-Wire arm bit-bangs GPIO16 itself, and the `ZACwire` arm's capture owns it as
/// an interrupt-free sampler. `cc_domain::sensor::probe::TemperatureProbe` is the
/// interface the state machine will see, and this is the one place that has to
/// know which is which.
#[allow(
    clippy::large_enum_variant,
    reason = "the two arms are only ever one of them; a `Box` here would be a \
              second allocation for a value that is moved once at boot and never \
              again, and boxing the 45-byte DS18B20 arm to save nothing on the \
              other is the wrong direction"
)]
enum TemperatureSensor {
    /// A bit-banged 1-Wire bus plus the `DS18B20` pipeline.
    Dallas {
        bus: GpioOneWire<'static>,
        driver: Ds18b20Driver,
        /// The most recent reading, so the control task can publish it without a
        /// match on which driver is fitted.
        last_reading: LastReading,
    },
    /// A `ZACwire` edge capture, which is also the driver's `EdgeSource`, so there
    /// is one owner of the pin, one owner of the ring, and nothing shared.
    Tsic {
        driver: Tsic306<ZacwireCapture<'static>>,
        /// The most recent reading. See [`TemperatureSensor::Dallas`].
        last_reading: LastReading,
    },
}

/// `(celsius, plausible)` — the most recent reading.
///
/// The pair is kept together because that is what S1 consumes: the C++ keeps
/// `TempSensor::value_` and `TempSensor::error_` apart
/// (`TempSensor.h:88-95`) and `EmergencyStopManager` branches on the *flag*,
/// not on a NaN.
type LastReading = Option<(f64, bool)>;

impl TemperatureSensor {
    /// The most recent reading, and whether it was plausible.
    ///
    /// `None` before the first conversion completes, which is why
    /// `/api/temperatures` reports a temperature before it reports a plausible
    /// one: reporting `null` for a probe that has not spoken yet is honest, and
    /// reporting `0.0` is a disconnected probe wearing a plausible value.
    #[must_use]
    pub fn last_reading(&self) -> LastReading {
        match self {
            Self::Dallas { last_reading, .. } | Self::Tsic { last_reading, .. } => *last_reading,
        }
    }

    /// One step of whichever driver is fitted, and a log line for it.
    ///
    /// Both arms are non-blocking by construction — the `DS18B20` waits on a
    /// deadline the loop's own sleep covers, and the `TSIC-306` samples for a
    /// bounded window and returns — so this never stalls the control loop.
    fn poll(&mut self, now: Millis) {
        match self {
            Self::Dallas {
                bus,
                driver,
                last_reading,
            } => {
                match driver.poll(bus, now) {
                    Ok(ds18b20_domain::Poll::Reading(Ok(celsius))) => {
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
                        warn!("temperature: read failed: {fault}");
                    }
                    Ok(ds18b20_domain::Poll::Started | ds18b20_domain::Poll::Waiting) => {}
                    Err(OneWireError::NoPresence) => {
                        warn!("temperature: 1-Wire device stopped responding");
                    }
                    Err(OneWireError::Bus(err)) => {
                        error!("temperature: 1-Wire bus error {err}");
                    }
                }
            }
            Self::Tsic {
                driver,
                last_reading,
            } => {
                let mut buffer = tsic306_domain::ring::EdgeBuffer::new();
                let outcome = driver.poll(&mut buffer);
                match outcome {
                    tsic306_domain::Outcome::Reading(celsius) => {
                        // A `ZACwire` frame that decodes is a reading, and a
                        // decoded frame is by construction plausible (the decoder
                        // range-checks), so the flag is unconditionally true here.
                        *last_reading = Some((f64::from(celsius), true));
                        info!("temperature: {celsius:.2} C (ZACwire)");
                    }
                    other => {
                        if let Some(fault) = other.probe_fault() {
                            warn!("temperature: {fault} (ZACwire)");
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

/// The heater transport, held by the control task.
///
/// One arm, not two. R1-07 had a `LedcPwm` arm here and a stand-in, and the
/// stand-in is what the build used because the `LEDC` arm panicked the chip; now
/// that the ISR is the real transport, an enum with one variant is a lie about
/// there being a choice, and the `LEDC` alternative lives in `cc-hal-esp32` behind
/// `HeaterDuty` where it belongs. `BRING_UP_HEATER_LEDC` is the switch, and it is
/// `false`.
type Heater = HeaterOutput<TimerIsrPwm>;

/// The control task: sole subscriber and sole feeder of the task watchdog
/// (04 §2, §3.4), the only task that may open the heater gate, and the only
/// owner of the configuration store.
///
/// It takes the heater by value, so the gate cannot be beaten from anywhere
/// else: there is exactly one holder of `&mut HeaterGate` in the program. It
/// takes the store by value for the same reason — `ConfigStore::load` and
/// `save` both need `&mut self`, and one owner is better than a lock.
#[allow(
    clippy::too_many_arguments,
    reason = "the control task's inputs are the task's inputs; grouping them \
              into a struct would be a struct that exists only to be \
              destructured, and 04 §2's priority table is clearer as an \
              explicit signature"
)]
fn control_task(
    twdt: TWDT<'_>,
    mut heater: Heater,
    temp: &mut TemperatureSensor,
    net: &Arc<network::Network>,
    commands: &Arc<cc_hal_esp32::task::CommandQueue>,
    setpoint: f64,
    mqtt_configured: bool,
    mqtt_connected: bool,
    mut store: cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    handoff: &network::Handoff,
    mut sta: Option<cc_hal_esp32::Sta>,
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
    //
    // **The chopper stays disarmed until after the first beat.** Arming it before
    // the gate has been beaten once would mean the ISR is running at duty 0
    // before anything had decided the heater may be considered at all, which is
    // the C++'s ordering (`ctx->isISRReady()`, `isr.h:70-73`) and the recovered
    // firmware's *"output held off until the supervisor beats"* (08 §3).
    let now = cc_domain::units::Millis::ZERO;
    let applied = heater.set_duty(now, Duty::new(0.0))?;
    info!("heater gate open? no — first beat not yet taken; duty {applied}");

    let mut tick: u32 = 0;
    let mut last_sse_ms: u32 = 0;
    let mut last_heap_log_ms: u32 = 0;
    let mut wifi_last_ms: u32 = 0;
    loop {
        watchdog.feed()?;
        tick = tick.wrapping_add(1);

        // The network→control queue, drained at the top of every tick (04 §3.2).
        // A command is a *request*: nothing here acts on the radio or the
        // actuators directly, so a POST cannot reach past the tick.
        while let Some(command) = commands.recv() {
            info!("control: command {command:?}");
            if command == cc_hal_esp32::web::Command::Restart {
                net.shared.set_reboot_requested();
            }
            // Every other command needs the state machine (R2-08's handlers),
            // which is not wired into this bring-up binary. Acknowledged and
            // dropped, with the log line above, so the request is visibly
            // understood rather than silently lost.
        }

        // A credential typed on the console. This is the one place a `wifi set`
        // becomes durable, and it is here because the store is: the UART task
        // cannot write what it does not own, so it hands the value over and
        // this task picks it up within one control period.
        //
        // The reboot is unconditional on success and absent on failure. A stored
        // credential is useless until the radio is re-brought-up against it, and
        // `CleverCoffeeWiFiManager.cpp:148-151` does the same after a portal
        // save; a failed write leaves the machine running on what it already
        // has, which is the safe direction, and the operator sees the `error!`.
        if let Some(staged) = handoff.take() {
            match network::apply_staged(&mut store, staged) {
                Ok(()) => {
                    info!("config: a wifi credential from the console was stored; rebooting");
                    restart_now();
                }
                Err(err) => {
                    error!("config: the credential from the console was NOT stored: {err}");
                }
            }
        }

        // The supervisor heartbeat. This is what opens the deadman, and it is
        // deliberately the *same* beat as the watchdog feed: one thing that is
        // alive, one signal, rather than two that could disagree.
        let now = cc_domain::units::Millis::new(tick.wrapping_mul(HEARTBEAT_MS));
        heater.gate().heartbeat(now);

        // The heater command. Duty 0 in this binary: there is no PID here yet,
        // and R1-07's hardware test has not been run with the boiler
        // disconnected. The call is made anyway so the gated path is the one that
        // runs, and so the log line below is real.
        let applied = heater.set_duty(now, Duty::new(0.0))?;
        info!(
            "control heartbeat {tick} — watchdog fed, duty {applied} of {}, \
             gate {}, ISR ticks {}, on {} ({:.3})",
            heater.max_duty(),
            heater.blocked_at(now).is_none(),
            heater.transport().ticks(),
            heater.transport().on_ticks(),
            heater.transport().measured_on_fraction(),
        );

        // The temperature read. Non-blocking by construction: the DS18B20's
        // conversion wait is a deadline the loop's own sleep covers, and the bus
        // transactions are the only time spent there (~2 ms of bit-banging for a
        // nine-byte scratchpad). The TSIC-306 arm samples for a bounded window
        // and returns.
        temp.poll(now);
        // The reading, for the telemetry publish. `TemperatureSensor` owns the
        // driver, so this is the one number the control task reads out of it per
        // tick — and it is `None` until the first conversion completes, which
        // `/api/temperatures` reports as `null` rather than as a fake 0 °C.
        let last_reading = temp.last_reading();

        // A reboot request, honoured here and not in the HTTP handler. A handler
        // that called `esp_restart` directly could reset the machine from inside
        // a request; this is between ticks, after the watchdog has been fed.
        if net.shared.take_reboot_request() {
            info!("control: reboot requested — restarting");
            // A 500 ms pause so the HTTP response has left the socket and the
            // `202 Accepted` has reached the operator's browser, rather than the
            // connection being cut mid-write. The C++ does the same
            // (`WebServerManager.cpp:696`, `delay(1000)` before its restart).
            FreeRtos::delay_ms(500);
            restart_now();
        }

        // The telemetry publish and the SSE broadcast, at the C++'s cadence
        // (`WebServerManager.cpp:1128-1143` driven from
        // `LoopManager::updateWebsite`, gated on `tempEventInterval_`).
        let uptime = now_ms();
        net.shared.publish(network::telemetry_from(
            network::Reading {
                state: MACHINE_STATE_PID_NORMAL,
                temperature_c: last_reading.map_or(f64::NAN, |(celsius, _)| celsius),
                setpoint_c: setpoint,
                // Always 0: there is no PID in this build, and a fabricated
                // non-zero heater power would be a lie on the display and in
                // every telemetry consumer.
                heater_power_pct: 0.0,
                mqtt_configured,
                mqtt_connected,
            },
            uptime,
        ));

        // The radio's readings, published **after** the telemetry above and on
        // every tick rather than only on a radio poll. The order is load-bearing:
        // `Shared::publish` replaces the whole slot, so a radio publish before it
        // would be erased by the very next tick, and `/api/status` would go back
        // to reporting `wifiAssociated: false` — which is exactly the bug this
        // replaced. The signal bucket and the DHCP address also change without a
        // reconnect, and `/api/status` polls far more often than the radio does.
        network::publish_radio(&net.shared, sta.as_ref());

        // The radio's own maintenance, on the C++'s 1 s cadence
        // (`CleverCoffeeWiFiManager::checkAndMaintainConnection`, and
        // `cc_hal_esp32::wifi::MONITOR_PERIOD_MS`). `Sta` is `Send` and the
        // control task is the one place a 1 s poll belongs — it is the task with
        // a heartbeat and a watchdog, so a poll that stalls is visible.
        if uptime.wrapping_sub(wifi_last_ms) >= WIFI_POLL_MS {
            wifi_last_ms = uptime;
            if let Some(radio) = sta.as_mut() {
                if radio.poll() {
                    // The C++'s `networkCoordinator_->setOfflineMode`. The
                    // machine keeps serving on the LAN, which is the point of
                    // offline mode.
                    warn!("wifi: offline mode — unreachable off-LAN");
                }
            }
        }
        if uptime.wrapping_sub(last_sse_ms) >= SSE_INTERVAL_MS {
            last_sse_ms = uptime;
            network::broadcast_temps(net);
        }
        if uptime.wrapping_sub(last_heap_log_ms) >= HEAP_LOG_INTERVAL_MS {
            last_heap_log_ms = uptime;
            network::log_heap_once_a_minute(net);
        }

        FreeRtos::delay_ms(CONTROL_TICK_MS);

        // Arm the chopper once a beat has been taken. Doing it *after* the first
        // `set_duty` means the first duty the ISR ever sees is one that went
        // through the gate. `tick == 1` rather than a flag, so there is exactly
        // one place that can arm it and it is obviously after the first beat.
        if tick == 1 {
            heater.transport().arm();
            info!("heater: 10 ms ISR armed after the first supervisor beat");
        }
    }
}

/// Spawn the UART provisioning task on UART0.
///
/// **Only called when there is no SSID**, per 04 §3.2. A failure to install the
/// UART driver is **not** fatal — it warns and returns, and the machine runs
/// with no network surface rather than refusing to boot.
fn start_provisioning(
    uart: esp_idf_hal::uart::UART0<'static>,
    tx: esp_idf_hal::gpio::Gpio1<'static>,
    rx: esp_idf_hal::gpio::Gpio3<'static>,
    handoff: network::Handoff,
) {
    let serial = match cc_hal_esp32::provisioning::Serial::new(uart, tx, rx) {
        Ok(serial) => serial,
        Err(err) => {
            warn!("serial: the UART driver did not install: {err:?} -- provisioning is off");
            return;
        }
    };
    if let Err(err) = std::thread::Builder::new()
        .name("provision".into())
        .stack_size(network::PROVISION_THREAD_STACK_BYTES)
        .spawn(move || network::run_provisioning(serial, handoff))
    {
        warn!("serial: the provisioning task did not start: {err}");
    }
}

/// Reset the chip.
///
/// The reboot paths (`POST /api/restart`, `/api/factory-reset`,
/// `/api/wifi-reset`, and the control task's own decision after it has stored a
/// credential typed on the console) all land here, and the alternative to this
/// call is a `loop { FreeRtos::delay_ms(1_000) }` that never returns — which is
/// worse, because the machine would sit there with its relays in whatever state
/// the last tick left them rather than dropping them on reset.
///
/// The **drain** is not here and must not be added back: it lives in
/// [`cc_hal_esp32::restart::restart_now`], which every reboot path in the
/// workspace now shares. `esp_restart()` does not flush UART0 — it cut the
/// clock on the bytes in the UART's shift register — so calling `esp_restart()`
/// directly loses the console lines printed just before the reboot. That was a
/// shipped device bug.
fn restart_now() -> ! {
    cc_hal_esp32::restart::restart_now()
}

/// Bring the station interface up and associate.
///
/// # Errors
///
/// [`EspError`] from the netif, the driver, or the association itself. The
/// caller treats a failure as "the machine runs offline", which is what a
/// machine with no radio does.
fn bring_up_wifi(
    modem: esp_idf_hal::modem::Modem<'static>,
    config: &cc_config::Config,
    sys_loop: &esp_idf_svc::eventloop::EspSystemEventLoop,
) -> Result<cc_hal_esp32::Sta, EspError> {
    // The hostname goes in with the netif, before `wifi.start()`, so the DHCP
    // client identifier is fixed before the first association. See
    // `cc_hal_esp32::wifi`'s module documentation.
    let mut sta = cc_hal_esp32::Sta::new(modem, &config.system.hostname, sys_loop)?;
    sta.connect(
        &config.system.wifi.ssid,
        config.system.wifi.password.expose(),
    )?;
    if sta.wait_for_connection() {
        info!("wifi: associated, {}", sta.describe());
    } else {
        // The C++'s behaviour at `CleverCoffeeWiFiManager.cpp:105-110`: an
        // explicit SSID that does not associate means offline mode, no portal.
        warn!("wifi: the configured network is unavailable — running offline");
        sta.leave_offline();
    }
    Ok(sta)
}
