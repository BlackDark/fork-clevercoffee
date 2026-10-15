//! The MQTT wire contract as pure functions: topic layout, publish registry,
//! value buffers, and the publish intervals.
//!
//! Everything here is a function of a [`Config`] and a machine state. No clock,
//! no heap gauge, no socket, and no allocation after construction — which is
//! what makes it host-testable, and why `just test` reaches it.
//!
//! # Boundary
//!
//! ```text
//!   cc-domain ── cc-config ──> cc-mqtt ──> cc-hal-esp32 ──> cc-firmware
//! ```
//!
//! `cc-domain` and `cc-config` are the only dependencies, and neither knows this
//! crate exists. `cc-hal-esp32` depends on this crate and never the reverse:
//! the socket, the esp-mqtt callback and the time-budgeted publisher are all
//! device-side, because the budget is a claim about the control task's 10 ms
//! period and only the device can honour it.
//!
//! # What each module owns
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`topics`] | the `<prefix>/<host>/<name>` layout and the inbound command matcher |
//! | [`registry`] | [`Registry`] (the publishable topics), [`PlanView`] (the flat plan) |
//! | [`payload`] | [`Payload`] and [`SetCommand`] — fixed capacity, never the allocator |
//! | [`budget`] | the per-state publish intervals and the per-tick time budget |
//!
//! # Two invariants a caller relies on
//!
//! * **The three phases partition the view.** [`PlanView`] slices parameters,
//!   sensors and binary sensors into non-overlapping ranges, each slice's end
//!   being the next one's start. Checked by
//!   `registry::tests::the_plan_slices_partition_the_view`. An earlier version
//!   stored slice *lengths* where offsets were expected, so any registry with a
//!   binary sensor read past the end of the view.
//! * **Nothing here reads the clock or the heap.** No `now_ms`, no
//!   `esp_get_free_heap_size`, no client handle. [`is_configured`] and
//!   [`interval_for`] are total functions of their arguments, and the two
//!   genuinely time-dependent decisions — discovery cadence and the publish
//!   budget — belong to the publisher in `cc-hal-esp32`.
//!
//! # Not here
//!
//! [`cc_config::discovery`] builds the Home Assistant discovery documents and
//! `cc-hal-esp32::mqtt` publishes them. They are a function of the
//! configuration alone and are host-tested where they already were.
#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

extern crate alloc;

pub mod budget;
pub mod payload;
pub mod registry;
pub mod topics;

pub use budget::{
    interval_for, DISCOVERY_INTERVAL_MS, INTERVAL_BREW_MS, INTERVAL_MS, INTERVAL_STANDBY_MS,
    TIME_BUDGET_MS,
};
pub use payload::{
    Payload, SetCommand, BUFFER_BYTES, INBOUND_DEPTH, PAYLOAD_MAX, READING_MAX, VALUE_MAX,
};
pub use registry::{PlanView, Registry};
pub use topics::{
    ParamTopic, Topics, BACKFLUSH_ON, CALIBRATION_ON, STEAM_MODE, TARE_ON, TOPIC_MAX,
};

use cc_config::Config;

/// Whether a broker is configured at all.
///
/// Empty `mqtt.broker` means **off**, not "try and fail": an empty broker is the
/// default, so a machine that was never provisioned must not spawn an MQTT task
/// at all — that is a task's stack and two buffers reserved for a client with
/// nowhere to connect.
///
/// A function of the configuration and of nothing else, which is why it is a
/// question this crate answers rather than one the caller must.
#[must_use]
pub fn is_configured(config: &Config) -> bool {
    config.mqtt.enabled && !config.mqtt.broker.is_empty()
}

#[cfg(test)]
mod tests;
