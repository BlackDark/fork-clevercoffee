//! Portable HTTP payloads and request validation over [`Telemetry`] and configuration.
//!
//! Sockets, route registration, streaming, and the shared snapshot are in
//! `cc-net-esp32`. Heap gauges are passed into renderers rather than read through
//! ESP-IDF.
//!
//! [`Telemetry`] must stay fixed-size and allocation-free to clone inside the
//! snapshot's critical section. Snapshot reads must copy and restore the value atomically;
//! a destructive read would make a second poll in one control period report defaults.

pub mod auth;
pub mod help;
pub mod history;
pub mod http_auth;
pub mod ota;
pub mod parameters;
pub mod payload;
pub mod request;
pub mod telemetry;
pub mod telnet;

/// The free-heap floor, in bytes, below which the device is "tight" (ADR-0002
/// decision 5): the telnet log stream stops writing and the large HTTP responses
/// (`/api/parameters`, `/api/config`) answer `503` instead of risking an OOM
/// abort. One number for both, so they cannot drift.
pub const HEAP_FLOOR_BYTES: u32 = 30_000;

pub use auth::Auth;
pub use help::{not_found_json, parameter_help, wants_json_not_found};
pub use ota::{Kind, PartReader, Progress, ReadError, MAX_FILESYSTEM_BYTES, MAX_FIRMWARE_BYTES};
pub use parameters::{
    classify_parameters, ParameterPost, MAX_CONFIG_UPLOAD_BYTES, MAX_PARAMETER_BODY_BYTES,
    MAX_PARAMETER_PAIRS,
};
pub use payload::{
    error_body, health_json, mime_for, nvs_debug_json, parameters_json, status_json,
    temperatures_json, unavailable_json, upload_response, weight_json,
};
pub use request::{explicit_value, first_of, parse_flag, parse_setpoint, query_of};
pub use telemetry::{Command, Telemetry};
pub use telnet::{Decision, Level, Ring, Shed};
