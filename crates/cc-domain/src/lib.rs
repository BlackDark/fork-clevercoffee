//! Pure domain vocabulary for the Clever Coffee firmware: units, enums, the 18 machine
//! states, error codes, the PID controller, and the compile-time policy whitelists.
//!
//! # Where this sits in the layering
//!
//! ```text
//!   cc-domain ──> cc-protocol ──> cc-hal-esp32
//!     │
//!     ├──> cc-safety ──> cc-machine ──┐
//!     └──> cc-config, cc-display ─────┴─> cc-hal-esp32
//! ```
//!
//! **This is the crate at the bottom, and it stays small.** It is the vocabulary
//! the rest of the firmware speaks: a temperature has a type, the machine is in
//! one of eighteen states, an error has a code, and a PID has a mode. Nothing
//! here decodes a byte, drives a pin, or holds a ring of samples -- those are
//! `cc-protocol` (protocols over bytes) and `cc-netpolicy` (link, publish and
//! retry policy). That is finding 4.4; this crate was ~18,700 lines of all
//! three, and a reviewer could no longer read it in one sitting.
//!
//! # Rules
//!
//! * `no_std`, no `alloc`, and **no dependency is on by default** (04 §1, §6).
//!   The single optional dependency is `serde`, off unless a crate that already
//!   depends on it turns it on, and it exists for one reason: `Secret<T>`'s
//!   transparent `Serialize`/`Deserialize` impls, which `cc-config` needs to
//!   round-trip the four credential fields. See [`secret`].
//! * This crate must never name `esp_idf_svc`, `esp_idf_hal` or `esp_idf_sys`.
//!   A CI grep enforces that (05 §6).
//! * Every unit is a newtype, never a bare `f32`. Mixing millimetres with
//!   inches is the class of bug the type system is here to prevent.

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

/// Add a `from_raw` constructor to an enum whose discriminants are contiguous
/// `i8` values starting at 0.
///
/// The schema in `cc-config` stores enumerations as their integer discriminant
/// so the JSON matches the C++ byte for byte, and needs to turn one back into
/// the enum. Doing that with a single macro keeps the mapping next to the
/// declaration, where a reviewer can check it, and means adding a variant is a
/// one-line change rather than an edit in two crates.
///
/// **Why it lives here and is not `#[macro_export]`ed.** This is one macro, used
/// by [`hardware`], [`process`] and [`system`]; it used to be a byte-identical
/// copy of itself in each of those three files, because `macro_rules` has no
/// crate-private export. It does have one, and it is this line: `macro_rules`
/// scoping is **textual**, so a definition at the crate root is visible to every
/// module declared *after* it. `#[macro_export]` would also work and would also
/// be wrong — it publishes `cc_domain::from_raw` as public API of a `no_std`
/// domain crate, and `missing_docs` would then demand docs on a macro that is
/// an implementation detail of three of its own modules.
///
/// **It must stay above the `pub mod` lines below.** Moving it into a submodule
/// and importing it is the other idiom, and it does not work here: `use
/// crate::…` cannot name a `macro_rules` macro that has not been exported.
macro_rules! from_raw {
    ($name:ident, { $($variant:ident => $value:literal),* $(,)? }) => {
        impl $name {
            /// Recover the variant from its wire representation.
            ///
            /// Returns `None` for a value that is not a declared variant, which
            /// is what rejects a hand-edited configuration file rather than
            /// letting it become an impossible state.
            #[must_use]
            pub const fn from_raw(raw: i8) -> Option<Self> {
                match raw {
                    $($value => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

pub mod error;
pub mod hardware;
pub mod heater;
pub mod pid;
pub mod process;
pub mod secret;
pub mod state;
pub mod switch;
pub mod system;
pub mod units;

/// Whether this build's emergency-temperature safety bounds are relaxed so the
/// over-temp trip can be exercised on a bench.
///
/// **Off unless `CC_BENCH_UNSAFE_TEMPERATURES` is set in the build environment.**
/// It is read with [`option_env!`], so it is baked in at compile time: there is
/// no runtime switch, no HTTP route, no parameter, and nothing an operator can
/// reach at runtime. `just bench-build` sets it; `just build-esp32` does not.
///
/// # Why it exists
///
/// The emergency threshold has a 120 °C floor, and `cc_safety::validate_config`
/// will not accept a threshold at or below either setpoint plus the hysteresis. A
/// bench boiler sits at room temperature with an LED on the heater pin, so the
/// over-temp trip in [`operations/runbook.md` §13.1] cannot be reached on a
/// bench at all without this — and it cannot be reached by hand either, because
/// warming a DS18B20 past 120 °C is not something a person should attempt.
///
/// # What it does NOT relax
///
/// The relay rules. `LOW_TRIGGER` on the heater, the pump or the valve stays
/// refused, because a floating pin that energises a relay is a hazard that no
/// test procedure creates and no test procedure should excuse.
pub const BENCH_UNSAFE_TEMPERATURES: bool = option_env!("CC_BENCH_UNSAFE_TEMPERATURES").is_some();

#[cfg(test)]
extern crate alloc;

#[cfg(test)]
mod pid_parity;

pub use error::ErrorCode;
pub use state::MachineState;
