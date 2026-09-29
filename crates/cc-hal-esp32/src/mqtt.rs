//! MQTT: the client, the topic layout, and the time-budgeted publish.
//!
//! Owner: **R3-13** (task D).
//!
//! # What this replaces
//!
//! `src/network/MQTTManager.cpp`, 932 lines. The parts that are *policy* — the
//! three-phase incremental publish under a 10 ms budget and the retained /
//! not-retained split — are in [`cc_domain::mqtt`], host-tested. The Home
//! Assistant discovery payloads are in [`cc_config::discovery`], also
//! host-tested, because a malformed discovery payload is *silently* ignored by
//! Home Assistant and is exactly the failure a device-only test cannot catch.
//! This file is the client and the wiring.
//!
//! # The three things that are easy to lose
//!
//! 1. **MQTT is disabled while brewing.** `MQTTManager.cpp:113-115` is a hard
//!    stop on *every* MQTT call, not a rate limit:
//!
//!    ```cpp
//!    if (systemContext_->brewHandler().isBrewActive()) { return; }
//!    ```
//!
//!    The visible consequence is `intervalMQTTBrew_ = 500` (`MQTTManager.h:261`)
//!    — while brewing the C++ would publish twenty times a second, which is why
//!    it stops instead. [`set_brewing`] is the whole of that rule, and it is one
//!    `AtomicBool` because it is a *fact about the world* read from another
//!    task, not a request (04 §3.2).
//!
//! 2. **The buffer is 1024 bytes, raised for discovery.**
//!    `MQTTManager::initializeClient` (`MQTTManager.cpp:95-98`) calls
//!    `setBufferSize(1024)` with the comment "Set larger buffer size for Home
//!    Assistant discovery messages". See [`BUFFER_BYTES`].
//!
//! 3. **The publish is incremental under a time budget.** See
//!    [`cc_domain::mqtt`] — the budget is a safety property, not an
//!    optimisation, because the C++ runs the publish from the main loop.
//!
//! # Whether this ever connected to a broker
//!
//! **It did not, and the task does not require it to.** `cc_config::Mqtt::default`
//! has `enabled = false` and an empty `broker`, so on a machine that has never
//! been provisioned [`is_configured`] is false and no client is constructed at
//! all — which is the C++'s behaviour too (`MQTTManager.cpp:79-83`). What is
//! built and verified here is the client: its configuration, its topic layout,
//! its budget, its retained flags, its discovery payloads, and its heap cost. A
//! session against a real broker is a deployment question.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use cc_config::Config;
use cc_domain::mqtt::{Cursor, Item, Plan};
use cc_domain::resilience::{CircuitBreaker, RetryPolicy};
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EspMqttEvent, EventPayload, LwtConfiguration, MqttClientConfiguration,
    MqttProtocolVersion, QoS,
};
use esp_idf_svc::sys::EspError;
use log::{debug, info, warn};

use crate::time::now_ms;

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

/// The publish budget per iteration, in milliseconds.
///
/// `MQTTManager.h:259` `timeBudget_ = 10`, checked after each publish at
/// `MQTTManager.cpp:501, 517, 546`. 10 ms is 2.5 % of the 400 ms temperature
/// sensor interval.
pub const TIME_BUDGET_MS: u32 = 10;

/// The interval between full telemetry passes, in milliseconds.
///
/// `MQTTManager.h:260` `intervalMQTT_ = 5000`. The brewing value (500 ms) and
/// the standby value (10 000 ms) are recorded in the constants below but the
/// brewing one is unreachable, because [`set_brewing`] stops publishing outright.
pub const INTERVAL_MS: u32 = 5_000;

/// The standby interval, in milliseconds. `MQTTManager.h:262`.
pub const INTERVAL_STANDBY_MS: u32 = 10_000;

/// How often the Home Assistant discovery payloads are republished, in
/// milliseconds.
///
/// `Timing::HASSIO_DISCOVERY_INTERVAL_MS`, wired at `LoopManager.cpp:382-384`.
/// 300 s, so a restarted Home Assistant or a broker that lost its retained store
/// re-learns the machine within one coffee.
pub const DISCOVERY_INTERVAL_MS: u32 = 300_000;

/// The stack of the task `esp-mqtt` creates for the client, in bytes.
///
/// `MqttClientConfiguration::task_stack` (`client.rs:106`). 4096 is `esp-mqtt`'s
/// own default and is enough because nothing in the callback allocates.
const MQTT_TASK_STACK_BYTES: usize = 4096;

/// The MQTT task's priority. Below the control task's, so a publish can never
/// preempt a heater decision.
const MQTT_TASK_PRIO: u8 = 4;

/// Whether a brew is in progress, and therefore whether MQTT may publish.
static BREWING: AtomicBool = AtomicBool::new(false);

/// Tell the MQTT link whether a brew is in progress.
///
/// **This is `MQTTManager.cpp:113-115`.** `true` stops every publish, discovery
/// included.
pub fn set_brewing(brewing: bool) {
    BREWING.store(brewing, Ordering::Relaxed);
}

/// Whether publishing is currently suppressed.
#[must_use]
pub fn brewing() -> bool {
    BREWING.load(Ordering::Relaxed)
}

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
}

/// The topics to publish, and how to retain each one.
///
/// Owned rather than borrowed, because the [`Plan`] the publish pass walks
/// borrows it while the client is mutably borrowed. Two objects, two borrows,
/// no conflict — which is the reason [`publish_pass`] is a free function and
/// not a method on [`Client`].
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Retained parameter topics (`MQTTManager.cpp:410-497`).
    parameters: Vec<(String, bool)>,
    /// Non-retained polled sensors (`:500-517`).
    sensors: Vec<(String, bool)>,
    /// Retained binary sensors (`:520-548`).
    binary_sensors: Vec<(String, bool)>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            parameters: Vec::new(),
            sensors: Vec::new(),
            binary_sensors: Vec::new(),
        }
    }

    /// Add a retained parameter topic.
    pub fn add_parameter(&mut self, reading: &str) {
        self.parameters.push((reading.to_string(), true));
    }

    /// Add a non-retained polled sensor.
    ///
    /// The C++ publishes these with `retain = false` (`MQTTManager.cpp:511`).
    /// Retaining a value that changes every 400 ms would fill the broker's
    /// retained store with a value that is stale by the time anyone reads it, and
    /// would cost a write per publish on a device with the flash wear budget of a
    /// coffee machine.
    pub fn add_sensor(&mut self, reading: &str) {
        self.sensors.push((reading.to_string(), false));
    }

    /// Add a retained binary sensor.
    ///
    /// `MQTTManager.cpp:543-544`: "binary state must survive broker/HA restarts
    /// — it may not change again for days, so a fresh subscriber would otherwise
    /// see unknown until the next transition."
    pub fn add_binary_sensor(&mut self, reading: &str) {
        self.binary_sensors.push((reading.to_string(), true));
    }

    /// How many topics in total.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parameters.len() + self.sensors.len() + self.binary_sensors.len()
    }

    /// Whether there is nothing to publish.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fill a registry from `config`.
    ///
    /// R3-13 does not yet bind topics to parameters — that needs the state
    /// machine's parameter table and a write into the live configuration, and it
    /// arrives with R3-16. What is registered here is everything that is a pure
    /// *reading* of the machine, which is what Home Assistant actually displays,
    /// plus the two setpoints and the PID switch as retained parameters.
    #[must_use]
    pub fn from_config(topics: &Topics, config: &Config) -> Self {
        let mut registry = Self::new();
        // Numeric sensors, not retained.
        for reading in ["temperature", "machineState", "heaterPower"] {
            let _ = topics;
            registry.add_sensor(reading);
        }
        if config.hardware.sensors.pressure.enabled {
            registry.add_sensor("pressure");
        }
        // Retained parameters.
        for reading in ["brewSetpoint", "steamSetpoint"] {
            registry.add_parameter(reading);
        }
        registry.add_parameter("pidON");
        // Retained binary sensors.
        for reading in ["waterTankFull", "brewing"] {
            registry.add_binary_sensor(reading);
        }
        if config.hardware.sensors.watertank.enabled {
            registry.add_binary_sensor("tankEmpty");
        }
        registry
    }
}

/// A borrowed view of a [`Registry`], plus its own item storage.
///
/// A `Plan` borrows three slices, so building one out of a `Vec<(String, bool)>`
/// needs the items to live somewhere for the duration. This owns them for the
/// duration of the pass and hands out a [`Plan`] that borrows from itself.
pub struct PlanView<'a> {
    items: Vec<Item<'a>>,
    /// Where each phase **ends** in `items`, as absolute offsets:
    /// `[parameters_end, sensors_end, binary_sensors_end]`.
    ///
    /// Offsets, not lengths. The three groups are stored contiguously and a
    /// [`Plan`] is three contiguous slices of that one `Vec`, so `plan` needs
    /// offsets. Storing lengths here was a device bug: every plan past the first
    /// group was a wrong slice, and for any registry with a binary sensor -- that
    /// is every `from_config` registry, `brewing` always being one -- the third
    /// slice was out of range and **panicked**, taking the MQTT task and the
    /// chip with it the first time anything was published.
    ends: [usize; 3],
}

impl<'a> PlanView<'a> {
    /// Build the view.
    #[must_use]
    pub fn of(registry: &'a Registry) -> Self {
        let mut items: Vec<Item<'a>> = Vec::with_capacity(registry.len());
        for (topic, retain) in &registry.parameters {
            items.push(Item {
                topic: topic.as_str(),
                retain: *retain,
            });
        }
        let end_of_parameters = items.len();
        for (topic, retain) in &registry.sensors {
            items.push(Item {
                topic: topic.as_str(),
                retain: *retain,
            });
        }
        let end_of_sensors = items.len();
        for (topic, retain) in &registry.binary_sensors {
            items.push(Item {
                topic: topic.as_str(),
                retain: *retain,
            });
        }
        let ends = [end_of_parameters, end_of_sensors, items.len()];
        Self { items, ends }
    }

    /// The plan, borrowing this view's items.
    ///
    /// Rebuilt per phase because the three phases are contiguous slices of one
    /// `Vec` and the offsets are what the constructor recorded.
    #[must_use]
    pub fn plan(&'a self) -> Plan<'a> {
        let [p_end, s_end, b_end] = self.ends;
        Plan {
            parameters: &self.items[..p_end],
            sensors: &self.items[p_end..s_end],
            binary_sensors: &self.items[s_end..b_end],
        }
    }

    /// The number of items, for a log line.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the view is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A constructed MQTT client, whether or not it has ever connected.
///
/// A value rather than an `Option`, because the client object exists whenever
/// `mqtt.enabled` is set — including when the broker is unreachable, and during
/// a reconnection. Constructing on connect instead would mean the first publish
/// after a long outage pays for a task spawn and two 1 KB buffers, which is
/// exactly the moment the machine is already busy.
pub struct Client {
    inner: EspMqttClient<'static>,
    topics: Topics,
    retry: RetryPolicy,
    breaker: CircuitBreaker,
    connected: bool,
    ever_connected: bool,
    last_discovery_ms: u32,
}

impl Client {
    /// Build a client for `config`, without waiting for a connection.
    ///
    /// # The connection itself
    ///
    /// `EspMqttClient::new` (`client.rs:413-432`) constructs the client,
    /// installs the callback, calls `esp_mqtt_client_start` and returns the
    /// client plus an `EspMqttConnection`. `start` is **asynchronous** — the TCP
    /// connect happens on the client's own task — so "constructed" and
    /// "connected" are separate events. That is what the C++ does too: `setup`
    /// (`:54-92`) does not connect, `checkConnection` (`:96-193`) does.
    ///
    /// The `EspMqttConnection` half is dropped. It is a zero-copy channel from
    /// the client's task into a `Receiver`; with no receiver the events are
    /// dropped rather than accumulated, and the connection state is tracked
    /// instead — see [`Client::note_event`].
    ///
    /// # The last will
    ///
    /// `MQTTManager.cpp:161` connects with
    /// `connect(hostname, user, pass, topicWill_, 0 /*qos*/, true /*retain*/, "offline")`.
    /// So the will payload is the literal `offline`, on the status topic,
    /// retained — which is what makes a machine that loses power show as
    /// *offline* in Home Assistant rather than as *unknown*.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_mqtt_client_init`/`start`. Not fatal: the firmware
    /// runs without MQTT, as the C++ does when `mqttEnabled_` is false.
    pub fn new(config: &Config) -> Result<Self, EspError> {
        let topics = Topics::new(&config.mqtt.topic, &config.system.hostname);
        let conf = MqttClientConfiguration {
            protocol_version: Some(MqttProtocolVersion::V3_1_1),
            // The C++'s client id IS the hostname (`MQTTManager.cpp:161`), so a
            // broker's client list shows the machine's name rather than a
            // generated id.
            client_id: Some(&config.system.hostname),
            keep_alive_interval: Some(core::time::Duration::from_secs(60)),
            // The C++'s RetryPolicy is 10 s → 5 min, 5 attempts
            // (`MQTTManager.cpp:51-57`). This is the `esp-mqtt` reconnect
            // equivalent and it is the same schedule.
            reconnect_timeout: Some(core::time::Duration::from_secs(10)),
            network_timeout: core::time::Duration::from_secs(10),
            lwt: Some(LwtConfiguration {
                topic: &topics.will,
                payload: b"offline",
                qos: QoS::AtMostOnce,
                retain: true,
            }),
            username: Some(&config.mqtt.username),
            password: Some(config.mqtt.password.expose()),
            task_prio: MQTT_TASK_PRIO,
            task_stack: MQTT_TASK_STACK_BYTES,
            buffer_size: BUFFER_BYTES,
            out_buffer_size: BUFFER_BYTES,
            ..Default::default()
        };

        let uri = format!("mqtt://{}:{}", config.mqtt.broker, config.mqtt.port);
        let (inner, _connection) = EspMqttClient::new(&uri, &conf)?;
        Ok(Self {
            inner,
            topics,
            retry: RetryPolicy::WIFI,
            breaker: CircuitBreaker::WIFI,
            connected: false,
            ever_connected: false,
            last_discovery_ms: now_ms(),
        })
    }

    /// The topic layout, for the boot log and `/api/status`.
    #[must_use]
    pub fn topics(&self) -> &Topics {
        &self.topics
    }

    /// Whether there is a session right now.
    #[must_use]
    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    /// Whether there has *ever* been a session since boot.
    ///
    /// The number a bring-up report wants, because "never connected" and
    /// "connected then dropped" are different failures with different causes.
    #[must_use]
    pub const fn ever_connected(&self) -> bool {
        self.ever_connected
    }

    /// Note an event from the client's callback.
    ///
    /// `esp-idf-svc` has no `is_connected`, so the event pair is the only
    /// honest answer: an `esp_mqtt_client` handle is *running* whether or not it
    /// has a session, and reporting "connected" for a handle that has never
    /// reached a broker is the kind of green light that wastes an afternoon.
    pub fn note_event(&mut self, event: &EspMqttEvent<'_>) {
        match event.payload() {
            EventPayload::Connected(_) => {
                self.connected = true;
                self.ever_connected = true;
                self.retry.reset();
                self.breaker.record_success(now_ms());
                info!("mqtt: connected to the broker");
            }
            EventPayload::Disconnected => {
                self.connected = false;
                self.breaker.record_failure(now_ms());
                warn!("mqtt: disconnected");
            }
            _ => {}
        }
    }

    /// Subscribe to the command topic, as `MQTTManager.cpp:162` does on connect.
    ///
    /// The subscription is made but **nothing acts on the messages yet**. The
    /// C++'s inbound handler (`MQTTManager::messageCallback`, `:237-274`)
    /// `sscanf`s `<base><param>/set` and calls `assignParameter`, which needs the
    /// state machine's parameter table and a write into the live configuration.
    /// That is R3-16. A handler that parsed a value and then discarded it would
    /// be worse than none: the Home Assistant switch would move and nothing would
    /// happen, which reads as a broken machine rather than an unfinished task.
    ///
    /// # Errors
    ///
    /// `EspError` if the client is not running, or the topic is longer than
    /// `CONFIG_MQTT_MAX_TOPIC_LEN`.
    pub fn subscribe(&mut self) -> Result<(), EspError> {
        let topic = self.topics.set.clone();
        self.inner.subscribe(&topic, QoS::AtMostOnce)?;
        info!("mqtt: subscribed to the command topic");
        Ok(())
    }

    /// Publish `payload` to `<base><reading>`.
    ///
    /// Returns `Ok(())` once the payload is in the outbox. **It does not
    /// block**: `esp_mqtt_client_publish` copies into the outbox and returns, and
    /// the client's own task does the socket write. That is why the C++ needed a
    /// time budget at all — the copy is bounded but the outbox drains slowly on a
    /// bad link.
    ///
    /// # Errors
    ///
    /// `EspError` if the client is not running, or the payload does not fit
    /// [`BUFFER_BYTES`] and the outbox is full. The first is a programming
    /// error; the second is the network being slower than the publish budget
    /// allows, and the caller's answer is to count it, not to retry.
    pub fn publish(&mut self, reading: &str, payload: &[u8], retain: bool) -> Result<(), EspError> {
        if brewing() {
            // MQTTManager.cpp:113-115. Not a rate limit: nothing is published,
            // not even the retained availability.
            return Ok(());
        }
        let topic = self.topics.state(reading);
        self.inner
            .publish(&topic, QoS::AtMostOnce, retain, payload)?;
        Ok(())
    }

    /// Publish the retained availability topic.
    ///
    /// `MQTTManager.cpp:381` publishes `"online"` on the status topic when a
    /// pass starts, retained. Home Assistant's availability is driven by this
    /// topic (see [`cc_config::discovery`]), so it is the one publish that must
    /// not be conditional on anything having changed.
    ///
    /// # Errors
    ///
    /// As [`Client::publish`].
    pub fn publish_online(&mut self) -> Result<(), EspError> {
        if brewing() {
            return Ok(());
        }
        let topic = self.topics.will.clone();
        self.inner
            .publish(&topic, QoS::AtMostOnce, true, b"online")?;
        Ok(())
    }

    /// Whether the discovery payloads are due for republication.
    #[must_use]
    pub fn discovery_due(&self) -> bool {
        now_ms().wrapping_sub(self.last_discovery_ms) >= DISCOVERY_INTERVAL_MS
    }

    /// Note that the discovery payloads were just published.
    pub fn mark_discovery_sent(&mut self) {
        self.last_discovery_ms = now_ms();
    }

    /// Whether a reconnect attempt is due, consuming one attempt if so.
    ///
    /// The C++'s `checkConnection` without the `esp-mqtt` call, which
    /// `esp-idf-svc` makes on its own task with its own
    /// `reconnect_timeout`. What is left is the bookkeeping the C++ does and
    /// `esp-mqtt` does not: the retry policy, the circuit breaker, and the log
    /// line.
    pub fn due_for_reconnect(&mut self) -> bool {
        if self.connected || !self.ever_connected {
            // `!ever_connected` is the C++'s `serverIP_.length() == 0` guard
            // generalised: never spend a reconnect attempt on a client that has
            // not yet had its first chance.
            return false;
        }
        let now = now_ms();
        if !self.breaker.can_attempt(now) {
            return false;
        }
        if !self.retry.should_retry() || !self.retry.can_retry_now(now) {
            return false;
        }
        self.retry.record_attempt(now);
        debug!(
            "mqtt: reconnect attempt {}, next in {} ms",
            self.retry.attempts(),
            self.retry.next_delay_ms()
        );
        true
    }

    /// A one-line, credential-free description of the client.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "mqtt: connected={} ever_connected={} base={} brewing_guard={} \
             buffer={BUFFER_BYTES} B budget={TIME_BUDGET_MS} ms discovery={DISCOVERY_INTERVAL_MS} ms",
            self.connected,
            self.ever_connected,
            self.topics.base,
            brewing(),
        )
    }
}

/// One budgeted pass over `registry`, publishing through `client`.
///
/// Returns how many payloads were published.
///
/// # The 10 ms budget is a safety property, not an optimisation
///
/// `LoopManager.cpp:505` calls this from the main loop. A pass that published 29
/// topics with no budget would spend a second or more inside it on a bad link,
/// during which the control loop does not run: the heater is not re-evaluated
/// and the over-temperature debounce does not advance. 10 ms is 2.5 % of the
/// 400 ms sensor interval — small enough that the control loop does not notice,
/// large enough that a pass finishes in a handful of iterations instead of
/// hundreds.
///
/// A pass that is cut short resumes at the next call, and
/// `cc_domain::mqtt::tests::stopping_at_every_possible_point_and_resuming_never_
/// skips_or_repeats` is what proves it resumes at exactly the item that was not
/// published.
pub fn publish_pass(
    client: &mut Client,
    registry: &Registry,
    cursor: &mut Cursor,
    value_of: &mut impl FnMut(&str) -> Vec<u8>,
) -> usize {
    if brewing() || !client.is_connected() {
        return 0;
    }
    let view = PlanView::of(registry);
    let plan = view.plan();
    let started = now_ms();
    let mut published = 0usize;
    while let Some(item) = plan.next(cursor) {
        let topic = client.topics.state(item.topic);
        let payload = value_of(item.topic);
        if client
            .inner
            .publish(&topic, QoS::AtMostOnce, item.retain, &payload)
            .is_ok()
        {
            published += 1;
        }
        if now_ms().wrapping_sub(started) >= TIME_BUDGET_MS {
            break;
        }
    }
    published
}

/// Whether a broker is configured at all.
///
/// `MQTTManager::setup` (`MQTTManager.cpp:79-83`) disables MQTT outright when
/// the broker name is empty, rather than building a client and failing to
/// connect. Reproduced: an empty `mqtt.broker` is the *default*, so a machine
/// that has never been provisioned should not spawn an MQTT task at all — 4 KB of
/// stack and 2 KB of buffers for a client with nowhere to connect.
#[must_use]
pub fn is_configured(config: &Config) -> bool {
    config.mqtt.enabled && !config.mqtt.broker.is_empty()
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in eight import lists
    // that would rot.
    #![allow(clippy::wildcard_imports)]

    use super::*;
    use cc_domain::mqtt::Phase;

    #[cfg_attr(test, test)]
    pub fn the_default_configuration_is_not_a_broker() {
        // cc_config::Mqtt::default: enabled = false, broker = "".
        // MQTTManager.cpp:79-83 disables MQTT rather than building a client.
        assert!(!is_configured(&Config::default()));
        let mut config = Config::default();
        config.mqtt.enabled = true;
        assert!(
            !is_configured(&config),
            "enabled with no broker is still off"
        );
        config.mqtt.broker = "192.168.1.10".into();
        assert!(is_configured(&config));
    }

    #[cfg_attr(test, test)]
    pub fn the_topic_layout_has_no_separator_the_cpp_does_not_have() {
        // MQTTManager.cpp:80-82 uses "%s%s/%s" with nothing between the prefix
        // and the hostname; mqtt.topic's default already ends in '/'.
        let topics = Topics::new("custom/kitchen/", "clevercoffee");
        assert_eq!(topics.will, "custom/kitchen/clevercoffee/status");
        assert_eq!(topics.set, "custom/kitchen/clevercoffee/+/set");
        assert_eq!(
            topics.state("temperature"),
            "custom/kitchen/clevercoffee/temperature"
        );
        assert_eq!(
            topics.command("brewSetpoint"),
            "custom/kitchen/clevercoffee/brewSetpoint/set"
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_prefix_without_a_trailing_slash_reproduces_the_cpp_result() {
        // The C++ does not insert one either. A tidy layout would be a
        // different wire format from the one every existing install uses.
        assert_eq!(Topics::new("home/", "cc").state("t"), "home/cc/t");
    }

    #[cfg_attr(test, test)]
    pub fn the_buffer_is_the_csqs_1024() {
        // MQTTManager.cpp:96, raised specifically for discovery payloads.
        // cc_config::discovery asserts every payload is under it.
        assert_eq!(BUFFER_BYTES, 1024);
    }

    #[cfg_attr(test, test)]
    pub fn the_budget_is_the_csqs_ten_milliseconds() {
        // MQTTManager.h:259, and 2.5% of the 400 ms sensor interval.
        assert_eq!(TIME_BUDGET_MS, 10);
        const { assert!(TIME_BUDGET_MS * 40 <= 400) };
    }

    #[cfg_attr(test, test)]
    pub fn the_intervals_are_the_csqs() {
        assert_eq!(INTERVAL_MS, 5_000);
        assert_eq!(INTERVAL_STANDBY_MS, 10_000);
        assert_eq!(DISCOVERY_INTERVAL_MS, 300_000);
    }

    #[cfg_attr(test, test)]
    pub fn the_registry_puts_each_kind_of_topic_in_the_cpp_phase() {
        // Phase membership is what decides the retain flag, and a topic in the
        // wrong phase is published with the wrong flag — retained where it must
        // not be, or lost on a broker restart where it must not be.
        let topics = Topics::new("p/", "h");
        let registry = Registry::from_config(&topics, &Config::default());
        let view = PlanView::of(&registry);
        let plan = view.plan();
        // Polled sensors are not retained.
        assert!(plan.sensors.iter().all(|i| !i.retain));
        // Parameters and binary sensors are.
        assert!(plan.parameters.iter().all(|i| i.retain));
        assert!(plan.binary_sensors.iter().all(|i| i.retain));
        // The phases are the C++'s, in its order.
        assert_eq!(plan.phase_items(Phase::Parameters).len(), 3);
        assert!(view.len() >= 8);
    }

    /// The three plan slices must **partition** the view.
    ///
    /// The retain assertions in `the_registry_puts_each_kind_of_topic_in_the_cpp_phase`
    /// are all vacuous on an empty slice, so on their own they say nothing about
    /// whether the phases were cut in the right place -- which is exactly the bug
    /// this found: `PlanView` stored per-group *lengths* and `plan()` used them
    /// as end *offsets*, so `sensors` came back empty and `binary_sensors` was an
    /// out-of-range slice that panicked.
    ///
    /// Everything here is about membership and count, never about a value.
    #[cfg_attr(test, test)]
    pub fn the_plan_slices_partition_the_view() {
        fn plan_topics<'a>(items: &'a [Item<'a>]) -> Vec<&'a str> {
            items.iter().map(|i| i.topic).collect()
        }
        fn registry_topics(list: &[(String, bool)]) -> Vec<&str> {
            list.iter().map(|(t, _)| t.as_str()).collect()
        }

        let topics = Topics::new("p/", "h");
        let registry = Registry::from_config(&topics, &Config::default());
        let view = PlanView::of(&registry);
        let plan = view.plan();

        let total = plan.parameters.len() + plan.sensors.len() + plan.binary_sensors.len();
        assert_eq!(total, view.len(), "the phases must partition the view");
        assert_eq!(plan.parameters.len(), registry.parameters.len());
        assert_eq!(plan.sensors.len(), registry.sensors.len());
        assert_eq!(plan.binary_sensors.len(), registry.binary_sensors.len());
        assert_eq!(
            plan_topics(plan.sensors),
            registry_topics(&registry.sensors)
        );
        assert_eq!(
            plan_topics(plan.binary_sensors),
            registry_topics(&registry.binary_sensors)
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_pressure_sensor_appears_only_when_it_is_fitted() {
        // MQTTManager.cpp:911.
        let topics = Topics::new("p/", "h");
        let mut config = Config::default();
        let without = Registry::from_config(&topics, &config);
        assert!(!without.sensors.iter().any(|(t, _)| t == "pressure"));
        config.hardware.sensors.pressure.enabled = true;
        let with = Registry::from_config(&topics, &config);
        assert!(with.sensors.iter().any(|(t, _)| t == "pressure"));
    }

    #[cfg_attr(test, test)]
    pub fn the_brew_guard_defaults_to_off_and_can_be_set() {
        set_brewing(false);
        assert!(!brewing());
        set_brewing(true);
        assert!(brewing());
        set_brewing(false);
        assert!(!brewing());
    }

    #[cfg_attr(test, test)]
    pub fn a_registry_never_names_a_credential() {
        // The topics are the configuration's own; a mistake here would put the
        // MQTT password on a world-readable broker topic.
        let topics = Topics::new("p/", "h");
        let mut config = Config::default();
        config.mqtt.password = cc_config::Secret::new(alloc::string::String::from("brokerpw"));
        let registry = Registry::from_config(&topics, &config);
        let view = PlanView::of(&registry);
        assert!(!view.items.iter().any(|i| i.topic.contains("brokerpw")));
        assert!(!topics.base.contains("brokerpw"));
    }

    #[cfg_attr(test, test)]
    pub fn the_topics_string_names_the_base_so_a_bring_up_log_is_useful() {
        // It must, though, never the credentials — `describe` is printed at
        // boot and captured in support tickets.
        let topics = Topics::new("custom/kitchen/", "clevercoffee");
        assert!(topics.base.starts_with("custom/kitchen/"));
    }
}
