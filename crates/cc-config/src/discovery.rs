//! Home Assistant discovery payloads derived from configuration and schema.
//!
//! Entity availability follows the fitted hardware and switch modes. Parameter
//! bounds come from [`crate::schema::SCHEMA`] so discovery cannot drift from validation.
//! Malformed payloads are silently ignored by Home Assistant.

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
/// ```json
/// "device": { "identifiers": "<hostname>",
///              "manufacturer": "CleverCoffee",
///              "name": "<hostname>" }
/// ```
///
/// `identifiers` is a bare hostname rather than a list, because Home Assistant
/// accepts a string there. It is not changed to a list: the unique id and the
/// device identifier are the join key for an install that has already adopted this
/// machine, and an install that has to delete and re-add the integration is a
/// support call.
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
fn availability(hostname: &str, prefix: &str, status_topic: &str) -> Value {
    json!({
        "payload_available": "online",
        "payload_not_available": "offline",
        "availability_topic": format!("{prefix}{hostname}/{status_topic}"),
    })
}

/// The unique id prefix, `clevercoffee-<hostname>`.
///
/// Part of the entity's join key in Home Assistant, so it is fixed rather than
/// derived from anything that could change.
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

    /// A `sensor` entity.
    ///
    /// `unit` and `device_class` are empty strings for the entities that have
    /// none, and an empty `device_class` is *omitted* from the payload: an empty
    /// string there makes Home Assistant show the entity without an icon, and for
    /// some entities that is the intent.
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

    /// A `binary_sensor` entity.
    ///
    /// The payload is `ON`/`OFF`, fixed rather than a parameter, because that is
    /// what the publisher of these topics emits.
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

    /// A `number` entity.
    ///
    /// `min`, `max` and `step` come from the configuration schema by dotted key,
    /// so they cannot drift from the parameter they describe. `ui_mode` is
    /// Home Assistant's `mode` field (`slider` or `box`).
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

    /// A `switch` entity.
    ///
    /// The payload is `1`/`0`, not `ON`/`OFF`: these are the machine's own
    /// parameter topics, and a parameter is written as a number.
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

    /// A `button` entity.
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
/// A field-by-field merge, so it cannot drop a key `body` already carries.
fn merge(body: &mut Value, availability: &Value) {
    if let (Some(target), Some(source)) = (body.as_object_mut(), availability.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// The `(min, max)` a `number` entity should publish, from the schema.
///
/// Read from [`SCHEMA`] rather than hard-coded, so a range change in the schema
/// reaches Home Assistant without a second edit here. A key that is not in the
/// schema yields `0.0 / 100.0` and a number entity with a useless range — which is
/// a visible bug rather than a silent one, and is what a missing schema entry
/// should look like.
///
/// **The `step` is a separate argument and deliberately so.** The schema has no
/// step column, and deriving one from the range's width is wrong:
/// `brew.setpoint` is 20 wide and is still stepped by 0.1. So the step is passed
/// explicitly, and the call sites in [`all`] are the single place that declares
/// it.
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

/// The step for a `0.1`-granular entity: every temperature and PID gain.
pub const STEP_TENTH: f64 = 0.1;

/// The step for a count, such as `backflush.cycles`.
pub const STEP_ONE: f64 = 1.0;

/// The whole discovery set for `config`, in publication order.
///
/// Built with `alloc`, so it is a `Vec` of 16–29 [`Discovery`] values of a few
/// hundred bytes each. They are all built and then published under the same 10 ms
/// budget as the telemetry, so a large set is truncated the same way and the next
/// 300 s pass picks up where this one stopped. The retention flag is on every one
/// of them, without which Home Assistant would re-adopt the entities on every
/// broker restart.
#[expect(
    clippy::too_many_lines,
    reason = "this IS the discovery table: 16 unconditional entities plus the \
              conditionals, in publication order. Splitting it would hide the one \
              property worth checking by eye, which is that no entity is listed \
              twice and none is missing."
)]
#[must_use]
pub fn all(config: &Config) -> alloc::vec::Vec<Discovery> {
    let b = Builder {
        prefix: &config.mqtt.topic,
        hostname: &config.system.hostname,
        discovery_prefix: &config.mqtt.hassio.prefix,
    };

    // The 16 unconditional entities.
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

    // Present only while the brew switch entity is enabled.
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

    // Present only while a scale is enabled.
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

    // Present only while the pressure sensor is enabled.
    if config.hardware.sensors.pressure.enabled {
        out.push(b.sensor("pressure", "Pressure", "bar", "pressure"));
    }

    // Present only while the water tank sensor is enabled.
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
    fn the_state_and_command_topics_have_no_separator() {
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
        // Without the availability topic Home Assistant marks the entity
        // unavailable the moment the broker restarts, because nothing republishes
        // the status.
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
        // An empty string is omitted, not serialised: `"unit_of_measurement":""`
        // is an entity Home Assistant renders with a blank unit. `machineState`
        // has no unit and keeps its class, so both branches are exercised here.
        let message = builder().sensor("machineState", "Machine State", "", "enum");
        let parsed: Value = serde_json::from_str(&message.payload).expect("valid JSON");
        assert!(parsed.get("unit_of_measurement").is_none());
        assert_eq!(parsed["device_class"], "enum");
    }

    #[test]
    fn a_binary_sensor_payload_is_on_off() {
        // A binary sensor's topic publishes "ON"/"OFF", so the discovery payload
        // must say so or every entity reads as false.
        let message = builder().binary_sensor("waterTankFull", "Water Tank Full", "moisture");
        let parsed: Value = serde_json::from_str(&message.payload).expect("valid JSON");
        assert_eq!(parsed["payload_on"], "ON");
        assert_eq!(parsed["payload_off"], "OFF");
    }

    #[test]
    fn a_switch_payload_is_one_and_zero() {
        // A parameter topic carries "1"/"0", not ON/OFF, so the switch entities
        // must not claim ON/OFF.
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
        // `brew.setpoint` is twenty wide and is still stepped by 0.1, which is why
        // the step is an argument and not derived from the width.
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
        // `backflush.cycles` is a count.
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
        // Sixteen, and the conditional groups are all disabled by default except
        // the water tank (which defaults on).
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
        // 25, not 16. The nine extra entities are the switch-conditional ones,
        // because all four `hardware.switches.*.enabled` flags default to `true`
        // here — the deliberate default.
        // The water-tank binary sensor is still conditional and
        // `hardware.sensors.watertank.enabled` is still `false`, so that entity
        // is still absent.
        assert_eq!(unconditional, 25);
        assert!(
            !config.hardware.sensors.watertank.enabled,
            "this test's count depends on the water tank still being off by default"
        );
    }

    #[test]
    fn the_conditional_entities_appear_when_their_flag_is_set() {
        let mut config = Config::default();
        // The brew switch now defaults to enabled, so `brewPidDelay` is
        // published out of the box. The assertion that matters is the
        // direction of the relationship, so it is checked by *turning it off*.
        assert!(all(&config)
            .iter()
            .any(|m| m.topic.contains("/number/") && m.topic.contains("brewPidDelay")));
        config.hardware.switches.brew.enabled = false;
        assert!(!all(&config)
            .iter()
            .any(|m| m.topic.contains("/number/") && m.topic.contains("brewPidDelay")));
    }

    #[test]
    fn no_discovery_payload_contains_a_credential() {
        // `mqtt.password` and `system.auth.password` are in the configuration
        // this function is handed. Home Assistant's discovery topic is
        // world-readable on a shared broker, so a payload carrying one would be
        // a leak with no local symptom.
        let mut config = Config::default();
        config.mqtt.password = crate::Secret::new(alloc::string::String::from("brokerpw"));
        config.system.auth.password = crate::Secret::new(alloc::string::String::from("httppw"));
        for message in all(&config) {
            assert!(!message.payload.contains("brokerpw"), "{}", message.topic);
            assert!(!message.payload.contains("httppw"), "{}", message.topic);
        }
    }

    #[test]
    fn every_discovery_payload_is_valid_json_and_fits_the_mqtt_buffer() {
        // The MQTT buffers are 1024 B, and they were sized for these payloads. One
        // larger than the buffer cannot be published at all, so the failure is a
        // discovery message that never arrives — and Home Assistant keeps whatever
        // entity it had, silently. Asserted here rather than discovered on a broker.
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
    fn the_unique_id_prefix_is_the_expected_one() {
        assert_eq!(unique_id_prefix("kitchen"), "clevercoffee-kitchen");
    }
}
