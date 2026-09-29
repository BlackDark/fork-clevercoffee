//! Home Assistant MQTT discovery payloads.
//!
//! Owner: **R3-13** (task D).
//!
//! # Why these live in `cc-config`
//!
//! A discovery payload is a JSON document whose *topics* are the
//! configuration's own topics and whose *min/max* are the configuration's own
//! parameter ranges. It is the one place where `cc-domain`'s vocabulary, the
//! configuration schema and the wire format meet, and the only portable crate
//! that has all three is this one: `cc-domain` has no `serde` by default and
//! `cc-machine` has no JSON at all.
//!
//! Keeping it here also keeps it host-testable. A discovery payload is the one
//! MQTT artifact whose *content* is the contract — Home Assistant silently
//! ignores a malformed one and the machine looks like it has no entities — and
//! "silently ignored" is not something a device test can catch.
//!
//! # What is ported, and what is not
//!
//! `MQTTManager::sendHASSIODiscoveryMsg` (`MQTTManager.cpp:828-926`) publishes
//! 16 entities unconditionally and 13 more behind configuration flags. All four
//! payload *shapes* are ported — sensor, binary sensor, number, switch, plus the
//! button shape (`generateButtonDevice`, `:684-724`) — and the entity list is
//! reproduced with the flags evaluated against the configuration.
//!
//! What is **not** reproduced is the C++'s per-entity `min`/`max`/`step`
//! numbers. Those are read from `defaults.h` (`BREW_SETPOINT_MIN`,
//! `PID_KP_REGULAR_MIN`, …) through ~20 macros, and `cc-config`'s schema carries
//! the same bounds as a `ParamSpec`. So they are read from
//! [`crate::schema::SCHEMA`] by dotted key rather than hard-coded, which is both
//! shorter and impossible to get out of step with the parameter it describes.

use alloc::format;
use alloc::string::{String, ToString};

use serde_json::{json, Value};

use crate::config::Config;
use crate::schema::SCHEMA;

/// The Home Assistant component a discovery payload declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    /// `sensor/<name>/config`.
    Sensor,
    /// `binary_sensor/<name>/config`.
    BinarySensor,
    /// `number/<name>/config`.
    Number,
    /// `switch/<name>/config`.
    Switch,
    /// `button/<name>/config`.
    Button,
}

impl Component {
    /// The path segment Home Assistant's MQTT discovery uses for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sensor => "sensor",
            Self::BinarySensor => "binary_sensor",
            Self::Number => "number",
            Self::Switch => "switch",
            Self::Button => "button",
        }
    }
}

/// The device block every payload carries.
///
/// `populateDeviceMap` / `attachDeviceAndAvailability`
/// (`MQTTManager.cpp:589-601`) and the hand-rolled copies of it in each
/// `generate*Device`:
///
/// ```json
/// "device": { "identifiers": "<hostname>",
///              "manufacturer": "CleverCoffee",
///              "name": "<hostname>" }
/// ```
///
/// `identifiers` is a bare hostname rather than a list. The C++ does the same
/// (`deviceMapDoc["identifiers"] = hostname_;`, `:662`) and Home Assistant
/// accepts a string there. Reproduced because changing it would make the machine
/// a *different device* to any Home Assistant that had already adopted it — the
/// unique id and the device identifier are the join key, and an install that
/// has to delete and re-add the integration is a support call.
#[must_use]
fn device_block(hostname: &str) -> Value {
    json!({
        "identifiers": hostname,
        "manufacturer": "CleverCoffee",
        "name": hostname,
    })
}

/// The availability fields, identical in every payload.
///
/// `MQTTManager.cpp:597-600`.
fn availability(hostname: &str, prefix: &str, status_topic: &str) -> Value {
    json!({
        "payload_available": "online",
        "payload_not_available": "offline",
        "availability_topic": format!("{prefix}{hostname}/{status_topic}"),
    })
}

/// The unique id prefix, `clevercoffee-<hostname>`.
///
/// `MQTTManager.cpp:658` and every `generate*Device` after it.
#[must_use]
pub fn unique_id_prefix(hostname: &str) -> String {
    format!("clevercoffee-{hostname}")
}

/// One discovery message: where it goes and what it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// The retained topic, `<prefix>/<component>/<unique_id>/<name>/config`.
    pub topic: String,
    /// The JSON payload.
    pub payload: String,
}

/// Everything needed to build a discovery payload.
pub struct Builder<'a> {
    /// `mqtt.topic`.
    pub prefix: &'a str,
    /// `system.hostname`.
    pub hostname: &'a str,
    /// `mqtt.hassio.prefix`, `homeassistant` by default.
    pub discovery_prefix: &'a str,
}

impl Builder<'_> {
    /// The state topic for one reading: `<prefix><hostname>/<name>`.
    #[must_use]
    pub fn state_topic(&self, name: &str) -> String {
        format!("{}{hostname}/{name}", self.prefix, hostname = self.hostname)
    }

    /// The command topic for one reading: `<prefix><hostname>/<name>/set`.
    #[must_use]
    pub fn command_topic(&self, name: &str) -> String {
        format!(
            "{}{hostname}/{name}/set",
            self.prefix,
            hostname = self.hostname
        )
    }

    /// The discovery topic for one entity.
    #[must_use]
    pub fn discovery_topic(&self, component: Component, name: &str) -> String {
        format!(
            "{}/{}/{}/{name}/config",
            self.discovery_prefix,
            component.as_str(),
            unique_id_prefix(self.hostname)
        )
    }

    /// Assemble one message.
    fn message(&self, component: Component, name: &str, body: &Value) -> Discovery {
        Discovery {
            topic: self.discovery_topic(component, name),
            payload: body.to_string(),
        }
    }

    /// A `sensor` entity. `MQTTManager::generateSensorDevice`, `:725-753`.
    ///
    /// `unit` and `device_class` are `Option` because the C++ passes empty
    /// strings for some entities and *omits* `device_class` only for binary
    /// sensors (`:770-772`); an empty `device_class` in a sensor payload makes
    /// Home Assistant show the entity without an icon, which is the intent for
    /// `machineState`, so an empty string is preserved as "no class".
    #[must_use]
    pub fn sensor(&self, name: &str, display: &str, unit: &str, device_class: &str) -> Discovery {
        let mut body = json!({
            "name": display,
            "state_topic": self.state_topic(name),
            "unique_id": format!("{}-{name}", unique_id_prefix(self.hostname)),
        });
        if !unit.is_empty() {
            body["unit_of_measurement"] = Value::from(unit);
        }
        if !device_class.is_empty() {
            body["device_class"] = Value::from(device_class);
        }
        let availability = availability(self.hostname, self.prefix, "status");
        merge(&mut body, &availability);
        body["device"] = device_block(self.hostname);
        self.message(Component::Sensor, name, &body)
    }

    /// A `binary_sensor` entity. `MQTTManager::generateBinarySensorDevice`,
    /// `:756-797`.
    ///
    /// The payload is `ON`/`OFF` because the C++ publishes exactly that
    /// (`MQTTManager.cpp:534`), so `payload_on`/`payload_off` default to it here
    /// rather than being a parameter.
    #[must_use]
    pub fn binary_sensor(&self, name: &str, display: &str, device_class: &str) -> Discovery {
        let mut body = json!({
            "name": display,
            "state_topic": self.state_topic(name),
            "unique_id": format!("{}-{name}", unique_id_prefix(self.hostname)),
            "payload_on": "ON",
            "payload_off": "OFF",
        });
        if !device_class.is_empty() {
            body["device_class"] = Value::from(device_class);
        }
        let availability = availability(self.hostname, self.prefix, "status");
        merge(&mut body, &availability);
        body["device"] = device_block(self.hostname);
        self.message(Component::BinarySensor, name, &body)
    }

    /// A `number` entity. `MQTTManager::generateNumberDevice`, `:762-808`.
    ///
    /// `min`, `max` and `step` come from the configuration schema by dotted key,
    /// so they cannot drift from the parameter they describe. `ui_mode` is
    /// Home Assistant's `mode` field (`slider` or `box`); the C++ passes it
    /// through and this port defaults to `slider`, which is what every one of
    /// the C++'s own call sites ends up with.
    #[must_use]
    pub fn number(
        &self,
        name: &str,
        display: &str,
        param: &str,
        step: f64,
        unit: &str,
        ui_mode: &str,
    ) -> Discovery {
        let (min, max) = bounds(param);
        let mut body = json!({
            "name": display,
            "command_topic": self.command_topic(name),
            "state_topic": self.state_topic(name),
            "unique_id": format!("{}-{name}", unique_id_prefix(self.hostname)),
            "min": min,
            "max": max,
            "step": step,
            "mode": ui_mode,
        });
        if !unit.is_empty() {
            body["unit_of_measurement"] = Value::from(unit);
        }
        let availability = availability(self.hostname, self.prefix, "status");
        merge(&mut body, &availability);
        body["device"] = device_block(self.hostname);
        self.message(Component::Number, name, &body)
    }

    /// A `switch` entity. `MQTTManager::generateSwitchDevice`, `:600-656`.
    #[must_use]
    pub fn switch(&self, name: &str, display: &str) -> Discovery {
        let mut body = json!({
            "name": display,
            "command_topic": self.command_topic(name),
            "state_topic": self.state_topic(name),
            "unique_id": format!("{}-{name}", unique_id_prefix(self.hostname)),
            "payload_on": "1",
            "payload_off": "0",
        });
        let availability = availability(self.hostname, self.prefix, "status");
        merge(&mut body, &availability);
        body["device"] = device_block(self.hostname);
        self.message(Component::Switch, name, &body)
    }

    /// A `button` entity. `MQTTManager::generateButtonDevice`, `:684-724`.
    #[must_use]
    pub fn button(&self, name: &str, display: &str, payload_press: &str) -> Discovery {
        let mut body = json!({
            "name": display,
            "command_topic": self.command_topic(name),
            "state_topic": self.state_topic(name),
            "unique_id": format!("{}-{name}", unique_id_prefix(self.hostname)),
            "payload_press": payload_press,
        });
        let availability = availability(self.hostname, self.prefix, "status");
        merge(&mut body, &availability);
        body["device"] = device_block(self.hostname);
        self.message(Component::Button, name, &body)
    }
}

/// Copy the availability fields into `body`.
///
/// `attachDeviceAndAvailability` assigns them one at a time
/// (`MQTTManager.cpp:597-600`); `Object::insert` on an `ArduinoJson` object is a
/// merge, so the result is the same.
fn merge(body: &mut Value, availability: &Value) {
    if let (Some(target), Some(source)) = (body.as_object_mut(), availability.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// The `(min, max)` a `number` entity should publish, from the schema.
///
/// Read from [`SCHEMA`] rather than hard-coded, so a range change in
/// `cc_config`'s schema reaches Home Assistant without a second edit here. A key
/// that is not in the schema yields `0.0 / 100.0` and a number entity with a
/// useless range — which is a visible bug rather than a silent one, and is what
/// a missing schema entry should look like.
///
/// **The `step` is a separate argument and deliberately so.** The C++ passes a
/// step per call site (`0.1` for every temperature and PID gain, `1` for
/// `backflush.cycles`) and `cc_config`'s schema has no step column. Deriving it
/// from the range's width is wrong — `brew.setpoint` is 80..=100, twenty wide,
/// and the C++ still steps it by 0.1 — so it is passed explicitly and the call
/// sites are the single place that has to match the C++.
#[must_use]
pub fn bounds(param: &str) -> (Value, Value) {
    let Some(spec) = SCHEMA.iter().find(|spec| spec.key == param) else {
        return (Value::from(0.0_f64), Value::from(100.0_f64));
    };
    // `Value::from(0.0)` would infer an *integer* literal and produce a JSON
    // integer, whose `as_f64()` is `None` -- so the type is written out.
    let min = spec.min.map_or_else(|| Value::from(0.0_f64), Value::from);
    let max = spec.max.map_or_else(|| Value::from(100.0_f64), Value::from);
    (min, max)
}

/// The step the C++ uses for a `0.1`-granular entity. `MQTTManager.cpp:855-871`.
pub const STEP_TENTH: f64 = 0.1;

/// The step the C++ uses for an integer entity. `MQTTManager.cpp:891`.
pub const STEP_ONE: f64 = 1.0;

/// The whole discovery set for `config`, in the C++'s publication order.
///
/// `MQTTManager::sendHASSIODiscoveryMsg` (`MQTTManager.cpp:848-926`).
///
/// Built with `alloc`, so it is a `Vec` of 16–29 `Discovery` values of a few
/// hundred bytes each. The C++ builds them one at a time and publishes each
/// before building the next; here they are all built and then published under
/// the same 10 ms budget as the telemetry, so a large set is truncated the same
/// way and the next 300 s pass picks up where this one stopped. The retention
/// flag is on every one of them, without which Home Assistant would re-adopt
/// the entities on every broker restart.
// The C++ published these one at a time from inside a loop; this is the same
// set as one table, which is the whole point of building it here rather than on
// the device. Splitting it would hide the correspondence.
#[allow(
    clippy::too_many_lines,
    reason = "this IS the discovery table: 16 unconditional entities plus the \
              conditionals, in the C++'s publication order. Splitting it would \
              hide the one property worth checking by eye, which is that this \
              list is the C++'s list."
)]
#[must_use]
pub fn all(config: &Config) -> alloc::vec::Vec<Discovery> {
    let b = Builder {
        prefix: &config.mqtt.topic,
        hostname: &config.system.hostname,
        discovery_prefix: &config.mqtt.hassio.prefix,
    };

    // The 16 unconditional entities of MQTTManager.cpp:848-871.
    let mut out: alloc::vec::Vec<Discovery> = alloc::vec![
        b.sensor("machineState", "Machine State", "", "enum"),
        b.sensor("temperature", "Boiler Temperature", "°C", "temperature"),
        b.sensor("heaterPower", "Heater Power", "%", "power_factor"),
        b.sensor("shotsSinceBackflush", "Shots Since Backflush", "shots", ""),
        b.sensor("backflushReminderDue", "Backflush Reminder Due", "", ""),
        b.number(
            "brewSetpoint",
            "Brew setpoint",
            "brew.setpoint",
            STEP_TENTH,
            "°C",
            "slider"
        ),
        b.number(
            "steamSetpoint",
            "Steam setpoint",
            "steam.setpoint",
            STEP_TENTH,
            "°C",
            "slider"
        ),
        b.number(
            "brewTempOffset",
            "Brew Temp. Offset",
            "brew.temp_offset",
            STEP_TENTH,
            "°C",
            "slider"
        ),
        b.number(
            "steamKp",
            "Steam Kp",
            "pid.steam.kp",
            STEP_TENTH,
            "",
            "slider"
        ),
        b.number("aggKp", "aggKp", "pid.regular.kp", STEP_TENTH, "", "slider"),
        b.number("aggTn", "aggTn", "pid.regular.tn", STEP_TENTH, "", "slider"),
        b.number("aggTv", "aggTv", "pid.regular.tv", STEP_TENTH, "", "slider"),
        b.number(
            "aggIMax",
            "aggIMax",
            "pid.regular.i_max",
            STEP_TENTH,
            "",
            "slider"
        ),
        b.switch("pidON", "Use PID"),
        b.switch("steamON", "Steam"),
        b.switch("usePonM", "Use PonM"),
    ];

    // Conditional on the brew switch — MQTTManager.cpp:875-899.
    if config.hardware.switches.brew.enabled {
        out.push(b.sensor("currBrewTime", "Current Brew Time ", "s", "duration"));
        out.push(b.number(
            "brewPidDelay",
            "Brew Pid Delay",
            "brew.pid_delay",
            STEP_TENTH,
            "s",
            "slider",
        ));
        out.push(b.number(
            "targetBrewTime",
            "Target Brew time",
            "brew.by_time.target_time",
            STEP_TENTH,
            "s",
            "slider",
        ));
        out.push(b.number(
            "preinfusion",
            "Preinfusion filling time",
            "brew.pre_infusion.time",
            STEP_TENTH,
            "s",
            "slider",
        ));
        out.push(b.number(
            "preinfusionPause",
            "Preinfusion pause time",
            "brew.pre_infusion.pause",
            STEP_TENTH,
            "s",
            "slider",
        ));
        out.push(b.number(
            "backflushCycles",
            "Backflush Cycles",
            "backflush.cycles",
            STEP_ONE,
            "",
            "slider",
        ));
        out.push(b.number(
            "backflushFillTime",
            "Backflush filling time",
            "backflush.fill_time",
            STEP_TENTH,
            "s",
            "slider",
        ));
        out.push(b.number(
            "backflushFlushTime",
            "Backflush flushing time",
            "backflush.flush_time",
            STEP_TENTH,
            "s",
            "slider",
        ));
        out.push(b.switch("backflushOn", "Backflush"));
    }

    // Conditional on the scale — MQTTManager.cpp:902-908.
    if config.hardware.sensors.scale.enabled {
        out.push(b.sensor("currReadingWeight", "Weight", "g", "weight"));
        out.push(b.sensor("currBrewWeight", "current Brew Weight", "g", "weight"));
        out.push(b.button("scaleCalibrationOn", "Calibrate Scale", "1"));
        out.push(b.button("scaleTareOn", "Tare Scale", "1"));
        out.push(b.number(
            "targetBrewWeight",
            "Brew Weight Target",
            "brew.by_weight.target_weight",
            STEP_TENTH,
            "g",
            "slider",
        ));
    }

    // Conditional on the pressure sensor — MQTTManager.cpp:911.
    if config.hardware.sensors.pressure.enabled {
        out.push(b.sensor("pressure", "Pressure", "bar", "pressure"));
    }

    // Conditional on the water tank — MQTTManager.cpp:914.
    if config.hardware.sensors.watertank.enabled {
        out.push(b.binary_sensor("waterTankFull", "Water Tank Full", "moisture"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builder() -> Builder<'static> {
        Builder {
            prefix: "custom/kitchen/",
            hostname: "clevercoffee",
            discovery_prefix: "homeassistant",
        }
    }

    #[test]
    fn the_discovery_topic_is_the_ha_layout() {
        // homeassistant/<component>/clevercoffee-<hostname>/<name>/config
        assert_eq!(
            builder().discovery_topic(Component::Sensor, "temperature"),
            "homeassistant/sensor/clevercoffee-clevercoffee/temperature/config"
        );
        assert_eq!(
            builder().discovery_topic(Component::BinarySensor, "waterTankFull"),
            "homeassistant/binary_sensor/clevercoffee-clevercoffee/waterTankFull/config"
        );
    }

    #[test]
    fn the_state_and_command_topics_have_no_separator_the_cpp_does_not_have() {
        let b = builder();
        assert_eq!(
            b.state_topic("temperature"),
            "custom/kitchen/clevercoffee/temperature"
        );
        assert_eq!(
            b.command_topic("brewSetpoint"),
            "custom/kitchen/clevercoffee/brewSetpoint/set"
        );
    }

    #[test]
    fn every_payload_carries_the_device_and_availability_blocks() {
        // MQTTManager.cpp:589-601. Without the availability topic Home Assistant
        // marks the entity unavailable the moment the broker restarts, because
        // nothing republishes the status.
        let b = builder();
        for message in [
            b.sensor("temperature", "Boiler Temperature", "°C", "temperature"),
            b.binary_sensor("waterTankFull", "Water Tank Full", "moisture"),
            b.number(
                "brewSetpoint",
                "Brew setpoint",
                "brew.setpoint",
                STEP_TENTH,
                "°C",
                "slider",
            ),
            b.switch("pidON", "Use PID"),
            b.button("scaleTareOn", "Tare Scale", "1"),
        ] {
            let parsed: Value = serde_json::from_str(&message.payload).expect("valid JSON");
            assert_eq!(
                parsed["availability_topic"],
                "custom/kitchen/clevercoffee/status"
            );
            assert_eq!(parsed["payload_available"], "online");
            assert_eq!(parsed["payload_not_available"], "offline");
            assert_eq!(parsed["device"]["identifiers"], "clevercoffee");
            assert_eq!(parsed["device"]["manufacturer"], "CleverCoffee");
            assert_eq!(parsed["device"]["name"], "clevercoffee");
            assert!(
                parsed["unique_id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("clevercoffee-")),
                "{}",
                message.payload
            );
        }
    }

    #[test]
    fn a_sensor_omits_an_empty_unit_and_class() {
        // The C++ passes "" for both on `machineState` and the payload is then
        // serialised without them — `serializeJson` on an ArduinoJson value set
        // to an empty String would emit `"unit_of_measurement":""`, which Home
        // Assistant renders as an entity with a blank unit. Omitting is what
        // `MQTTManager.cpp:748-750` does for `device_class` on a binary sensor
        // and is the behaviour wanted for both here.
        let message = builder().sensor("machineState", "Machine State", "", "enum");
        let parsed: Value = serde_json::from_str(&message.payload).expect("valid JSON");
        assert!(parsed.get("unit_of_measurement").is_none());
        assert_eq!(parsed["device_class"], "enum");
    }

    #[test]
    fn a_binary_sensor_payload_is_on_off() {
        // MQTTManager.cpp:534 publishes "ON"/"OFF" for binary sensors, so the
        // discovery payload must say so or every entity reads as false.
        let message = builder().binary_sensor("waterTankFull", "Water Tank Full", "moisture");
        let parsed: Value = serde_json::from_str(&message.payload).expect("valid JSON");
        assert_eq!(parsed["payload_on"], "ON");
        assert_eq!(parsed["payload_off"], "OFF");
    }

    #[test]
    fn a_switch_payload_is_one_and_zero() {
        // The C++'s parameter topics carry "1"/"0" (MQTTManager.cpp:455), not
        // ON/OFF, so the switch entities must not claim ON/OFF.
        let parsed: Value = serde_json::from_str(&builder().switch("pidON", "Use PID").payload)
            .expect("valid JSON");
        assert_eq!(parsed["payload_on"], "1");
        assert_eq!(parsed["payload_off"], "0");
    }

    #[test]
    fn a_number_takes_its_range_from_the_schema_not_from_a_literal() {
        // The point of `bounds`: the range cannot drift from the parameter.
        // cc_config's schema: brew.setpoint is 20.0..=110.0, default 95.0.
        let (min, max) = bounds("brew.setpoint");
        assert_eq!(min.as_f64(), Some(20.0), "{min}");
        assert_eq!(max.as_f64(), Some(110.0), "{max}");
    }

    #[test]
    fn a_wide_range_still_steps_by_a_tenth() {
        // `brew.setpoint` is twenty wide and the C++ steps it by 0.1
        // (MQTTManager.cpp:855), which is why the step is an argument and not
        // derived from the width.
        let parsed: Value = serde_json::from_str(
            &builder()
                .number(
                    "brewSetpoint",
                    "Brew setpoint",
                    "brew.setpoint",
                    STEP_TENTH,
                    "°C",
                    "slider",
                )
                .payload,
        )
        .expect("valid JSON");
        assert_eq!(parsed["step"].as_f64(), Some(0.1));
    }

    #[test]
    fn an_integer_entity_steps_by_one() {
        // `backflush.cycles`, MQTTManager.cpp:891.
        let parsed: Value = serde_json::from_str(
            &builder()
                .number(
                    "backflushCycles",
                    "Backflush Cycles",
                    "backflush.cycles",
                    STEP_ONE,
                    "",
                    "slider",
                )
                .payload,
        )
        .expect("valid JSON");
        assert_eq!(parsed["step"].as_f64(), Some(1.0));
    }

    #[test]
    fn an_unknown_parameter_gets_a_visible_placeholder_not_a_silent_zero() {
        let (min, max) = bounds("no.such.parameter");
        assert_eq!(min.as_f64(), Some(0.0));
        assert_eq!(max.as_f64(), Some(100.0));
    }

    #[test]
    fn the_default_configuration_publishes_the_sixteen_unconditional_entities() {
        // MQTTManager.cpp:848-871 is 16, and the conditional groups are all
        // disabled by default except the water tank (which defaults on).
        let config = Config::default();
        let messages = all(&config);
        let unconditional = messages
            .iter()
            .filter(|m| {
                ![
                    "currBrewTime",
                    "brewPidDelay",
                    "targetBrewTime",
                    "preinfusion",
                    "preinfusionPause",
                    "backflushCycles",
                    "backflushFillTime",
                    "backflushFlushTime",
                    "backflushOn",
                    "currReadingWeight",
                    "currBrewWeight",
                    "scaleCalibrationOn",
                    "scaleTareOn",
                    "targetBrewWeight",
                    "pressure",
                ]
                .contains(&m.topic.rsplit('/').nth(2).unwrap_or(""))
            })
            .count();
        // 16, exactly MQTTManager.cpp:848-871. The water-tank binary sensor is
        // conditional (`:914`) and `hardware.sensors.watertank.enabled` defaults
        // to false, so the default configuration publishes none of the
        // conditional entities.
        assert_eq!(unconditional, 16);
        assert!(
            !config.hardware.sensors.watertank.enabled,
            "this test's count depends on the water tank being off by default"
        );
    }

    #[test]
    fn the_conditional_entities_appear_when_their_flag_is_set() {
        let mut config = Config::default();
        assert!(!all(&config)
            .iter()
            .any(|m| m.topic.contains("/number/") && m.topic.contains("brewPidDelay")));
        config.hardware.switches.brew.enabled = true;
        assert!(all(&config)
            .iter()
            .any(|m| m.topic.contains("brewPidDelay")));
    }

    #[test]
    fn no_discovery_payload_contains_a_credential() {
        // `mqtt.password` and `system.auth.password` are in the configuration
        // this function is handed. Home Assistant's discovery topic is
        // world-readable on a shared broker, so a payload carrying one would be
        // a leak with no local symptom.
        let mut config = Config::default();
        config.mqtt.password = crate::secret::Secret::new(alloc::string::String::from("brokerpw"));
        config.system.auth.password =
            crate::secret::Secret::new(alloc::string::String::from("httppw"));
        for message in all(&config) {
            assert!(!message.payload.contains("brokerpw"), "{}", message.topic);
            assert!(!message.payload.contains("httppw"), "{}", message.topic);
        }
    }

    #[test]
    fn every_discovery_payload_is_valid_json_and_fits_the_mqtt_buffer() {
        // MQTTManager.cpp:96 raises the buffer to 1024 *for these payloads*. If
        // one exceeded it, the C++'s publishLargeMessage would have split it
        // across `beginPublish`/`print`/`endPublish` and the test that would
        // catch that is here instead.
        for message in all(&Config::default()) {
            let parsed: Result<Value, _> = serde_json::from_str(&message.payload);
            assert!(parsed.is_ok(), "{} is not valid JSON", message.topic);
            assert!(
                message.payload.len() < 1024,
                "{} is {} bytes, over the 1024 buffer",
                message.topic,
                message.payload.len()
            );
        }
    }

    #[test]
    fn the_unique_id_prefix_is_the_csqs() {
        assert_eq!(unique_id_prefix("kitchen"), "clevercoffee-kitchen");
    }
}
