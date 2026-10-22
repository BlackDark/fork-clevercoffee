//! ESP-IDF transport, pin ownership, and device-side interlocks.
//!
//! [`pins`] validates legal mappings at compile time and actual wiring at boot.
//! [`actuators::Actuators`] owns pump and valve control; [`heater`] owns the heater
//! pin. Interlocks belong inside their methods.
//!
//! This crate requires the device toolchain; validate with `just lint-esp32`.
//! FFI and `Send` exceptions carry local safety arguments. In particular,
//! [`web_async`] detaches SSE requests so the single httpd task can keep serving;
//! its cross-task writes depend on `CONFIG_HTTPD_VALIDATE_REQ` staying disabled.

#![no_std]
#![deny(clippy::pedantic)]
// workspace lints already do this; restated per crate
// The doc lints do not apply to this crate's own TEST bodies.
//
// `cc-hal-esp32`'s tests live in `#[cfg(any(test, feature = "device-tests"))]`
// modules and are `pub fn` (not `#[test]`), because the on-target runner in
// `cc-device-tests` registers them by name -- that is the whole point of
// `scripts/device-test-audit.py`. When the crate is compiled with the
// `device-tests` FEATURE they carry `#[cfg_attr(feature = "device-tests",
// doc(hidden))]`, and rustdoc's lints skip hidden items, which is why
// `just lint-esp32` has always been clean.
//
// Compiled as `-p cc-hal-esp32 --all-targets` WITHOUT that feature -- which any
// IDE, a `cargo clippy -p <crate>` invocation, or a future recipe would do -- the
// modules are reached through `cfg(test)` alone, `doc(hidden)` is not applied,
// and 282 `clippy::missing_panics_doc` errors appear on functions whose only
// body is an assertion. That is a real build break hiding behind a coincidence
// in one recipe, and it is this line that closes it.
//
// Scoped to `cfg(test)` on purpose: the firmware's own code keeps every doc lint.
#![cfg_attr(
    test,
    allow(
        clippy::missing_panics_doc,
        // `missing_docs` is the workspace's, and it is on these items only
        // because they are `pub fn` inside a `#[cfg(test)]` module -- which is
        // what lets the on-target runner register them by name.
        missing_docs,
        reason = "the test bodies' assertions ARE the test; a # Panics section \
                  and a doc comment on 140 assertion wrappers would be noise. \
                  Both lints are denied workspace-wide for the firmware's own \
                  code and neither is waived outside cfg(test)"
    )
)]

// `Arc`, for the one place the heater ISR and the control task must share a
// value. `alloc` is already linked (`esp-idf-hal`'s `std` feature pulls it in
// and the firmware is a `std` binary), so this costs nothing; what it would cost
// is `&mut` across a thread boundary, which is `unsafe` and is not available.
extern crate alloc;

// `std` for exactly one thing: `std::sync::Mutex`, which the HTTP handlers use
// to read the telemetry snapshot the control task publishes. `esp-idf-svc` is
// built with its `std` feature and the firmware is a `std` binary, so this
// links nothing new -- but it does mean this crate is not `no_std`, and the
// alternative (a hand-rolled spin lock, or `critical-section`, which
// is a FreeRTOS recursive
// mutex and therefore not ISR-safe) is worse.
extern crate std;

#[cfg(feature = "device-tests")]
#[doc(hidden)]
pub mod device_tests;

pub mod actuators;
pub mod display;
pub mod display_shared;
pub mod heap;
pub mod heater;
pub mod leds;
pub mod mqtt;
pub mod nvs;
pub mod onewire;
pub mod ota;
pub mod pins;
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
pub use heater::{HeaterOutput, TimerIsrPwm, CARRIER_HZ, RESOLUTION};
pub use nvs::EspNvsBlob;
pub use onewire::GpioOneWire;
pub use restart::{drain_console, restart_now};
pub use scale::{GpioHx711, Sampler, SamplerCommand, SamplerEvent};
pub use sensors::{Abp2I2c, Abp2Pressure, GpioIn};
pub use switches::{Levels, SwitchBank};
pub use task::CommandQueue;
pub use time::now_ms;
pub use web::{Shared, Web};
pub use wifi::Sta;
pub use zacwire::ZacwireCapture;
