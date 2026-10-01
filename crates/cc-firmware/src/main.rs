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

mod control;
mod display_task;
mod network;
/// Why there is no sensor task: a measured kernel defect, not an oversight.
mod sensor_task;
mod slots;

use core::error::Error;
use std::sync::Arc;

use cc_config::ConfigStore;
use cc_domain::hardware::TemperatureSensorType;
use cc_domain::sensor::ds18b20::{self as ds18b20_domain, Driver as Ds18b20Driver};
use cc_domain::sensor::onewire::{OneWireError, Rom};
use cc_domain::sensor::tsic306 as tsic306_domain;
use cc_domain::sensor::tsic306::Tsic306;
use cc_domain::units::{Celsius, Millis};
use cc_hal_esp32::heater::{HeaterOutput, TimerIsrPwm};
use cc_hal_esp32::onewire::GpioOneWire;
use cc_hal_esp32::sensors::pins;
use cc_hal_esp32::time::now_ms;
use cc_hal_esp32::zacwire::{self, ZacwireCapture};
use cc_hal_esp32::SwitchBank;
use cc_machine::Event;
use core::fmt::Write as _;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{InputOutput, InputPin, Level, OutputPin, PinDriver, Pull};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, TWDT};
use log::{debug, error, info, warn};

type EspError = esp_idf_svc::sys::EspError;

/// The level that means "actuator de-energised" for a `HIGH_TRIGGER` relay.
/// The level that de-energises a `HIGH_TRIGGER` relay — the level the bring-up
/// sequence drives each actuator pin to before anything reads it back.
///
/// **Why it is a constant and not the configuration:** a `LOW_TRIGGER` relay
/// energises whenever its pin *floats*, which is before this firmware runs, so
/// driving such a pin low to "de-energise" it would in fact **energise** it.
/// `cc_safety::validate_config` refuses `LOW_TRIGGER` for every relay for
/// exactly that reason, which means a stored configuration carrying one is
/// discarded at boot and the defaults — high-trigger — are what run here. The
/// relay *drivers* do honour the configured polarity
/// (`actuators::Polarity`); this is the one place where honouring it would be
/// the unsafe choice, and it is the reason the two differ.
const INACTIVE: Level = Level::Low;

/// The deadman gate's beat period, and therefore the upper bound on how long the
/// heater can stay energised after the control task stops.
///
/// **The beat is now every tick, not every second.** Before R4-01 the heartbeat
/// was synthesised as `tick * HEARTBEAT_MS` — a *guess* at the clock made from
/// the tick count, which is only the elapsed time if every tick took exactly
/// [`CONTROL_TICK_MS`]. It now reads [`cc_hal_esp32::time::now_ms`], so the beat
/// is the real time. This constant is what the 2-interlock-period deadman
/// (`cc_domain::heater::DEADMAN_TIMEOUT_MS`, 1000 ms) is measured against: the
/// 400 ms period is comfortably inside it, and the previous 1000 ms period was
/// *exactly* it, which left no margin for a late tick.
///
/// The watchdog feed has the same period, deliberately: one signal that the task
/// is alive, not two that could disagree.
const HEARTBEAT_MS: u32 = CONTROL_PERIOD_MS;

// Stated at compile time so the relationship cannot rot: the deadman drops the
// heater if the beat is older than `DEADMAN_TIMEOUT_MS`, so a tick period at or
// above it would mean a single late tick drops the heater. One interlock period
// of headroom is what makes the deadman a *supervisor* failure detector rather
// than a jitter detector. See `cc_domain::heater::DEADMAN_TIMEOUT_MS`.
const _: () = assert!(
    HEARTBEAT_MS * 2 <= cc_domain::heater::DEADMAN_TIMEOUT_MS,
    "the control tick must be at most half the deadman timeout, or one late \
     tick drops the heater"
);

/// The control task's period, in milliseconds — 100 Hz.
///
/// **This was 400 ms, and that was the bug the human reported twice.** The
/// value was justified in the C++'s terms: 400 ms is the temperature sensor's
/// cadence (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS`, `constants/Timing.h:42`)
/// and the ABP2's 50 ms divides into it exactly. But it coupled *every* thing in
/// the loop to the slowest sensor:
///
/// * a switch press waited up to 400 ms to be noticed, before the 20 ms
///   debounce and before the panel's 100 ms refresh — which is the "the screen
///   takes half a second to react" report;
/// * the whole 1 KB display frame was written **inside** the tick, so the tick
///   overran its 10 ms budget in ~62 % of ticks (09 §24) for a reason that had
///   nothing to do with control;
/// * and 04 §2 — the architecture of record — says "one `FreeRTOS` task, priority
///   5, **100 Hz**, hard 10 ms period". The code had diverged from the plan.
///
/// The three tasks now have their own cadences ([`sensor_task::SWITCH_POLL_MS`]
/// at 10 ms, [`display_task::REFRESH_MS`] at 100 ms) and this one keeps the
/// documented 100 Hz. The loop waits on the wake channel with this as the
/// timeout, so the period is a floor and an event is acted on at once.
const CONTROL_PERIOD_MS: u32 = 10;

/// How often a frame is handed to the display task, in milliseconds.
///
/// The panel's own refresh interval. Publishing more often would copy a
/// `DisplayInput` 100 times a second for a frame the panel drops.
const FRAME_PUBLISH_MS: u32 = display_task::REFRESH_MS;

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

/// Stack size of the display task.
///
/// 4 KB. The 1 KB scratch framebuffer is **heap** allocated (`Box`, in
/// `DisplayTask::new`) precisely so it is not in this budget — the same reason
/// `refresh_display` took it as an argument, recorded there as a stack overflow
/// on the first frame.
const DISPLAY_STACK_BYTES: usize = 8 * 1024;

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
/// the period: [`CONTROL_PERIOD_MS`] is the period, and a tick that spends longer
/// than this awake has overrun whatever it was given.
const TICK_BUDGET_MS: u32 = 10;

/// How many ticks form the pre-scale baseline.
///
/// 1,000 ticks at [`CONTROL_PERIOD_MS`] is 10 seconds — long enough for the
/// first-conversion settling to have happened, so the baseline is not
/// contaminated by the scale's own start-up, and short enough to be over before
/// an operator is waiting for a number.
const TICK_BASELINE_TICKS: u32 = 1_000;

/// How often the tick-timing report is logged, in milliseconds.
///
/// 60 s, matching the heap report. A 10 ms budget measured every 400 ms would
/// bury the boot log; once a minute is what an operator comparing "before" and
/// "after" the scale needs.
const TICK_REPORT_INTERVAL_MS: u32 = 30_000;

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
    //
    // **The transport choice is the firmware's, and it is stated here** rather
    // than as an enum in `cc-hal-esp32::actuators`: R1-07 had a `LedcPwm` arm and
    // a stand-in, and the stand-in is what the build used because the `LEDC` arm
    // panicked the chip (`HEATER_LEDC_DEFECT` above). `LedcPwm` stays in
    // `cc-hal-esp32` behind the same `HeaterDuty` seam for a target whose chip
    // does not have the spin, and this is the one line a different target changes.
    let heater: HeaterOutput<TimerIsrPwm> = HeaterOutput::new(transport, 1);

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

    // 5e. The I²C pins, reserved for whichever of the two I²C devices this
    //     machine has fitted.
    //
    //     **The ABP2 and the SSD1306 share one bus** — `PIN_I2CSDA`/`PIN_I2CSCL`
    //     are GPIO21/22 (`pinmapping.h:53-54`) and `04 §7` calls the bus
    //     "a mutex-guarded shared resource". The display driver is a separate
    //     piece of work (R4-xx) and it has not landed, so **this** task takes
    //     `I2C0` for the pressure sensor and the display task will have to share
    //     it. The reservation is here, next to the scale's, so the person who
    //     wires the OLED finds the conflict at the pin stage rather than as a
    //     `Peripherals::take` failure. See the note in `bring_up_pressure`.
    let i2c_pins = I2cPins {
        peripheral: peripherals.i2c0,
        sda: peripherals.pins.gpio21,
        scl: peripherals.pins.gpio22,
    };

    // 5b-bis. The shared I²C bus.
    //
    //     The ABP2 and the SSD1306 are on the same two wires (SCL 22, SDA 21)
    //     and an ESP32 I²C peripheral has exactly one owner, so the bus is put
    //     behind a `Mutex` **once**, here, and both users take it for the length
    //     of one transaction. Neither user may hold it across a frame: a panel
    //     that held the bus would starve the ABP2, and an ABP2 that held it
    //     would make the panel flicker. `display_shared::SharedBus` is the type
    //     that enforces this by only handing out a guard.
    let shared_i2c = match build_shared_i2c(i2c_pins) {
        Ok(bus) => {
            info!(
                "i2c: I2C0 (SDA GPIO{} SCL GPIO{}) at {} kHz, shared between the \
                 ABP2 and the panel",
                cc_hal_esp32::sensors::pins::I2C_SDA,
                cc_hal_esp32::sensors::pins::I2C_SCL,
                cc_hal_esp32::sensors::I2C_HZ / 1000,
            );
            Some(bus)
        }
        Err(err) => {
            warn!("i2c: the bus did not come up: {err:?} — no pressure, no display");
            None
        }
    };

    // 5c. The actuator facade, which becomes the single owner of the pump, the
    //     valve relay and the heater.
    //
    //     Built here, from the three drivers the readback above just proved
    //     inactive, and **moved into the control task** below. That move is the
    //     ownership statement: after it, the only `PinDriver<'static,
    //     InputOutput>` for GPIO17, GPIO27 and GPIO2 in the whole program is
    //     inside `cc_hal_esp32::Actuators`, and the only way to reach one of them
    //     is `cc_machine::applier::apply`. That is 04 §3.1's "a new state cannot
    //     accidentally poke a relay, because it has no way to reach one" made
    //     structural rather than aspirational.
    //
    //     The `test_only` inhibit is set here and **never changed afterwards**.
    //     See [`TEST_ONLY_INHIBIT`] for what is held off and why.
    // The relay polarities are **not known yet** — the configuration is loaded
    // further down, and reading it before that is what made the setting a lie in
    // the first place. The facade is built with the default, which is what
    // `validate_config` accepts for every relay, and
    // `set_relay_polarities` applies the configured values once the configuration
    // is in hand and before the control task exists.
    let mut actuators = cc_hal_esp32::Actuators::new(
        pump,
        water_valve,
        heater,
        cc_hal_esp32::actuators::RelayPolarities::all_high_trigger(),
    );
    actuators.set_inhibit(TEST_ONLY_INHIBIT);
    info!(
        "actuators: pump=GPIO27 valve=GPIO17 heater=GPIO2 owned by the control task; \
         test_only inhibit pump={} valve={} heater={}",
        TEST_ONLY_INHIBIT.pump, TEST_ONLY_INHIBIT.valve, TEST_ONLY_INHIBIT.heater
    );

    // 5d. The five operator inputs: the four switches and the tank float.
    //
    //     These are **inputs**, so taking them here cannot energise anything, and
    //     the configuration that decides how they are wired is not loaded yet —
    //     so the pins are reserved and `SwitchBank::new` is called below, once
    //     `config` exists. Same shape as the scale's pin reservation above and
    //     for the same reason: `Peripherals::take` happens once, and a driver
    //     that only sometimes exists must not be what decides which pins it
    //     hands out.
    let switch_pins = SwitchPins {
        power: peripherals.pins.gpio39,
        brew: peripherals.pins.gpio34,
        steam: peripherals.pins.gpio35,
        hot_water: peripherals.pins.gpio36,
        water_tank: peripherals.pins.gpio23,
    };

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

    // **The relay polarities, now that the configuration exists.** `Relay::on()`'s
    // branch, which the C++ wires from all three `trigger_type` parameters
    // (`HardwareManager.cpp:73,80,87`); the port carried two module-level
    // constants instead, so `hardware.relays.pump.trigger_type` was read by
    // nothing and a `LOW_TRIGGER` pump or valve relay was driven **inverted** —
    // `enable_pump` de-energised — reachable over plain
    // `POST /api/parameters`.
    //
    // In practice this reads high-trigger for every relay, because
    // `cc_safety::validate_config` refuses `LOW_TRIGGER` for all of them: such a
    // relay energises while its pin floats, which is before this firmware runs.
    // Applying the configured value is what makes the parameter honest rather
    // than silently ignored, and it is what would carry the day if that refusal
    // were ever relaxed.
    actuators.set_relay_polarities(cc_hal_esp32::actuators::RelayPolarities::from_config(
        &config.hardware.relays,
    ));
    info!(
        "actuators: relay polarity from hardware.relays.*.trigger_type — pump \
         active={:?} valve active={:?}",
        actuators.relay_polarity().pump.active,
        actuators.relay_polarity().valve.active
    );

    // 7a. The temperature probe, **after the configuration and before anything
    // that reads it**.
    //
    //     It used to be step 5, before the configuration existed, because the
    //     driver was chosen by a compile-time `const` rather than by
    //     `hardware.sensors.temperature.type`. Now the setting decides — as it
    //     does in the C++ — so the probe cannot be built before the value that
    //     selects it is known. The ordering that matters for safety is
    //     untouched: the actuators were driven inactive and the heater read back
    //     above, before this line.
    //
    //     GPIO16 is `PIN_TEMPSENSOR` (`pinmapping.h:27`) and whichever driver is
    //     selected is built on it. This is a **read**: the 1-Wire bus is
    //     open-drain and the ZACwire line is an input, so nothing on this pin is
    //     ever energised.
    let temp_sensor = bring_up_temperature_sensor(peripherals.pins.gpio16, &config)?;

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

    // 7c. The five operator inputs. Built here because this is the first point
    //     at which `config` exists, and the switch's type and mode come from it
    //     (`hardware.switches.*.type` / `.mode`, `Config.h:988-1060`).
    //
    //     A failure here is **fatal**, unlike the scale's. A scale that will not
    //     start costs a weight reading; a switch bank that will not configure
    //     costs every physical control the machine has, and the machine would sit
    //     there accepting web commands it could not be overridden on. The C++
    //     ignores the return of `pinMode` (`GPIOPin.cpp`) and reports a dead
    //     switch as a machine that does nothing, which is the same fault with
    //     less information.
    let switches = SwitchBank::new(
        switch_pins.power,
        switch_pins.brew,
        switch_pins.steam,
        switch_pins.hot_water,
        switch_pins.water_tank,
        &config,
    )?;

    // 7d. The ABP2 pressure sensor, when one is fitted.
    //
    //     `hardware.sensors.pressure.enabled` is the gate, exactly as
    //     `SensorCoordinator::updatePressure` uses it
    //     (`src/coordinators/SensorCoordinator.cpp:108-110`) — and the default is
    //     `false`, so a machine with no ABP2 fitted does not spend a bus, a
    //     driver or a 20 Hz deadline on one. What the C++ *cannot* do is read one
    //     without blocking: `pressureSensor.h:35` does `delay(10)` on every
    //     50 ms sample, which is 20 % of the C++ loop's wall clock (01 §4, and
    //     R4-01b's first listed win). `cc_domain::abp2::Driver` makes the 10 ms a
    //     deadline instead, so the sample is spread across ticks and nothing
    //     sleeps.
    // The bus is boxed and **moved into the control task**, which is what makes
    // `'static` references to it possible: the task is spawned with a `'static`
    // bundle, and a borrow of a local is not `'static` however long the local
    // lives in practice.
    //
    // Only the *bus* travels. The panel and the pressure sensor are built
    // **inside** the task, from that bus — putting them in the bundle too would
    // make it self-referential (a struct holding a reference into itself), which
    // no amount of `Box`ing resolves.
    //
    // A machine whose bus failed to come up still runs: the panel reports itself
    // absent and the pressure sensor is simply not fitted. Losing a display is
    // not a reason to stop making coffee.
    let shared_i2c: Option<Box<cc_hal_esp32::display_shared::SharedBus>> = shared_i2c.map(Box::new);

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
    //
    //     The parameter mailbox is created here, beside the command queue, because
    //     it is the same seam: two producers' worth of request, one consumer, and
    //     the control task drains both at the top of its tick.
    let parameters = Arc::new(cc_hal_esp32::task::ParameterHandoff::new());
    let _http = network::start_http(
        &net,
        &config,
        &nvs_description,
        Arc::clone(&commands),
        &parameters,
    )?;

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
    //        heater gate, the *only* thing that can write an actuator pin, and —
    //        since step 7 moved the store here — the only thing that can write the
    //        configuration.
    //
    //        `actuators` and `switches` move in. That move is the whole of 04 §3.1
    //        at the task level: after it, this thread holds every capability that
    //        can move a relay, and `cc_machine::applier::apply` is the only path
    //        from the state machine to one of them.
    //
    //        `config` moves in too, and it is a **clone**: `network::start_http`
    //        already took its own `Arc<Config>` for `/api/config` and
    //        `/api/parameters`, and `Context` borrows rather than owns (it is
    //        `Copy` and rebuilt every event — `cc_machine::context`'s rationale).
    //        So the control task holds the authoritative value the HTTP server
    //        does *not* see, and a `POST /api/parameters` that changes the
    //        setpoint is visible to the state machine on the next event and to
    //        `/api/config` only after a reboot. **That asymmetry is real and is
    //        recorded**, not papered over: it is the same shape as the C++'s
    //        singleton, minus the singleton.
    let control_config = config.clone();
    let known_weight = config.hardware.sensors.scale.known_weight;

    // ---- the two peer tasks, and the channels between the three -------------
    //
    // `switches` and `temp` move **out** of the control task and into the sensor
    // task, and the panel moves out into the display task. What stays here is
    // the wiring: one [`slots::SensorSlots`] for the readings, the edges and the
    // frame request, and one [`cc_hal_esp32::task::SignalQueue`] the producers
    // use to wake the consumer.
    //
    // The bus is leaked deliberately. It was a `Box` owned by the control task's
    // frame so its two users could hold `&'static` references into it; now two
    // *different* tasks need one, and a self-referential bundle holding a
    // reference into itself is not constructible. A `Box::leak` is the honest
    // spelling of what was always true — the bus lives for the whole program,
    // because all three of its users run forever.
    let frame = Arc::new(slots::FrameSlot::new());

    // The I²C bus, leaked. It has two users in **different** tasks — the ABP2 in
    // the control task and the panel in the display task — and both want a
    // `&'static` reference into it, which a `Box` owned by one task cannot
    // honestly give the other. A `Box::leak` says what was always true: the bus
    // lives for the whole program, because both of its users run until the
    // process ends. One bounded leak, documented, beats a lifetime lie.
    let shared_i2c: Option<&'static cc_hal_esp32::display_shared::SharedBus> =
        shared_i2c.map(|bus| &*Box::leak(bus));

    // The sensor task settles the switches and publishes where they rest. The
    // control task waits for that before it boots, because the C++'s
    // `finalizeMachineState` reads the power switch to choose between
    // `PID_NORMAL` and `PID_DISABLED` (`SystemInitializer.cpp:606-641`).
    // The panel, for the display task. Boxed, for the same reason `ControlArgs`
    // is: `bring_up` runs on ESP-IDF's main task, and a task bundle built *by
    // value* in this frame is a frame cost this function cannot see.
    let display_template = template_for(config.display.template);
    // `hardware.oled.enabled` — the flag exists in the schema and in the C++
    // (`Config.h:940-941`) and was read by nothing at all, so there was no way
    // to turn the panel off. The C++ gates the **display**, never the bus:
    // `SystemInitializer.cpp:338` only wires the display into the hardware
    // context when the flag is set, so every renderer early-outs on a null
    // display pointer. Here that means not calling `bring_up` at all, which is
    // also the cheaper reading on a shared bus — no `INIT_SEQUENCE`, no address
    // probe, no traffic.
    //
    // **Not** `set_blank(true)`: blanking is the *standby* mechanism
    // (`machine.standby.should_turn_off_display()`), it keeps the controller
    // alive so waking is free, and it does not stop the boot screens being
    // drawn. "Off" has to mean the panel is never touched.
    let panel = if config.hardware.oled.enabled {
        shared_i2c.map(cc_hal_esp32::display_shared::SharedPanel::bring_up)
    } else {
        info!("display: hardware.oled.enabled is false — the panel is not brought up");
        None
    };
    let display = Box::new(display_task::DisplayTask::new(
        panel,
        display_template,
        Arc::clone(&frame),
        Arc::clone(&net.shared),
    ));

    // The radio moves into the control task rather than staying in this frame.
    // It is `Send` (`EspWifi` is, and `Monitor` and `String` are), and the
    // control task is the only task with a watchdog subscription, so the 1 s
    // `checkAndMaintainConnection` poll and the `/api/status` radio fields
    // (`network::publish_radio`) both belong to the task whose stalls are already
    // fatal. Before this, the radio was polled nowhere and `/api/status`
    // reported `wifiAssociated: false` on a machine that was associated.
    //
    // **The box is built here, in `bring_up`'s 16 KB frame**, and the closure
    // carries only a pointer. See `ControlArgs` for why that matters: a by-value
    // argument is materialised in the caller's frame, and the caller here *is*
    // the 8 KB control stack.
    let args = Box::new(ControlArgs {
        twdt,
        actuators,
        switches,
        shared_i2c,
        temp: temp_sensor,
        net: Arc::clone(&net),
        commands: Arc::clone(&commands),
        frame: Arc::clone(&frame),
        parameters,
        config: control_config,
        known_weight,
        mqtt_configured,
        mqtt_connected,
        store,
        sampler,
        handoff: handoff.clone(),
        sta: wifi,
    });
    let control = std::thread::Builder::new()
        .name("control".into())
        .stack_size(CONTROL_STACK_BYTES)
        .spawn(move || {
            if let Err(err) = control_task(args) {
                error!("control task failed: {err}");
            }
        })?;

    // The two peers, spawned after the control task so the watchdog — which the
    // control task owns and which panics the chip when it trips — is already
    // subscribed before anything else starts moving.
    //
    // **The display task and nothing else.** A sensor task was written, measured
    // and removed: the DS18B20's bit-bang is the only user of
    // `esp_idf_hal::interrupt::free`, which on this chip is `vPortEnterCritical`
    // on a process-global cross-core critical section, and running it from a
    // second task asserts inside the FreeRTOS kernel on every boot. The bisect
    // table is in `sensor_task.rs`, which is now a note about why the sensor
    // task does not exist. The display task was in the same bisect and was clean
    // in every combination, so it stays.
    let display_thread = std::thread::Builder::new()
        .name("display".into())
        .stack_size(DISPLAY_STACK_BYTES)
        .spawn(move || display.run());
    if let Err(err) = display_thread {
        error!("display task could not be spawned: {err}");
    }

    // The control task is the only feed point, so `main` must not return while
    // it is alive. A panic inside it is fatal and is reported, not swallowed.
    control.join().map_err(|_| "control task panicked")?;

    Ok(())
}

/// Log where the switches are resting, once, at boot.
///
/// The four operator switches are **floating inputs** — GPIO34/35/36/39 have no
/// internal pull, and the C++ asks ESP-IDF for `IN_HARDWARE` (`pinmapping.h`,
/// `IOSwitch.cpp`). On a board where a switch is not wired a floating pin wanders
/// and the debouncer will eventually report a press. The switches are enabled by
/// default at the human's request, so this line is what answers "is my board
/// about to start a brew by itself?" — and it must say the level, not just that
/// the switch exists.
fn log_switch_levels(levels: cc_hal_esp32::switches::Levels) {
    info!(
        "switch resting levels after settling: power={} brew={} steam={} \
         hot_water={} water_tank={} -- a floating input reading high here with no \
         switch wired will eventually read as a press",
        levels.power, levels.brew, levels.steam, levels.hot_water, levels.water_tank_full,
    );
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
/// Both arms are compiled and type-checked, and now both are **linked**: the
/// selection is a runtime value, so the arm the operator does not choose is dead
/// weight rather than dead code. That is the honest cost of honouring the
/// setting, and it is what `just size` now reports.
fn bring_up_temperature_sensor(
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
        samples: 0,
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
type LastReading = Option<(f64, bool)>;

/// Which sensor fault was last written to the log.
///
/// A three-variant tag rather than the driver's own `Ds18b20Fault` because the
/// no-presence and bus-error arms report a different type (`OneWireError`), and
/// the point of the value is only "have I already said this".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DallasFaultTag {
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
    pub fn sample_seq(&self) -> u32 {
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
    fn poll(&mut self, now: Millis, sensor_fault_logged: &mut Option<DallasFaultTag>) {
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

/// The five operator inputs, taken from `Peripherals` and held until the
/// configuration says how they are wired.
///
/// Same shape and same reason as [`ScalePins`]: the pins are reserved before the
/// configuration is loaded, and the driver is built after. `SwitchBank::new`
/// takes them by value and returns the bank, so there is one owner of each pin
/// from `Peripherals::take()` onwards.
struct SwitchPins {
    /// `PIN_POWERSWITCH` (GPIO39).
    power: esp_idf_hal::gpio::Gpio39<'static>,
    /// `PIN_BREWSWITCH` (GPIO34).
    brew: esp_idf_hal::gpio::Gpio34<'static>,
    /// `PIN_STEAMSWITCH` (GPIO35).
    steam: esp_idf_hal::gpio::Gpio35<'static>,
    /// `PIN_WATERSWITCH` (GPIO36) — the hot-water button.
    hot_water: esp_idf_hal::gpio::Gpio36<'static>,
    /// `PIN_WATERTANKSENSOR` (GPIO23) — the tank float.
    water_tank: esp_idf_hal::gpio::Gpio23<'static>,
}

/// The shared I²C bus, taken from `Peripherals` and held until the ABP2 is built.
///
/// `I2C0` plus its two pins. **The ABP2 and the SSD1306 share this bus** —
/// `PIN_I2CSDA`/`PIN_I2CSCL` are GPIO21/22 for both (`pinmapping.h:53-54`) —
/// so whichever of the two lands second has to share the driver. 04 §7 calls it
/// "a mutex-guarded shared bus"; the mutex is R3-12's work and does not exist
/// yet, so **R4-01 takes the bus for the pressure sensor** and the display task
/// will have to either share this driver (`Abp2I2c::from_driver` exists for
/// exactly that) or wait.
struct I2cPins {
    /// `I2C0`, the only I²C peripheral this build uses.
    peripheral: esp_idf_hal::i2c::I2C0<'static>,
    /// `PIN_I2CSDA` (GPIO21).
    sda: esp_idf_hal::gpio::Gpio21<'static>,
    /// `PIN_I2CSCL` (GPIO22).
    scl: esp_idf_hal::gpio::Gpio22<'static>,
}

/// Build the ABP2 pressure sensor, when one is fitted.
///
/// `hardware.sensors.pressure.enabled` is the gate, exactly as
/// `SensorCoordinator::updatePressure` uses it
/// (`src/coordinators/SensorCoordinator.cpp:108-110`). It defaults to `false`, and
/// so a machine with no ABP2 spends no bus, no driver and no 20 Hz deadline on
/// one — and `/api/status` publishes `pressure: null`, which is the C++'s
/// "no pressure sensor" (`WebServerManager.cpp:356-372` omits the key).
///
/// A failure is **not** fatal. A pressure sensor that does not answer costs a
/// telemetry field, not the machine: the C++ guards on the same flag and, when
/// the flag is on, would simply log and carry on. Refusing to boot would be a
/// regression.
///
/// # What this fixes
///
/// `pressureSensor.h:35` does `delay(10)` inside `measurePressure()`, called from
/// `SensorCoordinator::updatePressure` on a 50 ms cadence
/// (`constants/Timing.h`). That is 10 ms of a 50 ms period — **20 % of the
/// control loop's wall clock spent asleep** (01 §4, and the first of R4-01b's
/// three listed wins). `cc_domain::abp2::Driver` makes the 10 ms a *deadline*
/// instead: the conversion command is written on one tick and the answer read on
/// a later one, so the sensor costs two I²C transactions spread over 10 ms of
/// normal control-loop work and no sleep at all.
/// Construct the I²C bus once and wrap it for sharing.
///
/// Deliberately separate from [`bring_up_pressure`]: the pressure sensor and
/// the panel are peers on this bus, and a helper that handed the bus to one of
/// them would be the bug this restructure exists to remove. The C++ sidesteps
/// it because Arduino's `Wire` is a singleton everyone reaches for, which is
/// convenient and is exactly the kind of implicit global this port is trying to
/// get rid of.
fn build_shared_i2c(
    pins: I2cPins,
) -> Result<cc_hal_esp32::display_shared::SharedBus, esp_idf_svc::sys::EspError> {
    let sda: cc_hal_esp32::sensors::SdaPin = pins.sda.into();
    let scl: cc_hal_esp32::sensors::SclPin = pins.scl.into();
    let bus = cc_hal_esp32::Abp2I2c::new(pins.peripheral, sda, scl)?;
    // `into_driver`: the `Abp2I2c` facade is a thin wrapper whose only job was
    // to own the bus, and the shared type owns it now.
    Ok(cc_hal_esp32::display_shared::SharedBus::new(
        bus.into_inner(),
    ))
}

fn bring_up_pressure<'bus>(
    bus: Option<&'bus cc_hal_esp32::display_shared::SharedBus>,
    config: &cc_config::Config,
) -> Option<cc_hal_esp32::Abp2Pressure<&'bus cc_hal_esp32::display_shared::SharedBus>> {
    if !config.hardware.sensors.pressure.enabled {
        info!(
            "pressure: hardware.sensors.pressure.enabled is false — no ABP2 (the C++ \
             makes the same check, SensorCoordinator.cpp:108)"
        );
        return None;
    }
    // The bus is shared with the panel, so the sensor borrows it rather than
    // owning it: it takes the lock for one transaction and gives it straight
    // back. `None` here means the bus itself failed to come up in `bring_up`.
    let bus = bus?;
    info!(
        "pressure: ABP2 on the shared I2C0 (SDA GPIO{} SCL GPIO{}) at 0x{:02X}, \
         non-blocking — the C++'s 10 ms delay is a deadline here, cadence {} ms",
        cc_hal_esp32::sensors::pins::I2C_SDA,
        cc_hal_esp32::sensors::pins::I2C_SCL,
        cc_domain::abp2::ADDRESS,
        cc_domain::abp2::CADENCE.raw(),
    );
    Some(cc_hal_esp32::Abp2Pressure::on_shared_bus(bus))
}

/// Persist `brew.setpoint` and report the outcome.
///
/// `WebServerManager.cpp:400` persists it inside the same handler that sets it,
/// so the two cannot disagree. Here the split is because the *running* setpoint
/// belongs to the control task's `Control` and the *stored* one belongs to the
/// store, and the store is the only durable thing — so the write is what makes
/// the change survive a reboot, and a failure to write is an `error!` rather than
/// a silent divergence.
fn persist_setpoint(
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    celsius: f64,
    config: &cc_config::Config,
) {
    let mut updated = config.clone();
    updated.brew.setpoint = celsius;
    match store.save(&updated) {
        Ok(()) => info!("config: brew.setpoint = {celsius} persisted"),
        Err(err) => {
            error!("config: brew.setpoint = {celsius} was applied but NOT persisted: {err}");
        }
    }
}

/// Persist `pid.enabled`, for the same reason as [`persist_setpoint`].
///
/// `setUserPidEnabled` persists the preference **and** sets the runtime flag
/// (`SystemUtils.h:34-40`), and `Command::SetUserPidEnabled` is the reducer half
/// of that. This is the other half.
fn persist_pid_enabled(
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    enabled: bool,
    config: &cc_config::Config,
) {
    let mut updated = config.clone();
    updated.pid.enabled = enabled;
    match store.save(&updated) {
        Ok(()) => info!("config: pid.enabled = {enabled} persisted"),
        Err(err) => error!("config: pid.enabled = {enabled} was applied but NOT persisted: {err}"),
    }
}

/// Persist the whole configuration after `POST /api/parameters`.
///
/// The C++ writes **one NVS key per parameter**, inside the setter
/// (`Config.h:164-172`), so a request with four parameters is four `Preferences`
/// transactions and a power cut between two of them leaves a configuration where
/// two values are new and ninety-six are old — for a machine that heats to
/// 150 °C. This store holds one blob ([`cc_config::store`]), so the whole
/// request is one write, and either all of it is durable or none of it is.
///
/// A failure is an `error!` and not a `400`: the HTTP response has already gone
/// by the time this runs, and the C++ counts a NVS failure as a parameter
/// failure (`Config.h:171-172`) only because its write is synchronous with the
/// request. Reporting it here is the honest equivalent.
fn persist_config(
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    config: &cc_config::Config,
) {
    match store.save(config) {
        Ok(()) => info!("config: the configuration was persisted"),
        Err(err) => error!(
            "config: the parameters were applied but NOT persisted, and a reboot will lose \
             them: {err}"
        ),
    }
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

/// 🔴 The `test_only` inhibit for R4-01's acceptance run.
///
/// **The pump and the valve relay are held off. The heater is not.** That split
/// is the whole of the safety argument for this task, so it is spelled out.
///
/// * **Why the pump and the valve are inhibited.** A brew switch press — or a
///   `POST /api/brew`, or a `backflush` command — makes the reducer emit
///   `EnablePump` and `OpenWaterValve`, and the state machine's job is to emit
///   them. The human has a real machine with a real reservoir, and R4-01's
///   acceptance criterion is about the **PID**, not about water. The inhibit
///   makes "the pump did not run" mean "the pump was inhibited", which
///   `cc_hal_esp32::actuators` counts and logs, rather than leaving the
///   distinction to a reading of `/api/status`.
///
/// * **Why the heater is not.** The acceptance criterion the human will check by
///   hand is *"with a target temperature of 30 °C the heater must NOT be at
///   100 % output"*. That is a statement about a duty the heater **reached**,
///   and it cannot be answered by a duty the code computed and then refused to
///   apply. The heater runs through `HeaterOutput::set_duty` and the 10 ms ISR
///   either way, and the deadman gate is armed on the first supervisor beat
///   exactly as it is in any other build — so the thing being proven is the real
///   path, not a shadow of it.
///
/// * **What bounds it.** A 30 °C setpoint against a ~24 °C boiler is a ~6 K
///   error, which is well inside the emergency threshold
///   (`safety.emergency_temp`, default 150 °C, `Config.h:813-829`) and inside the
///   200 °C plausibility ceiling. Nothing here can run away: S1 trips on three
///   consecutive readings above the threshold, and the state machine's own
///   `should_pid_be_enabled` refuses the duty in `PID_DISABLED`, `SENSOR_ERROR`,
///   `EMERGENCY_STOP`, `STANDBY` and every backflush state.
///
/// **To return to a machine that can brew:** set
/// [`cc_hal_esp32::Inhibit::NONE`]. That is a one-line change and a rebuild, and
/// it is the correct ship state once R4-04's safety-path procedures have been
/// written and reviewed — 06 lists them as a separate task for exactly this
/// reason.
const TEST_ONLY_INHIBIT: cc_hal_esp32::Inhibit = cc_hal_esp32::Inhibit {
    pump: true,
    valve: true,
    heater: false,
};

/// The long-press "REBOOTING" pause, in milliseconds.
///
/// `PowerHandler::triggerSystemReboot` (`PowerHandler.h:177-192`) shows the
/// message and waits. There is no display in this build, so the wait is all that
/// survives of it — and it is still load-bearing: it is what lets the operator
/// see that the long press did something before the console goes away.
const REBOOT_DISPLAY_MS: u32 = 1_000;

/// How often the PID's own P/I/D and the actuator refusals are logged, in
/// milliseconds.
///
/// Once a second: often enough that a saturation is visible while it is
/// happening, rare enough that the console stays readable during a brew. The
/// C++ logs the same numbers on a state change only
/// (`ProcessController.cpp:176-197`), which is not enough — a PID that pins at
/// 100 % without a state change is exactly the failure this exists to catch.
/// How often the heartbeat *line* is written. The heartbeat itself is every
/// [`HEARTBEAT_MS`]; the line is a diagnostic, and a 115200-baud console makes
/// it an expensive one. See the note at the line itself.
const HEARTBEAT_LOG_INTERVAL_MS: u32 = 1_000;

const PID_LOG_INTERVAL_MS: u32 = 1_000;

/// The control task's inputs, as one struct.
///
/// A struct rather than thirteen positional arguments because the list has grown
/// past the point where a call site can be read: the argument order stopped
/// carrying information, which is the failure a `too_many_arguments` allowance
/// papers over rather than fixes. Named fields at the call site are also what
/// makes a "did the heater go in here or there?" question answerable.
///
/// # It is passed as a `Box`, and that is load-bearing
///
/// 04 §2 gives the control task **8 KB** of stack. This struct is ~1.5 KB of it
/// (`Config` alone is 632 bytes, plus five debounced switches, the actuator
/// facade with its heater transport, the scale, the radio and the store), and a
/// by-value argument is **materialised in the caller's frame and then copied into
/// the callee's**: the thread closure's frame *is* the control task's stack, so
/// passing it by value cost two live copies of the whole struct before
/// `control_task`'s first statement. That overflowed the 8 KB stack on hardware
/// and the machine died in `Handoff::take` on a garbage pointer, four steps into
/// the tick — a fault that points at nothing resembling its cause.
///
/// `Box` puts one pointer on the stack and the bytes on the heap, and the heap
/// is measured (ADR-0002's floor, the once-a-minute report). The alternative —
/// a bigger stack — spends a permanent 8 KB more of DRAM on a value that is
/// genuinely large, and would hide the next frame that grows.
struct ControlArgs {
    /// The task watchdog, moved in so the subscription belongs to this task and
    /// no other.
    twdt: TWDT<'static>,
    /// The actuator facade: the only owner of the pump, the valve and the
    /// heater, and the only thing that can beat the heater's deadman.
    actuators: cc_hal_esp32::Actuators,
    /// The five operator inputs, debounced.
    switches: cc_hal_esp32::SwitchBank,
    /// The leaked I²C bus, lent to the ABP2 this task polls every tick. The
    /// panel in the display task holds the same reference; see the `Box::leak`
    /// in `bring_up` for why it is leaked rather than owned here.
    shared_i2c: Option<&'static cc_hal_esp32::display_shared::SharedBus>,
    /// The temperature probe. **Owned**, not borrowed: the control task polls it
    /// every tick for the rest of the process, and a `&'static mut` would be a
    /// lifetime this call site cannot honestly promise.
    ///
    /// It stays on *this* task, and not on a sensor task of its own, because of
    /// a measured toolchain defect: the DS18B20's bit-bang is the only user of
    /// `esp_idf_hal::interrupt::free`, which on this chip is `vPortEnterCritical`
    /// on a process-global cross-core critical section, and running it from a
    /// second task asserts inside the `FreeRTOS` kernel on every boot. The full
    /// bisect is in the module that used to hold the sensor task; the short
    /// version is three rows of a table and it is worth reading before anyone
    /// tries this again.
    temp: TemperatureSensor,
    /// The shared HTTP/MQTT telemetry slot.
    net: Arc<network::Network>,
    /// The bounded network→control command queue (04 §3.2).
    commands: Arc<cc_hal_esp32::task::CommandQueue>,
    /// The frame hand-off to the display task. See [`slots::FrameSlot`].
    frame: Arc<slots::FrameSlot>,
    /// The parameter-write mailbox, drained at the same point in the tick.
    ///
    /// Separate from `commands` because a parameter write is a *list* of pairs
    /// and a `Command` is `Copy` (04 §3.2, and `esp-idf-hal` 0.47 `task.rs:978`).
    /// See [`cc_hal_esp32::task::ParameterHandoff`].
    parameters: Arc<cc_hal_esp32::task::ParameterHandoff>,
    /// The authoritative configuration, as the control task's own copy.
    config: cc_config::Config,
    /// `hardware.sensors.scale.known_weight`, for a calibration request.
    known_weight: f64,
    /// Whether a broker is configured at all.
    mqtt_configured: bool,
    /// Whether MQTT has a session.
    mqtt_connected: bool,
    /// The configuration store. **Moved**, not borrowed: `ConfigStore::load` and
    /// `save` both take `&mut self` and one owner beats a lock.
    store: cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    /// The scale's sampling task, owned for the rest of the process.
    sampler: Option<cc_hal_esp32::Sampler>,
    /// The UART provisioning handoff.
    handoff: network::Handoff,
    /// The radio.
    sta: Option<cc_hal_esp32::Sta>,
}

// Stated at compile time, because the number is the whole argument for the
// `Box` and it is the kind of thing that grows silently. `Config` is 632 bytes
// of it; the rest is five debounced switches, the actuator facade (which
// contains the heater transport), the scale, the radio and the store.
//
// A raise here is not automatically wrong — the struct is on the heap now, so
// the cost is heap rather than stack — but it should be a decision rather than a
// diff, because it is roughly this many bytes of the ~154 KB DRAM heap.
const _: () = assert!(
    std::mem::size_of::<ControlArgs>() < 2048,
    "ControlArgs has outgrown 2 KB; see its documentation before raising this"
);

/// The control task: sole subscriber and sole feeder of the task watchdog
/// (04 §2, §3.4), the only task that may open the heater gate, the only task
/// that may write an actuator pin, and the only owner of the configuration
/// store.
///
/// # What it owns, and why that is the point
///
/// Before R4-01 this task owned the watchdog and a heater transport and nothing
/// else, and the state machine did not exist on the device at all. Now it owns
/// [`cc_hal_esp32::Actuators`], which owns every actuator pin, and
/// [`control::Control`], which owns the reducer. The tick below is therefore the
/// **only** path from a sensor reading to a relay in the whole program, and
/// `cc_machine::applier::apply` is the only function in it that writes hardware.
///
/// # The tick, in order, and why the order is the C++'s
///
/// 1. **Feed the watchdog.** First, always. A control loop that can starve
///    itself is the failure the watchdog exists to catch, and feeding it last
///    would mean a tick that overran is reported as a tick that hung.
/// 2. **Drain the command queue** (04 §3.2). Every command becomes an
///    [`cc_machine::Event::Command`] and goes through the reducer, so a `POST`
///    cannot reach past the tick into control state — the coupling 04 §3.2 exists
///    to remove.
/// 3. **Take a staged Wi-Fi credential**, if the console staged one.
/// 4. **Sense**: the temperature, the five switches, the pressure, the tank.
/// 5. **Beat the heater's deadman**, on the *same* signal as the watchdog feed.
///    One thing that is alive, one signal, rather than two that could disagree.
/// 6. **Decide**: fold the events through `cc_machine::reduce`.
/// 7. **Act**: `cc_machine::apply`, front to back, no coalescing.
/// 8. **Notify**: telemetry, SSE, the radio, the scale's events.
/// 9. **Sleep** for the period.
///
/// Steps 4-7 are [`control::Control::tick`] plus the applier, and they are the
/// part R4-01b measures.
#[allow(
    clippy::too_many_lines,
    reason = "this IS the control tick, and it is read as a list of what \
              happens in one period. Splitting it would hide the ordering, which \
              is the one property that matters -- the watchdog is fed first and \
              the reboot is taken last, and both of those are properties of the \
              list rather than of any one step."
)]
fn control_task(args: Box<ControlArgs>) -> Result<(), EspError> {
    let ControlArgs {
        twdt,
        mut actuators,
        mut switches,
        // The I²C bus, owned by this frame and lent to the pressure sensor built
        // immediately below. Binding it is what keeps it alive for the life of
        // the task, which is what makes the sensor's `&'static` reference sound.
        shared_i2c,
        mut temp,
        net,
        commands,
        frame,
        parameters,
        mut config,
        known_weight,
        mqtt_configured,
        mqtt_connected,
        mut store,
        sampler,
        handoff,
        mut sta,
    } = *args;
    // `TWDTConfig::new()` takes the timeout and the panic-on-trigger behaviour
    // from the ESP-IDF kconfig. R3-10 replaces this with explicit values, which
    // needs an `enumset` dependency to build the `EnumSet<Core>` of subscribed
    // idle tasks — deliberately not added in the spike.
    let wdt_config = TWDTConfig::new();
    info!(
        "control task: watchdog timeout {:?}, panic_on_trigger {}",
        wdt_config.duration, wdt_config.panic_on_trigger
    );

    let mut driver = TWDTDriver::new(twdt, &wdt_config)?;
    let mut watchdog = driver.watch_current_task()?;
    info!(
        "control task: esp_task_wdt_add -> 0 (subscribed); tick and deadman beat \
         every {HEARTBEAT_MS} ms, deadman {} ms",
        cc_domain::heater::DEADMAN_TIMEOUT_MS
    );

    let mut side = cc_hal_esp32::FirmwareSide::new();

    // The ABP2, borrowed from the shared I²C bus. It is a peer of the panel, not
    // an owner: neither holds the bus across a transaction, so the panel cannot
    // starve it and it cannot make the panel flicker.
    let mut pressure = shared_i2c.and_then(|bus| bring_up_pressure(Some(bus), &config));

    // The machine, booted through the reducer. `SystemInitializer::finalizeMachineState`
    // reads the power switch *before* the state machine exists
    // (`SystemInitializer.cpp:606-641`), and the switch bank now belongs to the
    // sensor task — so the levels are **waited for** here rather than polled
    // here, with a bound so a sensor task that never starts cannot hang the boot
    // forever.
    let boot_now = Millis::new(now_ms());
    // The switch bank's first poll, which is a **dead** read for a debounced
    // switch and deliberately so: `Debounced` seeds its state to "not pressed"
    // and only accepts a change after [`cc_domain::switch::DEBOUNCE`] (20 ms),
    // so the level reported here is the C++'s `currentState == HIGH` on the
    // first loop, which `IOSwitch.cpp:19` also seeds to `LOW`. A toggle power
    // switch therefore reads "off" at boot even if the operator has it on, and
    // the machine starts in `PID_DISABLED` — which is the C++'s behaviour, not
    // a bug in the port, and the reason the next tick (ten debounce windows
    // later) is what settles it.
    let _ = switches.poll(Millis::new(now_ms()));
    let power_pressed = config
        .hardware
        .switches
        .power
        .enabled
        .then_some(switches.levels().power);
    log_switch_levels(switches.levels());
    let (mut control, boot_effects) = control::Control::boot(&config, boot_now, power_pressed);
    {
        // The boot effects are applied by the same path as every other tick's,
        // and the facade is told the clock first because its methods take none.
        actuators.set_now(boot_now);
        actuators.set_state(control.state());
        actuators.set_water_tank_full(switches.water_tank_full());
        actuators.set_latched(control.safety_state().latched);
        cc_machine::apply(&mut actuators, &mut side, control.machine(), &boot_effects);
    }

    let mut tick: u32 = 0;
    let mut last_sse_ms: u32 = 0;
    let mut last_heap_log_ms: u32 = 0;
    let mut wifi_last_ms: u32 = 0;
    // When the live parameter snapshot was last published to the HTTP layer.
    let mut last_publish_ms: u32 = 0;
    let mut last_pid_log_ms: u32 = 0;
    let mut last_heartbeat_log_ms: u32 = 0;

    // The boot screens moved to the display task, which owns the panel. See
    // `display_task`'s header: the C++ draws them from its init code because
    // there is one task and one loop, and this firmware no longer has that.

    // When the last frame was handed to the display task.
    let mut last_frame_ms: u32 = 0;
    // The frame's inputs, **carried between frames**.
    //
    // This is not a convenience. `DisplayInput::brew_timer` is the brew-timer
    // FSM's state — `Idle -> Running -> PostBrew -> Idle` — and the transition
    // out of `PostBrew` is a deadline against `post_brew_timer_duration_s`, so a
    // value rebuilt from defaults every 100 ms can never leave `Idle` and the
    // brew timer can never be shown. ADR-0001 §4 says exactly this: the C++
    // advances the state as a side effect of `shouldDisplayBrewTimer` inside
    // `UICoordinator`, and `DisplayInput::brew_timer` is how the port carries it
    // between frames. The display crate was written for it; nothing ever called
    // `step_brew_timer`, and a brew showed no timer at all.
    let mut display_input = cc_display::model::DisplayInput::default();
    // The last sensor fault logged, so a probe that is simply not there says so
    // once instead of fifty times a second.
    let mut sensor_fault_logged: Option<DallasFaultTag> = None;

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
    let mut tick_work_total_ms: u64 = 0;
    let mut tick_period_total_ms: u64 = 0;
    // Ticks since the last report. The means are over **this** window: dividing
    // a window's totals by the cumulative tick count understates them by the
    // number of reports that have gone before, which is how "mean work 8 ms" was
    // printed by a loop whose mean was 3 ms.
    let mut ticks_in_window: u32 = 0;
    let mut last_tick_began_ms: u32 = now_ms();
    loop {
        // Where this tick began, so the time spent in it can be measured. Taken
        // at the top of the loop, immediately after the last tick's sleep, so it
        // excludes the sleep itself — the sleep is the tick's *period*, and
        // including it would report 400 ms every time and say nothing.
        let tick_began_ms = now_ms();
        watchdog.feed()?;
        tick = tick.wrapping_add(1);
        let now = Millis::new(tick_began_ms);

        // ---- 2. the network→control queue, drained at the top of every tick --
        //
        // A command is a *request*: each one becomes a
        // `cc_machine::Event::Command` and is folded by the reducer, so a POST
        // cannot reach past the tick into control state. The reboot and the scale
        // commands are the two that are not reducer events, and they are the two
        // that are genuinely not about the machine's state.
        let mut effects: Vec<cc_machine::Effect> = Vec::new();
        let mut commands_applied: u32 = 0;
        while let Some(command) = commands.recv() {
            info!("control: command {command:?}");
            // Counted, not acked, here: the ack is only honest once the
            // telemetry the caller will read has been published **after** this
            // command was folded in, which happens further down the tick. See
            // the `note_applied()` calls after the publish.
            commands_applied += 1;
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
                // The setpoint. `WebServerManager.cpp:391-408` does three things:
                // set the process setpoint, reset the standby countdown, and
                // **persist** `brewSetpoint`. All three happen here — the first
                // two as reducer events, the third as a store write, because the
                // store is this task's.
                cc_hal_esp32::web::Command::SetSetpoint(celsius) => {
                    let celsius = f64::from(celsius);
                    config.brew.setpoint = celsius;
                    control.set_setpoint(celsius);
                    persist_setpoint(&mut store, celsius, &config);
                    // `requestNormalOperation(systemContext_)` — the C++'s third
                    // line, and the reason a setpoint change also wakes the
                    // machine.
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::NormalOperation),
                        &mut effects,
                    );
                }
                // `POST /api/pid?on=0|1` toggles `Config::pidEnabled` **and** calls
                // `setUserPidEnabled` (`WebServerManager.cpp:472-492`), which
                // persists the preference *and* sets the runtime flag. The web
                // layer here sends the intended value rather than a toggle, so
                // the command is that one `cc_machine::Command::SetUserPidEnabled`.
                cc_hal_esp32::web::Command::SetPid(enabled) => {
                    config.pid.enabled = enabled;
                    persist_pid_enabled(&mut store, enabled, &config);
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::SetUserPidEnabled(enabled)),
                        &mut effects,
                    );
                }
                // The C++'s `POST /api/pid` with no field:
                // `!Config::getInstance().pidEnabled.get()`
                // (`WebServerManager.cpp:466`). The web task sends no value
                // because it cannot know the current one without racing the
                // machine, so the negation happens here, against the machine
                // this task owns.
                cc_hal_esp32::web::Command::TogglePid => {
                    // Negate the **operator's setting**, not the heater's gate.
                    //
                    // The C++ computes `!Config::pidEnabled.get()`
                    // (`WebServerManager.cpp:466`) — the config value, i.e.
                    // what the operator asked for. Negating `mode_enabled`
                    // instead meant the toggle computed its next value from a
                    // flag that `process_control` rewrites every tick, so a
                    // second press in a machine sitting in `PID_DISABLED` would
                    // toggle back to the value it already had and the switch
                    // could not be turned on from off at all.
                    let enabled = !control.machine().pid.runtime_enabled;
                    config.pid.enabled = enabled;
                    persist_pid_enabled(&mut store, enabled, &config);
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::SetUserPidEnabled(enabled)),
                        &mut effects,
                    );
                    // The C++'s other two lines on this handler
                    // (`WebServerManager.cpp:469-470`), omitted here:
                    // `standbyCoordinator().reset()` and
                    // `requestNormalOperation(...)`. Together they mean "asking
                    // for the PID also asks for normal operation", so turning
                    // the PID on **wakes the machine** rather than leaving it in
                    // standby with a live setting nobody can see the effect of.
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::NormalOperation),
                        &mut effects,
                    );
                    info!("config: POST /api/pid toggled pid.enabled -> {enabled}");
                }
                // `!isSteamModeActive()` (`WebServerManager.cpp:444`), plus the
                // C++'s `standbyCoordinator().reset()` and
                // `requestNormalOperation(...)` on the same handler.
                cc_hal_esp32::web::Command::ToggleSteam => {
                    let on = !control.machine().steam_mode;
                    let request = if on {
                        cc_machine::Command::SteamStart
                    } else {
                        cc_machine::Command::SteamStop
                    };
                    control.feed(&config, Event::Command(request), &mut effects);
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::NormalOperation),
                        &mut effects,
                    );
                    info!("config: POST /api/steam toggled steam mode -> {on}");
                }
                // `!systemContext_->backflushMode()`
                // (`WebServerManager.cpp:490`), with the same wake-the-machine
                // pair the steam handler does.
                cc_hal_esp32::web::Command::ToggleBackflush => {
                    let on = !control.machine().backflush.on;
                    let request = if on {
                        cc_machine::Command::BackflushEnter
                    } else {
                        cc_machine::Command::BackflushStop
                    };
                    control.feed(&config, Event::Command(request), &mut effects);
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::NormalOperation),
                        &mut effects,
                    );
                    info!("config: POST /api/backflush toggled backflush mode -> {on}");
                }
                // `POST /api/steam?on=0|1` **toggles** steam mode in the C++
                // (`WebServerManager.cpp:445-446`); the web layer here sends the
                // intended value, so it is mapped to the two requests the reducer
                // understands. A steam *mode* toggle with no steam state is what
                // `SteamRunning`'s entry does; `SteamStart` is the request that
                // gets there.
                cc_hal_esp32::web::Command::SetSteam(on) => {
                    let request = if on {
                        cc_machine::Command::SteamStart
                    } else {
                        cc_machine::Command::SteamStop
                    };
                    control.feed(&config, Event::Command(request), &mut effects);
                }
                // `setBackflushMode(newState)` (`WebServerManager.cpp:502`) —
                // including `currBackflushCycles_ = 1` on the enable arm, which
                // is `apply_backflush_mode`'s job and is already the reducer's
                // (`cc_machine::backflush`).
                cc_hal_esp32::web::Command::SetBackflush(on) => {
                    // The reducer's `BackflushEnter` is the enable arm only; the
                    // disable arm is `BackflushStop`, which the C++ reaches
                    // through the same handler. Mapping `false` to the stop
                    // request is the honest translation: it is what "leave
                    // backflush mode" means to the state machine.
                    let request = if on {
                        cc_machine::Command::BackflushEnter
                    } else {
                        cc_machine::Command::BackflushStop
                    };
                    control.feed(&config, Event::Command(request), &mut effects);
                }
                cc_hal_esp32::web::Command::StartBackflush => {
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::BackflushCycleStart),
                        &mut effects,
                    );
                }
                // `requestStandby(systemContext_)` / `requestNormalOperation(...)`
                // (`WebServerManager.cpp:417-425`).
                cc_hal_esp32::web::Command::Sleep => {
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::Standby),
                        &mut effects,
                    );
                }
                cc_hal_esp32::web::Command::Wake => {
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::NormalOperation),
                        &mut effects,
                    );
                }
                // `maintenanceCoordinator().resetSinceBackflush()`
                // (`WebServerManager.cpp:528-537`). See
                // `Control::reset_shots_since_backflush` for why this one write
                // exists outside the reducer and why it is the only one.
                cc_hal_esp32::web::Command::ResetBackflushCounter => {
                    control.reset_shots_since_backflush();
                }
                // The three that are still inert, listed so the log line says
                // *which* rather than "acknowledged and dropped".
                cc_hal_esp32::web::Command::WifiReset => {
                    warn!("control: POST /api/wifi-reset is not wired into this build (R3-16)");
                }
                cc_hal_esp32::web::Command::FactoryReset => {
                    warn!("control: POST /api/factory-reset is not wired into this build (R3-16)");
                }
            }
        }

        // ---- 2b. `POST /api/parameters` — the C++'s `handleParameters` POST arm
        //
        // (`WebServerManager.cpp:821-878`.) Every parameter is independent and a
        // rejected one does not undo the accepted ones, which is what
        // `cc_config::assign::apply` is. The pairs arrive already validated by
        // the handler, which needed the verdict to answer `200` or `400`
        // synchronously; re-applying them here is the idempotent second half of
        // one rule, through the same `cc_config::assign::parse`.
        //
        // It is here, and not in the handler, because the store is here: it moved
        // into this task in step 7 precisely so that one task owns the
        // configuration, and a `Mutex<ConfigStore>` shared with the httpd task
        // would put a 2 KB blob write on whichever task the web server happened
        // to be serving.
        //
        // **R3-13's inbound MQTT calls `apply` at this line too**, with no
        // handoff: the radio moved into this task, so an MQTT message is
        // delivered here and there is nothing to hand over. One writer, one
        // place, one store write.
        for pairs in parameters.take_all() {
            // What the two cached runtime values were *before* the write, so the
            // push-into-the-machine below can tell whether they actually moved.
            // Read before `apply`, not after.
            let pid_enabled_before = config.pid.enabled;
            let brew_setpoint_before = config.brew.setpoint;
            let applied = cc_config::assign::apply(&mut config, &pairs);
            for (key, err) in &applied.failed {
                // The handler already logged each rejection with the request
                // that caused it. This line is the one that matters if the two
                // verdicts ever disagree, which is the only way a pair can
                // arrive here rejected.
                warn!("config: {key} was not written: {err}");
            }
            if applied.updated == 0 {
                // Nothing moved, but the request was still *received and
                // handled*, and the handler is blocked waiting for exactly that
                // answer. Acking here is what stops a save of a value the
                // machine already holds from timing out.
                parameters.note_applied();
                continue;
            }
            info!(
                "config: {} parameter(s) written: {applied:?}",
                applied.updated
            );
            // A write that leaves the machine unable to run safely is persisted,
            // and the fail-closed rule discards it at the next boot (08 §4.1).
            // Saying so now is the difference between "my setting vanished" and a
            // diagnosis; the C++ has no check on this path and loses it silently.
            if let Err(violation) = cc_safety::validate_config(&control::safety_config(&config)) {
                error!(
                    "config: the stored configuration is now UNSAFE ({violation:?}) and the \
                     next boot will discard it"
                );
            }
            persist_config(&mut store, &config);
            // **The running machine must change too**, not just NVS.
            //
            // This is the defect behind "the UI said success and the PID stayed
            // off". `apply` writes the `Config` value and the value reaches NVS,
            // so it survives a reboot — but several parameters are *also* cached
            // in `cc_machine::Machine`, and nothing copies the new value across.
            // The C++ has no such split because `Config` is a singleton the
            // state machine reads directly on every tick; here the reducer owns
            // its own copy, so a write has to be pushed into it explicitly.
            //
            // `pid.enabled` is the case the human hit: `Machine::pid.mode_enabled`
            // is the flag `should_pid_be_enabled` consults, and it is only ever
            // set by `SetUserPidEnabled` — which until now only
            // `POST /api/pid?on=…` sent. So `POST /api/parameters pid.enabled=1`
            // persisted the preference and did nothing to the machine until a
            // reboot, and the handler answered `200 {"success":true}` throughout.
            if config.pid.enabled != pid_enabled_before {
                control.feed(
                    &config,
                    Event::Command(cc_machine::Command::SetUserPidEnabled(config.pid.enabled)),
                    &mut effects,
                );
                info!(
                    "config: pid.enabled={} pushed into the running machine (was {pid_enabled_before})",
                    config.pid.enabled
                );
            }
            // The setpoint is cached the same way (`Control::set_setpoint`), and
            // `brew.setpoint` is what `effective_setpoint` reads on every tick —
            // so a write to it has to be pushed too, or the display and the PID
            // keep targeting the old temperature.
            if (config.brew.setpoint - brew_setpoint_before).abs() > f64::EPSILON {
                control.set_setpoint(control::effective_setpoint(
                    &config,
                    control.machine().steam_mode,
                ));
                info!(
                    "config: brew.setpoint={} pushed into the running machine (was {brew_setpoint_before})",
                    config.brew.setpoint
                );
            }
            // `standbyCoordinator().reset(); requestNormalOperation(...)` — the
            // C++'s last two lines (`:870-872`), on the same "a POST wakes the
            // machine" rule as `/api/setpoint`.
            control.feed(
                &config,
                Event::Command(cc_machine::Command::NormalOperation),
                &mut effects,
            );
            // **Publish the new values now, not on the next heartbeat.**
            //
            // This is the read-after-write half of "the UI saved it and the UI
            // then read the old value back". The apply above has written the
            // `Config` *and* NVS, so the value is real; but `GET /api/parameters`
            // is answered from `publish_live`, which used to run only on the 1 s
            // heartbeat, so for up to a second after a successful save the API
            // served the previous values. A browser that refetches on save
            // therefore got the old number, put it back into the form, and the
            // toggle appeared to spring back.
            parameters.publish_live(cc_hal_esp32::parameters_json(&config));
            // The ack the `POST` handler is blocked on. See
            // `ParameterHandoff::stage_and_wait`.
            parameters.note_applied();
        }

        // ---- 3. a credential typed on the console ---------------------------
        //
        // This is the one place a `wifi set` becomes durable, and it is here
        // because the store is: the UART task cannot write what it does not own,
        // so it hands the value over and this task picks it up within one control
        // period.
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

        // ---- 4. SENSE ---------------------------------------------------------
        //
        // The probe, the five switches and the pressure sensor, on their own
        // cadences inside one 10 ms period. The period is what changed, and that
        // is the whole of "the screen takes half a second to react": a press is
        // now recognised within one 20 ms debounce window rather than within one
        // 400 ms control period, and the frame reaches the panel from the
        // display task rather than at the end of this same iteration.
        //
        // The probe stays on **this** task on purpose — see [`sensor_task`] for
        // the measurement that says a second task cannot own it on this
        // toolchain.
        temp.poll(now, &mut sensor_fault_logged);
        let last_reading = temp.last_reading();
        let pressure_bar = pressure.as_mut().and_then(|sensor| match sensor.poll(now) {
            Ok(cc_domain::abp2::Poll::Sample(sample)) => Some(f64::from(sample.pressure.raw())),
            Ok(_) => None,
            Err(err) => {
                debug!("control: ABP2 read: {err:?}");
                None
            }
        });
        let tank_full = switches.water_tank_full();
        let edges = switches.poll(now);

        // ---- 5. beat the deadman, on the same signal as the watchdog feed ----
        //
        // One thing that is alive, one signal, rather than two that could
        // disagree. The gate is *only* opened by this line, and only the control
        // task holds a `&mut HeaterGate`, so "the supervisor is running" is a
        // precondition of heating rather than a hope about task ordering
        // (08 §3's "output held off until the supervisor beats").
        actuators.gate().heartbeat(now);

        // The facade is told the clock and the two facts the interlocks are a
        // function of, **before** the effects are applied, so a duty the reducer
        // emits in this very tick is judged against this tick's state.
        actuators.set_now(now);
        actuators.set_water_tank_full(tank_full);
        actuators.set_state(control.state());
        actuators.set_latched(control.safety_state().latched);

        // ---- 6 + 7. DECIDE, then ACT ------------------------------------------
        //
        // The sample the reducer sees. `has_temperature_error` is the C++'s
        // `hasTemperatureSensorError()` and is `true` until the first *plausible*
        // reading, which is what keeps S1 from tripping on the boot-time zero and
        // is also what sends the machine to `SENSOR_ERROR` rather than pretending
        // a boiler is at 25 °C.
        let sensors = cc_machine::Sensors {
            // 🔴 The reading is `Celsius::new(0.0)` until the first conversion
            // completes, and `has_temperature_error` is `true` for exactly that
            // window. **That pairing is deliberate and it is what keeps the
            // machine safe at boot:** `has_sensor_error()` sends the reducer to
            // `SENSOR_ERROR` (`BaseState.h:145-148`), whose
            // `should_pid_be_enabled` is `false`, so the duty is zeroed before a
            // boiler that has not been measured can be asked for full power. The
            // C++ gets this for free from `SensorCoordinator`'s own error flag
            // being false and its cached temperature being 0.0 — which means the
            // C++'s first PID compute sees a 0 °C input against a 95 °C setpoint.
            // The emergency threshold (150 °C default) does not catch that,
            // because 0 °C is a *plausible* temperature; only the sensor-error
            // path does. **The C++'s behaviour here is not reproduced**, and this
            // is a deliberate divergence rather than a bug fix: see the boot-window
            // note in `docs/rust-migration/intentional-diffs.md` (#12).
            temperature: last_reading.map_or(Celsius::new(0.0), |(celsius, _)| {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "a DS18B20 reading is a multiple of its own \
                              0.0625 C resolution, so f32 holds it exactly"
                )]
                Celsius::new(celsius as f32)
            }),
            water_tank_full: tank_full,
            has_temperature_error: last_reading.is_none_or(|(_, plausible)| !plausible),
            // The scale is not part of `Sensors::has_sensor_error`'s contract
            // here: `has_scale_error` is a separate field and the C++'s
            // `hasSensorError()` ORs the two (`SensorCoordinator.h:190-192`).
            // A machine with no scale has no scale error, so this is `false`.
            has_scale_error: sampler.as_ref().is_some_and(|s| s.telemetry().faulted()),
            brew_weight: 0.0,
            // The probe's own conversion counter, so S1's debounce counts
            // readings rather than ticks. See `Sensors::sample_seq`.
            sample_seq: temp.sample_seq(),
        };

        let mut tick_effects = control.tick(&config, sensors, &edges, now);
        // The queue's effects were already folded above; `Control::tick` starts
        // its own vector, so the two are concatenated in the C++'s order —
        // commands first (step 3 of the loop), then the tick's own.
        effects.append(&mut tick_effects);
        cc_machine::apply(&mut actuators, &mut side, control.machine(), &effects);

        // Publish the values this task is actually running with, so
        // `GET /api/parameters` does not answer from the boot snapshot.
        //
        // Once per heartbeat, not once per tick: this copies 98 values onto the
        // heap, and at 2.5 ticks a second that is 245 copies a second for a
        // number an operator looks at once a second. The heartbeat is the same
        // 1 s cadence `/api/status` publishes on.
        if now_ms().wrapping_sub(last_publish_ms) >= HEARTBEAT_MS {
            last_publish_ms = now_ms();
            parameters.publish_live(cc_hal_esp32::parameters_json(&config));
        }

        // The scale's events, drained every tick, and the weight. See
        // `drain_scale`: the event drain is the only place a tare can be
        // persisted, because this task is the only holder of the store.
        let weight_g = drain_scale(sampler.as_ref(), &mut store);

        // ---- 8. SHOW ----------------------------------------------------------
        //
        // **Published, not drawn.** The panel belongs to the display task
        // (`display_task`), and this is the hand-off: one `DisplayInput`, the
        // display's own view of the configuration, and the blank decision, on
        // the panel's own cadence rather than the control task's.
        //
        // It is published **after** the effects are applied, so the frame shows
        // the state the machine is actually in rather than the one it was about
        // to enter — drawing before the applier would put a "brewing" screen up
        // one tick before the pump started.
        //
        // The panel's 100 ms interval is the floor on how fast a screen change
        // becomes visible, which is the C++'s floor too
        // (`DISPLAY_REFRESH_INTERVAL_MS`). Publishing more often than that would
        // be a `DisplayInput` copied 100 times a second for a frame that is
        // dropped 90 of them.
        if tick_began_ms.wrapping_sub(last_frame_ms) >= FRAME_PUBLISH_MS {
            last_frame_ms = tick_began_ms;
            // The **gains**, not the last P/I/D terms — see `Control::pid_gains`
            // for why, and for the `4444|81|0` this replaces.
            let (p, i, d) = control.pid_gains();
            let machine = *control.machine();
            // The display's view of the configuration, refreshed every frame:
            // fifteen of its flags change at runtime through
            // `POST /api/parameters`, and a copy taken at boot would show the
            // machine the screen it had when it booted.
            let display_view = display_config(&config);
            // The fields that change. Everything else in `display_input` is kept,
            // and the kept part is what carries the brew-timer FSM.
            display_input.temperature = last_reading.map_or(0.0, |(celsius, _)| celsius);
            display_input.setpoint = control.setpoint();
            display_input.pid_output = f64::from(control.pid_output());
            display_input.pid_kp = p;
            display_input.pid_ki = i;
            display_input.pid_kd = d;
            display_input.state = control.state();
            // The wall clock, for the post-brew deadline.
            //
            // `step_brew_timer` compares `now_ms` against the moment the brew ended
            // to decide when the post-brew screen goes away
            // (`DisplayBrewTimerState.h`, the C++'s `millis() -
            // ui.getBrewTimerEndTime()`). Without this field the comparison is
            // `0 - 0`, which never exceeds the duration, and **the post-brew screen
            // never goes back** — the third "the display does not return" bug of the
            // day, and the third one with the same cause: a field `DisplayInput`
            // expects that the firmware never filled in.
            display_input.now_ms = tick_began_ms;
            display_input.state = control.state();
            // The brew row's two numbers, `processCurrentBrewTime()` and
            // `processTotalTargetBrewTime()` — `BrewProgress` is exactly those.
            display_input.brew_time_ms = machine.brew.elapsed_ms;
            display_input.target_brew_time_ms = machine.brew.target_ms;
            // `BrewHandler::isBrewActive()` — the C++'s own definition, which
            // `shouldDisplayBrewTimer` uses to leave `Idle`. `BrewFinished` is
            // excluded because the FSM has already moved on to `PostBrew` by
            // then, and it is the state in which the post-brew timer runs.
            display_input.brew_active = machine.state.is_brew_state()
                && machine.state != cc_domain::state::MachineState::BrewFinished;
            // The post-brew deadline is configuration, and configuration can
            // change under a running machine, so the view is refreshed here
            // rather than captured at boot.
            // The FSM step. One call per published frame, which is what the C++
            // gets from one `printScreen()` per loop.
            let _ = cc_display::templates::step_brew_timer(&mut display_input, &display_view);
            frame.publish(slots::FrameRequest {
                input: display_input,
                blank: machine.standby.should_turn_off_display(),
                config: display_view,
            });
        }

        // A reboot request from the HTTP layer, honoured here and not in the
        // handler. A handler that called `esp_restart` directly could reset the
        // machine from inside a request; this is between ticks, after the
        // watchdog has been fed.
        if net.shared.take_reboot_request() {
            info!("control: reboot requested — restarting");
            // **Shut the hardware down first.** The power-switch branch below
            // does exactly this and says why; this one did not, so during the
            // 500 ms pause below the loop was not running: no heartbeat, no
            // watchdog feed, no interlock — while the heater ISR kept chopping at
            // the last commanded duty. Half a second of full-power heating with
            // the safety paths switched off is not a reboot, it is a hazard.
            let machine = *control.machine();
            cc_machine::apply_one(
                &mut actuators,
                &mut side,
                &machine,
                cc_machine::Effect::SafeHardwareShutdown,
            );
            // A 500 ms pause so the HTTP response has left the socket and the
            // `202 Accepted` has reached the operator's browser, rather than the
            // connection being cut mid-write. The C++ does the same
            // (`WebServerManager.cpp:696`, `delay(1000)` before its restart).
            FreeRtos::delay_ms(500);
            restart_now();
        }

        // A reboot the *reducer* asked for — the power switch's long press, which
        // is `PowerHandler::triggerSystemReboot` (`PowerHandler.h:177-192`) and
        // the only path to `ESP.restart()` the C++ has that does not come from
        // HTTP. `FirmwareSide` records it rather than restarting inside the
        // applier, because `Effect::RequestReboot` sits in the middle of this
        // tick's effect list and restarting there would abandon the effects
        // after it — including a `CloseWaterValve`.
        if side.take_reboot_request() {
            info!("control: the state machine asked for a reboot (power switch long press)");
            // `PowerHandler::triggerSystemReboot`'s own order: shut the hardware
            // down safely, then restart. The safe shutdown is a real effect
            // through the real applier, so a valve left open by a state that
            // forgot to close it is closed before the chip resets.
            let machine = *control.machine();
            cc_machine::apply_one(
                &mut actuators,
                &mut side,
                &machine,
                cc_machine::Effect::SafeHardwareShutdown,
            );
            FreeRtos::delay_ms(REBOOT_DISPLAY_MS);
            restart_now();
        }

        // ---- 8. NOTIFY ------------------------------------------------------
        //
        // The telemetry publish and the SSE broadcast, at the C++'s cadence
        // (`WebServerManager.cpp:1128-1143` driven from
        // `LoopManager::updateWebsite`, gated on `tempEventInterval_`).
        let uptime = now_ms();
        let state = control.state();
        let machine = *control.machine();
        net.shared.publish(network::telemetry_from(
            network::Reading {
                state: state as i32,
                temperature_c: last_reading.map_or(f64::NAN, |(celsius, _)| celsius),
                setpoint_c: control.setpoint(),
                heater_power_pct: f64::from(control.pid_output()) / 10.0,
                // `runtime_enabled`, **not** `mode_enabled` — and the difference
                // is the whole bug the human reported.
                //
                // They are two different facts:
                //
                //   * `runtime_enabled` — `SystemContext::isProcessPidEnabled()`,
                //     the **operator's setting**. This is what `POST /api/pid`
                //     toggles (via `setUserPidEnabled`, which is exactly what
                //     the C++ calls at `WebServerManager.cpp:468`) and what is
                //     persisted in `config.pid.enabled`.
                //   * `mode_enabled` — `ProcessController::isPIDEnabled()`, the
                //     heater's **derived gate**, recomputed every tick by
                //     `should_pid_be_enabled` from the machine state, and forced
                //     to `false` whenever the PID is not *permitted* right now
                //     (sensor error, empty tank, standby, a brew in progress).
                //
                // Reporting the gate made `POST /api/pid` answer
                // `{"success":true,"pidEnabled":true}` and the very next
                // `/api/status` report `false`, because in `PID_DISABLED` the gate
                // is false by definition and `process_control` drives it straight
                // back. The UI showed a switch that turned itself off.
                //
                // `runtime_enabled` is both the faithful answer (the C++'s
                // `/api/pid` reads `!Config::pidEnabled`, the same operator's
                // setting) and the only one a switch can be bound to. The gate
                // is still observable, and it is what `heater_power_pct` and the
                // state field are for.
                pid_enabled: machine.pid.runtime_enabled,
                // The two toggle inputs. `POST /api/steam` and
                // `POST /api/backflush` are toggles in the C++ and compute
                // `!current` from live machine state, which is only reachable
                // from this task — so the httpd task gets the current value
                // through the telemetry snapshot rather than by reaching into
                // the machine.
                steam_mode: machine.steam_mode,
                backflush_mode: machine.backflush.on,
                brewing: state.is_brew_state()
                    && state != cc_domain::state::MachineState::BrewFinished,
                standby: state == cc_domain::state::MachineState::Standby,
                standby_remaining_ms: machine.standby.remaining_ms,
                // `currBackflushCycles_` starts at 1 and only ever counts up, so
                // a negative value is unreachable; the cast is a formality that
                // documents it. `shots_since_backflush` is `i32` because that is
                // what `MachineStateContext.h:788` declares and what the reducer
                // keeps.
                #[allow(
                    clippy::cast_sign_loss,
                    reason = "the counter is `i32` because \
                              MachineStateContext.h:788 declares it that way, \
                              but it starts at 0 and is only ever incremented \
                              while brewing and reset to 0 on entering \
                              BACKFLUSH_FINISHED, so it is never negative"
                )]
                shots_since_backflush: machine.shots_since_backflush.max(0) as u32,
                // `isReminderDueForCount(shots, enabled, threshold)`
                // (`MaintenanceCoordinator.cpp:68-73`) — the count has to reach
                // the threshold *and* the reminder has to be enabled. Both
                // halves come from the control task's own `Config`, because
                // that is the only place either is readable.
                backflush_threshold: u32::try_from(config.maintenance.backflush_reminder.threshold)
                    .unwrap_or(0),
                backflush_due: config.maintenance.backflush_reminder.enabled
                    && machine.shots_since_backflush
                        >= config.maintenance.backflush_reminder.threshold,
                water_tank_full: config
                    .hardware
                    .sensors
                    .watertank
                    .enabled
                    .then_some(tank_full),
                pressure_bar,
                mqtt_configured,
                mqtt_connected,
            },
            uptime,
            weight_g,
        ));

        // **Acknowledge the commands drained this tick, now that the telemetry
        // the caller reads is published.**
        //
        // The order is the whole point. A `POST /api/pid` is blocked in
        // `Shared::wait_applied`, and when it wakes it re-reads the snapshot to
        // answer "what is the PID now". Acknowledging before the publish would
        // wake it against the *previous* tick's snapshot, which is how the
        // first version answered `true` for a toggle that turned the PID off.
        for _ in 0..commands_applied {
            net.shared.note_applied();
        }

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
            // `tempHistory.addPoint` (`WebServerManager.cpp:1134`), on the same
            // call and at the same cadence as the SSE broadcast — in the C++ both
            // happen inside `sendTempEvent`. The ring drops two of every three
            // samples itself, so this is a sample per second and a point per
            // three, which is the spacing the UI's x axis assumes.
            //
            // The `f32` narrowing is the C++'s: its `HistoryPoint` members are
            // `float` (`WebServerManager.cpp:113`). A DS18B20 reading is a
            // multiple of its own 0.0625 C resolution and the setpoint is
            // schema-bounded to 0..=150 C, so both are exact in an `f32`.
            let (current_c, target_c) = (
                last_reading.map_or(f64::NAN, |(celsius, _)| celsius),
                control.setpoint(),
            );
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a temperature in -273.15..=150 C is exact in f32"
            )]
            let (current_c, target_c) = (current_c as f32, target_c as f32);
            net.shared.push_history(
                current_c,
                target_c,
                // The C++'s own promill-to-per cent conversion. The PID output
                // is 0..=1000 by construction (`WINDOW_MS`), so this is exact.
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "the PID output is bounded to 0..=1000 by WINDOW_MS"
                )]
                {
                    f64::from(control.pid_output()) as f32 / 10.0
                },
            );
            network::broadcast_temps(&net);
        }
        if uptime.wrapping_sub(last_heap_log_ms) >= HEAP_LOG_INTERVAL_MS {
            last_heap_log_ms = uptime;
            network::log_heap_once_a_minute(&net);
            // The panel's counters, on the same cadence and for the same reason:
            // a display that has stopped updating is invisible from the outside
            // except by looking at the machine, and `frames=` not advancing is
            // the one number that says so without anyone having to notice.
        }

        // The heartbeat line, and the PID's own numbers beside them.
        //
        // **Once a second, not once a tick.** At 100 Hz this line was being
        // written 100 times a second, and the console is a 115200-baud UART: one
        // line of this length is ~13 ms on the wire, so the *logging* was most of
        // the tick's work and the loop ran at 45 ms instead of 10. The heartbeat
        // itself is 10 ms and unconditional — it is the deadman and the watchdog
        // feed, both of which are above this line. The *line* is a diagnostic, and
        // a diagnostic that costs the loop a third of its budget is not one.
        //
        // 🔴 The `duty` and `requested` figures here are the **PID's output**,
        // and `on_ticks`/`on` are the ISR's own counters — the only view of the
        // heater pin that exists, because the pin belongs to the ISR (see
        // `cc_hal_esp32::heater::TimerIsrPwm`). A non-zero `duty` with a zero `on`
        // is therefore a *readback failure*, not a heater that is not working, and
        // a non-zero `on` with a zero `duty` is impossible: the duty is the only
        // thing that moves the pin.
        let heater = actuators.heater();
        if tick_began_ms.wrapping_sub(last_heartbeat_log_ms) >= HEARTBEAT_LOG_INTERVAL_MS {
            last_heartbeat_log_ms = tick_began_ms;
            info!(
                "control heartbeat {tick} — watchdog fed, state {:?}, duty {:.0} ms, \
             applied {}, gate {}, ISR ticks {}, on {} ({:.3})",
                state,
                control.pid_output(),
                heater.applied_duty(),
                heater.blocked_at(now).is_none(),
                heater.transport().ticks(),
                heater.transport().on_ticks(),
                heater.transport().measured_on_fraction(),
            );
        }

        // The PID's own P/I/D, once a second. This is the line that answers
        // "is the controller doing something sensible", and it is why
        // `cc_domain::Controller` kept the Arduino library's three getters
        // (`PID_v1.h:279-292`) that nothing in the firmware needed until now.
        if tick_began_ms.wrapping_sub(last_pid_log_ms) >= PID_LOG_INTERVAL_MS {
            last_pid_log_ms = tick_began_ms;
            let (p, i, d) = control.pid_terms();
            let refusals = actuators.refusals();
            let (pump_active, valve_active) = actuators.pins_read_active();
            // `PID_v1.h:127` defines the error as `setpoint - input`, so the
            // log prints that, not its negation: a cold boiler at a 30 °C
            // setpoint must read `error=+6.4 K`, which is the sign the P term
            // was computed from.
            let measured_c = f64::from(sensors.temperature.raw());
            let error_k = control.setpoint() - measured_c;
            info!(
                "control: T={:.2} C  setpoint={:.2} C  error={:.2} K  \
                 duty={:.1} ms ({:.1} %)  P={p:.1} I={i:.1} D={d:.1}  \
                 tank_full={tank_full}  valve={:?}  \
                 refused pump={} water={} steam={} heater={}  \
                 pins pump={pump_active} valve={valve_active}",
                measured_c,
                control.setpoint(),
                error_k,
                f64::from(control.pid_output()),
                f64::from(control.pid_output()) / 10.0,
                actuators.valve_state(),
                refusals.pump,
                refusals.water_valve,
                refusals.steam_valve,
                refusals.heater,
            );
        }

        // 🔴 The tick's own cost, measured **before** the sleep.
        //
        // A first revision of this took the timestamp *after*
        // the block and so reported ~431 ms — the sleep
        // itself, which is the tick's *period* and not its work. The tick budget
        // is about what the work costs; including the sleep makes every tick
        // look like a 40x overrun and the number says nothing. Measured on
        // hardware and fixed here, which is the only reason it is worth writing
        // down: a timing instrument that has never disagreed with a result is
        // not known to be working.
        let tick_elapsed_ms = now_ms().wrapping_sub(tick_began_ms);
        if tick_elapsed_ms > tick_worst_ms {
            tick_worst_ms = tick_elapsed_ms;
        }
        // The mean, and the *achieved* period, because the two answer different
        // questions and only having the worst tick is how a loop that runs at
        // 17 ms while claiming 10 ms stays invisible: `worst` says a tick overran
        // its budget, the mean says the loop is not the rate it claims to be.
        tick_work_total_ms = tick_work_total_ms.saturating_add(u64::from(tick_elapsed_ms));
        ticks_in_window = ticks_in_window.saturating_add(1);
        tick_period_total_ms = tick_period_total_ms
            .saturating_add(u64::from(tick_began_ms.wrapping_sub(last_tick_began_ms)));
        // The cursor for the next tick's period. Without this line every tick
        // measured the time since *boot* rather than since the previous tick, and
        // the reported mean period was the sum divided by the tick count — which
        // is why an instrument that had been printing 41 seconds as a "13 ms"
        // mean looked plausible for several builds.
        last_tick_began_ms = tick_began_ms;
        if tick <= TICK_BASELINE_TICKS {
            if tick_elapsed_ms > baseline_worst_ms {
                baseline_worst_ms = tick_elapsed_ms;
            }
        } else if tick_elapsed_ms > TICK_BUDGET_MS {
            tick_over_budget += 1;
        }

        if tick_began_ms.wrapping_sub(last_tick_report_ms) >= TICK_REPORT_INTERVAL_MS {
            last_tick_report_ms = tick_began_ms;
            let ticks = u64::from(ticks_in_window.max(1));
            // The per-window figures. Read before the reset, or the line reports
            // the window that just ended as zeroes.
            let (work_mean_ms, period_mean_ms) = (
                u32::try_from(tick_work_total_ms / ticks).unwrap_or(0),
                u32::try_from(tick_period_total_ms / ticks).unwrap_or(0),
            );
            tick_work_total_ms = 0;
            tick_period_total_ms = 0;
            ticks_in_window = 0;
            info!(
                "control tick: worst {tick_worst_ms} ms of the last {tick} \
                 (baseline {baseline_worst_ms} ms over the first \
                 {TICK_BASELINE_TICKS}, budget {TICK_BUDGET_MS} ms, \
                 {tick_over_budget} over budget) — mean work {} ms, \
                 achieved period {} ms of a {} ms target — scale: {}{}",
                work_mean_ms,
                period_mean_ms,
                CONTROL_PERIOD_MS,
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

        // ---- 9. the tick's period: sleep the rest of it ---------------------
        //
        // The control period is 04 §2's 100 Hz. The sleep is the remainder of it
        // rather than a fixed 400 ms, so a slow tick shortens the next one
        // instead of drifting — the loop holds its *rate*, not its rhythm.
        //
        // **A blocking wait on a wake channel was tried here and is not in the
        // build.** `SignalQueue`, a `std::sync::Mutex` and an ESP-IDF task
        // notification were each tried as the way for a producer to shorten this
        // sleep, and each one asserts inside the FreeRTOS kernel on this build
        // (`xTaskRemoveFromEventList`, `pxUnblockedTCB` NULL). The bisect table
        // is in `sensor_task.rs`; the short version is that every cross-task
        // *blocking* primitive reachable from a Rust task trips it, while a
        // non-blocking `CommandQueue::try_send` never has. So the loop runs on
        // its deadline and consumes every event — a switch edge, a `POST
        // /api/parameters`, a reboot request, a staged credential — at the top
        // of the next period, which at 10 ms is not a latency anyone can feel.
        // `saturating_sub` on the *elapsed* time, not `wrapping_sub` on the
        // deadline. A tick that overran its period — the first one always does,
        // at 77 ms against a 10 ms budget — would make
        // `next_deadline - now` wrap to about 2^32 ms, and
        // `delay_ms(4_294_967_295)` is a forty-nine-day sleep. The machine would
        // stop answering: it happened, and it is the reason this line is
        // written as elapsed-time-from-a-signed-comparison rather than as the
        // more obvious deadline arithmetic.
        let elapsed = now_ms().wrapping_sub(tick_began_ms);
        FreeRtos::delay_ms(CONTROL_PERIOD_MS.saturating_sub(elapsed));

        // Arm the chopper once a beat has been taken. Doing it *after* the first
        // `set_duty` means the first duty the ISR ever sees is one that went
        // through the gate. `tick == 1` rather than a flag, so there is exactly
        // one place that can arm it and it is obviously after the first beat.
        if tick == 1 {
            actuators.arm_heater_isr();
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

/// The `cc_display` template for a configured [`DisplayTemplate`].
///
/// `DisplayTemplate` is the config enum with the C++'s numeric values
/// (`Config.h:1144` registers them as an `EnumParamDef`); `TemplateId` is the
/// display crate's own enum. They are the same six layouts in the same order,
/// but they are two crates' types and nothing guarantees they stay in step, so
/// the mapping is written out rather than cast. An out-of-range value falls
/// back to `Standard`, which is the C++'s default and a layout that always
/// exists.
fn template_for(
    configured: cc_domain::system::DisplayTemplate,
) -> cc_display::templates::TemplateId {
    use cc_display::templates::TemplateId;
    use cc_domain::system::DisplayTemplate;
    match configured {
        DisplayTemplate::Standard => TemplateId::Standard,
        DisplayTemplate::Minimal => TemplateId::Minimal,
        DisplayTemplate::TemperatureOnly => TemplateId::TemperatureOnly,
        DisplayTemplate::Scale => TemplateId::Scale,
        DisplayTemplate::Upright => TemplateId::Upright,
        DisplayTemplate::Modern => TemplateId::Modern,
    }
}

/// The `cc_display` view of the machine's configuration.
///
/// `cc_display::model::Config` is deliberately a *separate* type from
/// `cc_config::Config`: the display crate is a pure renderer with 48 goldens and
/// a pixel-parity oracle against real U8g2, and it has no business knowing about
/// Wi-Fi credentials or the NVS blob. The consequence is that this mapping
/// exists, and it is the seam where a new display-relevant parameter has to be
/// wired up by hand — which is the point.
///
/// **Every field is listed, with no `..Default::default()`.** That is what makes
/// the seam load-bearing: a field added to the display crate's `Config` is a
/// compile error here until someone decides what the machine's setting means for
/// it, rather than a template that silently keeps using the default. The goldens
/// can afford `..default()` because they are fixtures; this is not a fixture.
fn display_config(config: &cc_config::Config) -> cc_display::model::Config {
    use cc_display::model::{BrewMode, Config as DisplayConfig, Language, ScaleType};
    use cc_domain::system::DisplayTemplate;

    DisplayConfig {
        brew_switch_enabled: config.hardware.switches.brew.enabled,
        scale_enabled: config.hardware.sensors.scale.enabled,
        // The display crate's `ScaleType` is a two-way question (wired load
        // cell or radio) where the config's is three (one cell, two cells, or
        // radio). Single and dual are the same *layout* to a template, so they
        // collapse here; which one it is belongs to the driver, not the screen.
        scale_type: match config.hardware.sensors.scale.r#type {
            cc_domain::hardware::ScaleType::Bluetooth => ScaleType::Bluetooth,
            cc_domain::hardware::ScaleType::Hx711Dual
            | cc_domain::hardware::ScaleType::Hx711Single => ScaleType::Hx711,
        },
        pressure_enabled: config.hardware.sensors.pressure.enabled,
        oled_enabled: config.hardware.oled.enabled,
        upright_template: config.display.template == DisplayTemplate::Upright,
        inverted: config.display.inverted,
        language: match config.display.language {
            cc_domain::system::Language::English => Language::English,
            cc_domain::system::Language::German => Language::German,
            cc_domain::system::Language::Spanish => Language::Spanish,
        },
        heating_logo: u8::from(config.display.heating_logo),
        pid_off_logo: u8::from(config.display.pid_off_logo),
        fullscreen_brew_timer: config.display.fullscreen_brew_timer,
        fullscreen_manual_flush_timer: config.display.fullscreen_manual_flush_timer,
        fullscreen_hot_water_timer: config.display.fullscreen_hot_water_timer,
        post_brew_timer_duration_s: config.display.post_brew_timer_duration,
        blinking_delta: config.display.blinking.delta,
        backflush_reminder_enabled: config.maintenance.backflush_reminder.enabled,
        brew_mode: match config.brew.mode {
            cc_domain::process::BrewMode::Manual => BrewMode::Manual,
            cc_domain::process::BrewMode::Automatic => BrewMode::Automatic,
        },
        brew_by_time_enabled: config.brew.by_time.enabled,
        brew_by_weight_enabled: config.brew.by_weight.enabled,
        brew_by_weight_target: config.brew.by_weight.target_weight,
        mqtt_enabled: config.mqtt.enabled,
        // `cycles` is an i32 in the config and a u8 on screen. `unwrap_or` rather
        // than a cast: the C++ stores it as a float param and a machine
        // configured with a nonsense value should render the default layout
        // detail, not wrap to 255 cycles.
        backflush_cycles: u8::try_from(config.backflush.cycles).unwrap_or(5),
    }
}
