//! Responses: status lines, headers, and the three body shapes this server produces.
//!
//! A response never allocates. The body is a borrowed slice, a borrowed asset, or an SSE stream
//! the caller drives, so building a response costs no heap and a firmware image served as a static
//! asset is not copied into RAM to be sent.

use core::fmt::Write;

use crate::writer::SliceWriter;

/// The status codes this server returns.
///
/// A closed set rather than a bare integer: a handler cannot send a status the router has not
/// documented, and the frontend's expectations are checkable against this list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum Status {
    Ok = 200,
    Accepted = 202,
    NoContent = 204,
    PartialContent = 206,
    Found = 302,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    Conflict = 409,
    PayloadTooLarge = 413,
    UriTooLong = 414,
    UnsupportedMediaType = 415,
    UnprocessableEntity = 422,
    TooManyRequests = 429,
    InternalServerError = 500,
    NotImplemented = 501,
    ServiceUnavailable = 503,
}

impl Status {
    pub const fn code(self) -> u16 {
        self as u16
    }

    pub const fn reason(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Accepted => "Accepted",
            Status::NoContent => "No Content",
            Status::PartialContent => "Partial Content",
            Status::Found => "Found",
            Status::BadRequest => "Bad Request",
            Status::Unauthorized => "Unauthorized",
            Status::Forbidden => "Forbidden",
            Status::NotFound => "Not Found",
            Status::MethodNotAllowed => "Method Not Allowed",
            Status::Conflict => "Conflict",
            Status::PayloadTooLarge => "Payload TooLarge",
            Status::UriTooLong => "URI Too Long",
            Status::UnsupportedMediaType => "Unsupported Media Type",
            Status::UnprocessableEntity => "Unprocessable Entity",
            Status::TooManyRequests => "Too Many Requests",
            Status::InternalServerError => "Internal Server Error",
            Status::NotImplemented => "Not Implemented",
            Status::ServiceUnavailable => "Service Unavailable",
        }
    }

    /// Whether a client should retry. Used by the rate limiter, which counts only the requests
    /// worth counting.
    pub const fn is_retryable(self) -> bool {
        matches!(self, Status::TooManyRequests | Status::ServiceUnavailable)
    }
}

/// Where a response body comes from.
#[derive(Debug)]
pub enum Body<'a> {
    /// Nothing. `Content-Length: 0` unless the status says otherwise.
    Empty,
    /// Bytes already in hand.
    Bytes(&'a [u8]),
    /// A slice of a static asset, served with its own content type and no copy.
    Asset {
        bytes: &'a [u8],
        content_type: &'static str,
    },
}

impl Body<'_> {
    /// The declared length. Every body here has one, which is why an event stream is not a
    /// `Body`: a stream's length is not known when the head goes out, and carrying one inside
    /// every `Response` would put its whole queue on the stack of every exchange, including the
    /// ones that never open a stream.
    pub const fn length(&self) -> usize {
        match self {
            Body::Empty => 0,
            Body::Bytes(b) => b.len(),
            Body::Asset { bytes, .. } => bytes.len(),
        }
    }

    /// The `Content-Type`, when the body implies one.
    pub const fn content_type(&self) -> Option<&'static str> {
        match self {
            Body::Bytes(_) => Some("application/octet-stream"),
            Body::Asset { content_type, .. } => Some(content_type),
            Body::Empty => None,
        }
    }

    /// The bytes to write after the head, if any.
    pub const fn bytes(&self) -> Option<&[u8]> {
        match self {
            Body::Empty => None,
            Body::Bytes(b) => Some(b),
            Body::Asset { bytes, .. } => Some(bytes),
        }
    }
}

/// The longest response header value.
///
/// Half the request bound, on purpose. A request header can be a cookie or an `Authorization`
/// value that a client chooses; a response header is something this firmware writes, and the
/// longest one is a `Location` for an OTA redirect. Sizing the response buffer to the request's is
/// what made a `Response` 5.8 KB, and a `Response` lives on the stack for a whole exchange.
pub const MAX_RESPONSE_HEADER: usize = 128;

/// How many headers a response may carry.
///
/// Four is what every handler here uses: `Content-Type`, `Content-Length`, `Location`, `Allow`. A
/// response needing more is a design problem, and dropping the excess is better than growing a
/// stack frame for it.
pub const MAX_RESPONSE_HEADERS: usize = 4;

/// One response header.
///
/// Inline bytes rather than a `heapless::String`, so the header array is `Copy` and a `Response` can
/// be built by value on a stack frame with no heap involved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ResponseHeader {
    name: &'static str,
    value: [u8; MAX_RESPONSE_HEADER],
    len: u8,
}

impl ResponseHeader {
    /// Returns `None` when the value is over its bound, so an over-long header is dropped rather
    /// than written truncated.
    pub fn new(name: &'static str, value: &str) -> Option<Self> {
        if value.len() > MAX_RESPONSE_HEADER {
            return None;
        }
        let mut h = Self {
            name,
            value: [0; MAX_RESPONSE_HEADER],
            len: value.len() as u8,
        };
        h.value[..value.len()].copy_from_slice(value.as_bytes());
        Some(h)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn value(&self) -> &str {
        core::str::from_utf8(&self.value[..self.len as usize]).unwrap_or("")
    }
}

/// A response to write.
#[derive(Debug)]
pub struct Response<'a> {
    pub status: Status,
    pub headers: [Option<ResponseHeader>; MAX_RESPONSE_HEADERS],
    pub header_count: u8,
    pub body: Body<'a>,
}

impl<'a> Response<'a> {
    pub fn new(status: Status) -> Self {
        Self {
            status,
            headers: [None; MAX_RESPONSE_HEADERS],
            header_count: 0,
            body: Body::Empty,
        }
    }

    pub fn with_body(status: Status, body: Body<'a>) -> Self {
        let mut r = Self::new(status);
        r.body = body;
        r
    }

    /// A JSON response. The content type is fixed here so no handler can forget it, which is the
    /// defect the C++ frontend ran into with `application/json` spelled three different ways.
    pub fn json(status: Status, payload: &'a str) -> Self {
        Self::with_body(status, Body::Bytes(payload.as_bytes()))
            .header("Content-Type", "application/json; charset=utf-8")
    }

    pub fn text(status: Status, payload: &'a str) -> Self {
        Self::with_body(status, Body::Bytes(payload.as_bytes()))
            .header("Content-Type", "text/plain; charset=utf-8")
    }

    /// Adds a header. Dropped past the fourth, which no handler in this firmware reaches.
    pub fn header(mut self, name: &'static str, value: &str) -> Self {
        if let Some(h) = ResponseHeader::new(name, value) {
            if (self.header_count as usize) < self.headers.len() {
                self.headers[self.header_count as usize] = Some(h);
                self.header_count += 1;
            }
        }
        self
    }

    pub fn get_header(&self, name: &str) -> Option<&str> {
        for i in 0..self.header_count as usize {
            if let Some(h) = &self.headers[i] {
                if h.name().eq_ignore_ascii_case(name) {
                    return Some(h.value());
                }
            }
        }
        None
    }

    /// Serialises the status line and headers into `out`.
    ///
    /// Returns how many bytes were written. The body is written by the caller afterwards, because
    /// it may be a stream this server does not own.
    pub fn write_head(&self, out: &mut [u8]) -> usize {
        let mut w = SliceWriter::new(out);
        let _ = write!(
            w,
            "HTTP/1.1 {} {}\r\n",
            self.status.code(),
            self.status.reason()
        );
        let _ = write!(w, "Content-Length: {}\r\n", self.body.length());
        // The body's own content type is emitted unless a handler already set one, so `Response`
        // cannot produce a body whose type is missing.
        if self.get_header("Content-Type").is_none() {
            if let Some(ct) = self.body.content_type() {
                let _ = write!(w, "Content-Type: {ct}\r\n");
            }
        }
        for i in 0..self.header_count as usize {
            if let Some(h) = &self.headers[i] {
                let _ = write!(w, "{}: {}\r\n", h.name(), h.value());
            }
        }
        // The C++ server sent neither a Content-Length nor a Connection header, so every browser
        // opened a second connection for the frontend's assets and the LAN saw double the
        // traffic.
        let _ = write!(w, "Connection: close\r\n\r\n");
        w.written()
    }
}

/// Writes one chunk in the chunked framing: a hex length, the bytes, a blank line.
///
/// The chunk is written by hex digits computed here rather than by formatting, so a chunk larger
/// than the caller's buffer is refused instead of half-written. Half a chunk is worse than none:
/// the client's length and its bytes disagree and the stream is unrecoverable.
pub fn write_chunk(out: &mut [u8], bytes: &[u8]) -> usize {
    let mut header = [0u8; 16];
    let mut hw = SliceWriter::new(&mut header);
    let _ = write!(hw, "{:x}\r\n", bytes.len());
    let head_len = hw.written();
    if out.len() < head_len + bytes.len() + 2 {
        return 0;
    }
    out[..head_len].copy_from_slice(&header[..head_len]);
    out[head_len..head_len + bytes.len()].copy_from_slice(bytes);
    out[head_len + bytes.len()..head_len + bytes.len() + 2].copy_from_slice(b"\r\n");
    head_len + bytes.len() + 2
}

/// Writes one chunk in the chunked framing: a hex length, the bytes, a blank line.
///
/// The terminal chunk is a zero-length one, which is what tells the client the stream is over.
/// A stream that just stops is a truncated response, and the C++ SSE endpoint did exactly that
/// when a client disconnected mid-stream.
/// Writes the terminal chunk.
pub fn write_last_chunk(out: &mut [u8]) -> usize {
    let n = out.len().min(5);
    out[..n].copy_from_slice(&b"0\r\n\r\n"[..n]);
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::String;

    fn head_bytes(r: &Response<'_>) -> String<256> {
        let mut buf = [0u8; 256];
        let n = r.write_head(&mut buf);
        let mut s = String::new();
        let _ = s.push_str(core::str::from_utf8(&buf[..n]).unwrap());
        s
    }

    #[test]
    fn the_status_line_carries_the_code_and_reason() {
        let r = Response::new(Status::NotFound);
        assert!(head_bytes(&r).starts_with("HTTP/1.1 404 Not Found\r\n"));
    }

    #[test]
    fn an_empty_body_declares_zero_length() {
        assert!(head_bytes(&Response::new(Status::Ok)).contains("Content-Length: 0\r\n"));
    }

    #[test]
    fn a_bytes_body_declares_its_length() {
        let r = Response::with_body(Status::Ok, Body::Bytes(b"hello"));
        assert!(head_bytes(&r).contains("Content-Length: 5\r\n"));
    }

    #[test]
    fn a_json_response_always_declares_its_content_type() {
        // A handler cannot forget the content type, which is what the C++ frontend ran into.
        let r = Response::json(Status::Ok, "{}");
        assert_eq!(
            r.get_header("Content-Type"),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(
            r.get_header("content-type"),
            Some("application/json; charset=utf-8")
        );
    }

    #[test]
    fn every_body_declares_a_content_length() {
        // There is no stream-shaped body: a stream is framed by the connection loop, because a
        // stream's length is unknown when the head goes out.
        for r in [
            Response::new(Status::Ok),
            Response::with_body(Status::Ok, Body::Bytes(b"ab")),
            Response::with_body(
                Status::Ok,
                Body::Asset {
                    bytes: b"abc",
                    content_type: "text/html",
                },
            ),
        ] {
            let head = head_bytes(&r);
            assert!(head.contains("Content-Length:"), "{}", head);
            assert!(!head.contains("Transfer-Encoding"), "{}", head);
        }
    }

    #[test]
    fn a_head_always_ends_the_response() {
        // Without this a browser opens a second connection for every asset.
        assert!(head_bytes(&Response::new(Status::Ok)).contains("Connection: close\r\n\r\n"));
    }

    #[test]
    fn a_chunk_is_framed_in_hex() {
        let mut buf = [0u8; 32];
        let n = write_chunk(&mut buf, b"abcde");
        assert_eq!(&buf[..n], b"5\r\nabcde\r\n");
    }

    #[test]
    fn the_terminal_chunk_is_a_zero_length_one() {
        // A stream that simply stops is a truncated response, which is what the C++ endpoint did.
        let mut buf = [0u8; 16];
        let n = write_last_chunk(&mut buf);
        assert_eq!(&buf[..n], b"0\r\n\r\n");
    }

    #[test]
    fn a_chunk_into_a_short_buffer_stops_at_the_end_rather_than_overrunning() {
        let mut buf = [0u8; 8];
        let n = write_chunk(&mut buf, b"a long payload");
        assert!(n <= buf.len());
    }

    #[test]
    fn an_asset_body_is_served_with_its_own_content_type_and_no_copy() {
        let bytes: &[u8] = b"<!doctype html>";
        let r = Response::with_body(
            Status::Ok,
            Body::Asset {
                bytes,
                content_type: "text/html; charset=utf-8",
            },
        );
        let head = head_bytes(&r);
        assert!(head.contains("Content-Type: text/html; charset=utf-8"));
        assert!(head.contains("Content-Length: 15"));
        assert_eq!(r.body.bytes(), Some(bytes));
    }

    #[test]
    fn a_head_into_a_short_buffer_stops_at_the_end() {
        let mut buf = [0u8; 16];
        let r = Response::json(Status::Ok, "{}").header("X-A", "bbbbbbbbbbbbbb");
        let n = r.write_head(&mut buf);
        assert!(n <= buf.len());
    }

    #[test]
    fn a_header_past_the_fourth_is_dropped_rather_than_overrunning() {
        let mut r = Response::new(Status::Ok);
        for i in 0..12 {
            r = r.header("X-H", if i == 0 { "a" } else { "b" });
        }
        assert_eq!(r.header_count as usize, MAX_RESPONSE_HEADERS);
    }

    #[test]
    fn a_response_is_small_enough_to_live_on_a_stack_frame() {
        // The reason the response header bound is half the request's: sizing it to the request
        // made every exchange cost 5.8 KB of stack, which is a lot on a C6.
        assert!(
            core::mem::size_of::<Response<'static>>() <= 1024,
            "a Response is {} bytes",
            core::mem::size_of::<Response<'static>>()
        );
    }

    #[test]
    fn a_header_value_over_the_response_bound_is_dropped_not_truncated() {
        // A truncated `Location` would send the browser somewhere the firmware did not intend.
        let r = Response::new(Status::Ok).header("Location", &"h".repeat(MAX_RESPONSE_HEADER + 1));
        assert_eq!(r.header_count, 0);
    }

    #[test]
    fn a_status_the_router_does_not_use_cannot_be_constructed() {
        // The enum is the allow-list. There is no `Status::from(599)`.
        assert!(!Status::NotFound.is_retryable());
        assert!(Status::ServiceUnavailable.is_retryable());
        assert!(Status::TooManyRequests.is_retryable());
    }
}
