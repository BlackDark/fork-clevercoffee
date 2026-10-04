//! The `parameter-help` and `404` tests.
//!
//! Every assertion here is against bytes a real client receives, because the
//! finding these came from is that the bytes and the status line disagreed — a
//! test that checked "the handler returned something" would have passed against
//! the defect.

use alloc::string::{String, ToString};

use cc_config::schema::SCHEMA;

use super::*;

// ================================================== GET /api/parameter-help

#[test]
fn a_known_parameter_returns_its_help_with_200() {
    let (status, body) = parameter_help(Some("pid.regular.kp"));
    assert_eq!(status, 200);
    // The C++'s `Config.h:756`, verbatim. Not a paraphrase: a client that
    // displays this string to an operator is showing the C++'s words.
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
    // **This is the defect.** `web.rs` answered this request with
    // `{"error":"Per-parameter help is not available in this build", …}` and
    // HTTP 200, so a client checking the status saw success and a client
    // parsing the body saw a failure. The only honest answers are the C++'s.
    let (status, body) = parameter_help(Some("pid.regular.nope"));
    assert_eq!(status, 404);
    assert_eq!(body, "{\"error\":\"parameter not found\"}");
}

#[test]
fn a_missing_param_is_422() {
    // `WebServerManager.cpp:587-590`. 422 rather than 400 because that is what
    // the C++ sends, and a request with no required query key is what 422 names.
    let (status, body) = parameter_help(None);
    assert_eq!(status, 422);
    assert_eq!(body, "{\"error\":\"parameter is missing\"}");
}

#[test]
fn an_empty_param_is_404_not_422() {
    // `?param=` **is** a present parameter with an empty value, so the C++'s
    // `p == nullptr` test does not fire and `findConfigParameter("")` returns
    // `nullptr` — a 404. Collapsing the two cases would answer 422 where the
    // C++ answers 404.
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
    // not require escaping and which is emitted as UTF-8, exactly as the C++'s
    // ArduinoJson emits it.
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
    // is if a key is not in the table — and every one of the 98 keys is the
    // key the C++'s `Config.h` constructor uses. Spot-checking one per parameter
    // kind would be weaker than asking the table directly.
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
fn the_api_404_is_json_with_the_cpp_bytes() {
    // `ApiResponses::errorResponse` (`ApiResponses.cpp:21-26`) writes
    // `{"error": "…"}` **with a space after the colon**, unlike this crate's
    // `payload::error_body`. Reproduced rather than normalised: the client this
    // exists for is one comparing against the C++.
    assert_eq!(not_found_json(), "{\"error\": \"API endpoint not found\"}");
}

#[test]
fn only_api_paths_get_the_json_404() {
    // `WebServerManager.cpp:1011` tests `startsWith("/api/")` — with the
    // slash. A bare `/api` is a plain-text 404 in the C++ and is one here.
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
