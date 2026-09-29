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

use cc_config::ConfigStore;
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

/// Whether to start the scale driver even when the configuration says it is off.
///
/// **ON in this build, and it is a bring-up override, not a default.** The
/// reasoning is the same one [`PROBE`] carries, and it is worth stating plainly
/// because a reader who does not will draw the wrong conclusion from a boot log
/// that says "scale: sampling".
///
/// * `hardware.sensors.scale.enabled` defaults to `false` (`cc-config`,
///   `HardwareSensorsScale::default`), and it is the right default: **no scale
///   is fitted to the machine this firmware was built for.** With the setting
///   honoured, the R3-17 driver would be dead code on this hardware — built,
///   type-checked, and never executed — which is *exactly* what the C++ does
///   with its scale (09 §23), and precisely the thing this task exists to stop.
///   The only difference the override can make here is the fault path, which is
///   the one acceptance criterion that **is** provable without a scale.
/// * The C++ guards on the same setting and then does nothing
///   (`src/main.cpp:145-150`: it logs `"Scale sensor support via
///   SensorCoordinator"` under a comment saying the work is pending). This
///   override is what makes the guard *mean* something.
///
/// **What this is not:** it does not make the scale work. There is no load cell,
/// no amplifier and no wiring on GPIO32/25/33, so the driver will report the
/// weight as absent and raise its fault within `SIGNAL_TIMEOUT` (100 ms). That
/// is the honest outcome and the boot log says so. When a scale is fitted, set
/// `hardware.sensors.scale.enabled` and this constant becomes irrelevant.
///
/// **Ship state is `false`, so the user's config actually governs.** Measured on
/// hardware 2026-09-29: with this on, 86 of 138 ticks over budget; with it off,
/// 111 of 136 — the same 32 ms worst either way, so the sampling task is *not*
/// what costs the tick (see the tick-timing section above). Nothing about the
/// scale needs the override, and leaving it on would mean a shipped firmware
/// ignores a setting the operator controls, which is the shape of 09 §23.
const BRING_UP_SCALE: bool = false;

/// The control tick's budget, in milliseconds.
///
/// 04 §2's hard 10 ms period, and the number R4-01b's acceptance criterion is
/// stated against ("zero ticks > 10 ms"). It is a *budget for the work*, not
/// the period: [`CONTROL_TICK_MS`] is the period, and a tick that spends longer
/// than this awake has overrun whatever it was given.
const TICK_BUDGET_MS: u32 = 10;

/// How many ticks form the pre-scale baseline.
///
/// 25 ticks at [`CONTROL_TICK_MS`] is 10 seconds — long enough for the
/// first-conversion settling to have happened, so the baseline is not
/// contaminated by the scale's own start-up, and short enough to be over before
/// an operator is waiting for a number.
const TICK_BASELINE_TICKS: u32 = 25;

/// How often the tick-timing report is logged, in milliseconds.
///
/// 60 s, matching the heap report. A 10 ms budget measured every 400 ms would
/// bury the boot log; once a minute is what an operator comparing "before" and
/// "after" the scale needs.
const TICK_REPORT_INTERVAL_MS: u32 = 60_000;

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

    // 5b. The scale's pins, if one is configured, and nothing else yet.
    //
    //     The three pins are GPIO32/GPIO25 (data) and GPIO33 (clock) — the
    //     scale's, and nothing else's (`pinmapping.h`). They are **inputs and
    //     one clock**, so nothing here can drive an actuator, and they are
    //     taken after the actuator readback above rather than before it, so
    //     the readback still covers every pin that can move a relay.
    //
    //     The configuration is not loaded yet at this point in the sequence, so
    //     whether a scale is fitted is decided below, once `config` exists. What
    //     is done here is the *pin* reservation, which is why the pins are
    //     taken unconditionally and the driver is built later: a driver that
    //     only sometimes exists must not be the thing that decides whether
    //     `Peripherals::take` hands out these pins.
    let scale_pins = ScalePins {
        data_1: peripherals.pins.gpio32,
        data_2: peripherals.pins.gpio25,
        clock: peripherals.pins.gpio33,
    };

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
        mut store,
        nvs_description,
    } = network::bring_up_config()?;
    info!(
        "nvs: {nvs_description} — the C++ firmware's `config` namespace is a \
         different key space and is ignored by design (R3-08, decided 2026-09-28)"
    );
    info!("nvs: the boot decision was `{origin:?}`");

    // 7b. The scale, **constructed at boot** — the one thing the C++ never does.
    //
    //     `src/main.cpp:145-150` guards on `hardwareSensorsScaleEnabled` and
    //     then logs `"Scale sensor support via SensorCoordinator"` under a
    //     comment saying the work is pending, binding `sensorCoord` and never
    //     using it. Nothing in the tree calls `HardwareContext::setScale`, so
    //     the C++ can never read a weight (09 §23). This is therefore new
    //     functionality, and the branch below is real: the driver is built, its
    //     sampling task started, and the weight reaches the telemetry the
    //     display and MQTT read.
    //
    //     The type selects one cell or two (`hardware.sensors.scale.type`,
    //     `cc_domain::hardware::ScaleType`), which is what makes that setting do
    //     something. A `Bluetooth` type is R3-18 and is refused here rather
    //     than silently reporting zero grams.
    let sampler = match bring_up_scale(scale_pins, &config, &mut store) {
        Ok(sampler) => sampler,
        Err(err) => {
            // Not fatal. A machine whose scale will not start must still run its
            // control loop, and the absence is visible in `/api/status` as a
            // null weight rather than as a machine that will not boot.
            warn!("scale: not started — {err}. The weight will be reported absent.");
            None
        }
    };
    if let Some(sampler) = sampler.as_ref() {
        info!(
            "scale: hardware.sensors.scale.type is {:?}, enabled — {}",
            config.hardware.sensors.scale.r#type,
            sampler.telemetry().describe(),
        );
    } else {
        info!(
            "scale: hardware.sensors.scale.enabled is {} — not fitted",
            config.hardware.sensors.scale.enabled,
        );
    }

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
                    config.hardware.sensors.scale.known_weight,
                    mqtt_configured,
                    mqtt_connected,
                    store,
                    sampler,
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

/// The three scale pins, taken from `Peripherals` and held until the driver is
/// built.
///
/// A named struct rather than three locals because the pins are taken in one
/// place and used in another: `bring_up` reserves them unconditionally — so
/// that the actuator readback above covers every pin before any of them is
/// spoken for — and the driver is built later, once the configuration says
/// whether a scale is fitted.
struct ScalePins {
    /// `PIN_HXDAT`, the first data line.
    data_1: esp_idf_hal::gpio::Gpio32<'static>,
    /// `PIN_HXDAT2`, the second data line, on a dual scale.
    data_2: esp_idf_hal::gpio::Gpio25<'static>,
    /// `PIN_HXSCK`, the clock, shared by both cells.
    clock: esp_idf_hal::gpio::Gpio33<'static>,
}

/// Build the scale driver and start its sampling task.
///
/// Returns `Ok(None)` when `hardware.sensors.scale.enabled` is `false` — which
/// is the **default** (`cc-config`, `HardwareSensorsScale::default`) and is not
/// an error: a machine with no scale must not spend a task, a stack and a
/// priority level on one. The `hardware.sensors.scale.*` settings are read here
/// and nowhere else, which is what makes them do something rather than describe
/// a feature that does not exist.
///
/// # Errors
///
/// [`EspError`] if the pins cannot be configured or the task cannot be created.
/// The caller treats this as "the machine runs without a scale and says so".
fn bring_up_scale(
    pins: ScalePins,
    config: &cc_config::Config,
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
) -> Result<Option<cc_hal_esp32::Sampler>, EspError> {
    let scale_config = &config.hardware.sensors.scale;
    // `hardware.sensors.scale.enabled` decides, **overridden by
    // `BRING_UP_SCALE`**. See that constant for why the override exists and
    // what it is not.
    if !scale_config.enabled && !BRING_UP_SCALE {
        return Ok(None);
    }
    if !scale_config.enabled {
        warn!(
            "scale: hardware.sensors.scale.enabled is false and BRING_UP_SCALE \
             is on — starting the driver anyway. This is a bring-up build: the \
             weight will read as absent because no scale is fitted, and that is \
             the fault path being exercised, not a working scale."
        );
    }

    // The rate is the C++'s `setGain(128)` from `begin()`
    // (`HX711_ADC.cpp:32`), which is the default of
    // `cc_domain::sensor::hx711::Rate`.
    let rate = cc_domain::sensor::hx711::Rate::default();

    // `samples` is an `i32` config parameter in `1..=20`; anything else in a
    // blob is clamped by the domain crate rather than rejected, because
    // refusing to sample would stop the scale entirely.
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "the config range is 1..=20 (`cc-config`) and the domain crate \
                  clamps again; a corrupt blob must not stop the scale, and a \
                  negative one must not be read as a huge positive"
    )]
    let average = scale_config.samples.clamp(1, i32::from(u8::MAX)) as u8;

    // 🔴 `hardware.sensors.scale.type` selects one cell or two, and this is
    // where that setting becomes real. The C++ has the same enum
    // (`Hardware::ScaleType`, `cc_domain::hardware::ScaleType`) and honours it
    // in `HX711Scale`'s two constructors (`HX711Scale.cpp:16-24`) — but never
    // constructs either, so the setting is inert there (09 §23).
    //
    // The two HX711 arms are the same shape — configure a bus, build a driver
    // — and differ only in how many data lines, so they are built first and
    // paired afterwards. `Bluetooth` is not an HX711 at all and returns before
    // either is built.
    let (bus, driver) = match scale_config.r#type {
        cc_domain::hardware::ScaleType::Hx711Dual => {
            let bus = cc_hal_esp32::GpioHx711::dual(pins.data_1, pins.data_2, pins.clock)?;
            let driver = cc_domain::sensor::hx711::Scale::dual(
                scale_config.calibration,
                scale_config.calibration2,
                average,
            );
            (bus, driver)
        }
        cc_domain::hardware::ScaleType::Hx711Single => {
            let bus = cc_hal_esp32::GpioHx711::single(pins.data_1, pins.clock)?;
            let driver = cc_domain::sensor::hx711::Scale::single(scale_config.calibration, average);
            (bus, driver)
        }
        // An Acaia BLE scale is R3-18: a different driver on a different
        // transport, reading a different device. Refused here, explicitly and by
        // name, rather than quietly reporting 0 g — which is the C++'s failure
        // mode in a different shape, a setting that reads as "the scale is
        // working" and measures nothing. Naming the variant rather than `_` is
        // deliberate: a fourth `ScaleType` then becomes a compile error here
        // instead of a fourth silently-refused case.
        other @ cc_domain::hardware::ScaleType::Bluetooth => {
            warn!(
                "scale: hardware.sensors.scale.type is {other:?}, which is an Acaia \
                 BLE scale (R3-18) and not an HX711 — refused. The weight is \
                 reported absent rather than as 0 g."
            );
            return Ok(None);
        }
    };

    // 🔴 The stored tare, handed to the sampler before its first reading.
    //
    // A tare in NVS is only useful if it is applied *before* the first weight
    // is published, or the machine briefly reports the un-tared offset — which
    // on a 267 g default known weight is a cup of coffee's worth of phantom
    // weight in `/api/status` and on the display. So this is read here and
    // queued immediately, before the task's first pass can publish anything.
    match cc_hal_esp32::nvs::load_tare(store.backend()) {
        Ok(Some(record)) => info!(
            "scale: a stored tare was found — cell 1 offset {}, cell 2 offset {}",
            record.offset_1, record.offset_2
        ),
        Ok(None) => info!("scale: no stored tare — the scale will tare at start-up"),
        Err(err) => warn!("scale: the stored tare could not be read ({err}) — taring at start-up"),
    }
    let stored_tare = cc_hal_esp32::nvs::load_tare(store.backend()).ok().flatten();

    let telemetry = Arc::new(cc_hal_esp32::scale::Telemetry::new());
    let sampler = cc_hal_esp32::Sampler::start(bus, driver, rate, Arc::clone(&telemetry))?;
    if let Some(record) = stored_tare {
        // A record the sampler refuses is a warning there, not an error here:
        // it will tare for itself, which is the correct outcome for a tare that
        // does not describe this scale. A `false` here means the command queue
        // was full, which at boot it cannot be.
        if !sampler.request_restore(record) {
            warn!("scale: the stored tare could not be handed to the sampler");
        }
    }
    Ok(Some(sampler))
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
#[allow(
    clippy::too_many_lines,
    reason = "this IS the control tick, and it is read as a list of what \
              happens in one period: feed the watchdog, drain the command \
              queue, take the staged credential, beat the heater gate, poll the \
              temperature, drain the scale, publish. Splitting it would hide the \
              ordering, which is the one property that matters -- the watchdog is \
              fed first and the reboot is taken last, and both of those are \
              properties of the list rather than of any one step."
)]
#[allow(
    clippy::needless_pass_by_value,
    reason = "`sampler` is owned by this task for the rest of the process: it \
              holds a queue shared with a task at a higher priority, and a \
              `&` would suggest the caller could still stop or replace it"
)]
fn control_task(
    twdt: TWDT<'_>,
    mut heater: Heater,
    temp: &mut TemperatureSensor,
    net: &Arc<network::Network>,
    commands: &Arc<cc_hal_esp32::task::CommandQueue>,
    setpoint: f64,
    known_weight: f64,
    mqtt_configured: bool,
    mqtt_connected: bool,
    mut store: cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    sampler: Option<cc_hal_esp32::Sampler>,
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

    // 🔴 The tick-timing measurement, which is R3-17's "the control tick is
    // unaffected" acceptance criterion and R4-01b's instrument.
    //
    // The number that matters is the **worst** tick, not the mean: the control
    // loop's budget is 10 ms and a mean says nothing about whether a tick ever
    // blew it. So this keeps a running max and a count, and the report names
    // both. It is reported in two phases, because "unaffected" is a comparison
    // and not an absolute: the first [`TICK_BASELINE_TICKS`] ticks are the
    // **baseline**, before the scale's sampling task is doing anything a
    // connected cell would not also do, and everything after is the
    // measurement. On a machine with no scale fitted the two phases are
    // identical, which is itself the result worth having.
    let mut tick_worst_ms: u32 = 0;
    let mut baseline_worst_ms: u32 = 0;
    let mut tick_over_budget: u32 = 0;
    let mut last_tick_report_ms: u32 = 0;
    loop {
        // Where this tick began, so the time spent in it can be measured. Taken
        // at the top of the loop, immediately after the last tick's sleep, so it
        // excludes the sleep itself — the sleep is the tick's *period*, and
        // including it would report 400 ms every time and say nothing.
        let tick_begun_ms = now_ms();
        watchdog.feed()?;
        tick = tick.wrapping_add(1);

        // The network→control queue, drained at the top of every tick (04 §3.2).
        // A command is a *request*: nothing here acts on the radio or the
        // actuators directly, so a POST cannot reach past the tick.
        while let Some(command) = commands.recv() {
            info!("control: command {command:?}");
            match command {
                cc_hal_esp32::web::Command::Restart => net.shared.set_reboot_requested(),
                // The scale commands are the first ones that are **not** inert.
                // In the C++ they set a flag on a `SensorCoordinator` that has
                // no scale registered (`WebServerManager.cpp:540-580`,
                // `MQTTManager.cpp:307-322`, 09 §23): accepted, answered 200,
                // and with no effect on any hardware. Here they reach the
                // sampling task, which owns the pins and does the work.
                cc_hal_esp32::web::Command::Tare => match sampler.as_ref() {
                    Some(sampler) if sampler.request_tare() => {
                        info!("scale: tare requested");
                    }
                    Some(_) => warn!("scale: the tare request was dropped — the sampler is behind"),
                    None => warn!("scale: tare requested with no scale fitted"),
                },
                cc_hal_esp32::web::Command::Calibrate => match sampler.as_ref() {
                    Some(sampler) if sampler.request_calibrate(known_weight) => {
                        info!("scale: calibration requested against {known_weight} g");
                    }
                    Some(_) => {
                        warn!("scale: the calibration request was dropped — the sampler is behind");
                    }
                    None => warn!("scale: calibration requested with no scale fitted"),
                },
                // Every other command needs the state machine (R2-08's handlers),
                // which is not wired into this bring-up binary. Acknowledged and
                // dropped, with the log line above, so the request is visibly
                // understood rather than silently lost.
                _ => {}
            }
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

        // The scale's events, drained every tick, and the weight. See
        // `drain_scale`: the event drain is the only place a tare can be
        // persisted, because this task is the only holder of the store.
        let weight_g = drain_scale(sampler.as_ref(), &mut store);

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
            weight_g,
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

        // 🔴 The tick's own cost, measured **before** the sleep.
        //
        // A first revision of this took the timestamp *after*
        // `delay_ms(CONTROL_TICK_MS)` and so reported ~431 ms — the sleep
        // itself, which is the tick's *period* and not its work. The tick budget
        // is about what the work costs; including the sleep makes every tick
        // look like a 40x overrun and the number says nothing. Measured on
        // hardware and fixed here, which is the only reason it is worth writing
        // down: a timing instrument that has never disagreed with a result is
        // not known to be working.
        let tick_elapsed_ms = now_ms().wrapping_sub(tick_begun_ms);
        if tick_elapsed_ms > tick_worst_ms {
            tick_worst_ms = tick_elapsed_ms;
        }
        if tick <= TICK_BASELINE_TICKS {
            if tick_elapsed_ms > baseline_worst_ms {
                baseline_worst_ms = tick_elapsed_ms;
            }
        } else if tick_elapsed_ms > TICK_BUDGET_MS {
            tick_over_budget += 1;
        }

        if tick_begun_ms.wrapping_sub(last_tick_report_ms) >= TICK_REPORT_INTERVAL_MS {
            last_tick_report_ms = tick_begun_ms;
            info!(
                "control tick: worst {tick_worst_ms} ms of the last {tick} \
                 (baseline {baseline_worst_ms} ms over the first \
                 {TICK_BASELINE_TICKS}, budget {TICK_BUDGET_MS} ms, \
                 {tick_over_budget} over budget) — scale: {}{}",
                sampler
                    .as_ref()
                    .map_or_else(|| "not fitted".into(), |s| s.telemetry().describe()),
                if sampler.as_ref().is_some_and(|s| s.telemetry().faulted()) {
                    " [FAULTED]"
                } else {
                    ""
                },
            );
        }

        // The tick's period. This is the sleep, and it is what the 10 ms figure
        // in 04 §2 is about; the measurement above is the work, which is the
        // number that has to stay under `TICK_BUDGET_MS`.
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

/// Drain the sampling task's events, persisting whatever must outlive a reboot,
/// and return the current weight.
///
/// # Why this is a function and not an inline block in the tick
///
/// Two reasons, and the second is the one that matters. The first is length:
/// the tick is read as a list of what happens in a period, and inlining sixty
/// lines of NVS error handling into it hides that. The second is that the
/// **control task is the only holder of the configuration store** —
/// `ConfigStore::load` and `save` both take `&mut self`, and one owner beats a
/// lock — so this is the only place on the machine where a completed tare or a
/// new calibration factor can be written down. Making that a named function is
/// what makes it findable.
///
/// The weight itself is *not* taken from an event. It is read from the shared
/// telemetry because a 10 Hz sample is a snapshot, not a message: losing one
/// loses nothing, and routing it through a queue that the 400 ms tick drains
/// would just add a copy and a second writer. Events — a completed tare, a new
/// factor — are the opposite: dropping one loses an operator's action, so they
/// come over a queue that is drained every tick.
///
/// # Errors
///
/// Never. Every failure here is a persistence failure, and it is reported on the
/// console and in the log rather than propagated: a scale that cannot be tare
/// persisted is still a working scale, and stopping the control task over it
/// would turn a cosmetic failure into a machine with no temperature reading.
fn drain_scale(
    sampler: Option<&cc_hal_esp32::Sampler>,
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
) -> Option<f64> {
    // A missing scale is a `None` weight, not an error: there is nothing here
    // to fail at, and returning early is the whole of the "not fitted" case.
    let sampler = sampler?;

    while let Some(event) = sampler.next_event() {
        match event {
            cc_hal_esp32::SamplerEvent::Tared { record } => {
                // 🔴 The acceptance criterion: a tare survives a reboot. The
                // C++ holds the tare in a `long` member (`HX711_ADC.h:66`) and
                // loses it on every reset, so a power cut means re-taring by
                // hand — and `HX711Scale::init` tares at boot anyway
                // (`HX711Scale.cpp:44`), so the C++ would re-tare on every boot
                // if it ran at all.
                match cc_hal_esp32::nvs::save_tare(store.backend_mut(), record) {
                    Ok(()) => info!(
                        "scale: tare persisted to NVS ({} B) — it survives a reboot",
                        cc_domain::sensor::hx711::TARE_RECORD_BYTES
                    ),
                    Err(err) => error!(
                        "scale: the tare was taken but could NOT be persisted: \
                         {err}. It will be lost on reboot."
                    ),
                }
            }
            cc_hal_esp32::SamplerEvent::Calibrated { factor_1, factor_2 } => {
                // The factor is a **configuration parameter**
                // (`hardware.sensors.scale.calibration` and `calibration2`),
                // not a tare, so it goes into the blob rather than beside it.
                // That is also what makes the setting survive a reboot, which
                // matters because recalibrating is a deliberate act an operator
                // performs once.
                let mut updated = match store.load() {
                    Ok(config) => config.unwrap_or_default(),
                    Err(err) => {
                        error!(
                            "scale: cannot read the configuration to store the calibration: {err}"
                        );
                        continue;
                    }
                };
                updated.hardware.sensors.scale.calibration = factor_1;
                if let Some(factor) = factor_2 {
                    updated.hardware.sensors.scale.calibration2 = factor;
                }
                match store.save(&updated) {
                    Ok(()) => {
                        info!(
                            "scale: calibration persisted — cell 1 {factor_1}{}",
                            match factor_2 {
                                Some(factor) => format!(", cell 2 {factor}"),
                                None => String::new(),
                            }
                        );
                    }
                    Err(err) => {
                        error!("scale: the calibration was applied but NOT persisted: {err}");
                    }
                }
            }
            cc_hal_esp32::SamplerEvent::Refused { what } => {
                warn!("scale: the sampler refused a {what} request");
            }
        }
    }

    // `None` when no scale is fitted, when the cell has produced nothing yet, or
    // when it is not answering. All three publish as `null` rather than as 0 g,
    // because 0 g is a real weight and a UI showing it shows a full cup.
    sampler.telemetry().weight_g()
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
