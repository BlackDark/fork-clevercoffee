//! Clever Coffee firmware — R1-01 workspace bring-up, plus the R1-07 heater
//! output.
//!
//! # What this binary is (and is not)
//!
//! This is the **R1-01 feasibility spike** extended by **R1-07**, not the
//! firmware. Its jobs are to prove that the toolchain builds, links, boots and
//! runs on this host for `xtensa-esp32-espidf`, to make the first image-size
//! measurement (07 §5), and — from R1-07 — to bring up the heater output behind a
//! deadman gate that no code in this binary can open.
//!
//! What it does, in order:
//!
//! 1. Initialise ESP-IDF and route `log` to UART0 at 115200 baud — the same
//!    stream and baud rate the C++ firmware uses.
//! 2. Configure the pump and water-valve pins — GPIO17 and GPIO27
//!    (`include/clevercoffee/hardware/pinmapping.h:39-40`) — as outputs and
//!    drive them **inactive**.
//! 3. **Drive GPIO2 — the heater — inactive and read it back as a pin.** The
//!    heater is chopped by a 10 ms `GPTimer` ISR ([`cc_hal_esp32::heater::TimerIsrPwm`]),
//!    which is what the C++ did (`isr.h:85-118`); the ISR is built disarmed, so
//!    the pin does not move until the deadman gate is beaten.
//! 4. **Read every actuator pin back and assert it is inactive.** This is the
//!    startup assertion of 04 §4 / R3-16 in its smallest possible form, done
//!    before anything else exists so a failure is unambiguous. For the heater this
//!    happens at the one moment it can be a pin readback at all: the ISR takes the
//!    pin immediately afterwards.
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
//! and `docs/history/divergences.md` #5.
//!
//! There is deliberately **no** control loop, no state machine and no sensor
//! here. Those arrive at R2-08 and R3-xx.

mod config_io;
mod control;
mod display_task;
mod mqtt_link;
mod network;
mod probe;
/// Why there is no sensor task: a measured kernel defect, not an oversight.
mod sensor_task;
mod slots;

use core::error::Error;
use std::sync::Arc;

use cc_domain::units::{Celsius, Millis};
use cc_hal_esp32::heater::{HeaterOutput, TimerIsrPwm};
use cc_hal_esp32::time::now_ms;
use cc_hal_esp32::SwitchBank;
// `Telemetry` is `cc_web`'s, named from its one owner since finding 4.5 deleted
// the second copy in `network.rs`. `parameters_json` is here for the same
// reason: the control task publishes the `/api/parameters` body it would serve.
use cc_web::Telemetry;

// The board's pin map: one copy, in `cc_hal_esp32::pins`, checked for legality
// at compile time and checked against the wiring below at bring-up. Every
// `GPIO17` in this file is the field `Peripherals` hands out; every pin number
// *printed* comes from here, so the log cannot describe a machine that is not
// the one that booted.
use cc_hal_esp32::pins;
// The probe's driver, enum and bring-up are in `probe`; this file keeps the pin
// that goes in, because the wiring is what `pins::assert_wiring` is checked
// against.
use crate::probe::{bring_up_temperature_sensor, DallasFaultTag, TemperatureSensor};
// The configuration writers are in `config_io`. They are called from the control
// tick below and, for `push_into_machine`, from `mqtt_link` as well.
use crate::config_io::{
    drain_scale, persist_config, persist_pid_enabled, persist_setpoint, push_into_machine,
};
use cc_machine::Event;
// `FirmwareSide` implements this; it is imported so the control task can call
// `on_reset_shots_since_backflush` for the operator's HTTP reset rather than
// reaching past the applier for a second way to clear the counter.
use cc_machine::MachineChannels;
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

/// How often the *published parameter snapshot* is rebuilt, in milliseconds.
///
/// A SEPARATE constant, and it has to be one.
///
/// The `parameters_json()` publish used to be gated on [`HEARTBEAT_MS`], which
/// is the control tick period -- 10 ms. The comment above that gate still
/// explained the intent as "at 2.5 ticks a second that is 245 copies a second",
/// which was true when the tick was 400 ms; commit 158f61b5 collapsed the tick
/// onto the heartbeat and the gate with it. The effect, measured on the host
/// against the real `SCHEMA`: **420 heap allocations and 20.5 KB per call, at
/// 100 calls a second** -- about 42,000 allocations/s and 2 MB/s of allocator
/// churn driven by the control task, next to the heater deadman. Fixed by
// `a2f597c2`.
///
/// It could not simply be `HEARTBEAT_MS = 1000`:
/// `const _: () = assert!(HEARTBEAT_MS * 2 <= DEADMAN_TIMEOUT_MS)` above would
/// become `2000 <= 1000` and stop the firmware compiling, because
/// `HEARTBEAT_MS` also *is* the deadman beat. Two different cadences, two
/// constants, each named for what it paces.
///
/// 1 s is the cadence the C++'s `/api/status` and the UI poll on, and the age of
/// a parameter snapshot older than that is not observable by anyone.
const PARAMETERS_PUBLISH_MS: u32 = 1_000;

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

// The original ESP32's `ledc_ll_set_duty_start` spins inside
// `portENTER_CRITICAL` for up to one carrier period, and no carrier that is slow
// enough for the contactor is fast enough for the 300 ms interrupt watchdog. See
// [`HEATER_LEDC_DEFECT`] and 09 §17.
//
// This used to be enforced by a `const BRING_UP_HEATER_LEDC: bool = false` plus
// a `const _: () = assert!(!BRING_UP_HEATER_LEDC, …)`, so the finding could not
// be reversed by flipping a `const`. That guard is gone because the thing it
// guarded is: there is no `LEDC` transport left in `cc-hal-esp32` to bring up,
// so setting the flag to `true` would now compile and do nothing. The finding
// itself is in [`HEATER_LEDC_DEFECT`], and the carrier arithmetic it rests on is
// in `cc_hal_esp32::heater`'s module docs.

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
/// higher duties: the LEDC transport's constructor wrote duty 0, and
/// `duty_start` self-clears at the next period regardless of the duty value, so
/// the very first write panicked.
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
/// There **is** no LEDC transport in `cc-hal-esp32` any more. `LedcPwm` existed
/// behind a one-method `HeaterDuty` seam for a target whose chip does not have
/// the spin, and it was deleted: zero construction sites, never brought up, and
/// the spin is unique to the original ESP32 — every other `ledc_ll.h` in this
/// tree (`esp32c2`, `esp32c3`, `esp32c5`, and the s3/h2/p4 equivalents) has the
/// loop removed. That is a target that does not exist yet, so the transport is
/// written when there is one; the carrier arithmetic and the two constants it
/// would use are in `cc_hal_esp32::heater`'s module docs and are asserted against
/// `cc_domain`'s host-tested values on every build, so they cannot rot.
const HEATER_LEDC_DEFECT: &str =
    "R1-07's 1 Hz LEDC carrier spins in ESP-IDF's ledc_ll_set_duty_start for up \
      to one period with interrupts masked, which exceeds the ESP32's 300 ms \
      interrupt watchdog. The spin is unique to this chip -- every other \
      ledc_ll.h in the tree has it removed -- so there is no carrier that is both \
      slow enough for the contactor and fast enough for the watchdog. The heater \
      is driven by the C++'s own 10 ms GPTimer ISR instead. The LEDC transport is \
      not written: a target whose chip lacks the spin gets one, and the carrier \
      arithmetic survives in cc-hal-esp32::heater's module docs. See \
      09-cpp-findings.md section 17.";

/// Stack size of the control task, from the priority table in 04 §2.
///
/// **16 KB, and the measured reason is in `network::apply_staged`.** This was 8 KB, and the 8 KB was
/// one `wifi apply` away from fatal: the credential the console stages is
/// written by `store.load()` + `store.save()` **on this task**, inside the
/// tick, and that pair — a ~2 KB JSON document parsed and re-serialised, with a
/// whole `Config` live on the stack three frames deep — needs about 8.5 KB of
/// stack of its own.
///
/// Measured on the device with `uxTaskGetStackHighWaterMark` at the top of the
/// tick that takes the staged credential:
///
/// * at 8 KB: **68 bytes** of the stack had never been used, and the task died
///   inside `store.save()` — a stack overflow, which on this chip reboots the
///   chip with `rst:0xc (SW_CPU_RESET)` and prints nothing at all, because the
///   panic handler has no stack left to run on. That is why this read as "the
///   watchdog tripped": the `task_wdt` lines in the same log are the unrelated
///   IDLE1 trip described in `network::run_provisioning`, and the credential
///   was lost with no message.
/// * at 32 KB: 23 408 bytes free at the same point, i.e. the deepest frame
///   this task ever reaches is ~8.6 KB.
///
/// 16 KB is the smallest power of two above the measured need, it is what
/// [`BRING_UP_STACK_BYTES`] already gives the boot task for the same load and
/// save, and it leaves ~7 KB of margin for a configuration that grows.
const CONTROL_STACK_BYTES: usize = 16 * 1024;

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
    // The log sink, and it is the telnet fan-out rather than
    // `esp_idf_svc::log::init_from_env()`: `Fanout` wraps `EspIdfLogger` — the
    // UART0 writer — and additionally copies each record into
    // `cc_hal_esp32::telnet::RING`, from which the port-23 listener streams
    // them. `log::set_logger` succeeds once per process, so the stream wraps
    // ESP-IDF's logger rather than sitting beside it; the UART0 half is not
    // optional and losing it silences the console with no telnet client
    // attached.
    cc_hal_esp32::telnet::init_log().map_err(|err| {
        Box::<dyn Error>::from(format!("the log fan-out could not be installed: {err}"))
    })?;
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

    // The pin map against the wiring, before a single pin is configured.
    //
    // `cc_hal_esp32::pins` holds the numbers every log line below prints; the
    // `peripherals.pins.gpioNN` fields are what the machine is actually wired
    // to. Nothing about `esp-idf-hal` 0.47's typed pin fields lets the second
    // be derived from the first without `unsafe` — so this checks, once, and
    // panics with both numbers if they ever disagree. Without it, changing
    // either side silently makes the boot log describe a machine that is not
    // the one that booted, which is exactly the failure `04 §1.4` names and
    // what the C++'s 21 `static_assert`s in `pinmapping.h:79-99` guarded.
    pins::assert_wiring(&peripherals);
    // The **complete** map, so this one line answers "what is this board
    // actually wired as". Every other pin line below is a subset of it, printed
    // from the same constants.
    info!(
        "pins: heater=GPIO{} valve=GPIO{} pump=GPIO{} switch power=GPIO{} \
         brew=GPIO{} steam=GPIO{} hot_water=GPIO{} tank=GPIO{} \
         temp=GPIO{} i2c sda=GPIO{} scl=GPIO{} scale data=GPIO{}/GPIO{} \
         clock=GPIO{} uart tx=GPIO{} rx=GPIO{}",
        pins::HEATER,
        pins::WATER_VALVE,
        pins::PUMP,
        pins::POWER_SWITCH,
        pins::BREW_SWITCH,
        pins::STEAM_SWITCH,
        pins::WATER_SWITCH,
        pins::WATER_TANK_SENSOR,
        pins::TEMP_SENSOR,
        pins::I2C_SDA,
        pins::I2C_SCL,
        pins::SCALE_DATA_1,
        pins::SCALE_DATA_2,
        pins::SCALE_CLOCK,
        pins::UART_TX,
        pins::UART_RX,
    );

    // The TWDT driver is *moved* into the control task so the subscription
    // belongs to the control task and to nothing else (04 §2: "Watchdog feed —
    // control task only").
    let twdt = peripherals.twdt;
    // The LEDC peripheral is deliberately left unused, and *taking* the
    // peripheral is how a previous build ended up one step away from calling
    // `ledc_set_duty_and_update`. Not taking it at all means no future edit can
    // reach a duty write by accident.
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
    //    There is no LEDC construction site in this binary and no LEDC transport
    //    type in `cc-hal-esp32`, so there is no path on which a duty write can
    //    reach `ledc_ll_set_duty_start` and its watchdog-eating spin. See
    //    `HEATER_LEDC_DEFECT` above.
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
    // `HeaterOutput::set_duty` converts the count back through
    // `cc_domain::heater::CHOSEN_MAX_DUTY`. A `max_duty` of 1 means the control
    // task's `set_duty` is a pure pass-through of the gate's decision, with no
    // second quantisation.
    //
    // **The transport choice is the firmware's, and it is stated here.** R1-07
    // had an LEDC arm and a stand-in, and the stand-in is what the build used
    // because the LEDC arm panicked the chip (`HEATER_LEDC_DEFECT` above). A
    // target whose `ledc_ll.h` has no spin changes this line and re-adds a
    // transport in `cc-hal-esp32::heater`.
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
        "pin readback OK: heater=GPIO{} ({heater_description}) valve=GPIO{} \
         pump=GPIO{} all inactive",
        pins::HEATER,
        pins::WATER_VALVE,
        pins::PUMP,
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
                pins::I2C_SDA,
                pins::I2C_SCL,
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
    //     Nothing is inhibited: the pump, the valve relay and the heater all
    //     follow the reducer's effects. R4-01's bring-up inhibit is gone, and
    //     `cc_hal_esp32::actuators` defaults to `Inhibit::NONE`, so a build that
    //     wants the water path held off sets it explicitly and logs it.
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
    info!(
        "actuators: pump=GPIO{} valve=GPIO{} heater=GPIO{} owned by the control task; \
         no inhibit, the water path follows the reducer",
        pins::PUMP,
        pins::WATER_VALVE,
        pins::HEATER,
    );

    // 5c-bis. The status LED pins, **reserved** here and configured below once
    //     `config` exists.
    //
    //     Reserved for the same reason as the switches' and the scale's: the
    //     `hardware.leds.*.enabled` flags decide whether these pins are driven at
    //     all, and `Peripherals::take` happens exactly once. Taking them here —
    //     unconditionally, before the flags are known — is what makes it
    //     impossible for a flag to decide *which pins the rest of the firmware
    //     gets*, which is the same reasoning `SwitchPins` below records.
    //
    //     **GPIO26 and GPIO19 only.** `PIN_STEAMLED` is GPIO1, and GPIO1 belongs
    //     to the UART provisioning console (`start_provisioning` below). A pin
    //     has one owner; see `cc_hal_esp32::pins` for the full reasoning and
    //     `intentional-diffs.md` for the entry.
    let led_pins = LedPins {
        status: peripherals.pins.gpio26,
        brew: peripherals.pins.gpio19,
    };

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
    //    control task (the only writer — `BlobConfigStore::load`/`save` take
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

    // 7a-ter. The status LEDs, now that `hardware.leds.*` is known.
    //
    //     Built here rather than at the pin reservation above for the same reason
    //     the switches' bank is: the `enabled` flags decide whether a pin is
    //     driven, and reading them before the configuration exists would make the
    //     driver — not the operator's setting — the thing that decided.
    //
    //     The C++ does this in `HardwareManager::initializeLEDs` (`:95-125`),
    //     behind the same two flags, and also turns each LED **off** as it builds
    //     it. `StandardLed::new` does the same, which is why a machine powered up
    //     mid-brew does not look idle.
    //
    //     `inverted` comes from `hardware.leds.*.inverted` and **not** from the
    //     relay trigger types — the brief for 3.1 said "an `inverted` flag from the
    //     relay trigger config", which is not what the C++ does:
    //     `HardwareManager.cpp:101,109,117` read the three `inverted` parameters
    //     directly. They are separate settings for separate reasons, and the LEDs'
    //     are about common-anode parts rather than relay coils.
    //
    //     A failure to configure a pin is **not** fatal. `bring_up` returns
    //     `Result`, and an operator whose LED pin is broken should still get a
    //     machine that makes coffee; this is the same call the I²C bus above makes
    //     with `no pressure, no display`.
    let status_pin = led_output_pin(config.hardware.leds.status.enabled, led_pins.status);
    let brew_pin = led_output_pin(config.hardware.leds.brew.enabled, led_pins.brew);
    let leds = cc_hal_esp32::leds::Leds::new(
        status_pin,
        brew_pin,
        config.hardware.leds.status.inverted,
        config.hardware.leds.brew.inverted,
    );
    if leds.any_configured() {
        info!(
            "leds: status=GPIO{} brew=GPIO{} driven by the control task \
             (status inverted={} brew inverted={})",
            cc_hal_esp32::leds::STATUS_PIN,
            cc_hal_esp32::leds::BREW_PIN,
            config.hardware.leds.status.inverted,
            config.hardware.leds.brew.inverted,
        );
    } else {
        info!("leds: hardware.leds.status.enabled and .brew.enabled are both false");
    }
    // The third LED, and the only reason it is missing: `PIN_STEAMLED` is
    // GPIO1 (`pinmapping.h:45`), which is UART0's TXD and therefore the
    // provisioning console's recovery path. `Peripherals::take` will not hand
    // one pin to two owners, so an operator who left `hardware.leds.steam.enabled`
    // true gets a setting this firmware silently ignores. One `warn!` at boot is
    // the whole diagnostic — the ledger entry is
    // `intentional-diffs.md` ("The steam LED and GPIO1").
    if config.hardware.leds.steam.enabled {
        warn!(
            "leds: hardware.leds.steam.enabled is set but this firmware drives no steam \
             LED — PIN_STEAMLED (GPIO1) is UART0 TX, reserved for the wifi \
             provisioning console. The steam mode indicator will not light."
        );
    }

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
    //     R4-01b's first listed win). `cc_protocol::abp2::Driver` makes the 10 ms a
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

    // 10a. The telnet log stream. `telnet esp32.local 23` is what
    //     `docs/operations/runbook.md` §4 documents and what the
    //     field-diagnosis story in `intentional-diffs.md` §1 is built on, and
    //     finding 3.2 of `32-findings-2026-10-03.md` was that the Rust port had
    //     the shed policy and no way to serve it. Spawned here, beside the HTTP
    //     server and
    //     after it, because the listener is a network-tier service with no
    //     bearing on the control loop and a failure to bind is a `warn!` inside
    //     the task rather than a boot failure.
    //
    //     It is at `task::TELNET_PRIO` = 2, the lowest priority in the firmware:
    //     a terminal that has stopped reading costs the ring a dropped line and
    //     nothing else, and the ring push is a bounded copy with no allocation.
    if let Err(err) = cc_hal_esp32::telnet::start() {
        error!("telnet: the log stream task could not be spawned: {err}");
    }

    // 11b. The UART provisioning task. 04 §3.2 says "only spawned when no valid
    //     credentials exist, and it exits after success" — and that rule is what
    //     made a **wrong** network unfixable over the cable: with a credential
    //     stored the console never armed, and the only way to change it was
    //     `POST /api/wifi-reset`, which needs a machine that is already online.
    //     That is the wrong shape for a recovery path. Found on the bench: the
    //     machine was configured for a network that did not exist, and could not
    //     be pointed at one that did.
    //
    //     So it always starts, it still **exits after success** (it is not a
    //     permanent task), and `wifi set` + `wifi apply` replaces whatever is
    //     stored. The task costs one UART poll loop and it is the only way out
    //     of "the network you want is not the network you are on".
    //
    //     The handoff is shared with the control task, which is what does the
    //     writing: the store moved into that task in step 7, and a credential
    //     cannot be stored by a task that does not hold the store.
    let handoff = network::Handoff::new();
    if config.is_wifi_provisioned() {
        info!(
            "wifi: a credential is stored — the provisioning console is still \
             available, and `wifi set` + `wifi apply` will replace it"
        );
    }
    start_provisioning(
        peripherals.uart0,
        peripherals.pins.gpio1,
        peripherals.pins.gpio3,
        handoff.clone(),
    );

    // 11. Whether MQTT is configured, and nothing else. The **client** is built
    //     inside the control task, not here.
    //
    //     It used to be built here and read two lines later, which meant the
    //     `EspMqttClient` was dropped at the end of this `match` arm — and
    //     `impl Drop for EspMqttClient` calls `esp_mqtt_client_destroy`
    //     (`esp-idf-svc` `src/mqtt/client.rs:742-747`). The client was therefore
    //     destroyed microseconds after `esp_mqtt_client_start`, and nothing in
    //     the firmware ever called `publish`, `publish_online`,
    //     `discovery_due`, `due_for_reconnect` or `subscribe`: a fully
    //     implemented client that never did anything.
    //
    //     It lives in the control task because that is where the C++'s is:
    //     `LoopManager::updateNetwork` (`LoopManager.cpp:485-508`) drives
    //     `checkConnection` and `writeSysParamsToMQTT` from the main loop, and
    //     this task is the main loop — it owns the clock, the configuration, the
    //     store, the machine, and the only watchdog subscription, so a publish
    //     that stalls is visible. See `mqtt_link`.
    //
    //     `mqtt.enabled` is false and `mqtt.broker` is empty by default, so an
    //     unprovisioned machine still spends nothing here: no 4 KB of task stack
    //     and no two 1 KB buffers for a client with nowhere to connect, which is
    //     the C++'s `MQTTManager.cpp:79-83` too.
    let mqtt_configured = cc_hal_esp32::mqtt::is_configured(&config);
    if !mqtt_configured {
        info!("mqtt: not configured (mqtt.enabled is false or mqtt.broker is empty)");
    }

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
        leds,
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
        store,
        sampler,
        handoff: handoff.clone(),
        sta: wifi,
    });
    let control = cc_hal_esp32::task::spawn_with_prio(
        c"control",
        CONTROL_STACK_BYTES,
        cc_hal_esp32::task::CONTROL_PRIO,
        move || {
            if let Err(err) = control_task(args) {
                error!("control task failed: {err}");
            }
        },
    )?;

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
    let display_thread = cc_hal_esp32::task::spawn_with_prio(
        c"display",
        DISPLAY_STACK_BYTES,
        cc_hal_esp32::task::DISPLAY_PRIO,
        move || display.run(),
    );
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

/// The two status LED pins, taken from `Peripherals` at bring-up and held until
/// `hardware.leds.*.enabled` says whether they are driven.
///
/// GPIO26 (`PIN_STATUSLED`) and GPIO19 (`PIN_BREWLED`), and **no third field**.
/// `PIN_STEAMLED` is GPIO1 in `pinmapping.h:45`, and GPIO1 is the UART0 TXD this
/// firmware gives to the provisioning console; `Peripherals::take` will not hand
/// one pin to two owners. The full analysis, including what moving the steam LED
/// to GPIO32 would cost, is the comment block in `cc_hal_esp32::pins`.
struct LedPins {
    /// `PIN_STATUSLED` (GPIO26).
    status: esp_idf_hal::gpio::Gpio26<'static>,
    /// `PIN_BREWLED` (GPIO19).
    brew: esp_idf_hal::gpio::Gpio19<'static>,
}

/// Configure one LED pin as a push-pull output, or `None` when the operator has
/// the LED disabled.
///
/// The `enabled` gate is here, not at the call site, so that "no LED" is always
/// `None` and never a pin somebody could still write. A disabled LED's pin is
/// simply dropped — `led_pins`' field is moved in and gone — which is the
/// strongest form of the C++'s `if (enabled) { make_unique<StandardLED>(...) }`
/// (`HardwareManager.cpp:96-124`).
///
/// # Errors
///
/// `EspError` from [`esp_idf_hal::gpio::PinDriver::output`], which fails if the
/// pin is already owned. A machine that hits this has a wiring fault; the caller
/// logs it and runs without the LED rather than refusing to boot, because
/// "the brew LED does not work" is not a reason a coffee machine makes no
/// coffee.
fn led_output_pin<P>(enabled: bool, pin: P) -> Option<PinDriver<'static, esp_idf_hal::gpio::Output>>
where
    P: esp_idf_hal::gpio::OutputPin + 'static,
{
    if !enabled {
        return None;
    }
    match esp_idf_hal::gpio::PinDriver::output(pin) {
        Ok(driver) => Some(driver),
        Err(err) => {
            warn!("leds: a status LED pin could not be configured as an output: {err:?}");
            None
        }
    }
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
/// three listed wins). `cc_protocol::abp2::Driver` makes the 10 ms a *deadline*
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
        pins::I2C_SDA,
        pins::I2C_SCL,
        cc_protocol::abp2::ADDRESS,
        cc_protocol::abp2::CADENCE.raw(),
    );
    Some(cc_hal_esp32::Abp2Pressure::on_shared_bus(bus))
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
    // `cc_protocol::sensor::hx711::Rate`.
    let rate = cc_protocol::sensor::hx711::Rate::default();

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
            let driver = cc_protocol::sensor::hx711::Scale::dual(
                scale_config.calibration,
                scale_config.calibration2,
                average,
            );
            (bus, driver)
        }
        cc_domain::hardware::ScaleType::Hx711Single => {
            let bus = cc_hal_esp32::GpioHx711::single(pins.data_1, pins.clock)?;
            let driver =
                cc_protocol::sensor::hx711::Scale::single(scale_config.calibration, average);
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

/// The long-press "REBOOTING" pause, in milliseconds.
///
/// `PowerHandler::triggerSystemReboot` (`PowerHandler.h:177-192`) shows the
/// message and waits. There is no display in this build, so the wait is all that
/// survives of it — and it is still load-bearing: it is what lets the operator
/// see that the long press did something before the console goes away.
const REBOOT_DISPLAY_MS: u32 = 1_000;

/// The pause between a successful OTA and the restart, in milliseconds.
///
/// `OTA_RESTART_DELAY_MS` (`src/ota.cpp:41`), 1000, and the reason is the same
/// as the reboot branch's 500 ms: the response has to leave the socket before the
/// chip resets, or the browser reports a network error instead of the success the
/// firmware actually achieved. The C++ schedules it with `millis()` arithmetic
/// (`ota.cpp:456-459`); here it is a `delay` between ticks, which is the same
/// wait from the same place in the loop.
const OTA_RESTART_DELAY_MS: u32 = 1_000;

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
    /// The status and brew LEDs. Owned here for the same reason `actuators` is:
    /// the control task is where `LoopManager::updateLEDs` runs in the C++, and a
    /// second owner would be a second writer of a pin this type owns. The
    /// **steam** LED is not in it — see `LedPins`.
    leds: cc_hal_esp32::leds::Leds,
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
    ///
    /// A `bool` and not the client: `/api/status` reports `mqttConfigured` from
    /// `bring_up`'s copy of the configuration, which the control task's copy may
    /// later change, and the session state is read from the client the control
    /// task owns.
    mqtt_configured: bool,
    /// The configuration store. **Moved**, not borrowed: `BlobConfigStore::load`
    /// and `save` both take `&mut self` and one owner beats a lock.
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
        mut leds,
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

    // 🔴 The shot counter, restored before the machine exists.
    //
    // `SystemInitializer.cpp:282` calls `maintenanceCoordinator().begin()` as one
    // of the first things it does, and `begin()` is the NVS read
    // (`MaintenanceCoordinator.cpp:17-27`). It is read here for the same reason
    // and handed to `Control::boot`, so the machine is *born* holding the count
    // rather than being told about it afterwards.
    //
    // A read that fails is a `warn!` and a zero, not a fault: a lost count is a
    // reminder that fires early, and the alternative — refusing to boot over a
    // maintenance counter — is what `begin()`'s own failure path avoids.
    let restored_shots = match cc_hal_esp32::nvs::load_shots_since_backflush(store.backend()) {
        Ok(Some(shots)) => {
            info!("maintenance: loaded {shots} shots since backflush");
            shots
        }
        Ok(None) => {
            info!("maintenance: no stored shot count — starting at 0");
            0
        }
        Err(err) => {
            warn!("maintenance: the stored shot count could not be read ({err}) — starting at 0");
            0
        }
    };
    // Seeded with the restored value so the first tick does not re-write a count
    // that is already stored. See `FirmwareSide::with_shots_since_backflush`.
    let mut side = cc_hal_esp32::FirmwareSide::with_shots_since_backflush(Some(restored_shots));

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
    let (mut control, boot_effects) =
        control::Control::boot(&config, boot_now, power_pressed, restored_shots);
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
    // The MQTT link, **built here** rather than in `bring_up` and moved in.
    //
    // `EspMqttClient` is `Send` (`esp-idf-svc` `src/mqtt/client.rs:786`) and
    // everything else in `mqtt_link::Link` is too, so it could be moved through
    // `ControlArgs` — but it does not have to be, and not moving it is better:
    // this task already holds the authoritative `Config`, the store and the
    // machine, and the client is what turns them into MQTT traffic.
    //
    // A failure is not fatal, exactly as it is in the C++ with `mqttEnabled_`
    // false: the machine runs without MQTT and `/api/status` says so.
    let mut mqtt = if mqtt_configured {
        match mqtt_link::Link::new(&config) {
            Ok(link) => Some(link),
            Err(err) => {
                warn!("mqtt: the client did not start: {err:?} — the machine runs without it");
                None
            }
        }
    } else {
        None
    };
    // `TARE_ON` / `CALIBRATION_ON`: the C++'s `scaleTareMode_` /
    // `scaleCalibrationMode_` latches. See `mqtt_link::ScaleModes`.
    let mut scale_modes = mqtt_link::ScaleModes::default();
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
        // `cc_machine::Effects`, not `Vec`: this is filled by the command
        // queue, appended to by `Control::tick`, and applied — four to five times
        // per 10 ms tick. A `Vec` here was a heap allocation per tick in the same
        // loop that runs the heater deadman (fixed by `284ad17a`).
        let mut effects = cc_machine::Effects::new();
        let mut commands_applied: u32 = 0;
        // Set by `Command::OtaBegin` and discharged after `apply`. See the arm.
        let mut ota_shutdown_pending = false;
        while let Some(command) = commands.recv() {
            info!("control: command {command:?}");
            // Counted, not acked, here: the ack is only honest once the
            // telemetry the caller will read has been published **after** this
            // command was folded in, which happens further down the tick. See
            // the `note_applied()` calls after the publish.
            commands_applied += 1;
            match command {
                cc_hal_esp32::web::Command::Restart => net.shared.set_reboot_requested(),
                // **S8.** An OTA session has started, so the hardware goes off
                // before any flash write — the pump, the water valve and the
                // heater duty, through the real applier, on this task.
                //
                // The C++ calls `otaPrepareHardware()`
                // (`SystemInitializer.cpp:57-63`) directly from the OTA module and
                // gets `disableTimer1()` plus `disableHeater()`: the pump and the
                // valve are left in whatever state they were in, which is the gap
                // 04 §4 names when it says *"OTA must call `safe_hardware_shutdown`,
                // not just `disable_heater`"*. `cc_machine::ota::begin_session`
                // emits the effect that closes them all.
                //
                // **Only the request is handled here.** The decision is not, and
                // that is the fix for the OTA admission race. The httpd task asked
                // against a telemetry snapshot up to one control period old, and
                // between that read and this tick the machine stayed live — it
                // honours a `brew_start` off MQTT or a brew-switch press, and
                // `BREW_PREINFUSION`'s `onEntryImpl` opens the water valve
                // (`BrewStates.cpp:67-79`). A verdict is only good for the instant
                // it was computed, so admission is re-taken below, against the
                // state this tick's own transitions produced, which is the state
                // the effects being applied are for.
                //
                // So this arm records the *request* and nothing else;
                // `ota_shutdown_pending` is discharged after `apply`, where the
                // live state is known and the shutdown can be applied as its own
                // pass — which is also what makes it ordering-proof.
                cc_hal_esp32::web::Command::OtaBegin => {
                    info!("control: OTA session requested — admission re-checked after apply");
                    ota_shutdown_pending = true;
                }
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
                //
                // The value arrives already inside the schema's `brew.setpoint`
                // range: the handler parses it with `cc_config::assign::parse`
                // (`web::parse_setpoint`) rather than the C++'s permissive
                // `0.0..=150.0`, which is what let a 150 °C setpoint be written
                // and reloaded on every boot here. The `validate_config` call
                // below is the second half of the same rule — the cross-
                // parameter one, `emergency_temp` against the setpoint plus
                // hysteresis — and it is here for the same reason it is on the
                // `/api/parameters` and MQTT paths below: a write that leaves
                // the machine unable to run safely is *reported*, and the
                // fail-closed rule discards it at the next boot (08 §4.1).
                cc_hal_esp32::web::Command::SetSetpoint(celsius) => {
                    config.brew.setpoint = celsius;
                    control.set_setpoint(celsius);
                    if let Err(violation) =
                        cc_safety::validate_config(&control::safety_config(&config))
                    {
                        error!(
                            "config: brew.setpoint = {celsius} leaves the configuration \\
                             UNSAFE ({violation:?}); the next boot will discard it"
                        );
                    }
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
                    // **One command carrying the value**, which is what the C++
                    // does: `setBackflushMode(!backflushMode())`
                    // (`WebServerManager.cpp:489-491`). This used to branch to
                    // `BackflushEnter` / `BackflushStop`, and neither of those
                    // turns the mode *off* — `BackflushStop` stops a running
                    // cycle and leaves `backflush.on` set. Bench-measured: four
                    // toggles in a row all answered `backflushOn: true`.
                    let on = !control.machine().backflush.on;
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::SetBackflushMode(on)),
                        &mut effects,
                    );
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
                    // One command, both directions — the C++'s single
                    // `setBackflushMode(newState)`. Mapping `false` to
                    // `BackflushStop` was wrong: that stops a running cycle and
                    // leaves `backflush.on` set, so `?on=0` answered
                    // `backflushOn: true` on the bench.
                    control.feed(
                        &config,
                        Event::Command(cc_machine::Command::SetBackflushMode(on)),
                        &mut effects,
                    );
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
                //
                // The second line is the other half of the C++'s one method: the
                // reducer's `Effect::ResetShotsSinceBackflush` clears the counter
                // and calls `on_reset_shots_since_backflush`, and this route
                // clears it and calls **the same method**, so the NVS write is
                // one code path rather than two that have to be kept in step.
                // Without it the counter cleared and came straight back on the
                // next boot, which is the shape of bug this whole area had.
                cc_hal_esp32::web::Command::ResetBackflushCounter => {
                    control.reset_shots_since_backflush();
                    side.on_reset_shots_since_backflush(0);
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
        // configuration, and a `Mutex<BlobConfigStore>` shared with the httpd task
        // would put a 2 KB blob write on whichever task the web server happened
        // to be serving.
        //
        // **R3-13's inbound MQTT calls `apply` at this line too**, with no
        // handoff: the radio moved into this task, so an MQTT message is
        // delivered here and there is nothing to hand over. One writer, one
        // place, one store write.
        //
        // **One request per tick, not all of them.** This used to be
        // `parameters.take_all()`, and every drained request ran its own
        // `persist_config` — a ~2 KB JSON serialise plus an NVS erase-and-write —
        // on this task, inside the 10 ms period, *before* the heater decision
        // for that period. `STAGED_PARAMETER_DEPTH` is 4, so four browser tabs
        // saving at once meant four flash transactions in one tick. The mailbox
        // is bounded at 4 and the ack timeout is 1600 ms, so taking one per
        // tick bounds the flash work per tick at one transaction and still
        // answers the last of four within 40 ms. The reasoning, and why
        // coalescing was the alternative, are on
        // `ParameterHandoff::take_one`.
        //
        // **The request is taken, not drained-then-iterated**, so a `stage`
        // arriving *after* this line waits for the next tick rather than being
        // picked up by this one — which is the point: the bound is on the work
        // this tick does, not on the moment it reads the mailbox.
        if let Some(pairs) = parameters.take_one() {
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
            } else {
                info!(
                    "config: {} parameter(s) written: {applied:?}",
                    applied.updated
                );
                // A write that leaves the machine unable to run safely is persisted,
                // and the fail-closed rule discards it at the next boot (08 §4.1).
                // Saying so now is the difference between "my setting vanished" and a
                // diagnosis; the C++ has no check on this path and loses it silently.
                if let Err(violation) = cc_safety::validate_config(&control::safety_config(&config))
                {
                    error!(
                        "config: the stored configuration is now UNSAFE ({violation:?}) and the \
                         next boot will discard it"
                    );
                }
                persist_config(&mut store, &config);
                push_into_machine(
                    &mut control,
                    &config,
                    (pid_enabled_before, brew_setpoint_before),
                    &pairs,
                    &mut effects,
                );
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
                parameters.publish_live(cc_web::parameters_json(&config));
                // The ack the `POST` handler is blocked on. See
                // `ParameterHandoff::stage_and_wait`.
                parameters.note_applied();
            }
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
                    // **Same shape as the two reboot paths below**, and for the
                    // same two reasons. Without the shutdown, the relays are
                    // left as the last tick left them until the reset lands --
                    // the hazard the `take_reboot_request` branch documents. And
                    // without the pause the line above does not reach the wire:
                    // measured on a bench ESP32, `restart_now()` immediately
                    // after this `info!` reset the chip before UART0 had
                    // drained, so `just wifi-provision` waited out its window
                    // and reported "no confirmation that the credential was
                    // stored" for a credential that had been stored.
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
                Err(err) => {
                    error!("config: the credential from the console was NOT stored: {err}");
                }
            }
        }

        // ---- 4. SENSE ---------------------------------------------------------
        //
        // The probe, the five switches, the pressure sensor and the load cell, on
        // their own cadences inside one 10 ms period. The period is what changed,
        // and that is the whole of "the screen takes half a second to react": a
        // press is now recognised within one 20 ms debounce window rather than
        // within one 400 ms control period, and the frame reaches the panel from
        // the display task rather than at the end of this same iteration.
        //
        // The probe stays on **this** task on purpose — see [`sensor_task`] for
        // the measurement that says a second task cannot own it on this
        // toolchain.
        temp.poll(now, &mut sensor_fault_logged);
        let last_reading = temp.last_reading();
        let pressure_bar = pressure.as_mut().and_then(|sensor| match sensor.poll(now) {
            Ok(cc_protocol::abp2::Poll::Sample(sample)) => Some(f64::from(sample.pressure.raw())),
            Ok(_) => None,
            Err(err) => {
                debug!("control: ABP2 read: {err:?}");
                None
            }
        });
        let tank_full = switches.water_tank_full();
        let edges = switches.poll(now);

        // **The weight, here, with the other readings and not after the tick.**
        //
        // This is the number `Sensors::brew_weight` carries, and it used to be
        // unreachable from there: `drain_scale` returned it at step 7b, which is
        // below `control.tick`, so the sample the reducer saw could not be built
        // from it and the field was written as the literal `0.0`. The weight was
        // already published to `/api/status` and MQTT from the same value, so the
        // machine was measuring a shot correctly and refusing to stop on it.
        //
        // See [`scale_weight`] for the reading-versus-event argument and
        // [`config_io::drain_scale`] for why the *drain* stayed where it was: its NVS
        // commit must not sit between the reducer's decision and the pins.
        let weight_g = scale_weight(sampler.as_ref());

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
        // emits in this very tick is judged against this tick's state. The
        // machine **state** is not one of them: it is told after the tick, beside
        // the `apply` it belongs to — see there.
        actuators.set_now(now);
        actuators.set_water_tank_full(tank_full);
        actuators.set_latched(control.safety_state().latched);

        // ---- 6 + 7. DECIDE, then ACT ------------------------------------------
        //
        // The sample the reducer sees. `has_temperature_error` is the C++'s
        // `hasTemperatureSensorError()`, and it is the OR of **two independent
        // facts**, because that is what the C++ has: the boot-window fact that
        // no plausible reading has arrived yet, and the driver's own latched
        // fault flag.
        let temperature_faulted = temp.has_error();
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
            // note in `docs/history/divergences.md` (#12).
            temperature: last_reading.map_or(Celsius::new(0.0), |(celsius, _)| {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "a DS18B20 reading is a multiple of its own \
                              0.0625 C resolution, so f32 holds it exactly"
                )]
                Celsius::new(celsius as f32)
            }),
            water_tank_full: tank_full,
            // 🔴 **The driver's latched flag is the load-bearing term**, and the
            // plausibility of `last_reading` is only the boot window.
            //
            // `last_reading` is a *cache* that `poll` writes but never
            // invalidates, so a probe that reads 95 °C and then goes silent
            // leaves `Some((95.0, true))` behind forever. Asking only
            // `is_none_or(|(_, plausible)| !plausible)` therefore reports a dead
            // probe as a healthy one — and the consequences are not cosmetic:
            // the sensor-error guard never fires, so `SENSOR_ERROR` is
            // unreachable, `sample_seq` freezes with the counter, so S1's
            // over-temperature debounce cannot advance either, and the PID keeps
            // regulating against a frozen number with the heater energised.
            //
            // `TempSensor::error_` (`TempSensor.h:50-53`) is what the C++ reads
            // for this, and it is latched independently of the cached value —
            // that independence is the whole point, and it is what this term
            // restores. The OR keeps the boot window working: the driver's flag
            // is `false` until its tenth consecutive failure, so without the
            // first term the machine would heat a boiler it has never measured.
            has_temperature_error: temperature_faulted
                || last_reading.is_none_or(|(_, plausible)| !plausible),
            // The scale is not part of `Sensors::has_sensor_error`'s contract
            // here: `has_scale_error` is a separate field and the C++'s
            // `hasSensorError()` ORs the two (`SensorCoordinator.h:190-192`).
            // A machine with no scale has no scale error, so this is `false`.
            has_scale_error: sampler.as_ref().is_some_and(|s| s.telemetry().faulted()),
            // 🔴 **The real weight, and this line is the whole of the fix.**
            //
            // It was the literal `0.0`, which made the by-weight arm of
            // `BrewRunningState::checkSpecificTransitions` (`BrewStates.cpp:294-299`)
            // unreachable for any target above 0 g — and with `brew.by_time`
            // disabled, `initTotalTargetBrewTime` (`:53-62`) returning `0.0` takes
            // the other arm with it, so the only thing left was the brew switch or
            // the 300-second pump watchdog. The same zero reached
            // `recordBrewIfQualified` (`BrewStates.cpp:311`) through
            // `Machine::brew_weight`, killing the weight arm of shot counting too.
            //
            // `None` is `0.0` and not an error: an absent scale reports no weight,
            // and `brew.by_weight` on a machine with no scale is refused by
            // `cc_safety::validate_config` (`ConfigViolation::BrewByWeightWithNoScale`)
            // rather than being left to fail silently here.
            //
            // `f64 -> f32` is the C++'s own narrowing: `getCurrentBrewWeight()`
            // is `static_cast<float>` (`MachineStateContext.cpp:130-133`) and the
            // field is `float`. A load cell resolves well under 0.1 g.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "the C++ casts `getBrewWeight()` to `float` at \
                          `MachineStateContext.cpp:130-133` and `Sensors::brew_weight` \
                          is that `float`; an HX711 at Gain128 resolves 0.19 g, so f32 \
                          is exact to well below the cell's own resolution"
            )]
            brew_weight: weight_g.unwrap_or(0.0) as f32,
            // The probe's own conversion counter, so S1's debounce counts
            // readings rather than ticks. See `Sensors::sample_seq`.
            sample_seq: temp.sample_seq(),
        };

        let tick_effects = control.tick(&config, sensors, &edges, now);
        // The queue's effects were already folded above; `Control::tick` starts
        // its own list, so the two are concatenated in the C++'s order —
        // commands first (step 3 of the loop), then the tick's own.
        effects.extend(&tick_effects);
        // **And the facade is told the machine state first.** `may_open_water`
        // and `may_open_steam` are whitelists over it, so an effect the reducer
        // emitted *because of* this tick's transition has to be judged against
        // the state that transition produced. Told before the tick, the cache
        // still holds the state this tick left — so entering `BREW_PREINFUSION`
        // (whose `onEntryImpl` opens the water valve, `BrewStates.cpp:67-79`)
        // would be judged against `PID_NORMAL` and refused for one tick. The
        // MQTT path below already had to do this for the same reason.
        actuators.set_state(control.state());
        cc_machine::apply(&mut actuators, &mut side, control.machine(), &effects);

        // ---- 6b. the OTA session's shutdown, decided and applied last --------
        //
        // Two things happen here that could not happen inside the pass above,
        // and both are the point.
        //
        // **One: admission is re-read from the live state.** `effects` is the
        // queue's list followed by this tick's, applied front to back with no
        // coalescing (`applier.rs:190-201`). An `OtaBegin` extended into that
        // list would put `SafeHardwareShutdown` *first*, and anything the same
        // tick emitted after it — `EnablePump`, `OpenWaterValve` from entering
        // `BREW_PREINFUSION` — would overwrite it in the same pass. The flash
        // would then erase 1.8 MB with the pump and the valve open. So the
        // admission check happens here, against `control.state()` as this tick's
        // transitions left it, which is the state the pass above just acted on.
        //
        // **Two: the shutdown is its own applier pass**, applied after the one
        // above rather than inside it, so nothing in that pass can re-energise
        // hardware after it. That is the ordering-proof half, and it is why this
        // is not folded back into `effects`.
        //
        // A refusal is not a dropped effect — it is the answer, and it goes back
        // to the httpd task, which turns it into a `409` and never calls
        // `esp_ota_begin`. The same three reboot paths below reach
        // `SafeHardwareShutdown` directly and deliberately skip admission: a
        // reboot is asked for by the operator who is already talking to the
        // machine, and an OTA erase is not something to start mid-brew.
        if ota_shutdown_pending {
            match cc_machine::ota::begin_session(control.state()) {
                Ok(session_effects) => {
                    info!("control: OTA admitted — safe hardware shutdown");
                    cc_machine::apply(
                        &mut actuators,
                        &mut side,
                        control.machine(),
                        &session_effects,
                    );
                    net.ota.note_verdict(cc_hal_esp32::ota::Admission::Admitted);
                }
                Err(refusal) => {
                    // **Refuse the flash.** Not "skip the shutdown and carry
                    // on": the session is dead, and the httpd task is waiting on
                    // exactly this answer.
                    warn!("control: OTA refused — {}", refusal.message());
                    net.ota
                        .note_verdict(cc_hal_esp32::ota::Admission::Refused(refusal));
                }
            }
        }

        // ---- 7b. write down the shot counter, if it moved ---------------------
        //
        // `MaintenanceCoordinator::recordBrewIfQualified` and
        // `resetSinceBackflush` both end in a `Preferences` write
        // (`MaintenanceCoordinator.cpp:44,59`) and this is where this firmware
        // does the same, because the applier cannot: the store is this task's,
        // and `FirmwareSide` only records what it was told (see
        // `FirmwareSide::shots_to_persist`).
        //
        // It is here rather than inside the applier for a second reason: an NVS
        // commit is an erase-and-write, measured in milliseconds, and the applier
        // runs in the middle of a 10 ms control period.
        if let Some(shots) = side.take_shots_to_persist() {
            match cc_hal_esp32::nvs::save_shots_since_backflush(store.backend_mut(), shots) {
                Ok(()) => {
                    side.note_shots_persisted(shots);
                    info!("maintenance: shots since backflush = {shots} persisted");
                }
                Err(err) => error!(
                    "maintenance: shots since backflush = {shots} was NOT persisted: {err}. \
                     The count in memory is still correct and the next counted brew will \
                     write it again, but a reboot before then loses this one."
                ),
            }
        }

        // ---- 7a-bis. the status LEDs ------------------------------------------
        //
        // `LoopManager::updateLEDs` (`LoopManager.cpp:255-286`), which in the C++
        // is one of the four things the 10 ms main loop does. It sits here for
        // the same reason the applier does: it is a function of the state this
        // tick's transition produced, so asking before `apply` would light the
        // wrong LED for one tick on every entry and exit.
        //
        // **Two of three.** `LedOutput::steam` is computed and discarded — see
        // `cc_hal_esp32::pins` for why GPIO1 is the console's and not this
        // machine's steam LED, and `intentional-diffs.md` for the entry.
        //
        // The `isr_counter` is the C++'s `systemContext_.isrCounter()`, which
        // `isr.h:110-117` builds by adding `ISR_COUNTER_INCREMENT` (10, one
        // 10 ms tick) and wrapping at `processWindowSize()` (1000,
        // `ProcessState.h:183`) — so it is `millis()` truncated to 10 ms steps,
        // modulo one second. `isBlinkPhaseOn` is `< 500`, i.e. the first half of
        // that second, and the brew LED's manual-flush exception blinks on it.
        // Deriving it from the tick clock rather than counting ticks is what makes
        // it agree with the display's blink: the two halves have to come from one
        // counter or the panel and the LED drift apart, and `tick_began_ms` is
        // already this tick's 10 ms-aligned timestamp.
        leds.apply(cc_display::leds::LedOutput::from_state(
            &cc_display::model::DisplayInput {
                state: control.state(),
                temperature: last_reading.map_or(0.0, |(celsius, _)| celsius),
                setpoint: control.setpoint(),
                isr_counter: (tick_began_ms % 1000) / 10 * 10,
                ..Default::default()
            },
            config.display.blinking.delta,
        ));

        // Publish the values this task is actually running with, so
        // `GET /api/parameters` does not answer from the boot snapshot.
        //
        // Once per second, NOT once per tick. This copies all 98 schema values
        // onto the heap -- measured at 420 allocations and 20.5 KB per call --
        // so gating it on the 10 ms control tick drove ~2 MB/s of allocator
        // churn out of the control task for a number an operator looks at once a
        // second. The gate was `HEARTBEAT_MS`, and the comment above it still
        // argued from "2.5 ticks a second"; the tick is now 10 ms. See
        // `PARAMETERS_PUBLISH_MS`.
        if now_ms().wrapping_sub(last_publish_ms) >= PARAMETERS_PUBLISH_MS {
            last_publish_ms = now_ms();
            parameters.publish_live(cc_web::parameters_json(&config));
        }

        // The scale's **events**, drained every tick. The weight is not among
        // them: it was read at step 4, beside the temperature and the pressure,
        // because it is a reading and this is a message queue. See `drain_scale`
        // for why the drain stays here — its NVS commit must not sit between the
        // reducer's decision and the pins — and `scale_weight` for why the weight
        // could not come from here.
        drain_scale(sampler.as_ref(), &mut store, &mut scale_modes);

        // ---- 7c. the backflush reminder, decided once -------------------------
        //
        // `/api/status`'s `backflushReminderDue` and the display's reminder
        // widget are the C++'s one `isReminderDue()` read from two places —
        // `WebServerManager.cpp:363` and `DisplayWidgets.h:333-334` — and they
        // have to be the same answer, so the answer is computed here, once, and
        // the same `bool` goes to both. Two expressions in two places would be
        // two things to keep in step.
        //
        // Before this the predicate was written out inline for the API only, and
        // `DisplayInput::backflush_reminder_due` was **never assigned by
        // anything**: a permanently-false field on a permanently-zero counter,
        // so the widget could not fire however many shots were pulled.
        let backflush_due = cc_machine::maintenance::is_reminder_due(
            control.machine().shots_since_backflush,
            config.maintenance.backflush_reminder.enabled,
            config.maintenance.backflush_reminder.threshold,
        );

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
            // The reminder, and **the same value** `/api/status` reports this
            // tick — see step 7c. The widget itself gates on
            // `config.backflush_reminder_enabled` as well
            // (`cc-display/src/widgets.rs:648`, the C++'s
            // `DisplayWidgets.h:333`), which is why passing the count here
            // rather than the enabled flag keeps the two halves from drifting.
            display_input.backflush_reminder_due = backflush_due;
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
        // A **successful OTA** asks for its own restart, through the same
        // between-ticks path as `POST /api/restart` and for the same reason: a
        // handler that called `esp_restart` itself would reset the machine from
        // inside a request, abandoning the `200` the operator's browser is still
        // reading (`ota.cpp:461` schedules the restart for exactly this reason:
        // *"Restart only after the response has been handed to the client"*).
        //
        // The hardware is already off — `Command::OtaBegin` shut it down before
        // the first flash byte — and the shutdown is applied again below, which
        // is cheap and is the same belt-and-braces the reboot branch takes.
        if net.ota.take_restart() {
            info!("control: OTA completed — restarting into the new image");
            let machine = *control.machine();
            cc_machine::apply_one(
                &mut actuators,
                &mut side,
                &machine,
                cc_machine::Effect::SafeHardwareShutdown,
            );
            FreeRtos::delay_ms(OTA_RESTART_DELAY_MS);
            restart_now();
        }

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
        // The radio's four fields belong to `network::publish_radio`, and this
        // is a **whole-slot replace**, so they have to be carried forward rather
        // than defaulted.
        //
        // Ordering alone is not enough, and the ordering argument that used to
        // sit here was wrong. `publish_radio` runs once a second and this
        // publish runs every 10 ms tick, so "the radio publishes second" only
        // describes the last microsecond of each second: the very next tick
        // replaced the slot with `..Telemetry::default()` and the association
        // flag was gone again. Measured on a bench ESP32 associated at -45 dBm
        // with 10.0.0.7 — `/api/status` reported `wifiAssociated: false,
        // wifiSignal: 0, ip: null` continuously, and `publish_radio` was
        // provably writing the true values every second. A reader only ever saw
        // them if it polled inside the sub-10 ms window.
        let radio_fields = net.shared.snapshot();
        net.shared.publish(Telemetry {
            machine_state: state as i32,
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
            brewing: state.is_brew_state() && state != cc_domain::state::MachineState::BrewFinished,
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
            // Step 7c, computed once and shared with the display. The C++ is
            // `isReminderDue()` (`MaintenanceCoordinator.cpp:67-72`): the
            // count has to reach the threshold *and* the reminder has to be
            // enabled, and both halves come from this task's own `Config`
            // because that is the only place either is readable.
            backflush_due,
            water_tank_full: config
                .hardware
                .sensors
                .watertank
                .enabled
                .then_some(tank_full),
            pressure_bar,
            mqtt_configured,
            // **Read live, this tick.** It was read once, immediately after
            // the client was constructed — which is structurally always
            // `false`, because `esp_mqtt_client_start` connects on the
            // client's own task and no tick had run yet.
            mqtt_connected: mqtt.as_ref().is_some_and(mqtt_link::Link::connected),
            // **Carried forward, not written.** These belong to
            // `network::publish_radio`, which owns them; see the comment on
            // `radio_fields` above for why leaving them at their defaults here
            // was a real, measured defect.
            signal: radio_fields.signal,
            wifi_associated: radio_fields.wifi_associated,
            wifi_offline: radio_fields.wifi_offline,
            ip: radio_fields.ip.clone(),
            uptime_ms: uptime,
            weight_g,
            // `brew_weight_g` stays at its default here: it is the radio's weight
            // sample, published by the weight task, and nothing on the control
            // task writes it.
            ..Telemetry::default()
        });

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

        // The radio's readings. Order is load-bearing: `Shared::publish`
        // replaces the whole slot, so a radio publish BEFORE the telemetry
        // publish would be erased by it and `/api/status` would go back to
        // reporting `wifiAssociated: false` -- which is exactly the bug this
        // replaced. Hence: after.
        //
        // Cadence is the same 1 s poll as below, and deliberately NOT every
        // tick. It used to run at the 10 ms tick rate: four esp-idf FFI
        // round-trips plus a heap allocation, 100 times a second, for values
        // that change on DHCP events measured in minutes. It was also the write
        // side of the use-after-free fixed in `web::Snapshot` -- every
        // reassignment freed the previous IP buffer while the httpd task could
        // be reading it. Fixed by `30b59c48`.
        let radio_due = uptime.wrapping_sub(wifi_last_ms) >= WIFI_POLL_MS;
        if radio_due {
            wifi_last_ms = uptime;
            network::publish_radio(&net.shared, sta.as_ref());
        }

        // The radio's own maintenance, on the C++'s 1 s cadence
        // (`CleverCoffeeWiFiManager::checkAndMaintainConnection`, and
        // `cc_hal_esp32::wifi::MONITOR_PERIOD_MS`). `Sta` is `Send` and the
        // control task is the one place a 1 s poll belongs — it is the task with
        // a heartbeat and a watchdog, so a poll that stalls is visible.
        //
        // Shares the 1 s gate with the publish above, and deliberately does NOT
        // reset `wifi_last_ms` a second time: the two were one poll originally
        // and splitting them into two gates would double the FFI traffic for no
        // extra freshness.
        if radio_due {
            if let Some(radio) = sta.as_mut() {
                if radio.poll() {
                    // The C++'s `networkCoordinator_->setOfflineMode`. The
                    // machine keeps serving on the LAN, which is the point of
                    // offline mode.
                    warn!("wifi: offline mode — unreachable off-LAN");
                }
            }
        }

        // ---- 8b. MQTT: the network tier's second half -------------------------
        //
        // Beside the radio, because that is where it is in the C++:
        // `LoopManager::updateNetwork` (`LoopManager.cpp:485-508`) does the Wi-Fi
        // maintenance and then `checkConnection` + `loop` +
        // `writeSysParamsToMQTT`, all from the one loop.
        //
        // **After** the telemetry publish above, deliberately: `/api/status`
        // should report the session as it was when the tick began, and a
        // `writeSysParamsToMQTT` that pushed a parameter would otherwise make
        // `mqttConnected` and the state it just described disagree by one tick.
        //
        // **After** the actuator work and the deadman above it, for the reason
        // the whole budget exists: this is the only step in the tick that talks
        // to anything outside the chip, and it is bounded by `TIME_BUDGET_MS` --
        // 2 ms of this loop's 10 ms period. A machine with 46 registered topics
        // and a slow broker takes a few ticks to finish a pass and never
        // overruns one. See `mqtt::TIME_BUDGET_MS` for why the C++'s 10 ms was
        // not carried over.
        if let Some(link) = mqtt.as_mut() {
            let mut mqtt_effects = cc_machine::Effects::new();
            let live = mqtt_link::Live {
                temperature_c: last_reading.map_or(f64::NAN, |(celsius, _)| celsius),
                heater_power_pct: f64::from(control.pid_output()) / 10.0,
                standby_remaining_ms: machine.standby.remaining_ms,
                // The ABP2's **last decoded sample**, not this tick's poll
                // result. `sensorCoordinator().getFilteredPressure()`
                // (`MQTTManager.cpp:788`) is the coordinator's *cached*
                // reading, so the C++ publishes the last sample too -- and the
                // driver's own cadence is 50 ms (`abp2::CADENCE`), so a poll
                // result is `NotReady` on nineteen ticks out of twenty and the
                // topic would have been published for one tick in a hundred.
                pressure_bar: pressure
                    .as_ref()
                    .and_then(cc_hal_esp32::Abp2Pressure::last_sample)
                    .map(|sample| f64::from(sample.pressure.raw())),
                weight_g,
                brew_weight_g: 0.0,
                water_tank_full: tank_full,
                backflush_reminder_due: backflush_due,
                pid_gains: control.pid_gains(),
            };
            let report = link.service(
                &mut config,
                &mut control,
                &mut store,
                &mut mqtt_effects,
                &mut scale_modes,
                sampler.as_ref(),
                &live,
                uptime,
            );
            if report.cut_short || report.unresolved > 0 {
                // Logged only when something is worth saying. A tick that
                // publishes thirty topics and says so, a hundred times a
                // second, is a log that costs the loop its budget.
                debug!(
                    "mqtt: published {}, {} unresolved topic(s), cut short: {}",
                    report.published, report.unresolved, report.cut_short
                );
            }
            // The inbound commands' effects. A **fresh** list, not the tick's:
            // the tick's own effects were applied ten steps ago and applying them
            // a second time would drive the actuators from a stale event list.
            // An inbound `steamON` has to reach the applier in the tick that
            // produced it, or the steam valve is ten milliseconds behind a
            // Home Assistant switch.
            if !mqtt_effects.is_empty() {
                // The machine **after** the inbound commands, not the copy taken
                // for the telemetry publish above: an inbound `steamON` has
                // already moved it, and an effect list read against the state
                // before its own command is how a valve ends up open for a state
                // that has already left.
                let after = *control.machine();
                // **And the facade is told the new state first.** `Actuators`
                // caches the machine state for the interlocks -- `may_open_steam`
                // is a whitelist over it -- and the tick set that cache before
                // `control.tick`. An inbound `steamON` moves the machine into
                // `SteamRunning`, and without this the steam valve would be
                // refused by the very interlock it was just asked to satisfy.
                actuators.set_state(after.state);
                cc_machine::apply(&mut actuators, &mut side, &after, &mqtt_effects);
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
                 achieved period {} ms of a {} ms target \
                 — scale: {}{}",
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
    let spawned = cc_hal_esp32::task::spawn_with_prio(
        c"provision",
        network::PROVISION_THREAD_STACK_BYTES,
        cc_hal_esp32::task::PROVISION_PRIO,
        move || network::run_provisioning(serial, handoff),
    );
    if let Err(err) = spawned {
        warn!("serial: the provisioning task did not start: {err}");
    }
}

/// The load cell's current weight in grams, or `None` when there is none.
///
/// # Why this is a reading and not an event
///
/// A 10 Hz sample is a **snapshot, not a message**: losing one loses nothing,
/// because the next one carries the same information. Routing it through a queue
/// the control tick drains would add a copy and a second writer for no gain. So
/// the weight is read from the shared telemetry at step 4, beside the
/// temperature and the pressure, and it is the *events* — a completed tare, a
/// new calibration factor — that go through [`crate::config_io::drain_scale`],
/// because dropping
/// one of those loses an operator's action.
///
/// # Why it was not read where the events are drained
///
/// It used to be the return value of [`crate::config_io::drain_scale`], which runs at step 7b —
/// **after** `control.tick`. That is the right place for the NVS commit inside
/// it and the wrong place for a number the reducer is about to decide on, and
/// the result was that `Sensors::brew_weight` could not be filled from it at
/// all: `cc-firmware/src/main.rs` wrote the literal `0.0`.
///
/// That made the by-weight stop condition in
/// `BrewRunningState::checkSpecificTransitions` (`BrewStates.cpp:294-299`)
/// unreachable for every target above 0 g, and with it the weight arm of
/// `recordBrewIfQualified` (`BrewStates.cpp:311`). The Rust was *worse off than
/// the C++* here: `cc-hal-esp32::Sampler` measures a real weight and publishes
/// it to `/api/status` and MQTT, and the one consumer that acts on it was
/// handed a zero. The C++ has the same hole for a different reason — its scale
/// is never constructed at all (09 §23), so `getBrewWeight()` is 0 there
/// unconditionally — which makes this a case where matching the oracle's
/// *outcome* was the wrong target and its *structure* was right.
///
/// `None` when no scale is fitted, when the cell has produced nothing yet, or
/// when it is not answering. All three publish as `null` rather than as 0 g,
/// because 0 g is a real weight and a UI showing it shows a full cup.
fn scale_weight(sampler: Option<&cc_hal_esp32::Sampler>) -> Option<f64> {
    sampler.and_then(|sampler| sampler.telemetry().weight_g())
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
    // **Lengths only, never the values.** A machine that will not associate is
    // almost always holding a credential that is not the one it thinks it has, and
    // "the stored SSID is 12 bytes" settles a class of problem that reading
    // `/api/parameters` cannot reach when the machine is offline. `password` is a
    // `Secret` and is exposed only for its length here.
    // The SSID is not a secret — the C++ prints it on every connection attempt
    // too, and it is already in every boot log today — so it is printed to say
    // *which* network is configured, which is the thing a length cannot tell you.
    // The password is never printed at all, only its length.
    info!(
        "wifi: stored credential — ssid {:?} ({} bytes), password {} bytes",
        config.system.wifi.ssid,
        config.system.wifi.ssid.len(),
        config.system.wifi.password.expose().len(),
    );
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
