//! Turning a request's fields into a [`Command`]: [`parse_setpoint`],
//! [`parse_flag`], [`first_of`], [`query_of`] and [`explicit_value`].
//!
//! The five small pure functions that stand between an `esp_idf_svc` `Request`
//! and the control task's queue.

use alloc::string::String;

use crate::telemetry::Command;

/// The `POST /api/setpoint` field, parsed through the **schema's** bound.
///
/// # Why the bound is the schema's
///
/// A permissive `0.0..=150.0` filter lets `brew.setpoint = 150` through, and that
/// is not a cosmetic defect: `safety.emergency_temp` defaults to 150 and S1's test
/// is *strictly greater* (`cc_safety::SafetyState::is_over_threshold`), so a
/// 150 °C setpoint drives the boiler to the emergency threshold and holds it there
/// against a debounce that can never trip. `cc_safety::validate_config` refuses
/// that pair on every write path; this is the half that makes the HTTP contract
/// honest about it.
///
/// Nothing else on this route restricts the value, so the only bound that protects
/// it is the schema's. The only way to be sure of using that bound is to ask for
/// it: [`cc_config::assign::parse`] is the ask, and it cannot drift from
/// `ParamSpec` the way a repeated literal does. The divergence from a wider filter
/// is recorded as §26 of `docs/history/divergences.md`.
///
/// # Reject, do not clamp
///
/// The two candidates were a `400` and a silent clamp to 110. Clamping was
/// rejected because the caller would be told `202 {"accepted":true}` for a request
/// it did not make: a UI slider stuck at 110 looks like a stuck UI, not a refused
/// write, and the operator has no way to learn the machine is not holding what
/// they asked for. `400` is also what `POST /api/parameters` answers for the same
/// key through the same [`cc_config::assign::parse`], and what
/// `register_command` already sends when this returns `None`.
///
/// # Why a fractional setpoint is accepted
///
/// `brew.setpoint` is a float parameter and the machine carries an `f64` setpoint
/// from here to the PID, so `93.5` stays `93.5`. Truncating to a whole degree
/// would put the accepted values off the range the schema quotes, and would make
/// the setpoint appear not to respond for any fractional value.
#[must_use]
pub fn parse_setpoint(value: &str) -> Option<Command> {
    match cc_config::assign::parse("brew.setpoint", value) {
        Ok(cc_config::json::LiveValue::Float(celsius)) => Some(Command::SetSetpoint(celsius)),
        // Every other outcome — a non-number, `NaN`, `inf`, or a value outside
        // the schema's range — is a `None`, and `register_command` answers
        // `400`. `Ok` on a non-`Float` is unreachable (the key is registered as
        // a float) and is refused rather than unwrapped, so a future schema
        // retyping cannot turn into a panic on the httpd task.
        _ => None,
    }
}

/// A boolean field: `"1"` is true, and so are `"true"`, `"on"` and `"yes"`.
///
/// The documented spelling is `"1"`. The others are accepted because scripts use
/// them, and a caller that sends only `"1"` cannot be affected by the wider set.
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
/// `value` anywhere in the request beats `on` anywhere in it. Scanning the fields
/// once and taking whichever key matched first would flip that for a request
/// carrying both.
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
/// `r->uri + res->field_data[UF_QUERY].off` rather than from a member of its own.
/// So the query string needs no `esp-idf-sys` call to reach: the split is at the
/// first `?`, and everything after it is the query.
#[must_use]
pub fn query_of(uri: &str) -> &str {
    uri.split_once('?').map_or("", |(_, query)| query)
}

#[cfg(test)]
mod tests;
