//! Fixed-capacity MQTT buffers keep allocation off the control task.
//!
//! [`Payload`] is reused per topic; [`SetCommand`] crosses from the MQTT task.

use heapless::String as Bounded;
use log::warn;

/// MQTT receive and outbox buffers, in bytes.
///
/// Sized for a discovery payload with a `device` block, the largest this firmware
/// produces; `cc_config::discovery` asserts them under it. One constant for both
/// so they cannot drift.
pub const BUFFER_BYTES: usize = 1024;

/// Longest value payload, in bytes.
///
/// The widest value any topic can produce is a two-decimal number, under 16 B. 32
/// leaves headroom while keeping the buffer and the per-topic dedupe table small.
pub const PAYLOAD_MAX: usize = 32;

/// Longest reading name an inbound `set` topic may carry.
///
/// Generous. The failure it prevents is not a rejected command but a *silently
/// wrong* one: a truncated name is a different parameter, and the write lands
/// somewhere the operator did not ask for.
pub const READING_MAX: usize = 64;

/// Longest inbound `set` value, in bytes.
///
/// Every target is a number or a boolean; anything else the configuration's parser
/// refuses. 32 B holds an `f64`'s full decimal expansion.
pub const VALUE_MAX: usize = 32;

/// How many inbound `set` commands may wait for the control task.
///
/// A whole tick's worth at one drain per tick. An entity that spams its command
/// topic cannot grow this: the oldest is dropped and counted, which the boot log
/// reports, never silently lost.
pub const INBOUND_DEPTH: usize = 8;

/// Reusable value buffer, so a publish pass touches the allocator zero times.
///
/// The alternative — a fresh `String` per value — is hundreds of allocations a
/// second on the task that runs the heater deadman.
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

    /// `1` or `0`, **not** `ON`/`OFF`. `ON` is the binary-sensor spelling only; a
    /// parameter published as `ON` is rejected by the entity Home Assistant built
    /// for it.
    pub fn set_bool(&mut self, on: bool) {
        self.set_int(i64::from(on));
    }

    /// The decimal integer, unadorned.
    pub fn set_int(&mut self, value: i64) {
        self.write(&format_args!("{value}"));
    }

    /// Two decimal places, for every parameter *and* every polled sensor.
    pub fn set_float(&mut self, value: f64) {
        self.write(&format_args!("{value:.2}"));
    }

    /// A literal payload: a binary sensor's `ON`/`OFF`, or a text parameter.
    ///
    /// A value that does not fit leaves the buffer empty, not half-written:
    /// `is_empty` then reports it. A half-sent retained value is worse than none.
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
    /// The value-change dedupe, so a caller need not build a second [`Payload`]
    /// to compare against.
    #[must_use]
    pub fn holds(&self, value: &str) -> bool {
        self.0.as_str() == value
    }

    /// Replace the contents with `value`, without allocating.
    ///
    /// The dedupe table, one `Payload` per topic, lives in the publisher in
    /// `cc-hal-esp32`, so this crossed the crate boundary with it.
    pub fn overwrite(&mut self, value: &str) {
        self.0.clear();
        let _ = self.0.push_str(value);
    }

    /// Format into the buffer **directly**.
    ///
    /// Not `set_text(&format!("…"))`: that builds a `String` on the heap first, one
    /// allocation per value, hundreds a second on the heater-deadman task.
    /// `heapless::String` is a `core::fmt::Write`, so digits go straight in.
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

/// One inbound `set`: the reading a command topic named, and its payload.
///
/// Fixed buffers, not `String`s: it is produced on the `esp-mqtt` task and consumed
/// on the control task, and an allocation there is avoidable heap traffic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetCommand {
    /// The reading the topic named.
    pub reading: Bounded<READING_MAX>,
    /// The payload, as bytes.
    pub value: Bounded<VALUE_MAX>,
}
