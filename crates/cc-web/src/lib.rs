//! The HTTP application tier, as pure functions of a [`Telemetry`] snapshot and
//! a [`cc_config::Config`] — the payload builders rather than the socket.
//!
//! # Why this crate exists
//!
//! A `Telemetry` and a `Config` are plain values, every `*_json` is a `String`
//! builder, and `classify_parameters` is a validation pass over a slice of form
//! pairs. None of that needs a chip, and while it lived in the HAL's `web.rs` no
//! host test reached it: it was exercised only by flashing a board. The only
//! thing standing between it and `cargo test` was which file it sat in.
//!
//! # Where it sits in the layering
//!
//! ```text
//!   cc-safety ── cc-machine ─┐
//!                            ├─> cc-hal-esp32 ──> cc-firmware
//!   cc-domain ── cc-config ──┴─> cc-web ─────────┘
//! ```
//!
//! * **`cc-domain`, `cc-config` and `cc-protocol` are the only workspace
//!   dependencies**, and not one of them is allowed to know this crate exists:
//!   the dependency arrow points one way, a value flows *in*, and nothing about
//!   how it will be served is visible from down here. Beyond those three the
//!   manifest lists only `heapless` (for `Telemetry::ip`, which must stay a
//!   16-byte inline `String` so that `Snapshot::get`'s copy is a `memcpy` with
//!   no allocator traffic) and `log` (for the one line `Auth::from_config`
//!   warns on when authentication is enabled with no credentials). Neither is
//!   an ESP-IDF crate — the argument for this crate being portable rests on
//!   that, not on the count.
//! * **`cc-hal-esp32` depends on this crate**, not the other way round. What
//!   stayed behind is the part that genuinely needs ESP-IDF: route registration
//!   on `EspHttpServer`, the `respond`/`respond_large` chunked writers, the SSE
//!   broadcaster, the httpd `Configuration`, and the `Snapshot` cell that
//!   publishes a `Telemetry` between tasks.
//!
//! # The `Snapshot` boundary
//!
//! `cc_hal_esp32::web::Snapshot<T>` stays in the HAL. It is **generic over its
//! payload**, so only the payload type moved.
//!
//! The `unsafe` in `Snapshot` is a hand-written `Sync` whose whole justification
//! is that every read and write happens inside `esp_idf_hal::interrupt::free` —
//! `portENTER_CRITICAL`, which on this single-core target is mutual exclusion
//! between the control task, the httpd task and the display task. That is an
//! ESP-IDF primitive, so `Snapshot` cannot live here.
//!
//! A read must be a take-and-restore inside one critical section, not
//! `Cell::take`: with a destructive read, two `/api/status` polls inside one
//! 10 ms control period would have the second report a dead machine.
//!
//! The alternative — widening the critical section, or replacing the primitive
//! with a portable one that is not mutually exclusive on this target — would
//! have traded a reviewable safety argument for a portable-looking test. This
//! crate takes the split that keeps both: **the payload is portable, the
//! primitive is not.** `Telemetry` is a `Clone + Default` struct of `Copy` fields
//! and a `heapless::String<15>`, and that is precisely the property the `Sync`
//! impl's second safety bullet names when it explains why a reader's copy cannot
//! allocate.
//!
//! # Two renderers take their inputs as arguments
//!
//! * [`payload::status_json`] interpolates `free_heap()`;
//! * [`payload::nvs_debug_json`] interpolates `free_heap()` and `min_free_heap()`.
//!
//! Both are `esp_get_free_heap_size()` / `esp_get_minimum_free_heap_size()` —
//! FFI reads through `esp_idf_svc::sys`, and not something a host can answer.
//! They take the reading as a parameter instead. **The bytes they emit are
//! unchanged** — the caller passes the same gauge at the same point in the same
//! handler — and what is now testable is the part that was not: the JSON shape
//! around a given heap number.
//!
//! # What stayed behind, and why
//!
//! | Stayed in `cc-hal-esp32` | Why |
//! | --- | --- |
//! | route registration, `respond`, `respond_large` | a chunked response is an ESP-IDF call |
//! | `Shared`, `Snapshot`, the ack counter | `Snapshot` (above); `wait_applied` sleeps |
//! | `Sse`, the broadcaster, `web_async` | the sockets belong to ESP-IDF's one httpd task |
//! | `resolve_ui`, `UiTarget`, the embedded bundle | the asset table comes from `build.rs` |
//! | `routes()` | it returns `esp_idf_svc::http::Method`: what is registered |
//! | `history_json` | copying out under the lock keeps a 12 KB render off the control loop |
//! | all of `mqtt.rs` | its pure half is `cc_hal_esp32::mqtt`'s `Topics` and `Registry` |
//! | the telnet socket, accept loop, heartbeat | the `lwip_*` calls are FFI; the shed, the ring and the line format did move to [`telnet`] |

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod auth;
pub mod help;
pub mod ota;
pub mod parameters;
pub mod payload;
pub mod request;
pub mod telemetry;
pub mod telnet;

pub use auth::Auth;
pub use help::{not_found_json, parameter_help, wants_json_not_found};
pub use ota::{Kind, PartReader, Progress, ReadError, MAX_FILESYSTEM_BYTES, MAX_FIRMWARE_BYTES};
pub use parameters::{
    classify_parameters, ParameterPost, MAX_CONFIG_UPLOAD_BYTES, MAX_PARAMETER_BODY_BYTES,
    MAX_PARAMETER_PAIRS,
};
pub use payload::{
    error_body, health_json, mime_for, nvs_debug_json, parameters_json, status_json,
    temperatures_json, unavailable_json, upload_response, weight_json,
};
pub use request::{explicit_value, first_of, parse_flag, parse_setpoint, query_of};
pub use telemetry::{Command, Telemetry};
pub use telnet::{Decision, Level, Ring, Shed};
