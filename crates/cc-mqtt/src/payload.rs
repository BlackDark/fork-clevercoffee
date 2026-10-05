//! The two fixed-capacity buffers the publish pass runs on: [`Payload`], the
//! reused value buffer, and [`SetCommand`], one inbound `set`.
//!
//! Moved verbatim from `cc_hal_esp32::mqtt`. Both are `heapless::String`s
//! because both are on a path where an allocation is heap traffic in the task
//! that runs the heater deadman: `Payload` is formatted once per published
//! topic, and `SetCommand` is produced on the `esp-mqtt` task and consumed on
//! the control task.

use heapless::String as Bounded;
use log::warn;

/// The MQTT receive and outbox buffers, in bytes.
///
/// `MQTTManager.cpp:96` `setBufferSize(1024)`, "Set larger buffer size for Home
/// Assistant discovery messages". A `number` entity with a `device` block is the
/// largest payload this firmware produces; `cc_config::discovery` asserts every
/// one of them is under this.
///
/// `MqttClientConfiguration` has both `buffer_size` and `out_buffer_size`
/// (`client.rs:104-105`) and both are set, because the receive buffer holds an
/// inbound `set` command and the outbox buffer holds an outbound discovery
/// payload, and the C++'s single `setBufferSize` covered both.
pub const BUFFER_BYTES: usize = 1024;

/// The longest value payload, in bytes.
///
/// The C++'s `char data[256]` (`MQTTManager.cpp:405`) covers a parameter
/// printed with `%.2f`, a sensor through `number2string` (which uses a
/// 22-byte buffer, `helperUtils.h:38-62`) and the `ON`/`OFF` binaries. The
/// widest value any registered topic can produce is a `%0.2f` of a
/// schema-bounded number, well under 16 bytes, so 32 is generous while keeping
/// the reusable buffer — and the dedupe table that holds one per topic — small
/// on a 320 KB chip.
pub const PAYLOAD_MAX: usize = 32;

/// The longest reading name an inbound `set` topic may carry.
///
/// `MQTTManager.cpp:249` `char configVar[120]` with `%119[^\/]`. The longest
/// registered reading is `backflushReminderThreshold` (27 bytes), so 64 is
/// generous and an over-long name is dropped rather than truncated into a
/// different parameter.
pub const READING_MAX: usize = 64;

/// The longest inbound `set` value, in bytes.
///
/// `MQTTManager.cpp:250` `data_str[1024]` is sized for a JSON-ish payload, but
/// every registered target is a number or a boolean, and the C++'s own handler
/// throws the rest away after `sscanf("%lf")` (`MQTTManager.cpp:268`). 32 bytes
/// holds any of them with room for the decimal expansion of a `f64`.
pub const VALUE_MAX: usize = 32;

/// How many inbound `set` commands may be waiting for the control task.
///
/// The control task drains every tick, so four is a whole tick's worth. A
/// Home Assistant entity that spams its command topic cannot grow this: the
/// oldest is dropped and counted, which is visible in `cc_hal_esp32::mqtt::Link::dropped` and in
/// the boot log rather than being a silent loss.
pub const INBOUND_DEPTH: usize = 8;

/// A reusable value buffer, so a publish pass touches the allocator zero times.
///
/// The C++'s `char data[256]` (`MQTTManager.cpp:405`) is a stack buffer reused
/// across every parameter and every sensor. This is the same buffer, as a
/// [`heapless::String`] so it can be handed to [`core::fmt::Write`] and so a
/// caller can build one once and pass it in on every pass.
///
/// The alternative — returning a fresh `String` per value, which is what
/// `publish_pass` used to ask its closure for — is one heap allocation per topic
/// per pass, roughly 250 a second on a configured machine, in the task that also
/// runs the heater deadman.
#[derive(Clone, Debug, Default)]
pub struct Payload(Bounded<PAYLOAD_MAX>);

impl Payload {
    /// An empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self(Bounded::new())
    }

    /// Empty it, ready for the next value.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// `snprintf(data, ..., "%d", on ? 1 : 0)` — `MQTTManager.cpp:417`.
    ///
    /// And the same two `if` arms for a boolean parameter
    /// (`MQTTManager.cpp:448-449`). Not `ON`/`OFF`: that spelling is the
    /// **binary sensor** path only (`MQTTManager.cpp:534`), and a parameter
    /// published as `ON` would be rejected by the `number`/`switch` entity
    /// Home Assistant built for it.
    pub fn set_bool(&mut self, on: bool) {
        self.set_int(i64::from(on));
    }

    /// `snprintf(data, ..., "%d", value)` — `MQTTManager.cpp:449`.
    pub fn set_int(&mut self, value: i64) {
        self.write(&format_args!("{value}"));
    }

    /// `snprintf(data, ..., "%.2f", value)` — `MQTTManager.cpp:452-455`, and
    /// `number2string` (`helperUtils.h:44-47`) for every polled sensor.
    pub fn set_float(&mut self, value: f64) {
        self.write(&format_args!("{value:.2}"));
    }

    /// A literal payload: the `ON`/`OFF` of a binary sensor
    /// (`MQTTManager.cpp:534`) and a text parameter (`MQTTManager.cpp:456`).
    ///
    /// A payload that does not fit [`PAYLOAD_MAX`] leaves the buffer **empty**,
    /// which [`Self::is_empty`] reports, rather than being silently
    /// half-published: a half-sent retained value is worse than none, and the
    /// buffer cannot grow.
    pub fn set_text(&mut self, text: &str) {
        self.0.clear();
        if self.0.push_str(text).is_err() {
            warn!(
                "mqtt: a value of {} B does not fit the {PAYLOAD_MAX} B buffer",
                text.len()
            );
            self.0.clear();
        }
    }

    /// Whether this already holds exactly `value`.
    ///
    /// The `mqttLastSent_[mqttTopic] != value` comparison of
    /// `MQTTManager.cpp:512`, `:520` and `:538`, as a method so the call site
    /// does not have to build a second [`Payload`] to compare against.
    #[must_use]
    pub fn holds(&self, value: &str) -> bool {
        self.0.as_str() == value
    }

    /// Replace the contents with `value`, without allocating.
    ///
    /// Public because the dedupe table that stores one `Payload` per topic lives
    /// in the publisher, which stayed in `cc-hal-esp32` (`mqtt.rs`'s
    /// `Feed::remember`); this is the operation it performs on a stored value.
    /// It was private before the extraction and could not stay that way across
    /// a crate boundary.
    pub fn overwrite(&mut self, value: &str) {
        self.0.clear();
        let _ = self.0.push_str(value);
    }

    /// Format into the buffer **directly**.
    ///
    /// Not `self.set_text(&format!("…"))`: `format!` builds a `String` on the
    /// heap first, which is one allocation per published value — about 46 a
    /// pass, five times a second, in the task that also runs the heater
    /// deadman. `heapless::String` is a `core::fmt::Write`, so the digits go
    /// straight into the reused buffer.
    ///
    /// A value that does not fit leaves the buffer empty (a partial write is
    /// visible, an empty one is not), which is what [`Self::is_empty`] reports.
    fn write(&mut self, args: &core::fmt::Arguments<'_>) {
        use core::fmt::Write as _;
        self.0.clear();
        if self.0.write_fmt(*args).is_err() {
            warn!("mqtt: a value did not fit the {PAYLOAD_MAX} B buffer");
            self.0.clear();
        }
    }

    /// The payload, as the bytes handed to `esp_mqtt_client_publish`.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// The payload, as the string the dedupe table compares.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether the buffer is empty, which a truncated write leaves it as.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One inbound `set`, as the C++'s `(configVar, value_double)` pair.
///
/// Owned into fixed buffers rather than `String`s because it is produced on the
/// `esp-mqtt` task inside its callback and consumed on the control task: an
/// allocation on either side would be heap traffic that this path can do without.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetCommand {
    /// The reading the topic named.
    pub reading: Bounded<READING_MAX>,
    /// The payload, as bytes.
    pub value: Bounded<VALUE_MAX>,
}
