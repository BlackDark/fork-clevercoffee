//! The configuration schema and, later in this crate, the import and export of the config JSON.
//!
//! See [schema] for the table. The table is the single registry: the HTTP API, the MQTT bridge
//! and the USB provisioning channel all read the same rows, so a parameter cannot exist in one
//! and not another, which is how the C++ firmware ended up with an emergency-stop threshold that
//! the emergency-stop manager read but no user could ever set.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod import;
pub mod json;
pub mod schema;

pub use import::{validate, Import, Reason, Report, ResolvedDoc};
pub use schema::{
    by_index, count, find, group, is_secret, secret_keys, Group, Param, Value, ValueType, PARAMS,
};
