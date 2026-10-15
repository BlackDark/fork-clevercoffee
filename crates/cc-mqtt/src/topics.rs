//! The topic layout: `<prefix><hostname>/<reading>`, and the four inbound
//! targets that are machine state rather than configuration parameters.
//!
//! `Topics::new` is two `format!` calls over two borrowed `&str`, and
//! `command_of` is a `strip_prefix` and a `rsplit_once`. See the crate root for
//! why there is no heap gauge or clock anywhere in this crate.

use alloc::format;
use alloc::string::String;

/// The longest topic this firmware will build, in bytes.
///
/// A topic longer than this is a configuration error, and silently publishing to
/// a truncated topic would be worse than refusing it.
pub const TOPIC_MAX: usize = 120;

/// The topic layout, built once from the configuration.
///
/// ```text
///   status   <prefix><hostname>/status
///   command  <prefix><hostname>/+/set
///   state    <prefix><hostname>/<reading>
/// ```
///
/// With the defaults (`mqtt.topic = "custom/kitchen/"`, `system.hostname =
/// "clevercoffee"`) a state topic is `custom/kitchen/clevercoffee/temperature`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topics {
    /// The retained last-will and availability topic,
    /// `<prefix><hostname>/status`.
    pub will: String,
    /// The subscription wildcard, `<prefix><hostname>/+/set`.
    pub set: String,
    /// The prefix every state topic starts with, `<prefix><hostname>/`.
    pub base: String,
}

impl Topics {
    /// Build the layout from `mqtt.topic` and `system.hostname`.
    ///
    /// **No separator is inserted between the two**, because `mqtt.topic`'s
    /// default already ends in `/`. Do not "fix" this: a topic layout is a wire
    /// format, and existing Home Assistant installs hold retained topics under
    /// this exact spelling. Adding a separator would orphan every entity on
    /// every machine that upgraded.
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
    /// Three conditions, all required: the topic starts with the base, the tail
    /// after the last `/` is exactly `set`, and the reading contains no `/`.
    ///
    /// A retained `set` at depth 0 — a broker echoing our own command, or a
    /// `.../+/set` wildcard published to — is rejected here rather than becoming
    /// a parameter called `+`.
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

/// One retained parameter topic and what it maps to.
///
/// The mapping is not decoration: the inbound handler looks the topic up in it
/// and refuses anything absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamTopic {
    /// The topic name, relative to [`Topics::base`].
    pub topic: String,
    /// Parameters are always retained, so a subscriber gets the current value
    /// immediately rather than waiting for the next pass.
    pub retain: bool,
    /// The configuration key, or one of [`STEAM_MODE`], [`BACKFLUSH_ON`],
    /// [`TARE_ON`], [`CALIBRATION_ON`].
    ///
    /// The four are **not** configuration parameters: a `key` equal to one of
    /// them is a request against the running machine rather than a parameter
    /// write, so the comparison against these four happens before the registry
    /// is consulted.
    pub key: &'static str,
}

/// The steam-mode switch, which is machine state and not a configuration
/// parameter.
pub const STEAM_MODE: &str = "STEAM_MODE";

/// The backflush-mode switch.
pub const BACKFLUSH_ON: &str = "BACKFLUSH_ON";

/// The scale tare request.
pub const TARE_ON: &str = "TARE_ON";

/// The scale calibration request.
pub const CALIBRATION_ON: &str = "CALIBRATION_ON";
