//! The only crate that knows about ESP-IDF: pin map, `cc-hal` implementations
//! over `esp-idf-hal`, the NVS-backed config store and the heater PWM.
//!
//! Board variants live here as feature-selected pin-map modules; the shared
//! logic in `cc-domain` does not move when one is added.
//!
//! See `docs/rust-migration/architecture.md` section 6.4.
