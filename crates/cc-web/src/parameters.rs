//! [`classify_parameters`] and [`ParameterPost`]: what one `POST /api/parameters`
//! means, decided without writing anything.
//!
//! The handler has to answer `200` or `400` **synchronously**, and the `Config`
//! is the control task's, so the verdict cannot be a function of the store. This
//! is the verdict as a value, which is why it can be tested on a host and why
//! `ParameterPost::response` is a method rather than a `match` in the middle of
//! a socket handler.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// What one `POST /api/parameters` resolved to, before anything is written.
///
/// The C++'s `hasErrors` / `hasUpdates` pair (`WebServerManager.cpp:826-827`),
/// kept as a value so the handler's decision — which of three response bodies to
/// send, and whether to emit a command at all — is a function that can be tested
/// without a socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterPost {
    /// At least one parameter was rejected. The C++ answers `400` and does not
    /// report which, but the pairs that *were* accepted are still written:
    /// `apply` walks the request in order and only collects the failures.
    Rejected {
        /// The pairs that passed. Empty when all failed.
        accepted: Vec<(String, String)>,
        /// One line per rejection, for the log.
        reasons: Vec<String>,
    },
    /// Everything was written.
    Updated {
        /// Every pair in the request, validated.
        accepted: Vec<(String, String)>,
    },
    /// Nothing in the request named a parameter with a value, so nothing was
    /// written. The C++'s `"No parameters updated"` (`WebServerManager.cpp:877`).
    Nothing,
}

impl ParameterPost {
    /// The status code and the body, verbatim from the C++.
    ///
    /// * rejected → `400 {"error":"Some parameter updates failed"}` (`:868`)
    /// * updated → `200 {"success":true,"message":"Parameters updated and
    ///   saved"}` (`:874-875`)
    /// * nothing → `200 {"success":true,"message":"No parameters updated"}`
    ///   (`:877`)
    #[must_use]
    pub fn response(&self) -> (u16, &'static str) {
        match self {
            Self::Rejected { .. } => (400, "{\"error\":\"Some parameter updates failed\"}"),
            Self::Updated { .. } => (
                200,
                "{\"success\":true,\"message\":\"Parameters updated and saved\"}",
            ),
            Self::Nothing => (
                200,
                "{\"success\":true,\"message\":\"No parameters updated\"}",
            ),
        }
    }

    /// The accepted pairs that will not affect the running machine until a
    /// reboot.
    ///
    /// Most parameters are read from `Config` on every tick, so a write takes
    /// effect immediately — the control task pushes the ones the reducer caches
    /// (`pid.enabled`, `brew.setpoint`) into `cc_machine::Machine` explicitly.
    /// A few are read **once**, at bring-up: which switches exist
    /// (`hardware.switches.*.enabled` → `SwitchBank::new`), whether a scale is
    /// fitted, whether the tank float is fitted. Those cannot change under a
    /// running machine, and the only honest thing is to say so.
    ///
    /// The C++ does not say it, because in the C++ a switch's enable flag is read
    /// by `SystemInitializer` at boot too — the same limitation, reported as
    /// silence. This firmware names the keys, which is the difference between
    /// "my setting vanished" and a diagnosis.
    #[must_use]
    pub fn reboot_required(&self) -> Vec<&str> {
        let pairs = match self {
            Self::Rejected { accepted, .. } | Self::Updated { accepted } => accepted,
            Self::Nothing => return Vec::new(),
        };
        pairs
            .iter()
            .map(|(key, _)| key.as_str())
            .filter(|key| needs_reboot(key))
            .collect()
    }

    /// The pairs to hand to the control task, if any.
    ///
    /// Non-empty for both outcomes that wrote something: a `400` that rejected
    /// one of six parameters still applies the other five, and dropping them
    /// would make the response and the machine disagree.
    #[must_use]
    pub fn into_pairs(self) -> Vec<(String, String)> {
        match self {
            Self::Rejected { accepted, .. } | Self::Updated { accepted } => accepted,
            Self::Nothing => Vec::new(),
        }
    }
}

/// Whether a parameter is read once at bring-up, so a write needs a reboot.
///
/// The rule is "does `SwitchBank::new` / the sensor bring-up read it", not a
/// guess: `hardware.switches.*.enabled` decides whether `poll` emits an edge at
/// all (`switches.rs:220-241` prints "the reducer will IGNORE this switch"), and
/// the switch bank is constructed once in `bring_up` and never rebuilt. The two
/// sensor flags are the same shape — `hardware.sensors.watertank.enabled` becomes
/// `SwitchBank::tank_fitted` (`switches.rs:196`) and
/// `hardware.sensors.scale.enabled` decides whether a sampler exists at all
/// (`main.rs:1362`).
///
/// `system.auth.*` is the same kind of thing for a different reason, and it is
/// the one the C++ cannot report: the C++ installs its authentication
/// middleware in `setupMiddleware`, which runs once from
/// `WebServerManager::initialize` (`WebServerManager.cpp:272-296`), so
/// enabling `system.auth.enabled` protects **nothing** until the machine
/// restarts — an operator who sets it, sees `200`, and is still serving an open
/// API. [`crate::Auth::from_config`] reproduces the C++'s boot-time decision, so
/// this list has to say so, and `requiresRebootKeys` in the
/// `POST /api/parameters` answer is where it says it.
///
/// Everything else is read from `Config` per tick or per event, so a write is
/// live. That includes `pid.enabled`, which the control task pushes into the
/// machine explicitly — see the `POST /api/parameters` drain in `main.rs`.
fn needs_reboot(key: &str) -> bool {
    key.starts_with("hardware.switches.")
        || key.starts_with("hardware.sensors.watertank.enabled")
        || key == "hardware.sensors.scale.enabled"
        || key.starts_with("system.auth.")
        // **The probe type decides which driver is constructed**, so it is read
        // once at boot like every other `hardware.*` setting. It was missing
        // from this list, which is how a saved `TSIC_306` on a `DS18B20` board
        // looked applied while the machine carried on reading the other bus —
        // the operator changes it, the API says `success`, and nothing happens
        // until a reboot. The C++ has the same property (it builds
        // `TempSensorDallas` or `TempSensorTSIC` in `SystemInitializer`) and its
        // UI has to say so by hand; here the answer is the list.
        || key == "hardware.sensors.temperature.type"
}

/// The most pairs one `POST /api/parameters` may carry.
///
/// The C++ has no bound: it iterates `request->params()` and a client can send
/// ten thousand. Here the pairs are staged in a heap `Vec` on a 320 KB machine
/// and each one is a `String` pair, so an unbounded request is a
/// denial-of-service with one `curl`. `cc_hal_esp32::task::STAGED_PARAMETER_DEPTH`
/// requests times this is 256 pairs — eight times the 98 the firmware registers,
/// so no legitimate request is refused, and the body is bounded at 4 KB by
/// `drain_body` long before the count is reached.
pub const MAX_PARAMETER_PAIRS: usize = 64;

/// The most bytes one `POST /api/parameters` body may be.
///
/// 1024, and the reason it is not the 256 every other body gets is that this
/// body is a *list*: `hardware.sensors.watertank.keep_heater_on_empty=1` is 51
/// characters, so 256 fits five of them. A settings form that cannot be submitted
/// is the reason an operator ends up using `curl` sixteen times, so this is
/// generous — twenty parameters, or the whole of a small machine's switches — and
/// still three orders of magnitude below the heap it could otherwise take. The
/// query string has its own, smaller, bound that this firmware does not set:
/// `CONFIG_HTTPD_MAX_URI_LEN`, 512 bytes by default
/// (`esp_http_server.h:377`).
pub const MAX_PARAMETER_BODY_BYTES: usize = 1024;

/// The most bytes one `POST /api/config/upload` body may be.
///
/// 16 KB — `MAX_CONFIG_UPLOAD_SIZE` at `WebServerManager.cpp:48`, which the
/// C++ passes to `jsonHandler.setMaxContentLength` (`:762`). That is the
/// **transport** limit; the *policy* limit is `cc_config::MAX_CONFIG_BYTES`
/// (8 KB), which is what a document over it is refused with. Two limits on
/// purpose: the C++'s is the largest request it will accept off the wire, and the
/// Rust one is the largest configuration the store can hold, so a body
/// between the two gets the clearer of the two answers rather than being cut
/// off.
pub const MAX_CONFIG_UPLOAD_BYTES: usize = 16 * 1024;

/// Decide what a `POST /api/parameters` means, without writing anything.
///
/// The C++'s loop over `request->params()` (`WebServerManager.cpp:829-865`),
/// with the same two rules: a field with no name or no value is **skipped**
/// rather than rejected (`:830`), and a rejected value does not undo the
/// accepted ones.
///
/// The pairs are validated here because the handler has to answer `200` or `400`
/// **synchronously**, and the `Config` is the control task's. The control task
/// then applies the pairs with `cc_config::assign::apply`, which validates them
/// again — through the same [`cc_config::assign::parse`], so the two verdicts
/// cannot disagree, and the second is an idempotent re-application rather than a
/// second rule.
#[must_use]
pub fn classify_parameters(pairs: &[cc_config::form::Field]) -> ParameterPost {
    let mut accepted = Vec::new();
    let mut reasons = Vec::new();
    for (key, raw) in pairs {
        // `:830` — `p->name().length() > 0 && p->value().length() > 0`. A
        // valueless field is "not mentioned", which is also why a text parameter
        // cannot be set to the empty string over HTTP.
        if key.is_empty() || raw.is_empty() {
            continue;
        }
        match cc_config::assign::parse(key, raw) {
            Ok(_) => accepted.push((key.clone(), raw.clone())),
            Err(err) => reasons.push(format!("{key}: {err}")),
        }
    }
    if !reasons.is_empty() {
        ParameterPost::Rejected { accepted, reasons }
    } else if accepted.is_empty() {
        ParameterPost::Nothing
    } else {
        ParameterPost::Updated { accepted }
    }
}

#[cfg(test)]
mod tests;
