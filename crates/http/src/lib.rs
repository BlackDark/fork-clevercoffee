//! A minimal HTTP/1.1 server, over an injected byte stream.
//!
//! We own this because `esp-idf-svc` would drag in the IDF HTTP server and a TLS stack we do not
//! use. The whole surface the frontend needs is thirty routes over plain HTTP on a LAN, and a
//! general-purpose server is several hundred kilobytes we would spend to parse a fixed grammar.
//!
//! Two properties matter more than feature coverage, and both are enforced here rather than
//! documented and hoped for:
//!
//! - **Every buffer has a compile-time bound.** A request header list, a header line, a URL, a
//!   body chunk. On a C6 with 320 KB of RAM, an unbounded allocation driven by a request from
//!   the network is a denial of service anyone on the LAN can cause, including the user.
//! - **The socket is injected.** Every test runs the real parser and the real router over an
//!   in-memory stream, so there is no "tested up to the socket" gap.
//!
//! What is deliberately absent: TLS, HTTP/2, chunked *requests*, and keep-alive pipelining
//! beyond one in-flight request per connection. The frontend does not use any of them.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod parse;
pub mod request;
pub mod response;
pub mod route;
pub mod server;
pub mod sse;
pub mod writer;

pub use parse::ParseError;
pub use request::RequestHead;
pub use request::{Header, Method};
pub use response::{write_chunk, write_last_chunk, Body, Response, Status};
pub use route::{Decision, Route, Router};
pub use server::{serve_connection, Guard, Handler, Reply, Stream, StreamError};
pub use sse::{Event, SseStream};

/// The longest request line accepted: method, target, version.
///
/// A real request from this frontend is under 64 bytes. 256 leaves room for a long URL from the
/// firmware-from-URL OTA route, which is the longest target the machine serves, and rejects
/// anything larger before a single byte of it is copied.
pub const MAX_REQUEST_LINE: usize = 256;

/// The longest single header line, name and value together.
pub const MAX_HEADER_LINE: usize = 256;

/// The most headers one request may carry. A browser sends about a dozen; 32 is generous.
pub const MAX_HEADERS: usize = 32;

/// The longest request body accepted in one message. The largest body the machine accepts is a
/// config JSON of about 16 KB and an OTA image streamed in chunks, so 64 KB covers it with
/// headroom and is still small enough that a stack buffer of it would be unreasonable, which is
/// why bodies arrive through a caller-provided buffer rather than being allocated here.
pub const MAX_BODY: usize = 64 * 1024;
