//! (removed — see the note below)
//!
//! This module held the sensor task. It was removed after it was measured to be
//! unsafe on this toolchain, and the measurement is the reason it is written down
//! here rather than deleted without a word.
//!
//! # What was tried, and what the board said
//!
//! The control loop ran everything in one 400 ms task. The obvious fix — and the
//! one this module implemented — is to give the pins their own task on their own
//! cadences. Built, flashed, and bisected one variable at a time on hardware:
//!
//! | sensor task | display task | crashes |
//! | --- | --- | --- |
//! | not started | not started | **0** |
//! | started, DS18B20 poll **disabled** | not started | **0** |
//! | started, DS18B20 poll **enabled** | not started | 3–4 per boot |
//! | not started | started | **0** |
//! | started | started | 3–4 per boot |
//!
//! Every crash is one of two, both reproducible from the first sensor publish:
//!
//! ```text
//! assert failed: xTaskRemoveFromEventList tasks.c:3894 (pxUnblockedTCB)
//! Guru Meditation Error: Core  1 panic'ed (LoadProhibited). Exception was unhandled.
//! ```
//!
//! The first is inside `xQueueGenericSend`/a critical-section release removing a
//! blocked task from a list whose head has no owner. The second lands in
//! `sys_arch_mbox_fetch` in the **lwIP tcpip thread** — a task that has nothing
//! to do with the firmware, corrupted from outside.
//!
//! # The mechanism, as far as the evidence goes
//!
//! The DS18B20 is the only thing that calls [`esp_idf_hal::interrupt::free`], and
//! on the original ESP32 that is `vPortEnterCritical` on a **process-global**
//! `IsrCriticalSection` (`esp-idf-hal-0.47.0/src/interrupt.rs`: `pub(crate)
//! static CS`). esp-idf-hal's own comment on it says what happens when a second
//! task touches it from the other core:
//!
//! > *"the second core will then spinlock (busy-wait) in
//! > `IsrCriticalSection::enter`, until the first CPU releases the critical
//! > section"*
//!
//! 1-Wire holds that critical section 80-odd times per scratchpad read, for
//! 3–65 µs each. On one task that is unremarkable — it is what the C++ does with
//! `noInterrupts()`. On a second task it is a spinlock that the FreeRTOS port
//! also expects to be able to reschedule through, and this build asserts.
//!
//! It is reproducible, it is in the kernel rather than in this workspace, and it
//! is not worth shipping a firmware that trips it. So the temperature probe stays
//! on the control task, where it has run without incident, and the *display* —
//! which the bisect shows is safe — moves out. See
//! `docs/rust-migration/09-cpp-findings.md` for where this finding is recorded.
