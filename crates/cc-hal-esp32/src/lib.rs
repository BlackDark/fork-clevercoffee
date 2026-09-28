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
//! Owner: R3-01. This is the R1-01 workspace skeleton, plus the R1-07 heater
//! output in [`heater`].

#![no_std]
#![deny(clippy::pedantic)] // workspace lints already do this; restated per crate

pub mod heater;

pub use heater::{HeaterDuty, HeaterOutput, LedcPwm, CARRIER_HZ, RESOLUTION};
