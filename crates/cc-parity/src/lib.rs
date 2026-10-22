//! Scenario harness using the real reducer, safety monitor, and effect applier.
//!
//! `run::Recorder` records actuator calls without GPIO or a device dependency, so
//! `dry_run` scenarios cannot energise hardware. The format is specified in
//! [`docs/scenarios.md`](../../docs/scenarios.md).
//!
//! [`scenario`] validates inputs; [`run`](mod@run) drives them; [`observe`] records assertions' inputs.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod observe;
pub mod run;
pub mod scenario;

pub use observe::Observation;
pub use run::{run, RunError, Runner};
pub use scenario::{Scenario, ScenarioError, Stimulus, StimulusKind};
