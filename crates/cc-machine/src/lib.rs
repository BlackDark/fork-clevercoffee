//! Orchestration: the 18-state machine, its handlers, and the pure reducer
//! `reduce(state, ctx, event) -> (state, Vec<Effect>)` from 04 §3.1.
//!
//! # Rules
//!
//! * The reducer is pure: no I/O, no clock, no sleep. That is what makes the
//!   exhaustive `state x event` table a host test (R2-08) and the control loop
//!   host-benchmarkable (R2-09).
//! * Exactly one `applier.apply()` calls the actuator ports. Nothing else may.
//! * `water_flow_allowed` is a `match` on `MachineState` with **no `_` arm**, so
//!   adding a water-flow state without updating the whitelist (S5) is a
//!   *compile* error rather than a silent regression.
//! * `LoopManager::update()`'s eight ordered steps are deliberately NOT ported
//!   as-is: that god-function is the defect this migration exists to remove.
//!
//! Owner: R2-08. This is the R1-01 workspace skeleton.

#![no_std]

extern crate alloc;
