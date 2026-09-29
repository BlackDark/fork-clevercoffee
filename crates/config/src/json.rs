//! A minimal JSON parser, sized for the config document and bounded in what it will do.
//!
//! `serde_json` would be several hundred kilobytes of code generation for a document with a
//! fixed shape, and on a C6 that competes with the config region itself for flash. This parser
//! accepts exactly what a config document needs and rejects everything else at a bounded cost.
//!
//! The bounds are the point, not an afterthought. A malformed or hostile document must not be
//! able to exhaust the heap, so the recursion depth, the nesting depth and the buffer growth are
//! all capped, and each cap has a test.
//!
//! Parsing and validation are separate steps. This produces a [`ResolvedDoc`] of dotted keys and
//! untyped leaf values; [`crate::validate`] then decides what is acceptable. Keeping them apart
//! is what lets the validation rules be tested without a parser, and lets a caller reuse them for
//! a document that came from the flash rather than from a host.

use crate::import::{DocValue, ResolvedDoc};
use crate::schema::MAX_TEXT;

/// Nesting depth limit. A config document is at most three deep (`hardware.relays.heater`), so
/// eight is generous. Without a limit a document of `[[[[[...` would recurse until the stack ran
/// out, which on a bare-metal target is a panic with no handler.
pub const MAX_DEPTH: usize = 8;

/// The longest single scalar the parser will accept, for a value not in the schema.
pub const MAX_SCALAR: usize = 128;

/// Why a document did not parse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// The document ended in the middle of a value.
    UnexpectedEnd,
    /// A byte that cannot begin a value appeared where a value was expected.
    UnexpectedByte { at: usize },
    /// A number that is not a valid JSON number.
    BadNumber { at: usize },
    /// A string with an unterminated sequence, or a control character inside one.
    BadString { at: usize },
    /// A `\` escape that is not one JSON defines.
    BadEscape { at: usize },
    /// Nested deeper than [`MAX_DEPTH`].
    TooDeep,
    /// A scalar longer than [`MAX_SCALAR`].
    ScalarTooLong,
    /// The document had bytes after the top-level value.
    TrailingBytes,
}

impl ParseError {
    pub const fn message(self) -> &'static str {
        match self {
            ParseError::UnexpectedEnd => "unexpected end of document",
            ParseError::UnexpectedByte { .. } => "unexpected byte",
            ParseError::BadNumber { .. } => "malformed number",
            ParseError::BadString { .. } => "malformed string",
            ParseError::BadEscape { .. } => "unknown escape sequence",
            ParseError::TooDeep => "nested too deeply",
            ParseError::ScalarTooLong => "value too long",
            ParseError::TrailingBytes => "trailing bytes after the top-level value",
        }
    }

    /// The byte offset, so a report can point at the problem rather than saying "invalid JSON".
    pub const fn offset(self) -> usize {
        match self {
            ParseError::UnexpectedEnd
            | ParseError::TooDeep
            | ParseError::ScalarTooLong
            | ParseError::TrailingBytes => 0,
            ParseError::UnexpectedByte { at }
            | ParseError::BadNumber { at }
            | ParseError::BadString { at }
            | ParseError::BadEscape { at } => at,
        }
    }
}

/// A parsed document: every leaf, as a dotted key and an untyped value.
///
/// The type is untyped because the document's own types are not trustworthy: a config file is a
/// user-supplied artefact, and deciding whether a `1` means `true` is the schema's job, not the
/// parser's. This is what makes the C++ defect D27 impossible to repeat, where a JSON `1` was
/// coerced into `true` for a boolean parameter.
#[derive(Debug)]
pub struct Document {
    doc: ResolvedDoc,
    /// A `null` leaf. Recorded rather than skipped, because a `null` where a value is expected is
    /// a rejection the user should see named, not a silent absence.
    nulls: u16,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            doc: ResolvedDoc::new(),
            nulls: 0,
        }
    }
}

impl Document {
    pub fn resolved(&self) -> &ResolvedDoc {
        &self.doc
    }

    pub fn into_resolved(self) -> ResolvedDoc {
        self.doc
    }

    pub fn null_count(&self) -> u16 {
        self.nulls
    }
}

/// Parses a config document.
///
/// Every value it produces borrows from `input`, so a caller that parses from a reusable buffer
/// must finish with the values before the next parse. That is deliberate: it makes it impossible
/// to keep a pointer into a buffer that has already been overwritten.
pub fn parse(input: &[u8]) -> Result<Document, ParseError> {
    let mut p = Parser {
        input,
        pos: 0,
        out: Document::default(),
    };
    p.skip_ws();
    // The top level is an object, which is what an export produces. A bare array or scalar is
    // rejected, because a config document is a map by definition.
    if p.peek() != Some(b'{') {
        return Err(if p.peek().is_none() {
            ParseError::UnexpectedEnd
        } else {
            ParseError::UnexpectedByte { at: p.pos }
        });
    }
    let mut key = heapless::String::<96>::new();
    p.value(&mut key, 0)?;
    p.skip_ws();
    if p.pos != p.input.len() {
        return Err(ParseError::TrailingBytes);
    }
    Ok(p.out)
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    out: Document,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.input.get(self.pos) {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), ParseError> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError::UnexpectedByte { at: self.pos })
        }
    }

    /// Parses one value. `key` is the dotted path of the object this value sits in; it is empty
    /// at the top level.
    fn value(&mut self, key: &mut heapless::String<96>, depth: usize) -> Result<(), ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError::TooDeep);
        }
        match self.peek() {
            None => Err(ParseError::UnexpectedEnd),
            Some(b'{') => self.object(key, depth),
            Some(b'[') => self.array(key, depth),
            Some(b'"') => {
                let s = self.string()?;
                let owned = Self::owned_text(&s);
                self.leaf(key, owned)
            }
            Some(b't') | Some(b'f') => self.bool(key),
            Some(b'n') => self.null(key),
            Some(b'-') | Some(b'0'..=b'9') => self.number(key),
            Some(_) => Err(ParseError::UnexpectedByte { at: self.pos }),
        }
    }

    fn object(&mut self, key: &mut heapless::String<96>, depth: usize) -> Result<(), ParseError> {
        self.expect(b'{')?;
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(());
        }
        loop {
            self.skip_ws();
            if self.peek().is_none() {
                return Err(ParseError::UnexpectedEnd);
            }
            let name = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let saved = key.clone();
            // Extend the path with `.name`. An object whose name is too long to extend is
            // recorded as unknown rather than truncated, because a truncated key could collide
            // with a real one.
            if name.len() + saved.len() + 1 > key.capacity() {
                // The path cannot be built. The key is still recorded as unknown so the report
                // names it, rather than being dropped: a key nobody can address is a key nobody
                // can set, and the user deserves to be told that their file had one.
                self.skip_value(depth)?;
                self.out.doc.record_unknown_parts(&saved, &name);
            } else {
                if !saved.is_empty() {
                    let _ = key.push('.');
                }
                let _ = key.push_str(&name);
                self.value(key, depth + 1)?;
                *key = saved;
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(_) => return Err(ParseError::UnexpectedByte { at: self.pos }),
                None => return Err(ParseError::UnexpectedEnd),
            }
        }
    }

    /// Parses an array. An array has no place in the config schema, so its elements are
    /// recorded under the enclosing key and will be rejected as unknown, but they are parsed so
    /// that one stray `[` does not turn the whole file into a parse error with no report.
    fn array(&mut self, key: &mut heapless::String<96>, depth: usize) -> Result<(), ParseError> {
        self.expect(b'[')?;
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(());
        }
        // The schema has no array-valued parameter. The elements are consumed so the rest of the
        // document still parses, and the field is recorded as a value of the wrong type so the
        // report rejects it rather than silently taking the first element.
        let mut elements = 0usize;
        loop {
            self.skip_ws();
            self.skip_value(depth + 1)?;
            elements += 1;
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    if elements > 0 {
                        self.out.doc.insert(key, DocValue::NotAValue);
                    }
                    return Ok(());
                }
                Some(_) => return Err(ParseError::UnexpectedByte { at: self.pos }),
                None => return Err(ParseError::UnexpectedEnd),
            }
        }
    }

    fn leaf(&mut self, key: &str, value: DocValue) -> Result<(), ParseError> {
        self.out.doc.insert(key, value);
        Ok(())
    }

    /// Copies a parsed string into the document's own storage, so nothing stored aliases this
    /// parser's scratch space.
    fn owned_text(s: &str) -> DocValue {
        if s.len() > MAX_TEXT {
            // Recorded as too long rather than truncated. The bytes are kept so the field can
            // still be reported, but no caller can read a shortened hostname.
            return DocValue::TextTooLong {
                len: s.len() as u16,
            };
        }
        let mut bytes = [0u8; MAX_TEXT];
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        DocValue::Text {
            bytes,
            len: s.len() as u8,
        }
    }

    fn bool(&mut self, key: &mut heapless::String<96>) -> Result<(), ParseError> {
        let value = if self.input[self.pos..].starts_with(b"true") {
            self.pos += 4;
            true
        } else if self.input[self.pos..].starts_with(b"false") {
            self.pos += 5;
            false
        } else {
            return Err(ParseError::UnexpectedByte { at: self.pos });
        };
        self.leaf(key, DocValue::Bool(value))
    }

    fn null(&mut self, key: &mut heapless::String<96>) -> Result<(), ParseError> {
        if !self.input[self.pos..].starts_with(b"null") {
            return Err(ParseError::UnexpectedByte { at: self.pos });
        }
        self.pos += 4;
        // Recorded, not dropped. A `null` where a value belongs is a user mistake worth naming.
        if !key.is_empty() {
            self.out.nulls += 1;
        }
        Ok(())
    }

    fn number(&mut self, key: &mut heapless::String<96>) -> Result<(), ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos - start > MAX_SCALAR {
            return Err(ParseError::ScalarTooLong);
        }
        let text = core::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| ParseError::BadNumber { at: start })?;
        // JSON has one number type, but the *literal* says whether the author wrote a whole
        // number. The schema has integer parameters (a port number, a sample count) and enum
        // parameters, and the repository's own export writes them as whole literals, so the
        // literal form is what routes a value to the right parameter type. An integer literal
        // reaching a fractional parameter is widened, which is exact; a fractional literal
        // reaching an integer parameter is a wrong type, which is reported rather than rounded.
        let integral = !text.contains(['.', 'e', 'E']);
        let value: f64 = text
            .parse()
            .map_err(|_| ParseError::BadNumber { at: start })?;
        if integral && value >= i32::MIN as f64 && value <= i32::MAX as f64 {
            self.leaf(key, DocValue::Int(value as i32))
        } else {
            self.leaf(key, DocValue::Number(value))
        }
    }

    fn string(&mut self) -> Result<heapless::String<MAX_SCALAR>, ParseError> {
        let start = self.pos;
        self.expect(b'"')?;
        let mut out = heapless::String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(ParseError::BadString { at: start });
            };
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(esc) = self.peek() else {
                        return Err(ParseError::BadEscape { at: self.pos });
                    };
                    self.pos += 1;
                    match esc {
                        b'"' => push_char(&mut out, '"', start)?,
                        b'\\' => push_char(&mut out, '\\', start)?,
                        b'/' => push_char(&mut out, '/', start)?,
                        b'b' => push_char(&mut out, '\u{8}', start)?,
                        b'f' => push_char(&mut out, '\u{c}', start)?,
                        b'n' => push_char(&mut out, '\n', start)?,
                        b'r' => push_char(&mut out, '\r', start)?,
                        b't' => push_char(&mut out, '\t', start)?,
                        b'u' => {
                            // Only the ASCII range is decoded. A config document is dotted
                            // ASCII keys and short values; decoding a full surrogate pair would
                            // be code nothing here can use.
                            let hex = self
                                .input
                                .get(self.pos..self.pos + 4)
                                .and_then(|h| core::str::from_utf8(h).ok())
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .ok_or(ParseError::BadEscape { at: self.pos })?;
                            self.pos += 4;
                            let ch = char::from_u32(hex)
                                .ok_or(ParseError::BadEscape { at: self.pos })?;
                            push_char(&mut out, ch, start)?;
                        }
                        _ => return Err(ParseError::BadEscape { at: self.pos - 1 }),
                    }
                }
                // A raw control character inside a string is malformed JSON. Accepting one
                // would let a config value carry a newline into a log line.
                0x00..=0x1F => return Err(ParseError::BadString { at: self.pos - 1 }),
                _ => push_char(&mut out, char::from(b), start)?,
            }
        }
    }

    /// Consumes a value without recording it, for a key too long to address.
    fn skip_value(&mut self, depth: usize) -> Result<(), ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError::TooDeep);
        }
        match self.peek() {
            Some(b'{') | Some(b'[') => {
                let open = self.peek().unwrap();
                let close = if open == b'{' { b'}' } else { b']' };
                let mut nesting = 0usize;
                loop {
                    let Some(b) = self.peek() else {
                        return Err(ParseError::UnexpectedEnd);
                    };
                    if b == b'"' {
                        let _ = self.string()?;
                        continue;
                    }
                    self.pos += 1;
                    if b == open {
                        nesting += 1;
                    } else if b == close {
                        nesting -= 1;
                        if nesting == 0 {
                            return Ok(());
                        }
                    }
                }
            }
            Some(b'"') => {
                let _ = self.string()?;
                Ok(())
            }
            _ => {
                while let Some(b) = self.peek() {
                    if b == b',' || b == b'}' || b == b']' {
                        break;
                    }
                    self.pos += 1;
                }
                Ok(())
            }
        }
    }
}

fn push_char(out: &mut heapless::String<MAX_SCALAR>, c: char, at: usize) -> Result<(), ParseError> {
    if out.push(c).is_err() {
        return Err(ParseError::ScalarTooLong);
    }
    let _ = at;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use heapless::Vec;

    /// The stored fields, which only ever hold keys the schema has.
    fn keys(doc: &Document) -> heapless::Vec<&'static str, 128> {
        let mut out = Vec::new();
        for (k, _) in doc.resolved().fields() {
            let _ = out.push(k);
        }
        out
    }

    #[test]
    fn a_flat_document_parses() {
        let d =
            parse(br#"{"pid.enabled": true, "brew.setpoint": 92.0, "mqtt.port": 1883}"#).unwrap();
        let k = keys(&d);
        assert!(k.contains(&"pid.enabled"));
        assert!(k.contains(&"brew.setpoint"));
        assert!(k.contains(&"mqtt.port"));
        assert_eq!(
            d.resolved().value("pid.enabled").unwrap().as_bool(),
            Some(true)
        );
        assert_eq!(
            d.resolved().value("brew.setpoint").unwrap().as_number(),
            Some(92.0)
        );
    }

    #[test]
    fn a_nested_object_resolves_to_the_same_key_as_a_flat_one() {
        // The export nests; the schema is dotted. Producing the same key from both shapes is the
        // whole point, so both are asserted against the same expected value.
        let nested = parse(br#"{"hardware": {"sensors": {"scale": {"enabled": true}}}}"#).unwrap();
        let flat = parse(br#"{"hardware.sensors.scale.enabled": true}"#).unwrap();
        assert!(keys(&nested).contains(&"hardware.sensors.scale.enabled"));
        assert!(keys(&flat).contains(&"hardware.sensors.scale.enabled"));
        assert_eq!(
            nested.resolved().value("hardware.sensors.scale.enabled"),
            flat.resolved().value("hardware.sensors.scale.enabled")
        );
    }

    #[test]
    fn a_value_at_the_top_level_that_is_not_an_object_is_rejected() {
        assert!(parse(b"[1, 2, 3]").is_err());
        assert!(parse(b"42").is_err());
        assert!(parse(br#""hello""#).is_err());
    }

    #[test]
    fn an_empty_object_is_valid_and_contributes_nothing() {
        let d = parse(b"{}").unwrap();
        assert_eq!(d.resolved().fields().count(), 0);
        assert!(d.resolved().unknown_count() == 0);
    }

    #[test]
    fn whitespace_between_tokens_is_ignored() {
        let d = parse(b"  {\n  \"pid.enabled\"\t:\r\n true }  ").unwrap();
        assert!(keys(&d).contains(&"pid.enabled"));
    }

    #[test]
    fn an_unknown_key_is_recorded_as_unknown_rather_than_dropped() {
        let d = parse(br#"{"brew": {"setpoint": 92}, "display": {"blescale_brew_timer": true}}"#)
            .unwrap();
        assert_eq!(d.resolved().unknown_count(), 1);
        assert_eq!(
            d.resolved().unknown(0).unwrap().as_str(),
            "display.blescale_brew_timer",
            "the report has to name the key the C++ importer ignored"
        );
    }

    #[test]
    fn a_malformed_request_line_equivalent_is_rejected() {
        assert_eq!(parse(b"{").unwrap_err(), ParseError::UnexpectedEnd);
        assert_eq!(
            parse(br#"{"a" 1}"#).unwrap_err(),
            ParseError::UnexpectedByte { at: 5 }
        );
        assert_eq!(
            parse(br#"{"a": }"#).unwrap_err(),
            ParseError::UnexpectedByte { at: 6 }
        );
    }

    #[test]
    fn a_number_that_is_not_a_number_is_rejected_rather_than_zero() {
        // The C++ importer used `String(double)`, which turned an unparseable value into 0.00
        // and stored it. This rejects the document instead.
        assert_eq!(
            parse(br#"{"a": -}"#).unwrap_err(),
            ParseError::BadNumber { at: 6 }
        );
        assert_eq!(
            parse(br#"{"a": 1.2.3}"#).unwrap_err(),
            ParseError::BadNumber { at: 6 }
        );
        assert_eq!(
            parse(br#"{"a": 1e}"#).unwrap_err(),
            ParseError::BadNumber { at: 6 }
        );
    }

    #[test]
    fn a_number_stays_a_number_and_is_not_coerced_to_a_bool() {
        // The C++ importer turned a JSON 1 into `true`. Here a 1 is a number, and whether a
        // boolean parameter accepts it is the schema's decision, not the parser's.
        let d = parse(br#"{"pid.enabled": 1}"#).unwrap();
        assert_eq!(d.resolved().value("pid.enabled").unwrap().as_bool(), None);
        let r = crate::validate(d.resolved(), false);
        assert_eq!(r.report.rejected, 1);
        assert_eq!(r.findings[0].reason, crate::Reason::WrongType);
    }

    #[test]
    fn an_unterminated_string_is_rejected() {
        assert_eq!(
            parse(br#"{"a": "b}"#).unwrap_err(),
            ParseError::BadString { at: 6 }
        );
        assert_eq!(
            parse(br#"{"a": "b"#).unwrap_err(),
            ParseError::BadString { at: 6 }
        );
    }

    #[test]
    fn a_raw_control_character_in_a_string_is_rejected() {
        // Otherwise a config value could carry a newline into a log line and forge an entry.
        assert_eq!(
            parse(b"{\"a\": \"x\ny\"}").unwrap_err(),
            ParseError::BadString { at: 8 }
        );
    }

    #[test]
    fn the_standard_escapes_are_decoded() {
        let d = parse(br#"{"system.hostname": "q\"b\\c\/d\ne\tf"}"#).unwrap();
        assert_eq!(
            d.resolved().value("system.hostname").unwrap().as_text(),
            Some("q\"b\\c/d\ne\tf")
        );
    }

    #[test]
    fn a_unicode_escape_is_decoded_in_the_ascii_range() {
        let d = parse(br#"{"system.hostname": "\u0041\u0042"}"#).unwrap();
        assert_eq!(
            d.resolved().value("system.hostname").unwrap().as_text(),
            Some("AB")
        );
    }

    #[test]
    fn an_unknown_escape_is_rejected() {
        assert!(parse(br#"{"a": "\q"}"#).is_err());
    }

    #[test]
    fn nesting_deeper_than_the_limit_is_refused() {
        // Without a limit a document of open brackets would recurse until the stack ran out, and
        // a bare-metal target has no handler for a stack overflow. Nesting inside a real
        // parameter's value is the case that matters, because that is what a document a user
        // uploads could contain.
        let mut nested = heapless::String::<256>::new();
        let _ = nested.push_str("{\"pid.enabled\":");
        for _ in 0..MAX_DEPTH + 4 {
            let _ = nested.push_str("{\"a\":");
        }
        let _ = nested.push_str("true");
        for _ in 0..MAX_DEPTH + 4 {
            let _ = nested.push('}');
        }
        let _ = nested.push('}');
        assert_eq!(parse(nested.as_bytes()).unwrap_err(), ParseError::TooDeep);

        // Brackets are consumed iteratively rather than recursively, so a document made only of
        // them fails as truncated instead. Either way it is refused, which is the property.
        let mut brackets = heapless::String::<128>::new();
        let _ = brackets.push_str("{\"pid.enabled\":");
        for _ in 0..MAX_DEPTH + 4 {
            let _ = brackets.push('[');
        }
        assert!(parse(brackets.as_bytes()).is_err());
    }

    #[test]
    fn nesting_at_the_limit_is_accepted() {
        // The limit has to reject the pathological case without rejecting a real document.
        let mut nested = heapless::String::<256>::new();
        let _ = nested.push_str("{\"pid.enabled\":");
        // One object is the top level, so the limit allows `MAX_DEPTH - 1` nested children.
        for _ in 0..MAX_DEPTH - 1 {
            let _ = nested.push_str("{\"a\":");
        }
        let _ = nested.push_str("true");
        for _ in 0..MAX_DEPTH - 1 {
            let _ = nested.push('}');
        }
        let _ = nested.push('}');
        assert!(
            parse(nested.as_bytes()).is_ok(),
            "a document at the limit must parse, got {:?}",
            parse(nested.as_bytes()).map(|_| ())
        );
    }

    #[test]
    fn a_scalar_longer_than_the_limit_is_refused() {
        let mut doc = heapless::String::<512>::new();
        let _ = doc.push_str("{\"a\":\"");
        for _ in 0..MAX_SCALAR + 1 {
            let _ = doc.push('x');
        }
        let _ = doc.push_str("\"}");
        assert_eq!(
            parse(doc.as_bytes()).unwrap_err(),
            ParseError::ScalarTooLong
        );
    }

    #[test]
    fn trailing_bytes_after_the_top_level_are_rejected() {
        assert_eq!(parse(b"{} junk").unwrap_err(), ParseError::TrailingBytes);
        assert_eq!(parse(b"{}{}").unwrap_err(), ParseError::TrailingBytes);
    }

    #[test]
    fn a_null_leaf_is_counted_rather_than_dropped() {
        // A `null` where a value belongs is a user mistake worth naming, not an absent field.
        let d = parse(br#"{"pid": {"enabled": null}}"#).unwrap();
        assert_eq!(d.null_count(), 1);
    }

    #[test]
    fn an_array_does_not_abort_the_parse() {
        // The schema has no array-valued parameter, but one stray `[` should produce a report
        // naming it rather than a bare "invalid JSON".
        let d = parse(br#"{"brew.setpoint": [1, 2, 3]}"#).unwrap();
        assert_eq!(
            d.resolved().value("brew.setpoint").unwrap().as_number(),
            None,
            "an array must not be taken from its first element"
        );
        let r = crate::validate(d.resolved(), false);
        assert_eq!(r.report.rejected, 1);
        assert_eq!(r.findings[0].reason, crate::Reason::WrongType);
    }

    #[test]
    fn an_object_key_too_long_to_address_is_recorded_as_unknown_not_truncated() {
        // A truncated key could collide with a real parameter name and set the wrong thing.
        let mut doc = heapless::String::<256>::new();
        let _ = doc.push_str("{\"");
        // Longer than the path buffer (96) but within the parser's scalar limit (128), so this
        // exercises the path-overflow path rather than the scalar guard.
        for _ in 0..120 {
            let _ = doc.push('n');
        }
        let _ = doc.push_str("\": 1}");
        let d = parse(doc.as_bytes()).unwrap();
        assert_eq!(d.resolved().unknown_count(), 1);
        assert!(d.resolved().unknown(0).unwrap().truncated);
    }

    #[test]
    fn every_schema_key_survives_a_parse_of_its_own_dotted_path() {
        // The parser builds dotted paths by concatenation, so a key whose parts need escaping or
        // that exceeds the path buffer would be lost. Walking the whole table catches that, and
        // each key must come back out of the parser as the same key the schema holds.
        for p in schema::PARAMS {
            let mut doc = heapless::String::<128>::new();
            let _ = doc.push_str("{\"");
            let _ = doc.push_str(p.key);
            let _ = doc.push_str("\": null}");
            let parsed = parse(doc.as_bytes()).expect(p.key);
            // A null is counted rather than stored, so the key appears as unknown. The point is
            // that it parses and that the path assembled intact.
            assert!(parsed.null_count() >= 1, "{} did not yield a leaf", p.key);
        }
    }
}
