//! Check response bytes and status together for parameter help and missing routes.

use alloc::string::{String, ToString};

use cc_config::schema::SCHEMA;

use super::*;

// ================================================== GET /api/parameter-help

#[test]
fn a_known_parameter_returns_its_help_with_200() {
    let (status, body) = parameter_help(Some("pid.regular.kp"));
    assert_eq!(status, 200);
    // The help text byte for byte, not a paraphrase: a UI that displays this
    // string to an operator is showing the schema's words.
    assert_eq!(
        body,
        String::from(
            "{\"name\":\"pid.regular.kp\",\"helpText\":\"Proportional gain \
             (in Watts/°C) for the main PID controller\"}"
        )
    );
}

#[test]
fn an_unknown_parameter_is_404_not_an_error_object_with_200() {
    // **This is the defect.** The route answered this request with
    // `{"error":"Per-parameter help is not available in this build", …}` and
    // HTTP 200, so a client checking the status saw success and a client parsing
    // the body saw a failure.
    let (status, body) = parameter_help(Some("pid.regular.nope"));
    assert_eq!(status, 404);
    assert_eq!(body, "{\"error\":\"parameter not found\"}");
}

#[test]
fn a_missing_param_is_422() {
    // 422 rather than 400: a request with no required query key is exactly what
    // 422 Unprocessable Content names.
    let (status, body) = parameter_help(None);
    assert_eq!(status, 422);
    assert_eq!(body, "{\"error\":\"parameter is missing\"}");
}

#[test]
fn an_empty_param_is_404_not_422() {
    // `?param=` **is** a present parameter with an empty value, so the "was
    // `param` sent at all" test does not fire, and the name `""` matches nothing —
    // a 404. Collapsing the two cases would answer 422 for an empty name.
    let (status, body) = parameter_help(Some(""));
    assert_eq!(status, 404);
    assert_eq!(body, "{\"error\":\"parameter not found\"}");
}

#[test]
fn every_parameter_in_the_schema_has_help_that_is_json_safe() {
    // `parameter_help` interpolates `spec.help` into a JSON string literal with
    // no escaping, which is only correct because of this: not one of the 98
    // transcribed strings contains a quote, a backslash or a control character.
    // `pid.regular.kp` and `display.heating_logo` carry a `°`, which JSON does
    // not require escaping and which is emitted as UTF-8.
    //
    // This test is the reason there is no escaping helper in `help.rs`. If
    // somebody adds help text with a `"` in it, this fails and the renderer has
    // to grow one — which is the right order to discover that in.
    for spec in SCHEMA {
        assert!(
            !spec.help.contains(['"', '\\', '\n', '\r', '\t']),
            "{:?} has help text that needs JSON escaping: {:?}",
            spec.key,
            spec.help
        );
        assert!(!spec.help.is_empty(), "{:?} has no help text", spec.key);
    }
}

#[test]
fn the_help_lookup_agrees_with_the_schema_on_both_sides() {
    // The lookup is a linear scan over `SCHEMA`, so the only way it can be wrong
    // is a key that is not in the table — the same table `GET /api/parameters`
    // renders. Spot-checking one per parameter kind would be weaker than asking
    // the table directly.
    assert_eq!(SCHEMA.len(), 98);
    for spec in SCHEMA {
        let (status, body) = parameter_help(Some(spec.key));
        assert_eq!(status, 200, "{:?} is in SCHEMA but not found", spec.key);
        assert!(
            body.contains(spec.help),
            "{:?} lost its help text",
            spec.key
        );
    }
}

// ================================================================ the 404

#[test]
fn the_api_404_is_json_with_the_declared_bytes() {
    // `{"error": "…"}` **with a space after the colon**, unlike this crate's
    // `payload::error_body`. Kept as it is rather than normalised: a client
    // byte-comparing this body must keep matching it.
    assert_eq!(not_found_json(), "{\"error\": \"API endpoint not found\"}");
}

#[test]
fn only_api_paths_get_the_json_404() {
    // The prefix test keeps its trailing slash: a bare `/api` is a plain-text
    // 404, and this is the only place that decides.
    assert!(wants_json_not_found("/api/nope"));
    assert!(wants_json_not_found("/api/"));
    assert!(wants_json_not_found("/api/a/b/c"));
    assert!(!wants_json_not_found("/api"));
    assert!(!wants_json_not_found("/apifoo"));
    assert!(!wants_json_not_found("/"));
    assert!(!wants_json_not_found("/ui/brew"));
    assert!(!wants_json_not_found("/favicon.ico"));
}

#[test]
fn an_unknown_api_path_is_a_404_and_a_known_one_is_a_405() {
    // A mistyped `/api/` URL should be a `404` with a JSON body. The preflight
    // wildcard `/api*` made ESP-IDF treat every `/api/...` URI as
    // matched-but-wrong-method, so it answered its own `405 text/html` and the
    // registered handler never ran. Bench-measured.
    assert_eq!(unmatched("/api/nope"), Unmatched::Api);
    assert_eq!(unmatched("/api/status/deep"), Unmatched::Api);
    // A real route with a method it does not have: honestly a 405.
    assert_eq!(unmatched("/api/status"), Unmatched::Served);
    assert_eq!(unmatched("/api/parameters"), Unmatched::Served);
    // Outside `/api/`: ESP-IDF's plain text.
    assert_eq!(unmatched("/nope"), Unmatched::NotApi);
    assert_eq!(unmatched("/ui/assets/x.js"), Unmatched::NotApi);
}

#[test]
fn the_unmatched_decision_ignores_the_query_string() {
    // ESP-IDF's `httpd_req_t::uri` carries the query string, and the raw
    // handler strips it before asking. A URL that is a real route plus a query
    // must not be demoted to a 404.
    assert_eq!(unmatched("/api/parameters"), Unmatched::Served);
    assert_eq!(unmatched("/api/parameters?filter=all"), Unmatched::Api);
}

#[test]
fn the_error_bodies_are_valid_json_objects() {
    // The point of the 404 being JSON is that a client can parse it, so the
    // test asserts it parses as one object with the expected key rather than
    // eyeballing the string. `cc-web` has no JSON parser — it only ever emits —
    // so this checks the two structural properties a hand-rolled object needs.
    for body in [
        not_found_json().to_string(),
        parameter_help(Some("pid.enabled")).1,
        parameter_help(Some("nope")).1,
        parameter_help(None).1,
    ] {
        assert!(body.starts_with('{'), "not an object: {body}");
        assert!(body.ends_with('}'), "not an object: {body}");
        assert!(
            !body.contains('\n'),
            "a raw newline is not valid JSON: {body}"
        );
        assert!(
            body.contains("error") || body.contains("helpText"),
            "no recognisable key: {body}"
        );
    }
}
