//! The application: the machine, the task bodies, the API, provisioning, MQTT and logging.
//!
//! This crate is the seam between the pure logic in `clevercoffee-domain` and the hardware in
//! `clevercoffee-bsp-<board>`. Everything in it is `no_std` and hardware-free, which is what lets
//! a whole brew cycle be driven on a host against a recording fake and asserted as a sequence of
//! actuator commands.
//!
//! What lives here, and what deliberately does not:
//!
//! - [`boot`] is the boot order and the control loop, as far as they are not the firmware's.
//! - [`machine`] owns the state, the sensors, the PID and the actuators, and is the only place a
//!   transition becomes an actuator command. Generic over the [`Actuators`] implementation.
//! - [`tasks`] holds the task bodies as synchronous steps, so a scenario test is a loop of two
//!   calls rather than an executor. The scheduling and the watchdog timer are the firmware's.
//! - [`config_rt`] turns a validated configuration document into the values the machine reads.
//! - [`sensors`] reads each driver on its own period and hands the machine one snapshot a tick.
//! - [`api`] implements the thirty routes of `api-contract.md` as pure functions.
//! - [`prov`] is the device half of the provisioning protocol.
//! - [`mqtt`] is the typed parser and the Home Assistant discovery generator.
//! - [`log`] is the bounded ring buffer and the line server.
//!
//! # Layering
//!
//! `app` depends on `domain`, `hal-traits`, `config`, `storage` and `http`, and on nothing above
//! itself. `tools/check-deps.py` enforces that, so a sideways dependency fails CI rather than
//! becoming a cycle nobody notices.
//!
//! # What is not verified here
//!
//! Nothing in this crate has run on hardware. Every statement about behaviour is a statement
//! about the host tests, and the compatibility matrix records the difference.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod api;
pub mod boot;
pub mod config_rt;
pub mod log;
pub mod machine;
pub mod mqtt;
pub mod prov;
pub mod sensors;
pub mod store;
pub mod tasks;

pub use boot::Runtime;
pub use machine::{Machine, Request, RuntimeConfig, Sensors, Switches, TickOutcome};
pub use tasks::{control_step, safety_step, SafetyOutcome};
