//! The parsed shape of a request.
//!
//! A request head is a fixed-size structure: no allocation, no lifetime into the parse buffer.
//! That is what lets a caller keep a `RequestHead` on its stack while it streams a body, which
//! matters because the OTA route holds one for the whole upload.

use heapless::String;

use crate::{MAX_HEADERS, MAX_HEADER_LINE, MAX_REQUEST_LINE};

/// The methods this frontend and this firmware use.
///
/// Deliberately a closed set rather than a free-form string: a method this server does not
/// implement is answered with 405 rather than being passed through to a handler that would have
/// to guess. The C++ server accepted any token, so `PROPFIND` and a typo'd method both reached
/// handler code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    /// `PROPFIND` and the rest. Reached only through [`Method::parse`], and answered with 405,
    /// but named so the log says what arrived.
    Other,
}

impl Method {
    pub fn parse(token: &[u8]) -> Self {
        match token {
            b"GET" => Method::Get,
            b"HEAD" => Method::Head,
            b"POST" => Method::Post,
            b"PUT" => Method::Put,
            b"PATCH" => Method::Patch,
            b"DELETE" => Method::Delete,
            b"OPTIONS" => Method::Options,
            _ => Method::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Other => "OTHER",
        }
    }

    /// Whether a body is expected. A `GET` with a body is legal HTTP and simply ignored.
    pub const fn expects_body(self) -> bool {
        matches!(
            self,
            Method::Post | Method::Put | Method::Patch | Method::Delete
        )
    }
}

/// One header.
///
/// A name and a value in fixed-size buffers rather than `heapless::String`, because a request
/// head lives on the stack for the whole exchange and 32 headers of two 256-byte arrays is 16 KB
/// of stack. That is too much, so the buffers are sized to what a browser actually sends and the
/// limits are named.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Header {
    /// NUL-padded. `name_len` is the length.
    name: [u8; 64],
    name_len: u8,
    value: [u8; MAX_HEADER_LINE],
    value_len: u8,
}

impl Header {
    /// Builds a header, returning `None` when either part is over its bound.
    ///
    /// A header over the bound is refused rather than truncated: a truncated
    /// `Content-Length` would be a body-length disagreement, and a truncated `Authorization`
    /// would authenticate against the wrong value.
    pub fn new(name: &[u8], value: &[u8]) -> Option<Self> {
        if name.is_empty() || name.len() > 64 || value.len() > MAX_HEADER_LINE {
            return None;
        }
        let mut h = Self {
            name: [0; 64],
            name_len: name.len() as u8,
            value: [0; MAX_HEADER_LINE],
            value_len: value.len() as u8,
        };
        h.name[..name.len()].copy_from_slice(name);
        h.value[..value.len()].copy_from_slice(value);
        Some(h)
    }

    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("")
    }

    pub fn value(&self) -> &str {
        core::str::from_utf8(&self.value[..self.value_len as usize]).unwrap_or("")
    }

    /// A case-insensitive lookup, because HTTP header names are case-insensitive and a client
    /// that sends `content-length` instead of `Content-Length` must not be understood to have
    /// sent no body at all.
    pub fn get(&self, name: &str) -> Option<&str> {
        if self.name().eq_ignore_ascii_case(name) {
            Some(self.value())
        } else {
            None
        }
    }
}

/// A fully parsed request head.
///
/// `PartialEq` so a test can assert on a whole head rather than on the field it cares about, and
/// so a caller comparing two heads for the same request does not have to compare seven fields.
#[derive(Debug, PartialEq)]
pub struct RequestHead {
    pub method: Method,
    /// The path with the query string removed.
    pub path: String<MAX_REQUEST_LINE>,
    /// The query string, without the `?`.
    pub query: String<MAX_REQUEST_LINE>,
    /// The raw target, for the access log.
    pub target: String<MAX_REQUEST_LINE>,
    pub headers: [Option<Header>; MAX_HEADERS],
    pub header_count: u8,
    /// The declared body length, or `None` for no body.
    pub content_length: Option<usize>,
    pub keep_alive: bool,
}

impl RequestHead {
    pub const fn new(method: Method) -> Self {
        Self {
            method,
            path: String::new(),
            query: String::new(),
            target: String::new(),
            headers: [None; MAX_HEADERS],
            header_count: 0,
            content_length: None,
            keep_alive: true,
        }
    }

    /// The header at `index`, in the order they arrived.
    pub fn header(&self, index: usize) -> Option<Header> {
        self.headers.get(index).copied().flatten()
    }

    /// A case-insensitive header lookup across every header.
    ///
    /// Borrows the head rather than copying the header out, so the returned slice points into the
    /// head's own storage and cannot outlive it.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .flatten()
            .find(|h| h.name().eq_ignore_ascii_case(name))
            .map(Header::value)
    }

    /// A query parameter by name.
    ///
    /// Matching is on the whole `name=value` pair split at the first `=`, so a parameter without a
    /// value (`?mode`) and one with an empty value (`?mode=`) both read as `""`. Percent decoding
    /// is deliberately absent: no query this firmware accepts carries a percent escape, and
    /// decoding one would need a table for no gain.
    pub fn query_param(&self, name: &str) -> Option<&str> {
        for pair in self.query.split('&') {
            match pair.split_once('=') {
                Some((k, v)) if k == name => return Some(v),
                None if pair == name => return Some(""),
                _ => {}
            }
        }
        None
    }

    /// Whether the client accepts a given content coding.
    ///
    /// `gzip` is the only one this server can produce. A `q=0` is honoured, because a client that
    /// explicitly refuses gzip and receives it anyway has been told something untrue.
    pub fn accepts(&self, coding: &str) -> bool {
        let Some(accept) = self.get("Accept-Encoding") else {
            return false;
        };
        for part in accept.split(',') {
            let mut fields = part.split(';');
            let name = fields.next().unwrap_or("").trim();
            if !name.eq_ignore_ascii_case(coding) && name != "*" {
                continue;
            }
            let refused = fields.any(|f| {
                let f = f.trim();
                f.strip_prefix("q=")
                    .map(|q| q.parse::<f32>().is_ok_and(|q| q == 0.0))
                    .unwrap_or(false)
            });
            if !refused {
                return true;
            }
        }
        false
    }
}
