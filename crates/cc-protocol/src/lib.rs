//! Allocation-free wire protocols with caller-supplied transport and time.
//!
//! [`sensor`], [`abp2`], [`http_auth`], and [`provisioning`] decode into domain values;
//! the HAL owns peripherals. Machine safety decisions belong to `cc-safety` and
//! `cc-machine`; network scheduling belongs to `cc-netpolicy`.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod abp2;
pub mod http_auth;
pub mod provisioning;
pub mod sensor;

#[cfg(test)]
extern crate alloc;
