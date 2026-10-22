//! Temperature protocols and their shared probe-reading boundary.
//!
//! Each driver's `as_probe` converts an already-polled result without blocking or
//! dynamic dispatch. Keep driver-specific range rules: TSIC rejects <= 0 or >= 180 °C,
//! including sentinels 221/222; DS18B20 accepts -50..=150 °C.
//! [`probe::ProbeReading::is_usable`] is a shared query, not either driver's filter.

pub mod ds18b20;
pub mod hx711;
pub mod onewire;
pub mod probe;
pub mod tsic306;

pub use probe::{ProbeFault, ProbeReading, ProbeSource};
