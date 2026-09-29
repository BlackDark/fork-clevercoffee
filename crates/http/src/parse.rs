//! The request parser.
//!
//! A hand-written state machine over a byte slice, because the grammar is small and every bound
//! it enforces is a place the C++ server could be made to allocate without limit. Each failure
//! mode below is a test, because "the parser rejects it" is the only thing keeping a hostile or
//! broken client from taking the machine down while it is making coffee.
//!
//! Deliberately not supported, and answered with a specific error rather than misread:
//!
//! - Chunked request bodies. The C++ firmware used `ESP8266WebServer`, whose multipart upload
//!   handler buffered the whole part in a `String`, which is how a firmware upload could exhaust
//!   the heap. The OTA routes stream instead.
//! - HTTP/2 and upgrade requests. The frontend does not use them.
//! - Obsolete line folding. A folded header is rejected, not joined, because joining is how a
//!   header-count limit gets bypassed.

use crate::request::{Header, Method, RequestHead};
use crate::{MAX_BODY, MAX_HEADERS, MAX_HEADER_LINE, MAX_REQUEST_LINE};

/// Why a request was refused.
///
/// Every variant is a distinct answer with a distinct status, because "400 Bad Request" for an
/// oversized header and for a chunked body tells the user nothing about what to change. The
/// mapping to a status lives in [`ParseError::status`], so the parse and the response cannot
/// disagree about it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// The request line is not `METHOD TARGET VERSION`.
    MalformedRequestLine,
    /// The target is not an origin-form path.
    BadTarget,
    /// The version is neither HTTP/1.0 nor HTTP/1.1.
    UnsupportedVersion,
    /// A header line has no colon, or the name is empty.
    MalformedHeader,
    /// A header begins with a space or tab: obsolete line folding.
    FoldedHeader,
    /// A request line or header line is over its bound.
    RequestLineTooLong,
    HeaderTooLong,
    /// More headers than [`MAX_HEADERS`].
    TooManyHeaders,
    /// `Content-Length` is not a number, or is negative.
    BadContentLength,
    /// `Content-Length` is over [`MAX_BODY`].
    BodyTooLarge,
    /// `Transfer-Encoding` was offered. Not supported, by design.
    ChunkedRequestUnsupported,
    /// The client stopped sending mid-head.
    UnexpectedEnd,
}

impl ParseError {
    /// The status this error is answered with.
    pub const fn status(self) -> crate::response::Status {
        use crate::response::Status;
        match self {
            // A target or header over its bound is the URI being too long, not a malformed one:
            // the distinction tells the user whether to shorten a URL or fix a syntax error.
            ParseError::RequestLineTooLong | ParseError::HeaderTooLong => Status::UriTooLong,
            ParseError::BodyTooLarge => Status::PayloadTooLarge,
            // A chunked body is a request this server will not accept. 501 says the server does
            // not support it, which is true, rather than 400, which would blame the client's
            // syntax.
            ParseError::ChunkedRequestUnsupported => Status::NotImplemented,
            ParseError::TooManyHeaders => Status::BadRequest,
            ParseError::MalformedRequestLine
            | ParseError::BadTarget
            | ParseError::UnsupportedVersion
            | ParseError::MalformedHeader
            | ParseError::FoldedHeader
            | ParseError::BadContentLength
            | ParseError::UnexpectedEnd => Status::BadRequest,
        }
    }

    /// Whether retrying the same request could succeed. A request that is too long will be too
    /// long next time, and a retrying client is the mechanism behind a boot-time request flood.
    pub const fn is_retryable(self) -> bool {
        false
    }
}

/// Parses a request head from the start of `input`.
///
/// Returns the head and the number of bytes consumed, which is the offset at which the body
/// begins. The caller keeps whatever follows in its own buffer, because the OTA routes need to
/// read a body that is larger than anything held in one place.
pub fn parse(input: &[u8]) -> Result<(RequestHead, usize), ParseError> {
    // `cursor` is the index of the next byte to read. `find_line_end` returns the index of the
    // `\n` that ends a line, so the next line starts one byte after it.
    let request_end = find_line_end(input, 0).ok_or(ParseError::RequestLineTooLong)?;
    if request_end > MAX_REQUEST_LINE {
        return Err(ParseError::RequestLineTooLong);
    }
    let request_line = trim_cr(&input[..request_end]);
    let mut head = parse_request_line(request_line)?;
    let mut cursor = request_end + 1;

    loop {
        // Every exit from this loop returns, so the code after it is only reached when the input
        // ended without a blank line.
        // A blank line ends the head. It is either CRLF or a bare LF, and the cursor points at
        // whichever came first, so both have to be recognised: a parser that only recognises LF
        // treats every header line of a well-formed CRLF request as unterminated. The returned
        // offset is past the blank line, which is where the body begins.
        match input.get(cursor) {
            Some(b'\r') if input.get(cursor + 1) == Some(&b'\n') => return Ok((head, cursor + 2)),
            Some(b'\n') => return Ok((head, cursor + 1)),
            _ => {}
        }
        let Some(next) = find_line_end(input, cursor) else {
            // The client sent a head with no terminating blank line. Bounded, so this falls
            // through to the error below rather than waiting on a client that never finishes.
            break;
        };
        if next - cursor > MAX_HEADER_LINE {
            return Err(ParseError::HeaderTooLong);
        }
        if head.header_count as usize >= MAX_HEADERS {
            return Err(ParseError::TooManyHeaders);
        }
        let raw = trim_cr(&input[cursor..next]);
        cursor = next + 1;

        // The fold check comes before the split: a folded continuation line has no colon of its
        // own, so checking after would report a malformed header and lose the reason.
        if raw.first().is_some_and(|b| *b == b' ' || *b == b'\t') {
            return Err(ParseError::FoldedHeader);
        }
        let colon = raw
            .iter()
            .position(|b| *b == b':')
            .ok_or(ParseError::MalformedHeader)?;
        let (name, rest) = raw.split_at(colon);
        let value = &rest[1..];
        if name.is_empty() {
            return Err(ParseError::MalformedHeader);
        }
        let header = Header::new(name, trim_ascii(value)).ok_or(ParseError::HeaderTooLong)?;
        if header.get("Transfer-Encoding").is_some() {
            return Err(ParseError::ChunkedRequestUnsupported);
        }
        if let Some(len) = header.get("Content-Length") {
            let parsed: usize = len.parse().map_err(|_| ParseError::BadContentLength)?;
            if parsed > MAX_BODY {
                return Err(ParseError::BodyTooLarge);
            }
            head.content_length = Some(parsed);
        }
        if let Some(conn) = header.get("Connection") {
            // `close` wins over the version default; an explicit `keep-alive` wins over the
            // HTTP/1.0 default.
            head.keep_alive = !conn.eq_ignore_ascii_case("close");
        }
        head.headers[head.header_count as usize] = Some(header);
        head.header_count += 1;
    }

    // Reached only when the head ran out of input before its blank line.
    let _ = cursor;
    apply_version_defaults(&mut head, request_line);
    Err(if input.len() > MAX_HEADER_LINE {
        ParseError::HeaderTooLong
    } else {
        ParseError::UnexpectedEnd
    })
}

/// Splits `METHOD TARGET VERSION` and fills the head's path and query.
///
/// A target that is not a path (an absolute URI, or `*`) is rejected. The C++ server passed it
/// through and the handlers compared strings against paths, so a proxied request whose target was
/// a full URL matched nothing and returned 404 with no explanation.
fn parse_request_line(line: &[u8]) -> Result<RequestHead, ParseError> {
    let mut parts = line.splitn(3, |b| *b == b' ');
    let method = parts.next().ok_or(ParseError::MalformedRequestLine)?;
    let target = parts.next().ok_or(ParseError::MalformedRequestLine)?;
    let version = parts.next().ok_or(ParseError::MalformedRequestLine)?;

    if target.is_empty() || method.is_empty() || version.is_empty() {
        // A doubled space or a trailing one produces an empty field. Reported as a malformed
        // request line rather than a bad target, because that is where the fault is.
        return Err(ParseError::MalformedRequestLine);
    }
    let mut head = RequestHead::new(Method::parse(method));
    let target_str = core::str::from_utf8(target).map_err(|_| ParseError::BadTarget)?;
    if !target_str.starts_with('/') {
        // `*` is legal for `OPTIONS`, and absolute-form is legal for a proxy. Neither is something
        // this machine serves, and accepting them would mean every handler had to handle a target
        // it does not understand.
        if !(head.method == Method::Options && target_str == "*") {
            return Err(ParseError::BadTarget);
        }
        head.target
            .push_str(target_str)
            .map_err(|_| ParseError::BadTarget)?;
        return Ok(head);
    }
    head.target
        .push_str(target_str)
        .map_err(|_| ParseError::BadTarget)?;

    let (path, query) = match target_str.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target_str, ""),
    };
    head.path
        .push_str(path)
        .map_err(|_| ParseError::BadTarget)?;
    head.query
        .push_str(query)
        .map_err(|_| ParseError::BadTarget)?;

    // The version decides keep-alive, and a version this server does not speak is refused rather
    // than guessed at.
    if version.starts_with(b"HTTP/1.0") {
        head.keep_alive = false;
    } else if version.starts_with(b"HTTP/1.") {
        head.keep_alive = true;
    } else {
        return Err(ParseError::UnsupportedVersion);
    }
    Ok(head)
}

/// Applies the HTTP/1.0 keep-alive default unless a `Connection` header already decided it.
fn apply_version_defaults(head: &mut RequestHead, request_line: &[u8]) {
    let is_http10 = request_line.ends_with(b"HTTP/1.0");
    let connection_seen = (0..head.header_count as usize)
        .filter_map(|i| head.headers[i])
        .any(|h| h.name().eq_ignore_ascii_case("Connection"));
    if is_http10 && !connection_seen {
        head.keep_alive = false;
    }
}

/// Finds the end of the line starting at `from`: the `\n`, or the end of the input when the
/// client sent a head with no trailing blank line.
fn find_line_end(input: &[u8], from: usize) -> Option<usize> {
    if from > input.len() {
        return None;
    }
    input[from..]
        .iter()
        .position(|b| *b == b'\n')
        .map(|i| from + i)
}

fn trim_cr(line: &[u8]) -> &[u8] {
    match line.last() {
        Some(b'\r') => &line[..line.len() - 1],
        _ => line,
    }
}

fn trim_ascii(mut v: &[u8]) -> &[u8] {
    while let Some((first, rest)) = v.split_first() {
        if first.is_ascii_whitespace() {
            v = rest;
        } else {
            break;
        }
    }
    while let Some((last, rest)) = v.split_last() {
        if last.is_ascii_whitespace() {
            v = rest;
        } else {
            break;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::String;

    fn head(request: &[u8]) -> RequestHead {
        parse(request).expect("should parse").0
    }

    #[test]
    fn the_methods_the_frontend_uses_all_parse() {
        for (token, expected) in [
            (&b"GET /x HTTP/1.1\r\n\r\n"[..], Method::Get),
            (&b"HEAD /x HTTP/1.1\r\n\r\n"[..], Method::Head),
            (&b"POST /x HTTP/1.1\r\n\r\n"[..], Method::Post),
            (&b"PUT /x HTTP/1.1\r\n\r\n"[..], Method::Put),
            (&b"PATCH /x HTTP/1.1\r\n\r\n"[..], Method::Patch),
            (&b"DELETE /x HTTP/1.1\r\n\r\n"[..], Method::Delete),
            (&b"OPTIONS /x HTTP/1.1\r\n\r\n"[..], Method::Options),
        ] {
            assert_eq!(
                head(token).method,
                expected,
                "{}",
                core::str::from_utf8(token).unwrap()
            );
        }
    }

    #[test]
    fn an_unknown_method_is_named_rather_than_rejected() {
        // It reaches the router and gets 405, which says more than a parse error would.
        let h = head(b"PROPFIND /x HTTP/1.1\r\n\r\n");
        assert_eq!(h.method, Method::Other);
        assert_eq!(h.path.as_str(), "/x");
    }

    #[test]
    fn the_target_is_split_into_a_path_and_a_query() {
        let h = head(b"GET /api/parameters?filter=all&x=1 HTTP/1.1\r\n\r\n");
        assert_eq!(h.path.as_str(), "/api/parameters");
        assert_eq!(h.query.as_str(), "filter=all&x=1");
        assert_eq!(h.query_param("filter"), Some("all"));
        assert_eq!(h.query_param("x"), Some("1"));
        assert_eq!(h.query_param("missing"), None);
    }

    #[test]
    fn a_query_parameter_with_no_value_reads_as_empty() {
        let h = head(b"GET /x?mode HTTP/1.1\r\n\r\n");
        assert_eq!(h.query_param("mode"), Some(""));
    }

    #[test]
    fn a_query_parameter_name_is_matched_whole_not_as_a_prefix() {
        // `?mode=partial` must not satisfy a lookup for `mode`.
        let h = head(b"GET /x?mode_name=partial HTTP/1.1\r\n\r\n");
        assert_eq!(h.query_param("mode"), None);
        assert_eq!(h.query_param("mode_name"), Some("partial"));
    }

    #[test]
    fn headers_are_parsed_and_looked_up_case_insensitively() {
        let h = head(b"GET /x HTTP/1.1\r\ncontent-length: 0\r\nHOST: kitchen\r\n\r\n");
        assert_eq!(h.get("Content-Length"), Some("0"));
        assert_eq!(h.get("content-length"), Some("0"));
        assert_eq!(h.get("HOST"), Some("kitchen"));
        assert_eq!(h.header_count, 2);
    }

    #[test]
    fn whitespace_around_a_header_value_is_trimmed() {
        let h = head(b"GET /x HTTP/1.1\r\nX-A:   spaced   \r\n\r\n");
        assert_eq!(h.get("x-a"), Some("spaced"));
    }

    #[test]
    fn a_content_length_is_parsed_and_bounds_checked() {
        let h = head(b"POST /x HTTP/1.1\r\nContent-Length: 1234\r\n\r\n");
        assert_eq!(h.content_length, Some(1234));
    }

    #[test]
    fn a_content_length_that_does_not_fit_the_limit_is_refused() {
        // This is the OOM guard. The C++ upload path buffered a whole firmware image in a heap
        // String; a length above the limit is refused before a byte is read.
        let mut big = String::<128>::new();
        let _ = big.push_str("POST /ota HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n");
        assert_eq!(parse(big.as_bytes()), Err(ParseError::BodyTooLarge));
    }

    #[test]
    fn a_content_length_that_is_not_a_number_is_refused() {
        assert_eq!(
            parse(b"POST /x HTTP/1.1\r\nContent-Length: abc\r\n\r\n"),
            Err(ParseError::BadContentLength)
        );
        assert_eq!(
            parse(b"POST /x HTTP/1.1\r\nContent-Length: -1\r\n\r\n"),
            Err(ParseError::BadContentLength)
        );
    }

    #[test]
    fn a_request_line_over_the_limit_is_refused() {
        let mut long = String::<512>::new();
        let _ = long.push_str("GET /");
        for i in 0..400 {
            let _ = long.push(if i % 2 == 0 { 'a' } else { 'b' });
        }
        let _ = long.push_str(" HTTP/1.1\r\n\r\n");
        assert_eq!(parse(long.as_bytes()), Err(ParseError::RequestLineTooLong));
    }

    #[test]
    fn a_header_line_over_the_limit_is_refused() {
        let mut long = String::<512>::new();
        let _ = long.push_str("GET / HTTP/1.1\r\nX-Long: ");
        for _ in 0..400 {
            let _ = long.push('x');
        }
        let _ = long.push_str("\r\n\r\n");
        assert_eq!(parse(long.as_bytes()), Err(ParseError::HeaderTooLong));
    }

    #[test]
    fn too_many_headers_is_refused_before_any_of_them_is_kept() {
        // The limit is what stops a client from filling the stack with header lines. Each header
        // is 320 bytes, so an unbounded count is an unbounded stack.
        let mut req = String::<4096>::new();
        let _ = req.push_str("GET / HTTP/1.1\r\n");
        for i in 0..(MAX_HEADERS + 8) {
            let _ = core::fmt::Write::write_fmt(&mut req, format_args!("X-{i}: v\r\n"));
        }
        let _ = req.push_str("\r\n");
        assert_eq!(parse(req.as_bytes()), Err(ParseError::TooManyHeaders));
    }

    #[test]
    fn a_folded_header_is_refused_with_the_reason_named() {
        // Reported as folded, not as a malformed header, so the log says what was actually wrong.
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\nX-A: one\r\n\ttwo\r\n\r\n"),
            Err(ParseError::FoldedHeader)
        );
    }

    #[test]
    fn a_folded_header_is_refused_rather_than_joined() {
        // Joining is how a header-count limit gets bypassed: one logical header becomes two
        // lines, or a folded Authorization extends a credential the server did not bound.
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\nX-A: one\r\n  two\r\n\r\n"),
            Err(ParseError::FoldedHeader)
        );
    }

    #[test]
    fn a_chunked_request_body_is_refused_with_a_specific_error() {
        // The OTA routes stream with a content length. Accepting chunked would mean buffering a
        // body of unknown size, which is the defect the streaming routes exist to avoid.
        assert_eq!(
            parse(b"POST /ota HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Err(ParseError::ChunkedRequestUnsupported)
        );
    }

    #[test]
    fn a_malformed_request_line_is_refused() {
        assert_eq!(parse(b"\r\n\r\n"), Err(ParseError::MalformedRequestLine));
        assert_eq!(parse(b"GET\r\n\r\n"), Err(ParseError::MalformedRequestLine));
        assert_eq!(
            parse(b"GET /x\r\n\r\n"),
            Err(ParseError::MalformedRequestLine)
        );
        assert_eq!(
            parse(b"GET  /x HTTP/1.1\r\n\r\n"),
            Err(ParseError::MalformedRequestLine)
        );
    }

    #[test]
    fn a_target_that_is_not_a_path_is_refused() {
        // The C++ server passed these through and every handler then matched nothing.
        assert_eq!(
            parse(b"GET http://evil/x HTTP/1.1\r\n\r\n"),
            Err(ParseError::BadTarget)
        );
        assert_eq!(parse(b"GET * HTTP/1.1\r\n\r\n"), Err(ParseError::BadTarget));
    }

    #[test]
    fn options_star_is_accepted_because_it_is_legal() {
        let h = head(b"OPTIONS * HTTP/1.1\r\n\r\n");
        assert_eq!(h.method, Method::Options);
        assert_eq!(h.target.as_str(), "*");
    }

    #[test]
    fn an_unsupported_version_is_refused() {
        assert_eq!(
            parse(b"GET / HTTP/2.0\r\n\r\n"),
            Err(ParseError::UnsupportedVersion)
        );
        assert_eq!(
            parse(b"GET / SPDY/3\r\n\r\n"),
            Err(ParseError::UnsupportedVersion)
        );
    }

    #[test]
    fn a_header_with_no_colon_is_refused() {
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\nbroken\r\n\r\n"),
            Err(ParseError::MalformedHeader)
        );
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\n: empty\r\n\r\n"),
            Err(ParseError::MalformedHeader)
        );
    }

    #[test]
    fn keep_alive_follows_the_version_and_the_connection_header() {
        assert!(head(b"GET / HTTP/1.1\r\n\r\n").keep_alive);
        assert!(!head(b"GET / HTTP/1.0\r\n\r\n").keep_alive);
        assert!(!head(b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n").keep_alive);
        assert!(head(b"GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n").keep_alive);
    }

    #[test]
    fn a_head_with_no_trailing_blank_line_is_still_bounded() {
        // A client that connects and never sends the blank line must not make the parser wait
        // forever on a target that grows without limit. It returns an error instead.
        let mut open = String::<512>::new();
        let _ = open.push_str("GET / HTTP/1.1\r\n");
        for i in 0..400 {
            let _ = core::fmt::Write::write_fmt(&mut open, format_args!("X-{i}: "));
        }
        assert!(parse(open.as_bytes()).is_err());
    }

    #[test]
    fn the_body_starts_where_the_parser_says_it_does() {
        let raw = b"POST /x HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        let (h, offset) = parse(raw).unwrap();
        assert_eq!(h.content_length, Some(5));
        assert_eq!(
            &raw[offset..],
            b"hello",
            "the caller must be able to find the body"
        );
    }

    #[test]
    fn gzip_is_negotiated_and_a_refusal_is_honoured() {
        let yes = head(b"GET / HTTP/1.1\r\nAccept-Encoding: gzip, deflate\r\n\r\n");
        assert!(yes.accepts("gzip"));
        let no = head(b"GET / HTTP/1.1\r\n\r\n");
        assert!(!no.accepts("gzip"));
        let refused = head(b"GET / HTTP/1.1\r\nAccept-Encoding: gzip;q=0\r\n\r\n");
        assert!(!refused.accepts("gzip"), "a q=0 refusal must be honoured");
        let wild = head(b"GET / HTTP/1.1\r\nAccept-Encoding: *\r\n\r\n");
        assert!(wild.accepts("gzip"));
        let other = head(b"GET / HTTP/1.1\r\nAccept-Encoding: br\r\n\r\n");
        assert!(!other.accepts("gzip"));
    }

    #[test]
    fn a_head_with_no_headers_parses() {
        let h = head(b"GET / HTTP/1.1\r\n\r\n");
        assert_eq!(h.header_count, 0);
        assert_eq!(h.content_length, None);
    }
}
