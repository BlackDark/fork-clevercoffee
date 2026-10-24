//! MQTT topics, publish plans, payload buffers, and cadence.
//!
//! No allocation after construction. Clocks, sockets, and enforcement of the
//! control task's publish budget belong to `cc-hal-esp32::mqtt`.
//! [`cc_config::discovery`] builds Home Assistant payloads.
//!
//! [`PlanView`] stores slice end offsets, not lengths. Confusing them drops sensors
//! and can index binary sensors past the end.
pub mod budget;
pub mod cursor;
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
