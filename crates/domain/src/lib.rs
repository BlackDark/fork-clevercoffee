//! Pure machine logic. No I/O, no hardware, no allocation.
//!
//! Everything here is host-testable, and everything here is on the safety path: the state
//! machine decides what the pump and the valve do, and the PID decides how hard the heater runs.
//! The C++ original spread that across a 677-line loop, an ISR and four singletons; here it is a
//! pure function of an input struct, so a wrong transition is a failing test rather than a
//! flooded kitchen.
//!
//! Two conventions carry over from the C++ firmware on purpose, because the frontend depends on
//! them:
//!
//! - [`State`] discriminants are the same integers the C++ `MachineStateId` enum used, so the
//!   dashboard keeps rendering states correctly.
//! - The PID keeps the Arduino-PID time-based formulation, the 0-to-`window` output range and
//!   the integral anti-windup rules, because changing them changes the machine's behaviour in a
//!   way nobody asked for.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod backflush;
pub mod emergency;
pub mod pid;
pub mod sensor;
pub mod state;
pub mod timing;
pub mod transition;

pub use backflush::{
    resolve_cycle_advance, resolve_mode_change, CycleAdvanceEffect, ModeChangeEffect,
};
pub use emergency::{EmergencyDecision, EmergencyStop};
pub use pid::{Gains, Pid, PidOutput};
pub use sensor::{SensorFault, TemperatureFilter};
pub use state::{classify, group_of, State, StateGroup};
pub use timing::Timing;
pub use transition::{actuators_for, next_state, Actuators, Inputs};
