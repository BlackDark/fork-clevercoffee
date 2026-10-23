//! ESP-IDF MQTT client, inbound channel, and budgeted publisher.
//!
//! [`cc_net::mqtt`] supplies topics and buffers; [`cc_net::mqtt::cursor`] supplies the cursor;
//! [`cc_config::discovery`] supplies Home Assistant payloads.
//!
//! [`Feed::service`] resumes within [`TIME_BUDGET_MS`] because it shares the heater's
//! control task. Reuse buffers, retain value-change deduplication, and keep
//! [`BUFFER_BYTES`] large enough for discovery. Brewing changes publish cadence
//! through [`interval_for`]; it suppresses reconnect attempts, not publishing.
//! The client must outlive its creation scope: dropping it destroys the session.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use cc_config::Config;
use cc_net::mqtt::cursor::{Cursor, Item, Phase};
use cc_net::resilience::{CircuitBreaker, RetryPolicy};
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EspMqttEvent, EventPayload, LwtConfiguration, MqttClientConfiguration,
    MqttProtocolVersion, QoS,
};
use esp_idf_svc::sys::EspError;
use heapless::{Deque, String as Bounded};
use log::{debug, info, warn};

use cc_hal_esp32::time::now_ms;

// Re-exported so the firmware names one module for MQTT; the definitions are
// portable and host-tested in `cc_net::mqtt`.
pub use cc_net::mqtt::{
    interval_for, is_configured, ParamTopic, Payload, PlanView, Registry, SetCommand, Topics,
    BACKFLUSH_ON, BUFFER_BYTES, CALIBRATION_ON, DISCOVERY_INTERVAL_MS, INBOUND_DEPTH,
    INTERVAL_BREW_MS, INTERVAL_MS, INTERVAL_STANDBY_MS, PAYLOAD_MAX, READING_MAX, STEAM_MODE,
    TARE_ON, TIME_BUDGET_MS, TOPIC_MAX, VALUE_MAX,
};

/// The stack of the task `esp-mqtt` creates for the client, in bytes.
///
/// `MqttClientConfiguration::task_stack` (`client.rs:106`). 4096 is `esp-mqtt`'s
/// own default and is enough because nothing in the callback allocates.
const MQTT_TASK_STACK_BYTES: usize = 4096;

/// The MQTT task's priority. Below the control task's, so a publish can never
/// preempt a heater decision.
const MQTT_TASK_PRIO: u8 = 4;
/// What the callback saw, shared with the [`Client`] that owns it.
///
/// `EspMqttClient::new_cb` runs the closure **on the `esp-mqtt` task**, so this
/// is a cross-task channel and is built from atomics and one short-held mutex.
///
/// # Why `new_cb` and not `new` + `EspMqttConnection`
///
/// `EspMqttClient::new` returns an `EspMqttConnection` whose `next()`
/// (`esp-idf-svc` `src/mqtt/client.rs:797-807`) calls
/// `Receiver::get_shared()` — which **blocks** until the producer has something
/// (`src/private/zerocopy.rs:29-40`, a `condvar.wait`). There is no
/// non-blocking poll and no `is_connected` accessor in the crate: an
/// `esp_mqtt_client` handle is *running* whether or not it has a session, so
/// reporting "connected" for a handle that has never reached a broker is the
/// kind of green light that wastes an afternoon.
///
/// A rendezvous on the 10 ms control tick would therefore stall the tick for
/// the length of a network event — the exact failure the [`TIME_BUDGET_MS`]
/// budget exists to prevent, and the watchdog's deadman is on the same
/// signal. So the events are handled **where they are delivered**, in the
/// callback, and what crosses back is two booleans and a bounded queue.
pub struct Link {
    /// The topic layout, needed to parse an inbound command.
    topics: Topics,
    /// Whether there is a session right now.
    connected: AtomicBool,
    /// Whether there has *ever* been a session since boot.
    ever_connected: AtomicBool,
    /// Inbound `set` commands waiting for the control task.
    inbound: Mutex<Deque<SetCommand, INBOUND_DEPTH>>,
    /// How many were dropped because the queue was full.
    dropped: AtomicU32,
    /// The retry policy and circuit breaker, driven from the events.
    backoff: Mutex<Backoff>,
}

/// When to retry, and when to stop trying.
struct Backoff {
    retry: RetryPolicy,
    breaker: CircuitBreaker,
}

impl Link {
    fn new(topics: &Topics) -> Self {
        Self {
            topics: topics.clone(),
            connected: AtomicBool::new(false),
            ever_connected: AtomicBool::new(false),
            inbound: Mutex::new(Deque::new()),
            dropped: AtomicU32::new(0),
            backoff: Mutex::new(Backoff {
                retry: RetryPolicy::WIFI,
                breaker: CircuitBreaker::WIFI,
            }),
        }
    }

    /// Whether there is a session right now.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// Whether there has *ever* been a session since boot.
    #[must_use]
    pub fn ever_connected(&self) -> bool {
        self.ever_connected.load(Ordering::Acquire)
    }

    /// How many inbound commands have been dropped for want of room.
    #[must_use]
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Take everything the callback has queued, oldest first.
    #[must_use]
    pub fn drain_inbound(&self) -> Vec<SetCommand> {
        let Ok(mut slot) = self.inbound.lock() else {
            return Vec::new();
        };
        // `heapless::Deque` has no `drain`, and popping front is the right order
        // anyway: each message is applied as it arrives, so two writes to the
        // same parameter land in the order they were sent.
        let mut out = Vec::with_capacity(slot.len());
        while let Some(command) = slot.pop_front() {
            out.push(command);
        }
        out
    }

    /// Note an event from the client's own task. See [`note_event_on`].
    fn note(&self, event: &EspMqttEvent<'_>) {
        note_event_on(self, event);
    }
}

/// Record one `esp-mqtt` event.
///
/// Called from the client's callback, on the `esp-mqtt` task. The two log lines
/// are once per connection change, not per message: an inbound `set` is logged
/// by the control task, after it has been acted on, because logging from here
/// would put a telnet-ring-buffer lock in the middle of the client's own task.
fn note_event_on(link: &Link, event: &EspMqttEvent<'_>) {
    match event.payload() {
        EventPayload::Connected(_) => {
            link.connected.store(true, Ordering::Release);
            link.ever_connected.store(true, Ordering::Release);
            if let Ok(mut backoff) = link.backoff.lock() {
                backoff.retry.reset();
                backoff.breaker.record_success(now_ms());
            }
            info!("mqtt: connected to the broker");
        }
        EventPayload::Disconnected => {
            link.connected.store(false, Ordering::Release);
            if let Ok(mut backoff) = link.backoff.lock() {
                backoff.breaker.record_failure(now_ms());
            }
            warn!("mqtt: disconnected");
        }
        EventPayload::Received {
            topic: Some(topic),
            data,
            ..
        } => receive(link, topic, data),
        _ => {}
    }
}

/// Parse one inbound `set` and queue it for the control task.
///
/// The topic parse happens here, because it is pure, and the write happens on
/// the control task, because that is the task that owns the configuration and
/// the store.
fn receive(link: &Link, topic: &str, data: &[u8]) {
    let Some(reading) = link.topics.command_of(topic) else {
        warn!("mqtt: invalid topic/command: {topic}");
        return;
    };
    let mut command = SetCommand {
        reading: Bounded::new(),
        value: Bounded::new(),
    };
    // A payload that is not UTF-8 cannot be a number, so it is refused here
    // rather than carried as bytes the configuration's own parser would have to
    // reject anyway.
    let Ok(text) = core::str::from_utf8(data) else {
        warn!("mqtt: command {reading} carried a non-UTF-8 payload");
        return;
    };
    if command.value.push_str(text).is_err() {
        warn!(
            "mqtt: command {reading} carried {} B, over the {VALUE_MAX} B limit",
            text.len()
        );
        return;
    }
    if command.reading.push_str(reading).is_err() {
        // Refusing is the honest answer, because a truncated name is a
        // *different* parameter.
        warn!("mqtt: command topic '{reading}' is longer than {READING_MAX} B");
        return;
    }
    let Ok(mut slot) = link.inbound.lock() else {
        return;
    };
    if slot.push_back(command).is_err() {
        // Drop-newest, counted. A burst from a misconfigured client would
        // otherwise grow an unbounded queue on a 320 KB chip.
        link.dropped.fetch_add(1, Ordering::Relaxed);
        warn!("mqtt: the inbound queue is full — a command was dropped");
    }
}

/// A constructed MQTT client, whether or not it has ever connected.
///
/// A value rather than an `Option`, because the client object exists whenever
/// `mqtt.enabled` is set — including when the broker is unreachable, and during
/// a reconnection. Constructing on connect instead would mean the first publish
/// after a long outage pays for a task spawn and two 1 KB buffers, which is
/// exactly the moment the machine is already busy.
///
/// # `Send`
///
/// Yes, and nothing in this firmware relies on it: `EspMqttClient` is
/// `unsafe impl Send` (`esp-idf-svc` `src/mqtt/client.rs:786`), `Link` is
/// atomics plus two short-held mutexes, and `Topics` is three `String`s. The
/// client is nonetheless built **inside** the control task rather than in
/// `bring_up` and moved through `ControlArgs`, because that is where the store,
/// the configuration and the machine are, and so it never has to be moved at
/// all.
pub struct Client {
    inner: EspMqttClient<'static>,
    link: std::sync::Arc<Link>,
    last_discovery_ms: u32,
}

impl Client {
    /// Build a client for `config`, without waiting for a connection.
    ///
    /// # The connection itself
    ///
    /// `EspMqttClient::new_cb` constructs the client, installs the callback and
    /// calls `esp_mqtt_client_start` (`client.rs:437-448`). `start` is
    /// **asynchronous** — the TCP connect happens on the client's own task — so
    /// "constructed" and "connected" are separate events, and only the events
    /// say when the second has happened.
    ///
    /// # The last will
    ///
    /// The will payload is the literal `offline`, on the status topic, retained
    /// — which is what makes a machine that loses power show as *offline* in
    /// Home Assistant rather than as *unknown*.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_mqtt_client_init`/`start`. Not fatal: the firmware
    /// runs without MQTT.
    pub fn new(config: &Config) -> Result<Self, EspError> {
        let topics = Topics::new(&config.mqtt.topic, &config.system.hostname);
        let conf = MqttClientConfiguration {
            protocol_version: Some(MqttProtocolVersion::V3_1_1),
            // The client id IS the hostname, so a broker's client list shows
            // the machine's name rather than a generated id.
            client_id: Some(&config.system.hostname),
            keep_alive_interval: Some(core::time::Duration::from_secs(60)),
            // `esp-mqtt`'s reconnect schedule: 10 s → 5 min, 5 attempts.
            reconnect_timeout: Some(core::time::Duration::from_secs(10)),
            network_timeout: core::time::Duration::from_secs(10),
            lwt: Some(LwtConfiguration {
                topic: &topics.will,
                payload: b"offline",
                qos: QoS::AtMostOnce,
                retain: true,
            }),
            username: Some(&config.mqtt.username),
            // `Secret::expose`, never `Display`: the credential must not reach a
            // log line or a heapless buffer that outlives the call.
            password: Some(config.mqtt.password.expose()),
            task_prio: MQTT_TASK_PRIO,
            task_stack: MQTT_TASK_STACK_BYTES,
            buffer_size: BUFFER_BYTES,
            out_buffer_size: BUFFER_BYTES,
            ..Default::default()
        };

        let uri = format!("mqtt://{}:{}", config.mqtt.broker, config.mqtt.port);
        let link = std::sync::Arc::new(Link::new(&topics));
        let callback_link = std::sync::Arc::clone(&link);
        let inner = EspMqttClient::new_cb(&uri, &conf, move |event| {
            callback_link.note(&event);
        })?;
        Ok(Self {
            inner,
            link,
            last_discovery_ms: now_ms(),
        })
    }

    /// The topic layout, for the boot log and `/api/status`.
    #[must_use]
    pub fn topics(&self) -> &Topics {
        &self.link.topics
    }

    /// The shared event state, for [`Feed`]'s reconnect bookkeeping.
    #[must_use]
    pub fn link(&self) -> &std::sync::Arc<Link> {
        &self.link
    }

    /// Whether there is a session right now.
    ///
    /// Read from the client's own task's events and **not** from a boot-time
    /// snapshot. `/api/status`'s `mqttConnected` used to read
    /// `Client::ever_connected()` immediately after `Client::new` returned,
    /// which is before the asynchronous TCP connect could possibly have
    /// completed — so the field was structurally always `false` and no code
    /// change could ever have made it otherwise.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.link.is_connected()
    }

    /// Whether there has *ever* been a session since boot.
    ///
    /// The number a bring-up report wants, because "never connected" and
    /// "connected then dropped" are different failures with different causes.
    #[must_use]
    pub fn ever_connected(&self) -> bool {
        self.link.ever_connected()
    }

    /// Take every inbound `set` the callback has queued.
    #[must_use]
    pub fn drain_inbound(&self) -> Vec<SetCommand> {
        self.link.drain_inbound()
    }

    /// Note an event, for a host test that has no client to deliver one.
    pub fn note_event(&self, event: &EspMqttEvent<'_>) {
        self.link.note(event);
    }

    /// Subscribe to the command topic, on connect.
    ///
    /// # When it is called
    ///
    /// From the control task, on the first tick after `Connected`. `esp-mqtt`
    /// redelivers subscriptions on its own for a session it restores, so this is
    /// called once per process rather than once per reconnect.
    ///
    /// # Errors
    ///
    /// `EspError` if the client is not running, or the topic is longer than
    /// `CONFIG_MQTT_MAX_TOPIC_LEN`.
    pub fn subscribe(&mut self) -> Result<(), EspError> {
        self.inner
            .subscribe(self.link.topics.set.as_str(), QoS::AtMostOnce)?;
        info!("mqtt: subscribed to the command topic");
        Ok(())
    }

    /// Publish `payload` to a fully-built `topic`.
    ///
    /// Returns `Ok(())` once the payload is in the outbox. **It does not
    /// block**: `esp_mqtt_client_publish` copies into the outbox and returns, and
    /// the client's own task does the socket write. The copy is bounded, but the
    /// outbox drains slowly on a bad link, which is why the pass has a budget.
    ///
    /// The topic is taken as a `&str` rather than built here, because building it
    /// would allocate and this is called up to thirty times inside a 10 ms budget
    /// on the control tick. [`Feed`] owns the reusable buffer.
    ///
    /// # Errors
    ///
    /// `EspError` if the client is not running, or the payload does not fit
    /// [`BUFFER_BYTES`] and the outbox is full. The first is a programming
    /// error; the second is the network being slower than the publish budget
    /// allows, and the caller's answer is to count it, not to retry.
    pub fn publish_to(
        &mut self,
        topic: &str,
        payload: &[u8],
        retain: bool,
    ) -> Result<(), EspError> {
        self.inner
            .publish(topic, QoS::AtMostOnce, retain, payload)?;
        Ok(())
    }

    /// Publish the retained availability topic.
    ///
    /// Published when a pass starts. **Retained**: it costs one write per pass,
    /// matches the retained `offline` last will on the same topic, and removes
    /// the window in which a broker restart leaves every entity unavailable.
    ///
    /// # Errors
    ///
    /// As [`Client::publish_to`].
    pub fn publish_online(&mut self) -> Result<(), EspError> {
        // Borrowed, not cloned: this runs once per pass inside the control
        // tick's budget, and `Topics::will` outlives the call.
        self.inner.publish(
            self.link.topics.will.as_str(),
            QoS::AtMostOnce,
            true,
            b"online",
        )?;
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
    /// Without the `esp-mqtt` call, which `esp-idf-svc` makes on its own task
    /// with its own `reconnect_timeout`. What is left is the bookkeeping
    /// `esp-mqtt` does not do: the retry policy, the circuit breaker, and the log
    /// line.
    #[must_use]
    pub fn due_for_reconnect(&self) -> bool {
        if self.is_connected() || !self.ever_connected() {
            // `!ever_connected`: never spend a reconnect attempt on a client
            // that has not yet had its first chance.
            return false;
        }
        let now = now_ms();
        let Ok(mut backoff) = self.link.backoff.lock() else {
            return false;
        };
        if !backoff.breaker.can_attempt(now) {
            return false;
        }
        if !backoff.retry.should_retry() || !backoff.retry.can_retry_now(now) {
            return false;
        }
        backoff.retry.record_attempt(now);
        debug!(
            "mqtt: reconnect attempt {}, next in {} ms — esp-mqtt is doing the connecting",
            backoff.retry.attempts(),
            backoff.retry.next_delay_ms()
        );
        true
    }

    /// A one-line, credential-free description of the client.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "mqtt: connected={} ever_connected={} base={} buffer={BUFFER_BYTES} B \
             budget={TIME_BUDGET_MS} ms discovery={DISCOVERY_INTERVAL_MS} ms",
            self.is_connected(),
            self.ever_connected(),
            self.link.topics.base,
        )
    }
}

/// What one budgeted step did, for the control task's log line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Service {
    /// Payloads handed to the outbox this call.
    pub published: u16,
    /// Topics for which the machine had no value.
    pub unresolved: u16,
    /// Whether a pass is in progress and the budget ran out mid-way.
    pub cut_short: bool,
}

/// Everything a budgeted publish pass must remember between calls.
///
/// The dedupe table, the cursor, the phase and the last-pass stamp.
///
/// # Why this is built once and never rebuilt
///
/// The `PlanView` the [`cc_net::mqtt::cursor::Cursor`] walks borrows the registry's
/// topic strings, so a `Feed` that owns both needs a self-reference — or a
/// registry that outlives it. The registry is built from the configuration once,
/// at boot, and **never changes**: nothing adds an MQTT topic after bring-up,
/// and the four conditional groups are decided by hardware that is fixed for the
/// life of the machine. So the registry is
/// leaked once ([`Box::leak`]) and the plan is built against `'static`, and the
/// alternative — rebuilding a `Vec` of ~60 `Item`s every pass, five times a
/// second — is an allocation the control tick does not need to make.
///
/// # Why the buffers are here
///
/// `topic`, `payload` and the [`BTreeMap`] of last-sent values are all reused.
/// See [`Payload`].
pub struct Feed {
    /// The topic registry, leaked at boot.
    registry: &'static Registry,
    /// The flattened three-phase plan, built once from `registry` and **leaked
    /// alongside it**.
    ///
    /// A `&'static` rather than an owned `PlanView` so that `plan()` yields a
    /// `Plan<'static>`: an owned view would hand back a plan whose lifetime is
    /// tied to `&self`, and the item topics would be unusable as the
    /// `&'static str` keys of `Self::last_sent` without re-borrowing the very
    /// field the publish loop is iterating.
    view: &'static PlanView<'static>,
    /// Where the pass is: the cursor and the phase.
    cursor: Cursor,
    /// The value-change dedupe, per topic.
    ///
    /// Keys are `&'static str` borrowed from the leaked registry, and values are
    /// fixed-size [`Payload`]s, so neither inserting a first value nor replacing
    /// a changed one touches the allocator. A `BTreeMap` rather than a `Vec`
    /// because the key is the topic name.
    last_sent: BTreeMap<&'static str, Payload>,
    /// When the pass in progress started.
    ///
    /// Zero at construction, so the first pass is due as soon as the interval has
    /// elapsed since boot.
    last_pass_ms: u32,
    /// The Home Assistant discovery messages, rebuilt at the start of each cycle.
    discovery: Vec<cc_config::discovery::Discovery>,
    /// Where in `discovery` the budgeted publish has got to.
    discovery_cursor: usize,
    /// `char topic[120]`, reused. See [`TOPIC_MAX`].
    topic: Bounded<TOPIC_MAX>,
    /// `char data[256]`, reused. See [`Payload`].
    payload: Payload,
    /// The topic prefix, copied once so building a topic needs no borrow of the
    /// client while the client is mutably borrowed.
    base: Bounded<TOPIC_MAX>,
}

impl Feed {
    /// Build the durable publish state for `config`.
    ///
    /// `topics` is the same layout [`Client`] built, and `config` is the control
    /// task's live copy — the conditional groups are read from it here, once.
    #[must_use]
    pub fn new(topics: &Topics, config: &Config) -> Self {
        let registry: &'static Registry =
            Box::leak(Box::new(Registry::from_config(topics, config)));
        let view: &'static PlanView<'static> = Box::leak(Box::new(PlanView::of(registry)));
        let mut base = Bounded::new();
        // `TOPIC_MAX` is the whole topic budget and the base is a prefix of it,
        // so this cannot fail for a topic the client itself built.
        let _ = base.push_str(topics.base.as_str());
        info!(
            "mqtt: {} parameters, {} sensors, {} binary sensors",
            view.len(),
            registry.sensors().len(),
            registry.binary_sensors().len()
        );
        Self {
            registry,
            view,
            cursor: Cursor::new(),
            last_sent: BTreeMap::new(),
            last_pass_ms: 0,
            discovery: Vec::new(),
            discovery_cursor: 0,
            topic: Bounded::new(),
            payload: Payload::new(),
            base,
        }
    }

    /// The registry, for resolving an inbound reading to a configuration key.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        self.registry
    }

    /// The same registry, with the lifetime it was built with.
    ///
    /// The caller building a publish closure needs the topic→key map **while**
    /// `service` is walking the same topics, and an owned `&Registry` borrowed
    /// from `&self` would be an immutable borrow of the feed across a mutable
    /// one. The reference is `Copy` and `'static`, so handing it out costs
    /// nothing and collides with nothing.
    #[must_use]
    pub const fn static_registry(&self) -> &'static Registry {
        self.registry
    }

    /// How far through the current pass the cursor is, for a log line.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.cursor.phase()
    }

    /// One budgeted step of the connection check and the publish pass.
    ///
    /// Called from the control task's 10 ms tick.
    ///
    /// # The shape, in five lines
    ///
    /// 1. Nothing at all without a session — no connected client short-circuits
    ///    before anything is sent.
    /// 2. Then Home Assistant discovery, if its own 300 s timer is due, one
    ///    budgeted slice of the document list, rebuilt from the live
    ///    configuration when the cycle starts.
    /// 3. Then, if `now - last_pass_ms >= interval_ms`, the pass starts: the
    ///    last-pass stamp is set and the retained `online` is published.
    /// 4. The cursor walks parameters → sensors → binary sensors, and **stops**
    ///    the moment [`TIME_BUDGET_MS`] has elapsed, resuming at the next topic on
    ///    the next call.
    /// 5. A topic whose value has not changed is skipped without a publish, and
    ///    the outbox is not written at all in that case.
    ///
    /// # Allocation
    ///
    /// **None, from the second pass onwards**, and that is a property of this
    /// signature rather than an accident. `topic` and `payload` are reused
    /// buffers, the dedupe table's values are fixed-size, and `values` writes
    /// into a buffer the caller reuses across calls.
    ///
    /// Exactly three things allocate, and none of them is per topic:
    ///
    /// * the first publish of each topic inserts one node into
    ///   `Self::last_sent` — once per topic, ever, so at most
    ///   `registry().len()` times in the life of the process;
    /// * `cc_config::discovery::all`, once per [`DISCOVERY_INTERVAL_MS`] at the
    ///   start of a cycle;
    /// * `Box::leak` in [`Self::new`], once, at boot.
    ///
    /// # `values`
    ///
    /// Writes the value for `reading` into `payload` and returns `true`, or
    /// returns `false` for a topic this build has no producer for. This counts it
    /// in [`Service::unresolved`] instead of logging per topic, because a
    /// disconnected sensor would otherwise produce fifty identical lines a pass.
    pub fn service(
        &mut self,
        client: &mut Client,
        config: &Config,
        now: u32,
        interval_ms: u32,
        values: &mut impl FnMut(&str, &mut Payload) -> bool,
    ) -> Service {
        let mut out = Service::default();
        if !client.is_connected() {
            // The gate is before everything, including the availability publish.
            // A machine with no session must not queue a pass it cannot finish.
            return out;
        }

        let started = now;

        // ---- 1. Home Assistant discovery, on its own 300 s timer ----------
        //
        // Every `HASSIO_DISCOVERY_INTERVAL_MS` the set is built at the start of
        // the cycle and published a few per call under the same budget, so 29
        // entities do not become a 300 ms stall in the control task.
        if client.discovery_due() {
            if self.discovery_cursor == 0 {
                self.discovery = cc_config::discovery::all(config);
                info!(
                    "mqtt: advertising {} Home Assistant entities",
                    self.discovery.len()
                );
            }
            out.published = out
                .published
                .saturating_add(self.discovery_step(client, started));
            if self.discovery_cursor >= self.discovery.len() {
                self.discovery_cursor = 0;
                client.mark_discovery_sent();
                info!("mqtt: discovery published");
            } else {
                out.cut_short = true;
            }
        }

        // ---- 2. the telemetry pass ------------------------------------------
        if now.wrapping_sub(self.last_pass_ms) < interval_ms {
            return out;
        }
        if self.cursor.phase() == Phase::Parameters && self.cursor.index() == 0 {
            // A cursor at the start of the list is this branch: it is true
            // exactly once per pass, so the retained availability is republished
            // on the pass's cadence and not on every resumed slice.
            self.last_pass_ms = now;
            if let Err(err) = client.publish_online() {
                warn!("mqtt: the availability publish failed: {err:?}");
            } else {
                out.published = out.published.saturating_add(1);
            }
        }

        let plan = self.view.plan();
        while let Some(item) = plan.next(&mut self.cursor) {
            self.payload.clear();
            if !values(item.topic, &mut self.payload) || self.payload.is_empty() {
                out.unresolved = out.unresolved.saturating_add(1);
            } else {
                self.publish_one(client, item, &mut out);
            }
            // After *every* topic, published or skipped, because a skipped topic
            // costs a lookup too.
            if now_ms().wrapping_sub(started) >= TIME_BUDGET_MS {
                out.cut_short = true;
                break;
            }
        }
        out
    }

    /// Publish one item, honouring the value-change dedupe.
    fn publish_one(&mut self, client: &mut Client, item: Item<'static>, out: &mut Service) {
        let value = self.payload.as_str();
        // The dedupe is applied to all three phases, not just the parameters.
        if self
            .last_sent
            .get(item.topic)
            .is_some_and(|sent| sent.holds(value))
        {
            return;
        }
        self.topic.clear();
        if self.topic.push_str(self.base.as_str()).is_err()
            || self.topic.push_str(item.topic).is_err()
        {
            // Unreachable for a topic this build made: `base` is a strict prefix
            // of a topic that already fits `TOPIC_MAX` when joined.
            warn!(
                "mqtt: topic '{}{}' exceeds {TOPIC_MAX} B",
                self.base, item.topic
            );
            return;
        }
        if client
            .publish_to(self.topic.as_str(), self.payload.as_bytes(), item.retain)
            .is_ok()
        {
            out.published = out.published.saturating_add(1);
            self.remember(item.topic);
        } else {
            // A failed publish is **not** recorded in the dedupe, so the next
            // pass retries it. Only success advances the dedupe.
            debug!("mqtt: {} was not published", item.topic);
        }
    }

    /// Record a published value for `topic`, without allocating.
    fn remember(&mut self, topic: &'static str) {
        let value = self.payload.as_str();
        if let Some(slot) = self.last_sent.get_mut(topic) {
            slot.overwrite(value);
        } else {
            let mut stored = Payload::new();
            stored.overwrite(value);
            let _ = self.last_sent.insert(topic, stored);
        }
    }

    /// Publish as many outstanding discovery documents as the budget allows.
    fn discovery_step(&mut self, client: &mut Client, started: u32) -> u16 {
        let mut published = 0_u16;
        while self.discovery_cursor < self.discovery.len() {
            let message = &self.discovery[self.discovery_cursor];
            // Retained for the discovery documents, which is the arm that
            // matters: an unretained discovery document is forgotten by the next
            // broker restart and Home Assistant deletes the entity.
            if client
                .publish_to(message.topic.as_str(), message.payload.as_bytes(), true)
                .is_ok()
            {
                published = published.saturating_add(1);
            } else {
                debug!("mqtt: discovery for {} was not published", message.topic);
            }
            self.discovery_cursor += 1;
            if now_ms().wrapping_sub(started) >= TIME_BUDGET_MS {
                break;
            }
        }
        published
    }
}
