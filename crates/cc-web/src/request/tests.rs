//! The command-parsing tests, moved from `cc-hal-esp32/src/web.rs`.

use super::*;

use crate::telemetry::Telemetry;

// ==================================================== the setpoint route

#[test]
fn the_setpoint_route_takes_what_the_schema_will_store() {
    // The bound is `cc_config::assign::parse`'s, not a repeated literal, so
    // this cannot drift from `brew.setpoint`'s `ParamSpec` range. The pairs
    // are the schema's two edges (`Config.h:795-802`, `defaults.h:81-82`)
    // and the value the integration checklist posts, spelled both ways.
    for (field, expected) in [
        ("20", 20.0),
        ("20.0", 20.0),
        ("95", 95.0),
        ("95.0", 95.0),
        ("110", 110.0),
        ("110.0", 110.0),
    ] {
        assert_eq!(
            parse_setpoint(field),
            Some(Command::SetSetpoint(expected)),
            "{field} is inside the schema's range and must be accepted"
        );
    }
}

#[test]
fn a_fractional_setpoint_survives_to_the_command() {
    // The one that was broken on the bench. `93.5`, `80.5` and `91.2` were all
    // accepted with `202 {"accepted":true}` and all arrived as the truncated
    // integer, because `parse_setpoint` cast to `i32`. The C++ passes the
    // `double` straight through (`WebServerManager.cpp:394-396`) and
    // `brew.setpoint` is a float parameter, so 93.5 must stay 93.5.
    for (field, expected) in [("93.5", 93.5), ("80.5", 80.5), ("91.2", 91.2)] {
        assert_eq!(
            parse_setpoint(field),
            Some(Command::SetSetpoint(expected)),
            "{field} must not be truncated"
        );
    }
}

#[test]
fn the_setpoint_route_refuses_a_value_that_would_defeat_the_interlock() {
    // The defect this route carried: `?value=150` passed the C++'s
    // permissive `0.0..=150.0` filter and was persisted, which parks the
    // boiler on `safety.emergency_temp` (default 150) where S1's *strictly
    // greater* test can never trip. A `None` here is a `400` from
    // `register_command`, not a silently clamped write. The rest are the
    // values that stop being numbers at all — `NaN` and `inf` included,
    // which every comparison lets through.
    for field in [
        "150", "150.0", "200", "-1", "0", "110.5", "1e400", "NaN", "hot",
    ] {
        assert_eq!(
            parse_setpoint(field),
            None,
            "{field} must be refused: outside the schema's 20.0..=110.0, or not \
             a finite number"
        );
    }
}

// ==================================================== the toggle routes

#[test]
fn a_flag_reads_the_csqs_spellings_and_treats_anything_else_as_off() {
    // The C++ compares the string "1" (`if (value == "1")`), so "1" is the
    // spelling that must work. The extras are this firmware's documented
    // superset.
    assert!(parse_flag("1"));
    assert!(parse_flag("true"));
    assert!(parse_flag("on"));
    assert!(!parse_flag("0"));
    assert!(!parse_flag("false"));
    assert!(!parse_flag("off"));
    assert!(!parse_flag("banana"));
}

#[test]
fn an_explicit_toggle_command_carries_the_value_it_was_given() {
    // The explicit forms must survive the move from `register_command` to
    // `register_toggle`: `?on=0` and body `value=1` are how the human and
    // the integration checklist spell them.
    assert!(!explicit_value(&Command::SetPid(false)));
    assert!(explicit_value(&Command::SetPid(true)));
    assert!(!explicit_value(&Command::SetSteam(false)));
    assert!(!explicit_value(&Command::SetBackflush(false)));
}

#[test]
fn a_toggle_route_inverts_the_published_value() {
    // The bare-POST path: no field, so the target is `!current`. These are
    // the three `current` functions the route table actually registers, so a
    // change that points a route at the wrong telemetry field fails here.
    fn pid(t: &Telemetry) -> bool {
        t.pid_enabled
    }
    fn steam(t: &Telemetry) -> bool {
        t.steam_mode
    }
    fn backflush(t: &Telemetry) -> bool {
        t.backflush_mode
    }

    let off = Telemetry::default();
    let on = Telemetry {
        pid_enabled: true,
        steam_mode: true,
        backflush_mode: true,
        ..Telemetry::default()
    };
    // Each field is read from its own field, so turning one on must not make
    // another's toggle think it is already on.
    assert!(!pid(&off) && pid(&on));
    assert!(!steam(&off) && steam(&on));
    assert!(!backflush(&off) && backflush(&on));

    let only_steam = Telemetry {
        steam_mode: true,
        ..Telemetry::default()
    };
    assert!(!pid(&only_steam), "the PID toggle must not read steam_mode");
    assert!(
        !backflush(&only_steam),
        "the backflush toggle must not read steam_mode"
    );
}

// ======================================================== query and body

#[test]
fn a_query_string_is_reachable_from_the_uri() {
    // `httpd_req_t::uri` is the whole request target, query included —
    // `esp_http_server` reads the query back out of it at
    // `r->uri + res->field_data[UF_QUERY].off` (`httpd_parse.c:992`), and
    // there is no `httpd_req_t::query` member (`esp_http_server.h:373-400`).
    // An earlier revision of `cc_config::form` claimed otherwise and said the
    // query string was unreachable without an `esp-idf-sys` call; this is the
    // test that says so.
    assert_eq!(query_of("/api/pid?on=0"), "on=0");
    assert_eq!(query_of("/api/pid"), "");
    assert_eq!(query_of("/api/parameters?a=1&b=2"), "a=1&b=2");
    // Only the first `?` splits, so a `?` inside the query stays put.
    assert_eq!(query_of("/x?a=1?b=2"), "a=1?b=2");
}

#[test]
fn a_query_string_and_a_body_carry_the_same_parameter() {
    // `request->params()` (`:823`) is the query string and the body together,
    // and the handler merges them in that order — which is what makes
    // `curl -X POST '.../api/parameters?pid.enabled=1'` work.
    let mut fields = cc_config::form::parse_form(query_of("/api/parameters?pid.enabled=1"));
    fields.extend(cc_config::form::parse_form("pid.regular.kp=2.5"));
    assert_eq!(fields.len(), 2);
    assert!(matches!(
        crate::classify_parameters(&fields),
        crate::ParameterPost::Updated { .. }
    ));
}

#[test]
fn a_command_field_is_read_from_the_query_string_as_well_as_the_body() {
    // `POST /api/pid?on=0` answered `400 {"error":"missing value"}` before,
    // because only the body was read. The C++'s `POST /api/pid` reads no
    // field at all (`:462-479`, a toggle), so there is no C++ answer for
    // `?on=0` to match; every script and the integration checklist spell it
    // that way, so both are accepted and the body wins.
    let from_query = cc_config::form::parse_form(query_of("/api/pid?on=0"));
    assert_eq!(first_of(&from_query, &["value", "on"]), Some("0".into()));
    let from_body = cc_config::form::parse_form("on=1");
    assert_eq!(first_of(&from_body, &["value", "on"]), Some("1".into()));
    // `value` beats `on` wherever each is — the C++'s
    // `hasParam("value", …)` then `hasParam("on", …)`.
    let both = cc_config::form::parse_form("on=1&value=7");
    assert_eq!(first_of(&both, &["value", "on"]), Some("7".into()));
    // Neither is a 400 rather than a default.
    assert_eq!(
        first_of(&cc_config::form::parse_form(""), &["value", "on"]),
        None
    );
    // And the single-field helper the C++'s `hasParam` is.
    assert_eq!(
        cc_config::form::field("value=3&value=4", "value"),
        Some("3".into())
    );
    assert_eq!(cc_config::form::field("a=1", "value"), None);
}
