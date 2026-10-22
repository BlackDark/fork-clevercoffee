//! Portable HTTP payloads and request validation over [`Telemetry`] and configuration.
//!
//! Sockets, route registration, streaming, and the shared snapshot remain in the HAL.
//! Heap gauges are passed into renderers rather than read through ESP-IDF.
//!
//! [`Telemetry`] must stay fixed-size and allocation-free to clone inside the HAL's
//! critical section. Snapshot reads must copy and restore the value atomically;
//! a destructive read would make a second poll in one control period report defaults.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod auth;
pub mod help;
pub mod ota;
pub mod parameters;
pub mod payload;
pub mod request;
pub mod telemetry;
pub mod telnet;

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
