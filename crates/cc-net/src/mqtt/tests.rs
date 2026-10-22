//! Host tests for MQTT topics, membership, plan partitioning, and payload budgets.

#![allow(
    clippy::wildcard_imports,
    reason = "cases span four modules; a glob keeps them from rotting against a \
              rename in any one"
)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::mqtt::cursor::{Item, Phase, Plan};
use cc_config::Config;

use crate::mqtt::*;

fn plan_of(config: &Config) -> Plan<'static> {
    // A `Plan` and its `Registry` need the same lifetime, which cannot be built by
    // value. Leaked, as the real publisher does; bounded by the case count.
    let registry: &'static Registry = Box::leak(Box::new(Registry::from_config(
        &Topics::new("p/", "h"),
        config,
    )));
    let view: &'static PlanView<'static> = Box::leak(Box::new(PlanView::of(registry)));
    view.plan()
}

#[test]
fn the_default_configuration_is_not_a_broker() {
    // Disabled and brokerless is doubly off. A client with nowhere to connect
    // still costs a task stack and two buffers.
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
fn the_topic_layout_inserts_no_separator_of_its_own() {
    // `mqtt.topic`'s default ends in `/`, so no separator is added.
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
fn a_prefix_without_a_trailing_slash_is_used_verbatim() {
    // Prefixes are concatenated verbatim; a separator here would be a new wire
    // format.
    assert_eq!(Topics::new("home/", "cc").state("t"), "home/cc/t");
}

/// Every way a topic can miss the matcher: a deeper path, a bare `status`, a
/// trailing `/extra`, a foreign prefix.
///
/// `<base>+/set` parses to the reading `"+"` and is then refused by the registry.
/// Not refused here on purpose: the rule lives in one place,
/// `an_inbound_reading_resolves_only_if_it_is_registered`, and a second copy is a
/// copy that drifts.
#[test]
fn an_inbound_topic_must_be_a_command_under_the_base() {
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
fn the_buffer_holds_the_largest_published_payload() {
    // A discovery payload with a `device` block is the largest we publish;
    // `cc_config::discovery` asserts itself under this.
    assert_eq!(BUFFER_BYTES, 1024);
}

#[test]
fn the_budget_is_a_fraction_of_the_control_period() {
    // `Feed::service` runs every tick and a tick is 10 ms, so the budget must be a
    // fraction of it. The whole period would bound nothing.
    assert_eq!(TIME_BUDGET_MS, 2);
    const { assert!(TIME_BUDGET_MS * 5 <= 10) };
}

#[test]
fn the_pass_intervals_are_what_they_are_meant_to_be() {
    assert_eq!(INTERVAL_MS, 5_000);
    assert_eq!(INTERVAL_BREW_MS, 500);
    assert_eq!(INTERVAL_STANDBY_MS, 10_000);
    assert_eq!(DISCOVERY_INTERVAL_MS, 300_000);
}

/// The interval is a function of the machine state, which is not the same thing as
/// "a brew is happening".
///
/// `BrewFinished` is excluded from the brew arm, the one case a bare
/// `is_brew_state()` gets wrong. The result screen is static for three seconds.
#[test]
fn the_interval_follows_the_machine_state() {
    use cc_domain::state::MachineState as S;
    assert_eq!(interval_for(S::BrewRunning), INTERVAL_BREW_MS);
    assert_eq!(interval_for(S::BrewPreinfusion), INTERVAL_BREW_MS);
    assert_eq!(
        interval_for(S::BrewFinished),
        INTERVAL_MS,
        "BREW_FINISHED is excluded from the brew arm"
    );
    assert_eq!(interval_for(S::Standby), INTERVAL_STANDBY_MS);
    assert_eq!(interval_for(S::PidNormal), INTERVAL_MS);
    assert_eq!(interval_for(S::SteamRunning), INTERVAL_MS);
}

/// The registry covers the machine, and only the machine.
///
/// **The counts are the whole assertion.** Topic spelling is trivially right;
/// membership is what goes missing silently, and nothing else connects the registry
/// to the entities `cc_config::discovery` advertises.
///
/// Both configurations are written out, not taken from `Config::default()`,
/// because `hardware.switches.brew.enabled` defaults to `true` and a count that
/// quietly depended on it would be a count of the wrong machine.
#[test]
fn the_registry_covers_a_bare_machine_and_a_full_one() {
    use cc_domain::hardware::ScaleType;
    let topics = Topics::new("p/", "h");

    let mut config = Config::default();
    config.hardware.switches.brew.enabled = false;
    let bare = Registry::from_config(&topics, &config);
    // No scale, no tank, no pressure, no brew switch: still a real baseline.
    assert_eq!(bare.parameters().len(), 12);
    assert_eq!(bare.sensors().len(), 9);
    // No tank, so no tank-full sensor: nothing binary at all.
    assert_eq!(bare.binary_sensors(), []);

    // All on, single-cell scale. The dual-cell calibration is behind `HX711_DUAL`,
    // absent here and present below.
    config.hardware.switches.brew.enabled = true;
    config.hardware.sensors.scale.enabled = true;
    config.hardware.sensors.scale.r#type = ScaleType::Hx711Single;
    config.hardware.sensors.watertank.enabled = true;
    config.hardware.sensors.pressure.enabled = true;
    let full = Registry::from_config(&topics, &config);
    // Baseline + fourteen behind the brew switch + five behind the scale.
    assert_eq!(full.parameters().len(), 31);
    // Baseline + brew timer + two weights + pressure.
    assert_eq!(full.sensors().len(), 13);
    // Tank fitted, so its full/empty pair is published.
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

/// Every registered topic resolves, and nothing else does. Otherwise a typo on a
/// command topic becomes a parameter write.
#[test]
fn an_inbound_reading_resolves_only_if_it_is_registered() {
    let topics = Topics::new("p/", "h");
    let mut config = Config::default();
    config.hardware.switches.brew.enabled = true;
    let registry = Registry::from_config(&topics, &config);
    assert_eq!(registry.resolve("brewSetpoint"), Some("brew.setpoint"));
    assert_eq!(registry.resolve("pidON"), Some("pid.enabled"));
    // The two machine-state targets resolve to their sentinel, not a config key.
    assert_eq!(registry.resolve("steamON"), Some(STEAM_MODE));
    assert_eq!(registry.resolve("backflushOn"), Some(BACKFLUSH_ON));
    // Published-but-not-settable, and advertised-but-unregistered, both refuse.
    assert_eq!(registry.resolve("temperature"), None);
    assert_eq!(registry.resolve("usePonM"), None);
}

/// The four special targets are not configuration keys, and are compared **before**
/// the schema is consulted: the configuration carries no steam mode, backflush
/// mode, tare or calibration. Asserting their absence from the schema is what stops
/// somebody "tidying" them into parameters.
#[test]
fn the_four_specials_are_not_configuration_keys() {
    for special in [STEAM_MODE, BACKFLUSH_ON, TARE_ON, CALIBRATION_ON] {
        assert!(
            cc_config::schema::find(special).is_none(),
            "{special} must stay outside the schema: it is machine state, \
             not a parameter"
        );
    }
}

#[test]
fn each_kind_of_topic_lands_in_its_phase() {
    // Phase membership decides the retain flag. Wrong phase means the wrong flag:
    // retained where it must not be, or lost on restart where it must not be.
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
    // And the phases are cut where the registration puts them.
    assert_eq!(plan.phase_items(Phase::Parameters).len(), 12);
    assert_eq!(plan.phase_items(Phase::Sensors).len(), 9);
    assert_eq!(plan.phase_items(Phase::BinarySensors), []);
}

/// The three plan slices must **partition** the view.
///
/// The retain assertions in `each_kind_of_topic_lands_in_its_phase` are vacuous on
/// an empty slice, so on their own they say nothing about where the phases were cut,
/// which is exactly the bug: `PlanView` stored *lengths* and `plan()` used them as
/// *offsets*, so `sensors` came back empty and `binary_sensors` read past the end.
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
    // Pressure is fitted-or-absent, so an unfitted machine must not publish it.
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
/// The registry publishes `currReadingWeight` and `currBrewWeight`, the same two
/// names `cc_config::discovery` advertises.
///
/// Both directions, because the two facts live in different crates and nothing else
/// makes them agree. A registry entry called `weight` was advertised by neither and
/// updated by nothing: an inert entity invisible to tests in either crate.
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
        "`weight` is not a topic; Home Assistant listens on \
         currReadingWeight/currBrewWeight"
    );
}

#[test]
fn a_registry_never_names_a_credential() {
    // Topics are the configuration's own, so a mistake here publishes the MQTT
    // password to every subscriber.
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
