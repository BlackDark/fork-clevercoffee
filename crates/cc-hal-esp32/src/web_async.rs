//! The one `unsafe` seam the SSE stream needs, isolated and justified.
//!
//! Owner: **R3-14 follow-up** (the single-client-SSE denial of service).
//!
//! # Why this exists at all
//!
//! ESP-IDF's httpd is **one task** for the whole server. `httpd_server_init`
//! creates exactly one thread (`components/esp_http_server/src/httpd_main.c:533`,
//! `httpd_os_thread_create(..., "httpd", ..., httpd_thread, ...)`), and
//! `httpd_thread` (`:329-350`) is a `while (1) { httpd_server(hd); }` loop over
//! a single `select()`. Every URI handler runs on that task, one at a time. A
//! handler that does not return stops the entire web API — which is exactly what
//! a handler that holds an SSE stream open does.
//!
//! `esp-idf-svc` 0.53.0 adds nothing here: `EspHttpServer::handler_nonstatic`
//! (`src/http/server.rs:567-598`) just fills in an `httpd_uri_t` and calls
//! `httpd_register_uri_handler`, and `to_native_handler` (`:660-685`) invokes
//! the closure and then `complete()`s the response. There is no async layer for
//! HTTP in the crate (the only async scaffolding in the file is the commented
//! out WebSocket `asynch` sketch at `:1738-1945`).
//!
//! So the handler must return, and something else must do the writing. ESP-IDF
//! provides exactly that primitive, and it is the documented answer —
//! `esp_http_server.h:840-873` describes `httpd_req_async_handler_begin` as
//! "necessary in order to handle multiple requests simultaneously".
//!
//! # The three calls, and what each guarantees
//!
//! * `httpd_req_async_handler_begin(r, &out)` (`httpd_txrx.c:650-705`) `malloc`s
//!   a copy of the request and of its `httpd_req_aux`, **copies the response
//!   header block across** (`:685-687`, `memcpy(async_aux->resp_hdrs,
//!   r_aux->resp_hdrs, ...)`), sets the original's `remaining_len` to 0, and
//!   sets `r_aux->sd->for_async_req = true` (`:700`). That last flag is the
//!   whole point: `httpd_process_session` returns early for such a session
//!   (`httpd_main.c:256-258`, "session is busy in an async task, do not process
//!   here") and `httpd_sess_set_descriptors` skips its fd
//!   (`httpd_sess.c:91`), so the httpd task's `select()` stops watching the
//!   socket entirely and goes back to serving everyone else.
//! * `httpd_resp_send_chunk` (`httpd_txrx.c:339`) writes one chunk. It is the
//!   same function `EspHttpConnection::write` calls, so going through it rather
//!   than a raw `write(2)` keeps the chunked framing and the socket's
//!   `send_wait_timeout`.
//! * `httpd_req_async_handler_complete(r)` (`httpd_txrx.c:707-755`) frees the
//!   copy, clears `for_async_req` (`:719`) and pokes the control socket so the
//!   httpd task's `select()` wakes and re-arms the fd. **It must be called on
//!   every path**, and the header note is explicit that a request left
//!   incomplete eventually stops the server accepting connections at all
//!   (`esp_http_server.h:864-866`).
//!
//! # Why the header state is set before `begin`, not after
//!
//! `begin` copies `resp_hdrs` by value. A header set after `begin` lands in the
//! *original* request's header block, which is never sent — the copy is what
//! gets written. So the order in the `/events` handler is load-bearing:
//! `initiate_response` (status, content type, extra headers) first, `begin`
//! second, and nothing touches the connection after that.
//!
//! # Why calling `httpd_resp_send_chunk` from another task is sound here
//!
//! Every `httpd_resp_*` entry point guards on `httpd_valid_req(r)`, which is
//! `httpd_validate_req_ptr(r)` — a check that the caller is on the httpd
//! thread — but only when `CONFIG_HTTPD_VALIDATE_REQ` is set
//! (`esp_httpd_priv.h:361-365`). It is **not** set in this build
//! (`target/.../out/sdkconfig` has no `HTTPD_VALIDATE_REQ` line at all), so
//! `httpd_valid_req` is the literal `true` and the cross-task call is permitted.
//! That is the configuration the async pattern is designed for: the whole point
//! of `begin` is that the returned request belongs to another thread. **If that
//! Kconfig is ever enabled, this module's cross-task write must move to the raw
//! socket fd** ([`sockfd`]), and that is the trigger to revisit it.
//!
//! # The `Send` impl
//!
//! See [`AsyncReq`]. It is the second `unsafe` in the workspace, after
//! `crate::zacwire::now_us`, and it is scoped to this module.

// The second `unsafe` in the workspace. `crate::zacwire::now_us` is the first,
// and the workspace lint is `unsafe_code = "deny"`, so the allowance is here,
// scoped to this one module, rather than turned off crate-wide. The reasoning
// for every call is in this module's documentation above; in short: ESP-IDF's
// httpd is a single task, so an SSE handler that holds its connection open stops
// the whole web API, and `httpd_req_async_handler_begin` is ESP-IDF's own
// documented answer (`esp_http_server.h:840-873`) — the only one that exists,
// and not expressible through `esp-idf-svc` 0.53.0.
#![allow(
    unsafe_code,
    reason = "three `httpd_*` calls that let the `/events` handler return while \
              another task writes; see this module's docs"
)]

use core::ffi::c_char;

use esp_idf_svc::sys::EspError;
use esp_idf_sys::{
    httpd_req_async_handler_begin, httpd_req_async_handler_complete, httpd_req_get_hdr_value_len,
    httpd_req_get_hdr_value_str, httpd_req_t, httpd_resp_send, httpd_resp_send_chunk,
    httpd_resp_send_custom_err, httpd_resp_send_err, httpd_resp_set_hdr, httpd_resp_set_status,
    httpd_resp_set_type, ESP_FAIL, ESP_OK,
};

/// `httpd_err_code_t_HTTPD_500_INTERNAL_SERVER_ERROR`, named locally.
///
/// The generated binding spells it with its full enum prefix, which is
/// unreadable at four call sites.
const HTTPD_500: esp_idf_sys::httpd_err_code_t =
    esp_idf_sys::httpd_err_code_t_HTTPD_500_INTERNAL_SERVER_ERROR;

/// A request detached from the httpd task, ours to write until completed.
///
/// Obtained from [`begin_detached`]. Valid until [`AsyncReq::complete`], and
/// **must** be completed on every path — see the module docs for what an
/// uncompleted request costs.
pub struct AsyncReq {
    raw: *mut httpd_req_t,
}

// SAFETY: the claim is not that the pointer's provenance transfers (it does not
// need to) but that exactly one task may *use* it at a time.
// `httpd_req_async_handler_begin` hands ownership of the copy to the caller and
// marks the session so the httpd task will not touch it (`httpd_txrx.c:700`,
// plus the two skips at `httpd_main.c:256` and `httpd_sess.c:91`). The only two
// operations on this type are the write and the complete, both documented as
// async-task operations, and the value is moved into the broadcaster task's
// list, so there is one owner throughout and no aliasing. Created on the httpd
// task, consumed on the broadcaster task: that is the transfer being authorised.
unsafe impl Send for AsyncReq {}

impl AsyncReq {
    fn new(raw: *mut httpd_req_t) -> Self {
        Self { raw }
    }

    /// Send one chunk. `Err` means the peer is gone and this request must be
    /// completed.
    pub(crate) fn write(&mut self, frame: &str) -> Result<(), ()> {
        // A zero-length frame is refused rather than sent:
        // `httpd_resp_send_chunk(.., 0)` is the *terminator*
        // (`httpd_txrx.c:1164` is exactly that call), so an empty frame sent by
        // accident would close the stream. Nothing produces one —
        // `Sse::frame` and `Sse::keepalive` always end in a blank line — so
        // this is a guard, not a branch.
        if frame.is_empty() {
            return Err(());
        }
        // ESP-IDF takes the length as a signed count and reads exactly that many
        // bytes from the pointer, so a `&str` is precisely the argument shape it
        // wants. `len` is an `isize` for the cast: an SSE frame here is ~100
        // bytes and a string on this machine is 320 KB, so the conversion cannot
        // wrap, and refusing to compile would be noise.
        #[allow(
            clippy::cast_possible_wrap,
            reason = "an SSE frame is ~100 B and the heap is 320 KB, so usize -> \
                      isize cannot wrap; the C++ takes an ssize_t"
        )]
        let len: isize = frame.len() as isize;
        let rc = unsafe { httpd_resp_send_chunk(self.raw, frame.as_ptr().cast::<c_char>(), len) };
        if rc == ESP_OK {
            Ok(())
        } else {
            Err(())
        }
    }

    /// Send the terminating zero-length chunk, so a client that is still there
    /// sees a cleanly finished body rather than a truncated one.
    ///
    /// Not used on the paths that matter — a client that has gone away is
    /// discovered by a failed write, and there is nobody left to inform — but it
    /// is the correct end of a stream and belongs next to the writer.
    #[allow(dead_code, reason = "correct stream termination; see the doc comment")]
    pub(crate) fn finish(&mut self) {
        unsafe {
            httpd_resp_send_chunk(self.raw, core::ptr::null(), 0);
        }
    }

    /// Release the request. After this the pointer is dead and must not be
    /// touched again.
    pub(crate) fn complete(self) {
        unsafe {
            httpd_req_async_handler_complete(self.raw);
        }
    }

    /// Surrender ownership of the raw pointer without completing it.
    ///
    /// For the one path where the request must be released by a caller that is
    /// *not* going to stream on it — `Sse::attach` refusing a client for want of
    /// a slot (`register_raw_sse`'s error arm). Streaming on it after this would
    /// be a use-after-free; completing it twice would be a double free. The
    /// caller takes both responsibilities.
    pub(crate) fn into_raw(self) -> *mut httpd_req_t {
        // `ManuallyDrop` rather than `mem::forget`: `AsyncReq` has no `Drop`
        // impl, so forgetting it is a no-op that clippy flags as
        // `forget_non_drop`. `ManuallyDrop::new` says the intent directly — take
        // the pointer out, and do not let the wrapper's scope run anything — and
        // it keeps working if a `Drop` impl is ever added.
        let this = core::mem::ManuallyDrop::new(self);
        this.raw
    }
}

/// Detach the request a handler is currently serving, after its response
/// headers have been set.
///
/// # Safety
///
/// `raw` must be the live `httpd_req_t*` of the handler that is running, and
/// the response status, content type and any extra headers must already have
/// been set on it. In this crate that means
/// [`EspHttpConnection::initiate_response`](esp_idf_svc::http::server::EspHttpConnection::initiate_response)
/// has already run — it is the only thing that sets them — and that
/// `initiate_response` was the last thing to touch the connection.
///
/// [`EspHttpConnection::initiate_response`]: esp_idf_svc::http::server::EspHttpConnection::initiate_response
pub(crate) unsafe fn begin_detached(raw: *mut httpd_req_t) -> Option<AsyncReq> {
    // `out` is a C out-parameter: the callee stores a `malloc`'d request there.
    // `&mut out` coerces to the `*mut *mut httpd_req_t` the signature demands,
    // which is what `clippy::borrow_as_ptr` objects to — but the coercion is the
    // C API's shape, not a mistake, and there is no `Option<*mut T>` form to use
    // instead.
    #[allow(
        clippy::borrow_as_ptr,
        reason = "`httpd_req_async_handler_begin` is a C out-parameter; \
                  `&mut out` is the only way to spell the `*mut *mut` it takes"
    )]
    let begin = || {
        let mut out: *mut httpd_req_t = core::ptr::null_mut();
        (httpd_req_async_handler_begin(raw, &mut out), out)
    };
    let (rc, out) = begin();
    // `begin` returns ESP_ERR_NO_MEM when the two mallocs fail, which on a
    // 320 KB heap is a real answer and not a hypothetical one. The caller
    // answers 503 rather than pretending the stream exists.
    if rc == ESP_OK && !out.is_null() {
        Some(AsyncReq::new(out))
    } else {
        None
    }
}

/// The socket behind a request.
///
/// Not how this module writes — [`AsyncReq::write`] goes through
/// `httpd_resp_send_chunk` so the chunked framing and `send_wait_timeout` still
/// apply. It is here so the question "could we write to the fd from another
/// task instead?" has an answer next to the code that could have done it, and so
/// the `CONFIG_HTTPD_VALIDATE_REQ` fallback named in the module docs has a name
/// to point at.
#[allow(dead_code, reason = "the documented fallback; see the doc comment")]
pub(crate) fn sockfd(raw: *mut httpd_req_t) -> i32 {
    unsafe { esp_idf_sys::httpd_req_to_sockfd(raw) }
}

/// Register a handler that is **not** wrapped by `esp-idf-svc`'s
/// `to_native_handler`, for the one route that must not have anything written
/// after its handler returns.
///
/// # Why this exists — the double-response bug
///
/// Every route in this firmware is registered with `EspHttpServer::fn_handler`,
/// which wraps the closure in `to_native_handler`
/// (`esp-idf-svc` `src/http/server.rs:660-685`):
///
/// ```text
/// let result = connection.invoke(&handler);
/// match result {
///     Ok(()) => { connection.complete()?; }   // <-- ALWAYS
///     ...
/// }
/// ```
///
/// and `complete()` (`:1158-1170`) is:
///
/// ```text
/// if self.response_headers.is_some() {
///     httpd_resp_send(req, buf, 0)        // a COMPLETE response
/// } else {
///     httpd_resp_send_chunk(req, buf, 0)  // the chunked TERMINATOR
/// }
/// ```
///
/// For `/events` both arms are wrong:
///
/// * `initiate_response` sets `response_headers = Some(..)`
///   (`server.rs:1078`), so the **first** arm runs: `httpd_resp_send(.., 0)`
///   writes a complete `HTTP/1.1 200` with `Content-Length: 0`. The response is
///   *over* at that point, on the wire, before a single SSE frame exists.
/// * The detached request the handler handed to the broadcaster then writes a
///   **second** `HTTP/1.1 200 ... Transfer-Encoding: chunked` header block and
///   the frames — on the same socket, after a response that already ended.
///
/// That is exactly the two-concatenated-responses capture, and it is why Firefox
/// says "cannot connect" and Chrome says "connection lost while the page was
/// loading": both see a complete, empty response and then garbage where a
/// second response should not be.
///
/// The broadcaster-task design is **not** the problem and stays: the handler
/// must return, or the single-task httpd stops serving every other route. The
/// *wrapping* is the problem.
///
/// # The fix
///
/// Register the route with a raw `httpd_uri_t` whose handler is this module's
/// own `extern "C"` function, which does the work and returns without calling
/// anything from `EspHttpConnection`. `EspHttpServer` exposes its
/// `httpd_handle_t` through `RawHandle`, so `httpd_register_uri_handler` can be
/// called directly.
///
/// The handler then does, in order: set status/type/headers on the raw request,
/// `begin_detached`, hand the detached request to the broadcaster, return
/// `ESP_OK`. Nothing writes to the *original* request afterwards, so the first
/// response is the chunked one the broadcaster writes — and only one response is
/// ever sent.
///
/// `ESP_OK` is returned rather than an error on purpose: ESP-IDF treats a
/// non-`ESP_OK` return from a URI handler as a send failure and would try to
/// write an error response of its own (`httpd_uri.c`, after the handler
/// returns), which is the second response all over again.
///
/// # Safety of the `user_ctx`
///
/// `user_ctx` is the `Arc<Sse>` the handler needs, as a raw pointer. It is
/// created by [`crate::web::Web::start`] with `Arc::into_raw` and is **never**
/// freed: the server owns it for the life of the process, and ESP-IDF's own
/// async example leaks the same way (its `httpd_async_req_t` carries a
/// `malloc`'d request with no matching free on the success path). A double-free
/// or use-after-free here would be a crash on every SSE connect, which is why
/// the leak is stated rather than hidden.
pub(crate) fn register_raw_sse(
    server_handle: esp_idf_sys::httpd_handle_t,
    route: alloc::sync::Arc<crate::web::SseRoute>,
) -> Result<(), EspError> {
    let uri = c"/events";
    // `Arc::into_raw` — the handler receives this back as `*mut c_void` and
    // reborrows it. It is deliberately never reclaimed; see the docs above.
    let user_ctx = alloc::sync::Arc::into_raw(route)
        .cast::<core::ffi::c_void>()
        .cast_mut();
    let conf = esp_idf_sys::httpd_uri_t {
        uri: uri.as_ptr(),
        // `HTTP_GET`. `esp-idf-sys` exposes the method as a plain `u32` field
        // (`bindings.rs`, `httpd_uri_t::method`), and ESP-IDF's own `HTTP_GET`
        // is 1 (`include/esp_http_server.h`, `enum httpd_method`).
        // `Method::Get as u32` is the same number and keeps the two in step if
        // the enum ever moves.
        method: esp_idf_svc::http::server::Method::Get as u32,
        handler: Some(sse_handler),
        user_ctx,
    };
    // SAFETY: `conf.uri` points at a `c"/events"` literal, which is `'static` and
    // outlives the registration, and `conf.handler` is `sse_handler`, which has
    // the `extern "C" fn(*mut httpd_req_t) -> esp_err_t` signature the field
    // declares. `&conf` coerces to the `*const httpd_uri_t` the C API takes;
    // that coercion is the API's shape and has no `&raw` alternative.
    #[allow(
        clippy::borrow_as_ptr,
        reason = "`httpd_register_uri_handler` takes `*const httpd_uri_t`; \
                  `&conf` is the only way to spell it"
    )]
    let rc = unsafe { esp_idf_sys::httpd_register_uri_handler(server_handle, &conf) };
    match EspError::from(rc) {
        None => Ok(()),
        Some(err) => Err(err),
    }
}

/// Read a request header into a `&str` borrowed from the scratch buffer.
///
/// `None` when the header is absent, and `None` — not an empty string — when it
/// is **longer than the buffer**. A truncated credential must never be compared
/// against a configured one: `Authorization: Basic <the first 128 bytes of the
/// right password>…` would be a prefix that `constant_time_eq` rejects only by
/// accident of what follows.
fn authorization_header(req: *mut httpd_req_t, scratch: &mut [u8]) -> Option<&str> {
    // SAFETY: `req` is the live request on the httpd task, which is the only
    // context ESP-IDF documents for these two calls.
    let len = unsafe { httpd_req_get_hdr_value_len(req, c"Authorization".as_ptr()) };
    if len == 0 || len + 1 > scratch.len() {
        return None;
    }
    // SAFETY: the buffer is `len + 1` bytes, which is what
    // `httpd_req_get_hdr_value_str` writes plus its NUL.
    let rc = unsafe {
        httpd_req_get_hdr_value_str(
            req,
            c"Authorization".as_ptr(),
            scratch.as_mut_ptr().cast(),
            scratch.len(),
        )
    };
    if rc != ESP_OK {
        return None;
    }
    core::str::from_utf8(scratch.get(..len)?).ok()
}

/// Answer `status` with `challenge`, as a complete response.
///
/// `ESP_FAIL` is returned rather than `ESP_OK` on the failure arms, which is the
/// same shape as `sse_handler`'s other early exits: ESP-IDF's URI handler
/// contract treats `ESP_OK` as "this request has been answered".
fn refuse(
    req: *mut httpd_req_t,
    status: &core::ffi::CStr,
    challenge: &str,
) -> esp_idf_sys::esp_err_t {
    const BODY: &[u8] = b"{\"error\":\"authentication required\"}";
    // SAFETY: `req` is the live request on the httpd task, and every pointer
    // below is either a `'static` literal or the stack buffer that outlives the
    // `httpd_resp_send` call.
    unsafe {
        if httpd_resp_set_status(req, status.as_ptr()) != ESP_OK
            || httpd_resp_set_type(req, c"application/json".as_ptr()) != ESP_OK
            || httpd_resp_set_hdr(req, c"WWW-Authenticate".as_ptr(), challenge.as_ptr().cast())
                != ESP_OK
        {
            return ESP_FAIL;
        }
        httpd_resp_send(
            req,
            BODY.as_ptr().cast(),
            isize::try_from(BODY.len()).unwrap_or(0),
        )
    }
}

/// The raw `extern "C"` handler ESP-IDF calls for `/events`.
///
/// Runs on the httpd task. It must return promptly — everything long-lived
/// happens on the broadcaster task — and it must not let anything else write to
/// `req` after it returns. See [`register_raw_sse`] for why.
extern "C" fn sse_handler(req: *mut httpd_req_t) -> esp_idf_sys::esp_err_t {
    // SAFETY: `req` is the live request ESP-IDF is dispatching, on the httpd
    // task, which is the only task that may touch it until
    // `httpd_req_async_handler_begin` copies it out.
    unsafe {
        if req.is_null() {
            return ESP_FAIL;
        }
        // SAFETY: `user_ctx` is the `Arc<SseRoute>` pointer stored at
        // registration (see `register_raw_sse`), alive for the life of the
        // process, and `Arc::as_ref` reconstitutes the reference without
        // consuming it. The httpd task is the only reader, and it does not
        // outlive the server.
        let route: &crate::web::SseRoute = &*(*req).user_ctx.cast::<crate::web::SseRoute>();

        // **Authentication, before anything else.**
        //
        // `/events` is protected like every other route — see
        // `cc_hal_esp32::web::register`. A browser's `EventSource` cannot *set*
        // an `Authorization` header, but HTTP Basic credentials are cached per
        // origin and realm once a browser has answered a challenge, and it
        // replays them on every subsequent same-origin request including this
        // one; so the stream works in practice after the operator has logged in
        // through the UI. The C++ has exactly the same property, because its
        // middleware covers `/events` too.
        // The header is copied into a scratch buffer because ESP-IDF's accessor
        // writes into a caller-supplied one, and the bytes are only valid while
        // it is alive — so it lives no longer than the comparison.
        let mut scratch = [0u8; 192];
        let admitted = {
            let header = authorization_header(req, &mut scratch);
            route.auth.admits(header)
        };
        if !admitted {
            return refuse(
                req,
                c"401 Unauthorized",
                cc_protocol::http_auth::WWW_AUTHENTICATE,
            );
        }

        // The response headers. These go on the **original** request, which is
        // what `begin_detached` then copies into the detached one — so the order
        // is load-bearing (see the module docs).
        if httpd_resp_set_status(req, c"200 OK".as_ptr()) != ESP_OK
            || httpd_resp_set_type(req, c"text/event-stream".as_ptr()) != ESP_OK
            || httpd_resp_set_hdr(req, c"Cache-Control".as_ptr(), c"no-cache".as_ptr()) != ESP_OK
            || httpd_resp_set_hdr(req, c"Connection".as_ptr(), c"keep-alive".as_ptr()) != ESP_OK
        {
            return httpd_resp_send_err(req, HTTPD_500, c"header".as_ptr());
        }

        // SAFETY: `req` is live and its headers are set, which is
        // `begin_detached`'s contract.
        let Some(async_req) = begin_detached(req) else {
            return httpd_resp_send_err(req, HTTPD_500, c"sse unavailable".as_ptr());
        };

        // The broadcaster writes from here; this handler returns and nothing
        // else touches `req`.
        match route.sse.attach(async_req) {
            Ok(()) => ESP_OK,
            // Too many clients. `attach` handed the request back rather than
            // taking it, so it must be released here or the session stays marked
            // busy forever (`httpd_req_async_handler_begin` set `for_async_req`)
            // and the server eventually stops accepting connections.
            Err(refused) => {
                // Freshly detached, owned here, not handed to the
                // broadcaster, and released exactly once.
                httpd_req_async_handler_complete(refused.into_raw());
                // **Not** `httpd_resp_send_err`: ESP-IDF v5.5.5's
                // `httpd_err_code_t` has no 503 member (`bindings.rs`: 500, 501,
                // 505, 400, 401, 403, 404, 405, 408, 411, 413, 414, 431 and
                // `ERR_CODE_MAX`), so the only way to send a 503 is
                // `httpd_resp_send_custom_err` with the status line spelled out.
                // The C++ answers 503 for this case through ESPAsyncWebServer,
                // and 503 (not 429, not 500) is what `SSE_MAX_CLIENTS` being
                // full means.
                httpd_resp_send_custom_err(
                    req,
                    c"503 Service Unavailable".as_ptr(),
                    c"too many event-stream clients".as_ptr(),
                )
            }
        }
    }
}

// ============================================================ the JSON 404

/// Register the JSON `404` for `/api/` paths that matched no route.
///
/// Finding 3.8 of
/// [`32-findings-2026-10-03.md`](../../../docs/rust-migration/32-findings-2026-10-03.md).
/// The C++ registers `server_->onNotFound(...)`
/// (`WebServerManager.cpp:235`), whose `handleNotFound` (`:1006-1027`) answers a
/// JSON `{"error": …}` for any path starting `/api/` and plain text for the rest.
/// This firmware served ESP-IDF's default plain-text 404 for both, so a client
/// that parses JSON got a parse error rather than an answer it could read.
///
/// # Why one `httpd_register_err_handler` and no router
///
/// ESP-IDF's `httpd` has no catch-all URI: a wildcard match is `httpd_uri_match_wildcard`
/// (`httpd_uri.c:97-122`), which the httpd already uses for `/api*` and `/ui*`,
/// and a second `/api*` registration would be ambiguous with the preflight one.
/// The error handler is the mechanism the C++'s `onNotFound` actually corresponds
/// to, it needs no state and no `user_ctx`, and ESP-IDF calls it only when
/// nothing matched — which is precisely the `onNotFound` condition. It is
/// registered for `HTTPD_404_NOT_FOUND` alone; `405` and `400` keep ESP-IDF's
/// defaults, because the C++'s `onNotFound` is reached for those too but its
/// bodies are `ESPAsyncWebServer`'s, not its own.
///
/// `esp-idf-svc` 0.53.0 does not wrap this — there is no `err_handler` method in
/// `src/http/server.rs` — so it is a raw `esp-idf-sys` call, in the module that
/// already holds the raw `unsafe` seam and the reason for it.
///
/// The handler takes no `user_ctx`: `Auth` is not consulted here, and that is
/// deliberate rather than an oversight. **A 404 reveals nothing** — the reachable
/// set is the ~25 routes in [`crate::web::routes`], all of which are already
/// behind `Auth`, and a client that cannot authenticate learns from this only that
/// a path does not exist. The C++ puts its auth middleware in front of
/// `onNotFound`, so a C++ build answers `401` there; see the note in
/// [`crate::web::start`] where the deliberate difference is recorded.
pub(crate) fn register_raw_api_not_found(
    server_handle: esp_idf_sys::httpd_handle_t,
) -> Result<(), EspError> {
    // SAFETY: `not_found_handler` has the `httpd_err_handler_func_t` signature
    // (`extern "C" fn(*mut httpd_req_t, httpd_err_code_t) -> esp_err_t`,
    // `bindings.rs:56556-56559`) and `HTTPD_404_NOT_FOUND` is a member of
    // `httpd_err_code_t` (`:56545`). `server_handle` is the live server's handle
    // from `RawHandle::handle`, which outlives the registration.
    let rc = unsafe {
        esp_idf_sys::httpd_register_err_handler(
            server_handle,
            esp_idf_sys::httpd_err_code_t_HTTPD_404_NOT_FOUND,
            Some(not_found_handler),
        )
    };
    if let Some(err) = EspError::from(rc) {
        return Err(err);
    }

    // **`405` as well, and this is the fix.** The preflight wildcard `/api*` is
    // a URI handler to ESP-IDF, so a mistyped `/api/...` path matches it with
    // the wrong method and ESP-IDF raises `405` rather than `404` — the `404`
    // handler above never ran, and the client got `text/html` where the C++
    // returns JSON. Registering the same handler for `405` puts the decision
    // back in our hands: `cc_web::help::unmatched` tells a mistyped URL (the
    // C++'s `404`) from a real route with a method it lacks (an honest `405`).
    //
    // SAFETY: as above — same signature, same enum member
    // (`HTTPD_405_METHOD_NOT_ALLOWED`, `bindings.rs:56548`).
    let rc = unsafe {
        esp_idf_sys::httpd_register_err_handler(
            server_handle,
            esp_idf_sys::httpd_err_code_t_HTTPD_405_METHOD_NOT_ALLOWED,
            Some(not_found_handler),
        )
    };
    match EspError::from(rc) {
        None => Ok(()),
        Some(err) => Err(err),
    }
}

/// The raw `extern "C"` `404` ESP-IDF calls when no URI handler matched.
///
/// Returns `ESP_OK` — "this request has been answered" — after writing the body
/// itself. ESP-IDF forces `ESP_FAIL` for a `500` regardless
/// (`httpd_txrx.c:600-601`), which is the only reason the return value here is
/// not always `ESP_FAIL`: a returned `ESP_FAIL` on a `404` would close the
/// socket, and the body this writes has to reach the client first.
///
/// A non-`/api/` path is handed straight back to `httpd_resp_send_err`, which is
/// ESP-IDF's own default `404` — the byte-for-byte answer the Rust port gave
/// before this existed, and the C++'s `text/plain` "Not found" (`:1023-1024`)
/// in everything but wording.
extern "C" fn not_found_handler(
    req: *mut httpd_req_t,
    error: esp_idf_sys::httpd_err_code_t,
) -> esp_idf_sys::esp_err_t {
    // SAFETY: `req` is non-null (checked by the caller below) and `httpd_req_t::uri`
    // is a fixed 513-byte NUL-terminated field (`bindings.rs:56468`,
    // "The URI of this request (1 byte extra for null termination)"), which is
    // the field `EspHttpConnection::uri` reads (`esp-idf-svc` `server.rs:949-955`).
    // There is no `httpd_req_get_url_str` in ESP-IDF v5.5.5 — only the
    // query-string variant — so this reads the field directly, exactly as the
    // crate that wraps this server does. The query string is included, which is
    // harmless: the path comes first, so `starts_with("/api/")` is decided by
    // the path alone.
    // SAFETY: as above — a live request on the httpd task, whose `uri` field
    // is NUL-terminated.
    let uri = if req.is_null() {
        ""
    } else {
        // SAFETY: as above — a live request on the httpd task, whose `uri` field
        // is NUL-terminated.
        unsafe { core::ffi::CStr::from_ptr((*req).uri.as_ptr()) }
            .to_str()
            .unwrap_or("")
    };
    // The query string rides along in `uri`, which is harmless for the decision:
    // the path comes first, and `starts_with("/api/")` is decided by the path.
    let path = uri.split(['?', '#']).next().unwrap_or(uri);

    // SAFETY: `req` is live on the httpd task and both branches write a complete
    // response through ESP-IDF's own accessors.
    unsafe {
        match cc_web::help::unmatched(path) {
            cc_web::help::Unmatched::NotApi => {
                return httpd_resp_send_err(req, error, c"Not found".as_ptr());
            }
            cc_web::help::Unmatched::Api => {
                // The status is set **explicitly**, and it has to be: ESP-IDF
                // does not put one on the request before calling a custom error
                // handler, so `httpd_resp_send` alone answers `200`. The C++
                // answers this request with `404` (`handleNotFound`), and the
                // status it raised here was `405` because the preflight
                // wildcard matched the URI with the wrong method — which is the
                // divergence this whole path exists to close.
                if httpd_resp_set_status(req, c"404 Not Found".as_ptr()) != ESP_OK {
                    return ESP_FAIL;
                }
            }
            cc_web::help::Unmatched::Served => {
                // A real route with a method it does not have. `405` is the
                // honest status and only the body was wrong — but it still has
                // to be written, for the reason above.
                if httpd_resp_set_status(req, c"405 Method Not Allowed".as_ptr()) != ESP_OK {
                    return ESP_FAIL;
                }
            }
        }
        if httpd_resp_set_type(req, c"application/json".as_ptr()) != ESP_OK {
            return ESP_FAIL;
        }
        // `not_found_json()` is a `&'static str`, so the pointer below outlives
        // the send and cannot dangle.
        let body = cc_web::help::not_found_json();
        httpd_resp_send(
            req,
            body.as_ptr().cast(),
            isize::try_from(body.len()).unwrap_or(0),
        )
    }
}
