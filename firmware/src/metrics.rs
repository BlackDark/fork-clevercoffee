//! Values for `GET /api/metrics`. Counters bump where their event happens and are only
//! read by the handler: relaxed atomics, no lock, no allocation; counts run from boot.
//! Firmware version and reset reason are read when the reply is built.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use cc_api::Live;
use cc_core::machine::TICK_MS;

/// Longest gap between two control-loop tick starts, µs.
static LOOP_GAP_MAX_US: AtomicU32 = AtomicU32::new(0);
/// Slowest control-loop tick body, µs.
static LOOP_TICK_MAX_US: AtomicU32 = AtomicU32::new(0);
/// Ticks that came more than two periods after the previous one.
static LOOP_LATE_TICKS: AtomicU32 = AtomicU32::new(0);
static TSIC_GOOD_FRAMES: AtomicU32 = AtomicU32::new(0);
static TSIC_BAD_FRAMES: AtomicU32 = AtomicU32::new(0);
/// Time since boot (ms, low 32 bits) of the last decoded TSIC frame; 0 before the first.
static TSIC_LAST_GOOD_MS: AtomicU32 = AtomicU32::new(0);
static DS18B20_READ_ERRORS: AtomicU32 = AtomicU32::new(0);
static SETTINGS_SAVES: AtomicU32 = AtomicU32::new(0);
static SETTINGS_SAVE_FAILURES: AtomicU32 = AtomicU32::new(0);
/// Result of the last settings save: see [`SAVE_NONE`], [`SAVE_OK`], [`SAVE_FAILED`].
static SETTINGS_LAST_SAVE: AtomicU32 = AtomicU32::new(SAVE_NONE);
/// Bytes pushed to the log ring once it exists, wraps after 4 GiB.
static LOG_BYTES: AtomicU32 = AtomicU32::new(0);

const SAVE_NONE: u32 = 0;
const SAVE_OK: u32 = 1;
const SAVE_FAILED: u32 = 2;

/// Saturates: a gap of over an hour is reported as the maximum.
fn us_u32(us: u64) -> u32 {
    u32::try_from(us).unwrap_or(u32::MAX)
}

/// The control loop reports its tick: gap since the previous tick start, and body time.
pub fn control_tick(gap_us: u64, body_us: u64) {
    LOOP_GAP_MAX_US.fetch_max(us_u32(gap_us), Relaxed);
    LOOP_TICK_MAX_US.fetch_max(us_u32(body_us), Relaxed);
    if gap_us > 2 * TICK_MS * 1000 {
        LOOP_LATE_TICKS.fetch_add(1, Relaxed);
    }
}

/// A TSIC frame decoded; `now_ms` is the uptime.
pub fn tsic_good(now_ms: u64) {
    TSIC_GOOD_FRAMES.fetch_add(1, Relaxed);
    TSIC_LAST_GOOD_MS.store(now_ms as u32, Relaxed);
}

pub fn tsic_bad() {
    TSIC_BAD_FRAMES.fetch_add(1, Relaxed);
}

pub fn ds18b20_read_error() {
    DS18B20_READ_ERRORS.fetch_add(1, Relaxed);
}

/// One settings save run, successful or not.
pub fn settings_save(ok: bool) {
    if ok {
        SETTINGS_SAVES.fetch_add(1, Relaxed);
        SETTINGS_LAST_SAVE.store(SAVE_OK, Relaxed);
    } else {
        SETTINGS_SAVE_FAILURES.fetch_add(1, Relaxed);
        SETTINGS_LAST_SAVE.store(SAVE_FAILED, Relaxed);
    }
}

pub fn log_line(bytes: usize) {
    LOG_BYTES.fetch_add(bytes as u32, Relaxed);
}

/// The values `/api/metrics` reports, as of now. The MQTT diagnostics use the same.
#[cfg(feature = "mqtt")]
pub fn live_now() -> Live {
    use crate::shared;
    let heap = esp_alloc::HEAP.stats();
    let mut live = shared::with(|s| Live {
        mqtt_connected: s.net.mqtt_connected,
        wifi_reconnects: s.net.reconnects,
        ..s.live
    });
    live.free_heap = (heap.size - heap.current_usage) as u32;
    live.min_free_heap = shared::MIN_FREE_HEAP.load(Relaxed);
    live.min_free_stack = crate::stackmon::last();
    fill(&mut live);
    live
}

/// Copies the counters into the reply. Reads only: no formatting, no allocation.
pub fn fill(live: &mut Live) {
    live.loop_gap_max_us = LOOP_GAP_MAX_US.load(Relaxed);
    live.loop_tick_max_us = LOOP_TICK_MAX_US.load(Relaxed);
    live.loop_late_ticks = LOOP_LATE_TICKS.load(Relaxed);
    live.tsic_good_frames = TSIC_GOOD_FRAMES.load(Relaxed);
    live.tsic_bad_frames = TSIC_BAD_FRAMES.load(Relaxed);
    live.tsic_last_good_ms = TSIC_LAST_GOOD_MS.load(Relaxed);
    live.ds18b20_read_errors = DS18B20_READ_ERRORS.load(Relaxed);
    live.settings_saves = SETTINGS_SAVES.load(Relaxed);
    live.settings_save_failures = SETTINGS_SAVE_FAILURES.load(Relaxed);
    live.settings_last_save = SETTINGS_LAST_SAVE.load(Relaxed);
    live.log_bytes = LOG_BYTES.load(Relaxed);
    live.firmware_version = env!("CARGO_PKG_VERSION");
    // esp-hal's reset code (0 when the chip does not report one).
    live.reset_reason = esp_hal::system::reset_reason().map_or(0, |r| r as u32);
}
