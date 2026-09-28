//! Brewing option vocabulary.
//!
//! Port of the `Process` namespace in `include/clevercoffee/defaults.h:214-221`.

/// Add a `from_raw` constructor to an enum whose discriminants are contiguous
/// `i8` values starting at 0.
///
/// The schema in `cc-config` stores enumerations as their integer discriminant
/// so the JSON matches the C++ byte for byte, and needs to turn one back into
/// the enum. Doing that with a single macro keeps the mapping next to the
/// declaration, where a reviewer can check it, and means adding a variant is a
/// one-line change rather than an edit in two crates.
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

/// How a brew ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i8)]
pub enum BrewMode {
    /// The operator stops the shot with the brew switch.
    Manual = 0,
    /// The shot ends at `brew.by_time.target_time` or the target weight.
    Automatic = 1,
}

from_raw!(BrewMode, { Manual => 0, Automatic => 1 });
