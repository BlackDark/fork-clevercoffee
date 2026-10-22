//! Shared units, machine vocabulary, PID arithmetic, and heater policy.
//!
//! Keep byte protocols in `cc-protocol` and network scheduling in `cc-netpolicy`.
//! This crate has no default dependencies or allocation. Optional `serde` supports
//! [`secret`] serialization without coupling provisioning to configuration.

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

/// Declare an `i8`-backed option enum together with its `from_raw` constructor.
///
/// The schema in `cc-config` stores enumerations as their integer discriminant and needs
/// to turn one back into the enum. Wrapping the declaration means each discriminant is
/// written once, on its variant, and `from_raw` cannot disagree with it.
///
/// **Why it lives here and is not `#[macro_export]`ed.** It is used only by
/// [`hardware`], [`process`] and [`system`]. `macro_rules` scoping is **textual**, so a
/// definition at the crate root is visible to every module declared *after* it, and that is
/// the crate-private export. `#[macro_export]` would publish `cc_domain::from_raw` as
/// public API of a `no_std` domain crate.
///
/// **It must stay above the `pub mod` lines below.** `use crate::…` cannot name a
/// `macro_rules` macro that has not been exported.
macro_rules! from_raw {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident = $value:literal, )*
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$variant_meta])* $variant = $value, )*
        }

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
/// reach at runtime. `just bench-flash <port>` sets it; `just build-esp32`
/// does not.
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

#[cfg(test)]
mod tests {
    use crate::hardware::ScaleType;
    use crate::system::LogLevel;

    #[test]
    fn from_raw_inverts_the_discriminant_and_rejects_the_rest() {
        for v in [
            ScaleType::Hx711Dual,
            ScaleType::Hx711Single,
            ScaleType::Bluetooth,
        ] {
            assert_eq!(ScaleType::from_raw(v as i8), Some(v));
        }
        assert_eq!(ScaleType::from_raw(3), None);
        assert_eq!(ScaleType::from_raw(-1), None);
        assert_eq!(LogLevel::from_raw(6), Some(LogLevel::Silent));
        assert_eq!(LogLevel::from_raw(7), None);
    }
}
