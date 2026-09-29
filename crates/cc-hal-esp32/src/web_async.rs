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

use esp_idf_sys::{
    httpd_req_async_handler_begin, httpd_req_async_handler_complete, httpd_req_t,
    httpd_resp_send_chunk, ESP_OK,
};

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
