//! Turning a request's fields into a [`Command`]: [`parse_setpoint`],
//! [`parse_flag`], [`first_of`], [`query_of`] and [`explicit_value`].
//!
//! The five small pure functions that stood between an `esp_idf_svc`
//! `Request` and the control task's queue. Each was private to `web.rs` and each
//! was reachable only from a device test.

use alloc::string::String;

use crate::telemetry::Command;

/// The `POST /api/setpoint` field, parsed through the **schema's** bound.
///
/// # Why this is not `0.0..=150.0`
///
/// That was the C++ handler's own filter (`WebServerManager.cpp:393-395`) and
/// this port reproduced it verbatim — which is how `brew.setpoint = 150` could
/// be written and persisted. The C++ does not have the resulting bug only
/// because the *next* line, `Config::brewSetpoint.set(newSetpoint)`
/// (`WebServerManager.cpp:400`), range-checks 20..=110 (`Config.h:795-802`,
/// `defaults.h:81-82`) and returns false. This port has no such second line on
/// this route: it persisted through `persist_setpoint`
/// (`cc-firmware/src/main.rs`), which wrote whatever it was handed. So the
/// bound that actually protects the value is the schema's, and the only way to
/// be sure of using it is to ask for it — [`cc_config::assign::parse`] is that
/// ask, and it cannot drift from `ParamSpec` the way a repeated literal does.
///
/// A 150 °C setpoint is not a cosmetic defect: `safety.emergency_temp` defaults
/// to 150 and S1's test is *strictly greater*
/// (`cc_safety::SafetyState::is_over_threshold`), so it drives the boiler to
/// the emergency threshold and holds it there with a debounce that can never
/// trip. `cc_safety::validate_config` now refuses that pair on every write
/// path; this is the half that makes the HTTP contract honest about it.
///
/// # Reject, do not clamp
///
/// The two candidates were a `400` and a silent clamp to 110. Clamping was
/// rejected because the caller would be told `202 {"accepted":true}` for a
/// request it did not make: a UI slider stuck at 110 looks like a stuck UI,
/// not a refused write, and the operator has no way to learn the machine is not
/// holding what they asked for. `400` is also what the C++ already answers for
/// a value it will not take (`:404-406`), what `POST /api/parameters` answers
/// for the same key through the same [`cc_config::assign::parse`]
/// (`WebServerManager.cpp:843-859` → this port's `400`), and what
/// `register_command` already sends when this returns `None`.
///
/// # What this costs, deliberately
///
/// The C++ accepts `0.0..=150.0` here and applies `0..20` to the **running**
/// machine — `setProcessSetpoint` (`:398`) runs before the range-checked
/// `set` — while persisting nothing. So `?value=5` is a `202` on the C++ and a
/// `400` here, and `?value=150` is a `200` on the C++ that leaves a 150 °C
/// process setpoint in RAM. Narrowing the accepted range is a divergence from
/// the C++ and is recorded in
/// [`intentional-diffs.md`](../../docs/rust-migration/intentional-diffs.md).
/// What is *not* given up: a fractional setpoint still truncates rather than
/// being refused, because `setProcessSetpoint` takes a `double` in the C++ and
/// a truncated integer is not a safety question.
///
/// # Why an `i32`
///
/// `Command` is `Copy` and holds no `String` (04 §3.2), and a whole-degree
/// setpoint is what the schema's range is quoted in. 20..=110 fits with room
/// to spare.
#[must_use]
pub fn parse_setpoint(value: &str) -> Option<Command> {
    match cc_config::assign::parse("brew.setpoint", value) {
        Ok(cc_config::json::LiveValue::Float(celsius)) => {
            // **Not truncated.** This cast to `i32` and the comment above it
            // claimed "the C++ truncates the same way into its process setpoint".
            // It does not: `setProcessSetpoint(newSetpoint)` takes the `double`
            // straight (`WebServerManager.cpp:394-396`), so `93.5` is 93.5 in the
            // C++ and 93 here. Measured on a bench ESP32: `?value=93.5`,
            // `?value=80.5` and `?value=91.2` were all accepted and all landed
            // on the truncated integer, which is what "the setpoint control does
            // nothing" looks like from the UI. `brew.setpoint` is a float
            // parameter and the machine carries an `f64` setpoint end to end;
            // the only thing that was an integer was this cast.
            Some(Command::SetSetpoint(celsius))
        }
        // Every other outcome — a non-number, `NaN`, `inf`, or a value outside
        // the schema's range — is a `None`, and `register_command` answers
        // `400`. `Ok` on a non-`Float` is unreachable (the key is registered as
        // a float) and is refused rather than unwrapped, so a future schema
        // retyping cannot turn into a panic on the httpd task.
        _ => None,
    }
}

/// A boolean field, C++-style: what `AsyncWebServerRequest::getParam(...)->value()`
/// means when it is compared against `1`.
///
/// The C++ compares the **string** `"1"` (`if (value == "1")` throughout
/// `WebServerManager.cpp`), so `"true"` and `"on"` are not C++ spellings. They
/// are accepted anyway because this firmware already documented them
/// (`register_command`'s comment above) and scripts use them; accepting a
/// superset cannot break a C++-shaped caller.
#[must_use]
pub fn parse_flag(value: &str) -> bool {
    matches!(value, "1" | "true" | "on" | "yes")
}

/// The value an explicit `SetPid`/`SetSteam`/`SetBackflush` carries.
#[must_use]
pub fn explicit_value(command: &Command) -> bool {
    match command {
        Command::SetPid(on) | Command::SetSteam(on) | Command::SetBackflush(on) => *on,
        _ => true,
    }
}

/// The first value any of `names` has, in **name** order rather than field order.
///
/// `hasParam(name, true).orElse(hasParam(other, true))` — the C++'s
/// `hasParam("value", …)` then `hasParam("on", …)` (`:392`), so `value` anywhere
/// in the request beats `on` anywhere in it. Scanning the fields once and taking
/// whichever key matched first would flip that for a request carrying both.
#[must_use]
pub fn first_of(fields: &[cc_config::form::Field], names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    })
}

/// The query string of `uri`, or `""`.
///
/// `EspHttpConnection::uri()` (`esp-idf-svc` `src/http/server.rs:949-955`)
/// returns `httpd_req_t::uri`, and that field is the **whole** request target
/// including the `?…` — `esp_http_server` reads the query back out of it with
/// `r->uri + res->field_data[UF_QUERY].off` (`httpd_parse.c:992`) rather than
/// from a member of its own. So the query string needs no `esp-idf-sys` call to
/// reach, which is what `cc_config::form`'s module documentation used to say the
/// opposite of; the split is at the first `?` and everything after it.
#[must_use]
pub fn query_of(uri: &str) -> &str {
    uri.split_once('?').map_or("", |(_, query)| query)
}

#[cfg(test)]
mod tests;
