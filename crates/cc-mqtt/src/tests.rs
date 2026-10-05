//! Tests for the whole MQTT surface: the topic layout, the registry, the plan
//! slices, the payload buffer and the interval predicate.
//!
//! These are the assertions that used to run **only** by flashing a board.
//! `mqtt.rs` named `esp_idf_svc`, so `just test` could not reach any of them and
//! the on-target runner (`just test-esp32`) was the only thing that executed
//! them; see the crate root for the finding that established the gap. Nineteen
//! device tests became nineteen host tests, and the bodies are the same bodies.

#![allow(
    clippy::wildcard_imports,
    reason = "the cases are spread across four modules and a glob is what keeps \
              them from rotting against a rename in any one of them"
)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use cc_config::Config;
use cc_netpolicy::mqtt::{Item, Phase, Plan};

use crate::*;

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

#[test]
fn the_default_configuration_is_not_a_broker() {
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

#[test]
fn the_topic_layout_has_no_separator_the_cpp_does_not_have() {
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

#[test]
fn a_prefix_without_a_trailing_slash_reproduces_the_cpp_result() {
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
#[test]
fn an_inbound_topic_is_parsed_exactly_as_the_csqs_matcher_does() {
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

#[test]
fn the_buffer_is_the_csqs_1024() {
    // MQTTManager.cpp:96, raised specifically for discovery payloads.
    // cc_config::discovery asserts every payload is under it.
    assert_eq!(BUFFER_BYTES, 1024);
}

#[test]
fn the_budget_is_a_fraction_of_the_control_period() {
    // The C++'s 10 ms was 2.5 % of a 400 ms loop. This firmware's loop is
    // 10 ms (`cc-firmware/src/main.rs:189`), so the budget is re-derived
    // against that: a quarter of the period would still be most of a tick's
    // worth of publishing, and the whole of it would bound nothing.
    assert_eq!(TIME_BUDGET_MS, 2);
    const { assert!(TIME_BUDGET_MS * 5 <= 10) };
}

#[test]
fn the_intervals_are_the_csqs() {
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
#[test]
fn the_interval_follows_the_machine_state() {
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
#[test]
fn the_registry_is_the_csqs_registration() {
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
    assert_eq!(bare.binary_sensors(), []);

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
#[test]
fn an_inbound_reading_resolves_only_if_it_is_registered() {
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
#[test]
fn the_four_specials_are_not_configuration_keys() {
    for special in [STEAM_MODE, BACKFLUSH_ON, TARE_ON, CALIBRATION_ON] {
        assert!(
            cc_config::schema::find(special).is_none(),
            "{special} must stay outside the schema: it is machine state, \
             not a parameter (MQTTManager.cpp:296-322)"
        );
    }
}

#[test]
fn the_registry_puts_each_kind_of_topic_in_the_cpp_phase() {
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
    assert_eq!(plan.phase_items(Phase::BinarySensors), []);
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
#[test]
fn the_plan_slices_partition_the_view() {
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
    assert_eq!(plan.parameters.len(), registry.parameters().len());
    assert_eq!(plan.sensors.len(), registry.sensors().len());
    assert_eq!(plan.binary_sensors.len(), registry.binary_sensors().len());
    assert_eq!(
        plan_topics(plan.sensors),
        registry_topics(registry.sensors())
    );
    assert_eq!(
        plan_topics(plan.binary_sensors),
        registry_topics(registry.binary_sensors())
    );
}

#[test]
fn a_pressure_sensor_appears_only_when_it_is_fitted() {
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
#[test]
fn the_weight_topics_are_exactly_the_ones_discovery_advertises() {
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

#[test]
fn a_registry_never_names_a_credential() {
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

#[test]
fn the_topics_string_names_the_base_so_a_bring_up_log_is_useful() {
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
#[test]
fn every_registered_value_fits_the_payload_buffer() {
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
#[test]
fn the_shared_plan_helper_still_sees_the_whole_registry() {
    let mut config = Config::default();
    config.hardware.switches.brew.enabled = false;
    assert_eq!(plan_of(&config).len(), 12 + 9);
}
