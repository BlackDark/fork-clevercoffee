//! Minimal streaming JSON: a writer onto any `core::fmt::Write` and a pull reader
//! over a byte slice. No allocation, so large responses stream on the device.

use core::fmt::{self, Write};

/// Writes JSON incrementally. Commas are inserted automatically.
#[derive(Debug)]
pub struct Writer<W> {
    out: W,
    /// One bit per nesting level: whether the next item needs a leading comma.
    need_comma: u64,
    depth: u8,
    pretty: bool,
    /// A key was just written: the next value belongs to it and takes no comma.
    after_key: bool,
}

impl<W: Write> Writer<W> {
    pub fn new(out: W) -> Self {
        Self { out, need_comma: 0, depth: 0, pretty: false, after_key: false }
    }

    pub fn pretty(out: W) -> Self {
        Self { pretty: true, ..Self::new(out) }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    fn sep(&mut self) -> fmt::Result {
        let bit = 1u64 << self.depth;
        if self.need_comma & bit != 0 {
            self.out.write_char(',')?;
        }
        self.need_comma |= bit;
        if self.pretty && self.depth > 0 {
            self.out.write_char('\n')?;
            for _ in 0..self.depth {
                self.out.write_str("  ")?;
            }
        }
        Ok(())
    }

    fn open(&mut self, c: char) -> fmt::Result {
        self.out.write_char(c)?;
        self.depth += 1;
        self.need_comma &= !(1u64 << self.depth);
        Ok(())
    }

    fn close(&mut self, c: char) -> fmt::Result {
        let had_items = self.need_comma & (1u64 << self.depth) != 0;
        self.depth -= 1;
        if self.pretty && had_items {
            self.out.write_char('\n')?;
            for _ in 0..self.depth {
                self.out.write_str("  ")?;
            }
        }
        self.out.write_char(c)
    }

    pub fn begin_object(&mut self) -> fmt::Result {
        self.value_sep()?;
        self.open('{')
    }

    pub fn end_object(&mut self) -> fmt::Result {
        self.close('}')
    }

    pub fn begin_array(&mut self) -> fmt::Result {
        self.value_sep()?;
        self.open('[')
    }

    pub fn end_array(&mut self) -> fmt::Result {
        self.close(']')
    }

    /// Writes `"key":` inside an object; the next value call completes the pair.
    pub fn key(&mut self, k: &str) -> fmt::Result {
        self.sep()?;
        write_str_escaped(&mut self.out, k)?;
        self.out.write_char(':')?;
        if self.pretty {
            self.out.write_char(' ')?;
        }
        self.after_key = true;
        Ok(())
    }

    fn value_sep(&mut self) -> fmt::Result {
        if self.after_key {
            self.after_key = false;
            self.need_comma |= 1u64 << self.depth;
            Ok(())
        } else {
            self.sep()
        }
    }

    pub fn str(&mut self, v: &str) -> fmt::Result {
        self.value_sep()?;
        write_str_escaped(&mut self.out, v)
    }

    pub fn bool(&mut self, v: bool) -> fmt::Result {
        self.value_sep()?;
        self.out.write_str(if v { "true" } else { "false" })
    }

    pub fn int(&mut self, v: i64) -> fmt::Result {
        self.value_sep()?;
        write!(self.out, "{v}")
    }

    /// Shortest representation; non-finite values become `null`.
    pub fn num(&mut self, v: f64) -> fmt::Result {
        self.value_sep()?;
        if v.is_finite() { write!(self.out, "{v}") } else { self.out.write_str("null") }
    }

    /// Rounded to two decimals, like the C++ `round2`.
    pub fn num2(&mut self, v: f64) -> fmt::Result {
        self.num(libm::round(v * 100.0) / 100.0)
    }

    pub fn null(&mut self) -> fmt::Result {
        self.value_sep()?;
        self.out.write_str("null")
    }

    pub fn kv_str(&mut self, k: &str, v: &str) -> fmt::Result {
        self.key(k)?;
        self.str(v)
    }

    pub fn kv_bool(&mut self, k: &str, v: bool) -> fmt::Result {
        self.key(k)?;
        self.bool(v)
    }

    pub fn kv_int(&mut self, k: &str, v: i64) -> fmt::Result {
        self.key(k)?;
        self.int(v)
    }

    pub fn kv_num(&mut self, k: &str, v: f64) -> fmt::Result {
        self.key(k)?;
        self.num(v)
    }
}

pub fn write_str_escaped(out: &mut impl Write, s: &str) -> fmt::Result {
    out.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}

/// A token from [`Reader`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
    BeginObject,
    EndObject,
    BeginArray,
    EndArray,
    /// An object key; escapes are not decoded (keys are plain ASCII here).
    Key(&'a str),
    /// A string value, raw (escapes still encoded; see [`unescape`]).
    Str(&'a str),
    /// A number, as its source text.
    Num(&'a str),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxError {
    pub at: usize,
}

/// Pull tokenizer for well-formed JSON documents up to 32 levels deep.
#[derive(Debug)]
pub struct Reader<'a> {
    src: &'a [u8],
    pos: usize,
    /// Per level: in an object (bit set) or array; and whether a key is expected.
    in_object: u32,
    expect_key: u32,
    depth: u8,
    started: bool,
}

impl<'a> Reader<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Self { src, pos: 0, in_object: 0, expect_key: 0, depth: 0, started: false }
    }

    fn ws(&mut self) {
        while self.pos < self.src.len() && matches!(self.src[self.pos], b' ' | b'\n' | b'\r' | b'\t') {
            self.pos += 1;
        }
    }

    fn err(&self) -> SyntaxError {
        SyntaxError { at: self.pos }
    }

    fn bit(&self) -> u32 {
        1 << self.depth
    }

    /// The next token, or `None` at the end of the document.
    pub fn next_token(&mut self) -> Result<Option<Token<'a>>, SyntaxError> {
        self.ws();
        if self.pos >= self.src.len() {
            return if self.depth == 0 && self.started { Ok(None) } else { Err(self.err()) };
        }
        let in_obj = self.depth > 0 && self.in_object & self.bit() != 0;
        let c = self.src[self.pos];
        if (c == b'}' && in_obj) || (c == b']' && self.depth > 0 && !in_obj) {
            self.pos += 1;
            self.depth -= 1;
            self.after_value();
            return Ok(Some(if c == b'}' { Token::EndObject } else { Token::EndArray }));
        }
        if c == b',' {
            if self.depth == 0 {
                return Err(self.err());
            }
            self.pos += 1;
            self.ws();
        }
        if self.depth == 0 && self.started {
            return Err(self.err());
        }
        self.started = true;
        if in_obj && self.expect_key & self.bit() != 0 {
            let key = self.string()?;
            self.ws();
            if self.src.get(self.pos) != Some(&b':') {
                return Err(self.err());
            }
            self.pos += 1;
            self.expect_key &= !self.bit();
            return Ok(Some(Token::Key(key)));
        }
        let c = self.src[self.pos];
        let tok = match c {
            b'{' | b'[' => {
                self.pos += 1;
                if self.depth >= 31 {
                    return Err(self.err());
                }
                self.depth += 1;
                if c == b'{' {
                    self.in_object |= self.bit();
                    self.expect_key |= self.bit();
                    return Ok(Some(Token::BeginObject));
                }
                self.in_object &= !self.bit();
                return Ok(Some(Token::BeginArray));
            }
            b'"' => Token::Str(self.string()?),
            b't' => self.literal("true", Token::Bool(true))?,
            b'f' => self.literal("false", Token::Bool(false))?,
            b'n' => self.literal("null", Token::Null)?,
            b'-' | b'0'..=b'9' => {
                let start = self.pos;
                while self.pos < self.src.len()
                    && matches!(self.src[self.pos], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    self.pos += 1;
                }
                Token::Num(core::str::from_utf8(&self.src[start..self.pos]).map_err(|_| self.err())?)
            }
            _ => return Err(self.err()),
        };
        self.after_value();
        Ok(Some(tok))
    }

    fn after_value(&mut self) {
        if self.depth > 0 && self.in_object & self.bit() != 0 {
            self.expect_key |= self.bit();
        }
    }

    fn literal(&mut self, word: &str, tok: Token<'a>) -> Result<Token<'a>, SyntaxError> {
        if self.src[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(tok)
        } else {
            Err(self.err())
        }
    }

    fn string(&mut self) -> Result<&'a str, SyntaxError> {
        if self.src.get(self.pos) != Some(&b'"') {
            return Err(self.err());
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.src.len() {
            match self.src[self.pos] {
                b'\\' => self.pos += 2,
                b'"' => {
                    let s = core::str::from_utf8(&self.src[start..self.pos]).map_err(|_| self.err())?;
                    self.pos += 1;
                    return Ok(s);
                }
                _ => self.pos += 1,
            }
        }
        Err(self.err())
    }

    /// Skips the value whose first token was just read.
    pub fn skip_value(&mut self, first: Token<'a>) -> Result<(), SyntaxError> {
        if !matches!(first, Token::BeginObject | Token::BeginArray) {
            return Ok(());
        }
        let target = self.depth - 1;
        while self.depth > target {
            if self.next_token()?.is_none() {
                return Err(self.err());
            }
        }
        Ok(())
    }
}

/// Decodes JSON string escapes into `out`. Returns `None` if `out` is too small or
/// an escape is malformed.
pub fn unescape<'b>(raw: &str, out: &'b mut [u8]) -> Option<&'b str> {
    let b = raw.as_bytes();
    let (mut i, mut n) = (0, 0);
    let mut push = |out: &mut [u8], n: &mut usize, bytes: &[u8]| -> Option<()> {
        out.get_mut(*n..*n + bytes.len())?.copy_from_slice(bytes);
        *n += bytes.len();
        Some(())
    };
    while i < b.len() {
        if b[i] == b'\\' {
            let e = *b.get(i + 1)?;
            let mut tmp = [0u8; 4];
            let s: &[u8] = match e {
                b'"' => b"\"",
                b'\\' => b"\\",
                b'/' => b"/",
                b'n' => b"\n",
                b'r' => b"\r",
                b't' => b"\t",
                b'b' => b"\x08",
                b'f' => b"\x0c",
                b'u' => {
                    let hex = core::str::from_utf8(b.get(i + 2..i + 6)?).ok()?;
                    let c = char::from_u32(u32::from_str_radix(hex, 16).ok()?)?;
                    i += 4;
                    c.encode_utf8(&mut tmp).as_bytes()
                }
                _ => return None,
            };
            push(out, &mut n, s)?;
            i += 2;
        } else {
            push(out, &mut n, &b[i..=i])?;
            i += 1;
        }
    }
    core::str::from_utf8(&out[..n]).ok()
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::String;

    #[test]
    fn writes_nested_documents() {
        let mut w = Writer::new(String::new());
        w.begin_object().unwrap();
        w.kv_num("t", 93.5).unwrap();
        w.key("a").unwrap();
        w.begin_array().unwrap();
        w.int(1).unwrap();
        w.str("x\"y").unwrap();
        w.end_array().unwrap();
        w.kv_bool("b", true).unwrap();
        w.key("o").unwrap();
        w.begin_object().unwrap();
        w.end_object().unwrap();
        w.kv_num("n", f64::NAN).unwrap();
        w.end_object().unwrap();
        assert_eq!(w.into_inner(), r#"{"t":93.5,"a":[1,"x\"y"],"b":true,"o":{},"n":null}"#);
    }

    #[test]
    fn integral_floats_print_without_fraction() {
        let mut w = Writer::new(String::new());
        w.num(95.0).unwrap();
        assert_eq!(w.into_inner(), "95");
    }

    #[test]
    fn reads_tokens() {
        let src = br#" {"a": [1, -2.5e3, "s\"x"], "b": {"c": true, "d": null}} "#;
        let mut r = Reader::new(src);
        let mut toks = std::vec::Vec::new();
        while let Some(t) = r.next_token().unwrap() {
            toks.push(t);
        }
        use Token::*;
        assert_eq!(
            toks,
            [
                BeginObject,
                Key("a"),
                BeginArray,
                Num("1"),
                Num("-2.5e3"),
                Str(r#"s\"x"#),
                EndArray,
                Key("b"),
                BeginObject,
                Key("c"),
                Bool(true),
                Key("d"),
                Null,
                EndObject,
                EndObject
            ]
        );
    }

    #[test]
    fn rejects_garbage() {
        for bad in [&b"{"[..], b"{\"a\" 1}", b"[1,]x", b"nul", b"{} {}", b""] {
            let mut r = Reader::new(bad);
            let mut failed = false;
            for _ in 0..20 {
                match r.next_token() {
                    Err(_) => {
                        failed = true;
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                }
            }
            assert!(failed, "{:?}", core::str::from_utf8(bad));
        }
    }

    #[test]
    fn unescapes() {
        let mut buf = [0u8; 32];
        assert_eq!(unescape(r#"a\"b\né"#, &mut buf), Some("a\"b\né"));
    }
}
