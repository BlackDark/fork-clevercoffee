//! ESP-IDF network services: HTTP API and UI, SSE, OTA, MQTT, telnet log, Wi-Fi
//! station and UART provisioning.
//!
//! The protocol logic is in `cc-net`; this crate is the ESP-IDF transport for it.
//! Task, heap and clock primitives come from `cc-hal-esp32`. Validate with
//! `just lint-esp32`. [`web_async`] detaches SSE requests so the single httpd task
//! can keep serving; its cross-task writes depend on `CONFIG_HTTPD_VALIDATE_REQ`
//! staying disabled.

#![no_std]
// Test bodies are `pub fn` so `device_tests::CASES` can name them; see
// `cc-hal-esp32`'s crate root for the same exception.
#![cfg_attr(
    test,
    allow(
        clippy::missing_panics_doc,
        missing_docs,
        reason = "the test bodies' assertions are the test; neither lint is \
                  waived outside cfg(test)"
    )
)]

extern crate alloc;
// `std::sync::Mutex` for the snapshots the HTTP handlers read.
extern crate std;

#[cfg(feature = "device-tests")]
#[doc(hidden)]
pub mod device_tests;

pub mod mqtt;
pub mod ota;
pub mod provisioning;
pub mod telnet;
pub mod web;
pub mod web_async;
pub mod wifi;

pub use web::{Shared, Web};
pub use wifi::Sta;
