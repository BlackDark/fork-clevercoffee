//! The MQTT wire contract: topic layout, publish registry, value buffers, intervals.
//!
//! Pure functions of [`Config`] and machine state. No clock, no socket, no heap
//! gauge, no allocation after construction. That is what makes it host-testable,
//! and it is the boundary: everything needing a real clock or a real socket lives
//! in `cc-hal-esp32::mqtt`, because the publish budget is a claim about the
//! control task's 10 ms period and only the device can honour it.
//!
//! [`cc_config::discovery`] builds the Home Assistant documents; it is a function
//! of the configuration too and is tested where it lives.
//!
//! # Modules
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`topics`] | topic layout, inbound command matcher |
//! | [`registry`] | [`Registry`] (publishable topics), [`PlanView`] (flat three-phase plan) |
//! | [`payload`] | [`Payload`], [`SetCommand`] — fixed capacity, no allocator |
//! | [`budget`] | pass intervals, per-tick budget |
//!
//! # The one trap
//!
//! [`PlanView`] slices are **offsets, not lengths**. Each slice's end is the next
//! one's start; storing a length instead silently drops the sensors and reads
//! `binary_sensors` past the end. Pinned by
//! `registry::tests::the_plan_slices_partition_the_view`.
#![no_std]
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
/// Empty `mqtt.broker` means **off**, not "try and fail". It is the default, so an
/// unprovisioned machine must not reserve a task stack and two buffers for a
/// client with nowhere to connect.
#[must_use]
pub fn is_configured(config: &Config) -> bool {
    config.mqtt.enabled && !config.mqtt.broker.is_empty()
}

#[cfg(test)]
mod tests;
