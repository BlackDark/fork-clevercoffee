//! The two bodies `cc-hal-esp32` cannot build without a socket, as pure
//! functions: `GET /api/parameter-help` and the server-wide `404`.
//!
//! # Why these two and nothing else
//!
//! Both are finding 3.4 and 3.8 of
//! [`32-findings-2026-10-03.md`](../../../docs/rust-migration/32-findings-2026-10-03.md),
//! and both are wrong in the same way: **the status line does not match the
//! body.** `/api/parameter-help` returned an error object with `200`, so every
//! third-party client read a failure as a success; a `/api/*` path that matched
//! no route got ESP-IDF's plain-text `404`, so a client that parses JSON got a
//! parse error instead of a diagnosable answer. Neither is a socket question.
//!
//! What is left in the HAL is the part that *is* a socket question:
//! `cc_hal_esp32::web` reads `?param=` off the request and writes the bytes, and
//! `cc_hal_esp32::web_async` registers the 404 through `httpd_register_err_handler`,
//! which `esp-idf-svc` 0.53.0 does not wrap.
//!
//! # Status codes, and why each one
//!
//! Parity with the C++ is the point, so every code here is the C++'s.
//!
//! * **`200`** — the name named a parameter. The body is the C++'s
//!   `{"name":…,"helpText":…}` (`WebServerManager.cpp:598-608`).
//! * **`404`** — the name named no parameter (`findConfigParameter` returned
//!   `nullptr`, `:594-596`). Not `200`, because "this firmware has no such
//!   parameter" is a fact about the request, not a successful lookup.
//! * **`422`** — there was no `param` at all (`:587-590`). The C++'s code and
//!   not `400`: `422` is what the C++ sends, and a request missing a required
//!   query key is precisely what `422 Unprocessable Content` names.
//!
//! The `200` case is the one that was broken. A `404` that arrives *with* the
//! right body is still an improvement over an error object that arrives with a
//! success, so the codes are not cosmetic here.

use alloc::format;
use alloc::string::String;

use cc_config::schema::SCHEMA;

/// `GET /api/parameter-help?param=<name>` — the status code and the body.
///
/// The C++'s handler, in the C++'s order (`WebServerManager.cpp:585-618`):
/// absent `param` → `422`; a name no `ConfigParamDef` carries → `404`;
/// otherwise the parameter's own `helpText` with `200`.
///
/// The help text is [`cc_config::schema::ParamSpec::help`], which is
/// transcribed from the same `Config.h` constructor argument the C++ reads.
/// There is no second copy of the strings and no way for the two to disagree.
///
/// A `name` that is unknown is answered from the **schema**, not from the
/// request: the echoed `name` is the canonical key from `SCHEMA`, not the bytes
/// the client sent, so the body is a valid JSON string by construction. The
/// C++ echoes the request's own `paramName` through `ArduinoJson`, which escapes;
/// this cannot need escaping because the value it interpolates is one of the 98
/// literals in `schema.rs`. That is also why no escaping helper exists here
/// rather than being an oversight — see the schema test that asserts the
/// property.
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
/// The C++'s `handleNotFound` (`WebServerManager.cpp:1006-1027`) answers
/// `ApiResponses::errorResponse("API endpoint not found")` — which is
/// `{"error": "…"}` **with a space after the colon** (`ApiResponses.cpp:21-26`),
/// where this crate's [`crate::payload::error_body`] emits none. The bytes are
/// reproduced rather than normalised, because a client that byte-compares
/// against the C++ is exactly the client this finding is about.
///
/// Non-`/api/` paths keep ESP-IDF's own `404`, which is what the C++ also does
/// for them (`:1023-1024`, `text/plain`).
#[must_use]
pub fn not_found_json() -> &'static str {
    "{\"error\": \"API endpoint not found\"}"
}

/// Whether `path` gets the JSON `404` or ESP-IDF's plain-text one.
///
/// The C++ tests `path.startsWith("/api/")` (`WebServerManager.cpp:1011`) —
/// **with the trailing slash**, so a bare `/api` is a plain-text `404` there
/// and is one here. Matching the C++ on that detail is deliberate: the finding
/// is that this firmware's HTTP surface is not honest, and inventing a second
/// definition of "is this an API path" would be a new way to be wrong.
///
/// `/ui*` is served by its own wildcard handler and so never reaches a `404`
/// here, which is why the C++'s `serveUiIndex` fallback (`:1016-1022`) has no
/// counterpart: that branch is unreachable once the wildcard exists.
#[must_use]
pub fn wants_json_not_found(path: &str) -> bool {
    path.starts_with("/api/")
}

#[cfg(test)]
mod tests;
