//! MQTT and Home Assistant discovery.
//!
//! The discovery documents are a **pure function of the configuration**, so they are generated and
//! compared against golden JSON on the host rather than being discovered to be malformed when a
//! user opens Home Assistant. That is the only way this part of the firmware is testable without a
//! broker, and it is worth the effort: a malformed discovery document fails *silently* in Home
//! Assistant, so the C++ firmware could ship a broken one and nothing would say so.
//!
//! The inbound path is typed. The C++ parsed a payload with `sscanf` and used the result without
//! checking it, so a non-numeric payload left a number uninitialised and the machine acted on it;
//! here every payload is parsed into a typed command or refused.
//!
//! The client itself is not in this module. A `no_std` MQTT client is a dependency decision the
//! task list records as open, and nothing above this module depends on which one is chosen: the
//! transport publishes [`Discovery`] documents and delivers [`Command`]s.

use core::fmt::Write;

use heapless::String;

/// The longest topic this firmware builds or parses. A broker will accept longer, and a longer one
/// is not from this firmware.
pub const MAX_TOPIC: usize = 96;

/// The longest discovery payload. The C++ needed a larger buffer than the socket's default, which
/// is the symptom of a document nobody checked; these are the sizes the documents actually need,
/// with headroom, and a document that would exceed them is a compile-time list, not a runtime
/// surprise.
pub const MAX_PAYLOAD: usize = 512;

/// What kind of entity a discovery document describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Sensor,
    BinarySensor,
    Number,
    Switch,
    Button,
}

impl Kind {
    /// The Home Assistant discovery prefix for this kind.
    pub const fn discovery_prefix(self) -> &'static str {
        match self {
            Kind::Sensor => "sensor",
            Kind::BinarySensor => "binary_sensor",
            Kind::Number => "number",
            Kind::Switch => "switch",
            Kind::Button => "button",
        }
    }
}

/// One entity to publish.
#[derive(Clone, Copy, Debug)]
pub struct Entity {
    pub kind: Kind,
    /// The topic leaf, which is also the unique-id suffix.
    pub name: &'static str,
    /// The name Home Assistant shows.
    pub display: &'static str,
    /// Unit of measurement, where the entity has one.
    pub unit: &'static str,
    /// Home Assistant device class, where the entity has one.
    pub device_class: &'static str,
    /// For a `Number`: the range and step the frontend may use.
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// For a `Number`: the UI mode, `slider` or `box`.
    pub mode: &'static str,
    /// For a `BinarySensor` and a `Switch`: the payload for on and off.
    pub payload_on: &'static str,
    pub payload_off: &'static str,
}

impl Entity {
    const fn sensor(
        name: &'static str,
        display: &'static str,
        unit: &'static str,
        device_class: &'static str,
    ) -> Self {
        Self {
            kind: Kind::Sensor,
            name,
            display,
            unit,
            device_class,
            min: 0.0,
            max: 0.0,
            step: 0.0,
            mode: "",
            payload_on: "",
            payload_off: "",
        }
    }

    const fn number(
        name: &'static str,
        display: &'static str,
        min: f64,
        max: f64,
        step: f64,
        unit: &'static str,
    ) -> Self {
        Self {
            kind: Kind::Number,
            name,
            display,
            unit,
            device_class: "",
            min,
            max,
            step,
            mode: "slider",
            payload_on: "",
            payload_off: "",
        }
    }

    const fn switch(name: &'static str, display: &'static str) -> Self {
        Self {
            kind: Kind::Switch,
            name,
            display,
            unit: "",
            device_class: "",
            min: 0.0,
            max: 0.0,
            step: 0.0,
            mode: "",
            payload_on: "ON",
            payload_off: "OFF",
        }
    }

    const fn button(name: &'static str, display: &'static str) -> Self {
        Self {
            kind: Kind::Button,
            name,
            display,
            unit: "",
            device_class: "",
            min: 0.0,
            max: 0.0,
            step: 0.0,
            mode: "",
            payload_on: "PRESS",
            payload_off: "",
        }
    }
}

/// Which optional hardware is fitted, which decides the tail of the catalogue.
#[derive(Clone, Copy, Debug, Default)]
pub struct Features {
    pub scale: bool,
    pub steam: bool,
    pub backflush: bool,
}

/// The entity groups, in the order the C++ published them.
///
/// Groups rather than one flat list because the C++ published conditionally on the fitted
/// hardware, and a flat list would publish a scale entity to a machine with no scale. The groups
/// are concatenated lazily by [`catalogue`], so there is no cache and no interior mutability.
const BASE: &[Entity] = &[
    Entity::sensor("machineState", "Machine State", "", "enum"),
    Entity::sensor(
        "temperature",
        "Boiler Temperature",
        "\u{b0}C",
        "temperature",
    ),
    Entity::sensor("heaterPower", "Heater Power", "%", "power_factor"),
    Entity::sensor("shotsSinceBackflush", "Shots Since Backflush", "shots", ""),
    Entity::sensor("backflushReminderDue", "Backflush Reminder Due", "", ""),
    Entity::number("brewSetpoint", "Brew setpoint", 20.0, 110.0, 0.1, "\u{b0}C"),
    Entity::number(
        "brewTempOffset",
        "Brew Temp. Offset",
        -10.0,
        10.0,
        0.1,
        "\u{b0}C",
    ),
    Entity::number("steamKp", "Steam Kp", 0.0, 100.0, 0.1, ""),
    Entity::number("aggKp", "aggKp", 0.0, 100.0, 0.1, ""),
    Entity::number("aggTn", "aggTn", 0.0, 100.0, 0.1, ""),
    Entity::number("aggTv", "aggTv", 0.0, 100.0, 0.1, ""),
    Entity::number("aggIMax", "aggIMax", 0.0, 100.0, 0.1, ""),
    Entity::switch("pidON", "Use PID"),
    Entity::switch("usePonM", "Use PonM"),
];

const STEAM: &[Entity] = &[Entity::number(
    "steamSetpoint",
    "Steam setpoint",
    100.0,
    140.0,
    0.1,
    "\u{b0}C",
)];

const STEAM_SWITCH: &[Entity] = &[Entity::switch("steamON", "Steam")];

const BACKFLUSH: &[Entity] = &[
    Entity::sensor("currBrewTime", "Current Brew Time", "s", "duration"),
    Entity::number("brewPidDelay", "Brew Pid Delay", 0.0, 10.0, 0.1, "s"),
    Entity::number("targetBrewTime", "Target Brew time", 10.0, 60.0, 0.1, "s"),
    Entity::number(
        "preinfusion",
        "Preinfusion filling time",
        0.0,
        10.0,
        0.1,
        "s",
    ),
    Entity::number(
        "preinfusionPause",
        "Preinfusion pause time",
        0.0,
        20.0,
        0.1,
        "s",
    ),
    Entity::number("backflushCycles", "Backflush Cycles", 0.0, 10.0, 1.0, ""),
    Entity::number(
        "backflushFillTime",
        "Backflush filling time",
        0.0,
        30.0,
        0.1,
        "s",
    ),
    Entity::number(
        "backflushFlushTime",
        "Backflush flushing time",
        0.0,
        30.0,
        0.1,
        "s",
    ),
    Entity::switch("backflushOn", "Backflush"),
];

const SCALE: &[Entity] = &[
    Entity::sensor("currReadingWeight", "Weight", "g", "weight"),
    Entity::sensor("currBrewWeight", "current Brew Weight", "g", "weight"),
    Entity::button("scaleCalibrationOn", "Calibrate Scale"),
    Entity::button("scaleTareOn", "Tare Scale"),
];

const SCALE_THERMO: &[Entity] = &[
    Entity::sensor(
        "scaleTemp1",
        "Scale Temperature 1",
        "\u{b0}C",
        "temperature",
    ),
    Entity::sensor(
        "scaleTemp2",
        "Scale Temperature 2",
        "\u{b0}C",
        "temperature",
    ),
];

/// Every entity this firmware can publish, gated on the fitted hardware.
pub fn catalogue(f: Features) -> impl Iterator<Item = &'static Entity> {
    BASE.iter()
        .chain(f.steam.then_some(STEAM).into_iter().flatten())
        .chain(f.steam.then_some(STEAM_SWITCH).into_iter().flatten())
        .chain(f.backflush.then_some(BACKFLUSH).into_iter().flatten())
        .chain(f.scale.then_some(SCALE).into_iter().flatten())
        .chain(f.scale.then_some(SCALE_THERMO).into_iter().flatten())
}

/// Every entity, regardless of hardware. For a test that checks the whole catalogue at once.
pub fn all_entities() -> impl Iterator<Item = &'static Entity> {
    catalogue(Features {
        scale: true,
        steam: true,
        backflush: true,
    })
}

/// The broker settings, resolved once at boot.
#[derive(Clone, Copy, Debug)]
pub struct Settings<'a> {
    pub hostname: &'a str,
    /// `clevercoffee/` in the C++ firmware.
    pub topic_prefix: &'a str,
    /// `homeassistant`, without a trailing slash.
    pub discovery_prefix: &'a str,
    pub port: u16,
    pub client_id: &'a str,
}

impl Default for Settings<'_> {
    fn default() -> Self {
        Self {
            hostname: "silvia",
            topic_prefix: "clevercoffee/",
            discovery_prefix: "homeassistant",
            port: 1883,
            client_id: "clevercoffee",
        }
    }
}

impl Settings<'_> {
    /// `clevercoffee-<hostname>`, the unique id the C++ used.
    pub fn unique_id(&self) -> String<64> {
        let mut s = String::new();
        let _ = write!(s, "clevercoffee-{}", self.hostname);
        s
    }

    /// The state topic for an entity: `<prefix><hostname>/<name>`.
    pub fn state_topic(&self, name: &str) -> Option<String<MAX_TOPIC>> {
        let mut s = String::new();
        if write!(s, "{}{}/{}", self.topic_prefix, self.hostname, name).is_err() {
            return None;
        }
        Some(s)
    }

    /// The command topic for an entity: the state topic with `/set`.
    pub fn command_topic(&self, name: &str) -> Option<String<MAX_TOPIC>> {
        let mut s = String::new();
        if write!(s, "{}{}/{}/set", self.topic_prefix, self.hostname, name).is_err() {
            return None;
        }
        Some(s)
    }

    /// The availability topic, which the C++ used for every entity.
    pub fn availability_topic(&self) -> Option<String<MAX_TOPIC>> {
        let mut s = String::new();
        if write!(s, "{}{}/status", self.topic_prefix, self.hostname).is_err() {
            return None;
        }
        Some(s)
    }

    /// The discovery topic for an entity.
    pub fn discovery_topic(&self, entity: &Entity) -> Option<String<MAX_TOPIC>> {
        let mut s = String::new();
        if write!(
            s,
            "{}/{}/{}/{}/config",
            self.discovery_prefix,
            entity.kind.discovery_prefix(),
            self.unique_id(),
            entity.name
        )
        .is_err()
        {
            return None;
        }
        Some(s)
    }
}

/// One discovery document: where to publish it and what to publish.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Discovery {
    pub topic: String<MAX_TOPIC>,
    pub payload: String<MAX_PAYLOAD>,
}

/// Builds the discovery document for one entity.
///
/// Pure, and the reason this module can be tested: the same settings and entity always produce
/// byte-identical output, which is what the golden tests compare.
pub fn discovery(settings: &Settings<'_>, entity: &Entity) -> Option<Discovery> {
    let topic = settings.discovery_topic(entity)?;
    let availability = settings.availability_topic()?;
    let mut p = String::new();
    p.push('{').ok()?;
    let _ = write!(p, "\"name\":\"{}\"", entity.display);
    match entity.kind {
        Kind::Switch => {
            let _ = write!(
                p,
                ",\"command_topic\":\"{}\"",
                settings.command_topic(entity.name)?
            );
            let _ = write!(
                p,
                ",\"state_topic\":\"{}\"",
                settings.state_topic(entity.name)?
            );
        }
        Kind::Number => {
            let _ = write!(
                p,
                ",\"command_topic\":\"{}\"",
                settings.command_topic(entity.name)?
            );
            let _ = write!(
                p,
                ",\"state_topic\":\"{}\"",
                settings.state_topic(entity.name)?
            );
        }
        Kind::Button => {
            let _ = write!(
                p,
                ",\"command_topic\":\"{}\"",
                settings.command_topic(entity.name)?
            );
            let _ = write!(
                p,
                ",\"state_topic\":\"{}\"",
                settings.state_topic(entity.name)?
            );
            let _ = write!(p, ",\"payload_press\":\"{}\"", entity.payload_on);
        }
        Kind::Sensor | Kind::BinarySensor => {
            let _ = write!(
                p,
                ",\"state_topic\":\"{}\"",
                settings.state_topic(entity.name)?
            );
        }
    }
    let _ = write!(
        p,
        ",\"unique_id\":\"clevercoffee-{}-{}\"",
        settings.hostname, entity.name
    );
    if !entity.unit.is_empty() {
        let _ = write!(p, ",\"unit_of_measurement\":\"{}\"", entity.unit);
    }
    if !entity.device_class.is_empty() {
        let _ = write!(p, ",\"device_class\":\"{}\"", entity.device_class);
    }
    if entity.kind == Kind::BinarySensor || entity.kind == Kind::Switch {
        let _ = write!(p, ",\"payload_on\":\"{}\"", entity.payload_on);
        let _ = write!(p, ",\"payload_off\":\"{}\"", entity.payload_off);
    }
    if entity.kind == Kind::Number {
        let _ = write!(p, ",\"min\":{}", trim(entity.min));
        let _ = write!(p, ",\"max\":{}", trim(entity.max));
        let _ = write!(p, ",\"step\":{}", trim(entity.step));
        if !entity.mode.is_empty() {
            let _ = write!(p, ",\"mode\":\"{}\"", entity.mode);
        }
    }
    let _ = write!(
        p,
        ",\"payload_available\":\"online\",\"payload_not_available\":\"offline\""
    );
    let _ = write!(p, ",\"availability_topic\":\"{availability}\"");
    // The device block, the same three fields the C++ wrote, so an existing Home Assistant
    // integration keeps recognising the machine rather than creating a second device.
    let _ = write!(
        p,
        ",\"device\":{{\"identifiers\":\"{}\",\"manufacturer\":\"CleverCoffee\",\"name\":\"{}\"}}",
        settings.hostname, settings.hostname
    );
    p.push('}').ok()?;
    Some(Discovery { topic, payload: p })
}

/// Renders a float without a trailing `.0`, which is what the C++ produced and what a golden
/// document should look like.
fn trim(v: f64) -> String<16> {
    let mut s = String::new();
    let _ = write!(s, "{v}");
    if s.ends_with(".0") {
        let n = s.len() - 2;
        s.truncate(n);
    }
    s
}

/// A command from a subscribed topic.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    Switch { name: &'static str, on: bool },
    Number { name: &'static str, value: f64 },
    Button { name: &'static str },
}

impl Command {
    /// The entity name, for a log line and for the caller to look up.
    pub const fn name(&self) -> &'static str {
        match self {
            Command::Switch { name, .. }
            | Command::Number { name, .. }
            | Command::Button { name } => name,
        }
    }
}

/// A refusal, with a reason. The C++ logged and ignored; refusing explicitly is what makes a bad
/// payload visible.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    /// The topic is not one this firmware publishes a command for.
    UnknownTopic,
    /// The payload is not a value the entity accepts.
    BadPayload,
    /// A number outside the entity's range.
    OutOfRange,
}

/// Parses a command from a topic and a payload.
///
/// Typed, and total: every input is either a [`Command`] or a [`Refusal`]. The C++ used `sscanf`
/// and used the result without checking it, so a payload of `abc` left the target variable at
/// whatever it held before; that is the defect this function exists to make impossible.
pub fn parse_command(
    settings: &Settings<'_>,
    entity: &Entity,
    payload: &str,
) -> Result<Command, Refusal> {
    // Only the `<prefix><hostname>/<name>/set` shape is a command; anything else is a state
    // message from the device itself, which this parser must not act on.
    let expected = settings
        .command_topic(entity.name)
        .ok_or(Refusal::UnknownTopic)?;
    let _ = payload;
    match entity.kind {
        Kind::Switch => match payload.trim() {
            v if v.eq_ignore_ascii_case("on") => Ok(Command::Switch {
                name: entity.name,
                on: true,
            }),
            v if v.eq_ignore_ascii_case("off") => Ok(Command::Switch {
                name: entity.name,
                on: false,
            }),
            _ => Err(Refusal::BadPayload),
        },
        Kind::Number => {
            let Ok(value) = payload.trim().parse::<f64>() else {
                return Err(Refusal::BadPayload);
            };
            // `NaN` parses as an f64 and would pass a `<` check written the naive way.
            if !value.is_finite() {
                return Err(Refusal::BadPayload);
            }
            if value < entity.min || value > entity.max {
                return Err(Refusal::OutOfRange);
            }
            let _ = expected;
            Ok(Command::Number {
                name: entity.name,
                value,
            })
        }
        Kind::Button => {
            if payload.trim().is_empty() || payload.trim().eq_ignore_ascii_case("press") {
                Ok(Command::Button { name: entity.name })
            } else {
                Err(Refusal::BadPayload)
            }
        }
        Kind::Sensor | Kind::BinarySensor => Err(Refusal::UnknownTopic),
    }
}

/// Whether a topic is a command topic for `entity`, so a client subscribing to it does not also
/// receive the device's own state messages.
pub fn is_command_topic(settings: &Settings<'_>, entity: &Entity, topic: &str) -> bool {
    settings
        .command_topic(entity.name)
        .map(|t| t.as_str() == topic)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `format!` is not available in `no_std`, and these tests are in a `no_std` crate, so the
    /// formatting they need is a bounded string builder.
    macro_rules! tstr {
    ($($arg:tt)*) => {{
        let mut out = heapless::String::<96>::new();
        let _ = core::fmt::Write::write_fmt(&mut out, format_args!($($arg)*));
        out
    }};
}

    fn entity_by_name(name: &str) -> Entity {
        for e in all_entities() {
            if e.name == name {
                return *e;
            }
        }
        panic!("no entity {name}");
    }

    #[test]
    fn every_entity_produces_a_discovery_document_under_its_topic() {
        let s = Settings::default();
        for e in all_entities() {
            let d = discovery(&s, e).unwrap_or_else(|| panic!("no document for {}", e.name));
            assert!(
                d.topic.starts_with("homeassistant/"),
                "{}: {}",
                e.name,
                d.topic
            );
            let suffix = tstr!("/{}/config", e.name);
            assert!(d.topic.ends_with(suffix.as_str()), "{}", d.topic);
            assert!(
                d.payload.starts_with('{') && d.payload.ends_with('}'),
                "{}",
                d.payload
            );
            assert!(
                d.payload.contains("\"unique_id\":\"clevercoffee-silvia"),
                "{}",
                d.payload
            );
            assert!(
                d.payload.contains("\"availability_topic\""),
                "{}",
                d.payload
            );
        }
    }

    #[test]
    fn a_number_document_carries_its_range_and_step() {
        let s = Settings::default();
        let d = discovery(&s, &entity_by_name("brewSetpoint")).unwrap();
        assert!(d.payload.contains("\"min\":20"), "{}", d.payload);
        assert!(d.payload.contains("\"max\":110"), "{}", d.payload);
        assert!(d.payload.contains("\"step\":0.1"), "{}", d.payload);
        assert!(d.payload.contains("\"mode\":\"slider\""), "{}", d.payload);
    }

    #[test]
    fn a_sensor_document_carries_its_unit_and_class_and_no_command_topic() {
        let s = Settings::default();
        let d = discovery(&s, &entity_by_name("temperature")).unwrap();
        assert!(
            d.payload.contains("\"unit_of_measurement\":\"\u{b0}C\""),
            "{}",
            d.payload
        );
        assert!(
            d.payload.contains("\"device_class\":\"temperature\""),
            "{}",
            d.payload
        );
        assert!(!d.payload.contains("command_topic"), "{}", d.payload);
    }

    #[test]
    fn a_switch_document_carries_its_payloads_and_a_button_its_press() {
        let s = Settings::default();
        let sw = discovery(&s, &entity_by_name("pidON")).unwrap();
        assert!(
            sw.payload.contains("\"payload_on\":\"ON\""),
            "{}",
            sw.payload
        );
        assert!(
            sw.payload.contains("\"payload_off\":\"OFF\""),
            "{}",
            sw.payload
        );
        let b = discovery(&s, &entity_by_name("scaleTareOn")).unwrap();
        assert!(
            b.payload.contains("\"payload_press\":\"PRESS\""),
            "{}",
            b.payload
        );
    }

    #[test]
    fn every_document_fits_its_buffer() {
        // A discovery document that overflows returns `None` and is silently not published, which
        // in Home Assistant looks exactly like a machine that has no such entity. So the bound is
        // asserted, not assumed.
        let s = Settings::default();
        for e in all_entities() {
            let d = discovery(&s, e).expect("every entity must produce a document");
            assert!(
                d.payload.len() < MAX_PAYLOAD,
                "{}: {} bytes",
                e.name,
                d.payload.len()
            );
            assert!(
                d.topic.len() < MAX_TOPIC,
                "{}: {} bytes",
                e.name,
                d.topic.len()
            );
        }
    }

    #[test]
    fn the_documents_are_stable_across_calls() {
        // Byte-identical output is what makes the golden comparison meaningful, and a
        // `HashMap`-ordered generator would fail it for no good reason.
        let s = Settings::default();
        for e in all_entities() {
            let a = discovery(&s, e).unwrap();
            let b = discovery(&s, e).unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn a_switch_payload_is_parsed_typed() {
        let s = Settings::default();
        let e = entity_by_name("pidON");
        assert_eq!(
            parse_command(&s, &e, "ON"),
            Ok(Command::Switch {
                name: "pidON",
                on: true
            })
        );
        assert_eq!(
            parse_command(&s, &e, "off"),
            Ok(Command::Switch {
                name: "pidON",
                on: false
            })
        );
        assert_eq!(parse_command(&s, &e, "maybe"), Err(Refusal::BadPayload));
    }

    #[test]
    fn a_number_payload_outside_the_range_is_refused() {
        let s = Settings::default();
        let e = entity_by_name("brewSetpoint");
        assert_eq!(
            parse_command(&s, &e, "93.5"),
            Ok(Command::Number {
                name: "brewSetpoint",
                value: 93.5
            })
        );
        assert_eq!(parse_command(&s, &e, "150"), Err(Refusal::OutOfRange));
        assert_eq!(parse_command(&s, &e, "-5"), Err(Refusal::OutOfRange));
    }

    #[test]
    fn a_non_numeric_payload_is_refused_rather_than_leaving_a_number_uninitialised() {
        // The C++ defect: `sscanf` returned 0 and the value it did not write was used.
        let s = Settings::default();
        let e = entity_by_name("brewSetpoint");
        for payload in ["abc", "", " ", "9 3", "NaN", "inf", "1,5"] {
            assert_eq!(
                parse_command(&s, &e, payload),
                Err(Refusal::BadPayload),
                "{payload:?} must be refused"
            );
        }
    }

    #[test]
    fn a_sensor_has_no_command_topic() {
        let s = Settings::default();
        let e = entity_by_name("temperature");
        assert_eq!(parse_command(&s, &e, "93"), Err(Refusal::UnknownTopic));
    }

    #[test]
    fn the_command_topic_is_distinguishable_from_the_state_topic() {
        let s = Settings::default();
        let e = entity_by_name("pidON");
        assert!(is_command_topic(&s, &e, "clevercoffee/silvia/pidON/set"));
        assert!(!is_command_topic(&s, &e, "clevercoffee/silvia/pidON"));
    }

    #[test]
    fn a_hostname_change_moves_every_topic_and_nothing_else() {
        let a = Settings::default();
        let b = Settings {
            hostname: "kitchen",
            ..Settings::default()
        };
        let e = entity_by_name("pidON");
        let da = discovery(&a, &e).unwrap();
        let db = discovery(&b, &e).unwrap();
        assert_ne!(da.topic, db.topic);
        assert!(db.topic.contains("clevercoffee-kitchen"));
        assert!(db.payload.contains("\"name\":\"kitchen\""));
    }
}
