//! Clever Coffee `network policy`: what the link should do next, what to
//! publish, when to retry, and the ring the chart reads.
//!
//! Owner: finding **4.4** — the split of `cc-domain` into a vocabulary crate and
//! the transport policy it had accumulated.
//!
//! # Where this sits in the layering
//!
//! ```text
//!   cc-netpolicy ──> cc-hal-esp32
//! ```
//!
//! Each module here is a **decision** the transport has not made yet:
//!
//! * [`wifi`] — the link: connection order, RSSI buckets, whether a retry is
//!   worth a reconnect.
//! * [`mqtt`] — the publish cursor: which of the three phases a topic is in, and
//!   therefore what the next packet is.
//! * [`resilience`] — the retry and circuit-breaker arithmetic every one of the
//!   above calls, kept in one place so the backoff schedule is one table rather
//!   than two.
//! * [`history`] — the 600-point ring behind `GET /api/history`.
//!
//! They are portable because every one is a **pure function of state and
//! elapsed time**, with the socket, the clock and the flash left to
//! `cc-hal-esp32`. A reconnect backoff that has only ever been exercised on a
//! machine with flaky Wi-Fi is a reconnect backoff nobody can review.
//!
//! # Why this crate has no dependencies at all
//!
//! `[dependencies]` in `Cargo.toml` is empty, and that is the load-bearing part
//! of the boundary rather than a detail: none of these four modules names
//! `cc-domain`. They decide *when* to act, not *what a unit means*, so there is
//! nothing in the vocabulary crate they need — `wifi.rs` uses only
//! `crate::resilience`, and `mqtt`/`resilience`/`history` import nothing at all.
//!
//! That is also why nothing here may grow a `cc_domain::` path. The moment one
//! does, the direction of the arrow has reversed and this crate has become a
//! second place a vocabulary decision is made.
//!
//! # What is NOT here
//!
//! * **Transport.** The `EspHttpClient`, the `mqtt` network client and the
//!   clock are `cc-hal-esp32`'s. Nothing here names a socket, a topic string
//!   layout (that is `cc-mqtt`, a sibling) or a configuration key.
//! * **Vocabulary.** A temperature's type and the machine's current state are
//!   `cc-domain`'s, and the MQTT topic layout and registry are `cc-mqtt`'s.
//!
//! # Rules
//!
//! * `no_std` and **no `alloc` in shipped code**: `extern crate alloc` is under
//!   `#[cfg(test)]`. [`history`] in particular is a fixed `[Point; 600]` with
//!   indexed access precisely so this holds.
//! * This crate must never name `esp_idf_svc`, `esp_idf_hal` or `esp_idf_sys`.
//!   A CI grep enforces that (05 §6).
//! * This is a **move**. Every file here was `cc-domain/src/<same name>` and the
//!   text is byte-identical apart from `crate::resilience`, which stays in-crate.
//!   Parity with the C++ cannot be measured on this half of the port, so any
//!   edit beyond that would be drift nobody sees.

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

pub mod history;
pub mod mqtt;
pub mod resilience;
pub mod wifi;

#[cfg(test)]
extern crate alloc;
