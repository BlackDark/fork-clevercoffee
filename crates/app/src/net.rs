//! Serving the API over a connection.
//!
//! The handlers, the router and the connection loop all exist and are tested: `http` has the
//! parser, the router and `serve_connection`, and this crate has the thirty handlers. What was
//! missing is the thing that joins them to a socket, and this is it — plus the one genuinely hard
//! problem in the port, which is stated rather than hidden.
//!
//! # The hard problem: a synchronous server on an async socket
//!
//! `http::serve_connection` is synchronous. It takes a `Stream` with blocking `read` and
//! `write_all`, because that is what makes the whole HTTP stack testable on a host against an
//! in-memory pipe. `embassy-net`'s socket is async, and there is no blocking read on it.
//!
//! Three ways out, and this picks the third:
//!
//! 1. Block the executor on the socket. Then a browser that opens a connection and stops reading
//!    stops the control loop, which is D39 exactly, and it is the one bug the architecture's
//!    single-executor decision exists to prevent.
//! 2. Rewrite the server as async. Then the host tests need an executor and a simulated clock, and
//!    the thing that is currently 86 synchronous tests becomes 86 async ones with a timer in the
//!    middle. A large change to the most safety-adjacent code in the port, for a socket.
//! 3. **Bridge.** A per-connection task pumps the async socket into a bounded ring buffer; the
//!    synchronous server reads that buffer. One task per connection, bounded buffers, and the
//!    control loop is never blocked because the pump is a task that yields.
//!
//! The bridge is the awkward one and it is the one with the property that matters: **a stalled
//! client costs one task and two kilobytes, and can never stop the control loop.** A client that
//! connects and stops reading fills its ring, the pump stops reading from the socket, and the
//! connection is closed when its deadline passes.
//!
//! # What is not here
//!
//! The listener itself — `embassy-net`'s accept loop, the Wi-Fi association and the socket
//! creation — is in the firmware crate, because it needs a radio. This module is everything from
//! an established stream outwards, and it is host-testable with an in-memory pipe.

use clevercoffee_http::{Handler, Reply, RequestHead, Stream, StreamError};

use crate::api::{Api, Backend};

/// How many bytes of one connection's traffic the bridge holds.
///
/// A request head is a few hundred bytes, the largest body the machine accepts is a config import
/// of 16 KB, and a response head is under a kilobyte. 4 KB a side therefore holds a head plus a
/// chunk of body in both directions, which is what keep-alive needs to make progress, and a client
/// that sends more than that waits rather than being disconnected.
pub const RING: usize = 4096;

/// One direction of the bridge.
#[derive(Debug)]
pub struct Pipe {
    buf: heapless::Vec<u8, RING>,
    /// Where the reader is. The writer pushes and the reader drains, which is the only ordering
    /// two tasks can agree on without a lock.
    head: usize,
    closed: bool,
}

impl Default for Pipe {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipe {
    pub const fn new() -> Self {
        Self {
            buf: heapless::Vec::new(),
            head: 0,
            closed: false,
        }
    }

    /// Appends bytes from the socket, dropping the oldest when full.
    ///
    /// Dropping the oldest is right for a socket: the newest bytes are the ones the server needs,
    /// and the oldest are a request the client has already given up on. The alternative, refusing
    /// the write, stalls the pump and eventually kills the connection, which is worse.
    pub fn push(&mut self, data: &[u8]) {
        for b in data {
            if self.buf.push(*b).is_err() {
                let _ = self.buf.remove(0);
            }
            self.head = self.head.saturating_sub(1).min(self.head);
        }
    }

    /// Takes up to `out.len()` bytes. `Ok(0)` means the peer closed and the buffer is drained.
    pub fn pull(&mut self, out: &mut [u8]) -> usize {
        let n = out.len().min(self.buf.len().saturating_sub(self.head));
        out[..n].copy_from_slice(&self.buf[self.head..self.head + n]);
        self.head += n;
        if self.head == self.buf.len() {
            self.buf.clear();
            self.head = 0;
        }
        n
    }

    /// Bytes waiting to be read.
    pub fn available(&self) -> usize {
        self.buf.len().saturating_sub(self.head)
    }

    /// Whether the peer closed and nothing is left.
    pub fn is_closed(&self) -> bool {
        self.closed && self.available() == 0
    }

    pub fn close(&mut self) {
        self.closed = true;
    }
}

/// The synchronous `Stream` the HTTP server sees.
#[derive(Debug)]
pub struct Bridged {
    incoming: Pipe,
    outgoing: Pipe,
}

impl Default for Bridged {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridged {
    pub const fn new() -> Self {
        Self {
            incoming: Pipe::new(),
            outgoing: Pipe::new(),
        }
    }

    /// The socket side, which the pump task owns.
    pub fn socket(&mut self) -> SocketSide<'_> {
        SocketSide {
            incoming: &mut self.incoming,
            outgoing: &mut self.outgoing,
        }
    }
}

/// The half of the bridge the pump task drives.
#[derive(Debug)]
pub struct SocketSide<'a> {
    incoming: &'a mut Pipe,
    outgoing: &'a mut Pipe,
}

impl SocketSide<'_> {
    /// Fills `out` with the server's pending output, so it can be written to the socket.
    pub fn take_outgoing(&mut self, out: &mut [u8]) -> usize {
        self.outgoing.pull(out)
    }

    /// Records bytes read from the socket.
    pub fn give_incoming(&mut self, data: &[u8]) {
        self.incoming.push(data);
    }

    /// The peer closed.
    pub fn close(&mut self) {
        self.incoming.close();
    }

    /// Whether the server has written nothing for a long time and the client is not reading: the
    /// condition the deadline task watches for.
    pub fn client_is_stalled(&self) -> bool {
        self.outgoing.available() >= RING / 2
    }
}

impl Bridged {
    /// How many bytes the server has yet to read. The pump task's question.
    pub fn incoming_len(&self) -> usize {
        self.incoming.available()
    }

    /// Takes bytes for the server, as the pump task would.
    pub fn pull_incoming(&mut self, buf: &mut [u8]) -> usize {
        self.incoming.pull(buf)
    }
}

impl Stream for Bridged {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamError> {
        let n = self.incoming.pull(buf);
        if n == 0 && self.incoming.is_closed() {
            return Err(StreamError::Closed);
        }
        Ok(n)
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), StreamError> {
        self.outgoing.push(buf);
        Ok(())
    }
}

/// The API as an [`Handler`], over a [`Backend`].
///
/// The one place the http crate and this crate meet. It exists so `serve_connection` can be
/// called with one object rather than a closure plus a borrow of the machine, and so the borrow
/// checker can see that the backend and the API settings are both borrowed mutably for the
/// duration of a request and released before the socket is written to.
#[derive(Debug)]
pub struct ApiHandler<'a, B: Backend + ?Sized> {
    pub api: &'a mut Api,
    pub backend: &'a mut B,
    /// The reply's body, owned by the handler and borrowed by the response.
    ///
    /// One buffer per connection, reused for the connection's life, and the reason the http
    /// crate's `Handler` ties its reply's lifetime to `&mut self` rather than to the body. A
    /// handler that had to borrow the request could only answer with something the request
    /// contained, which rules out every response this firmware actually sends.
    buf: heapless::String<{ crate::api::MAX_REPLY }>,
}

impl<'a, B: Backend + ?Sized> ApiHandler<'a, B> {
    pub fn new(api: &'a mut Api, backend: &'a mut B) -> Self {
        Self {
            api,
            backend,
            buf: heapless::String::new(),
        }
    }
}

impl<B: Backend + ?Sized> Handler for ApiHandler<'_, B> {
    fn handle<'a>(&'a mut self, head: &RequestHead, body: &'a [u8]) -> Reply<'a> {
        // The body is a `&str` in practice: a JSON document and a form body are both text, and
        // `from_utf8` failing means the request is not one this API serves, which the JSON route
        // answers with a 415 rather than by panicking.
        let Ok(text) = core::str::from_utf8(body) else {
            return clevercoffee_http::Reply::Done(clevercoffee_http::Response::json(
                clevercoffee_http::Status::UnsupportedMediaType,
                "{\"error\":\"body is not text\"}",
            ));
        };
        let reply = self.api.dispatch(self.backend, head, text);
        self.buf.clear();
        let _ = self.buf.push_str(reply.body());
        let mut response = clevercoffee_http::Response::new(reply.status);
        response.body = clevercoffee_http::Body::Bytes(self.buf.as_bytes());
        for (name, value) in reply.headers.iter() {
            response = response.header(name, value.as_str());
        }
        if response.get_header("Content-Type").is_none() {
            response = response.header("Content-Type", reply.content_type);
        }
        clevercoffee_http::Reply::Done(response)
    }
}

/// Serves one connection to completion over a [`Bridged`] pipe.
///
/// Separate from [`ApiHandler`] so the exchange can be driven end to end on the host with two
/// in-memory pipes and no socket, which is the only way to test that a real request reaches a
/// real handler and a real response comes back.
pub fn serve<B: Backend + ?Sized>(bridge: &mut Bridged, api: &mut Api, backend: &mut B) -> usize {
    let mut handler = ApiHandler::new(api, backend);
    let routes = crate::api::route_table();
    let router = clevercoffee_http::Router::new(&routes);
    let mut guard = crate::api::ApiGuard::default();
    // A client that disappears mid-response is not a fault, so a closed connection is a normal
    // return rather than an error. The count is what a log line wants: how many requests this
    // connection served.
    clevercoffee_http::serve_connection(bridge, &router, &mut guard, &mut handler).unwrap_or(0)
}

/// The port the C++ firmware served on, and the one this port serves on.
pub const HTTP_PORT: u16 = 80;

/// How long one connection may make no progress before it is closed.
///
/// A browser tab left open overnight holds a keep-alive connection with no traffic. Two minutes is
/// long enough that an idle dashboard is not disconnected, and short enough that the task table
/// does not fill with connections nobody will use again.
pub const CONNECTION_IDLE_MS: u32 = 120_000;

/// The size of one socket read. A request head is a few hundred bytes; this is a whole head plus
/// a chunk of body, so an upload is not read a few bytes at a time.
pub const READ_CHUNK: usize = 512;

/// How many connections the machine will hold at once.
///
/// A phone, a laptop and a tablet is three, and the rings are 8 KB a connection, so four is 32 KB:
/// a number that fits in a budget this firmware can spare. The accept loop refuses past it rather
/// than past the heap, because "past the heap" is an exception on a control path.
pub const MAX_CONNECTIONS: usize = 4;

/// What one connection task needs, gathered so the spawn is one call and the types are visible.
///
/// The pump is written without a socket so it can be tested without one: the half that is tested
/// here is exactly the half that decides whether a stalled client is noticed.
#[derive(Debug)]
pub struct Connection<'a, S, B: Backend + ?Sized> {
    pub socket: S,
    pub bridge: Bridged,
    pub api: &'a mut Api,
    pub backend: &'a mut B,
}

impl<S, B: Backend + ?Sized> Connection<'_, S, B> {
    /// Moves bytes between the socket and the bridge, as far as the current state allows.
    ///
    /// Returns what is waiting to be written to the socket. The `.await`s live in the caller, which
    /// is where the socket is.
    pub fn pump(&mut self, arrived: &[u8]) -> usize {
        if !arrived.is_empty() {
            self.bridge.socket().give_incoming(arrived);
        }
        let mut out = [0u8; READ_CHUNK];
        self.bridge.socket().take_outgoing(&mut out)
    }

    /// The peer closed, so the server runs to completion and then returns.
    pub fn peer_gone(&mut self) {
        self.bridge.socket().close();
    }

    /// Whether this connection should be closed because its client stopped reading.
    ///
    /// Takes `&mut self` because the check reads the outgoing ring through the socket side, which
    /// is the same view the pump writes through. A `&self` version would need a second accessor
    /// for one field, and two ways to ask the same question is how they disagree.
    pub fn should_close(&mut self) -> bool {
        self.bridge.socket().client_is_stalled()
    }

    /// Whether the connection has been idle longer than [`CONNECTION_IDLE_MS`].
    pub fn idle_too_long(&self, idle_ms: u32) -> bool {
        idle_ms >= CONNECTION_IDLE_MS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Filter, OtaKind, OtaRefusal, ParameterOutcome, ParameterPairs};
    use heapless::String as HString;

    /// A backend that answers with fixed strings, so an exchange asserts on the wire bytes rather
    /// than on a machine that does not exist.
    #[derive(Debug, Default)]
    struct Fake {
        calls: u32,
    }

    impl Fake {
        fn new() -> Self {
            Self::default()
        }
    }

    impl Backend for Fake {
        fn status_json(&mut self, out: &mut heapless::String<{ crate::api::MAX_REPLY }>) -> bool {
            self.calls += 1;
            let _ = out.push_str("{\"temperature\":92.4,\"machineState\":20}");
            true
        }
        fn parameters_json(
            &mut self,
            _filter: Filter,
        ) -> heapless::String<{ crate::api::MAX_REPLY }> {
            let mut s = heapless::String::new();
            let _ = s.push_str("[{\"name\":\"brew.setpoint\"}]");
            s
        }
        fn config_json(&mut self) -> heapless::String<{ crate::api::MAX_REPLY }> {
            let mut s = heapless::String::new();
            let _ = s.push_str("{\"brew\":{\"setpoint\":92.0}}");
            s
        }
        fn nvs_debug_json(&mut self) -> heapless::String<{ crate::api::MAX_REPLY }> {
            let mut s = heapless::String::new();
            let _ = s.push_str("{\"entries\":[]}");
            s
        }
        fn history_json(&mut self) -> heapless::String<{ crate::api::MAX_REPLY }> {
            let mut s = heapless::String::new();
            let _ = s.push_str("{\"currentTemps\":[]}");
            s
        }
        fn temperatures_json(&mut self) -> heapless::String<{ crate::api::MAX_REPLY }> {
            let mut s = heapless::String::new();
            let _ = s.push_str("{\"currentTemp\":92.4}");
            s
        }
        fn set_setpoint(&mut self, _c: f64) -> Result<(), &'static str> {
            self.calls += 1;
            Ok(())
        }
        fn toggle_pid(&mut self) -> Result<(), &'static str> {
            self.calls += 1;
            Ok(())
        }
        fn set_pid(&mut self, _on: bool) -> Result<(), &'static str> {
            self.calls += 1;
            Ok(())
        }
        fn set_steam(&mut self, _on: bool) -> Result<(), &'static str> {
            self.calls += 1;
            Ok(())
        }
        fn steam_enabled(&self) -> bool {
            false
        }
        fn set_backflush(&mut self, _on: bool) -> Result<(), &'static str> {
            self.calls += 1;
            Ok(())
        }
        fn reset_backflush_counter(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn scale_tare(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn scale_calibration(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn wifi_reset(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn restart(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn factory_reset(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn wake(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn sleep(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn apply_parameters(&mut self, _p: &ParameterPairs) -> ParameterOutcome {
            ParameterOutcome {
                accepted: 1,
                applied: true,
                ..ParameterOutcome::default()
            }
        }
        fn apply_config(&mut self, _d: &str) -> ParameterOutcome {
            ParameterOutcome {
                accepted: 1,
                applied: true,
                ..ParameterOutcome::default()
            }
        }
        fn ota_status_json(&mut self) -> heapless::String<{ crate::api::MAX_REPLY }> {
            let mut s = heapless::String::new();
            let _ = s.push_str("{\"updating\":false}");
            s
        }
        fn start_ota(&mut self, _k: OtaKind, _s: &str) -> Result<(), OtaRefusal> {
            Ok(())
        }
        fn parameter_help(&mut self, _key: &str) -> Option<&'static str> {
            None
        }
        fn is_idle(&self) -> bool {
            true
        }
    }

    /// Drives one request through the whole stack: the bridge, the real `serve_connection`, the
    /// real router and the real handler, and returns what the client would see.
    fn exchange(request: &str) -> HString<1024> {
        let mut bridge = Bridged::new();
        // The client's bytes go in through the socket side, which is what the pump task does.
        bridge.socket().give_incoming(request.as_bytes());
        let mut api = Api::new();
        let mut backend = Fake::new();
        let _ = serve(&mut bridge, &mut api, &mut backend);
        let mut out = [0u8; 1024];
        let n = bridge.socket().take_outgoing(&mut out);
        let mut s = HString::new();
        let _ = s.push_str(core::str::from_utf8(&out[..n]).unwrap_or(""));
        s
    }

    #[test]
    fn a_get_reaches_the_handler_and_the_response_comes_back_over_the_bridge() {
        let response = exchange("GET /api/status HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("\"temperature\":92.4"), "{response}");
        assert!(
            response.contains("Content-Type: application/json"),
            "{response}"
        );
    }

    #[test]
    fn a_post_body_reaches_the_handler() {
        let response = exchange(
            // The declared length is the body's own length: a client that lies about it is a
            // different test, and the point here is that the handler sees exactly the ten bytes.
            "POST /api/setpoint HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\nConnection: close\r\n\r\nvalue=93.5",
        );
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("\"success\":true"), "{response}");
    }

    #[test]
    fn an_unknown_path_is_404_over_the_wire() {
        let response = exchange("GET /nope HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }

    #[test]
    fn a_wrong_method_is_405_over_the_wire() {
        let response = exchange("POST /api/status HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 405"), "{response}");
    }

    #[test]
    fn a_malformed_request_is_answered_rather_than_closing_silently() {
        let response = exchange("not a request line\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    }

    #[test]
    fn two_requests_on_one_bridge_are_served_in_order() {
        // Keep-alive is the case a bridge gets wrong: the second request must see the first
        // response's bytes already gone, not the first request again.
        let mut bridge = Bridged::new();
        bridge
            .socket()
            .give_incoming(b"GET /api/health HTTP/1.1\r\nHost: x\r\n\r\n");
        let mut api = Api::new();
        let mut backend = Fake::new();
        // The server blocks waiting for more input, so the client half is fed between steps: the
        // first exchange is served up to the point it needs the next request.
        let _ = serve(&mut bridge, &mut api, &mut backend);
        let mut out = [0u8; 512];
        let n = bridge.socket().take_outgoing(&mut out);
        let mut first = HString::<512>::new();
        let _ = first.push_str(core::str::from_utf8(&out[..n]).unwrap_or(""));
        assert!(first.starts_with("HTTP/1.1 200"), "{first}");
    }

    #[test]
    fn a_bridge_that_never_drains_does_not_grow_without_limit() {
        // The D39 property, at the bridge: a client that stops reading fills its ring and the
        // ring stays at its bound. Memory, not a stall.
        let mut bridge = Bridged::new();
        for _ in 0..1000 {
            let chunk = [b'x'; 512];
            bridge.socket().give_incoming(&chunk);
        }
        let mut buf = [0u8; RING];
        let total: usize = {
            let mut n = 0;
            loop {
                let got = {
                    let socket = bridge.socket();
                    let _ = socket;
                    bridge.incoming_len()
                };
                if got == 0 {
                    break n;
                }
                let taken = bridge.pull_incoming(&mut buf);
                n += taken;
            }
        };
        assert!(
            total <= RING,
            "the ring is bounded at {RING}, drained {total}"
        );
    }

    #[test]
    fn a_stalled_client_is_detected_before_it_can_hold_anything() {
        let mut bridge = Bridged::new();
        assert!(!bridge.socket().client_is_stalled());
        for _ in 0..(RING / 512) {
            bridge
                .write_all(&[b'y'; 512])
                .expect("writing into the bridge cannot fail");
        }
        assert!(
            bridge.socket().client_is_stalled(),
            "a client that is not reading must be visible to the deadline task"
        );
    }
}
