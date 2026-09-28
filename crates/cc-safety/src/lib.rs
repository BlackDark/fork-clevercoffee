//! The safety monitor: the single place that answers
//! "may the heater / pump / valve be energised right now?".
//!
//! # Rules
//!
//! * `no_std`, zero I/O, zero dependencies outside [`cc_domain`] (04 §1, §6).
//! * A pure reducer of `(telemetry, config, state) -> SafetyVerdict`, so every
//!   safety path S1-S5 in 01 §6 is a host unit test rather than a hardware test.
//! * A reviewer must be able to read this crate in one sitting and be certain
//!   nothing else influences the verdict. That is the entire reason it is a
//!   separate crate from `cc-domain`.
//!
//! Safety paths S1-S5 are owned by R2-05; S6-S11 by R3-10. Nothing is
//! implemented here yet — this is the R1-01 workspace skeleton.

#![no_std]
