//! Topic layout `<prefix><hostname>/<reading>`, the inbound command matcher, and
//! the four targets that are machine state rather than configuration parameters.

use alloc::format;
use alloc::string::String;

/// Longest topic, in bytes. Longer is a configuration error; publishing to a
/// truncated topic is worse than refusing it.
pub const TOPIC_MAX: usize = 120;

/// The topic layout, built once from the configuration.
///
/// ```text
///   status   <prefix><hostname>/status
///   command  <prefix><hostname>/+/set
///   state    <prefix><hostname>/<reading>
/// ```
///
/// Defaults (`custom/kitchen/` + `clevercoffee`) give the state topic
/// `custom/kitchen/clevercoffee/temperature`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topics {
    /// Retained last-will and availability topic.
    pub will: String,
    /// Subscription wildcard.
    pub set: String,
    /// Prefix every state topic starts with.
    pub base: String,
}

impl Topics {
    /// Build the layout from `mqtt.topic` and `system.hostname`.
    ///
    /// **No separator between the two** — `mqtt.topic`'s default already ends in
    /// `/`. Do not "fix" it: this is a wire format, and existing installs hold
    /// retained topics under this exact spelling. Adding one orphans their
    /// entities.
    #[must_use]
    pub fn new(prefix: &str, hostname: &str) -> Self {
        Self {
            will: format!("{prefix}{hostname}/status"),
            set: format!("{prefix}{hostname}/+/set"),
            base: format!("{prefix}{hostname}/"),
        }
    }

    /// The state topic for one reading, `<base><reading>`.
    #[must_use]
    pub fn state(&self, reading: &str) -> String {
        format!("{}{reading}", self.base)
    }

    /// The command topic for one reading, `<base><reading>/set`.
    #[must_use]
    pub fn command(&self, reading: &str) -> String {
        format!("{}{reading}/set", self.base)
    }

    /// The reading an inbound topic names, or `None` if it is not a command.
    ///
    /// Requires all three: starts with the base, tail after the last `/` is
    /// exactly `set`, reading contains no `/`. So a broker echoing our own
    /// `<base>+/set` is refused rather than becoming a parameter called `+`.
    #[must_use]
    pub fn command_of<'a>(&self, topic: &'a str) -> Option<&'a str> {
        let rest = topic.strip_prefix(self.base.as_str())?;
        let (reading, verb) = rest.rsplit_once('/')?;
        if verb != "set" || reading.is_empty() || reading.contains('/') {
            return None;
        }
        Some(reading)
    }
}

/// One retained parameter topic and the configuration key it writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamTopic {
    /// Topic name, relative to [`Topics::base`].
    pub topic: String,
    /// True. A subscriber then gets the value without waiting for the next pass.
    pub retain: bool,
    /// Configuration key to write, or one of [`STEAM_MODE`], [`BACKFLUSH_ON`],
    /// [`TARE_ON`], [`CALIBRATION_ON`] — those four are machine state, not
    /// parameters, so they are compared **before** the registry is consulted.
    pub key: &'static str,
}

/// Steam-mode switch: machine state, not a parameter.
pub const STEAM_MODE: &str = "STEAM_MODE";

/// The backflush-mode switch.
pub const BACKFLUSH_ON: &str = "BACKFLUSH_ON";

/// The scale tare request.
pub const TARE_ON: &str = "TARE_ON";

/// The scale calibration request.
pub const CALIBRATION_ON: &str = "CALIBRATION_ON";
