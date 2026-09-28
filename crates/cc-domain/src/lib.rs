//! Pure domain vocabulary for the Clever Coffee firmware: units, enums, the 18 machine states,
//! error codes, the PID controller, and the compile-time policy whitelists.
//!
//! # Rules
//!
//! * `no_std`, no `alloc`, no dependencies at all (04 §1, §6).
//! * This crate must never name `esp_idf_svc`, `esp_idf_hal` or `esp_idf_sys`.
//!   A CI grep enforces that (05 §6).
//! * Every unit is a newtype, never a bare `f32`. Mixing millimetres with
//!   inches is the class of bug the type system is here to prevent.

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
