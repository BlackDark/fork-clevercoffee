//! The network tier as pure, host-tested logic: HTTP, MQTT, Wi-Fi and provisioning.
//!
//! Sockets, tasks, timers and flash belong to `cc-hal-esp32`; this crate decides
//! what they say and when. Callers supply the clock.
//!
//! - [`web`]: payloads, request parsing, authentication, OTA framing, telnet, history.
//! - [`mqtt`]: topic layout, registry, payload buffers, and the resumable publish cursor.
//! - [`wifi`]: connection ordering and signal buckets; [`resilience`]: retry and
//!   circuit breaking.
//! - [`provisioning`]: the UART console's line protocol.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod mqtt;
pub mod provisioning;
pub mod resilience;
pub mod web;
pub mod wifi;
