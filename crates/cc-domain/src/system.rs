//! Machine-level option vocabulary.

from_raw! {
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
}

from_raw! {
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
}

from_raw! {
    /// Log verbosity. The values are compared numerically by the logger.
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
}
