//! The one crate allowed to import `esp_idf_{hal,svc,sys}` (04 §6, skill §4).
//!
//! # Rules
//!
//! * One crate, not seven. Splitting per-peripheral would scatter the pin map,
//!   and the pin map is the thing that must be edited in one place.
//! * [`Board`](board) is a trait with one impl per board, and the pin map is a
//!   `const` validated at compile time. A pin that does not exist on the chip
//!   must be a compile error, not a silent miswiring (04 §6 "Feature flags and
//!   target selection"; the C++ equivalent is the 21 `static_assert`s in
//!   `pinmapping.h:57-101`).
//! * `Actuators` is the **only** owner of the pump and the water valve, and
//!   `HeaterOutput` the only owner of the heater. Interlocks and the emergency
//!   latch are checked *inside* those methods, not at the call sites, so no
//!   caller can bypass them. The C++ `heaterEnabled_` drift (01 §4) is the bug
//!   this ownership rule removes.
//! * This crate does not compile for a host target. Validate it with
//!   `just lint-esp32`.
//!
//! Owner: R3-01. This is the R1-01 workspace skeleton, the R1-07 heater output in
//! [`heater`], and the R1-03/R3-07 temperature sensors in [`onewire`] and
//! [`zacwire`].
//!
//! # 🔴 Two `unsafe`, and they need a human to ratify them
//!
//! [`zacwire::now_us`] is a single call to ESP-IDF's `esp_timer_get_time()`,
//! behind a narrowly-scoped `#[allow(unsafe_code)]` with the reasoning written
//! out at the call site.
//!
//! [`web_async`] is the second, and it is the one that matters more: three
//! `httpd_*` calls that let the `/events` handler **return** while a separate
//! task does the writing. It exists because ESP-IDF's httpd is a single task
//! (`httpd_main.c:533`), so a handler that holds a stream open is a server that
//! serves nobody — measured on hardware, 55 of 60 concurrent API requests timed
//! out with one browser tab streaming. `esp-idf-svc` 0.53.0 has no safe way to
//! express it, and ESP-IDF's own `httpd_req_async_handler_begin` is the
//! documented answer (`esp_http_server.h:840-873`). The module documents each
//! call, the `Send` claim, and the Kconfig that would invalidate it.
//!
//! The workspace lint is `unsafe_code = "deny"`, so both are `#[allow]`ed with
//! their reasoning at the call site rather than globally.
//!
//! [`zacwire::now_us`] is there because `esp-idf-hal` 0.47 has **no** `esp_timer` module and no
//! safe monotonic clock of any kind — checked in `src/timer.rs` and
//! `src/delay.rs`) — and the `ZACwire` protocol cannot be decoded without
//! microsecond timestamps (04 §5's timing constraint, and the app note's
//! ≥ 128 kHz sampling requirement). The alternatives were all worse: a C shim is
//! the same `unsafe` in another file, a `GPTimer` count read is also `unsafe` FFI
//! *and* a 24-bit wrap to difference by hand, and `std::time::Instant` has no
//! documented clock on this target.
//!
//! If the decision is "no `unsafe`, anywhere", then the honest consequence is
//! that the TSIC-306 driver has no device implementation, and that is a decision
//! to take explicitly rather than by omission.

#![no_std]
#![deny(clippy::pedantic)] // workspace lints already do this; restated per crate

// `Arc`, for the one place the heater ISR and the control task must share a
// value. `alloc` is already linked (`esp-idf-hal`'s `std` feature pulls it in
// and the firmware is a `std` binary), so this costs nothing; what it would cost
// is `&mut` across a thread boundary, which is `unsafe` and is not available.
extern crate alloc;

// `std` for exactly one thing: `std::sync::Mutex`, which the HTTP handlers use
// to read the telemetry snapshot the control task publishes. `esp-idf-svc` is
// built with its `std` feature and the firmware is a `std` binary, so this
// links nothing new -- but it does mean this crate is not `no_std`, and the
// alternative (a hand-rolled spin lock, or `critical-section`, which 04 §3.2
// notes is a FreeRTOS recursive mutex and therefore *not* usable from a task
// that could be preempted by the httpd task) is worse.
extern crate std;

#[cfg(feature = "device-tests")]
#[doc(hidden)]
pub mod device_tests;

pub mod actuators;
pub mod display;
pub mod display_shared;
pub mod heap;
pub mod heater;
pub mod mqtt;
pub mod nvs;
pub mod onewire;
pub mod provisioning;
pub mod restart;
pub mod scale;
pub mod sensors;
pub mod switches;
pub mod task;
pub mod telnet;
pub mod time;
pub mod web;
pub mod web_async;
pub mod wifi;
pub mod zacwire;

pub use actuators::{Actuators, FirmwareSide, Inhibit, Interlock, ValveState};
pub use display::{Oled, PanelOled, FRAMEBUFFER_LEN, REFRESH_INTERVAL_MS};
pub use heap::{free_heap, min_free_heap, HEAP_SHED_BYTES};
pub use heater::{HeaterDuty, HeaterOutput, LedcPwm, TimerIsrPwm, CARRIER_HZ, RESOLUTION};
pub use nvs::EspNvsBlob;
pub use onewire::GpioOneWire;
pub use restart::{drain_console, restart_now};
pub use scale::{GpioHx711, Sampler, SamplerCommand, SamplerEvent};
pub use sensors::{Abp2I2c, Abp2Pressure, GpioIn};
pub use switches::{Levels, SwitchBank};
pub use task::CommandQueue;
pub use time::now_ms;
pub use web::{parameters_json, Shared, Telemetry, Web};
pub use wifi::Sta;
pub use zacwire::ZacwireCapture;
