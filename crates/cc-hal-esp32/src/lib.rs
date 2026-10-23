//! ESP-IDF drivers, pin ownership, and device-side interlocks, plus the task,
//! heap and clock primitives `cc-net-esp32` builds on.
//!
//! [`pins`] validates legal mappings at compile time and actual wiring at boot.
//! [`actuators::Actuators`] owns pump and valve control; [`heater`] owns the heater
//! pin. Interlocks belong inside their methods.
//!
//! This crate requires the device toolchain; validate with `just lint-esp32`.
//! FFI and `Send` exceptions carry local safety arguments.

#![no_std]
// Test bodies are `pub fn` so `device_tests::CASES` can name them. Under the
// `device-tests` feature they are `doc(hidden)` and skip the doc lints; under
// plain `cfg(test)` (an IDE, `cargo clippy --all-targets`) they would not.
#![cfg_attr(
    test,
    allow(
        clippy::missing_panics_doc,
        missing_docs,
        reason = "the test bodies' assertions are the test; neither lint is \
                  waived outside cfg(test)"
    )
)]

// `Arc`, for the value the heater ISR and the control task share.
extern crate alloc;
// `std::sync::Mutex`; the firmware is a `std` binary, so this links nothing new.
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
pub mod nvs;
pub mod onewire;
pub mod pins;
pub mod restart;
pub mod scale;
pub mod sensors;
pub mod switches;
pub mod task;
pub mod time;
pub mod zacwire;

pub use actuators::{Actuators, FirmwareSide, Inhibit, Interlock, ValveState};
pub use display::{Oled, PanelOled, FRAMEBUFFER_LEN, REFRESH_INTERVAL_MS};
pub use heap::{free_heap, min_free_heap};
pub use heater::{HeaterOutput, TimerIsrPwm, CARRIER_HZ, RESOLUTION};
pub use nvs::EspNvsBlob;
pub use onewire::GpioOneWire;
pub use restart::{drain_console, restart_now};
pub use scale::{GpioHx711, Sampler, SamplerCommand, SamplerEvent};
pub use sensors::{Abp2I2c, Abp2Pressure, GpioIn};
pub use switches::{Levels, SwitchBank};
pub use time::now_ms;
pub use zacwire::ZacwireCapture;
