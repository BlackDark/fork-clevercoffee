//! Clever Coffee `device protocols`, as pure state machines over bytes.
//!
//! Owner: finding **4.4** — the split of `cc-domain` into a vocabulary crate and
//! the protocol stacks it had accumulated.
//!
//! # Where this sits in the layering
//!
//! ```text
//!   cc-domain ──> cc-protocol ──> cc-hal-esp32
//! ```
//!
//! Every module here decodes or encodes a **wire format**: the two temperature
//! buses, the pressure sensor's I²C command set, an HTTP `Authorization`
//! header, and the UART provisioning line grammar. Each is a function of bytes
//! in and decisions out, with a pin and a clock supplied by the caller. That is
//! exactly why they are portable, and it is the property the whole port rests
//! on: neither the 1-Wire bus nor the `ZACwire` waveform can be exercised on a
//! host without it, and a bit-bang protocol that has only ever been run on a
//! machine with a sensor soldered to it is a bit-bang protocol nobody can
//! review.
//!
//! The vocabulary they decode *into* — [`cc_domain::units`] for a Celsius and
//! a `Millis`, nothing else — stays in `cc-domain`, so the arrow points one way
//! and a unit cannot change without every protocol that reports one.
//!
//! # What is NOT here
//!
//! * **Transport.** `cc-hal-esp32` owns the UART, the GPIO, the I²C controller
//!   and the `EspHttpServer`. Each module here names the *seam* it needs
//!   ([`sensor::onewire::OneWireBus`], [`sensor::tsic306::EdgeSource`],
//!   [`abp2::I2cBus`]) and never a peripheral.
//! * **Policy.** What the machine does with a reading is `cc-safety`'s and
//!   `cc-machine`'s. Nothing here may say whether a temperature is acceptable.
//!   [`sensor::probe::ProbeReading::is_usable`] is the shared *query* both
//!   drivers report through; it filters nothing.
//! * **Network link and publish policy.** `cc-netpolicy`.
//!
//! # Rules
//!
//! * `no_std` and **no `alloc` in shipped code**: `extern crate alloc` is under
//!   `#[cfg(test)]`, so a protocol that has to allocate to hold a 32-byte SSID
//!   or a 600-point ring cannot be written here at all.
//! * This crate must never name `esp_idf_svc`, `esp_idf_hal` or `esp_idf_sys`.
//!   A CI grep enforces that (05 §6).
//! * This is a **move**. Every file here was `cc-domain/src/<same path>` and the
//!   text is byte-identical apart from the `cc_domain::units::` paths that became
//!   `cc_domain::units::`. Parity with the C++ cannot be measured on this half of
//!   the port, so any edit beyond that path rewrite would be drift nobody sees.

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

pub mod abp2;
pub mod http_auth;
pub mod provisioning;
pub mod sensor;

#[cfg(test)]
extern crate alloc;
