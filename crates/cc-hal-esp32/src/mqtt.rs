//! MQTT: the client, the topic layout, and the time-budgeted publish.
//!
//! Owner: **R3-13** (task D).
//!
//! # What this replaces
//!
//! `src/network/MQTTManager.cpp`, 932 lines, plus the registration half of
//! `SystemInitializer::registerMQTTParameters` / `registerMQTTSensors`
//! (`SystemInitializer.cpp:687-800`). The parts that are *policy* — the
//! three-phase incremental publish under a 10 ms budget and the retained /
//! not-retained split — are in [`cc_domain::mqtt`], host-tested. The Home
//! Assistant discovery payloads are in [`cc_config::discovery`], also
//! host-tested, because a malformed discovery payload is *silently* ignored by
//! Home Assistant and is exactly the failure a device-only test cannot catch.
//! This file is the client, the registry, and the wiring.
//!
//! # The three things that are easy to lose
//!
//! 1. **The publish is incremental under a time budget.** See
//!    [`cc_domain::mqtt`] — the budget is a safety property, not an
//!    optimisation, because the C++ runs the publish from the main loop
//!    (`LoopManager.cpp:505`). [`Feed::service`] reproduces the C++'s shape:
//!    a cursor over the three phases, resumed across calls, with a
//!    [`TIME_BUDGET_MS`] budget on each call.
//! 2. **The buffer is 1024 bytes, raised for discovery.** See [`BUFFER_BYTES`].
//! 3. **The value-change dedupe.** `mqttLastSent_[topic] != value`
//!    (`MQTTManager.cpp:512`, `:520`, `:538`) is what stops a machine with a
//!    steady temperature from republishing thirty identical topics five times a
//!    second. See [`Feed`].
//!
//! # What is NOT what the C++ does
//!
//! **Brewing does not stop publishing.** An earlier revision of this file
//! carried a `BREWING` flag that made every `publish` a no-op while a brew was
//! running, justified as "`MQTTManager.cpp:113-115` is a hard stop on *every*
//! MQTT call". **That was a misreading of the oracle and it is gone.** Line
//! 113 is inside [`MQTTManager::checkConnection`], which is the *connection*
//! bookkeeping: it stops reconnect attempts, and `loop()` still runs. The
//! publish itself is in `writeSysParamsToMQTT`, which has no brew guard at all
//! and *selects a 500 ms interval* while a brew state is active
//! (`MQTTManager.cpp:385-387`). The flag was never even set by any caller, so
//! the behaviour was already parity; what was wrong was the comment. See
//! [`interval_for`].
//!
//! # Whether this ever connected to a broker
//!
//! **It did not.** [`Client::new`] was called in `bring_up` and the client was
//! dropped at the end of the `match` arm that created it — `EspMqttClient` has
//! an `impl Drop` that calls `esp_mqtt_client_destroy`
//! (`esp-idf-svc` `src/mqtt/client.rs:742-747`), so it was destroyed
//! microseconds after `esp_mqtt_client_start`, and nothing anywhere called
//! `publish`, `publish_online`, `discovery_due`, `due_for_reconnect` or
//! `subscribe`. The client now lives in the control task, which drives it from
//! its 10 ms tick exactly as `LoopManager::updateNetwork`
//! (`LoopManager.cpp:485-508`) drives the C++'s.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use cc_config::Config;
use cc_domain::mqtt::{Cursor, Item, Phase, Plan};
use cc_domain::resilience::{CircuitBreaker, RetryPolicy};
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EspMqttEvent, EventPayload, LwtConfiguration, MqttClientConfiguration,
    MqttProtocolVersion, QoS,
};
use esp_idf_svc::sys::EspError;
use heapless::{Deque, String as Bounded};
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
/// **Re-derived against this firmware's control period, not the C++'s.** The
/// C++ sets `timeBudget_ = 10` (`MQTTManager.h:259`), checked after each publish
/// at `MQTTManager.cpp:501, 517, 546`, and justifies it against a 400 ms
/// temperature-sensor interval. R4-01 moved the loop to 100 Hz, so that
/// justification is stale and the ratio is not: [`Feed::service`] is called from
/// **every** tick, and the control period is **10 ms**
/// (`cc-firmware/src/main.rs:189`, `CONTROL_PERIOD_MS`). A budget equal to the
/// whole period bounds nothing — it permits one publish attempt to occupy a
/// tick entirely, and the control task's own work (the SENSE reading, the PID,
/// the applier, the display hand-off) is what gets squeezed instead.
///
/// At 2 ms, a full pass of the ~46 registered topics still finishes well inside
/// the slowest interval that matters ([`INTERVAL_BREW_MS`], 500 ms) even when
/// every publish misses the budget and one topic is published per tick, so the
/// budget costs throughput nothing; it only costs a broker that has stopped
/// draining its outbox. 20 % of the period leaves the rest of the tick to the
/// machine.
pub const TIME_BUDGET_MS: u32 = 2;

/// The interval between full telemetry passes, in milliseconds.
///
/// `MQTTManager.h:260` `intervalMQTT_ = 5000`.
pub const INTERVAL_MS: u32 = 5_000;

/// The interval while a brew state is active, in milliseconds.
///
/// `MQTTManager.h:261` `intervalMQTTBrew_ = 500`. See [`interval_for`] and the
/// module documentation on why this is reachable.
pub const INTERVAL_BREW_MS: u32 = 500;

/// The interval in `STANDBY`, in milliseconds. `MQTTManager.h:262`.
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

/// The longest topic this firmware will build, in bytes.
///
/// `MQTTManager.cpp:194` `char topic[120]`, and the C++'s `snprintf` truncates
/// to it. Reproduced: a topic longer than this is a configuration error, and
/// silently publishing to a truncated topic would be worse than refusing.
pub const TOPIC_MAX: usize = 120;

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
/// oldest is dropped and counted, which is visible in [`Link::dropped`] and in
/// the boot log rather than being a silent loss.
pub const INBOUND_DEPTH: usize = 8;

/// The interval [`interval_for`] selects, from the machine state.
///
/// `MQTTManager.cpp:384-387`:
///
/// ```cpp
/// bool isBrewActive = (isBrewState(currentState) && currentState != BREW_FINISHED);
/// unsigned long interval = isBrewActive ? intervalMQTTBrew_
///                        : (currentState == STANDBY) ? intervalMQTTStandby_
///                        : intervalMQTT_;
/// ```
///
/// `BREW_FINISHED` is excluded from the brew arm, which is the same predicate
/// `BrewHandler::isBrewActive` uses (`BrewHandler.h:98-103`) and the same one
/// `cc_domain::state::MachineState::is_brew_state` plus the explicit exclusion
/// reproduces.
#[must_use]
pub fn interval_for(state: cc_domain::state::MachineState) -> u32 {
    let brew_active =
        state.is_brew_state() && state != cc_domain::state::MachineState::BrewFinished;
    if brew_active {
        INTERVAL_BREW_MS
    } else if state == cc_domain::state::MachineState::Standby {
        INTERVAL_STANDBY_MS
    } else {
        INTERVAL_MS
    }
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

/// The topics to publish, and how to retain each one.
///
/// Owned rather than borrowed, because the [`Plan`] the publish pass walks
/// borrows it while the client is mutably borrowed. Two objects, two borrows,
/// no conflict.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Retained parameter topics (`MQTTManager.cpp:410-497`).
    parameters: Vec<ParamTopic>,
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

    /// Add a retained parameter topic bound to a configuration key.
    pub fn add_parameter(&mut self, reading: &str, key: &'static str) {
        self.parameters.push(ParamTopic {
            topic: reading.to_string(),
            retain: true,
            key,
        });
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

    /// The parameter topics, in registration order.
    #[must_use]
    pub fn parameters(&self) -> &[ParamTopic] {
        &self.parameters
    }

    /// The polled sensor topics, in registration order.
    #[must_use]
    pub fn sensors(&self) -> &[(String, bool)] {
        &self.sensors
    }

    /// The binary sensor topics, in registration order.
    #[must_use]
    pub fn binary_sensors(&self) -> &[(String, bool)] {
        &self.binary_sensors
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

    /// The configuration key an inbound reading maps to, if it is registered.
    ///
    /// `MQTTManager::assignParameter`'s `mqttVars_.find(param)`
    /// (`MQTTManager.cpp:288`). `None` is the C++'s
    /// `"MQTT topic %s not found in mapping"`: an inbound `set` for a topic this
    /// machine does not publish is refused rather than guessed at.
    #[must_use]
    pub fn resolve(&self, reading: &str) -> Option<&'static str> {
        self.parameters
            .iter()
            .find(|p| p.topic == reading)
            .map(|p| p.key)
    }

    /// Fill a registry from `config`: the C++'s registration, in its order.
    ///
    /// `SystemInitializer::registerMQTTParameters` and `registerMQTTSensors`
    /// (`src/core/SystemInitializer.cpp:687-800`), verbatim in membership and
    /// in order — 12 unconditional parameters, 14 more behind
    /// `hardware.switches.brew.enabled`, 5 or 6 behind the scale; 9
    /// unconditional sensors plus `currBrewTime`, the two weights and
    /// `pressure`; and the single binary sensor `waterTankFull`.
    ///
    /// # What this replaces
    ///
    /// Three parameters, two setpoints, `pidON`, three "sensors" and two binary
    /// sensors. Two of those are **not in the C++ and not advertised by
    /// `cc_config::discovery`** — a `brewing` and a `tankEmpty` binary sensor
    /// that no Home Assistant entity ever subscribed to, i.e. the "advertised but
    /// inert" shape reached from the publishing side. One, `weight`, is the
    /// **opposite** defect: it was published while `cc_config::discovery`
    /// advertises `currReadingWeight` and `currBrewWeight`
    /// (`MQTTManager.cpp:903-904`), which are two different topics, so the weight
    /// Home Assistant showed stayed `unknown` forever.
    ///
    /// # The topic/key pairs are the C++'s, not the discovery table's
    ///
    /// `SystemInitializer.cpp:695` registers `pidUsePonM` while
    /// `MQTTManager.cpp:869` advertises the entity as `usePonM`. The two
    /// disagree in the C++ too — the Home Assistant switch moves a topic the
    /// registry does not know, so it lands in the `sscanf` arm, finds no
    /// mapping and is dropped. Reproduced, because "fixing" it here would make
    /// the port accept a command the C++ silently ignores, and the fix belongs
    /// where the advertisement is built (`cc_config::discovery`).
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "this IS the C++'s two registration functions. Splitting the \
                  32 parameters from the 13 sensors would hide the property the \
                  function exists to provide, which is that the conditional \
                  blocks are the C++'s conditional blocks."
    )]
    pub fn from_config(topics: &Topics, config: &Config) -> Self {
        let mut registry = Self::new();
        let _ = topics;

        // ---- parameters: the twelve unconditional ones, :690-701 ----
        registry.add_parameter("pidON", "pid.enabled");
        registry.add_parameter("brewSetpoint", "brew.setpoint");
        registry.add_parameter("brewTempOffset", "brew.temp_offset");
        registry.add_parameter("steamON", STEAM_MODE);
        registry.add_parameter("steamSetpoint", "steam.setpoint");
        registry.add_parameter("pidUsePonM", "pid.use_ponm");
        registry.add_parameter("aggKp", "pid.regular.kp");
        registry.add_parameter("aggTn", "pid.regular.tn");
        registry.add_parameter("aggTv", "pid.regular.tv");
        registry.add_parameter("aggIMax", "pid.regular.i_max");
        registry.add_parameter("steamKp", "pid.steam.kp");
        registry.add_parameter("standbyModeOn", "standby.enabled");

        // ---- parameters: behind the brew switch, :705-719 ----
        if config.hardware.switches.brew.enabled {
            registry.add_parameter("aggbKp", "pid.bd.kp");
            registry.add_parameter("aggbTn", "pid.bd.tn");
            registry.add_parameter("aggbTv", "pid.bd.tv");
            registry.add_parameter("pidUseBD", "pid.bd.enabled");
            registry.add_parameter("brewPidDelay", "brew.pid_delay");
            registry.add_parameter("targetBrewTime", "brew.by_time.target_time");
            registry.add_parameter("preinfusion", "brew.pre_infusion.time");
            registry.add_parameter("preinfusionPause", "brew.pre_infusion.pause");
            registry.add_parameter("backflushOn", BACKFLUSH_ON);
            registry.add_parameter("backflushCycles", "backflush.cycles");
            registry.add_parameter("backflushFillTime", "backflush.fill_time");
            registry.add_parameter("backflushFlushTime", "backflush.flush_time");
            registry.add_parameter(
                "backflushReminderEnabled",
                "maintenance.backflush_reminder.enabled",
            );
            registry.add_parameter(
                "backflushReminderThreshold",
                "maintenance.backflush_reminder.threshold",
            );
        }

        // ---- parameters: behind the scale, :722-734 ----
        if config.hardware.sensors.scale.enabled {
            registry.add_parameter("targetBrewWeight", "brew.by_weight.target_weight");
            registry.add_parameter("scaleCalibration", "hardware.sensors.scale.calibration");
            if config.hardware.sensors.scale.r#type == cc_domain::hardware::ScaleType::Hx711Dual {
                registry.add_parameter("scale2Calibration", "hardware.sensors.scale.calibration2");
            }
            registry.add_parameter("scaleKnownWeight", "hardware.sensors.scale.known_weight");
            registry.add_parameter("scaleTareOn", TARE_ON);
            registry.add_parameter("scaleCalibrationOn", CALIBRATION_ON);
        }

        // ---- sensors: the nine unconditional ones, :738-766 ----
        registry.add_sensor("temperature");
        registry.add_sensor("heaterPower");
        registry.add_sensor("standbyModeTimeRemaining");
        registry.add_sensor("shotsSinceBackflush");
        registry.add_sensor("backflushReminderDue");
        registry.add_sensor("currentKp");
        registry.add_sensor("currentKi");
        registry.add_sensor("currentKd");
        registry.add_sensor("machineState");

        // ---- sensors: behind the brew switch, :770-775 ----
        if config.hardware.switches.brew.enabled {
            registry.add_sensor("currBrewTime");
        }

        // ---- sensors: behind the scale, :778-784 ----
        if config.hardware.sensors.scale.enabled {
            registry.add_sensor("currReadingWeight");
            registry.add_sensor("currBrewWeight");
        }

        // ---- sensors: behind the pressure probe, :787-790 ----
        if config.hardware.sensors.pressure.enabled {
            registry.add_sensor("pressure");
        }

        // ---- binary sensors: behind the water tank, :794-798 ----
        if config.hardware.sensors.watertank.enabled {
            registry.add_binary_sensor("waterTankFull");
        }

        registry
    }
}

/// A borrowed view of a [`Registry`], plus its own item storage.
///
/// A `Plan` borrows three slices, so building one out of a `Vec<ParamTopic>` and
/// two `Vec<(String, bool)>` needs the items to live somewhere for the duration.
/// This owns them for the duration and hands out a [`Plan`] that borrows from
/// itself.
pub struct PlanView<'a> {
    items: Vec<Item<'a>>,
    /// Where each phase **ends** in `items`, as absolute offsets:
    /// `[parameters_end, sensors_end, binary_sensors_end]`.
    ///
    /// Offsets, not lengths. The three groups are stored contiguously and a
    /// [`Plan`] is three contiguous slices of that one `Vec`, so `plan` needs
    /// offsets. Storing lengths here was a device bug: every plan past the first
    /// group was a wrong slice, and for any registry with a binary sensor the
    /// third slice was out of range and **panicked**, taking the MQTT task and
    /// the chip with it the first time anything was published.
    ends: [usize; 3],
}

impl<'a> PlanView<'a> {
    /// Build the view.
    #[must_use]
    pub fn of(registry: &'a Registry) -> Self {
        let mut items: Vec<Item<'a>> = Vec::with_capacity(registry.len());
        for param in &registry.parameters {
            items.push(Item {
                topic: param.topic.as_str(),
                retain: param.retain,
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
        let [p_end, s_end, _b_end] = self.ends;
        Plan {
            parameters: &self.items[..p_end],
            sensors: &self.items[p_end..s_end],
            binary_sensors: &self.items[s_end..],
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
    fn overwrite(&mut self, value: &str) {
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

/// What the callback saw, shared with the [`Client`] that owns it.
///
/// `EspMqttClient::new_cb` runs the closure **on the `esp-mqtt` task**, so this
/// is a cross-task channel and is built from atomics and one short-held mutex.
///
/// # Why `new_cb` and not `new` + [`EspMqttConnection`]
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
    /// The C++'s retry policy and circuit breaker, driven from the events.
    backoff: Mutex<Backoff>,
}

/// The C++'s `retryPolicy_` and `circuitBreaker_` (`MQTTManager.cpp:48-58`).
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
        // anyway: `MQTTManager::assignParameter` applies each message as it
        // arrives, so two writes to the same parameter land in the order they
        // were sent.
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
/// The C++'s `messageCallback` (`MQTTManager.cpp:237-274`) does the whole thing
/// on its own task — parse the topic, `sscanf("%lf")` the payload and call
/// `assignParameter`, which reaches into `SystemContext` and the configuration
/// singleton. Here the topic parse happens here, because it is pure, and the
/// write happens on the control task, because that is the task that owns the
/// configuration and the store (04 §3.2).
fn receive(link: &Link, topic: &str, data: &[u8]) {
    let Some(reading) = link.topics.command_of(topic) else {
        warn!("mqtt: invalid topic/command: {topic}");
        return;
    };
    let mut command = SetCommand {
        reading: Bounded::new(),
        value: Bounded::new(),
    };
    // `MQTTManager.cpp:250` copies the bytes and NUL-terminates them, then
    // `sscanf("%lf")`s the result. A payload that is not UTF-8 cannot be a
    // number, so it is refused here rather than carried as bytes the
    // configuration's own parser would have to reject anyway.
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
        // `MQTTManager.cpp:249` truncates at `%119[^\/]`; refusing is the honest
        // answer, because a truncated name is a *different* parameter.
        warn!("mqtt: command topic '{reading}' is longer than {READING_MAX} B");
        return;
    }
    let Ok(mut slot) = link.inbound.lock() else {
        return;
    };
    if slot.push_back(command).is_err() {
        // Drop-newest, counted. The C++ has no equivalent limit because its
        // handler is synchronous; a burst from a misconfigured client would
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
    /// "constructed" and "connected" are separate events. That is what the C++
    /// does too: `setup` (`:54-92`) does not connect, `checkConnection`
    /// (`:96-193`) does.
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

    /// Subscribe to the command topic, as `MQTTManager.cpp:162` does on connect.
    ///
    /// # When it is called
    ///
    /// From the control task, on the first tick after `Connected` — which is
    /// what the C++ does, since it subscribes immediately after `connect()`
    /// returns. `esp-mqtt` redelivers subscriptions on its own for a session
    /// it restores, so this is called once per process rather than once per
    /// reconnect.
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
    /// the client's own task does the socket write. That is why the C++ needed a
    /// time budget at all — the copy is bounded but the outbox drains slowly on a
    /// bad link.
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
    /// `MQTTManager.cpp:400` publishes `"online"` on the status topic when a
    /// pass starts. **Retained here, unretained there**: the C++'s call is
    /// `publish("status", "online")` against a `bool retain = false` default
    /// (`MQTTManager.h:291`), and it survives only because it repeats every five
    /// seconds. A retained `online` costs one write per pass, is the pattern the
    /// retained `offline` last will on the same topic already implies, and
    /// removes the five-second window in which a broker restart leaves every
    /// entity unavailable. Recorded in `intentional-diffs.md`.
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
    /// The C++'s `checkConnection` without the `esp-mqtt` call, which
    /// `esp-idf-svc` makes on its own task with its own `reconnect_timeout`.
    /// What is left is the bookkeeping the C++ does and `esp-mqtt` does not: the
    /// retry policy, the circuit breaker, and the log line.
    #[must_use]
    pub fn due_for_reconnect(&self) -> bool {
        if self.is_connected() || !self.ever_connected() {
            // `!ever_connected` is the C++'s `serverIP_.length() == 0` guard
            // generalised: never spend a reconnect attempt on a client that has
            // not yet had its first chance.
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
/// The C++'s four members: `mqttLastSent_`, `mqttVarsIt_`, `mqttSensorsIt_`,
/// `mqttBinarySensorsIt_` and `publishPhase_`
/// (`MQTTManager.h:245-251`), plus `previousMillisMQTT_` and the two
/// `MillisecondTimer`s' cursors.
///
/// # Why this is built once and never rebuilt
///
/// The `PlanView` the [`cc_domain::mqtt::Cursor`] walks borrows the registry's
/// topic strings, so a `Feed` that owns both needs a self-reference — or a
/// registry that outlives it. The registry is built from the configuration once,
/// at boot, and **never changes**: no C++ code path adds an MQTT topic after
/// `SystemInitializer` has run, and the four conditional groups are decided by
/// hardware that is fixed for the life of the machine. So the registry is
/// leaked once ([`Box::leak`]) and the plan is built against `'static`, and the
/// alternative — rebuilding a `Vec` of ~60 `Item`s every pass, five times a
/// second — is an allocation the control tick does not need to make.
///
/// # Why the buffers are here
///
/// `topic`, `payload` and the [`BTreeMap`] of last-sent values are all reused.
/// See [`Payload`].
pub struct Feed {
    /// The C++'s `mqttVars_` / `mqttSensors_` / `mqttBinarySensors_`.
    registry: &'static Registry,
    /// The flattened three-phase plan, built once from `registry` and **leaked
    /// alongside it**.
    ///
    /// A `&'static` rather than an owned `PlanView` so that `plan()` yields a
    /// `Plan<'static>`: an owned view would hand back a plan whose lifetime is
    /// tied to `&self`, and the item topics would be unusable as the
    /// `&'static str` keys of [`Self::last_sent`] without re-borrowing the very
    /// field the publish loop is iterating.
    view: &'static PlanView<'static>,
    /// `mqttVarsIt_` / `publishPhase_`, as one cursor.
    cursor: Cursor,
    /// `mqttLastSent_[topic]`, the value-change dedupe.
    ///
    /// Keys are `&'static str` borrowed from the leaked registry, and values are
    /// fixed-size [`Payload`]s, so neither inserting a first value nor replacing
    /// a changed one touches the allocator. A `BTreeMap` rather than a `Vec`
    /// because the key is the topic name and the C++'s is a hash of it
    /// (`MQTTManager.h:245`).
    last_sent: BTreeMap<&'static str, Payload>,
    /// `previousMillisMQTT_`: when the pass in progress started.
    ///
    /// Zero at construction, matching `previousMillisMQTT_(0)`
    /// (`MQTTManager.h:255`), so the first pass is due as soon as the interval
    /// has elapsed since boot.
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
    /// `topics` is the same layout [`Client`] built, and `config` is the
    /// control task's live copy — the conditional groups are read from it here,
    /// once, exactly as `SystemInitializer` read `Config::getInstance()` once.
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

    /// One budgeted step of `MQTTManager::checkConnection` and
    /// `writeSysParamsToMQTT`.
    ///
    /// Called from the control task's 10 ms tick, where `LoopManager.cpp:490`
    /// and `:505` call the C++'s.
    ///
    /// # The shape, in five lines
    ///
    /// 1. Nothing at all without a session — `!mqttClient_.connected()`
    ///    (`MQTTManager.cpp:393`) short-circuits before anything is sent.
    /// 2. Then Home Assistant discovery, if its own 300 s timer is due, one
    ///    budgeted slice of the document list, rebuilt from the live
    ///    configuration when the cycle starts.
    /// 3. Then, if `now - last_pass_ms >= interval_ms`, the pass starts:
    ///    `previousMillisMQTT_` is stamped and the retained `online` is published
    ///    (`MQTTManager.cpp:398-401`).
    /// 4. The cursor walks parameters → sensors → binary sensors, and **stops**
    ///    the moment [`TIME_BUDGET_MS`] has elapsed, resuming at the next topic on
    ///    the next call (`MQTTManager.cpp:501, 517, 546`).
    /// 5. A topic whose value has not changed is skipped without a publish, and
    ///    the outbox is not written at all in that case
    ///    (`mqttLastSent_`, `MQTTManager.cpp:512`).
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
    ///   [`Self::last_sent`] — once per topic, ever, so at most
    ///   `registry().len()` times in the life of the process;
    /// * `cc_config::discovery::all`, once per [`DISCOVERY_INTERVAL_MS`] at the
    ///   start of a cycle;
    /// * `Box::leak` in [`Self::new`], once, at boot.
    ///
    /// # `values`
    ///
    /// Writes the value for `reading` into `payload` and returns `true`, or
    /// returns `false` for a topic this build has no producer for. The C++'s
    /// `continueOnError` arm logs `"Parameter %s not found for MQTT topic %s"`
    /// and moves on (`MQTTManager.cpp:440-443`); this counts it in
    /// [`Service::unresolved`] instead of logging per topic, because a
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
            // `MQTTManager.cpp:393` — the gate is before everything, including
            // the availability publish. A machine with no session must not queue
            // a pass it cannot finish.
            return out;
        }

        let started = now;

        // ---- 1. Home Assistant discovery, on its own 300 s timer ----------
        //
        // `LoopManager.cpp:382-384` fires `hassioDiscoveryTimer_` every
        // `HASSIO_DISCOVERY_INTERVAL_MS` and the callback builds and publishes
        // the whole set. Here the set is built at the start of the cycle and
        // published a few per call under the same budget, so 29 entities do not
        // become a 300 ms stall in the control task.
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
            // `MQTTManager.cpp:398-401`. `mqttVarsIt_ == mqttVars_.begin()` is
            // this branch: it is true exactly once per pass, so the retained
            // availability is republished on the pass's cadence and not on every
            // resumed slice.
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
            // `MQTTManager.cpp:501` — after *every* topic, published or skipped,
            // because a skipped topic costs a lookup too.
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
        // `mqttLastSent_[mqttTopic] != value` (`MQTTManager.cpp:512`, `:520`,
        // `:538`) — the dedupe is applied to all three phases, not just the
        // parameters.
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
            // `MQTTManager.cpp:517-521`: a failed publish is **not** recorded in
            // `mqttLastSent_`, so the next pass retries it. Only success advances
            // the dedupe.
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
            // `publishLargeMessage` (`MQTTManager.cpp:200-224`) is `beginPublish
            // (topic, length, /*retain=*/true)` for a payload over 128 bytes and
            // the default `retain = false` below it. The retained arm is the one
            // every discovery payload here takes, and it is the one that matters:
            // an unretained discovery document is forgotten by the next broker
            // restart and Home Assistant deletes the entity.
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

/// Whether a broker is configured at all.
///
/// `MQTTManager::setup` (`MQTTManager.cpp:79-83`) disables MQTT outright when the
/// broker name is empty, rather than building a client and failing to
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

    fn plan_of(config: &Config) -> Plan<'static> {
        // The tests need a `Plan` and a `Registry` with the same lifetime, which
        // is the one shape this module cannot build by value. The leak is what
        // `Feed` itself does, and it is bounded by the number of cases.
        let registry: &'static Registry = Box::leak(Box::new(Registry::from_config(
            &Topics::new("p/", "h"),
            config,
        )));
        let view: &'static PlanView<'static> = Box::leak(Box::new(PlanView::of(registry)));
        view.plan()
    }

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

    /// The C++'s topic matcher, and every way a topic can miss it.
    ///
    /// `MQTTManager.cpp:252-262` builds `"%s%s/%%119[^\/]/%%63[^\\/]"` and
    /// requires the second field to be exactly `set`; a deeper path, a bare
    /// `status`, a trailing `/extra` and a foreign prefix all land in the
    /// `"Invalid MQTT topic/command"` arm, and so must they here.
    ///
    /// **A wildcard is the interesting one.** `%119[^\/]` accepts `+`, so
    /// `<base>+/set` parses to the reading `"+"` in the C++ exactly as it does
    /// here -- and is then dropped by `mqttVars_.find("+")`, the
    /// `"MQTT topic %s not found in mapping"` arm of `MQTTManager.cpp:289-292`.
    /// Reproduced rather than "fixed", because the refusal is the *registry's*
    /// job and `an_inbound_reading_resolves_only_if_it_is_registered` is where
    /// it is checked; a matcher that also refused wildcards would be a second
    /// place with the same rule, and the two would drift.
    #[cfg_attr(test, test)]
    pub fn an_inbound_topic_is_parsed_exactly_as_the_csqs_matcher_does() {
        let topics = Topics::new("custom/kitchen/", "clevercoffee");
        assert_eq!(
            topics.command_of("custom/kitchen/clevercoffee/brewSetpoint/set"),
            Some("brewSetpoint")
        );
        assert_eq!(
            topics.command_of("custom/kitchen/clevercoffee/+/set"),
            Some("+"),
            r"%119[^\/] accepts a wildcard; the registry lookup is what refuses it"
        );
        assert_eq!(
            topics.command_of("custom/kitchen/clevercoffee/a/b/set"),
            None
        );
        assert_eq!(
            topics.command_of("custom/kitchen/clevercoffee/status"),
            None
        );
        assert_eq!(
            topics.command_of("custom/kitchen/clevercoffee/pidON/set/extra"),
            None
        );
        assert_eq!(
            topics.command_of("other/kitchen/clevercoffee/pidON/set"),
            None
        );
        // And the half that matters: the wildcard is not a parameter.
        assert_eq!(
            Registry::from_config(&topics, &Config::default()).resolve("+"),
            None
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_buffer_is_the_csqs_1024() {
        // MQTTManager.cpp:96, raised specifically for discovery payloads.
        // cc_config::discovery asserts every payload is under it.
        assert_eq!(BUFFER_BYTES, 1024);
    }

    #[cfg_attr(test, test)]
    pub fn the_budget_is_a_fraction_of_the_control_period() {
        // The C++'s 10 ms was 2.5 % of a 400 ms loop. This firmware's loop is
        // 10 ms (`cc-firmware/src/main.rs:189`), so the budget is re-derived
        // against that: a quarter of the period would still be most of a tick's
        // worth of publishing, and the whole of it would bound nothing.
        assert_eq!(TIME_BUDGET_MS, 2);
        const { assert!(TIME_BUDGET_MS * 5 <= 10) };
    }

    #[cfg_attr(test, test)]
    pub fn the_intervals_are_the_csqs() {
        assert_eq!(INTERVAL_MS, 5_000);
        assert_eq!(INTERVAL_BREW_MS, 500);
        assert_eq!(INTERVAL_STANDBY_MS, 10_000);
        assert_eq!(DISCOVERY_INTERVAL_MS, 300_000);
    }

    /// The interval is a function of the machine state, and the state is the
    /// C++'s predicate.
    ///
    /// `MQTTManager.cpp:384-387`. The brewing arm used to be unreachable,
    /// because a `BREWING` flag made every publish a no-op during a brew — and
    /// that flag was never set by anything, so the guard was both wrong and inert.
    /// `BrewFinished` is excluded from the brew arm by the C++ itself
    /// (`BrewHandler.h:98-103`), which is the one case a naive
    /// `is_brew_state()` would get wrong.
    #[cfg_attr(test, test)]
    pub fn the_interval_follows_the_machine_state() {
        use cc_domain::state::MachineState as S;
        assert_eq!(interval_for(S::BrewRunning), INTERVAL_BREW_MS);
        assert_eq!(interval_for(S::BrewPreinfusion), INTERVAL_BREW_MS);
        assert_eq!(
            interval_for(S::BrewFinished),
            INTERVAL_MS,
            "BREW_FINISHED is excluded from the brew arm: MQTTManager.cpp:385"
        );
        assert_eq!(interval_for(S::Standby), INTERVAL_STANDBY_MS);
        assert_eq!(interval_for(S::PidNormal), INTERVAL_MS);
        assert_eq!(interval_for(S::SteamRunning), INTERVAL_MS);
    }

    /// The registry is the C++'s, topic for topic.
    ///
    /// `SystemInitializer.cpp:687-800`. The counts are the whole assertion: the
    /// three parameters, three sensors and two binary sensors this file used to
    /// register were 32 parameters and 13 sensors' worth of nothing, so nothing
    /// Home Assistant displays was ever published.
    #[cfg_attr(test, test)]
    pub fn the_registry_is_the_csqs_registration() {
        use cc_domain::hardware::ScaleType;
        let topics = Topics::new("p/", "h");

        // A configuration with every conditional group **off**. Written out
        // rather than taken from `Config::default()` because
        // `hardware.switches.brew.enabled` defaults to `true` here — a declared
        // divergence from the C++'s `false` (`Config.h:985`), recorded in
        // `intentional-diffs.md` — and a count that silently depended on it
        // would be a count of the wrong thing.
        let mut config = Config::default();
        config.hardware.switches.brew.enabled = false;
        let bare = Registry::from_config(&topics, &config);
        // The 12 unconditional parameters, SystemInitializer.cpp:690-701.
        assert_eq!(bare.parameters().len(), 12);
        // The 9 unconditional sensors, :738-766.
        assert_eq!(bare.sensors().len(), 9);
        // No binary sensor without the tank, :794-798.
        assert!(bare.binary_sensors().is_empty());

        // Everything on, with a single-cell scale: the fifth conditional scale
        // parameter, `scale2Calibration`, is behind `HX711_DUAL`
        // (SystemInitializer.cpp:727-729).
        config.hardware.switches.brew.enabled = true;
        config.hardware.sensors.scale.enabled = true;
        config.hardware.sensors.scale.r#type = ScaleType::Hx711Single;
        config.hardware.sensors.watertank.enabled = true;
        config.hardware.sensors.pressure.enabled = true;
        let full = Registry::from_config(&topics, &config);
        // 12 + 14 (:705-719) + 5 (:722-734).
        assert_eq!(full.parameters().len(), 31);
        // 9 + 1 (:770-775) + 2 (:778-784) + 1 (:787-790).
        assert_eq!(full.sensors().len(), 13);
        assert_eq!(full.binary_sensors().len(), 1);

        // The dual cell adds exactly one, and only that one.
        config.hardware.sensors.scale.r#type = ScaleType::Hx711Dual;
        let dual = Registry::from_config(&topics, &config);
        assert_eq!(dual.parameters().len(), 32);
        assert_eq!(
            dual.resolve("scale2Calibration"),
            Some("hardware.sensors.scale.calibration2")
        );
    }

    /// Every topic the C++ registers resolves, and nothing else does.
    ///
    /// `MQTTManager.cpp:288-292` refuses an unregistered inbound topic with
    /// "MQTT topic %s not found in mapping", and that refusal is what keeps a
    /// typo on a command topic from becoming a parameter write.
    #[cfg_attr(test, test)]
    pub fn an_inbound_reading_resolves_only_if_it_is_registered() {
        let topics = Topics::new("p/", "h");
        let mut config = Config::default();
        config.hardware.switches.brew.enabled = true;
        let registry = Registry::from_config(&topics, &config);
        assert_eq!(registry.resolve("brewSetpoint"), Some("brew.setpoint"));
        assert_eq!(registry.resolve("pidON"), Some("pid.enabled"));
        // The four the C++ compares against literals rather than looking up
        // (MQTTManager.cpp:296-322).
        assert_eq!(registry.resolve("steamON"), Some(STEAM_MODE));
        assert_eq!(registry.resolve("backflushOn"), Some(BACKFLUSH_ON));
        // A sensor is published but not settable: `mqttVars_` has no entry.
        assert_eq!(registry.resolve("temperature"), None);
        assert_eq!(registry.resolve("usePonM"), None);
    }

    /// The four special targets are not configuration keys.
    ///
    /// `MQTTManager.cpp:296-322` branches on them **before** `findConfigParameter`
    /// is called, because `Config` does not carry a steam mode, a backflush
    /// mode, a tare mode or a calibration mode. A test that asserts they are not
    /// schema keys is what stops somebody "tidying" them into one.
    #[cfg_attr(test, test)]
    pub fn the_four_specials_are_not_configuration_keys() {
        for special in [STEAM_MODE, BACKFLUSH_ON, TARE_ON, CALIBRATION_ON] {
            assert!(
                cc_config::schema::find(special).is_none(),
                "{special} must stay outside the schema: it is machine state, \
                 not a parameter (MQTTManager.cpp:296-322)"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_registry_puts_each_kind_of_topic_in_the_cpp_phase() {
        // Phase membership is what decides the retain flag, and a topic in the
        // wrong phase is published with the wrong flag — retained where it must
        // not be, or lost on a broker restart where it must not be.
        let topics = Topics::new("p/", "h");
        let mut config = Config::default();
        config.hardware.switches.brew.enabled = false;
        let registry = Registry::from_config(&topics, &config);
        let view = PlanView::of(&registry);
        let plan = view.plan();
        // Polled sensors are not retained.
        assert!(plan.sensors.iter().all(|i| !i.retain));
        // Parameters and binary sensors are.
        assert!(plan.parameters.iter().all(|i| i.retain));
        assert!(plan.binary_sensors.iter().all(|i| i.retain));
        // The phases are the C++'s, in its order.
        assert_eq!(plan.phase_items(Phase::Parameters).len(), 12);
        assert_eq!(plan.phase_items(Phase::Sensors).len(), 9);
        assert!(plan.phase_items(Phase::BinarySensors).is_empty());
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
        let mut config = Config::default();
        config.hardware.sensors.watertank.enabled = true;
        let registry = Registry::from_config(&topics, &config);
        let view = PlanView::of(&registry);
        let plan = view.plan();

        let total = plan.parameters.len() + plan.sensors.len() + plan.binary_sensors.len();
        assert_eq!(total, view.len(), "the phases must partition the view");
        assert_eq!(plan.parameters.len(), registry.parameters.len());
        assert_eq!(plan.sensors.len(), registry.sensors.len());
        assert_eq!(plan.binary_sensors.len(), registry.binary_sensors.len());
        assert_eq!(
            plan_topics(plan.sensors),
            registry_topics(registry.sensors())
        );
        assert_eq!(
            plan_topics(plan.binary_sensors),
            registry_topics(registry.binary_sensors())
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_pressure_sensor_appears_only_when_it_is_fitted() {
        // MQTTManager.cpp:787-790.
        let topics = Topics::new("p/", "h");
        let mut config = Config::default();
        let without = Registry::from_config(&topics, &config);
        assert!(!without.sensors().iter().any(|(t, _)| t == "pressure"));
        config.hardware.sensors.pressure.enabled = true;
        let with = Registry::from_config(&topics, &config);
        assert!(with.sensors().iter().any(|(t, _)| t == "pressure"));
    }

    /// The weight is published on the topics `cc_config::discovery`
    /// advertises — and only those.
    ///
    /// `MQTTManager.cpp:903-904` registers `currReadingWeight` and
    /// `currBrewWeight`; `discovery.rs` advertises sensors of those same two
    /// names. The registry used to publish a topic called `weight`, which
    /// matches neither, so the Home Assistant weight entity was advertised and
    /// never updated. Both facts live in **different crates** and nothing made
    /// them agree.
    #[cfg_attr(test, test)]
    pub fn the_weight_topics_are_exactly_the_ones_discovery_advertises() {
        let topics = Topics::new("p/", "h");
        let mut config = Config::default();

        config.hardware.sensors.scale.enabled = false;
        let registry_without = Registry::from_config(&topics, &config);
        let discovery_without = cc_config::discovery::all(&config);
        config.hardware.sensors.scale.enabled = true;
        let registry_with = Registry::from_config(&topics, &config);
        let discovery_with = cc_config::discovery::all(&config);

        let advertises = |payloads: &[cc_config::discovery::Discovery], name: &str| {
            payloads.iter().any(|d| d.payload.contains(name))
        };
        let publishes =
            |registry: &Registry, name: &str| registry.sensors().iter().any(|(t, _)| t == name);

        for name in ["currReadingWeight", "currBrewWeight"] {
            assert_eq!(
                advertises(&discovery_without, name),
                publishes(&registry_without, name),
                "no scale fitted: {name} must be advertised and published together"
            );
            assert!(
                advertises(&discovery_with, name),
                "a fitted scale advertises {name}"
            );
            assert!(
                publishes(&registry_with, name),
                "a fitted scale must publish {name}"
            );
        }
        assert!(
            !publishes(&registry_with, "weight"),
            "`weight` was never a topic: Home Assistant listens on \
             currReadingWeight/currBrewWeight (MQTTManager.cpp:903-904)"
        );
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
        assert!(!view
            .items
            .iter()
            .any(|item| item.topic.contains("brokerpw")));
        assert!(!topics.base.contains("brokerpw"));
    }

    #[cfg_attr(test, test)]
    pub fn the_topics_string_names_the_base_so_a_bring_up_log_is_useful() {
        // It must, though, never the credentials -- `describe` is printed at
        // boot and captured in support tickets.
        let topics = Topics::new("custom/kitchen/", "clevercoffee");
        assert!(topics.base.starts_with("custom/kitchen/"));
    }

    /// Every value the firmware publishes fits the reusable buffer.
    ///
    /// A `Payload` that overflows is emptied, so a value too wide would publish
    /// as nothing and the dedupe would republish it for ever. `PAYLOAD_MAX` is
    /// 32 and the widest thing any registered topic can hold is a `%0.2f` of a
    /// schema-bounded number; this is what keeps that true.
    ///
    /// The second half is the half that actually matters: **no registered MQTT
    /// parameter is a `Text` parameter**, so nothing longer than a number can
    /// ever reach this buffer. A `mqtt.topic` or a `wifi.ssid` registered as a
    /// parameter would break it silently, and `LiveValue::Text` would be the
    /// arm that published it.
    #[cfg_attr(test, test)]
    pub fn every_registered_value_fits_the_payload_buffer() {
        use cc_config::json::LiveValue;

        let mut config = Config::default();
        config.hardware.switches.brew.enabled = true;
        config.hardware.sensors.scale.enabled = true;
        config.hardware.sensors.watertank.enabled = true;
        config.hardware.sensors.pressure.enabled = true;
        let registry = Registry::from_config(&Topics::new("p/", "h"), &config);

        for param in registry.parameters() {
            if matches!(
                param.key,
                STEAM_MODE | BACKFLUSH_ON | TARE_ON | CALIBRATION_ON
            ) {
                continue;
            }
            let live = cc_config::live_value(&config, param.key)
                .unwrap_or_else(|| panic!("{} is registered and not in Config", param.key));
            assert!(
                !matches!(live, LiveValue::Text(_)),
                "{} is a text parameter; the MQTT payload buffer is {PAYLOAD_MAX} B \
                 and a long string would publish as nothing",
                param.key
            );
            let mut payload = Payload::new();
            match live {
                LiveValue::Bool(v) => payload.set_bool(v),
                LiveValue::Int(v) => payload.set_int(i64::from(v)),
                LiveValue::Float(v) => payload.set_float(v),
                LiveValue::Enum(v) => payload.set_int(i64::from(v)),
                LiveValue::Text(_) => unreachable!("checked above"),
            }
            assert!(!payload.is_empty(), "{} produced no payload", param.key);
        }

        // And the widest a schema-bounded double gets.
        let mut payload = Payload::new();
        payload.set_float(999_999.5);
        assert!(!payload.is_empty());
    }

    /// The plan this module's tests build must actually partition.
    ///
    /// `plan_of` is the lifetime-shaped shortcut the other cases use; without
    /// this one, a change to [`PlanView`] that broke the slices would make every
    /// other assertion in the module vacuous rather than failing.
    #[cfg_attr(test, test)]
    pub fn the_shared_plan_helper_still_sees_the_whole_registry() {
        let mut config = Config::default();
        config.hardware.switches.brew.enabled = false;
        assert_eq!(plan_of(&config).len(), 12 + 9);
    }
}
