//! Why temperature polling remains on the control task.
//!
//! Moving DS18B20 polling to a separate task caused 3–4 crashes per boot; disabling
//! that polling or starting only the display task did not. Observed failures:
//!
//! ```text
//! assert failed: xTaskRemoveFromEventList tasks.c:3894 (pxUnblockedTCB)
//! Guru Meditation Error: Core  1 panic'ed (LoadProhibited). Exception was unhandled.
//! ```
//!
//! The latter landed in lwIP's `sys_arch_mbox_fetch`. On the original ESP32,
//! `esp-idf-hal` 0.47.0 uses one process-global critical section in
//! `interrupt::free`; scratchpad reads enter it once per GPIO write. The same
//! polling runs without incident on the control task. The precise kernel failure
//! mechanism is not established.
//!
//! Recheck in 2027-01 before moving the probe. On 2026-10-09 the HAL still used
//! that shared lock, and the alternative RMT driver's CRC helpers were unfinished.
