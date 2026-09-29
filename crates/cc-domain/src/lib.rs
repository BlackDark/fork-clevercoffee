//! Pure domain vocabulary for the Clever Coffee firmware: units, enums, the 18 machine
//! states, error codes, the PID controller, and the compile-time policy whitelists.
//!
//! # Rules
//!
//! * `no_std`, no `alloc`, and **no dependency is on by default** (04 §1, §6).
//!   The single optional dependency is `serde`, off unless a crate that already
//!   depends on it turns it on, and it exists for one reason: `Secret<T>`'s
//!   transparent `Serialize`/`Deserialize` impls, which `cc-config` needs to
//!   round-trip the four credential fields. See [`secret`].
//! * This crate must never name `esp_idf_svc`, `esp_idf_hal` or `esp_idf_sys`.
//!   A CI grep enforces that (05 §6).
//! * Every unit is a newtype, never a bare `f32`. Mixing millimetres with
//!   inches is the class of bug the type system is here to prevent.

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

pub mod abp2;
pub mod error;
pub mod hardware;
pub mod heater;
pub mod mqtt;
pub mod pid;
pub mod process;
pub mod provisioning;
pub mod resilience;
pub mod secret;
pub mod sensor;
pub mod state;
pub mod switch;
pub mod system;
pub mod units;
pub mod wifi;

#[cfg(test)]
extern crate alloc;

#[cfg(test)]
mod pid_parity;

pub use error::ErrorCode;
pub use state::MachineState;
