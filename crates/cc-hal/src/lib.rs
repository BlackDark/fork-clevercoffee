//! The platform seam: traits that `cc-domain` and `cc-drivers` need, with no
//! implementations.
//!
//! Nothing here may mention a concrete platform. This is the boundary that keeps
//! the control logic host-testable, and the one an alternative backend would
//! implement if ADR 0004 is ever revisited.
//!
//! See `docs/rust-migration/architecture.md` section 6.2.
#![no_std]
