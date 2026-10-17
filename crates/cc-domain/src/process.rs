//! Brewing option vocabulary.

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
