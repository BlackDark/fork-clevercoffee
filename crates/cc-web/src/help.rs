//! The two bodies `cc-hal-esp32` cannot build without a socket, as pure
//! functions: `GET /api/parameter-help` and the server-wide `404`.
//!
//! # Why these two and nothing else
//!
//! Both are wrong in the same way: **the status line does not match the body.** An
//! error object arrived with `200`, so every third-party client read a failure as a
//! success; a `/api/*` path that matched no route got a plain-text `404`, so a
//! client that parses JSON got a parse error instead of a diagnosable answer.
//! Neither is a socket question.
//!
//! What is left in the HAL is the part that *is* a socket question:
//! `cc_hal_esp32::web` reads `?param=` off the request and writes the bytes, and
//! `cc_hal_esp32::web_async` registers the 404 through
//! `httpd_register_err_handler`, which `esp-idf-svc` 0.53.0 does not wrap.
//!
//! # Status codes, and why each one
//!
//! * **`200`** — the name named a parameter. The body is
//!   `{"name":…,"helpText":…}`.
//! * **`404`** — the name named no parameter. Not `200`, because "this firmware
//!   has no such parameter" is a fact about the request, not a successful lookup.
//! * **`422`** — there was no `param` at all, and not `400`: `422 Unprocessable
//!   Content` names exactly this, a request missing a required query key.
//!
//! A `404` that arrives *with* the right body is still an improvement over an error
//! object that arrives with a success, so the codes are not cosmetic here.

use alloc::format;
use alloc::string::String;

use cc_config::schema::SCHEMA;

/// `GET /api/parameter-help?param=<name>` — the status code and the body.
///
/// In order: absent `param` → `422`; a name the schema does not carry → `404`;
/// otherwise that parameter's own help text with `200`.
///
/// The help text is [`cc_config::schema::ParamSpec::help`], so there is no second
/// copy of the strings and no way for the two to disagree.
///
/// An unknown `name` is answered from the **schema**, not from the request: the
/// echoed `name` is the canonical key, not the bytes the client sent, so the body
/// is a valid JSON string by construction. No escaping helper exists here because
/// every value interpolated is one of the schema's own literals — a schema test
/// asserts that property.
#[must_use]
pub fn parameter_help(name: Option<&str>) -> (u16, String) {
    let Some(name) = name else {
        return (422, String::from("{\"error\":\"parameter is missing\"}"));
    };
    let Some(spec) = SCHEMA.iter().find(|spec| spec.key == name) else {
        return (404, String::from("{\"error\":\"parameter not found\"}"));
    };
    (
        200,
        format!(
            "{{\"name\":\"{}\",\"helpText\":\"{}\"}}",
            spec.key, spec.help
        ),
    )
}

/// The JSON `404` for a `/api/` path that matched no route.
///
/// `{"error": "…"}` **with a space after the colon**, where this crate's
/// [`crate::payload::error_body`] emits none. Clients byte-compare this body, so it
/// is kept exactly as it is rather than normalised through the builder.
///
/// Non-`/api/` paths keep ESP-IDF's own plain-text `404`.
#[must_use]
pub fn not_found_json() -> &'static str {
    "{\"error\": \"API endpoint not found\"}"
}

/// Whether `path` gets the JSON `404` or ESP-IDF's plain-text one.
///
/// The test is `/api/` **with the trailing slash**, so a bare `/api` gets the
/// plain-text answer. A second definition of "is this an API path" would be a new
/// way to be inconsistent, so the prefix test is the only one.
///
/// `/ui*` is served by its own wildcard handler and so never reaches a `404`,
/// which is why a UI-index fallback has no counterpart here: that branch is
/// unreachable once the wildcard exists.
#[must_use]
pub fn wants_json_not_found(path: &str) -> bool {
    path.starts_with("/api/")
}

/// Which of ESP-IDF's two "no handler" answers an unmatched request deserves.
///
/// This exists because of a divergence found on the bench on 2026-10-05. An unknown
/// `/api/` path should get a JSON `404`. This firmware registers the CORS preflight
/// answer on the URI wildcard `/api*` (the stand-in for a CORS middleware, which is
/// not a URI handler and so shadows nothing). To ESP-IDF a wildcard **is** a URI
/// handler, so `GET /api/nope` matches it with the wrong method and ESP-IDF answers
/// its own `405 text/html` — the registered `404` handler never runs.
///
/// `known` answers "is this a route the server serves, for some method", which
/// separates the two cases ESP-IDF conflates into one `405`. It is
/// [`ROUTE_PATHS`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unmatched {
    /// A mistyped URL: nothing is registered for it, answered with
    /// [`not_found_json`].
    Api,
    /// A real route asked for with a method it does not have. Honestly a `405`;
    /// only the body was wrong.
    Served,
    /// A path outside `/api/`. ESP-IDF's own plain-text `404`.
    NotApi,
}

/// Every URI this firmware serves, for the raw unmatched-request handler.
///
/// A compile-time list, because the handler that needs it is an `extern "C"`
/// function ESP-IDF dispatches itself: it cannot capture, so a runtime-built table
/// would have to be stashed in a mutable `static`. `web.rs` builds its own
/// `routes()` at runtime, and a device case asserts the two agree — so the
/// duplication is test-pinned rather than free.
///
/// The two wildcards are in here deliberately: they *are* URI handlers to ESP-IDF,
/// and `/api*` is exactly why an unknown path has to be distinguished from a
/// wrong-method one.
pub const ROUTE_PATHS: &[&str] = &[
    "/api/status",
    "/api/health",
    "/api/temperatures",
    "/api/history",
    "/api/nvs-debug",
    "/api/parameter-help",
    "/api/config",
    "/api/config/download",
    "/api/config/upload",
    "/api/parameters",
    "/api*",
    "/api/setpoint",
    "/api/steam",
    "/api/pid",
    "/api/backflush",
    "/api/sleep",
    "/api/wake",
    "/api/scale/tare",
    "/api/scale/calibration",
    "/api/maintenance/reset-backflush-counter",
    "/api/wifi-reset",
    "/api/factory-reset",
    "/api/restart",
    "/api/ota/status",
    "/api/ota/firmware",
    "/api/ota/filesystem",
    "/api/ota/url",
    "/events",
    "/",
    "/ui*",
];

/// Decide which answer an unmatched request gets.
///
/// JSON-vs-text is decided on the `/api/` prefix; the status is split out further
/// so the preflight wildcard's shadowing is visible to the caller instead of
/// silently turning every unknown API path into a `405`.
#[must_use]
pub fn unmatched(path: &str) -> Unmatched {
    if !wants_json_not_found(path) {
        return Unmatched::NotApi;
    }
    // A wildcard URI answers anything under it, so `/api/anything` is "served"
    // for the method it does not have — and is an unknown path for every other.
    if ROUTE_PATHS.contains(&path) {
        Unmatched::Served
    } else {
        Unmatched::Api
    }
}

#[cfg(test)]
mod tests;
