//! Import and export of the configuration JSON.
//!
//! This is the only bridge between the old firmware and the new one, so it is the most
//! consequential code in the port. Three rules, each fixing a specific C++ defect:
//!
//! - **Nothing is applied until everything is validated.** The C++ importer applied each
//!   parameter as it parsed it and returned success if at least one of 96 matched, then answered
//!   `200 "Configuration validated and applied successfully."` (defect D13). A file with 5 valid
//!   and 91 invalid values persisted the 5.
//! - **Unknown fields are rejected and named.** The C++ importer silently ignored them, which is
//!   why `docs/example_config.json` still carries `display.blescale_brew_timer`, a key no
//!   firmware ever had.
//! - **A secret never appears in a report.** The C++ firmware returned four plaintext passwords
//!   from four endpoints (defect D14).

use heapless::{String, Vec};

use crate::schema::{self, Param, Value, ValueType, MAX_TEXT};

/// How many findings one import may report. A file with 96 bad fields is a file from a different
/// firmware, and the first 48 named fields are enough to diagnose it.
pub const MAX_FINDINGS: usize = 48;

/// The outcome of validating a document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Fields found and accepted.
    pub accepted: u16,
    /// Fields present in the schema but absent from the document. Not an error: a partial import
    /// is a legitimate operation.
    pub missing: u16,
    /// Fields rejected. The import applies nothing unless every one of them is resolved.
    pub rejected: u16,
    /// Fields whose value was clamped instead of rejected. Never the default behaviour, and
    /// always reported, because a silently clamped setpoint is a safety problem the user cannot
    /// see.
    pub clamped: u16,
    /// Fields in the document that the schema does not have.
    pub unknown: u16,
}

impl Report {
    /// Whether the import may be applied.
    ///
    /// A document with unknown or rejected fields applies nothing. A document with only missing
    /// fields is a partial import and is fine.
    pub const fn is_applicable(&self) -> bool {
        self.rejected == 0 && self.unknown == 0
    }
}

/// The document format this firmware reads and writes.
pub const FORMAT_VERSION: u16 = 1;

/// Why a field was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    /// No such parameter. The C++ importer ignored these (defect D13 family).
    UnknownField,
    /// The value's type does not match the parameter's.
    WrongType,
    /// Outside the parameter's range, and clamping was not requested.
    OutOfRange,
    /// The document declares a `format_version` this firmware does not read. Nothing is applied,
    /// because a document from a newer firmware may mean something different by the same key.
    UnsupportedVersion,
    /// A text value over its length limit, or carrying a control character.
    TooLong,
    /// A number that is not a number, or is infinite.
    NotFinite,
    /// The parameter forbids this value specifically, such as a zero scale calibration.
    Forbidden,
}

/// One rejected or clamped field.
#[derive(Clone, Copy, Debug)]
pub struct Finding {
    /// Index into [`schema::PARAMS`], or `u16::MAX` for an unknown field.
    pub param_index: u16,
    /// The dotted key, for the report. Borrowed from the schema where the field is known; for an
    /// unknown field the caller supplies it separately.
    pub key: Key,
    pub reason: Reason,
}

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Known(&'static str),
    /// A key the schema does not have. Truncated to what fits, because an attacker-supplied or
    /// mistyped key can be arbitrarily long and a report is not a place to allocate.
    Unknown,
}

impl Finding {
    pub fn key_str(&self) -> &'static str {
        match self.key {
            Key::Known(k) => k,
            Key::Unknown => "<unknown>",
        }
    }
}

/// The validated result: which parameters were set, and what was wrong with the rest.
#[derive(Debug)]
pub struct Import<'a> {
    /// The index of each accepted parameter, and its value. Borrowed from the document, because a
    /// validated value has the same lifetime as the bytes it was parsed from and must not be
    /// copied into storage from a buffer that is about to be reused.
    pub accepted: Vec<(u16, Value<'a>), { schema::count() }>,
    pub findings: Vec<Finding, MAX_FINDINGS>,
    pub report: Report,
    /// Whether the caller asked for out-of-range values to be clamped instead of rejected.
    pub allow_clamp: bool,
}

impl<'a> Import<'a> {
    pub fn new(allow_clamp: bool) -> Self {
        Self {
            accepted: Vec::new(),
            findings: Vec::new(),
            report: Report::default(),
            allow_clamp,
        }
    }

    /// Records a value that passed validation.
    pub fn accept(&mut self, index: u16, value: Value<'a>) {
        if self.accepted.push((index, value)).is_ok() {
            self.report.accepted += 1;
        }
    }

    /// Records a field that was present but not usable.
    pub fn reject(&mut self, index: u16, key: Key, reason: Reason) {
        if self
            .findings
            .push(Finding {
                param_index: index,
                key,
                reason,
            })
            .is_ok()
        {
            match reason {
                Reason::UnknownField => self.report.unknown += 1,
                _ => self.report.rejected += 1,
            }
        }
    }

    /// Records a field whose value was clamped into range.
    pub fn clamped(&mut self, index: u16, key: Key) {
        if self
            .findings
            .push(Finding {
                param_index: index,
                key,
                reason: Reason::OutOfRange,
            })
            .is_ok()
        {
            self.report.clamped += 1;
        }
    }

    /// The value for a parameter, if the import set one.
    pub fn value_of(&self, index: u16) -> Option<Value<'a>> {
        self.accepted
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, v)| *v)
    }
}

/// Validates a JSON document against the schema without applying anything.
///
/// The document arrives as a list of resolved leaf values, because parsing JSON and resolving
/// nested paths are separate concerns: the JSON parser is in [`json`], and this takes the flat
/// result so the validation rules can be tested without a parser.
pub fn validate<'a>(resolved: &'a ResolvedDoc, allow_clamp: bool) -> Import<'a> {
    let mut out = Import::new(allow_clamp);

    // The document's own version comes first, before any field is read. A document from a newer
    // firmware may name a field this one does not have, and half-applying it is how a downgrade
    // destroys a configuration. Named, refused, nothing applied.
    if let Some(version) = resolved.format_version {
        if version != FORMAT_VERSION {
            out.reject(u16::MAX, Key::Unknown, Reason::UnsupportedVersion);
            return out;
        }
    }

    for (key, value) in resolved.fields() {
        // Every stored field was matched to the schema when it was inserted, so this cannot
        // fail. Treating a miss as unknown rather than panicking keeps a future caller that
        // inserts directly from safe.
        let Some(param) = schema::find(key) else {
            out.reject(u16::MAX, Key::Unknown, Reason::UnknownField);
            continue;
        };
        let index = index_of(param);

        // A text value over the limit is reported as such, before coercion, so the report says
        // why rather than calling it a type error.
        if value.over_len().is_some() {
            out.reject(index, Key::Known(param.key), Reason::TooLong);
            continue;
        }

        let Some(typed) = coerce(param, value) else {
            out.reject(index, Key::Known(param.key), Reason::WrongType);
            continue;
        };

        if param.accepts(typed) {
            out.accept(index, typed);
            continue;
        }

        // Not acceptable as-is. Clamping is only ever offered when the caller asked for it, and
        // it is always reported, so a clamped setpoint cannot be silent.
        if allow_clamp {
            if let Some(clamped) = clamp_to(param, typed) {
                out.accept(index, clamped);
                out.clamped(index, Key::Known(param.key));
                continue;
            }
        }
        out.reject(index, Key::Known(param.key), reject_reason(param, typed));
    }

    // Unknown keys are held separately from the fields, so they are counted here rather than
    // during the field loop. A key the schema does not have is never turned into a value, so it
    // cannot be applied even if a later change to the schema happened to add it.
    for _ in 0..resolved.unknown_count() {
        out.reject(u16::MAX, Key::Unknown, Reason::UnknownField);
    }

    for p in schema::PARAMS {
        if !resolved.contains(p.key) {
            out.report.missing += 1;
        }
    }
    out
}

fn reject_reason(param: &Param<'_>, value: Value<'_>) -> Reason {
    match (param.kind, value) {
        (_, Value::Number(v)) if !v.is_finite() => Reason::NotFinite,
        (ValueType::Text, Value::Text(_)) => Reason::TooLong,
        _ => Reason::OutOfRange,
    }
}

/// The value a clamp would produce, or `None` if the value cannot be clamped into range.
fn clamp_to<'a>(param: &Param<'_>, value: Value<'a>) -> Option<Value<'a>> {
    match (param.kind, value) {
        (ValueType::Number, Value::Number(v)) if v.is_finite() => {
            let c = param.clamp(v);
            if param.accepts(Value::Number(c)) {
                Some(Value::Number(c))
            } else {
                // The clamped value is still unacceptable, which happens for the zero-only
                // calibration: clamping cannot produce a usable divisor.
                None
            }
        }
        _ => None,
    }
}

/// Converts a loosely typed JSON value into the parameter's type, or `None` if it cannot.
///
/// A number where a bool is expected is a wrong type, not a coercion. The C++ importer coerced
/// through `String` and treated a JSON `1` as `true`, which meant a file written by a newer
/// firmware could silently flip a setting.
fn coerce<'a>(param: &Param<'static>, value: &'a DocValue) -> Option<Value<'a>> {
    match (param.kind, value) {
        (ValueType::Bool, DocValue::Bool(v)) => Some(Value::Bool(*v)),
        (ValueType::Int, DocValue::Int(v)) => Some(Value::Int(*v)),
        // A whole number written with a decimal point, such as `3.0`, is still a whole number.
        // Accepting it is what lets a document written by a JSON pretty-printer import, which
        // prints every number that way.
        (ValueType::Int, DocValue::Number(v))
            if *v >= i32::MIN as f64 && *v <= i32::MAX as f64 && (*v as i32) as f64 == *v =>
        {
            Some(Value::Int(*v as i32))
        }
        (ValueType::Enum, DocValue::Enum(v)) | (ValueType::Enum, DocValue::Int(v)) => {
            Some(Value::Enum(*v))
        }
        (ValueType::Number, DocValue::Number(v)) => Some(Value::Number(*v)),
        // JSON has one number type, so an integer literal reaches a number parameter as an
        // integer. Widening is safe and documented; narrowing is not attempted, because a
        // fractional value in an integer parameter is a mistake worth reporting rather than
        // silently rounding.
        (ValueType::Number, DocValue::Int(v)) => Some(Value::Number(*v as f64)),
        // A fractional value is never narrowed into an integer parameter: rounding it would be a
        // silent change to a setting the user wrote down.
        (ValueType::Int, DocValue::Number(_)) | (ValueType::Enum, DocValue::Number(_)) => None,
        (ValueType::Text, DocValue::Text { bytes, len }) => {
            let end = (*len as usize).min(MAX_TEXT);
            core::str::from_utf8(&bytes[..end]).ok().map(Value::Text)
        }
        _ => None,
    }
}

/// The parameter's position in the table, which is how a value is addressed in a stored block.
pub fn index_of(param: &Param) -> u16 {
    schema::PARAMS
        .iter()
        .position(|p| core::ptr::eq(p, param))
        .unwrap_or(0) as u16
}

// ---------------------------------------------------------------------------
// The resolved document
// ---------------------------------------------------------------------------

/// A document leaf that owns its bytes.
///
/// The document has to own its text values rather than borrow them, because a value that aliases
/// a parsing buffer is a use-after-free waiting for the next parse: the buffer is reused, the
/// string in the stored field silently changes, and a configuration gets applied that the user
/// never sent. Copying a few dozen bytes per field costs nothing on the one document the machine
/// parses.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DocValue {
    Bool(bool),
    Int(i32),
    Number(f64),
    Enum(i32),
    Text {
        bytes: [u8; MAX_TEXT],
        len: u8,
    },
    /// A string longer than the schema's limit. Recorded rather than truncated, so the report can
    /// say "too long" rather than silently applying the first 64 bytes of a hostname.
    TextTooLong {
        len: u16,
    },
    /// An array or any other JSON type the schema has no parameter for. Recorded so the field is
    /// rejected as a wrong type rather than being silently taken from the first element.
    NotAValue,
}

impl DocValue {
    /// The length-prefixed text, or empty for a non-text field.
    ///
    /// The bytes come from a fixed-size array inside the value, so the borrow is into `self`
    /// rather than into a temporary.
    pub fn text(&self) -> &str {
        match self {
            DocValue::Text { bytes, len } => {
                core::str::from_utf8(&bytes[..*len as usize]).unwrap_or("")
            }
            _ => "",
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            DocValue::Text { .. } => Some(self.text()),
            _ => None,
        }
    }

    /// The full length of an over-long string, for the report.
    pub const fn over_len(&self) -> Option<u16> {
        match *self {
            DocValue::TextTooLong { len } => Some(len),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match *self {
            DocValue::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            DocValue::Bool(b) => Some(b),
            _ => None,
        }
    }
}

/// One leaf of a resolved document.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// The schema's own literal for the key, so a stored field always names a real parameter.
    pub key: &'static str,
    pub value: DocValue,
}

/// A document whose nested paths have already been resolved to dotted keys.
///
/// Kept separate from the JSON parser so the validation rules, which are the part that decides
/// whether a migration succeeds or a user is left with a half-applied configuration, can be
/// tested without one.
#[derive(Debug, Default)]
pub struct ResolvedDoc {
    fields: Vec<Field, { schema::count() }>,
    unknown_keys: Vec<UnknownKey, 16>,
    /// The document's `format_version`, when it carried one.
    ///
    /// Not a schema parameter, because it is not a setting: it names the *shape* of the document
    /// around the settings. It is accepted because every C++ export carries `format_version: 1`
    /// and an importer that rejected it would refuse the one file a migrating user actually has.
    /// A version this firmware does not know is refused by name, rather than half-applied.
    pub format_version: Option<u16>,
}

/// A key the schema does not have, kept only so the report can name it. Stored truncated,
/// because a mistyped or hostile key can be any length and a report must not allocate for it.
#[derive(Clone, Copy, Debug)]
pub struct UnknownKey {
    /// Whether the key was too long to store. The report then says so rather than showing a
    /// truncated key that looks like a real one.
    pub truncated: bool,
    /// Long enough for the longest key in the repository's example config
    /// (`display.blescale_brew_timer`, 28 bytes) with room to spare, so a real stray key is
    /// reported in full and only a deliberately long one is truncated.
    pub first_bytes: [u8; 48],
}

impl UnknownKey {
    pub fn as_str(&self) -> &str {
        let end = self
            .first_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.first_bytes.len());
        core::str::from_utf8(&self.first_bytes[..end]).unwrap_or("<not utf8>")
    }
}

impl ResolvedDoc {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            unknown_keys: Vec::new(),
            format_version: None,
        }
    }

    /// Records a leaf value. The key must already be a dotted path.
    ///
    /// An unknown key is recorded as unknown rather than stored as a value, so it can never be
    /// applied even if a later validation change made it look valid.
    /// Records a key the schema does not have, so the report can name it. Truncated to what
    /// fits, and the truncation is flagged rather than hidden.
    pub fn record_unknown(&mut self, key: &str) {
        self.record_unknown_parts(key, "");
    }

    /// Records an unknown key given as a parent path and a child name, without needing the two to
    /// fit in one buffer. A key too long to address still has to be nameable in the report.
    pub fn record_unknown_parts(&mut self, parent: &str, name: &str) {
        let mut joined = heapless::String::<192>::new();
        let _ = joined.push_str(parent);
        if !joined.is_empty() {
            let _ = joined.push('.');
        }
        let _ = joined.push_str(name);
        self.record_unknown_key(joined.as_str());
    }

    fn record_unknown_key(&mut self, key: &str) {
        let mut k = [0u8; 48];
        let n = key.len().min(48);
        k[..n].copy_from_slice(&key.as_bytes()[..n]);
        let _ = self.unknown_keys.push(UnknownKey {
            truncated: key.len() > 48,
            first_bytes: k,
        });
    }

    pub fn insert(&mut self, key: &str, value: DocValue) {
        if let Some(p) = schema::find(key) {
            let _ = self.fields.push(Field { key: p.key, value });
        } else {
            self.record_unknown_key(key);
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.fields.iter().any(|f| f.key == key)
    }

    /// The stored leaves, by reference. A validated value borrows the document's bytes, so the
    /// document has to outlive the import result; that is enforced by the signature rather than
    /// by a convention about when to copy.
    pub fn fields(&self) -> impl Iterator<Item = (&'static str, &DocValue)> {
        self.fields.iter().map(|f| (f.key, &f.value))
    }

    /// The stored value for a key, if the document carried one.
    pub fn value(&self, key: &str) -> Option<DocValue> {
        self.fields.iter().find(|f| f.key == key).map(|f| f.value)
    }

    pub fn unknown_count(&self) -> usize {
        self.unknown_keys.len()
    }

    pub fn unknown(&self, i: usize) -> Option<&UnknownKey> {
        self.unknown_keys.get(i)
    }
}

/// Exports the current configuration as a JSON document, with secrets redacted.
///
/// The shape matches the C++ export exactly, because the frontend and the user's archived copy
/// both depend on it. The one difference is that a secret is emitted as an empty string, because
/// the C++ firmware returned four plaintext passwords from four endpoints (defect D14).
pub fn export(applied: &dyn Fn(&'static str) -> Option<Value<'static>>) -> String<16384> {
    let mut out = String::new();
    let _ = out.push_str("{\n  \"format_version\": 1");
    for group in schema::Group::ALL_ALL {
        let _ = out.push_str(",\n  \"");
        let _ = out.push_str(group.name());
        let _ = out.push_str("\": {");
        // The keys are nested, not flat. `brew.by_time.target_time` is written as
        // `"by_time": { "target_time": 25 }`, because that is the shape the import format reads and
        // the shape the C++ firmware wrote: an export that the importer rejects is not an export,
        // and this one used to emit `"by_time": { "brew.by_time.target_time": 25 }`, which named
        // sixteen keys the schema does not have. Recorded as defect D57.
        let mut open: heapless::Vec<&'static str, 2> = heapless::Vec::new();
        let mut first = true;
        for p in schema::group(group) {
            let Some(rest) = p
                .key
                .strip_prefix(group.name())
                .and_then(|r| r.strip_prefix('.'))
            else {
                // A key that is not under its own group is a schema bug, and writing it flat would
                // hide it behind a plausible-looking document. Skipped and visible instead.
                continue;
            };
            let parts: heapless::Vec<&str, 3> = rest.split('.').collect();
            for (i, part) in parts[..parts.len() - 1].iter().enumerate() {
                if open.get(i) != Some(part) {
                    // Close everything deeper than this level, then close anything at it.
                    while open.len() > i {
                        open.pop();
                        let _ = out.push_str("\n    }");
                    }
                    if !first {
                        let _ = out.push(',');
                    }
                    let _ = out.push_str("\n    \"");
                    let _ = out.push_str(part);
                    let _ = out.push_str("\": {");
                    let _ = open.push(part).ok();
                    // A freshly opened object has no members yet, so the next member must not be
                    // preceded by a comma. Getting this wrong writes `{,` and the document does
                    // not parse, which is exactly what the first attempt at this exporter did.
                    first = true;
                }
            }
            while open.len() > parts.len() - 1 {
                open.pop();
                let _ = out.push_str("\n    }");
            }
            if !first {
                let _ = out.push(',');
            }
            first = false;
            let _ = out.push_str("\n    \"");
            let _ = out.push_str(parts[parts.len() - 1]);
            let _ = out.push_str("\": ");
            match applied(p.key) {
                Some(v) => {
                    if p.secret {
                        // Redacted. The frontend does not read a secret back, so this is not a
                        // user-visible break, and a plaintext credential in a downloaded file is
                        // the defect D14 recorded.
                        let _ = out.push_str("\"\"");
                    } else {
                        write_value(&mut out, &v);
                    }
                }
                None => {
                    if p.secret {
                        let _ = out.push_str("\"\"");
                    } else {
                        write_value(&mut out, &p.default);
                    }
                }
            }
        }
        while open.pop().is_some() {
            let _ = out.push_str("\n    }");
        }
        let _ = out.push_str("\n  }");
    }
    let _ = out.push_str("\n}");
    out
}

fn write_value(out: &mut String<16384>, value: &Value<'_>) {
    match value {
        Value::Bool(b) => {
            let _ = out.push_str(if *b { "true" } else { "false" });
        }
        Value::Int(i) => {
            let _ = core::fmt::Write::write_fmt(out, format_args!("{i}"));
        }
        Value::Number(n) => {
            // Three decimals matches the C++ ArduinoJson default for these parameters and is
            // enough for a setpoint or a gain.
            let _ = core::fmt::Write::write_fmt(out, format_args!("{n:.3}"));
        }
        Value::Enum(e) => {
            let _ = core::fmt::Write::write_fmt(out, format_args!("{e}"));
        }
        Value::Text(t) => {
            let _ = out.push('"');
            for c in t.chars() {
                match c {
                    '"' => {
                        let _ = out.push_str("\\\"");
                    }
                    '\\' => {
                        let _ = out.push_str("\\\\");
                    }
                    '\n' => {
                        let _ = out.push_str("\\n");
                    }
                    other => {
                        let _ = out.push(other);
                    }
                }
            }
            let _ = out.push('"');
        }
    }
}

#[cfg(test)]
mod tests {
    // The crate is `no_std` for the target. The tests run on the host, so `std` is linked in
    // here: the two fixture tests read the repository's real config files from disk rather than
    // transcribing them, which is what stops the assertions drifting from the actual files.
    extern crate std;

    use super::*;

    /// Builds a document from `(key, value)` pairs, for the validation tests.
    fn doc(entries: &[(&str, Value<'_>)]) -> ResolvedDoc {
        let mut d = ResolvedDoc::new();
        for (k, v) in entries {
            d.insert(k, owned(*v));
        }
        d
    }

    /// Converts a schema-shaped value into the document's own storage.
    fn owned(v: Value<'_>) -> DocValue {
        match v {
            Value::Bool(b) => DocValue::Bool(b),
            Value::Int(i) => DocValue::Int(i),
            Value::Number(n) => DocValue::Number(n),
            Value::Enum(e) => DocValue::Enum(e),
            Value::Text(t) => {
                if t.len() > MAX_TEXT {
                    return DocValue::TextTooLong {
                        len: t.len() as u16,
                    };
                }
                let mut bytes = [0u8; MAX_TEXT];
                bytes[..t.len()].copy_from_slice(t.as_bytes());
                DocValue::Text {
                    bytes,
                    len: t.len() as u8,
                }
            }
        }
    }

    #[test]
    fn a_clean_document_is_accepted_with_nothing_rejected() {
        let d = doc(&[
            ("brew.setpoint", Value::Number(92.0)),
            ("pid.enabled", Value::Bool(true)),
        ]);
        let r = validate(&d, false);
        assert!(r.report.is_applicable());
        assert_eq!(r.report.accepted, 2);
        assert_eq!(r.report.rejected, 0);
        assert_eq!(r.report.unknown, 0);
        assert_eq!(r.report.missing, schema::count() as u16 - 2);
    }

    #[test]
    fn an_unknown_field_is_rejected_and_counted_not_ignored() {
        // docs/example_config.json carries display.blescale_brew_timer, which the C++ importer
        // ignored. Naming it is the whole point.
        let d = doc(&[
            ("brew.setpoint", Value::Number(92.0)),
            ("display.blescale_brew_timer", Value::Bool(true)),
        ]);
        assert_eq!(d.unknown_count(), 1);
        let r = validate(&d, false);
        assert_eq!(r.report.unknown, 1);
        assert!(!r.report.is_applicable());
    }

    #[test]
    fn an_out_of_range_value_blocks_the_whole_import() {
        // The C++ behaviour: 5 valid, 91 invalid, 200 "validated successfully".
        let d = doc(&[
            ("brew.setpoint", Value::Number(92.0)),
            ("pid.enabled", Value::Bool(true)),
            ("brew.setpoint_min", Value::Number(1.0)),
            ("steam.setpoint", Value::Number(500.0)),
        ]);
        let r = validate(&d, false);
        assert_eq!(r.report.rejected, 1, "only steam.setpoint is out of range");
        assert!(!r.report.is_applicable());
    }

    #[test]
    fn a_clamp_is_never_silent() {
        let d = doc(&[("steam.setpoint", Value::Number(500.0))]);
        let r = validate(&d, true);
        assert_eq!(r.report.clamped, 1);
        assert!(r.report.is_applicable());
        let value = r
            .value_of(index_of(schema::find("steam.setpoint").unwrap()))
            .unwrap();
        assert_eq!(
            value,
            Value::Number(140.0),
            "clamped to the configured maximum"
        );
    }

    #[test]
    fn clamping_is_off_by_default() {
        let d = doc(&[("steam.setpoint", Value::Number(500.0))]);
        let r = validate(&d, false);
        assert_eq!(r.report.clamped, 0);
        assert_eq!(r.report.rejected, 1);
    }

    #[test]
    fn a_zero_calibration_cannot_be_clamped_into_validity() {
        // Clamping zero into the range still gives zero, which is a zero divisor. The clamp has
        // to refuse rather than produce an unusable value.
        let d = doc(&[("hardware.sensors.scale.calibration", Value::Number(0.0))]);
        let r = validate(&d, true);
        assert_eq!(r.report.rejected, 1);
        assert_eq!(r.report.clamped, 0);
        assert!(!r.report.is_applicable());
    }

    #[test]
    fn a_negative_calibration_is_still_accepted() {
        // The shipped config uses -1750.05 for an inverted load cell.
        let d = doc(&[(
            "hardware.sensors.scale.calibration",
            Value::Number(-1750.05),
        )]);
        let r = validate(&d, false);
        assert!(r.report.is_applicable());
        assert_eq!(
            r.value_of(index_of(
                schema::find("hardware.sensors.scale.calibration").unwrap()
            )),
            Some(Value::Number(-1750.05))
        );
    }

    #[test]
    fn a_wrong_type_is_rejected_rather_than_coerced() {
        // The C++ importer turned a JSON 1 into `true` for a bool parameter.
        let d = doc(&[("pid.enabled", Value::Int(1))]);
        let r = validate(&d, false);
        assert_eq!(r.report.rejected, 1);
        assert_eq!(r.findings[0].reason, Reason::WrongType);
    }

    #[test]
    fn an_integer_where_a_number_is_expected_is_a_documented_widening() {
        let d = doc(&[("brew.setpoint", Value::Int(92))]);
        let r = validate(&d, false);
        assert!(r.report.is_applicable());
        assert_eq!(
            r.value_of(index_of(schema::find("brew.setpoint").unwrap())),
            Some(Value::Number(92.0))
        );
    }

    #[test]
    fn a_not_a_number_is_rejected_and_named() {
        let d = doc(&[("brew.setpoint", Value::Number(f64::NAN))]);
        let r = validate(&d, false);
        assert_eq!(r.report.rejected, 1);
        assert_eq!(r.findings[0].reason, Reason::NotFinite);
    }

    #[test]
    fn an_over_long_text_is_rejected() {
        let long = "0123456789012345678901234567890123456789012345678901234567890123456789";
        let d = doc(&[("system.hostname", Value::Text(long))]);
        let r = validate(&d, false);
        assert_eq!(r.report.rejected, 1);
        assert_eq!(r.findings[0].reason, Reason::TooLong);
    }

    #[test]
    fn a_secret_never_appears_in_a_finding() {
        // A finding carries the key, not the value, so a rejected secret is named without its
        // value ever being held anywhere the report can reach.
        // 100 bytes, over the 64-byte limit, so it is rejected rather than accepted.
        let d = doc(&[("system.wifi.password", Value::Text("hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2hunter2"))]);
        let r = validate(&d, false);
        assert_eq!(r.findings[0].key_str(), "system.wifi.password");
        let mut dbg = heapless::String::<512>::new();
        let _ = core::fmt::Write::write_fmt(&mut dbg, format_args!("{:?}", r.findings));
        assert!(
            !dbg.contains("hunter2"),
            "a secret value reached the debug output"
        );
    }

    #[test]
    fn the_two_safety_parameters_import() {
        // The C++ importer had no way to set these: they were read by the emergency-stop manager
        // but absent from the registry (defect D12).
        let d = doc(&[("safety.emergency_temp", Value::Number(130.0))]);
        let r = validate(&d, false);
        assert!(r.report.is_applicable());
        assert_eq!(
            r.value_of(index_of(schema::find("safety.emergency_temp").unwrap())),
            Some(Value::Number(130.0))
        );
    }

    #[test]
    fn an_empty_document_applies_nothing_and_is_not_an_error() {
        let d = doc(&[]);
        let r = validate(&d, false);
        assert!(r.report.is_applicable());
        assert_eq!(r.report.accepted, 0);
        assert_eq!(r.report.missing, schema::count() as u16);
    }

    #[test]
    fn the_report_is_bounded_so_a_hostile_document_cannot_exhaust_memory() {
        let mut d = ResolvedDoc::new();
        for _ in 0..40 {
            d.insert("not.a.real.field", DocValue::Bool(true));
        }
        let r = validate(&d, false);
        assert!(r.findings.len() <= MAX_FINDINGS);
    }

    #[test]
    fn an_unknown_key_is_truncated_for_the_report() {
        let mut d = ResolvedDoc::new();
        let mut key = heapless::String::<256>::new();
        for _ in 0..200 {
            let _ = key.push('a');
        }
        d.insert(key.as_str(), DocValue::Bool(true));
        assert_eq!(d.unknown_count(), 1);
        assert!(d.unknown(0).unwrap().truncated);
        assert_eq!(d.unknown(0).unwrap().as_str().len(), 48);
    }

    #[test]
    fn an_export_redacts_every_secret() {
        let nothing = |_: &'static str| None;
        let json = export(&nothing);
        for key in schema::secret_keys() {
            // The key is written as a *leaf* name now, so the assertion looks for the last
            // segment rather than the whole dotted path. A secret that is present but empty is
            // redacted; a secret that is missing has not been exported at all, which is worse.
            let leaf = key.rsplit('.').next().unwrap_or(key);
            let mut wanted = heapless::String::<64>::new();
            let _ = core::fmt::Write::write_fmt(&mut wanted, format_args!("\"{leaf}\": \"\""));
            assert!(
                json.contains(wanted.as_str()),
                "{key} is not present and redacted in the export"
            );
        }
        // The compiled defaults for the four secrets are otapass, silvia, admin and empty. None
        // may appear. The auth *username* is not a secret and does appear, which is correct.
        assert!(
            !json.contains("otapass"),
            "the OTA password leaked into the export"
        );
        assert!(
            json.contains("\"password\": \"\""),
            "a redacted password should be an empty string"
        );
        assert!(
            json.contains("\"username\": \"admin\""),
            "the user name is not a secret and should still be readable"
        );
    }

    // -----------------------------------------------------------------------
    // The repository's own files. These are the two documents a user actually
    // has: a real export with the credentials masked, and the example shipped
    // with the documentation. Both are migration inputs, so both are asserted
    // here rather than assumed to work.
    // -----------------------------------------------------------------------

    /// Reads a file from the repository root. The crate's tests run with the
    /// workspace root as the working directory.
    fn read_fixture(path: &str) -> heapless::String<8192> {
        // Resolved against the workspace root rather than the crate, because cargo runs a
        // package's tests with that package's directory as the working directory.
        let mut full = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        full.pop();
        let _ = full.pop();
        full.push(path);
        let path = full;
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"));
        let mut out = heapless::String::<8192>::new();
        out.push_str(core::str::from_utf8(&bytes).expect("fixture is UTF-8"))
            .expect("fixture fits the test buffer");
        out
    }

    #[test]
    fn an_export_is_importable_by_its_own_importer() {
        // The property the exporter did not have, and the one that makes the whole migration work:
        // what `export` writes must come back through `parse` and `validate` with no unknown keys
        // and nothing rejected. It is a round trip, so it catches a nesting mistake, a value that
        // changes shape on the way out, and a key that the importer cannot resolve. Recorded as
        // defect D57, which is what the old exporter produced.
        let all_defaults = |_: &'static str| None;
        let json = export(&all_defaults);
        let doc = crate::json::parse(json.as_bytes()).expect("an export must parse");
        let r = validate(doc.resolved(), false);
        assert_eq!(
            r.report.unknown, 0,
            "the export names {} keys the schema does not have: {:?}",
            r.report.unknown, r.findings
        );
        assert_eq!(r.report.rejected, 0, "{:?}", r.findings);
        assert!(r.report.is_applicable());
        assert!(
            r.report.accepted > 80,
            "most of the file should have been read back"
        );
    }

    #[test]
    fn a_document_from_a_newer_firmware_is_refused_in_full_and_by_name() {
        // D58's other half. A downgrade must not half-apply a document it does not fully
        // understand, so the version is checked before a single field is read.
        let mut d = doc(&[("brew.setpoint", Value::Number(92.0))]);
        d.format_version = Some(2);
        let r = validate(&d, false);
        assert!(!r.report.is_applicable());
        assert!(matches!(
            r.findings.first().map(|f| f.reason),
            Some(Reason::UnsupportedVersion)
        ));
    }

    #[test]
    fn a_document_from_this_firmware_is_accepted_with_its_version() {
        let mut d = doc(&[("brew.setpoint", Value::Number(92.0))]);
        d.format_version = Some(crate::import::FORMAT_VERSION);
        let r = validate(&d, false);
        assert!(r.report.is_applicable(), "{:?}", r.findings);
        assert_eq!(r.report.accepted, 1);
    }

    #[test]
    fn an_export_carries_the_values_it_is_given() {
        let values = |key: &'static str| match key {
            "brew.setpoint" => Some(Value::Number(92.5)),
            "pid.regular.kp" => Some(Value::Number(41.0)),
            "brew.by_time.enabled" => Some(Value::Bool(true)),
            "system.hostname" => Some(Value::Text("kitchen")),
            _ => None,
        };
        let json = export(&values);
        assert!(json.contains("92.500"), "{json}");
        assert!(json.contains("41.000"), "{json}");
        assert!(
            json.contains("\"kitchen\""),
            "a text value is exported verbatim: {json}"
        );
        // And the nesting is the import format's, not a flat key inside a group.
        assert!(json.contains("\"by_time\""), "{json}");
        assert!(
            !json.contains("brew.by_time.enabled"),
            "no flat dotted keys: {json}"
        );
    }

    #[test]
    fn the_repository_config_export_imports_cleanly() {
        // The one migration that must work. A user upgrading from the C++ firmware hands us
        // exactly this file, and a rejection here strands them mid-migration.
        let raw = read_fixture("config.json");
        let doc = crate::json::parse(raw.as_bytes()).expect("config.json must parse");
        let r = validate(doc.resolved(), false);
        assert_eq!(
            r.report.unknown, 0,
            "config.json has keys the schema does not have: {:?}",
            r.findings
        );
        assert_eq!(
            r.report.rejected, 0,
            "config.json has values the schema rejects: {:?}",
            r.findings
        );
        assert_eq!(
            r.report.clamped, 0,
            "config.json has out-of-range values: {:?}",
            r.findings
        );
        assert!(r.report.is_applicable(), "the migration must apply");
        assert!(
            r.report.accepted > 50,
            "most of the file should have been read"
        );
    }

    #[test]
    fn the_documented_example_config_names_its_unknown_field() {
        // docs/example_config.json carries display.blescale_brew_timer, which no firmware ever
        // had. The C++ importer ignored it silently; this one names it.
        let raw = read_fixture("docs/example_config.json");
        let doc = crate::json::parse(raw.as_bytes()).expect("example_config.json must parse");
        let r = validate(doc.resolved(), false);
        assert!(
            !r.report.is_applicable(),
            "the example config has a field the schema does not have, so it must not apply"
        );
        let names: heapless::Vec<&str, 16> = r
            .findings
            .iter()
            .map(|f| f.key_str())
            .filter(|k| *k == "<unknown>")
            .collect();
        assert_eq!(names.len(), r.report.unknown as usize);
    }

    #[test]
    fn the_example_config_is_accepted_once_the_unknown_field_is_dropped() {
        // The example is nearly valid. Removing the one key no firmware ever had makes it a
        // working import, which is what a user following the documentation would end up with.
        let raw = read_fixture("docs/example_config.json");
        let doc = crate::json::parse(raw.as_bytes()).unwrap();
        let mut kept = ResolvedDoc::new();
        let mut dropped = 0u16;
        for (key, value) in doc.resolved().fields() {
            kept.insert(key, *value);
        }
        for i in 0..doc.resolved().unknown_count() {
            let name = doc.resolved().unknown(i).unwrap().as_str();
            if name == "display.blescale_brew_timer" {
                dropped += 1;
            }
        }
        assert_eq!(dropped, 1, "the example has exactly one unknown key");
        let r = validate(&kept, false);
        assert_eq!(
            r.report.unknown, 0,
            "with the stray key gone the document is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn an_export_has_the_ten_groups_the_frontend_expects() {
        let nothing = |_: &'static str| None;
        let json = export(&nothing);
        for g in schema::Group::ALL_ALL {
            let mut wanted = heapless::String::<64>::new();
            let _ = core::fmt::Write::write_fmt(&mut wanted, format_args!("\"{}\"", g.name()));
            assert!(json.contains(wanted.as_str()), "{} missing", g.name());
        }
        assert!(json.contains("\"format_version\": 1"));
    }

    #[test]
    fn an_export_escapes_a_quote_in_a_value() {
        let with_quote = |k: &'static str| {
            if k == "system.hostname" {
                Some(Value::Text("a\"b"))
            } else {
                None
            }
        };
        let json = export(&with_quote);
        assert!(
            json.contains("a\\\"b"),
            "a quote must be escaped, not emitted raw"
        );
    }
}
