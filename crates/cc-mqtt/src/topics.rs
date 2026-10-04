//! The topic layout: `<prefix><hostname>/<reading>`, and the four inbound
//! targets that are machine state rather than configuration parameters.
//!
//! Moved verbatim from `cc_hal_esp32::mqtt`. `Topics::new` is two `format!`
//! calls over two borrowed `&str`, and `command_of` is a `strip_prefix` and a
//! `rsplit_once` — see the crate root for why there is no heap gauge or clock
//! anywhere in this crate.

use alloc::format;
use alloc::string::String;

/// The longest topic this firmware will build, in bytes.
///
/// `MQTTManager.cpp:194` `char topic[120]`, and the C++'s `snprintf` truncates
/// to it. Reproduced: a topic longer than this is a configuration error, and
/// silently publishing to a truncated topic would be worse than refusing.
pub const TOPIC_MAX: usize = 120;

/// The topic layout, built once from the configuration.
///
/// `MQTTManager::setup` (`MQTTManager.cpp:80-82`):
///
/// ```cpp
/// snprintf(topicWill_, ..., "%s%s/%s", topicPrefix_, hostname_, "status");
/// snprintf(topicSet_,  ..., "%s%s/+/%s", topicPrefix_, hostname_, "set");
/// ```
///
/// and `publish` at `:194-196` builds `"<prefix><hostname>/<reading>"`. With
/// the defaults (`mqtt.topic = "custom/kitchen/"`, `system.hostname =
/// "clevercoffee"`) a topic is `custom/kitchen/clevercoffee/temperature`.
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
    /// The C++ does **not** insert a separator between the two (`"%s%s/%s"`) and
    /// `mqtt.topic`'s default already ends in `/`. Reproduced rather than
    /// tidied: a topic layout is a wire format, and an existing Home Assistant
    /// install holds retained topics under the C++'s exact spelling. A "fixed"
    /// layout would orphan every entity on every machine that upgrades.
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
    /// `MQTTManager::messageCallback` (`MQTTManager.cpp:252-262`) formats its
    /// matcher as `"%s%s/%%119[^\\/]/%%63[^\\/]"` and requires the second field
    /// to be exactly `set`, logging `"Invalid MQTT topic/command"` otherwise.
    /// This is the same three conditions, in the same order: the topic starts
    /// with the base, the tail after the last `/` is `set`, and the reading has
    /// no `/` in it.
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
/// The C++'s `mqttVars_` entry, `mqttTopic -> parameterId`
/// (`MQTTManager.cpp:357`). The mapping is not decoration: the inbound handler
/// looks the topic up in it and refuses anything absent
/// (`MQTTManager.cpp:289-292`, "MQTT topic %s not found in mapping").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamTopic {
    /// The topic name, relative to [`Topics::base`].
    pub topic: String,
    /// Parameters are always retained (`MQTTManager.cpp:492`).
    pub retain: bool,
    /// The configuration key, or one of [`STEAM_MODE`], [`BACKFLUSH_ON`],
    /// [`TARE_ON`], [`CALIBRATION_ON`].
    ///
    /// The four are **not** configuration parameters — the C++ compares the
    /// mapped id against those four literals before it looks the parameter up
    /// (`MQTTManager.cpp:296-322`) — so they are spelled the same way here, as
    /// the same four string constants, and the comparison is the same `==`.
    pub key: &'static str,
}

/// `MQTTManager.cpp:296` — the steam-mode switch, which is machine state and not
/// a configuration parameter.
pub const STEAM_MODE: &str = "STEAM_MODE";

/// `MQTTManager.cpp:303` — the backflush-mode switch.
pub const BACKFLUSH_ON: &str = "BACKFLUSH_ON";

/// `MQTTManager.cpp:309` — the scale tare request.
pub const TARE_ON: &str = "TARE_ON";

/// `MQTTManager.cpp:317` — the scale calibration request.
pub const CALIBRATION_ON: &str = "CALIBRATION_ON";
