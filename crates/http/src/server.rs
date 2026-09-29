//! The connection loop, over an injected byte stream.
//!
//! The socket is a trait, not a type, so every test drives the real parse, route, authenticate,
//! rate-limit and write path against an in-memory stream. There is no "tested up to the socket"
//! gap: the only thing the tests do not exercise is the socket itself.
//!
//! Connections are handled one at a time and end with `Connection: close`. That is a deliberate
//! reduction from a general server. The frontend makes a handful of short requests on a LAN, the
//! C++ firmware's `ESP8266WebServer` already did one request per connection, and a keep-alive
//! implementation would need a read timeout and a connection table, which is two more ways to
//! exhaust 320 KB of RAM for no visible benefit.

use crate::parse;
use crate::request::RequestHead;
use crate::response::{write_chunk, write_last_chunk, Response, Status};
use crate::route::{Decision, Router};
use crate::sse::SseStream;
use crate::{writer::SliceWriter, MAX_BODY};

/// A byte stream.
///
/// Split into read and write so a caller can be full-duplex, and so a test can assert on what the
/// server produced without a second buffer it has to swap in.
pub trait Stream {
    /// Reads into `buf`, returning how many bytes arrived. `Ok(0)` means the peer closed.
    ///
    /// May return `Ok(n)` with fewer bytes than asked for. Callers must treat a short read as
    /// partial, not as a whole message.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamError>;

    /// Writes everything or fails. A partial write must not be reported as success, because an
    /// HTTP response truncated mid-body is worse than a closed connection.
    fn write_all(&mut self, buf: &[u8]) -> Result<(), StreamError>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StreamError {
    /// The peer went away. A client disconnecting mid-response is normal, not a fault.
    Closed,
    /// The socket refused the write. Common when a client stops reading.
    Failed,
    /// The request was refused. Carries the status to answer with, so the parser's own
    /// classification reaches the client instead of being flattened into a generic 400.
    BadRequest(crate::response::Status),
}

/// What the caller must decide that this crate cannot.
///
/// Authentication needs the stored credential and a comparison; rate limiting needs a clock and a
/// per-client identity. Both are injected rather than implemented here so this crate has no
/// dependency on the config store or on a timer, and so a test can drive them directly.
pub trait Guard {
    /// Whether the request may proceed. Called before the handler, and only for routes marked
    /// authenticated, so a handler cannot be reached for a protected route without the check.
    fn authenticate(&mut self, head: &RequestHead) -> bool;

    /// Whether this request is within the rate limit for this client. Called for every request,
    /// including the ones that will be answered 404, so a flood of bad paths is bounded too.
    fn allow_rate(&mut self, head: &RequestHead) -> bool;
}

/// A guard that allows everything.
///
/// Only for a build with authentication compiled out. A default that denied everything would
/// break the machine's own frontend, and a default that allowed everything would be a silent
/// security hole, so the two cases are separate types and the caller has to choose.
#[derive(Debug, Default)]
pub struct OpenGuard;

impl Guard for OpenGuard {
    fn authenticate(&mut self, _head: &RequestHead) -> bool {
        true
    }
    fn allow_rate(&mut self, _head: &RequestHead) -> bool {
        true
    }
}

/// A guard that refuses every credential but applies no rate limit.
///
/// The two questions are separate, so this refuses one and not the other. A guard that refused
/// both would make the 401 path untestable, because the rate limit is evaluated first and would
/// answer 429 before authentication was ever asked.
#[derive(Debug, Default)]
pub struct ClosedGuard;

impl Guard for ClosedGuard {
    fn authenticate(&mut self, _head: &RequestHead) -> bool {
        false
    }
    fn allow_rate(&mut self, _head: &RequestHead) -> bool {
        true
    }
}

/// What a handler produced for one request.
///
/// Separate from [`Response`] so a handler cannot reach the socket directly: it returns bytes and
/// this type decides what is framed and written.
// The two variants differ in size because one carries a response's header block and the other an
// event queue. Both are bounded and neither grows at runtime, so the difference is a fixed cost of
// the union rather than a heap that can be exhausted.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Reply<'a> {
    /// A complete response, with a body whose length is known.
    Done(Response<'a>),
    /// An event stream. Drained by this crate's write loop, which frames it in chunks because its
    /// length is not known when the head goes out.
    ///
    /// Separate from `Done` rather than a body variant: a `Response` is built for every request, and
    /// an event queue inside it would put its whole buffer on the stack of every exchange,
    /// including the ones that never open a stream.
    Stream(SseStream),
}

/// Handles one request and returns what to write.
pub trait Handler {
    fn handle<'a>(&mut self, head: &RequestHead, body: &'a [u8]) -> Reply<'a>;
}

/// The read buffer size. A request head is at most a few hundred bytes and the largest body the
/// machine accepts is a config import, so this holds a head plus a body without splitting either.
pub const READ_BUFFER: usize = MAX_BODY + 1024;

/// The write buffer. A firmware image is served from flash, not buffered, so this only ever holds
/// a response head and one chunk.
pub const WRITE_BUFFER: usize = 1024;

/// Runs one connection to completion.
///
/// Returns the number of requests served: 1 for a client that sent a request, 0 for one that closed
/// without sending. A request that could not be parsed is answered with the parser's own status and
/// then reported as [`StreamError::BadRequest`], so a caller logging the reason gets the specific
/// one rather than a generic failure.
pub fn serve_connection<S, G, H>(
    stream: &mut S,
    router: &Router<'_>,
    guard: &mut G,
    handler: &mut H,
) -> Result<usize, StreamError>
where
    S: Stream,
    G: Guard,
    H: Handler,
{
    let mut buffer = [0u8; READ_BUFFER];
    let mut served = 0usize;

    let (head, consumed) = match read_head(stream, &mut buffer) {
        Ok(Some(pair)) => pair,
        // The client closed cleanly without sending anything. Not an error.
        Ok(None) => return Ok(served),
        Err(e) => {
            // A malformed request still gets an answer, with the parser's own status, before the
            // reason is reported upward. Silently dropping the connection is what leaves a user
            // staring at a browser error with nothing in the log.
            let status = match e {
                StreamError::BadRequest(status) => status,
                other => return Err(other),
            };
            write_reply(stream, Reply::Done(parse_error_response(status)))?;
            return Err(e);
        }
    };
    served += 1;

    let reply = decide(
        router,
        guard,
        handler,
        &head,
        &buffer[consumed..consumed + body_len(&head)],
    );
    write_reply(stream, reply)?;
    // Every response ends with `Connection: close`, so one request per connection is the whole
    // protocol. Returning here rather than looping keeps that true even if a response is later
    // changed to keep-alive.
    Ok(served)
}

/// Reads one request head, plus whatever body bytes arrived with it.
///
/// Returns `Ok(None)` when the client closed without sending a whole request, and
/// `Err(StreamError::BadRequest)` carrying the reason when the head is complete but malformed. The
/// distinction matters: the first is a client that hung up, the second is a client that sent
/// something wrong and deserves an answer.
///
/// The head is complete once its blank line has arrived. Before that a parse failure just means
/// "not enough bytes yet", so a client that dribbles its request out three bytes at a time is not
/// mistaken for one that sent nonsense.
fn read_head<S: Stream>(
    stream: &mut S,
    buffer: &mut [u8; READ_BUFFER],
) -> Result<Option<(RequestHead, usize)>, StreamError> {
    let mut filled = 0usize;
    loop {
        let n = stream.read(&mut buffer[filled..])?;
        if n == 0 {
            // Closed. If nothing arrived, that is a clean close; if a partial head arrived, there
            // is nothing to answer.
            return Ok(None);
        }
        filled += n;
        // Try to parse as soon as there might be a complete head.
        let head_end = has_blank_line(&buffer[..filled]);
        match parse::parse(&buffer[..filled]) {
            Ok((head, offset)) => return Ok(Some((head, offset))),
            Err(e) => {
                // A complete head that will not parse is a client error with a specific answer,
                // not something to keep reading for. Answering it is the difference between "413
                // Payload Too Large" and a hang.
                if head_end {
                    return Err(StreamError::BadRequest(e.status()));
                }
            }
        }
        if filled >= MAX_HEAD_BYTES {
            return Err(StreamError::BadRequest(crate::response::Status::UriTooLong));
        }
    }
}

/// Whether the buffer holds a blank line, which is what ends a request head.
fn has_blank_line(buffer: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < buffer.len() {
        if buffer[i] == b'\r' && buffer[i + 1] == b'\n' && buffer.get(i + 2) == Some(&b'\r') {
            return true;
        }
        if buffer[i] == b'\n' && buffer[i + 1] == b'\n' {
            return true;
        }
        i += 1;
    }
    false
}

/// The point past which a partial head is treated as hostile rather than as a slow client.
const MAX_HEAD_BYTES: usize = 8 * 1024;

/// The response for a request the parser refused.
///
/// The body names the reason's class rather than echoing anything the client sent: the C++
/// firmware's 404 reflected the requested path back into the body, which turned every endpoint
/// into a reflector for anything a browser would render.
fn parse_error_response(status: Status) -> Response<'static> {
    let body: &'static str = match status {
        Status::UriTooLong => r#"{"error":"request too large"}"#,
        Status::PayloadTooLarge => r#"{"error":"body too large"}"#,
        Status::NotImplemented => r#"{"error":"transfer encoding not supported"}"#,
        _ => r#"{"error":"malformed request"}"#,
    };
    Response::json(status, body)
}

/// The body length declared by a head that has already been bounds-checked by the parser.
fn body_len(head: &RequestHead) -> usize {
    head.content_length.unwrap_or(0)
}

/// Applies the router, the guard and the handler, in that order.
fn decide<'a, G: Guard, H: Handler>(
    router: &Router<'_>,
    guard: &mut G,
    handler: &mut H,
    head: &RequestHead,
    body: &'a [u8],
) -> Reply<'a> {
    // The rate limit comes first and applies to everything. A client that can make the server do
    // unbounded work with requests that all 404 is still a client that can make the server do
    // unbounded work.
    if !guard.allow_rate(head) {
        return Reply::Done(Response::json(
            Status::TooManyRequests,
            r#"{"error":"too many requests"}"#,
        ));
    }

    let decision = router.decide(head);
    if let Some(response) = router.pre_handled(decision) {
        return Reply::Done(response);
    }

    let Decision::Matched { index } = decision else {
        // Unreachable: `pre_handled` answered every other decision. Written as a fail-closed
        // rather than an `unreachable!()` so a future decision variant cannot panic the server.
        return Reply::Done(Response::json(
            Status::InternalServerError,
            r#"{"error":"no handler"}"#,
        ));
    };

    // Authentication is a property of the route, checked here, before the handler is reached.
    if router.is_authenticated(index) && !guard.authenticate(head) {
        return Reply::Done(Response::json(
            Status::Unauthorized,
            r#"{"error":"unauthorized"}"#,
        ));
    }

    handler.handle(head, body)
}

/// Writes a reply, framing a stream in chunks and terminating it with a zero-length chunk.
fn write_reply<S: Stream>(stream: &mut S, reply: Reply<'_>) -> Result<(), StreamError> {
    let mut out = [0u8; WRITE_BUFFER];
    match reply {
        Reply::Done(response) => {
            let n = response.write_head(&mut out);
            stream.write_all(&out[..n])?;
            if let Some(bytes) = response.body.bytes() {
                stream.write_all(bytes)?;
            }
            Ok(())
        }
        Reply::Stream(mut events) => {
            // The head goes out first with chunked framing. The stream is bounded at 16 events,
            // so a client that connects and never reads gets the queue filled, the producer starts
            // dropping, and nothing blocks.
            let n = chunked_head(&mut out);
            stream.write_all(&out[..n])?;
            while let Some(event) = events.pop() {
                let mut frame = [0u8; 256];
                let m = event.encode(&mut frame);
                let written = write_chunk(&mut out, &frame[..m]);
                if written == 0 {
                    // The frame does not fit the write buffer. Terminating is right: a
                    // half-written chunk leaves the client's declared length and its bytes
                    // disagreeing, and the stream cannot be recovered from that.
                    let mut term = [0u8; 8];
                    let n = write_last_chunk(&mut term);
                    stream.write_all(&term[..n])?;
                    return Ok(());
                }
                stream.write_all(&out[..written])?;
            }
            let n = write_last_chunk(&mut out);
            stream.write_all(&out[..n])?;
            Ok(())
        }
    }
}

/// The status line and headers for a chunked response.
fn chunked_head(out: &mut [u8]) -> usize {
    let mut w = SliceWriter::new(out);
    let _ = core::fmt::Write::write_fmt(&mut w, format_args!("HTTP/1.1 200 OK\r\n"));
    let _ =
        core::fmt::Write::write_fmt(&mut w, format_args!("Content-Type: text/event-stream\r\n"));
    let _ = core::fmt::Write::write_fmt(&mut w, format_args!("Cache-Control: no-cache\r\n"));
    let _ = core::fmt::Write::write_fmt(&mut w, format_args!("Transfer-Encoding: chunked\r\n"));
    let _ = core::fmt::Write::write_fmt(&mut w, format_args!("Connection: close\r\n\r\n"));
    w.written()
}

// ---------------------------------------------------------------------------
// Test support
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod fake {
    // The crate is `no_std` for the target. The test fake needs `Vec` and `String`, which the
    // target has no use for: a socket buffer is a fixed array, and the fake exists only on the
    // host so that the real parse, route and write path can be exercised without hardware.
    extern crate std;

    use super::*;
    use std::{string::String, vec::Vec};

    /// An in-memory stream: the client writes a request in, the test reads what came out.
    #[derive(Debug, Default)]
    pub struct FakeStream {
        /// What the client sent.
        pub incoming: Vec<u8>,
        /// How much of `incoming` has been read.
        read_up_to: usize,
        /// What the server wrote.
        pub outgoing: Vec<u8>,
        /// How many bytes each read returns, so a test can split a request across writes.
        pub chunk: usize,
        /// Whether the next read reports a closed peer.
        pub peer_closed: bool,
    }

    impl FakeStream {
        pub fn new(incoming: &[u8]) -> Self {
            Self {
                incoming: incoming.to_vec(),
                chunk: incoming.len().max(1),
                ..Default::default()
            }
        }

        pub fn with_chunk(mut self, chunk: usize) -> Self {
            self.chunk = chunk.max(1);
            self
        }

        pub fn closed(mut self) -> Self {
            self.peer_closed = true;
            self
        }

        /// The whole response as text.
        pub fn response(&self) -> String {
            String::from_utf8_lossy(&self.outgoing).into_owned()
        }
    }

    impl Stream for FakeStream {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamError> {
            if self.peer_closed || self.read_up_to >= self.incoming.len() {
                return Ok(0);
            }
            let remaining = &self.incoming[self.read_up_to..];
            let n = remaining.len().min(buf.len()).min(self.chunk);
            buf[..n].copy_from_slice(&remaining[..n]);
            self.read_up_to += n;
            Ok(n)
        }

        fn write_all(&mut self, buf: &[u8]) -> Result<(), StreamError> {
            self.outgoing.extend_from_slice(buf);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::request::Method;
    use crate::response::Body;

    use super::fake::FakeStream;
    use super::*;
    use crate::route::Route;
    use heapless::Vec as HV;
    use std::{string::String, vec::Vec};

    static ROUTES: &[Route<'_>] = &[
        Route {
            path: "/api/state",
            methods: &[Method::Get],
            authenticated: false,
        },
        Route {
            path: "/api/setpoint",
            methods: &[Method::Post],
            authenticated: true,
        },
    ];

    struct Echo;

    impl Handler for Echo {
        fn handle<'a>(&mut self, head: &RequestHead, body: &'a [u8]) -> Reply<'a> {
            if head.path.as_str() == "/api/echo" {
                return Reply::Done(Response::with_body(Status::Ok, Body::Bytes(body)));
            }
            Reply::Done(Response::json(Status::Ok, r#"{"ok":true}"#))
        }
    }

    fn router() -> Router<'static> {
        Router::new(ROUTES)
    }

    fn run<G: Guard>(request: &[u8], guard: &mut G) -> (String, Result<usize, StreamError>) {
        let mut stream = FakeStream::new(request);
        let result = serve_connection(&mut stream, &router(), guard, &mut Echo);
        (stream.response(), result)
    }

    fn get(path: &str) -> Vec<u8> {
        let mut r = std::vec::Vec::new();
        r.extend_from_slice(b"GET ");
        r.extend_from_slice(path.as_bytes());
        r.extend_from_slice(b" HTTP/1.1\r\nHost: kitchen\r\n\r\n");
        r
    }

    #[test]
    fn a_get_is_answered() {
        let (response, result) = run(&get("/api/state"), &mut OpenGuard);
        assert_eq!(result, Ok(1));
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains(r#"{"ok":true}"#));
    }

    #[test]
    fn the_body_is_handed_to_the_handler_at_the_offset_the_parser_reported() {
        static ECHO_ROUTES: &[Route<'_>] = &[Route {
            path: "/api/echo",
            methods: &[Method::Post],
            authenticated: false,
        }];
        let mut stream =
            FakeStream::new(b"POST /api/echo HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello");
        let mut guard = OpenGuard;
        let result = serve_connection(
            &mut stream,
            &Router::new(ECHO_ROUTES),
            &mut guard,
            &mut Echo,
        );
        assert_eq!(result, Ok(1));
        assert!(
            stream.response().ends_with("hello"),
            "got {}",
            stream.response()
        );
    }

    #[test]
    fn a_request_split_across_reads_is_still_answered() {
        // A client that dribbles its request out three bytes at a time must not be treated as
        // hostile. This is the case a naive "parse whatever arrived" server gets wrong.
        let mut stream = FakeStream::new(&get("/api/state")).with_chunk(3);
        let mut guard = OpenGuard;
        let result = serve_connection(&mut stream, &router(), &mut guard, &mut Echo);
        assert_eq!(result, Ok(1));
        assert!(stream.response().starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn a_client_that_closes_without_sending_gets_nothing_and_no_error() {
        let mut stream = FakeStream::new(b"").closed();
        let mut guard = OpenGuard;
        let result = serve_connection(&mut stream, &router(), &mut guard, &mut Echo);
        assert_eq!(result, Ok(0));
        assert!(
            stream.outgoing.is_empty(),
            "no response may be written to a closed peer"
        );
    }

    #[test]
    fn an_unknown_path_is_404_and_does_not_reach_the_handler() {
        let (response, _) = run(&get("/nope"), &mut OpenGuard);
        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn a_protected_route_without_credentials_is_401() {
        // The handler is never reached, which is the property: the C++ firmware had three OTA
        // routes with no authentication call at all (defect D01).
        let mut request = Vec::new();
        request.extend_from_slice(b"POST /api/setpoint HTTP/1.1\r\nContent-Length: 0\r\n\r\n");
        let (response, _) = run(&request, &mut ClosedGuard);
        assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
    }

    #[test]
    fn a_public_route_is_served_even_when_the_guard_denies_authentication() {
        // The guard is asked about authentication, not about permission in general. A machine with
        // auth enabled still serves the dashboard.
        let (response, _) = run(&get("/api/state"), &mut ClosedGuard);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
    }

    /// A guard that refuses the rate limit but allows authentication, so the rate-limit path can
    /// be tested without also testing the authentication path.
    struct RefuseRate;

    impl Guard for RefuseRate {
        fn authenticate(&mut self, _: &RequestHead) -> bool {
            true
        }
        fn allow_rate(&mut self, _: &RequestHead) -> bool {
            false
        }
    }

    #[test]
    fn the_rate_limit_applies_before_routing_so_a_flood_of_bad_paths_is_bounded() {
        // Otherwise a client can make the server do unbounded work with requests that all 404.
        let (response, _) = run(&get("/nope"), &mut RefuseRate);
        assert!(response.starts_with("HTTP/1.1 429 Too Many Requests"));
    }

    #[test]
    fn a_wrong_method_is_405_with_an_allow_header() {
        let mut request = Vec::new();
        request.extend_from_slice(b"POST /api/state HTTP/1.1\r\nContent-Length: 0\r\n\r\n");
        let (response, _) = run(&request, &mut OpenGuard);
        assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed"));
        assert!(response.contains("Allow: GET"));
    }

    #[test]
    fn a_parse_error_is_answered_with_the_status_that_explains_it() {
        let (response, _) = run(
            b"GET /nope HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n",
            &mut OpenGuard,
        );
        assert!(
            response.starts_with("HTTP/1.1 413 Payload TooLarge"),
            "an oversized body must say so, got {}",
            response.lines().next().unwrap_or("")
        );
    }

    #[test]
    fn a_chunked_request_is_refused_with_501() {
        let (response, _) = run(
            b"POST /api/setpoint HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            &mut OpenGuard,
        );
        assert!(response.starts_with("HTTP/1.1 501 Not Implemented"));
    }

    #[test]
    fn a_stream_is_chunked_and_terminated() {
        // A stream that just stops is a truncated response. The terminal chunk is what says the
        // stream is over, and the C++ SSE endpoint did not send one.
        static SSE_ROUTES: &[Route<'_>] = &[Route {
            path: "/events",
            methods: &[Method::Get],
            authenticated: false,
        }];
        struct Streams;
        impl Handler for Streams {
            fn handle<'a>(&mut self, _: &RequestHead, _: &'a [u8]) -> Reply<'a> {
                let mut s = SseStream::new();
                s.push("machineState", r#"{"state":33}"#);
                s.push("temperature", r#"{"celsius":92.5}"#);
                Reply::Stream(s)
            }
        }
        let mut stream = FakeStream::new(&get("/events"));
        let mut guard = OpenGuard;
        let result = serve_connection(
            &mut stream,
            &Router::new(SSE_ROUTES),
            &mut guard,
            &mut Streams,
        );
        assert_eq!(result, Ok(1));
        let response = stream.response();
        assert!(response.contains("Transfer-Encoding: chunked"));
        assert!(response.contains("event: machineState"));
        assert!(response.contains("event: temperature"));
        assert!(
            response.ends_with("0\r\n\r\n"),
            "the stream must end with a terminal chunk, got the last 20 bytes of {:?}",
            &response[response.len().saturating_sub(20)..]
        );
    }

    #[test]
    fn one_request_per_connection_is_what_the_protocol_does() {
        // Two requests on one connection: the second is not answered, because every response
        // carries `Connection: close`. Asserted so the reduction is documented rather than
        // accidental.
        let mut both = get("/api/state");
        both.extend_from_slice(&get("/api/state"));
        let (response, result) = run(&both, &mut OpenGuard);
        assert_eq!(result, Ok(1));
        assert_eq!(response.matches("HTTP/1.1 200 OK").count(), 1);
    }

    #[test]
    fn a_partial_head_that_never_completes_is_refused_rather_than_waited_on() {
        // Bounded: a client that opens a connection and sends a target forever must not be able to
        // hold a task.
        let mut stream = FakeStream::new(b"GET /");
        stream.incoming.extend(std::iter::repeat_n(b'x', 16 * 1024));
        let mut guard = OpenGuard;
        let result = serve_connection(&mut stream, &router(), &mut guard, &mut Echo);
        assert_eq!(result, Err(StreamError::BadRequest(Status::UriTooLong)));
    }

    #[test]
    fn a_long_query_parameter_value_survives_to_the_handler() {
        // The firmware-from-URL route carries a URL in the query, which is the longest target the
        // machine serves.
        let mut request = get("/api/state?url=https%3A%2F%2Fexample.com%2Ffirmware.bin");
        request.truncate(request.len() - 2);
        request.extend_from_slice(b"\r\n\r\n");
        let mut stream = FakeStream::new(&request);
        let mut guard = OpenGuard;
        assert_eq!(
            serve_connection(&mut stream, &router(), &mut guard, &mut Echo),
            Ok(1)
        );
    }

    #[test]
    fn every_registered_route_is_reachable_through_the_loop() {
        // A route that the loop can never answer is a route that only exists in a table.
        let mut served: HV<&str, 8> = HV::new();
        for route in ROUTES {
            let mut request = Vec::new();
            request.extend_from_slice(route.methods[0].as_str().as_bytes());
            request.extend_from_slice(b" ");
            request.extend_from_slice(route.path.as_bytes());
            request.extend_from_slice(b" HTTP/1.1\r\n");
            if route.methods[0].expects_body() {
                request.extend_from_slice(b"Content-Length: 0\r\n");
            }
            request.extend_from_slice(b"\r\n");
            let mut guard = OpenGuard;
            let mut stream = FakeStream::new(&request);
            assert_eq!(
                serve_connection(&mut stream, &router(), &mut guard, &mut Echo),
                Ok(1),
                "{} was not served",
                route.path
            );
            assert!(
                stream.response().starts_with("HTTP/1.1 200"),
                "{} was not answered",
                route.path
            );
            served.push(route.path).ok();
        }
        assert_eq!(served.len(), ROUTES.len());
    }
}
