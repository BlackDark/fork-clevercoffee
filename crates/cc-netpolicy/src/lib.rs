//! Allocation-free network decisions, independent of transport and domain types.
//!
//! [`wifi`] orders connections; [`mqtt`] tracks publish progress; [`resilience`]
//! schedules retries; [`history`] holds chart samples. Callers supply time and
//! perform socket and flash operations. Topic layout belongs to `cc-mqtt`.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod history;
pub mod mqtt;
pub mod resilience;
pub mod wifi;

#[cfg(test)]
extern crate alloc;
