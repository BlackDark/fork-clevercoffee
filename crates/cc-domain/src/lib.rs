//! Pure control logic for the CleverCoffee firmware: state machine, PID,
//! interlocks, config schema and display layout.
//!
//! This crate is deliberately free of any platform or HAL dependency so that all
//! of it is testable on the host. Anything that needs hardware belongs in
//! `cc-drivers` or `cc-board`, behind a `cc-hal` trait.
//!
//! See `docs/rust-migration/architecture.md` section 6.1.
#![no_std]
