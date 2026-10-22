//! The firmware binary: the entry point and the module map.
//!
//! - [`bring_up`] drives every actuator inactive, reads the pins back, loads the
//!   configuration and starts the tasks.
//! - [`control_task`] is the 10 ms loop: the only writer of a relay.
//! - [`display_task`] owns the panel; [`network`] the HTTP, telnet and UART tiers.

mod bring_up;
mod config_io;
mod control;
mod control_task;
mod display_task;
mod mqtt_link;
mod network;
mod probe;
/// Why there is no sensor task: a measured kernel defect, not an oversight.
mod sensor_task;
mod slots;

use core::error::Error;

use log::info;

type EspError = esp_idf_svc::sys::EspError;

/// ESP-IDF version this binary was compiled against, as a coarse string. Logged so
/// a device is never diagnosed against the wrong IDF version, which `justfile`'s
/// `ESP_IDF_VERSION` pins. `esp-idf-sys` emits `esp_idf_version_at_least_X_Y_Z`
/// cfgs rather than a version string
/// (`esp-idf-sys-0.38.1/build/common.rs:239`); the exact version is the first line
/// of the ESP-IDF boot banner anyway.
const IDF_VERSION: &str = if cfg!(esp_idf_version_at_least_6_0_0) {
    ">= 6.0.0 (UNEXPECTED: the toolchain pins v5.5.5)"
} else if cfg!(esp_idf_version_at_least_5_5_0) {
    "5.5.x"
} else if cfg!(esp_idf_version_at_least_5_1_0) {
    "5.1.x (UNEXPECTED: the toolchain pins v5.5.5)"
} else {
    "UNEXPECTED: older than 5.1"
};

/// Entry point. The `esp-idf-sys` `binstart` feature provides the C `main` that
/// calls this.
///
/// It does almost nothing: it hands the whole startup sequence to
/// [`bring_up::bring_up`] on a thread whose stack is [`BRING_UP_STACK_BYTES`], and joins
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
    info!("Clever Coffee Rust firmware: bring-up");
    info!("target: xtensa-esp32-espidf, ESP-IDF: {IDF_VERSION}");

    // `Box<dyn Error>` is not `Send`, so the thread's return type is the
    // message rather than the error value, and the message is carried back as
    // an `io::Error` because that is how a `String` becomes a `Box<dyn Error>`.
    // The chain is flattened here, at the one place a boot failure is reported;
    // everything downstream of it is a single line on the console anyway.
    let outcome = std::thread::Builder::new()
        .name("bring-up".into())
        .stack_size(BRING_UP_STACK_BYTES)
        .spawn(|| bring_up::bring_up().map_err(|err| std::io::Error::other(format!("{err}"))))
        .map_err(|err| -> Box<dyn Error> { err.into() })?
        .join()
        .map_err(|_| -> Box<dyn Error> { "the bring-up task panicked".into() })?;
    outcome.map_err(|err| -> Box<dyn Error> { err.into() })
}

/// Startup-task stack budget derived from diagnostic DWARF frames.
///
/// Deepest path: bring-up 2400 B, config bring-up 1584 B, blob serde 3088 B,
/// five nested deserialize frames at ~450 B, and float parsing 1712 B: about 11 KB.
/// Reserve 16 KB from the DRAM heap. Recheck `xtensa-esp32-elf-objdump --dwarf=frames`
/// after changing that path; diagnostic and release use the same code generation.
const BRING_UP_STACK_BYTES: usize = 16 * 1024;
