//! CleverCoffee's machine logic, free of hardware: everything here runs and is tested
//! on a PC. The firmware feeds [`machine::Inputs`] in and drives pins from
//! [`machine::Outputs`].

#![no_std]

pub mod config;
pub mod history;
pub mod machine;
pub mod scale;
pub mod state;
pub mod switch;
pub mod timers;

pub use config::Config;
pub use machine::{Command, Event, Inputs, Machine, Outputs, Reply, Status};
pub use state::State;
