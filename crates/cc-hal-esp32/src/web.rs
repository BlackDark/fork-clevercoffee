//! ESP-IDF routes, chunked responses, shared snapshots, and SSE transport.
//!
//! [`routes`] is the registration table; [`cc_web`] owns portable payloads and parsing.
//! Handlers read snapshots and queue commands rather than accessing control state.
//!
//! Follow [ADR-0002](../../../docs/adr/0002-wifi-logging-ota-memory-architecture.md):
//! stream large responses without intermediate strings, and let telnet shed under
//! heap pressure. Parallel UI requests otherwise multiply large JSON allocations.
//!
//! httpd has one task. An SSE handler must set headers, detach through
//! `httpd_req_async_handler_begin`, and return; [`crate::web_async`] lets the
//! broadcaster write separately. Keeping `/events` open on httpd caused 55 of 60
//! API requests to time out. Browser `EventSource` supports chunked transfer.
//!
//! `/api/ota/url` returns 501 with a reason; unsupported readings must not return
//! plausible placeholder values.

// The `unsafe` in this module is the lock-free `Cell` below: one writer,
// sequence-checked readers. The reasoning is on the type and at every access; the
// workspace lint is `unsafe_code = "deny"`, so it is allowed here for the same
// reason `crate::web_async` is.
#![allow(
    unsafe_code,
    reason = "`Snapshot`: one writer, and every read and write happens inside \
              `interrupt::free`, which is mutual exclusion on this single-core \
              target. The justification is on the impl itself and is checkable \
              there rather than resting on a promise about retry loops."
)]

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use cc_config::Config;
use cc_protocol::http_auth::WWW_AUTHENTICATE;
// The application tier lives in `cc-web`, which is `no_std` and host-testable.
// These are the payload builders and the two shared types; what stayed here is
// route registration, the chunked writers, the broadcaster and the httpd
// `Configuration`. See `cc_web`'s crate root for the boundary and for why
// `Snapshot` did not move with them.
use cc_web::{
    classify_parameters, error_body, explicit_value, first_of, health_json, mime_for,
    nvs_debug_json, parameter_help, parse_flag, parse_setpoint, query_of, status_json,
    temperatures_json, unavailable_json, upload_response, weight_json, Auth, Kind, ParameterPost,
};
pub use cc_web::{
    parameters_json, Command, Telemetry, MAX_CONFIG_UPLOAD_BYTES, MAX_PARAMETER_BODY_BYTES,
    MAX_PARAMETER_PAIRS,
};
use esp_idf_hal::interrupt;
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer, Request};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys::EspError;
// `error!` is here for the OTA routes, which are the only handlers in this file
// that report a flash-layer failure rather than a request-layer one.
use log::{error, info, warn};

use crate::heap::{free_heap, min_free_heap, HEAP_SHED_BYTES};
use crate::task::{COMMAND_ACK_POLL_MS, COMMAND_ACK_TIMEOUT_MS};
use crate::time::now_ms;

/// The HTTP port.
pub const HTTP_PORT: u16 = 80;

/// The telnet log stream's port.
pub const TELNET_PORT: u16 = 23;

/// The largest JSON response this server will build, in bytes.
///
/// `/api/parameters?filter=all` is the biggest: 96 registered parameters, each
/// an object with a name, a type, a value, a minimum and a maximum, measured at
/// ~19 KB (ADR-0002, §2). 32 KB is comfortably above that and comfortably below
/// the point at which one response could be the OOM.
pub const MAX_JSON_BYTES: usize = 32 * 1024;

/// The heap floor below which the large-response path refuses to build.
///
/// **The same number as the telnet shed's**, and deliberately: ADR-0002 set one
/// threshold for "the machine is tight", and two constants for the same judgement
/// is how they drift. Below it, `/api/parameters` and `/api/config` answer `503`
/// rather than risking the `abort()` the ADR exists to prevent — a refused
/// request is a visible error, an OOM is a reboot in the middle of a shot.
pub const HEAP_FLOOR_BYTES: u32 = HEAP_SHED_BYTES;

/// The interval between temperature events on `/events`, in milliseconds.
///
/// It matters for the same reason the publish budget does: `/events` pushes a
/// frame per client per interval, and with a browser open on a slow link the
/// frames queue.
pub const SSE_EVENT_INTERVAL_MS: u32 = 1_000;

/// The idle keep-alive frame, in milliseconds.
///
/// A browser's `EventSource` reconnects on its own but a NAT timeout will drop a
/// silent connection first, so an idle `/events` gets a comment frame. 15 s is
/// under the common 30 s NAT floor and well above the 1 s event cadence.
pub const SSE_KEEPALIVE_MS: u32 = 15_000;

/// How often the broadcaster task looks for a due frame.
///
/// 50 ms, against a 1 s event interval and a 15 s keepalive, so a due frame is
/// at most 50 ms late. The cost is one task wakeup every 50 ms whether or not
/// anyone is streaming, which is why it is not 10 ms: there is nothing to gain
/// from resolving a due frame faster than a browser paints.
pub const SSE_POLL_MS: u32 = 50;

/// The broadcaster task's stack.
///
/// 4096 B. It formats the same JSON the httpd task used to format on its own
/// 8192 B stack ([`configuration`]), so this is not smaller in the way that
/// matters — but it is a *different* task, so the httpd task's stack no longer
/// has to accommodate streaming at all, and 4096 is what the formatting needs
/// with nothing else on the stack.
const SSE_BROADCASTER_STACK_BYTES: usize = 4096;

/// How many pushed frames may wait for the broadcaster.
///
/// Four. The producers are the control task at [`SSE_EVENT_INTERVAL_MS`] and the
/// weight event, so the mailbox is emptied roughly 20× faster than it fills; it
/// exists to carry a push across the one broadcaster pass, not to buffer. Four
/// is enough that a broadcaster pass delayed by a slow client does not start
/// dropping, and small enough that a wedged broadcaster cannot grow the heap
/// without bound on a machine with 320 KB of it.
const SSE_MAILBOX_DEPTH: usize = 4;

/// The server's URI-handler budget.
///
/// `HttpServerConfiguration::max_uri_handlers` defaults to 32 (`server.rs:132`).
/// Raised to 40 because the route table carries every route plus
/// `POST /api/config/upload` and the `OPTIONS` preflight, and because
/// `ESP_ERR_HTTPD_HANDLERS_FULL` fires **at boot** — which is the worst possible
/// moment to discover a route was added. The headroom is against ESP-IDF's own
/// static allocation of `max_uri_handlers * sizeof(httpd_uri_t)`, so it costs
/// `8 * 8` bytes, not per-request memory.
/// `max_open_sockets` is raised from 4 to 5 for the same reason: a browser opens
/// the SPA, an SSE stream and several API requests at once, and 4 leaves no
/// session for the fourth.
pub const MAX_URI_HANDLERS: usize = 40;
/// See [`MAX_URI_HANDLERS`].
pub const MAX_OPEN_SOCKETS: usize = 5;

/// Snapshot shared through short interrupt-masked copies, without blocking queues.
///
/// A seqlock is unsound for non-atomic Rust payloads even when torn reads are
/// discarded; owned strings can additionally be freed during a reader's clone.
/// Protect the entire read and write with `interrupt::free`. Keep payload clones
/// bounded and nonblocking; the local `Sync`/`Send` argument states the conditions.
pub struct Snapshot<T> {
    // `core::cell::Cell`, deliberately: it is `!Sync`, which is what makes the
    // hand-written `Sync` below an honest statement of the invariant rather
    // than a consequence of the field type.
    value: core::cell::Cell<T>,
}

// SAFETY: the *only* ways to reach `value` are `set` and `get`, and both run
// their whole read-modify-write inside `interrupt::free`, i.e. inside
// `portENTER_CRITICAL`/`portEXIT_CRITICAL`. On this single-core target that is
// mutual exclusion between the control task, the httpd task and the display
// task, and it also excludes the ISR, so no access can overlap any other.
//
// The two rules that make that argument hold, both enforced by construction
// rather than by discipline:
//
// 1. `value` is private to this module and the only `unsafe` code in this crate
//    cannot name it — `Cell::as_ptr` would hand out a raw pointer, and nothing
//    calls it. Grep for `as_ptr` to confirm; if that grep ever finds a hit,
//    this `Sync` is void.
//
// 2. `set` and `get` do not call out to anything that can block. `get` clones
//    `T`; with `T = Telemetry` that is a fixed-size copy, because `ip` is a
//    `heapless::String<15>` and not a `String`. `T: Send` bounds the transfer.
//
// `T: Send` is the right bound rather than `T: Sync`: the value is moved between
// tasks, and a reader only ever *copies* it inside the critical section.
unsafe impl<T: Send> Sync for Snapshot<T> {}
// SAFETY: `Snapshot<T>` is just a `core::cell::Cell<T>` with no thread-affine
// state, and its `Sync` impl (above) already establishes that concurrent access
// is mutually excluded. Moving one between tasks moves a value, nothing else.
unsafe impl<T: Send> Send for Snapshot<T> {}

impl<T: Clone + Default> Snapshot<T> {
    /// An empty snapshot, readable as `T::default()` until it is written.
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: core::cell::Cell::new(T::default()),
        }
    }

    /// Replace the value. **One writer** — the control task.
    pub fn set(&self, value: T) {
        // `Cell::set` takes `&self`, so "one writer" is a discipline rather
        // than a type-level fact. It is honoured: `Shared::publish` and
        // `network::publish_radio` are both called from the control task, and
        // nothing else calls `set`. The mutual exclusion that makes a second
        // writer *safe* rather than merely racy is the critical section below.
        interrupt::free(|| self.value.set(value));
    }

    /// Clone the current value without consuming it.
    ///
    /// Take, clone, and restore inside one critical section. Leaving defaults behind
    /// would make a second API poll within one control period report a dead machine.
    #[must_use]
    pub fn get(&self) -> T {
        interrupt::free(|| {
            let value = self.value.take();
            self.value.set(value.clone());
            value
        })
    }
}

impl<T: Clone + Default> Default for Snapshot<T> {
    /// An empty snapshot -- the same value [`Snapshot::new`] produces, and what
    /// a `#[derive(Default)]` on a containing struct would reach for.
    fn default() -> Self {
        Self::new()
    }
}

/// The shared state the HTTP handlers close over.
pub struct Shared {
    /// The latest telemetry, republished by the control task.
    pub telemetry: Snapshot<Telemetry>,
    /// The temperature history, appended by the control task.
    ///
    /// The producer is the control task, so the ring is shared state and lives
    /// here — next to the telemetry, which the same task publishes for the same
    /// reason.
    ///
    /// **Boxed, and that is not a style choice.** The ring is 600 points, 7.2 KB;
    /// a `Shared` built *by value* — which two on-target tests do — put that on
    /// an 8 KB task stack and the device rebooted mid-suite, which the runner
    /// reports as LOST rather than as a failure. It is the same lesson as the
    /// 1 KB `Display` framebuffer and the 7.2 KB `history_json` return value:
    /// anything this size is heap or it is a crash.
    pub history: Mutex<alloc::boxed::Box<cc_netpolicy::history::History>>,
    /// How many control commands the control task has applied.
    ///
    /// The ack every command caller waits on. A command is a **request**: the
    /// handler hands it to the queue and returns, so anything it then says about
    /// the machine describes the machine as it was. The control task increments
    /// this once per command it drains, so `applied() == before` means "mine is
    /// still queued".
    applied: AtomicU32,
    /// Set when a handler has asked for a reboot; the firmware acts on it.
    pub reboot_requested: AtomicBool,
    /// The number of `/api/parameters?filter=all` responses served, for the
    /// ADR-0002 regression check.
    pub large_responses: AtomicU32,
    /// The number refused because the heap was below the floor.
    pub large_refused: AtomicU32,
}

impl Shared {
    /// A `Shared` with default telemetry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            telemetry: Snapshot::new(),
            history: Mutex::new(alloc::boxed::Box::new(cc_netpolicy::history::History::new())),
            applied: AtomicU32::new(0),
            reboot_requested: AtomicBool::new(false),
            large_responses: AtomicU32::new(0),
            large_refused: AtomicU32::new(0),
        }
    }

    /// Append one sample to the history ring, ignoring a poisoned lock.
    ///
    /// Called by the control task on the temperature cadence. A poisoned lock
    /// means some other thread panicked while holding it, and the only honest
    /// response to "the chart is a diagnostic" is to lose a point rather than
    /// refuse to regulate a boiler.
    pub fn push_history(&self, current_temp: f32, target_temp: f32, heater_power: f32) {
        if let Ok(mut ring) = self.history.lock() {
            ring.push(current_temp, target_temp, heater_power);
        }
    }

    /// Replace the telemetry snapshot.
    ///
    /// **A whole-slot replace, not a merge.** There are two publishers — the
    /// control task, through [`Telemetry`]'s machine fields, and the radio,
    /// through the four fields [`network::publish_radio`] owns — so whoever calls
    /// this must leave the other's fields alone. In practice that means the
    /// control task publishes first and the radio second in the same tick; see
    /// `publish_radio` for why the order matters.
    pub fn publish(&self, telemetry: Telemetry) {
        self.telemetry.set(telemetry);
    }

    /// A copy of the current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Telemetry {
        // Falls back to the default rather than blocking. A reader that could not
        // get a consistent snapshot in four attempts gets zeroes for one poll,
        // which `/api/status` already reports as "no reading yet" — and blocking
        // here would put a `pthread` mutex on the httpd task's hottest path,
        // which is the hazard `Cell` exists to remove.
        self.telemetry.get()
    }

    /// How many large responses have been served.
    ///
    /// The number ADR-0002's consequences section asks for: a regression that
    /// brought the double-copy back would show up as this falling while the
    /// heap minimum rises.
    #[must_use]
    pub fn large_responses(&self) -> u32 {
        self.large_responses.load(Ordering::Relaxed)
    }

    /// How many large responses were refused for want of heap.
    #[must_use]
    pub fn large_refused(&self) -> u32 {
        self.large_refused.load(Ordering::Relaxed)
    }

    /// Note that the control task has applied one more command.
    ///
    /// Called by the control task, once per command drained.
    pub fn note_applied(&self) {
        self.applied.fetch_add(1, Ordering::Release);
    }

    /// How many commands the control task has applied.
    #[must_use]
    pub fn applied(&self) -> u32 {
        self.applied.load(Ordering::Acquire)
    }

    /// Block until [`Self::applied`] passes `before`, or
    /// [`COMMAND_ACK_TIMEOUT_MS`] elapses. Returns whether it advanced.
    ///
    /// A web handler is not the control task, so a `POST /api/pid` cannot mutate
    /// the machine before it answers. A handler can only ask — so it asks, and
    /// waits a bounded time. Bounded because a stalled control task must fail
    /// visibly rather than hang a browser.
    pub fn wait_applied(&self, before: u32) -> bool {
        let deadline = now_ms().wrapping_add(COMMAND_ACK_TIMEOUT_MS);
        while self.applied() == before {
            if now_ms().wrapping_sub(deadline) < COMMAND_ACK_POLL_MS {
                return false;
            }
            crate::task::delay_ms(COMMAND_ACK_POLL_MS);
        }
        true
    }

    /// Ask for a reboot at the next opportunity.
    ///
    /// `POST /api/restart`, `/api/factory-reset` and `/api/wifi-reset` all land
    /// here, and none of them performs the reboot itself: a handler must not be
    /// able to reset the machine from inside an HTTP request, and
    /// [`Shared::take_reboot_request`] is read by the main task between ticks.
    pub fn set_reboot_requested(&self) {
        self.reboot_requested.store(true, Ordering::SeqCst);
    }

    /// Whether a reboot has been asked for, and clear the request.
    pub fn take_reboot_request(&self) -> bool {
        self.reboot_requested.swap(false, Ordering::SeqCst)
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

/// How the SSE stream frames are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SseMode {
    /// `httpd_resp_send_chunk`, via the detached request in
    /// [`crate::web_async::AsyncReq`].
    ///
    /// The default, and the only mode that can work: a chunked write needs a
    /// `httpd_req_t*`, and holding one across the handler's return is only
    /// sound once `httpd_req_async_handler_begin` has copied it out. It is also
    /// the mode that respects the socket's `send_wait_timeout`.
    #[default]
    Chunked,
}

/// The most `/events` clients that will be served at once.
///
/// The reason is [`MAX_OPEN_SOCKETS`]: ESP-IDF's httpd has a fixed session pool,
/// an SSE client occupies one session for as long as it is connected, and a
/// browser opens several connections of its own for the SPA. Two leaves three of
/// five for `/api/*`, which is the number the ADR-0002 reproducer (6–10 parallel
/// requests) needs to be served rather than queued behind a stream.
///
/// The cost of the cap is stated rather than hidden: a third tab gets a `503`
/// and an error in the browser console instead of a working stream. The
/// alternative — letting the session pool fill — is the failure this whole
/// change exists to remove, in the other direction.
pub const SSE_MAX_CLIENTS: usize = 2;

/// One connected `/events` client.
///
/// Owns the request handed over by `httpd_req_async_handler_begin`, plus the
/// two deadlines that decide what it gets next. Everything is per-client because
/// a browser that connects late must not inherit another's keepalive schedule —
/// and, more importantly, because a client whose write fails must be removable
/// without disturbing the others.
struct SseClient {
    req: crate::web_async::AsyncReq,
    /// Whether the greeting frame is still owed to this client.
    ///
    /// A browser's `EventSource` dispatches nothing until it sees a frame, so
    /// without it the connection looks dead to the UI until the first event
    /// arrives. Sent on connect.
    await_hello: bool,
    /// When this client last got a frame of any kind. The keepalive deadline is
    /// measured from here so a busy stream does not also emit keepalives.
    last_write_ms: u32,
}

/// The `/events` stream's broadcast state.
///
/// A real client list, bounded by [`SSE_MAX_CLIENTS`], in an `Arc` shared between
/// the httpd task (which adds and removes clients) and the broadcaster task
/// (which writes).
pub struct Sse {
    mode: SseMode,
    clients: Mutex<Vec<SseClient>>,
    mailbox: Mutex<VecDeque<String>>,
    connected: AtomicU32,
    rejected: AtomicU32,
    pushed: AtomicU32,
    sent: AtomicU32,
    dropped: AtomicU32,
    dropped_frames: AtomicU32,
}

impl Sse {
    /// An empty stream in the given framing mode.
    #[must_use]
    pub fn new(mode: SseMode) -> Self {
        Self {
            mode,
            clients: Mutex::new(Vec::new()),
            mailbox: Mutex::new(VecDeque::new()),
            connected: AtomicU32::new(0),
            rejected: AtomicU32::new(0),
            pushed: AtomicU32::new(0),
            sent: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            dropped_frames: AtomicU32::new(0),
        }
    }

    /// The framing mode.
    #[must_use]
    pub const fn mode(&self) -> SseMode {
        self.mode
    }

    /// How many clients have connected since boot.
    ///
    /// Counts *connections*, not concurrent clients, so it keeps rising after
    /// a tab is closed. [`Sse::connected_now`] is the concurrent figure.
    #[must_use]
    pub fn clients(&self) -> u32 {
        self.connected.load(Ordering::SeqCst)
    }

    /// How many clients are connected right now.
    #[must_use]
    pub fn connected_now(&self) -> usize {
        self.clients.lock().map_or(0, |c| c.len())
    }

    /// How many connections were refused for want of a free client slot.
    #[must_use]
    pub fn rejected(&self) -> u32 {
        self.rejected.load(Ordering::SeqCst)
    }

    /// How many event frames have been written.
    #[must_use]
    pub fn sent(&self) -> u32 {
        self.sent.load(Ordering::SeqCst)
    }

    /// How many frames were dropped because the client had gone away.
    ///
    /// A `write` to a closed socket is not an error worth surfacing. Counting it
    /// is what makes "the stream delivered events for ten minutes with zero
    /// drops" a measurable claim rather than an assertion.
    #[must_use]
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::SeqCst)
    }

    /// How many pushes have been offered to the stream.
    #[must_use]
    pub fn pushed(&self) -> u32 {
        self.pushed.load(Ordering::SeqCst)
    }

    /// How many pushed frames were discarded because the mailbox was full.
    ///
    /// Separate from [`Sse::dropped`] because they are different failures: a
    /// `dropped` frame reached a client that had gone, a `dropped_frames` frame
    /// never left the machine at all.
    #[must_use]
    pub fn dropped_frames(&self) -> u32 {
        self.dropped_frames.load(Ordering::SeqCst)
    }

    /// One SSE frame: `event: <name>\ndata: <payload>\n\n`.
    #[must_use]
    pub fn frame(event: &str, payload: &str) -> String {
        // The blank line is required by the WHATWG spec and is the single most
        // common way a hand-rolled SSE endpoint delivers nothing at all: the
        // browser buffers the frame and never dispatches it.
        format!("event: {event}\ndata: {payload}\n\n")
    }

    /// An SSE comment frame, which carries no event and keeps the socket warm.
    #[must_use]
    pub fn keepalive() -> String {
        format!(": {}\n\n", now_ms())
    }

    /// Add a client, or hand the request back because the stream is full.
    ///
    /// `Result<(), AsyncReq>` rather than `bool` because the request **must** be
    /// completed on every path: `httpd_req_async_handler_begin` has already set
    /// `sd->for_async_req`, and a request left incomplete
    /// keeps the socket out of the httpd task's `select()` for ever
    /// (`esp_http_server.h:864-866`). Returning it lets the caller release it;
    /// returning `false` and dropping it would leak the socket — which is
    /// exactly how a second refused client would cost the whole API.
    pub(crate) fn attach(
        &self,
        req: crate::web_async::AsyncReq,
    ) -> Result<(), crate::web_async::AsyncReq> {
        let now = now_ms();
        // A poisoned lock means some other client write panicked. Refusing is
        // the only safe answer: the list's length is the bound that keeps the
        // httpd task's socket budget intact.
        let Ok(mut clients) = self.clients.lock() else {
            return Err(req);
        };
        if clients.len() >= SSE_MAX_CLIENTS {
            return Err(req);
        }
        clients.push(SseClient {
            req,
            await_hello: true,
            last_write_ms: now,
        });
        Ok(())
    }
}

/// Write a JSON response, streamed.
///
/// **This is ADR-0002 decision 2.** `httpd_resp_send_chunk` is called once per
/// chunk, so the payload is serialised into one `Vec<u8>` and handed to the
/// socket in pieces; there is no intermediate `String`, and no second copy inside
/// the web server. Returning one owned `String` to a framework that copies it
/// again is the shape that produced three ~19 KB allocations.
fn respond(conn: &mut EspHttpConnection<'_>, status: u16, json: &str) -> Result<(), EspError> {
    conn.initiate_response(status, Some("OK"), &[("Content-Type", "application/json")])?;
    // 512-byte chunks: small enough that the send path's buffer is not the
    // largest allocation on the heap, large enough that a 19 KB response is 38
    // calls rather than 1900.
    for chunk in json.as_bytes().chunks(512) {
        let _ = conn.write(chunk);
    }
    // A zero-length chunk terminates the chunked body; without it the browser
    // waits for more data that never comes.
    let _ = conn.write(&[]);
    Ok(())
}

/// The `401` a request that failed [`Auth`] gets.
///
/// `WWW-Authenticate: Basic realm="CleverCoffee"` is the header that makes a
/// browser show its credential prompt.
///
/// The body is not [`error_body`], because a browser shows the challenge and
/// *not* the body: this text is what an operator reading a `curl` transcript
/// sees, and "authentication required" is a better answer to that than
/// `{"error":"Unauthorized"}`.
fn unauthorized(conn: &mut EspHttpConnection<'_>) -> Result<(), EspError> {
    conn.initiate_response(
        401,
        Some("Unauthorized"),
        &[
            ("Content-Type", "application/json"),
            // The header that makes a browser raise its credential prompt. A
            // different realm is a different protection space, and a browser
            // holding credentials for one will not send them for another.
            ("WWW-Authenticate", WWW_AUTHENTICATE),
        ],
    )?;
    // The same chunked idiom as [`respond`], which is what `fn_handler`'s
    // wrapper expects of every response on this server.
    let _ = conn.write_all(b"{\"error\":\"authentication required\"}");
    let _ = conn.write(&[]);
    Ok(())
}

/// Register `handler` on `uri`, behind [`Auth`].
///
/// **Every data route on this server goes through here, and that is the point.**
/// The alternative — a credential check at the top of each of two dozen
/// closures — is a control whose strength is the number of places nobody
/// forgets, and a forgotten one is an unauthenticated `/api/restart`. Funnelling
/// registration through one function makes "this route is not protected" a thing
/// a reader can see at the call site rather than deduce.
///
/// The two deliberate exceptions are both visible where they are made:
/// [`register_preflight`] for `OPTIONS`, and `crate::web_async::register_raw_sse`
/// for `/events`, which is an `extern "C"` handler ESP-IDF dispatches itself.
fn register<F>(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    uri: &str,
    method: Method,
    handler: F,
) -> Result<(), EspError>
where
    F: for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> Result<(), EspError> + Send + 'static,
{
    let auth = Arc::clone(auth);
    server
        .fn_handler::<EspError, _>(uri, method, move |mut req| {
            // `req.header` borrows, so the verdict is taken before
            // `req.connection` asks for `&mut`. Nothing derived from the
            // header outlives this statement.
            let admitted = auth.admits(req.header("Authorization"));
            if !admitted {
                return unauthorized(req.connection());
            }
            handler(req)
        })
        .map(|_| ())
}

/// Register the CORS preflight answer, and **not** behind [`Auth`].
///
/// A preflight is the one request a browser sends **without** credentials — that
/// is what makes it a preflight — so challenging one makes every cross-origin
/// request fail, permanently and confusingly. It is registered through
/// `fn_handler` and not [`register`] deliberately, and this is the one place in
/// the file where that is true.
///
/// `/api*` rather than one URI per route: ESP-IDF matches URI and method
/// independently (`httpd_uri.c:97-122` — a URI match with the wrong method sets
/// 405 and the search *continues*), so a single wildcard answers every API
/// preflight while leaving every `GET`/`POST` to its own exact handler. This is
/// also what `routes()` advertises, which is the point: the entry used to be
/// `("/api/status", Method::Options)` with no handler behind it at all.
///
/// **No per-response `Access-Control-Allow-Origin: *`**, and the omission is
/// deliberate: the SPA is served same-origin from `/ui` and needs nothing, and a
/// wildcard origin on an endpoint that can reboot a boiler or change its safety
/// cut-offs is a widening with no consumer.
fn register_preflight(server: &mut EspHttpServer<'static>) -> Result<(), EspError> {
    server
        .fn_handler::<EspError, _>("/api*", Method::Options, |mut req| {
            let conn = req.connection();
            conn.initiate_response(
                204,
                Some("No Content"),
                &[
                    ("Access-Control-Allow-Origin", "*"),
                    (
                        "Access-Control-Allow-Methods",
                        "GET,POST,PUT,DELETE,OPTIONS",
                    ),
                    (
                        "Access-Control-Allow-Headers",
                        "Content-Type,Authorization,X-Requested-With",
                    ),
                    ("Content-Length", "0"),
                ],
            )?;
            let _ = conn.write(&[]);
            Ok(())
        })
        .map(|_| ())
}

/// `GET /api/history` — three parallel arrays, oldest point first, each value
/// rounded to two decimals.
///
/// The body is one `String` handed to [`respond_large`], which streams it in
/// 512-byte chunks and refuses it below the ADR-0002 heap floor.
///
/// **The spacing is part of the contract.** The ring keeps every third sample,
/// and the UI reconstructs the x axis as `now - 3 * i` seconds
/// (`CleverCoffeeContext.tsx:240-243`). A ring that kept every sample would draw
/// 30 minutes of data across a 10-minute axis.
///
/// An empty ring answers three empty arrays, not an error: a machine that has
/// been up for four seconds has no history, and the chart's own loading state is
/// a better answer than a failure.
#[must_use]
pub fn history_json(shared: &Shared) -> String {
    // **Copied out under the lock, formatted after it.**
    //
    // Holding the guard across the formatting is the mistake this replaces: the
    // control task appends a sample on the same mutex, so ~1800 `write!` calls
    // and a heap growing to ~12 KB stall the control loop for the duration — a
    // missed 10 ms period and a late heartbeat, caused by a chart. The copy goes
    // to the **heap**, because 600 points is 7.2 KB and the httpd task has 8 KB
    // of stack — the same lesson as the 1 KB display framebuffer, and the same
    // fix.
    //
    // The ring's own `len()` is read under the lock too. Inferring the length
    // from the copied points would truncate at the first zero — and a boiler
    // genuinely at 0 °C is a value this machine reports.
    let (points, len) = match shared.history.lock() {
        Ok(guard) => {
            let len = guard.len();
            let mut copy = alloc::boxed::Box::new([cc_netpolicy::history::Point::default(); 600]);
            for (i, point) in copy.iter_mut().enumerate().take(len) {
                if let Some(p) = guard.get(i) {
                    *point = p;
                }
            }
            (copy, len)
        }
        Err(_) => {
            return String::from("{\"currentTemps\":[],\"targetTemps\":[],\"heaterPowers\":[]}");
        }
    };
    let _ = points;

    let mut out = String::with_capacity(64 + len * 18);
    out.push_str("{\"currentTemps\":[");
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{:.2}", points[i].current_temp);
    }
    out.push_str("],\"targetTemps\":[");
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{:.2}", points[i].target_temp);
    }
    out.push_str("],\"heaterPowers\":[");
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{:.2}", points[i].heater_power);
    }
    out.push_str("]}");
    out
}

/// Write a large JSON response, or refuse.
///
/// The [`MAX_JSON_BYTES`] / [`HEAP_FLOOR_BYTES`] guard, which is ADR-0002's
/// actual lesson: a large response is affordable *until* it is not, and the
/// moment it is not is exactly when a second browser tab or a 5 s MQTT pass is
/// also running.
fn respond_large(
    conn: &mut EspHttpConnection<'_>,
    shared: &Shared,
    json: &str,
) -> Result<(), EspError> {
    if json.len() > MAX_JSON_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        return respond(
            conn,
            503,
            "{\"error\":\"response too large\",\"limit\":32768}",
        );
    }
    if free_heap() < HEAP_FLOOR_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        warn!(
            "http: refusing a {} B response with {} B of heap free (floor {HEAP_FLOOR_BYTES} B)",
            json.len(),
            free_heap()
        );
        return respond(
            conn,
            503,
            "{\"error\":\"insufficient memory\",\"retry\":true}",
        );
    }
    shared.large_responses.fetch_add(1, Ordering::Relaxed);
    respond(conn, 200, json)
}

/// [`respond_large`], plus the `Content-Disposition` that makes the browser save
/// the file instead of rendering it.
///
/// `respond_large` is reused rather than reimplemented so the ADR-0002 heap floor
/// and the 32 KB ceiling apply to this route too — the download serialises the
/// same document `/api/config` does, so it is the same size and has the same
/// failure modes.
fn respond_download(
    conn: &mut EspHttpConnection<'_>,
    shared: &Shared,
    json: &str,
) -> Result<(), EspError> {
    if json.len() > MAX_JSON_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        return respond(
            conn,
            503,
            "{\"error\":\"response too large\",\"limit\":32768}",
        );
    }
    if free_heap() < HEAP_FLOOR_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        warn!(
            "http: refusing a {} B config download with {} B of heap free (floor {HEAP_FLOOR_BYTES} B)",
            json.len(),
            free_heap()
        );
        return respond(
            conn,
            503,
            "{\"error\":\"insufficient memory\",\"retry\":true}",
        );
    }
    shared.large_responses.fetch_add(1, Ordering::Relaxed);
    conn.initiate_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "application/json"),
            (
                "Content-Disposition",
                "attachment; filename=\"config.json\"",
            ),
        ],
    )?;
    for chunk in json.as_bytes().chunks(UI_CHUNK_BYTES) {
        let _ = conn.write(chunk);
    }
    let _ = conn.write(&[]);
    Ok(())
}

// The built React SPA, embedded by `build.rs`. The table is `&'static` and the
// bytes live in flash, so serving the UI costs no RAM beyond the httpd send
// buffer — which is the reason the bundle is embedded rather than mounted: see
// `build.rs` for the size arithmetic that decided it.
mod bundle {
    include!(concat!(env!("OUT_DIR"), "/ui_bundle.rs"));
}

use bundle::{UiAsset, UI_ASSETS, UI_FILE_COUNT, UI_INDEX, UI_TOTAL_BYTES};

/// The chunk size the embedded-file writer streams at.
///
/// The same 512 B [`respond`] uses, and for the same reason: the httpd task's
/// stack is 8 KB and the send path buffers, so a large chunk is a large
/// allocation on a machine whose heap is the scarce resource. At 512 B the
/// 183 KB bundle is 357 `httpd_resp_send_chunk` calls, which is nothing.
const UI_CHUNK_BYTES: usize = 512;

/// What `GET /ui...` resolved to.
///
/// The three cases are distinct because conflating them is what makes a broken
/// SPA hard to diagnose: serving `index.html` for a missing `.js` produces a
/// `200`, a correct-looking HTML body, and a browser console full of MIME
/// errors — which looks like a broken app rather than a missing file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTarget {
    /// Serve this embedded file.
    File(&'static UiAsset),
    /// No such file, but the path has no extension: it is a client-side route,
    /// so serve the SPA shell and let the router sort it out.
    SpaRoute,
    /// No such file, and it names an extension: the caller sends a 404.
    Missing,
}

/// Resolve a request URI to an embedded asset.
///
/// `/ui` and `/ui/` are the shell. Everything else is looked up under `/ui`, and
/// an unknown extensionless path falls back to the shell — that is what makes a
/// deep link like `/ui/config/behavior` survive a page reload.
///
/// The lookup is a linear scan over a `&'static` table comparing borrowed
/// strings, so a request allocates nothing. Path traversal is unrepresentable
/// rather than filtered: there is no filesystem, and a path is either a key in
/// the table or it is nothing.
///
/// The query string is stripped before the extension test. Testing the URL
/// *including* the query would treat an SPA route with `?x=1.5` as an asset
/// request and 404 it.
#[must_use]
pub fn resolve_ui(uri: &str) -> UiTarget {
    let path = uri.split_once('?').map_or(uri, |(path, _)| path);
    let Some(rest) = path.strip_prefix("/ui") else {
        return UiTarget::Missing;
    };
    // `/ui*` is the registered template, so `/uixyz` reaches this handler too
    // and must not be treated as a child of `/ui`.
    if !rest.is_empty() && !rest.starts_with('/') {
        return UiTarget::Missing;
    }
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        return UiTarget::File(UI_INDEX);
    }
    if let Some(asset) = UI_ASSETS
        .iter()
        .find(|asset| asset.path.strip_prefix('/') == Some(rest))
    {
        return UiTarget::File(asset);
    }
    if rest.contains('.') {
        UiTarget::Missing
    } else {
        UiTarget::SpaRoute
    }
}

/// Write one embedded file, with its MIME type and encoding.
///
/// `esp-idf-svc` 0.53.0 cannot send a `Content-Length`: the `httpd_resp_set_len`
/// call is commented out in `server.rs:1056-1058`, so every response it writes
/// is chunked. The terminating zero-length chunk below is therefore not
/// optional — without it the browser waits for a body that never ends. This is
/// the same reason [`respond`] ends with `conn.write(&[])`.
///
/// Compressed assets are sent with `Content-Encoding: gzip` unconditionally
/// rather than negotiated. Only the gzip form is embedded, so there is no
/// identity variant to fall back to, and every browser advertises gzip. The
/// ceiling: a client that cannot decode gzip cannot load the UI, and `curl`
/// needs `--compressed`.
fn write_ui_file(
    conn: &mut EspHttpConnection<'_>,
    asset: &'static UiAsset,
) -> Result<(), EspError> {
    let mime = mime_for(asset.path);
    // Cache policy: the shell must not be cached or a rebuilt UI is unreachable
    // until the entry expires, while Vite's content-hashed asset names make the
    // rest safe to pin.
    let cache = if asset.path == UI_INDEX.path {
        "no-cache, no-store, must-revalidate"
    } else {
        "max-age=604800"
    };

    if asset.gzip {
        conn.initiate_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", mime),
                ("Content-Encoding", "gzip"),
                ("Cache-Control", cache),
            ],
        )?;
    } else {
        conn.initiate_response(
            200,
            Some("OK"),
            &[("Content-Type", mime), ("Cache-Control", cache)],
        )?;
    }

    for chunk in asset.bytes.chunks(UI_CHUNK_BYTES) {
        let _ = conn.write(chunk);
    }
    let _ = conn.write(&[]);
    Ok(())
}

/// `GET /ui` and everything under it.
///
/// Registered once, on the template `/ui*`. ESP-IDF's
/// `httpd_uri_match_wildcard` treats a trailing `*` as
/// "prefix", and a template *without* `*` or `?` still requires an exact length
/// match — so enabling [`Configuration::uri_match_wildcard`] for this one route
/// leaves all 23 exact `/api/*` handlers exactly as strict as they were.
fn serve_ui(mut req: Request<&mut EspHttpConnection<'_>>) -> Result<(), EspError> {
    let target = resolve_ui(req.uri());
    let conn = req.connection();
    match target {
        UiTarget::File(asset) => write_ui_file(conn, asset),
        UiTarget::SpaRoute => write_ui_file(conn, UI_INDEX),
        UiTarget::Missing => {
            // "File not found", deliberately not the shell: a missing asset must
            // be visible as a 404 rather than smuggled in as HTML the browser
            // then fails to parse as JavaScript.
            conn.initiate_response(404, Some("Not Found"), &[("Content-Type", "text/plain")])?;
            conn.write_all(b"File not found")
        }
    }
}

/// The routes this server registers, for the boot log and a route test.
#[must_use]
pub fn routes() -> Vec<(&'static str, Method)> {
    vec![
        ("/api/status", Method::Get),
        ("/api/health", Method::Get),
        ("/api/temperatures", Method::Get),
        ("/api/history", Method::Get),
        ("/api/nvs-debug", Method::Get),
        ("/api/parameter-help", Method::Get),
        ("/api/config", Method::Get),
        ("/api/config/download", Method::Get),
        ("/api/config/upload", Method::Post),
        ("/api/parameters", Method::Get),
        ("/api/parameters", Method::Post),
        // A real preflight handler, on a wildcard. It used to be advertised as
        // `("/api/status", Method::Options)` with nothing registered behind it,
        // so a preflight 404'd; see `register_preflight`.
        ("/api*", Method::Options),
        ("/api/setpoint", Method::Post),
        ("/api/steam", Method::Post),
        ("/api/pid", Method::Post),
        ("/api/backflush", Method::Post),
        ("/api/sleep", Method::Post),
        ("/api/wake", Method::Post),
        ("/api/scale/tare", Method::Post),
        ("/api/scale/calibration", Method::Post),
        ("/api/maintenance/reset-backflush-counter", Method::Post),
        ("/api/wifi-reset", Method::Post),
        ("/api/factory-reset", Method::Post),
        ("/api/restart", Method::Post),
        // All four OTA routes stay registered. `/api/ota/firmware` and
        // `/api/ota/filesystem` are implemented (real multipart upload);
        // `/api/ota/url` is deferred and answers 501. A 404 here would be
        // indistinguishable from a lost feature.
        ("/api/ota/status", Method::Get),
        ("/api/ota/firmware", Method::Post),
        ("/api/ota/filesystem", Method::Post),
        ("/api/ota/url", Method::Post),
        ("/events", Method::Get),
        ("/", Method::Get),
        ("/ui*", Method::Get),
    ]
}

/// What `/events` needs, behind the `user_ctx` pointer ESP-IDF hands back.
///
/// `register_raw_sse` can only pass **one** pointer, and the route now needs two
/// things: the stream itself and the credential check. They travel together in
/// one `Arc` rather than in a second global, so the check cannot be wired for
/// one route and forgotten for the other.
pub(crate) struct SseRoute {
    /// The broadcast state the handler attaches the detached request to.
    pub sse: Arc<Sse>,
    /// The same [`Auth`] every `fn_handler` route goes through.
    pub auth: Arc<Auth>,
}

/// A running HTTP server.
///
/// The `EspHttpServer` is moved into its own thread by the caller, because
/// `esp-idf-svc` registers every handler on an internal httpd task
/// (`server.rs:499-500`, "Registered Httpd server handler ... for URI") and the
/// handlers are `Send + 'static` closures over an `Arc<Shared>`. So there is
/// nothing to do here but hold the handle, and the value of this type is the
/// route table above plus the ownership statement.
pub struct Web {
    server: EspHttpServer<'static>,
    sse: Arc<Sse>,
    shared: Arc<Shared>,
    /// The one OTA update in flight, kept so [`Web::ota_session`] can hand it to
    /// the control task — which is the only task that may reboot.
    ota: Arc<crate::ota::Session>,
}

impl Web {
    /// Start the server and register every route.
    ///
    /// `commands` is the network→control channel. A handler never calls into the
    /// state machine: it formats a [`Command`], and the control task drains the
    /// queue at the top of its tick. `send` is non-blocking and drops on a full
    /// queue, which is the right answer for a request nobody is waiting on.
    ///
    /// # Errors
    ///
    /// `EspError` from `EspHttpServer::new` (`ESP_ERR_HTTPD_ALLOC_MEM` or
    /// `ESP_ERR_HTTPD_TASK` — both are "this machine has no heap for a web
    /// server", which is the running-out-of-room case and is not worth
    /// distinguishing at boot) or from any `httpd_register_uri_handler`, whose
    /// `ESP_ERR_HTTPD_HANDLERS_FULL` is the one worth naming: it fires at boot
    /// if the route table outgrows [`MAX_URI_HANDLERS`], and
    /// `the_route_table_fits_the_servers_handler_budget` is the test that
    /// catches it before a flash.
    ///
    /// `parameters` is the mailbox `POST /api/parameters` hands its accepted
    /// pairs to. It is a parameter and not a field of [`Shared`] because it is
    /// not telemetry: it is a request, it is drained by the control task rather
    /// than read by a handler, and [`Shared`] is documented as "the telemetry
    /// every handler reads". See [`crate::task::ParameterHandoff`] for why the
    /// pairs do not travel as a [`Command`].
    #[allow(
        clippy::too_many_lines,
        reason = "this IS a route table. 25 registrations with their handlers, \
                  one after another, is the clearest possible form of it; \
                  splitting it into `register_reads`/`register_commands` would \
                  hide the one property that matters, which is that the whole \
                  table is visible at once against `routes()`."
    )]
    /// `session` is the one OTA update in flight, shared with the control task
    /// so a successful upload's restart happens between ticks rather than inside
    /// a handler. Owned by `cc-firmware` and created at boot.
    pub fn start(
        shared: Arc<Shared>,
        session: &Arc<crate::ota::Session>,
        sse: Arc<Sse>,
        config: &Arc<Config>,
        nvs_description: &str,
        send: &Arc<dyn Fn(Command) + Send + Sync + 'static>,
        parameters: &Arc<crate::task::ParameterHandoff>,
    ) -> Result<Self, EspError> {
        // Every handler is `Send + 'static` (`server.rs:530-538`), so each one
        // captures its own `Arc::clone`. Taking these three by reference and
        // cloning into a local owned `Arc` once is what makes that possible
        // without an `unsafe fn` (`handler_nonstatic`, `server.rs:568`, which
        // this workspace denies).
        let send = Arc::clone(send);
        let parameters = Arc::clone(parameters);
        // Decided once, here. See [`Auth`].
        let auth = Arc::new(Auth::from_config(config));
        if auth.is_enforced() {
            info!("http: web authentication enabled (HTTP Basic, realm CleverCoffee)");
        }
        // `EspHttpServer::new` returns `EspIOError` (`server.rs:345`) while every
        // `httpd_*` call returns `EspError`, so the one conversion is here
        // rather than repeated in every handler.
        let mut server = EspHttpServer::new(&configuration()).map_err(|e| e.0)?;
        let ota = Arc::clone(session);

        // --- reads -------------------------------------------------------
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/status",
                Method::Get,
                move |mut req| {
                    // `free_heap()` is an FFI read, so the renderer takes it as
                    // an argument rather than calling it itself — see
                    // `cc_web::payload::status_json`. The bytes are unchanged.
                    let body = status_json(&shared.snapshot(), free_heap());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/health",
                Method::Get,
                move |mut req| {
                    let body = health_json(&shared.snapshot());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/temperatures",
                Method::Get,
                move |mut req| {
                    let body = temperatures_json(&shared.snapshot());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/history",
                Method::Get,
                move |mut req| {
                    // The same refusal below the heap floor that `respond_large`
                    // applies to every response over `MAX_JSON_BYTES` — and 600
                    // points is about 12 KB of JSON, so this route is the
                    // second-largest the server serves after
                    // `/api/parameters?filter=all`.
                    let body = history_json(&shared);
                    respond_large(req.connection(), &shared, &body)
                },
            )?;
        }
        {
            let described = String::from(nvs_description);
            register(
                &mut server,
                &auth,
                "/api/nvs-debug",
                Method::Get,
                move |mut req| {
                    // The store itself stays with the control task, which is the
                    // only writer; a handler gets the description string it needs and
                    // nothing that could write. `BlobConfigStore::describe` takes
                    // `&self` and this is its whole result — namespace, key, schema
                    // version and byte count — so nothing is lost and a `Send +
                    // 'static` handler needs no shared NVS handle at all.
                    // Both heap readings are arguments for the same reason
                    // `status_json`'s is; see `cc_web::payload::nvs_debug_json`.
                    let body = nvs_debug_json(
                        &described,
                        cc_config::SCHEMA.len(),
                        free_heap(),
                        min_free_heap(),
                    );
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            register(
                &mut server,
                &auth,
                "/api/parameter-help",
                Method::Get,
                |mut req| {
                    // Reads `?param=` and answers 422 / 404 / 200. Answering
                    // **200 with an error object** was finding 3.4: a client
                    // checking the status saw success and a client parsing the
                    // body saw a failure, and neither could tell it apart from
                    // the feature working. The body and the code are one value
                    // here so they cannot disagree again.
                    //
                    // `first_of` over the parsed query rather than a substring
                    // search, because `param` is percent-decoded by
                    // `cc_config::form::parse_form` — a dotted key arrives
                    // intact, which a byte comparison against `/api/parameters`
                    // would break the first time anything encoded it.
                    let fields = cc_config::form::parse_form(query_of(req.uri()));
                    let name = first_of(&fields, &["param"]);
                    let (status, body) = parameter_help(name.as_deref());
                    respond(req.connection(), status, &body)
                },
            )?;
        }
        {
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/config",
                Method::Get,
                move |mut req| {
                    // ADR-0002 decision 2: serialise ONCE. A `JsonDocument`
                    // copied to a `String` and copied again by the web server is
                    // three ~19 KB allocations, which is the abort the ADR exists
                    // to prevent. `json_export` returns a single `String` and
                    // `respond_large` streams it out in chunks.
                    let Ok(json) = cc_config::json_export(&config) else {
                        return respond(
                            req.connection(),
                            500,
                            &error_body("Failed to generate config"),
                        );
                    };
                    let body = json;
                    respond_large(req.connection(), &shared, &body)
                },
            )?;
        }
        {
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            // The **published** body, not the boot snapshot.
            //
            // `config` is the `Config` as it was when the server was built, so
            // rendering from it answers a question about the past: a
            // `POST /api/parameters` write lands in the control task's own
            // `Config` and in NVS, and this GET would still report the old
            // number. The human saw exactly that -- "the UI says success, the PID
            // is still disabled" -- and the UI was faithfully reporting a stale
            // answer.
            //
            // The control task publishes the body it would have served once a
            // heartbeat (`main.rs`), so the read reflects the running machine.
            // Before the first publish it falls back to the boot snapshot, which
            // is the right answer for the first second after boot.
            let parameters = Arc::clone(&parameters);
            register(
                &mut server,
                &auth,
                "/api/parameters",
                Method::Get,
                move |mut req| {
                    // `live()` hands back an `Arc` clone (a refcount bump),
                    // not a copy of ~8.8 KB. The fallback is built ONLY when
                    // nothing has been published yet, so the httpd task's own
                    // stack never holds two copies of the body -- and the guard
                    // is dropped before `respond_large` reads it.
                    let published = parameters.live();
                    let generated = published.is_none().then(|| parameters_json(&config));
                    let body: &str = match (&published, &generated) {
                        (Some(shared_body), _) => shared_body.as_str(),
                        (None, Some(body)) => body.as_str(),
                        // Unreachable: `generated` is `Some` exactly when
                        // `published` is `None`. Written out anyway so that if
                        // that ever stops being true it is a compile error rather
                        // than a 200-bytes-of-nothing response.
                        (None, None) => unreachable!("generated is Some when published is None"),
                    };
                    respond_large(req.connection(), &shared, body)
                },
            )?;
        }
        {
            // `GET /api/config/download`. The body is the same document
            // `/api/config` serves; what makes it a download is the
            // `Content-Disposition` header, which is the whole point of the
            // separate route.
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/config/download",
                Method::Get,
                move |mut req| {
                    let Ok(json) = cc_config::json_export(&config) else {
                        // The document did not fit the export buffer.
                        return respond(
                            req.connection(),
                            500,
                            &error_body("Failed to generate config"),
                        );
                    };
                    // `Content-Disposition: attachment; filename="config.json"`.
                    // Without it a browser renders the JSON instead of saving it,
                    // which is the whole reason this route exists separately.
                    respond_download(req.connection(), &shared, &json)
                },
            )?;
        }
        {
            // `POST /api/parameters` — the writer half of the same route as the
            // `GET` above. Two registrations rather than one `HTTP_ANY` because
            // `esp-idf-svc`'s `Method` has no "any" (embedded-svc 0.29
            // `http.rs:17-52`). A third method still gets the right status:
            // ESP-IDF's own "method not registered for this URI" handler answers
            // **405**, though its body is ESP-IDF's "Specified method is invalid
            // for this resource" rather than `{"error":"Method not allowed"}`.
            let handoff = Arc::clone(&parameters);
            register(
                &mut server,
                &auth,
                "/api/parameters",
                Method::Post,
                move |mut req| {
                    // **A body that did not fit is refused, not silently empty.**
                    // `drain_body_bounded` turns an over-cap body into `""`, which
                    // parses to zero pairs and answers `200 {"success":true,
                    // "message":"No parameters updated"}` -- a success for a
                    // request that changed nothing. Measured on a bench ESP32:
                    // the web UI's homepage save posts all 98 parameters
                    // (~2.7 KB), lands here, and reported success while the
                    // setpoint stayed put.
                    let Some(body) = drain_body_checked(req.connection(), MAX_PARAMETER_BODY_BYTES)
                    else {
                        warn!(
                            "http: /api/parameters body exceeded {MAX_PARAMETER_BODY_BYTES} B \
                             and was refused -- send only the parameters that changed"
                        );
                        return respond(
                            req.connection(),
                            413,
                            &error_body("request body too large; send only what changed"),
                        );
                    };
                    // `request->params()` is the query string *and* the body,
                    // in that order, because `AsyncWebServerRequest` appends the query
                    // args before the POST fields. So `?pid.enabled=1` and
                    // `pid.enabled=1` are the same request, and a request may carry
                    // both.
                    let mut pairs = cc_config::form::parse_form(query_of(req.uri()));
                    pairs.extend(cc_config::form::parse_form(&body));
                    if pairs.len() > MAX_PARAMETER_PAIRS {
                        return respond(
                            req.connection(),
                            400,
                            &error_body("too many parameters in one request"),
                        );
                    }

                    let verdict = classify_parameters(&pairs);
                    if let ParameterPost::Rejected { reasons, .. } = &verdict {
                        // One failure per parameter would flood the log; a single
                        // 400 that names none of them is a guessing game. Naming
                        // them is the whole diagnostic value of a rejected
                        // parameter.
                        for reason in reasons {
                            warn!("http: /api/parameters rejected {reason}");
                        }
                    }
                    let accepted = verdict.clone().into_pairs();
                    // Read the refusal only after this request was staged. A leftover
                    // flag would 400 an empty post.
                    let staged = !accepted.is_empty();
                    if staged && !handoff.stage_and_wait(accepted) {
                        // Either the mailbox was full, or the control task had not
                        // applied the request within the ack timeout. The response
                        // is about to say the parameters were saved, so this is the
                        // one case where this handler's `200` would be a lie: it is
                        // a 503 and the UI keeps the value it typed.
                        warn!("http: /api/parameters was not applied by the control task in time");
                        return respond(
                            req.connection(),
                            503,
                            &error_body("the control task did not apply the parameters, retry"),
                        );
                    }
                    // Finding #12: unsafe result, nothing stored. 200 would claim a save.
                    if staged {
                        if let Some((keys, n)) = handoff.take_refused() {
                            let implicated = keys[..n].join(", ");
                            return respond(
                                req.connection(),
                                400,
                                &error_body(&format!(
                                    "refused: the resulting configuration would be unsafe \
                                     (implicated: {implicated}), so nothing was written."
                                )),
                            );
                        }
                    }
                    let (status, payload) = verdict.response();
                    // A `200 {"success":true}` on a write that cannot affect the
                    // running machine is a lie the human reported. Naming the
                    // keys that need a reboot turns it into an answer.
                    let reboot = verdict.reboot_required();
                    if !reboot.is_empty() {
                        let keys = reboot
                            .iter()
                            .map(|k| format!("\"{k}\""))
                            .collect::<Vec<_>>()
                            .join(",");
                        let body = format!(
                            "{{\"success\":true,\"message\":\"Parameters updated and saved\",\
\"requiresReboot\":true,\"requiresRebootKeys\":[{keys}],\"reason\":\"read once at startup\"}}"
                        );
                        return respond(req.connection(), status, &body);
                    }
                    respond(req.connection(), status, payload)
                },
            )?;
        }
        {
            // `POST /api/config/upload`.
            //
            // **The body is `application/json`, not multipart.**
            // `ui/packages/frontend/src/pages/SystemPage.tsx:180-183` posts the
            // selected file's text with `Content-Type: application/json` and no
            // boundary. A multipart reader here would answer the live "Upload
            // configuration" button with 400.
            //
            // The four outcomes, and which is which:
            //
            // * **over the transport cap** → `413`, and nothing is parsed. See
            //   [`drain_body_checked`] for why this cannot truncate.
            // * **not a top-level object** → `400`.
            // * **rejected by the reader** → `400`, and the offending keys are
            //   named in the log rather than the body, because the body is what
            //   the UI shows.
            // * **applied** → `200` and `restart: true`, which tells the browser
            //   to call `POST /api/restart`.
            let handoff = Arc::clone(&parameters);
            register(
                &mut server,
                &auth,
                "/api/config/upload",
                Method::Post,
                move |mut req| {
                    let Some(body) = drain_body_checked(req.connection(), MAX_CONFIG_UPLOAD_BYTES)
                    else {
                        // A 16 KB cap makes an over-long body a `413`. Nothing is
                        // parsed, so nothing can be half-applied; and the tail
                        // left unread is purged by ESP-IDF itself
                        // (`httpd_req_delete`, `httpd_parse.c:841-855`).
                        return respond(
                            req.connection(),
                            413,
                            &upload_response(
                                false,
                                "Configuration is too large; the limit is 16384 bytes",
                            ),
                        );
                    };

                    // `cc_config::json::document_pairs` is the whole validation:
                    // size, syntax, top-level object, flat dotted keys, one known
                    // key, every value in range — and it is **total**, so a
                    // document with nine good keys and one impossible one
                    // produces no pairs at all rather than nine.
                    let pairs = match cc_config::document_pairs(&body) {
                        Ok(pairs) => pairs,
                        Err(rejection) => {
                            // The reader names two of these exactly; the rest
                            // share one "invalid values" message.
                            if let cc_config::ImportError::InvalidValues { rejected } = &rejection {
                                for value in rejected {
                                    warn!(
                                        "http: /api/config/upload rejected {}: {}",
                                        value.key,
                                        cc_config::json::describe_reason(value.reason)
                                    );
                                }
                            }
                            let message = match &rejection {
                                cc_config::ImportError::NotAnObject => {
                                    "JSON body must be a top-level object"
                                }
                                cc_config::ImportError::FlatDottedKeys { .. } => {
                                    "Flat dotted-key JSON is not supported. Use nested objects (see Download Config)."
                                }
                                _ => "Configuration validation failed. Use a file from Download Config.",
                            };
                            return respond(
                                req.connection(),
                                400,
                                &upload_response(false, message),
                            );
                        }
                    };

                    // Belt and braces, and the reason this route can promise it
                    // never half-applies.
                    //
                    // `document_pairs` renders each value into the string form
                    // `cc_config::assign::parse` accepts, so every pair is
                    // already good by the only rule that matters — and that is a
                    // *property*, pinned by `cc-config`'s
                    // `an_every_pair_from_an_upload_is_accepted_by_the_one_writer`,
                    // not a promise. If a future parameter kind ever breaks the
                    // rendering, `classify_parameters` turns a silent partial
                    // apply into a visible `400` here, and refuses the **whole**
                    // document rather than staging the part that happened to
                    // parse — which is the property `/api/parameters` cannot
                    // offer, because that route deliberately applies what it can.
                    let verdict = classify_parameters(&pairs);
                    if let ParameterPost::Rejected { reasons, .. } = &verdict {
                        for reason in reasons {
                            warn!("http: /api/config/upload rejected {reason}");
                        }
                        return respond(
                            req.connection(),
                            400,
                            &upload_response(
                                false,
                                "Configuration validation failed. Use a file from Download Config.",
                            ),
                        );
                    }

                    // `document_pairs` answers `NoKnownParameters` when the
                    // document names nothing this firmware knows, so `pairs` is
                    // non-empty on every path that reaches here — there is no
                    // "nothing to do" case to skip the control task for, as
                    // there is on `/api/parameters`.
                    if !handoff.stage_and_wait(pairs) {
                        // The same 503 `POST /api/parameters` answers
                        // (`:1873-1882`): the response is about to say the
                        // configuration was saved, so it must not say it unless
                        // the control task applied it.
                        warn!(
                            "http: /api/config/upload was not applied by the control task in time"
                        );
                        return respond(
                            req.connection(),
                            503,
                            &upload_response(
                                false,
                                "the control task did not apply the configuration, retry",
                            ),
                        );
                    }
                    // Same 400 as `/api/parameters`. Upload is all or nothing. Finding #12.
                    if let Some((keys, n)) = handoff.take_refused() {
                        let implicated = keys[..n].join(", ");
                        return respond(
                            req.connection(),
                            400,
                            &upload_response(
                                false,
                                &format!(
                                    "refused: the resulting configuration would be unsafe \
                                     (implicated: {implicated}), so nothing was written"
                                ),
                            ),
                        );
                    }
                    info!("http: /api/config/upload validated and applied");
                    respond(
                        req.connection(),
                        200,
                        &upload_response(true, "Configuration validated and applied successfully."),
                    )
                },
            )?;
        }

        // --- CORS preflight ------------------------------------------------
        register_preflight(&mut server)?;

        // --- commands ----------------------------------------------------
        register_command(
            &mut server,
            &auth,
            "/api/setpoint",
            Arc::clone(&send),
            parse_setpoint,
        )?;
        // Three toggle routes. All three read no field; all three are what the
        // UI's buttons call with a bare POST.
        register_toggle(
            &mut server,
            &auth,
            &shared,
            Arc::clone(&send),
            &Toggle {
                uri: "/api/steam",
                key: "steamMode",
                toggled: Command::ToggleSteam,
                explicit: Command::SetSteam,
                current: |t| t.steam_mode,
            },
        )?;
        register_toggle(
            &mut server,
            &auth,
            &shared,
            Arc::clone(&send),
            &Toggle {
                uri: "/api/pid",
                key: "pidEnabled",
                toggled: Command::TogglePid,
                explicit: Command::SetPid,
                current: |t| t.pid_enabled,
            },
        )?;
        register_toggle(
            &mut server,
            &auth,
            &shared,
            Arc::clone(&send),
            &Toggle {
                uri: "/api/backflush",
                key: "backflushOn",
                toggled: Command::ToggleBackflush,
                explicit: Command::SetBackflush,
                current: |t| t.backflush_mode,
            },
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/sleep",
            Arc::clone(&send),
            Command::Sleep,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/wake",
            Arc::clone(&send),
            Command::Wake,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/scale/tare",
            Arc::clone(&send),
            Command::Tare,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/scale/calibration",
            Arc::clone(&send),
            Command::Calibrate,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/maintenance/reset-backflush-counter",
            Arc::clone(&send),
            Command::ResetBackflushCounter,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/wifi-reset",
            Arc::clone(&send),
            Command::WifiReset,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/factory-reset",
            Arc::clone(&send),
            Command::FactoryReset,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/restart",
            Arc::clone(&send),
            Command::Restart,
        )?;

        // --- OTA -----------------------------------------------------------
        //
        // Four routes: two binary uploads, a URL update and a status document.
        // Three are implemented here and the URL update is **not** — see
        // `ota_upload_route` for what was left out and why.
        //
        // All four stay registered either way, because the UI has a live tab
        // that calls them and a 404 is indistinguishable from a lost feature.
        {
            let session = Arc::clone(session);
            register(
                &mut server,
                &auth,
                "/api/ota/status",
                Method::Get,
                move |mut req| respond(req.connection(), 200, &session.status().status_json()),
            )?;
        }
        for (uri, kind) in [
            ("/api/ota/firmware", Kind::Firmware),
            ("/api/ota/filesystem", Kind::Filesystem),
        ] {
            // `Arc::clone` per iteration: `server` borrows `auth` and the handlers
            // capture owned clones, and a loop that moved one `Arc` out would take
            // the value on its first pass.
            let session = Arc::clone(session);
            let send = Arc::clone(&send);
            let shared = Arc::clone(&shared);
            register(&mut server, &auth, uri, Method::Post, move |req| {
                ota_upload_route(&session, &shared, &send, kind, req)
            })?;
        }
        {
            // `/api/ota/url` is registered and refuses. Implementing it needs an
            // HTTP client, a second long-lived task and a restart-on-failure
            // path, for a feature whose every part is already reachable from a
            // browser upload. Not built, and this answer says so rather than
            // 404ing.
            register(
                &mut server,
                &auth,
                "/api/ota/url",
                Method::Post,
                move |mut req| {
                    respond(
                        req.connection(),
                        501,
                        &unavailable_json("OTA from a URL", "R3-15"),
                    )
                },
            )?;
        }

        // --- static ------------------------------------------------------
        {
            register(&mut server, &auth, "/", Method::Get, |mut req| {
                // Redirect `/` to `/ui/`.
                let conn = req.connection();
                conn.initiate_response(302, Some("Found"), &[("Location", "/ui/")])?;
                conn.write_all(b"")
            })?;
        }
        {
            // One handler for the shell, the assets and the client-side routes.
            // The `*` is what makes `/ui/brew` reach it at all; see [`serve_ui`].
            register(&mut server, &auth, "/ui*", Method::Get, serve_ui)?;
        }

        // --- SSE ---------------------------------------------------------
        // Registered as a **raw** handler, not through `fn_handler`. That is the
        // whole of the double-response fix: `fn_handler` wraps every closure in
        // `to_native_handler`, which calls `complete()` after the handler
        // returns, and `complete()` writes a complete response
        // (`httpd_resp_send(.., 0)` with `Content-Length: 0`) that ends the
        // response before the broadcaster has sent a frame — then the detached
        // request's chunked response follows as a second response on the same
        // socket. See `crate::web_async::register_raw_sse` for the full
        // argument and the wire capture.
        //
        // The handler still returns immediately, which is the other half of the
        // requirement: ESP-IDF's httpd is one task, so a handler that does not
        // return is a server that does not serve. `spawn_broadcaster` below
        // owns the writing.
        crate::web_async::register_raw_sse(
            esp_idf_svc::handle::RawHandle::handle(&server),
            Arc::new(SseRoute {
                sse: Arc::clone(&sse),
                auth: Arc::clone(&auth),
            }),
        )?;

        // --- the JSON 404 --------------------------------------------------
        // One `httpd_register_err_handler` for `HTTPD_404_NOT_FOUND`, which
        // ESP-IDF calls only when no URI handler matched. A `/api/` path gets
        // JSON; anything else keeps ESP-IDF's plain text.
        //
        // **The one place this firmware's 404 is not behind `Auth`.** The
        // authentication middleware covers every registered route, and this
        // error handler is not a route, so an unknown `/api/` path answers `404`
        // rather than `401`. Left as it is deliberately: the handler carries no
        // `user_ctx` and no `Arc<Auth>`, and the alternative would be a raw
        // handler reading the `Authorization` header — a second credential path,
        // written a second time, guarding a response that says only "no such
        // route". Every route that exists is behind `Auth`; the set of routes
        // that do not exist is not a secret.
        crate::web_async::register_raw_api_not_found(esp_idf_svc::handle::RawHandle::handle(
            &server,
        ))?;

        // The broadcaster is the only writer of `/events`, and it is not the
        // httpd task. Started after the routes so a client cannot connect to a
        // stream nobody is servicing.
        spawn_broadcaster(Arc::clone(&sse))?;

        info!(
            "http: listening on port {HTTP_PORT}, {} routes, {} B handler budget",
            routes().len(),
            MAX_URI_HANDLERS
        );
        // The bundle is embedded rather than mounted, so this line is the only
        // proof at runtime that the UI is in the image at all -- and the two
        // numbers are the ones to check against a Vite build.
        info!("http: web UI embedded in flash: {UI_FILE_COUNT} files, {UI_TOTAL_BYTES} B");
        Ok(Self {
            server,
            sse,
            shared,
            ota,
        })
    }

    /// The OTA session, for the control task.
    ///
    /// The control task polls [`crate::ota::Session::take_restart`] between ticks,
    /// which is why the session is reachable from here rather than owned by the
    /// route: a handler must not be the thing that reboots the machine, for the
    /// same reason `POST /api/restart` is a `Command`.
    #[must_use]
    pub fn ota_session(&self) -> &Arc<crate::ota::Session> {
        &self.ota
    }

    /// The SSE counters, for the boot log and `/api/status`.
    #[must_use]
    pub fn sse(&self) -> &Sse {
        &self.sse
    }

    /// The shared state, for the control task.
    #[must_use]
    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// The `EspHttpServer` handle.
    ///
    /// Dropping it stops the server (`server.rs:732-736`), which is why the
    /// firmware must not let it fall out of scope: the httpd task would be torn
    /// down while a browser is mid-request.
    #[must_use]
    pub fn server(&self) -> &EspHttpServer<'static> {
        &self.server
    }

    /// Push one `new_temps` event to every connected client.
    ///
    /// A `ping` and then a `new_temps` carrying exactly the `/api/temperatures`
    /// body. Both are sent: the `ping` is a liveness marker the UI uses to show
    /// that the stream is alive independently of the data, and dropping it would
    /// make a stream that is connected but idle indistinguishable from one that
    /// has died.
    pub fn send_temp_event(&self) {
        let snapshot = self.shared.snapshot();
        let payload = temperatures_json(&snapshot);
        self.sse.broadcast(Sse::frame("new_temps", &payload));
    }

    /// Push one `weight` event.
    pub fn send_weight_event(&self) {
        let snapshot = self.shared.snapshot();
        let payload = weight_json(&snapshot);
        self.sse.broadcast(Sse::frame("weight", &payload));
    }
}

/// The broadcaster task: everything `/events` writes, on a task that is not
/// the httpd task.
///
/// This function is why `/api/*` latency is independent of how many streams are
/// open. ESP-IDF's httpd is one task for the whole server (`httpd_main.c:533`
/// creates the single `httpd` thread; `httpd_thread` at `:329` is `while (1) {
/// httpd_server(hd); }` over one `select()`), so a handler that loops while it
/// streams is a server that serves nobody. Measured on hardware with one browser
/// tab holding a stream open: 55 of 60 sequential `GET /api/parameters` requests
/// timed out at the client's 5 s limit, and the five that did get through took
/// 6–7 s.
///
/// The handler registers a detached request and returns; this task does the
/// writing. Nothing in this loop runs on the httpd task.
///
/// The 50 ms sleep is the loop's only cost when idle. It is not a busy-wait on
/// the httpd task, and it is shorter than it needs to be for correctness — it
/// only bounds how late a due frame is noticed.
///
/// # Errors
///
/// Never, in practice, and that is deliberate: a thread that cannot be created
/// is not a condition the firmware can serve around, but it is also not a reason
/// to refuse to boot. The web API works without `/events`, so a failure here is
/// logged and the machine comes up. The `Result` is kept because the caller is
/// already in a `Result`-returning function and inventing a second error type
/// for "the SSE stream will not run" would be worse than an always-`Ok` one.
pub fn spawn_broadcaster(sse: Arc<Sse>) -> Result<(), EspError> {
    let spawned = std::thread::Builder::new()
        .name("sse-broadcast".into())
        .stack_size(SSE_BROADCASTER_STACK_BYTES)
        .spawn(move || broadcaster(&sse));
    match spawned {
        Ok(_) => {
            info!("sse: broadcaster task started");
            Ok(())
        }
        Err(e) => {
            warn!("sse: no broadcaster task ({e}) — /events will not stream");
            Ok(())
        }
    }
}

/// Transport frames to every client. It originates only the keepalive.
///
/// It reads no machine state, and deliberately so: the producer is the control
/// task (`broadcast_temps`), and giving the broadcaster a `Shared` would invite
/// the next reader to start generating telemetry here — the coupling the
/// command/telemetry split exists to prevent.
fn broadcaster(sse: &Sse) {
    loop {
        let now = now_ms();

        // Take the pushed frames out first, so the lock is not held across the
        // client writes below. Frames go to every client before the cadence
        // frames do, so a pushed `weight` is not stuck behind a `new_temps`.
        let pushed: VecDeque<String> = if let Ok(mut m) = sse.mailbox.lock() {
            std::mem::take(&mut *m)
        } else {
            VecDeque::new()
        };

        let Ok(mut locked) = sse.clients.lock() else {
            esp_idf_hal::delay::FreeRtos::delay_ms(SSE_POLL_MS);
            continue;
        };
        let mut clients = std::mem::take(&mut *locked);
        drop(locked);

        let mut survivors = Vec::with_capacity(clients.len());
        for mut client in clients.drain(..) {
            let mut ok = true;

            if client.await_hello {
                let hello = Sse::frame("hello", "{\"connected\":true}");
                ok = client.req.write(&hello).is_ok();
                if ok {
                    sse.sent.fetch_add(1, Ordering::SeqCst);
                    client.await_hello = false;
                }
            }

            if ok {
                for frame in &pushed {
                    if client.req.write(frame).is_err() {
                        ok = false;
                        sse.dropped.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                    sse.sent.fetch_add(1, Ordering::SeqCst);
                    client.last_write_ms = now;
                }
            }

            // The keepalive is the broadcaster's own, and the only thing it
            // originates: events come from the control task through
            // `broadcast_temps`. So the two producers are the control task
            // (events) and this task (liveness), and neither duplicates the
            // other. It is measured from the last write of any kind, so a stream
            // that is receiving events does not also emit keepalives.
            if ok && now.wrapping_sub(client.last_write_ms) >= SSE_KEEPALIVE_MS {
                let keepalive = Sse::keepalive();
                if client.req.write(&keepalive).is_err() {
                    ok = false;
                } else {
                    sse.sent.fetch_add(1, Ordering::SeqCst);
                    client.last_write_ms = now;
                }
            }

            if ok {
                survivors.push(client);
            } else {
                // A closed client is the normal end of an SSE stream, not an
                // error. Counting it is what makes "zero drops over ten
                // minutes" a measurement.
                sse.dropped.fetch_add(1, Ordering::Relaxed);
                client.req.complete();
            }
        }

        // Put the survivors back, and keep any client that connected while this
        // task had the list detached. The handler appends, so the fresh one
        // goes after the survivors; ordering does not matter because every
        // client gets the same frames.
        if let Ok(mut slot) = sse.clients.lock() {
            survivors.append(&mut *slot);
            *slot = survivors;
        } else {
            // The lock is poisoned, so nobody can be using the list. Completing
            // every request is the only way to hand the sessions back; leaking
            // them would eventually stop httpd accepting connections at all
            // (`esp_http_server.h:864-866`).
            for client in survivors {
                client.req.complete();
            }
        }

        esp_idf_hal::delay::FreeRtos::delay_ms(SSE_POLL_MS);
    }
}

/// Register a `POST` handler that takes no parameters and emits a command.
///
/// The eight no-argument POSTs. The `202 Accepted` is deliberate and different
/// from a `200`: the command has been *queued*, not performed. A `200` on
/// `POST /api/factory-reset` would be a lie — the machine is about to reboot.
fn register_flag(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    uri: &'static str,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    command: Command,
) -> Result<(), EspError> {
    register(server, auth, uri, Method::Post, move |mut req| {
        let _ = drain_body(req.connection());
        send(command);
        respond(req.connection(), 202, "{\"accepted\":true}")
    })
}

/// Toggle route metadata kept together to prevent mismatched command/value pairs.
///
/// An absent field queues a toggle resolved against live control state, avoiding
/// two rapid requests toggling the same stale snapshot. Explicit query/body
/// `value` or `on` instead selects a set command. Return 200 with `success` and
/// the route's resulting boolean key, as clients expect.
#[derive(Clone, Copy)]
struct Toggle {
    /// The route, `/api/…`.
    uri: &'static str,
    /// The key the response reports.
    key: &'static str,
    /// The command a bare `POST` emits. Carries no value: see
    /// [`Command::ToggleSteam`].
    toggled: Command,
    /// Wraps an explicit `value`/`on` field into the absolute command.
    explicit: fn(bool) -> Command,
    /// Reads the current value out of the telemetry snapshot, for the response.
    current: fn(&Telemetry) -> bool,
}

/// Register one [`Toggle`] route.
fn register_toggle(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    shared: &Arc<Shared>,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    toggle: &Toggle,
) -> Result<(), EspError> {
    let Toggle {
        uri,
        key,
        toggled: on_toggle,
        explicit,
        current,
    } = *toggle;
    let shared = Arc::clone(shared);
    register(server, auth, uri, Method::Post, move |mut req| {
        let mut fields = cc_config::form::parse_form(query_of(req.uri()));
        let body = drain_body(req.connection());
        fields.extend(cc_config::form::parse_form(&body));
        // `first_of` scans in *name* order, so `value` anywhere beats `on`
        // anywhere.
        let (chosen, value) = match first_of(&fields, &["value", "on"]) {
            // `start` is the one non-boolean field, and only on
            // `/api/backflush`; see [`Command::StartBackflush`].
            Some(field) if uri == "/api/backflush" && field == "start" => {
                (Command::StartBackflush, None)
            }
            Some(field) => (explicit(parse_flag(&field)), None),
            // No field at all: a toggle.
            None => (on_toggle, Some(!current(&shared.snapshot()))),
        };
        // **The value is read after the command has been applied.**
        //
        // Computing it *before* sending — `!current(&snapshot)` for a bare
        // toggle, from the last telemetry the control task published — makes the
        // answer describe the machine as it was, and a client that trusts it
        // writes the old value into its own state. The report was "I press the
        // toggle, the device switches, and the switch stays active until I
        // refresh" — the device was right and the answer was a lie.
        //
        // The wait is bounded (400 ms) and the fallback is the requested value
        // rather than a panic: a stalled control task must not hang a browser,
        // and a refetch is the client's other route to truth.
        let before = shared.applied();
        send(chosen);
        let settled = shared.wait_applied(before);
        let value = if settled {
            current(&shared.snapshot())
        } else {
            warn!(
                "http: {uri} answered from the requested value — the control \
                     task has not applied the command after {COMMAND_ACK_TIMEOUT_MS} ms"
            );
            value.unwrap_or_else(|| explicit_value(&chosen))
        };
        let body = format!("{{\"success\":true,\"{key}\":{value}}}");
        respond(req.connection(), 200, &body)
    })
}

/// Register a `POST` handler that parses one field into a command.
fn register_command(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    uri: &'static str,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    parse: fn(&str) -> Option<Command>,
) -> Result<(), EspError> {
    register(server, auth, uri, Method::Post, move |mut req| {
        // A field is present or it is not, and 0 is a valid setpoint, so presence
        // is what matters, not truthiness.
        //
        // The query string is read too, and the **body wins** where both carry
        // the field. The toggle route reads no field at all, so there is nothing
        // for `?on=0` to match there, and every script, the UI's own button and
        // the integration checklist spell it `?on=0` or `?on=1`. Accepting both
        // is what `curl -X POST '.../api/pid?on=0'` needs; the body is checked
        // first because a form post that also carries a stale query string should
        // do what the form says.
        let mut fields = cc_config::form::parse_form(query_of(req.uri()));
        let body = drain_body(req.connection());
        fields.extend(cc_config::form::parse_form(&body));
        let value = first_of(&fields, &["value", "on"]);
        let Some(value) = value else {
            return respond(req.connection(), 400, &error_body("missing `value`"));
        };
        match parse(&value) {
            Some(command) => {
                send(command);
                respond(req.connection(), 202, "{\"accepted\":true}")
            }
            None => respond(req.connection(), 400, &error_body("value out of range")),
        }
    })
}

/// Read a request body into a `String`, bounded.
///
/// [`crate::telnet::LINE_BUFFER_BYTES`] (256), the same bound the log stream
/// uses. A `POST /api/setpoint` body is `value=94.5` — nine characters — and a
/// body that does not fit is a client bug or an attack, not a large legitimate
/// request. An unbounded body on a 320 KB heap is a denial of service with three
/// words.
fn drain_body(conn: &mut EspHttpConnection<'_>) -> String {
    drain_body_bounded(conn, crate::telnet::LINE_BUFFER_BYTES)
}

/// Read a request body into a `String`, bounded by `limit`.
///
/// A second bound rather than one, because the two routes that read a body need
/// different ones: a `POST /api/pid` body is six characters and a
/// `POST /api/parameters` body is a *list* of them, twenty of which is a
/// plausible settings form. One bound for both would be either too small for the
/// second or too generous for the first, and the first is the one a stranger can
/// reach.
fn drain_body_bounded(conn: &mut EspHttpConnection<'_>, limit: usize) -> String {
    drain_body_checked(conn, limit).unwrap_or_default()
}

/// Read a request body, **reporting** whether it did not fit.
///
/// [`drain_body_bounded`] truncates silently, which is the right answer for a
/// form body — a truncated `a=1&b=2` is a request that sets fewer parameters,
/// and `POST /api/parameters` re-validates every pair it acts on, so nothing
/// wrong reaches the machine.
///
/// It is the **wrong** answer for `POST /api/config/upload`, and the difference
/// is the whole reason this exists. An upload is a *document*: it is applied
/// key by key, and a document cut short mid-way is a document whose keys are
/// individually valid and collectively a different machine. Truncating at the
/// cap and handing the remainder to the reader would apply **half a
/// configuration** — a new PID gain with the old emergency cutoff — to a machine
/// that may be mid-shot. So this returns `None` the moment the body exceeds
/// `limit`, and the caller refuses without parsing.
fn drain_body_checked(conn: &mut EspHttpConnection<'_>, limit: usize) -> Option<String> {
    let mut body = String::new();
    let mut buf = [0u8; 128];
    loop {
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return Some(body),
            Ok(n) => {
                if body.len() + n > limit {
                    return None;
                }
                body.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
    }
}

impl Sse {
    /// Note that a push was attempted, whether or not a client received it.
    ///
    /// Counted so that "the control task is producing events" and "a client is
    /// receiving them" are two different numbers.
    pub fn note_push_attempt(&self) {
        self.pushed.fetch_add(1, Ordering::Relaxed);
    }

    /// Push a frame to every connected client.
    ///
    /// Every producer pushes, and every client receives. It cannot write from the
    /// caller's task — the sockets belong to the broadcaster — so it hands the
    /// frame over through a bounded mailbox, and the broadcaster drains it on its
    /// next pass (at most [`SSE_POLL_MS`] later).
    ///
    /// A full mailbox drops the **newest** frame and counts it in
    /// [`Sse::dropped_frames`], rather than blocking the caller. The caller is
    /// the control task, and blocking it to serve a browser tab is the same
    /// mistake as blocking the httpd task, one layer over.
    pub fn broadcast(&self, frame: String) {
        let Ok(mut mailbox) = self.mailbox.lock() else {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if mailbox.len() == SSE_MAILBOX_DEPTH && mailbox.pop_front().is_some() {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
        mailbox.push_back(frame);
    }
}

/// Why a streamed upload stopped, and what the client is told.
///
/// A struct rather than a `Result<(), StatusMessage>` because the three cases
/// carry **three different HTTP statuses** — a malformed envelope is a `400`, a
/// flash failure is a `500`, and a mid-stream write failure is a `500` with a
/// different word. Collapsing them to one error would force the caller to
/// re-derive which.
struct OtaFailure {
    /// 400 for a malformed body, 500 for a device-side failure.
    status: u16,
    /// The `StatusMessage` for `GET /api/ota/status`.
    message: cc_web::ota::StatusMessage,
    /// The body `result.message`, which the UI shows verbatim.
    text: &'static str,
}

/// Read the socket to its end, stripping the envelope and writing each run to
/// flash.
///
/// Split out of [`ota_upload_route`] so that function is the *checks* and this
/// is the *loop*, which is the only honest way to keep both readable: the
/// ordering of the checks is the safety argument, and a 155-line function does
/// not let a reader see it.
///
/// # The memory claim, at the one place it happens
///
/// `buf` is the **only** per-upload memory and it is a stack array. The sink
/// closure writes straight through to `esp_ota_write`, so no chunk is ever held
/// twice, and `PartReader`'s hold-back is a `Vec` whose capacity is reached on
/// the first push and never grows. A 1.6 MB image therefore costs the same heap
/// as a 4 KB one — which is the difference between this endpoint and an OOM
/// abort on a part with ~320 KB.
///
/// # Why a flash error does not unwind
///
/// The sink is `FnMut`, so it cannot return. A flash failure is recorded in
/// `failure` and the loop stops on the next chunk; `panic!` inside the closure
/// would unwind through `esp_ota_write`'s C frame, which is not a thing Rust can
/// do safely.
fn ota_stream(
    session: &crate::ota::Session,
    writer: &mut crate::ota::Writer,
    reader: &mut cc_web::ota::PartReader,
    req: &mut Request<&mut EspHttpConnection<'_>>,
) -> Result<(), OtaFailure> {
    let mut buf = [0u8; crate::ota::OTA_CHUNK_BYTES];
    let failed = false;
    loop {
        let read = match req.connection().read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut flash_error = None;
        let parsed = reader.push(&buf[..read], &mut |run| {
            if failed || flash_error.is_some() {
                return;
            }
            if let Err(err) = writer.write(run) {
                error!(
                    "ota: flash write failed at byte {}: {err}",
                    writer.written()
                );
                flash_error = Some(err);
            }
        });
        if let Err(err) = parsed {
            return Err(OtaFailure {
                status: 400,
                message: cc_web::ota::StatusMessage::Read(err),
                text: err.message(),
            });
        }
        if flash_error.is_some() {
            return Err(OtaFailure {
                status: 500,
                message: cc_web::ota::StatusMessage::Flash,
                text: "Write failed",
            });
        }
        // 0 rather than a guess for `total`: a browser's `Content-Length` covers
        // the multipart envelope, so it is not the image length. Only bytes
        // received are reported.
        session.note_progress(cc_web::ota::Phase::Uploading, writer.written(), 0);
    }
    Ok(())
}

/// Everything that can only be checked once the body has ended.
///
/// Four checks, in this order, and the order is the argument:
///
/// 1. **Termination.** The closing delimiter arrived. Checked first because a
///    truncated body makes every later check meaningless.
/// 2. **The extension rule**, on the filename the part headers carried. After
///    termination, because the filename is only known once the header block has
///    been read, and the payload had to be consumed either way to answer the
///    request.
/// 3. **The size rule**, against the partition — *after* the erase, which is the
///    only order available and the reason the `min_accepted` floor exists at
///    all: it cannot save the erase, only refuse to finalise.
///
/// # Why the extension check is not before the erase
///
/// It would be nicer. It cannot be: a `multipart/form-data` filename lives in the
/// part's headers, and the headers arrive *inside* the body, so the only way to
/// learn it is to read at least that far.
fn ota_validate(
    reader: &cc_web::ota::PartReader,
    written: usize,
    kind: Kind,
) -> Result<(), OtaFailure> {
    if let Err(err) = reader.finish() {
        return Err(OtaFailure {
            status: 400,
            message: cc_web::ota::StatusMessage::Read(err),
            text: err.message(),
        });
    }
    if let Some(name) = reader.filename() {
        if !cc_web::ota::extension_allowed(name, kind) {
            return Err(OtaFailure {
                status: 400,
                message: cc_web::ota::StatusMessage::BadExtension(kind),
                text: cc_web::ota::StatusMessage::BadExtension(kind).message(),
            });
        }
    }
    if !cc_web::ota::fits(kind, written) {
        return Err(OtaFailure {
            status: 400,
            message: cc_web::ota::StatusMessage::BadSize(kind),
            text: cc_web::ota::StatusMessage::BadSize(kind).message(),
        });
    }
    Ok(())
}

/// The refusal text for an OTA request the control task never answered.
///
/// Its own message rather than a `FlashRefusal`'s, because there is no state to
/// blame: the control task did not get to a decision within
/// [`COMMAND_ACK_TIMEOUT_MS`], or the command was lost. The UI prints
/// `result.message` verbatim, so it is operator-facing text and not a log line,
/// and it says what to do about it.
const NO_ANSWER_REFUSAL: &str =
    "The machine did not confirm a safe state for the update. Try again.";

/// The refusal text for a session another upload already holds.
const SESSION_BUSY_REFUSAL: &str =
    "OTA update already in progress. Please wait for current update to complete.";

/// Checks 1–3 of [`ota_upload_route`], in order: admission on the published
/// snapshot, the claim, and the safe hardware shutdown the control task applies
/// against its **live** state.
///
/// Split out of the route because the three together are the safety argument, and
/// the route is otherwise over [`clippy::too_many_lines`]. Returns the
/// operator-facing refusal text, or `Ok(())` when the flash may begin — which is
/// to say, once the control task has confirmed against the state the machine is
/// actually in that a `SafeHardwareShutdown` is now applied.
fn ota_open_session(
    session: &crate::ota::Session,
    shared: &Arc<Shared>,
    send: &Arc<dyn Fn(Command) + Send + Sync + 'static>,
    kind: Kind,
) -> Result<(), &'static str> {
    // 1. Admission, from the state the control task published. A snapshot is at
    //    most one control period stale, which is the same staleness every other
    //    read-only handler accepts — and it is why this is a *first filter* and
    //    not the decision. Check 3 is.
    let telemetry = shared.snapshot();
    // An id the firmware does not know is treated as "not safe to flash", which
    // is the conservative direction: an unrecognised state might be one that
    // flows water. `from_id`'s `None` has to mean something, and refusal is what
    // it means here.
    let admission = match u16::try_from(telemetry.machine_state)
        .ok()
        .and_then(cc_domain::state::MachineState::from_id)
    {
        Some(state) => cc_machine::ota::admit(state),
        None => cc_machine::ota::Admission::Refused(cc_machine::ota::FlashRefusal::FlowActive),
    };
    if let cc_machine::ota::Admission::Refused(refusal) = admission {
        info!("ota: refused — {}", refusal.message());
        return Err(refusal.message());
    }

    // 2. One at a time. Claimed **before** the shutdown so a second request sees
    //    a busy session rather than racing this one into the shutdown.
    if !session.claim(kind) {
        return Err(SESSION_BUSY_REFUSAL);
    }

    // 3. S8, and the wait that makes it mean something. The pump, the valve and
    //    the heater go off through the applier, on the control task, before
    //    `Writer::begin` erases anything — and the control task re-reads the
    //    **live** state on its way, because the snapshot above can be a tick
    //    stale and the machine is still running.
    let before = shared.applied();
    send(Command::OtaBegin);
    // `wait_applied` is the existing ack every command route uses, and it is
    // honest here for the same reason it is there: the control task notes the
    // command applied only after the telemetry a caller would read is published
    // (`main.rs`, step 8), so a settled wait means the verdict is in place.
    shared.wait_applied(before);

    // `None` covers both "the control task refused" (it would have written a
    // reason) and "the control task never answered", and both are the same
    // answer to this route's only question, which is whether `esp_ota_begin`
    // may run. No answer, no flash.
    let Some(admission) = session.take_verdict() else {
        session.finish_err(cc_web::ota::StatusMessage::Refused(NO_ANSWER_REFUSAL));
        return Err(NO_ANSWER_REFUSAL);
    };
    if let crate::ota::Admission::Refused(refusal) = admission {
        info!("ota: refused — {}", refusal.message());
        session.finish_err(cc_web::ota::StatusMessage::Refused(refusal.message()));
        return Err(refusal.message());
    }
    Ok(())
}

/// OTA upload ordering: snapshot admission, exclusive claim, acknowledged live
/// admission/shutdown, multipart boundary, slot erase, then streaming chunks.
///
/// The snapshot is only a first filter. Wait for control to recheck its state
/// after normal effects and apply shutdown separately. Refusal returns 409;
/// timeout means no flash. Never call `esp_ota_begin` before that acknowledgement.
/// Stream through a 4 KiB stack buffer without allocating per chunk.
fn ota_upload_route(
    session: &crate::ota::Session,
    shared: &Arc<Shared>,
    send: &Arc<dyn Fn(Command) + Send + Sync + 'static>,
    kind: Kind,
    mut req: Request<&mut EspHttpConnection<'_>>,
) -> Result<(), EspError> {
    // 1-3. Admission on the published snapshot, the one-at-a-time claim, and the
    //    safe hardware shutdown the control task applies against its LIVE state.
    //    `Err` is the refusal text, and it is already logged and — where a
    //    session had been claimed — recorded on the status document.
    if let Err(refusal) = ota_open_session(session, shared, send, kind) {
        return respond(req.connection(), 409, &upload_response(false, refusal));
    }

    // 4. The multipart boundary. Read before the flash is touched, because a
    //    request with no envelope is a client error and must not cost an erase.
    // Copied because `req.header` borrows the connection the loop below then
    // reads from, and the borrow checker is right that the header does not
    // outlive it.
    let content_type: Option<String> = req.header("Content-Type").map(String::from);
    let Some(boundary) = cc_web::ota::PartReader::boundary_of(content_type.as_deref()) else {
        session.finish_err(cc_web::ota::StatusMessage::Read(
            cc_web::ota::ReadError::NoPart,
        ));
        return respond(
            req.connection(),
            400,
            &upload_response(false, "No firmware file provided"),
        );
    };
    let mut reader = cc_web::ota::PartReader::new(boundary);

    // 5. The erase.
    let mut writer = match crate::ota::Writer::begin(kind) {
        Ok(writer) => writer,
        Err(err) => {
            session.finish_err(cc_web::ota::StatusMessage::Flash);
            error!("ota: could not open the {kind:?} slot: {err}");
            return respond(
                req.connection(),
                500,
                &upload_response(false, "Failed to begin update"),
            );
        }
    };

    // 6. Stream.
    if let Err(outcome) = ota_stream(session, &mut writer, &mut reader, &mut req) {
        session.finish_err(outcome.message);
        writer.abort();
        return respond(
            req.connection(),
            outcome.status,
            &upload_response(false, outcome.text),
        );
    }

    // A truncated body is refused rather than finalised: `esp_ota_end` would
    // validate a partial image, and a half-written slot is worse than an erased
    // one because the erase has already happened.
    if let Err(failure) = ota_validate(&reader, writer.written(), kind) {
        session.finish_err(failure.message);
        writer.abort();
        return respond(
            req.connection(),
            failure.status,
            &upload_response(false, failure.text),
        );
    }

    let written = writer.written();

    // 8. Finalise. For firmware this is where `esp_ota_end` validates the image
    //    and switches the boot partition.
    match writer.end() {
        Ok(()) => {
            info!("ota: {kind:?} update complete — {written} B");
            session.note_progress(cc_web::ota::Phase::Processing, written, written);
            session.finish_ok();
            respond(
                req.connection(),
                200,
                &upload_response(true, "Update successful. Device will restart."),
            )
        }
        // **Which of the two failures this is matters.** `esp_ota_end`
        // rejecting the image and the `otadata` write that selects it failing
        // are different problems, and collapsing them into "invalid firmware
        // image" tells an operator whose flash is failing to re-upload a good
        // file — a symptom identical to the bug the slot selection was added to
        // fix. `StatusMessage::Flash` already exists for the second.
        Err(err @ (crate::ota::EndError::Image(_) | crate::ota::EndError::SelectSlot(_))) => {
            let status = match err {
                crate::ota::EndError::Image(_) => cc_web::ota::StatusMessage::Invalid,
                crate::ota::EndError::SelectSlot(_) => cc_web::ota::StatusMessage::Flash,
            };
            session.finish_err(status);
            error!("ota: {kind:?} finalise failed — {err}");
            respond(
                req.connection(),
                500,
                &upload_response(false, status.message()),
            )
        }
    }
}

/// Build the [`Configuration`] for this firmware.
///
/// `stack_size` is raised from the default 6144 (`server.rs:141`) to 8192
/// because the SSE handler formats JSON on the httpd task's stack, and
/// `serde_json`'s formatter wants more than 6 KB of headroom on a task that also
/// has to be able to unwind. `keep_alive` is on with a 5 s idle timeout
/// (`KeepAlive::new()`, `server.rs:88-95`) so a browser's pooled connection
/// survives between API calls.
#[must_use]
pub fn configuration() -> Configuration {
    Configuration {
        http_port: HTTP_PORT,
        max_uri_handlers: MAX_URI_HANDLERS,
        max_open_sockets: MAX_OPEN_SOCKETS,
        stack_size: 8192,
        keep_alive: Some(esp_idf_svc::http::server::KeepAlive::new()),
        // Required by the single `/ui*` handler: without it ESP-IDF compares
        // URIs exactly and no path under `/ui` is ever routed, so the SPA would
        // 404 on its own assets. It is a server-wide switch, so it was checked
        // rather than assumed -- see [`serve_ui`] for why the other 23
        // registrations stay exact.
        uri_match_wildcard: true,
        ..Default::default()
    }
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in eight import lists
    // that would rot.
    #![allow(clippy::wildcard_imports)]

    use super::*;

    #[cfg_attr(test, test)]
    pub fn every_api_route_is_registered() {
        // 20 /api/* routes, `/api/config/download`, the four OTA routes, and
        // `/`, `/ui` and `/events`.
        //
        // This list is the parity contract with `ui/packages/frontend/src/lib/
        // routes.ts`, the frontend's own `API_ROUTES` table. A route the UI can
        // call and this server does not register is a 404 in the browser and a
        // blank page in the UI, so the omission is the bug this test exists to
        // prevent.
        let routes = routes();
        for expected in [
            "/api/status",
            "/api/health",
            "/api/temperatures",
            "/api/history",
            "/api/nvs-debug",
            "/api/parameter-help",
            "/api/config",
            "/api/config/download",
            "/api/parameters",
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
        ] {
            assert!(
                routes.iter().any(|(path, _)| *path == expected),
                "{expected} is missing"
            );
        }
    }

    /// The frontend's `API_ROUTES` table, verbatim from
    /// `ui/packages/frontend/src/lib/routes.ts`.
    ///
    /// Transcribed here rather than parsed at build time so that adding a route
    /// to one side and forgetting the other is a **test failure** and not a 404
    /// somebody finds by clicking. `getApiRoute` prefixes `/api`, so each entry
    /// is compared with that prefix applied.
    #[cfg_attr(test, test)]
    pub fn every_route_the_frontend_calls_is_registered() {
        const FRONTEND: &[&str] = &[
            "/api/steam",
            "/api/pid",
            "/api/backflush",
            "/api/setpoint",
            "/api/wake",
            "/api/sleep",
            "/api/config",
            "/api/config/download",
            "/api/parameters",
            "/api/parameter-help",
            "/api/status",
            "/api/health",
            "/api/temperatures",
            "/api/history",
            "/api/scale/tare",
            "/api/scale/calibration",
            "/api/ota/status",
            "/api/ota/firmware",
            "/api/ota/filesystem",
            "/api/ota/url",
            "/api/restart",
            "/api/factory-reset",
            "/api/wifi-reset",
            "/api/nvs-debug",
            "/api/maintenance/reset-backflush-counter",
        ];
        let routes = routes();
        for path in FRONTEND {
            assert!(
                routes.iter().any(|(registered, _)| registered == path),
                "the UI calls {path} but no handler is registered for it"
            );
        }
    }

    // ==================================================== the setpoint route

    /// Every `/api/` route the table advertises is a path the JSON `404`
    /// claims.
    ///
    /// Finding 3.8. The JSON `404` decides JSON-vs-text with
    /// `path.starts_with("/api/")`. If a route were ever advertised as `/api` —
    /// no trailing slash — the 404 would not cover it and a client would get
    /// plain text from a route the boot log claims exists. That is a whole-table
    /// property rather than a property of one entry, which is why it is asserted
    /// over the table and not per route.
    #[cfg_attr(test, test)]
    pub fn every_advertised_api_route_is_covered_by_the_json_404() {
        for (path, _) in routes() {
            // A wildcard answers the requests it matches, so it never reaches
            // the 404 and the rule cannot apply to it. The two registered
            // wildcards are checked for their own method in
            // `wildcard_matching_leaves_the_api_routes_exact`.
            if path.ends_with('*') {
                continue;
            }
            if path.starts_with("/api") {
                assert!(
                    cc_web::help::wants_json_not_found(path),
                    "{path} is advertised but the JSON 404 does not cover it"
                );
            }
        }
    }

    /// `/api/parameter-help` is registered for `GET` and nothing else.
    ///
    /// Finding 3.4. The route existed and was wrong in the worst way — it
    /// answered `200` with an error object — so the registration was never in
    /// doubt and no route test could have caught it. What is worth pinning is
    /// the method list, and a `POST` to it falls through to the JSON `404`
    /// rather than being answered.
    #[cfg_attr(test, test)]
    pub fn every_registered_route_is_in_the_raw_handlers_own_table() {
        // `cc_web::help::ROUTE_PATHS` is a compile-time list, because the `extern
        // "C"` handler that consults it cannot capture one. `routes()` is the
        // runtime list the server actually registers, and this is what stops the
        // two drifting: a path in `routes()` and missing from `ROUTE_PATHS` would
        // answer a `404` where a `405` is correct, and the only way that happens
        // is someone adding a route here and not there.
        for (path, _) in routes() {
            assert!(
                cc_web::help::ROUTE_PATHS.contains(&path),
                "{path} is registered but the raw 404/405 handler does not know it"
            );
        }
        for path in cc_web::help::ROUTE_PATHS {
            assert!(
                routes().iter().any(|(registered, _)| registered == path),
                "{path} is in the raw handler's table but no handler is registered"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_parameter_help_route_is_a_get_and_nothing_else() {
        let entries: Vec<&'static str> = routes()
            .iter()
            .filter(|(path, _)| *path == "/api/parameter-help")
            .map(|(path, _)| *path)
            .collect();
        // Exactly one entry — so no second method is advertised for it, which is
        // what makes the assertion below mean "and nothing else".
        assert_eq!(entries, vec!["/api/parameter-help"]);
        assert!(routes().contains(&("/api/parameter-help", Method::Get)));
    }

    // ==================================================== the toggle routes

    // ==================================================== OTA (deferred)

    // ============================================ POST /api/parameters honesty

    #[cfg_attr(test, test)]
    pub fn the_route_table_fits_the_servers_handler_budget() {
        // If this ever grows past MAX_URI_HANDLERS the server will fail to start
        // with ESP_ERR_HTTPD_HANDLERS_FULL, at boot, which is a bad place to find
        // out.
        assert!(routes().len() <= MAX_URI_HANDLERS);
    }

    // ================================================ POST /api/config/upload

    #[cfg_attr(test, test)]
    pub fn the_config_upload_route_is_registered() {
        // `ui/packages/frontend/src/pages/SystemPage.tsx:182` posts here. It was
        // a live button with no route behind it, so every operator who clicked
        // "Upload configuration" got a 404.
        assert!(
            routes().contains(&("/api/config/upload", Method::Post)),
            "the upload route must be in the table"
        );
    }

    // ============================================================== CORS

    #[cfg_attr(test, test)]
    pub fn the_advertised_options_handler_is_a_wildcard_over_the_api() {
        // It used to be advertised as `("/api/status", Method::Options)` with no
        // `fn_handler` behind it anywhere, so a preflight 404'd while the boot
        // log claimed the route existed. See `register_preflight`.
        let options: Vec<&str> = routes()
            .iter()
            .filter(|(_, method)| *method == Method::Options)
            .map(|(uri, _)| *uri)
            .collect();
        assert_eq!(options, vec!["/api*"], "one real preflight route");
    }

    // ==================================================== POST /api/parameters

    #[cfg_attr(test, test)]
    pub fn the_parameter_route_is_registered_for_both_methods() {
        // Registered once per method rather than as `HTTP_ANY`; two
        // registrations is the closest `esp-idf-svc` can get, because its
        // `Method` has no "any" (embedded-svc 0.29 `http.rs:17-52`).
        let routes = routes();
        assert!(routes.contains(&("/api/parameters", Method::Get)));
        assert!(routes.contains(&("/api/parameters", Method::Post)));
    }

    #[cfg_attr(test, test)]
    pub fn the_redirect_and_the_ui_are_routes() {
        let routes = routes();
        assert!(
            routes.contains(&("/", Method::Get)),
            "the / -> /ui/ redirect"
        );
        assert!(
            routes.contains(&("/ui*", Method::Get)),
            "the UI is one wildcard route: the shell, its assets and its \
             client-side routes all reach the same handler"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_sse_stream_is_a_route() {
        assert!(routes().contains(&("/events", Method::Get)));
    }

    // ---- SSE framing ---------------------------------------------------

    #[cfg_attr(test, test)]
    pub fn an_sse_frame_ends_with_a_blank_line() {
        // The WHATWG parser dispatches on the blank line. Without it the
        // browser buffers the frame and delivers nothing, forever, with no
        // error — the single most common way a hand-rolled SSE endpoint is
        // "broken".
        let frame = Sse::frame("new_temps", "{\"currentTemp\":90.0}");
        assert!(frame.starts_with("event: new_temps\n"), "{frame}");
        assert!(frame.contains("data: {"), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
    }

    #[cfg_attr(test, test)]
    pub fn a_keepalive_is_a_comment_and_carries_no_event() {
        let frame = Sse::keepalive();
        assert!(frame.starts_with(':'), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
        assert!(!frame.contains("event:"), "{frame}");
    }

    #[cfg_attr(test, test)]
    pub fn the_default_sse_mode_is_the_chunked_one() {
        // Chunked framing is the only mode left: it is the one that keeps
        // `send_wait_timeout` and the only one that can be written from a
        // detached request.
        assert_eq!(SseMode::default(), SseMode::Chunked);
        assert_eq!(Sse::new(SseMode::default()).mode(), SseMode::Chunked);
    }

    #[cfg_attr(test, test)]
    pub fn the_client_cap_leaves_sockets_for_the_api() {
        // The bound that keeps one browser tab from starving `/api/*`. ESP-IDF's
        // httpd has a fixed session pool and an SSE client holds its session for
        // as long as it is connected, so the cap is a socket budget, not a
        // politeness limit. ADR-0002's reproducer fires 6-10 parallel API
        // requests; this asserts at least three sockets stay available.
        const { assert!(SSE_MAX_CLIENTS < MAX_OPEN_SOCKETS) };
        const { assert!(MAX_OPEN_SOCKETS - SSE_MAX_CLIENTS >= 3) };
    }

    #[cfg_attr(test, test)]
    pub fn a_fresh_stream_has_no_clients_and_no_counters() {
        let sse = Sse::new(SseMode::Chunked);
        assert_eq!(sse.connected_now(), 0);
        assert_eq!(sse.clients(), 0);
        assert_eq!(sse.rejected(), 0);
        assert_eq!(sse.sent(), 0);
        assert_eq!(sse.dropped(), 0);
        assert_eq!(sse.pushed(), 0);
        assert_eq!(sse.dropped_frames(), 0);
    }

    /// How many frames the mailbox is holding.
    ///
    /// A `usize` rather than the `Result`: the on-target build has no `std`, so
    /// `MutexGuard` is not `PartialEq` there and comparing the `Result` directly
    /// would compile for the host and fail for the device. That asymmetry is
    /// exactly what `just lint-esp32` exists to catch, and this is the shape it
    /// caught.
    fn mailbox_len(sse: &Sse) -> usize {
        sse.mailbox.lock().map_or(0, |m| m.len())
    }

    #[cfg_attr(test, test)]
    pub fn a_push_counts_and_a_full_mailbox_counts_the_drop() {
        // `Sse::broadcast` must not block its caller: the caller is the control
        // task. A mailbox that filled is counted, not waited on.
        let sse = Sse::new(SseMode::Chunked);
        for i in 0..SSE_MAILBOX_DEPTH {
            sse.broadcast(Sse::frame("weight", &format!("{{\"n\":{i}}}")));
        }
        sse.note_push_attempt();
        assert_eq!(sse.pushed(), 1);
        assert_eq!(sse.dropped_frames(), 0);
        assert_eq!(mailbox_len(&sse), SSE_MAILBOX_DEPTH);

        // One more than the mailbox holds: the oldest is discarded and counted.
        sse.broadcast(Sse::frame("weight", "{\"n\":99}"));
        assert_eq!(sse.dropped_frames(), 1);
        assert_eq!(mailbox_len(&sse), SSE_MAILBOX_DEPTH);
    }

    #[cfg_attr(test, test)]
    pub fn the_broadcaster_cadence_fits_under_the_poll_interval() {
        // The broadcaster can only be as responsive as its poll, and the poll
        // must not be the thing that makes the loop a busy-wait.
        const { assert!(SSE_POLL_MS < SSE_EVENT_INTERVAL_MS) };
        const { assert!(SSE_POLL_MS <= 100) };
    }

    #[cfg_attr(test, test)]
    pub fn the_keepalive_interval_is_under_the_nat_floor() {
        // 15 s against the common 30 s NAT UDP timeout; a silent stream is
        // dropped by the router long before the browser notices.
        const { assert!(SSE_KEEPALIVE_MS <= 15_000) };
        const { assert!(SSE_KEEPALIVE_MS > SSE_EVENT_INTERVAL_MS) };
    }

    // ---- the heap guard -------------------------------------------------

    #[cfg_attr(test, test)]
    pub fn the_large_response_floor_is_the_adrs_floor() {
        // ADR-0002 decision 5. Two subscribers, one number.
        assert_eq!(HEAP_FLOOR_BYTES, 30_000);
    }

    #[cfg_attr(test, test)]
    pub fn the_large_response_limit_is_above_19kb() {
        // ADR-0002 measured `/api/parameters?filter=all` at 19 KB. A limit
        // below that would 503 the endpoint that serves it.
        const { assert!(MAX_JSON_BYTES > 19 * 1024) };
        const {
            assert!(MAX_JSON_BYTES < 64 * 1024, "and not so large it is the OOM");
        };
    }

    #[cfg_attr(test, test)]
    pub fn the_shared_telemetry_round_trips() {
        let shared = Shared::new();
        let t = Telemetry {
            temperature_c: 88.0,
            machine_state: 33,
            ..Telemetry::default()
        };
        shared.publish(t.clone());
        assert_eq!(shared.snapshot(), t);
    }

    #[cfg_attr(test, test)]
    pub fn a_read_does_not_consume_the_snapshot() {
        // `Snapshot::get` was `Cell::take`, so reading it left `T::default()`
        // behind and the *second* read of a live value returned zeroes. Reading
        // twice and requiring the two to be equal is the only assertion that
        // fails on a destructive read -- one that only checks "the read returns
        // what was set" passes either way.
        let shared = Shared::new();
        let published = Telemetry {
            machine_state: 33,
            temperature_c: 88.0,
            pid_enabled: true,
            ..Telemetry::default()
        };
        shared.publish(published.clone());
        let first = shared.snapshot();
        let second = shared.snapshot();
        assert_eq!(first, second, "the first read consumed the snapshot");
        assert_eq!(second, published, "the second read saw an emptied slot");
    }

    #[cfg_attr(test, test)]
    pub fn the_radio_fields_survive_a_machine_publish() {
        // The two-publisher contract, and the reason the control task publishes
        // the radio *after* the machine telemetry. `publish` replaces the whole
        // slot, so a machine publish that ran second would erase the radio's
        // four fields and `/api/status` would go back to `wifiAssociated: false`
        // — which is the bug this ordering exists to prevent.
        let shared = Shared::new();
        // The radio publishes first — a read-modify-write, exactly as
        // `network::publish_radio` does it.
        let mut radio = shared.snapshot();
        radio.wifi_associated = true;
        radio.signal = 4;
        radio.ip = heapless::String::<15>::try_from("10.0.0.7").ok();
        shared.publish(radio);
        // …and the control task's publish is the one that must not run second.
        let before = shared.snapshot();
        shared.publish(telemetry_with_radio_untouched(&before));
        let after = shared.snapshot();
        assert!(
            after.wifi_associated,
            "the machine publish erased the radio's association flag"
        );
        assert_eq!(after.signal, 4);
        assert_eq!(after.ip, before.ip);
    }

    /// What a control task's `publish` looks like when it is careful: it carries
    /// the machine fields and *reuses* the radio's, because the two are separate
    /// publishers and a whole-slot replace is the only thing `Shared` offers.
    fn telemetry_with_radio_untouched(previous: &Telemetry) -> Telemetry {
        Telemetry {
            machine_state: 20,
            temperature_c: 93.5,
            signal: previous.signal,
            wifi_associated: previous.wifi_associated,
            wifi_offline: previous.wifi_offline,
            ip: previous.ip.clone(),
            ..Telemetry::default()
        }
    }

    #[cfg_attr(test, test)]
    pub fn a_reboot_request_is_one_shot() {
        let shared = Shared::new();
        shared.reboot_requested.store(true, Ordering::SeqCst);
        assert!(shared.take_reboot_request());
        assert!(
            !shared.take_reboot_request(),
            "a second take must not reboot again"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_ui_shell_and_its_assets_are_embedded_and_gzipped() {
        // The size assertion is the one that matters: the bundle is embedded rather
        // than mounted, and it only fits because it is compressed. In a `const`
        // block so a build that outgrows the app partition fails HERE rather
        // than as an OTA that will not fit on a machine that is already wired
        // up.
        const _: () = assert!(
            UI_TOTAL_BYTES < 400 * 1024,
            "the embedded web UI no longer fits the app partition"
        );
        const _: () = assert!(UI_FILE_COUNT >= 3, "index.html, one JS and one CSS bundle");

        // A JS file served as the wrong type is a blank page behind a 200, so
        // the embedded assets are checked for the pairing that causes it.
        for asset in UI_ASSETS {
            assert!(
                mime_for(asset.path) != "text/plain",
                "{} would be served as text/plain",
                asset.path
            );
            assert!(!asset.bytes.is_empty(), "{} is empty", asset.path);
        }
        let shell = resolve_ui("/ui");
        assert_eq!(shell, UiTarget::File(UI_INDEX));
        assert_eq!(UI_INDEX.path, "/index.html");
        assert!(UI_INDEX.gzip, "the shell must be gzip-encoded");
    }

    #[cfg_attr(test, test)]
    pub fn a_client_side_route_serves_the_shell_but_a_missing_asset_does_not() {
        // A reload on a deep link must boot the app, not 404.
        assert_eq!(resolve_ui("/ui/config/behavior"), UiTarget::SpaRoute);
        assert_eq!(resolve_ui("/ui/system"), UiTarget::SpaRoute);
        // A missing script must be a 404. Serving the shell here is the failure
        // that produces "200, HTML where JS was expected, blank screen".
        assert_eq!(resolve_ui("/ui/assets/missing.js"), UiTarget::Missing);
        // A query string is not part of the extension test.
        assert_eq!(resolve_ui("/ui/system?x=1.5"), UiTarget::SpaRoute);
        // `/ui*` also matches `/uixyz`, which is not a child of `/ui`.
        assert_eq!(resolve_ui("/uixyz"), UiTarget::Missing);
        // And the real asset is found.
        let js = UI_ASSETS
            .iter()
            .find(|a| mime_for(a.path) == "application/javascript")
            .map_or_else(|| panic!("no embedded JS"), |a| a.path);
        assert_eq!(
            resolve_ui(&alloc::format!("/ui{js}")),
            UiTarget::File(
                UI_ASSETS
                    .iter()
                    .find(|a| a.path == js)
                    .unwrap_or(&UI_ASSETS[0])
            )
        );
    }

    #[cfg_attr(test, test)]
    pub fn wildcard_matching_leaves_the_api_routes_exact() {
        // `/ui*` only works because the server matches wildcards, and that
        // switch is global. `httpd_uri_match_wildcard` requires an exact length
        // match for a template with no `*`/`?` (httpd_uri.c:57-60), so this
        // asserts the invariant the whole `/api/*` surface depends on.
        // Exactly two wildcards are registered and both are deliberate: the UI
        // shell (`/ui*`), and the `/api*` preflight, which answers `Options` and
        // therefore cannot serve the wrong body to a `GET`. Anything else with a
        // `*` would match a real route by prefix.
        let mut wildcards: Vec<&str> = routes()
            .iter()
            .filter(|(uri, _)| uri.ends_with('*'))
            .map(|(uri, _)| *uri)
            .collect();
        wildcards.sort_unstable();
        assert_eq!(wildcards, ["/api*", "/ui*"], "wildcard routes changed");
        for (uri, method) in routes() {
            if !uri.ends_with('*') {
                continue;
            }
            // Each wildcard is paired with the one method it may answer, so
            // neither can start serving a body to a request it was not meant for.
            let allowed = match uri {
                "/ui*" => Method::Get,
                "/api*" => Method::Options,
                other => panic!("unexpected wildcard route {other}"),
            };
            assert_eq!(method, allowed, "{uri} answers the wrong method");
        }
        assert!(configuration().uri_match_wildcard);
    }
}
