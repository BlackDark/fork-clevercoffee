//! Streaming `multipart/form-data` parser for firmware uploads: feed it the body in
//! chunks of any size, get each part's headers and data back without buffering it.

use heapless::{String, Vec};

const MAX_BOUNDARY: usize = 70;
const MAX_HEADERS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadBoundary,
    HeadersTooLong,
    Malformed,
}

/// What the parser found.
#[derive(Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// A part begins. `name` is the form field, `filename` set for file inputs.
    Part {
        name: &'a str,
        filename: Option<&'a str>,
    },
    Data(&'a [u8]),
    PartEnd,
    /// The closing boundary was seen; anything after it is ignored.
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Before the first boundary: data is discarded.
    Preamble,
    /// After a boundary: the next two bytes say `--` (end) or CRLF (headers).
    AfterBoundary {
        first: Option<u8>,
    },
    Headers,
    Body,
    Done,
}

#[derive(Debug)]
pub struct Parser {
    /// `\r\n--boundary`
    delim: Vec<u8, { MAX_BOUNDARY + 4 }>,
    matched: usize,
    phase: Phase,
    headers: Vec<u8, MAX_HEADERS>,
}

/// Extracts the boundary from a `Content-Type` header value.
pub fn boundary_from_content_type(ct: &str) -> Option<&str> {
    let (mime, params) = ct.split_once(';')?;
    if !mime.trim().eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    params.split(';').find_map(|p| {
        let (k, v) = p.trim().split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| v.trim().trim_matches('"'))
    })
}

impl Parser {
    pub fn new(boundary: &str) -> Result<Self, Error> {
        if boundary.is_empty() || boundary.len() > MAX_BOUNDARY || boundary.contains(['\r', '\n']) {
            return Err(Error::BadBoundary);
        }
        let mut delim = Vec::new();
        let _ = delim.extend_from_slice(b"\r\n--");
        let _ = delim.extend_from_slice(boundary.as_bytes());
        // Pretend a CRLF preceded the body, so a boundary on the first line matches.
        Ok(Self {
            delim,
            matched: 2,
            phase: Phase::Preamble,
            headers: Vec::new(),
        })
    }

    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Feeds a chunk; `sink` receives the events in order.
    pub fn push(&mut self, mut input: &[u8], sink: &mut impl FnMut(Event<'_>)) -> Result<(), Error> {
        while !input.is_empty() {
            match self.phase {
                Phase::Done => return Ok(()),
                Phase::Preamble | Phase::Body => {
                    // Bytes matching a delimiter prefix are held back; they equal
                    // `delim[..matched]`, so on a mismatch they are re-emitted from there.
                    let emit = self.phase == Phase::Body;
                    let mut run_start = 0;
                    let mut found = None;
                    for (i, &b) in input.iter().enumerate() {
                        if b == self.delim[self.matched] {
                            if self.matched == 0 && emit && i > run_start {
                                sink(Event::Data(&input[run_start..i]));
                            }
                            self.matched += 1;
                            run_start = i + 1;
                            if self.matched == self.delim.len() {
                                found = Some(i + 1);
                                break;
                            }
                            continue;
                        }
                        if self.matched > 0 {
                            if emit {
                                sink(Event::Data(&self.delim[..self.matched]));
                            }
                            self.matched = 0;
                            run_start = i;
                            if b == self.delim[0] {
                                self.matched = 1;
                                run_start = i + 1;
                            }
                        }
                    }
                    if let Some(end) = found {
                        if emit {
                            sink(Event::PartEnd);
                        }
                        self.matched = 0;
                        self.phase = Phase::AfterBoundary { first: None };
                        input = &input[end..];
                    } else {
                        if emit && run_start < input.len() {
                            sink(Event::Data(&input[run_start..]));
                        }
                        input = &[];
                    }
                }
                Phase::AfterBoundary { first: None } => {
                    self.phase = Phase::AfterBoundary {
                        first: Some(input[0]),
                    };
                    input = &input[1..];
                }
                Phase::AfterBoundary { first: Some(a) } => {
                    let b = input[0];
                    input = &input[1..];
                    match (a, b) {
                        (b'-', b'-') => {
                            self.phase = Phase::Done;
                            sink(Event::End);
                        }
                        (b'\r', b'\n') => {
                            self.headers.clear();
                            self.phase = Phase::Headers;
                        }
                        _ => return Err(Error::Malformed),
                    }
                }
                Phase::Headers => {
                    let b = input[0];
                    input = &input[1..];
                    self.headers.push(b).map_err(|_| Error::HeadersTooLong)?;
                    if self.headers.ends_with(b"\r\n\r\n") {
                        let text = core::str::from_utf8(&self.headers).map_err(|_| Error::Malformed)?;
                        let (name, filename) = parse_disposition(text).ok_or(Error::Malformed)?;
                        sink(Event::Part {
                            name: &name,
                            filename: filename.as_deref(),
                        });
                        self.phase = Phase::Body;
                        self.matched = 0;
                    }
                }
            }
        }
        Ok(())
    }
}

fn parse_disposition(headers: &str) -> Option<(String<64>, Option<String<128>>)> {
    let line = headers
        .split("\r\n")
        .find(|l| l.len() > 20 && l[..20].eq_ignore_ascii_case("content-disposition:"))?;
    let mut name = None;
    let mut filename = None;
    for p in line[20..].split(';') {
        if let Some((k, v)) = p.trim().split_once('=') {
            let v = v.trim().trim_matches('"');
            match k.trim() {
                "name" => name = String::try_from(v).ok(),
                "filename" => filename = String::try_from(v).ok(),
                _ => {}
            }
        }
    }
    Some((name?, filename))
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec as StdVec;

    #[derive(Debug, PartialEq)]
    enum Owned {
        Part(std::string::String, Option<std::string::String>),
        Data(StdVec<u8>),
        PartEnd,
        End,
    }

    fn parse(body: &[u8], chunk: usize) -> StdVec<Owned> {
        let mut p = Parser::new("XyZ").unwrap();
        let mut out: StdVec<Owned> = StdVec::new();
        for c in body.chunks(chunk) {
            p.push(c, &mut |e| match e {
                Event::Part { name, filename } => {
                    out.push(Owned::Part(name.into(), filename.map(Into::into)));
                }
                Event::Data(d) => match out.last_mut() {
                    Some(Owned::Data(v)) => v.extend_from_slice(d),
                    _ => out.push(Owned::Data(d.to_vec())),
                },
                Event::PartEnd => out.push(Owned::PartEnd),
                Event::End => out.push(Owned::End),
            })
            .unwrap();
        }
        out
    }

    fn body(payload: &[u8]) -> StdVec<u8> {
        let mut b = StdVec::new();
        b.extend_from_slice(b"preamble\r\n--XyZ\r\nContent-Disposition: form-data; name=\"firmware\"; filename=\"fw.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n");
        b.extend_from_slice(payload);
        b.extend_from_slice(
            b"\r\n--XyZ\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhi\r\n--XyZ--\r\n",
        );
        b
    }

    #[test]
    fn parses_in_any_chunking() {
        // Payload with near-misses of the delimiter.
        let payload: StdVec<u8> = b"\x00\r\n-\r\n--Xy\r\r\n--XyQ\xff end\r"
            .iter()
            .copied()
            .chain((0..=255u8).cycle().take(3000))
            .collect();
        let b = body(&payload);
        let expected = std::vec![
            Owned::Part("firmware".into(), Some("fw.bin".into())),
            Owned::Data(payload.clone()),
            Owned::PartEnd,
            Owned::Part("note".into(), None),
            Owned::Data(b"hi".to_vec()),
            Owned::PartEnd,
            Owned::End,
        ];
        for chunk in [1, 2, 3, 5, 7, 16, 64, 1000, 100_000] {
            assert_eq!(parse(&b, chunk), expected, "chunk {chunk}");
        }
    }

    #[test]
    fn boundary_from_header() {
        assert_eq!(
            boundary_from_content_type("multipart/form-data; boundary=----WebKitFormBoundaryAbc"),
            Some("----WebKitFormBoundaryAbc")
        );
        assert_eq!(
            boundary_from_content_type("multipart/form-data; boundary=\"q\""),
            Some("q")
        );
        assert_eq!(boundary_from_content_type("application/json"), None);
    }

    #[test]
    fn rejects_bad_boundaries() {
        assert_eq!(Parser::new("").unwrap_err(), Error::BadBoundary);
        assert_eq!(Parser::new("a\r\nb").unwrap_err(), Error::BadBoundary);
    }
}
