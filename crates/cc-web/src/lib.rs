//! The HTTP application tier, as pure functions of a [`Telemetry`] snapshot and
//! a [`cc_config::Config`] — the part of `cc-hal-esp32`'s `web.rs` that is a
//! payload builder rather than a socket.
//!
//! # Why this crate exists
//!
//! **Finding 4.1 of
//! [`32-findings-2026-10-03.md`](../../../docs/rust-migration/32-findings-2026-10-03.md)
//! — `just test` could not reach the REST surface at all.** `web.rs` was ~4,530
//! lines and named `esp_idf_svc`, so the five portable crates named by `just
//! test` reached none of it: every `*_json` renderer, `classify_parameters`,
//! `Telemetry` and `Command` were tested **only** by flashing a board
//! (`just test-esp32`). Two real device bugs shipped through exactly that gap —
//! a Wi-Fi provisioning password window that lasted zero milliseconds, and
//! console lines lost across `esp_restart()`. Neither was caught by review; both
//! had a test that had never been executed.
//!
//! None of that code needed a chip. A `Telemetry` and a `Config` are plain
//! values, `*_json` is a `String` builder, and `classify_parameters` is a
//! validation pass over a slice of form pairs. The only thing standing between
//! them and `cargo test` was which file they sat in.
//!
//! # Where it sits in the layering
//!
//! ```text
//!   cc-safety ── cc-machine ─┐
//!                            ├─> cc-hal-esp32 ──> cc-firmware
//!   cc-domain ── cc-config ──┴─> cc-web ─────────┘
//! ```
//!
//! * **`cc-domain` and `cc-config` are the only workspace dependencies**, and
//!   neither is allowed to know this crate exists — `cc-config` may not depend
//!   on `cc-safety`, and 04 §6 makes them siblings. The dependency arrow points
//!   one way: a value flows *in*, and nothing about how it will be served is
//!   visible from down here.
//! * **`cc-hal-esp32` depends on this crate**, not the other way round. What
//!   stayed behind is the part that genuinely needs ESP-IDF: route registration
//!   on `EspHttpServer`, the `respond`/`respond_large` chunked writers, the SSE
//!   broadcaster, the httpd `Configuration`, and the `Snapshot` cell that
//!   publishes a `Telemetry` between tasks.
//!
//! # The `Snapshot` boundary, stated once
//!
//! `cc_hal_esp32::web::Snapshot<T>` stays in the HAL, and this is not a
//! compromise — it is the shape the type already had. `Snapshot` is **generic
//! over its payload**; only the payload type moved.
//!
//! The `unsafe` in `Snapshot` is a hand-written `Sync` whose entire justification
//! is that every read and write happens inside `esp_idf_hal::interrupt::free` —
//! `portENTER_CRITICAL`, which on this single-core target is mutual exclusion
//! between the control task, the httpd task and the display task. That is an
//! ESP-IDF primitive, so `Snapshot` cannot live here; and commit `44c546db`
//! fixed a real defect in it (`get` was `Cell::take`, so a read *destroyed* the
//! value it returned, and two `/api/status` polls inside one 10 ms control
//! period reported a dead machine on the second). That fix is a take-and-restore
//! inside one critical section, and it is untouched by this move.
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
//! # Two renderers take their inputs as arguments, and why
//!
//! **`Telemetry`, `Command` and every `*_json` renderer are pure functions of a
//! `Config` and a snapshot" is not quite true of two of them**, and the finding
//! that says so is slightly wrong:
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
//! | route registration, `respond`, `respond_large`, `respond_download` | `EspHttpServer` / `EspHttpConnection`, and ADR-0002 decision 2 is a statement about `httpd_resp_send_chunk` |
//! | `Shared`, `Snapshot`, the ack counter, `wait_applied` | `Snapshot` (above); `wait_applied` reads `now_ms` and sleeps |
//! | `Sse`, the broadcaster, `web_async` | the sockets belong to ESP-IDF's single httpd task |
//! | `resolve_ui`, `UiTarget`, the embedded bundle | the asset table is generated by `build.rs` from the Vite output |
//! | `routes()` | returns `esp_idf_svc::http::Method`; the route table *is* a statement about what is registered on the httpd |
//! | `history_json` | it takes `&Shared` and the copy-out-under-the-lock that keeps a 12 KB render off the control loop is the point of it. It had no direct test before this move and has none now. |
//! | all of `mqtt.rs` | its pure half (`cc_hal_esp32::mqtt`'s `Topics`, `Registry`, `interval_for`) is a second extraction with the same shape. Not attempted here rather than attempted and left half-done. |

#![no_std]
#![forbid(unsafe_code)] // already denied workspace-wide; restated for clarity
#![deny(missing_docs)]

extern crate alloc;

pub mod auth;
pub mod help;
pub mod parameters;
pub mod payload;
pub mod request;
pub mod telemetry;

pub use auth::Auth;
pub use help::{not_found_json, parameter_help, wants_json_not_found};
pub use parameters::{
    classify_parameters, ParameterPost, MAX_CONFIG_UPLOAD_BYTES, MAX_PARAMETER_BODY_BYTES,
    MAX_PARAMETER_PAIRS,
};
pub use payload::{
    error_body, health_json, mime_for, nvs_debug_json, ota_status_json, parameters_json,
    status_json, temperatures_json, unavailable_json, upload_response, weight_json,
};
pub use request::{explicit_value, first_of, parse_flag, parse_setpoint, query_of};
pub use telemetry::{Command, Telemetry};
