//! Machine-level option vocabulary.
//!
//! Port of the `System` namespace in `include/clevercoffee/defaults.h:183-212`.

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

/// Which of the six display layouts to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i8)]
pub enum DisplayTemplate {
    /// The full temperature/pressure/weight layout.
    Standard = 0,
    /// Large temperature only.
    Minimal = 1,
    /// Temperature, nothing else.
    TemperatureOnly = 2,
    /// Scale-oriented layout. Dead code — see [`crate::hardware::ScaleType`].
    Scale = 3,
    /// Upright portrait layout.
    Upright = 4,
    /// High-contrast layout.
    Modern = 5,
}

/// The display language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i8)]
pub enum Language {
    /// English.
    English = 0,
    /// German.
    German = 1,
    /// Spanish.
    Spanish = 2,
}

/// Log verbosity. The values are the C++ `Logger::Level` levels
/// (`defaults.h:203-211`) and are compared numerically by the logger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(i8)]
pub enum LogLevel {
    /// Per-iteration tracing. Very noisy.
    Trace = 0,
    /// Developer diagnostics.
    Debug = 1,
    /// Normal operation. The default.
    Info = 2,
    /// Something unexpected that the machine recovered from.
    Warning = 3,
    /// Something that stopped an operation.
    Error = 4,
    /// Unrecoverable.
    Fatal = 5,
    /// Log nothing.
    Silent = 6,
}

from_raw!(DisplayTemplate, { Standard => 0, Minimal => 1, TemperatureOnly => 2, Scale => 3, Upright => 4, Modern => 5 });
from_raw!(Language, { English => 0, German => 1, Spanish => 2 });
from_raw!(LogLevel, { Trace => 0, Debug => 1, Info => 2, Warning => 3, Error => 4, Fatal => 5, Silent => 6 });
