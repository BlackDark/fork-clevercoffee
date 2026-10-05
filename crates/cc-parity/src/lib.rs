//! The parity scenario harness (06 R1-08).
//!
//! # What this is
//!
//! `just parity <port> <host>` replays a declarative script against a firmware and
//! compares what it observes against a recorded reference. The format is
//! specified in
//! [`docs/history/scenario-format.md`](../../docs/history/scenario-format.md).
//!
//! # The safety property, and where it comes from
//!
//! **A `dry_run` scenario cannot energise an actuator, and that is structural
//! rather than a promise.** The runner drives the real reducer
//! ([`cc_machine::reduce`]) and the real safety monitor
//! ([`cc_safety::reduce`]) in process, and the [`Actuators`] implementation it
//! hands them is [`Recorder`] — a `Vec` of call names. There is no GPIO in this
//! crate, no device crate in its dependency tree, and no path from a scenario
//! file to a pin.
//!
//! That is what makes a `brew_by_time` scenario possible at all: it emits
//! `Effect::EnablePump` and `Effect::OpenWaterValve`, the harness asserts that
//! they were emitted *and* that the safety ordering around them held, and
//! nothing is ever connected to anything.
//!
//! # The three modules
//!
//! * [`scenario`] — the file format: parse, validate, and reject.
//! * [`observe`] — the canonical observation, which is what gets diffed.
//! * [`diff`] — the diff, and the classification against the divergence ledger.
//!
//! [`run`] is the driver: scenario in, observation out.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod diff;
pub mod observe;
pub mod run;
pub mod scenario;

pub use diff::{classify, Classification, DiffLine, Divergence, Ledger, Verdict};
pub use observe::Observation;
pub use run::{run, RunError, Runner};
pub use scenario::{Scenario, ScenarioError, Stimulus, StimulusKind};
