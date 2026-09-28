//! The configuration model: the registered parameter schema, the in-memory
//! value, the [`ConfigStore`] trait, and NVS-independent JSON import/export.
//!
//! # Rules
//!
//! * Portable. Storage is a *trait*, so the `no_std` model and the NVS-backed
//!   `cc-hal-esp32` implementation are decoupled (04 §6).
//! * No global mutable `Config` singleton. Configuration is a value that is
//!   threaded through the control loop (skill §4).
//! * `Secret<T>` redacts in both `Debug` and `Display` so no `log::info!("{cfg:?}")`
//!   can leak a Wi-Fi password. See 05 §5.
//!
//! Owner: R2-06. This is the R1-01 workspace skeleton.

#![no_std]
