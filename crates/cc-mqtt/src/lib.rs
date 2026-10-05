//! The MQTT topic layout, the publish registry, and the value buffers — the
//! part of `cc-hal-esp32`'s `mqtt.rs` that is a function of a [`Config`] and
//! the machine state rather than of a socket.
//!
//! # Why this crate exists
//!
//! **Finding 4.1b of
//! [`32-findings-2026-10-03.md`](../../../docs/history/review-2026-10-03.md)
//! — `just test` could not reach the MQTT surface at all.** `mqtt.rs` was ~2,030
//! lines and named `esp_idf_svc`, so the six portable crates named by `just
//! test` reached none of it. Every assertion about the wire contract — that the
//! registry is the C++'s 32 parameters and 13 sensors, that the phase slices
//! partition the plan, that an inbound reading resolves only if it is
//! registered — was executed **only** by flashing a board (`just test-esp32`).
//! That is a worse gap than the REST surface had, and it was left open on
//! purpose: `cc_web`'s own crate doc records that "not attempted here rather
//! than attempted and left half-done" was the deliberate call, because the
//! registry and the discovery payloads are *verified parity* and a half-moved
//! `mqtt.rs` is a firmware that does not compile.
//!
//! It is now moved, whole, in one commit, with every assertion that moved with
//! it. Nothing below is rewritten: the bodies are the `mqtt.rs` bodies.
//!
//! # Where it sits in the layering
//!
//! ```text
//!   cc-safety ── cc-machine ─┐
//!                            ├─> cc-hal-esp32 ──> cc-firmware
//!   cc-domain ── cc-config ──┴─> cc-web ─────────┤
//!                       cc-domain ── cc-config ──> cc-mqtt ──> cc-hal-esp32
//! ```
//!
//! **`cc-domain` and `cc-config` are the only workspace dependencies**, and
//! neither knows this crate exists — 04 §6 makes them siblings, and the
//! dependency arrow points one way: a `Config` flows *in*, and nothing about
//! how it will be published is visible from down here.
//!
//! **`cc-hal-esp32` depends on this crate**, never the reverse. `cc-web` is the
//! template for that direction and for the crate-level documentation that says
//! so out loud.
//!
//! # What moved, and what is here
//!
//! | Moved out of `cc_hal_esp32::mqtt` | Where it is now |
//! | --- | --- |
//! | [`Topics`] — `MQTTManager::setup`'s `"%s%s/%s"` layout | [`topics`] |
//! | [`ParamTopic`], `STEAM_MODE`/`BACKFLUSH_ON`/`TARE_ON`/`CALIBRATION_ON` | [`topics`] |
//! | [`Registry`] and its `from_config` — the C++'s 32 + 13 + 1 registration | [`registry`] |
//! | [`PlanView`] — the flattened three-phase plan | [`registry`] |
//! | [`Payload`] — the reused `char data[256]`, as a `heapless::String` | [`payload`] |
//! | [`SetCommand`] — one inbound `set`, in fixed buffers | [`payload`] |
//! | the intervals, the budget and the buffer sizes | [`budget`], [`payload`] |
//! | `interval_for`, `is_configured` | [`interval_for`], [`is_configured`] |
//!
//! # Every function here is pure, and that was checked one at a time
//!
//! The three traps that made `cc-web`'s extraction smaller than its brief
//! claimed — an interpolated `free_heap()`, an unexamined dependency, and a
//! borrow of shared state — were checked against every MQTT item before
//! anything was moved. **One of the three turned up, and one did not:**
//!
//! * **`is_configured` and `interval_for` are pure.** `is_configured` reads
//!   `config.mqtt.enabled` and `config.mqtt.broker` and nothing else;
//!   `interval_for` is a `match` on a [`cc_domain::state::MachineState`]. No
//!   heap gauge, no `now_ms`, no client handle.
//! * **`Registry::from_config` is pure.** It reads four booleans and one
//!   `ScaleType` out of the configuration. Its doc comment already said so —
//!   "the conditional groups are read from it here, once, exactly as
//!   `SystemInitializer` read `Config::getInstance()` once" — and that is the
//!   claim the move had to be true of.
//! * **`Topics` has no separator the C++ does not have, and no default that
//!   could drift**: `format!` over two borrowed `&str`.
//! * **No MQTT item interpolates a heap gauge or the clock.** This is the trap
//!   that caught `cc_web::payload::status_json`, and it does **not** apply
//!   here: `esp_get_free_heap_size()` appears nowhere in `mqtt.rs`, and neither
//!   do `now_ms()` or a free-running tick. The two time-dependent items —
//!   `Client::discovery_due` and the `TIME_BUDGET_MS` check inside
//!   `Feed::service` — are both inside the publisher, which stayed.
//!
//! # What stayed behind, and why
//!
//! | Stayed in `cc-hal-esp32` | Why |
//! | --- | --- |
//! | `Client` | `EspMqttClient`, `MqttClientConfiguration`, `LwtConfiguration`; `esp_mqtt_client_start` and `esp_mqtt_client_publish` are FFI |
//! | `Link`, `note_event_on`, `receive` | the callback runs on the `esp-mqtt` task and the channel is `AtomicBool` + `Mutex<Deque>`; the *policy* around it stayed whole because there is no portable half to split |
//! | `Feed`, `Service`, the `BTreeMap` dedupe | it walks the cursor and calls `publish_to` under a clock reading; the budget is a claim about the control task's 10 ms period, which is a device claim |
//! | `MQTT_TASK_STACK_BYTES`, `MQTT_TASK_PRIO` | they are arguments to `MqttClientConfiguration::task_stack`/`task_prio` and nothing else |
//!
//! `receive` is the one judgement call worth naming, because it is the C++'s
//! `MQTTManager::messageCallback` (`MQTTManager.cpp:237-274`) and it is *not*
//! pure: it locks the inbound queue and pushes onto it. **The parse it performs
//! is pure** — [`Topics::command_of`] moved, so the `%119[^\/]` matcher is
//! host-tested from here — and the queue push is what stayed. Splitting
//! `receive` itself into a pure half that returns a [`SetCommand`] and an
//! impure half that enqueues it would be a restructure with no test to justify
//! it, so it stayed as it is.
//!
//! # Why the discovery payloads are still in `cc-config` and not here
//!
//! [`cc_config::discovery`] builds the Home Assistant documents, and this
//! crate publishes them — but building them is a function of the configuration
//! alone and has been host-tested where it already was. Moving it here would
//! have been a second move with its own parity risk (17–29 entities, pinned
//! against the C++) for no additional reachability.
//!
//! # The one test that could not move, and why it did not need to
//!
//! `the_shared_plan_helper_still_sees_the_whole_registry` existed to keep the
//! on-target runner honest about `plan_of`, the leaked-registry shortcut the
//! other cases use. Every case that used `plan_of` moved, so it moved with
//! them; the fact it guarded — that the three slices **partition** the view —
//! is now checked directly and per-configuration by
//! `registry::tests::the_plan_slices_partition_the_view`, which is the bug
//! class it was written for (`PlanView` stored *lengths* and used them as
//! *offsets*, so any registry with a binary sensor panicked the MQTT task).
#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

extern crate alloc;

pub mod budget;
pub mod payload;
pub mod registry;
pub mod topics;

pub use budget::{
    interval_for, DISCOVERY_INTERVAL_MS, INTERVAL_BREW_MS, INTERVAL_MS, INTERVAL_STANDBY_MS,
    TIME_BUDGET_MS,
};
pub use payload::{
    Payload, SetCommand, BUFFER_BYTES, INBOUND_DEPTH, PAYLOAD_MAX, READING_MAX, VALUE_MAX,
};
pub use registry::{PlanView, Registry};
pub use topics::{
    ParamTopic, Topics, BACKFLUSH_ON, CALIBRATION_ON, STEAM_MODE, TARE_ON, TOPIC_MAX,
};

use cc_config::Config;

/// Whether a broker is configured at all.
///
/// `MQTTManager::setup` (`MQTTManager.cpp:79-83`) disables MQTT outright when the
/// broker name is empty, rather than building a client and failing to
/// connect. Reproduced: an empty `mqtt.broker` is the *default*, so a machine
/// that has never been provisioned should not spawn an MQTT task at all — 4 KB of
/// stack and 2 KB of buffers for a client with nowhere to connect.
///
/// This is a function of the configuration and of nothing else — no heap gauge,
/// no clock, no client handle — which is why it is here rather than a question
/// the caller has to answer before it can build a client.
#[must_use]
pub fn is_configured(config: &Config) -> bool {
    config.mqtt.enabled && !config.mqtt.broker.is_empty()
}

#[cfg(test)]
mod tests;
