//! Allocation-free wire protocols with caller-supplied transport and time.
//!
//! [`sensor`] and [`abp2`] decode device traffic into domain values; the HAL owns
//! the peripherals. Safety decisions belong to `cc-safety` and `cc-machine`; the
//! network tier to `cc-net`.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod abp2;
pub mod sensor;

#[cfg(test)]
extern crate alloc;
