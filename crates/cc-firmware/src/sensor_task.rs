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
//! On the original ESP32 [`esp_idf_hal::interrupt::free`] enters one
//! process-global `static CS` via `xPortEnterCriticalTimeout`
//! (`esp-idf-hal-0.47.0/src/interrupt.rs`). The display frame, telemetry, and
//! the HX711 shift use it too. The display task's use was clean in the bisect
//! above. esp-idf-hal's own comment on a second core:
//!
//! > *"the second core will then spinlock (busy-wait) in
//! > `IsrCriticalSection::enter`, until the first CPU releases the critical
//! > section"*
//!
//! A scratchpad read enters that lock once per GPIO write. The wait sits
//! outside it (`onewire.rs` `pulse_low`). On one task that matches the C++
//! `noInterrupts()` around the edge. On a second task this build asserts.
//!
//! It is reproducible, it is in the kernel rather than in this workspace, and it
//! is not worth shipping a firmware that trips it. So the temperature probe stays
//! on the control task, where it has run without incident, and the *display* —
//! which the bisect shows is safe — moves out. See
//! `docs/history/cpp-findings.md` for where this finding is recorded.
//!
//! Recheck 2027-01. On 2026-10-09 crate 0.47.0 was still the latest release and
//! `free` still entered that one `CS`. The other path is RMT `OWDriver` in the
//! same crate; its CRC helpers are still `todo`.
