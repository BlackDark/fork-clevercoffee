//! The on-target unit test runner for `cc-hal-esp32`. **Never the firmware.**
//!
//! # What this binary is
//!
//! The 67 `#[test]` functions in `cc-hal-esp32` were type-checked by
//! `just lint-esp32` and executed by **nothing**. `cargo test` cannot reach that
//! crate — it names `esp_idf_hal`, so it does not build for a host target, and
//! `just test` lists only the portable crates. Two real device bugs shipped
//! through that gap: a Wi-Fi provisioning password window that lasted zero
//! milliseconds, and console lines lost across `esp_restart()`.
//!
//! This binary closes the gap. `main` runs the registered cases in
//! [`cc_hal_esp32::device_tests::CASES`], one line of result per case over
//! UART0, and `scripts/device-test-audit.py` (`just test-audit`) fails the build
//! if the registry and the `#[test]` markers ever disagree.
//!
//! # Why the harness is hand-rolled rather than `cargo test` over the link
//!
//! `cargo test` for this target would build libtest, and libtest reports a
//! failure by **catching** a panic. The `esp` toolchain builds this target with
//! `panic = "abort"` — `unwinding` is not supported on Xtensa LX6 here, and
//! `-Zbuild-std=std,panic_unwind` does not link (verified: `can't find crate for
//! panic_abort`). So a failing `assert!` aborts the chip instead of unwinding.
//!
//! The runner is built around that fact rather than fighting it:
//!
//! * a `std::panic` hook reports the failing case, then calls
//!   [`cc_hal_esp32::restart::restart_now`];
//! * the resume index lives in NVS, so the next boot continues after the case
//!   that died instead of looping on it forever;
//! * `scripts/device-tests.py` reconstructs the outcome from the line stream, so
//!   the pass/fail accounting never depends on a boot that did not finish.
//!
//! # The one thing this binary must never do
//!
//! **Drive an actuator.** It runs on the real machine, not an emulator. It
//! configures the pump, valve and heater pins exactly as the firmware's own
//! startup does — output, inactive level, read back, and refuse to continue if
//! any of them reads active — and it never calls anything that opens the heater
//! gate. No test in `cc-hal-esp32` touches a peripheral; they are pure logic,
//! and the registry is the audited list of them. The boot log carries a
//! `CCTEST actuator ... INACTIVE` line and the host runner fails the run if it
//! is absent.

use std::error::Error;
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use cc_hal_esp32::device_tests::Case;
use esp_idf_hal::gpio::{InputOutput, InputPin, OutputPin, PinDriver, Pull};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::sys::{
    esp_reset_reason, esp_reset_reason_t_ESP_RST_EXT, esp_reset_reason_t_ESP_RST_POWERON, printf,
};
use log::info;

/// Every prefix the host runner parses starts with this. Chosen so it cannot
/// collide with an ESP-IDF log line (`<LEVEL><space>(<ms>)<space><tag>:`) and
/// cannot be produced by any firmware string.
const TAG: &str = "CCTEST";

/// The NVS namespace and key holding the resume index.
const CKPT_NAMESPACE: &str = "cctest";
/// The resume index: the index of the case to run first on this boot.
const CKPT_KEY: &str = "next";

/// The case currently running, for the panic hook. `usize::MAX` = "none".
static CURRENT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// The name of the case currently running, for the panic hook.
///
/// A `Mutex` rather than a bare `static mut`: the panic hook runs on whichever
/// thread panicked and must not race the runner. `Mutex::new` is const, so this
/// is not a lazy initialisation.
static CURRENT_NAME: Mutex<&'static str> = Mutex::new("<none>");

/// A marker written through **libc's** buffered `stdout` immediately before the
/// reboot, with no explicit flush.
///
/// This is the half of the console-drain bug that `fflush(NULL)` exists for, and
/// it is the half a Rust `println!` cannot reproduce: `std::io::stdout` writes
/// straight to fd 1 and never sits in libc's `FILE` buffer. The host runner
/// requires this line to arrive; without the `fflush` in
/// [`cc_hal_esp32::restart::drain_console`] it does not.
const LIBC_BUFFERED_MARKER: &[u8] = b"CCTEST-MARKER-libc-buffered\n";

/// A marker written through **libc** and deliberately **not** flushed, written
/// immediately before a bare `esp_restart()`. Whether it arrives is the
/// observation; see [`REBOOT_UNFLUSHED`].
const UNFLUSHED_LIBC_MARKER: &[u8] = b"CCTEST-MARKER-unflushed\n";

/// The same idea through the other half: written with `println!` as the very
/// last thing before the reset, so the bytes are still in the UART's own TX
/// path. `uart_wait_tx_done(UART0)` is what pushes them out.
const DIRECT_MARKER: &str = "CCTEST-MARKER-direct";

/// The same idea for the unflushed reboot: written straight to fd 1 through
/// Rust, as the last thing before a bare `esp_restart()`.
const UNFLUSHED_DIRECT_MARKER: &str = "CCTEST-MARKER-unflushed-direct";

/// The line the runner prints when it is about to reset the chip **on purpose**.
/// Without it a reboot mid-run is indistinguishable from a crash, and the host
/// would report the case as failed.
const EXPECT_REBOOT: &str = "expect-reboot";

/// The name of the one case that is not a unit test: the end-to-end proof that
/// the console survives a reboot.
const REBOOT_CASE: &str = "runner::the_console_reaches_the_wire_before_a_reboot";

/// The case that reproduces the **pre-fix** firmware, in the same binary.
///
/// It writes both markers and then calls `esp_restart()` directly, with no
/// drain — which is exactly what the firmware did before
/// `cc_hal_esp32::restart::restart_now` existed. Its verdict is decided by the
/// host from what reached the wire, and it is reported as an **observation**
/// rather than a pass or a fail, because "the marker was lost" is the finding
/// and asserting its loss would be a test that passes only while a bug is
/// present. Its whole job is to make [`REBOOT_CASE`] mean something: without
/// the comparison, "both markers arrived" does not distinguish "the drain
/// works" from "the console was never buffered".
const REBOOT_UNFLUSHED: &str = "runner::a_bare_esp_restart_drops_what_was_never_flushed";

fn main() -> Result<(), Box<dyn Error>> {
    // Same first two calls as `cc-firmware`, and for the same reasons: the
    // linker patches and the log sink are process-wide.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::init_from_env();
    info!("CleverCoffee on-target unit test runner");

    let cases = cc_hal_esp32::device_tests::CASES;
    // + the two reboot cases below.
    let total = cases.len() + 2;

    // The actuators, first, before anything else exists — the ordering 04 §4
    // specifies, and the reason is that a test binary on a live machine has to
    // be in the safe state before it starts doing anything at all.
    let _actuators = hold_actuators_inactive()?;

    let checkpoint = Checkpoint::open();
    let start = start_index(checkpoint.as_ref(), total);
    install_panic_hook();

    line(&[
        "begin".to_owned(),
        format!("total={total}"),
        format!("from={start}"),
    ]);

    let mut ran = 0usize;
    for case in cases.iter().enumerate().skip(start) {
        let index = case.0;
        run_case(checkpoint.as_ref(), index, case.1);
        ran += 1;
    }
    // Both reboot cases, in order: the unflushed one first, because the drained
    // one is the assertion and the comparison is what gives it meaning.
    //
    // `start` is the first index to run and the reboot cases sit at
    // `cases.len()` and `cases.len() + 1`, so each runs when `start` is at or
    // below **its own** index. Getting these two the wrong way round makes the
    // unflushed case re-run on every boot and the run never terminates.
    if start <= cases.len() {
        run_reboot_case(checkpoint.as_ref(), cases.len(), REBOOT_UNFLUSHED, false);
        ran += 1;
    }
    if start <= cases.len() + 1 {
        run_reboot_case(checkpoint.as_ref(), cases.len() + 1, REBOOT_CASE, true);
        ran += 1;
    }

    line(&["done".to_owned(), format!("ran={ran}")]);
    info!("on-target unit tests complete: {ran} of {total} ran on this boot");
    Ok(())
}

/// Hold the pump, the water valve and the heater at their inactive level and
/// prove it by reading the pins back.
///
/// Identical in shape to `cc_firmware::bring_up` steps 2-5, and deliberately so:
/// this is a machine that is powered, wired and (per `AGENTS.md`'s hardware
/// rules) possibly full of water, so "the test binary does not use the
/// actuators" is a claim about the code, and this is the claim about the pins.
///
/// `inactive` is `Level::Low`, which is correct for the `HIGH_TRIGGER` relays
/// this board's defaults select. If the board were wired `LOW_TRIGGER` the
/// production firmware would already be driving the boiler on every boot, which
/// is the pre-existing open question (R0-01), not something a test binary
/// changes.
type Actuators = (
    PinDriver<'static, InputOutput>,
    PinDriver<'static, InputOutput>,
    PinDriver<'static, InputOutput>,
);

fn hold_actuators_inactive() -> Result<Actuators, Box<dyn Error>> {
    let peripherals = Peripherals::take().map_err(|_| "the peripherals were already taken")?;
    let valve = drive_inactive(peripherals.pins.gpio17, "water valve")?;
    let pump = drive_inactive(peripherals.pins.gpio27, "pump")?;
    let heater = drive_inactive(peripherals.pins.gpio2, "heater")?;

    for (name, is_inactive) in [
        ("water valve", valve.is_low()),
        ("pump", pump.is_low()),
        ("heater", heater.is_low()),
    ] {
        if !is_inactive {
            return Err(format!("startup readback failed: {name} is not inactive").into());
        }
    }

    info!("pin readback OK: valve=GPIO17 pump=GPIO27 heater=GPIO2 all inactive");
    line(&[
        "actuator".to_owned(),
        "valve=GPIO17".to_owned(),
        "LOW".to_owned(),
        "pump=GPIO27".to_owned(),
        "LOW".to_owned(),
        "heater=GPIO2".to_owned(),
        "LOW".to_owned(),
        "INACTIVE".to_owned(),
    ]);
    Ok((valve, pump, heater))
}

/// Configure one pin as an output and drive it to its inactive level.
fn drive_inactive<'d, P>(
    pin: P,
    name: &str,
) -> Result<PinDriver<'d, InputOutput>, esp_idf_svc::sys::EspError>
where
    P: InputPin + OutputPin + 'd,
{
    let mut driver = PinDriver::input_output(pin, Pull::Floating)?;
    driver.set_level(esp_idf_hal::gpio::Level::Low)?;
    info!("{name} driven inactive");
    Ok(driver)
}

/// Run one registered case and report it.
///
/// The index is written to NVS **before** the case runs, not after it passes.
/// That ordering is the whole resume mechanism: after an abort the stored index
/// is the case that died, so the next boot starts after it. Storing on success
/// instead would leave the stored index pointing at the *last case that
/// finished*, and the failing one would be re-run forever.
fn run_case(checkpoint: Option<&Checkpoint>, index: usize, case: &Case) {
    CURRENT.store(index, Ordering::Relaxed);
    *CURRENT_NAME.lock().expect("the name lock is not poisoned") = case.name;
    if let Some(checkpoint) = checkpoint {
        checkpoint.store(index);
    }

    let started = cc_hal_esp32::time::now_ms();
    line(&["run".to_owned(), format!("n={index}"), case.name.to_owned()]);
    (case.run)();
    let elapsed = cc_hal_esp32::time::now_ms().wrapping_sub(started);

    line(&[
        "ok".to_owned(),
        format!("n={index}"),
        case.name.to_owned(),
        format!("ms={elapsed}"),
    ]);
    info!("test {index} {} PASSED", case.name);
    CURRENT.store(usize::MAX, Ordering::Relaxed);
}

/// The one case that is not a unit test: prove the console survives a reboot.
///
/// Writes two markers through the two different paths a caller can use, both of
/// which are still un-flushed at the instant of the reset, then resets. The host
/// runner fails this case unless **both** markers are in the byte stream it
/// captured before the chip went down — which is the whole of the
/// `esp_restart()` bug, observed rather than asserted.
///
/// It is the last case because it ends the boot.
#[allow(
    unsafe_code,
    reason = "Two libc calls with no pointer argument and nothing to return: \
              `printf` on a NUL-terminated literal with no `%` in it, so it is a \
              format string with no conversions; and `esp_restart()`, which is \
              `void (void)`, allocates nothing, and does not return. The second \
              is the point of the `drained == false` arm: it reproduces the \
              pre-fix firmware, which called `esp_restart()` directly."
)]
fn run_reboot_case(
    checkpoint: Option<&Checkpoint>,
    index: usize,
    name: &'static str,
    drained: bool,
) {
    CURRENT.store(index, Ordering::Relaxed);
    *CURRENT_NAME.lock().expect("the name lock is not poisoned") = name;
    if let Some(checkpoint) = checkpoint {
        checkpoint.store(index);
    }

    line(&["run".to_owned(), format!("n={index}"), name.to_owned()]);

    // 1. Through libc's buffered `stdout`. No flush of any kind.
    //
    // SAFETY: `printf(const char *, ...) -> int`. The argument is a
    // NUL-terminated string literal with **no** `%` in it, so it is a format
    // string with no conversions and no variadic arguments are required. It
    // writes to `stdout`, which is a valid stream on this target.
    unsafe {
        printf(
            if drained {
                LIBC_BUFFERED_MARKER.as_ptr()
            } else {
                UNFLUSHED_LIBC_MARKER.as_ptr()
            }
            .cast(),
        );
    }
    // 2. Straight to fd 1 through Rust, as the last thing before the reset.
    println!(
        "{}",
        if drained {
            DIRECT_MARKER
        } else {
            UNFLUSHED_DIRECT_MARKER
        }
    );

    line(&[
        EXPECT_REBOOT.to_owned(),
        format!("n={index}"),
        name.to_owned(),
        if drained { "drained=1" } else { "drained=0" }.to_owned(),
    ]);
    info!(
        "resetting now; the markers above must {} arrive before the chip goes down",
        if drained { "BOTH" } else { "be watched for" }
    );

    if drained {
        // The fix under test. Without the drain in here, one or both of the
        // markers above never reach the wire.
        cc_hal_esp32::restart::restart_now();
    }
    // SAFETY: `esp_restart()` is `void esp_restart(void)`: no arguments, no
    // allocation, no lock, callable from any task, and it does not return. This
    // is deliberately the *wrong* call — it is the pre-fix firmware's call, kept
    // in the test binary so the comparison can be made on the same chip, in the
    // same run, with the same bytes.
    unsafe {
        esp_idf_svc::sys::esp_restart();
    }
}

/// Install the panic hook that turns an aborted case into a report.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let index = CURRENT.load(Ordering::Relaxed);
        // `PoisonError::into_inner` rather than `expect`: a panic in one case
        // must not stop the panic hook itself from reporting.
        let name = *CURRENT_NAME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        let location = info
            .location()
            .map_or_else(|| "<unknown>".to_owned(), ToString::to_string);

        // `panic = "abort"`, so this hook is the last thing that runs. Report
        // first, flush, then reset — and `restart_now` flushes again, which is
        // exactly the code path the last case is testing.
        line(&[
            "panic".to_owned(),
            format!("n={index}"),
            name.to_owned(),
            format!("at={location}"),
            format!("msg={}", one_line(message)),
        ]);
        let _ = std::io::stdout().flush();
        cc_hal_esp32::restart::restart_now();
    }));
}

/// Where this boot starts in [`cc_hal_esp32::device_tests::CASES`].
///
/// Two inputs and nothing else:
///
/// * the **reset reason**, which distinguishes "the host started a run" — a
///   power-on reset from the DTR/RTS pulse — from "the chip died mid-run";
/// * the resume index in NVS, which says *which* case was running when it did.
///
/// The rule is **fresh unless the reset came from the chip itself**: anything
/// other than `ESP_RST_POWERON` or `ESP_RST_EXT` resumes. Narrowing that to
/// `ESP_RST_SW` looks right and is wrong. `esp_restart()` sets the reset-state
/// register so `esp_reset_reason()` reports `SW`, but `abort()` reaches
/// `esp_restart_noos()`, which does not — verified on the device, where the one
/// case that took the chip down with a 192 KB allocation printed `rst:0xc` on
/// the console and was *not* seen as `SW`, so the suite restarted from zero
/// instead of resuming. One failure became an endless loop.
///
/// The alternative — a marker the host writes over the serial link — would make
/// the device's behaviour depend on a line arriving, and a run whose reset
/// raced the host's write would skip a case silently.
#[allow(
    unsafe_code,
    reason = "One register read: `esp_reset_reason()` is `esp_reset_reason_t \
              esp_reset_reason(void)`, takes no pointer, allocates nothing, and \
              is safe from any task. The alternative is inferring the reset kind \
              from side effects, which is how a stale checkpoint silently skips \
              a case."
)]
fn start_index(checkpoint: Option<&Checkpoint>, total: usize) -> usize {
    // SAFETY: `esp_reset_reason()` is declared
    // `esp_reset_reason_t esp_reset_reason(void)` in `esp_system.h`. It reads a
    // hardware register, takes no pointer, allocates nothing, and is safe from
    // any task at any point in the process lifetime -- which is the only place
    // this is called from.
    let reason = unsafe { esp_reset_reason() };
    if reason == esp_reset_reason_t_ESP_RST_POWERON || reason == esp_reset_reason_t_ESP_RST_EXT {
        // The host, or a person, started this run. Overwrite whatever is in NVS
        // rather than reading it, so a half-finished previous run cannot make
        // this one skip a case.
        if let Some(checkpoint) = checkpoint {
            checkpoint.store(0);
        }
        return 0;
    }

    let next = checkpoint.map_or(0, Checkpoint::load);
    line(&[
        "resume".to_owned(),
        format!("after={next}"),
        format!("reason={}", reason as u32),
    ]);
    next.saturating_add(1).min(total)
}

/// The resume index, in NVS.
///
/// NVS rather than `.rtc_noinit`: NVS survives every kind of reset by
/// definition, and `.rtc_noinit` does not — ESP-IDF's reset path puts the RTC
/// domain through `esp_restart_noos_dig()`, so whether an RTC-resident counter
/// survives `esp_restart()` is a property of a vendor linker script rather than
/// something to bet the runner on. One `u32`, written once per case.
struct Checkpoint(EspDefaultNvs);

impl Checkpoint {
    /// Open the checkpoint, or `None` if NVS is unusable.
    ///
    /// `None` degrades the runner to "start at zero every boot", which turns a
    /// failing case into a loop rather than a progress. That is still better than
    /// refusing to run: the host runner sees the same case start twice and
    /// reports it.
    fn open() -> Option<Self> {
        let partition = EspDefaultNvsPartition::take().ok()?;
        let nvs = EspNvs::new(partition, CKPT_NAMESPACE, true).ok()?;
        Some(Self(nvs))
    }

    /// The stored index, or `0` if absent or unreadable.
    fn load(&self) -> usize {
        self.0.get_u32(CKPT_KEY).ok().flatten().unwrap_or(0) as usize
    }

    /// Store the index. A failure is logged, not propagated: the worst outcome
    /// is that one failing case becomes a loop, which the host runner reports.
    fn store(&self, index: usize) {
        // Bounded by the registry length (68), so the narrowing is exact.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "index < the registry length, which is 68"
        )]
        let index = index as u32;
        if let Err(error) = self.0.set_u32(CKPT_KEY, index) {
            log::warn!("the resume index could not be stored: {error:?}");
        }
    }
}

/// One `CCTEST` line on the console, flushed before returning.
///
/// Flushed explicitly rather than relying on the newline: the whole protocol
/// depends on a line being on the wire before the next thing happens, and the
/// last thing that happens is sometimes a reset.
fn line(fields: &[String]) {
    let mut text = String::from(TAG);
    for field in fields {
        text.push(' ');
        text.push_str(field);
    }
    println!("{text}");
    let _ = std::io::stdout().flush();
}

/// Collapse a panic message into one line so it cannot break the protocol.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}
