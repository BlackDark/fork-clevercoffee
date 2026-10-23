//! Form decoding for `POST /api/parameters`, shared by query and body pairs.
//!
//! Query pairs precede body pairs. ESP-IDF keeps the query inside the request URI;
//! `split_once('?')` suffices, without FFI. `GET /api/parameters` ignores `filter`.
//!
//! The [WHATWG parser](https://url.spec.whatwg.org/#application/x-www-form-urlencoded)
//! splits on `&` and the first `=`, skips empty fields, and decodes `+` and `%XX`.
//! Invalid UTF-8 is rejected: lossy decoding could change a parameter's name.
//! Decoded strings are allocated because they cannot borrow transformed bytes.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

/// A parsed `name=value` pair.
///
/// **Owned, not borrowed**, because percent-decoding produces bytes the body
/// does not contain: `name=caf%C3%A9` has to produce `"café"`, and a `&str`
/// borrowed from `name` cannot. A field that needs no decoding is still copied,
/// which is why [`field`] — the single-field lookup every handler uses — is
/// separate from [`parse_form`] and returns a `String` directly.
pub type Field = (String, String);

/// Parse a form-encoded body.
///
/// Malformed fields are skipped rather than failing the whole request:
/// `/api/parameters` reports per-parameter success, and a request with one bad
/// field out of twenty should still update the other nineteen.
#[must_use]
pub fn parse_form(body: &str) -> Vec<Field> {
    let mut out = Vec::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        // `let-else` does not bind its pattern inside the `else` block, so the
        // bare-name branch re-splits rather than reusing `raw_name`.
        let Some((raw_name, raw_value)) = pair.split_once('=') else {
            // A bare name with no `=`. The HTML spec makes this an empty value;
            // treating it as one is what a browser does with a valueless input.
            let Some(name) = decode(pair) else { continue };
            out.push((name, String::new()));
            continue;
        };
        let (Some(name), Some(value)) = (decode(raw_name), decode(raw_value)) else {
            continue;
        };
        out.push((name, value));
    }
    out
}

/// The first value for `name`, if the body carries it.
///
/// A first-match lookup, which is what `/api/parameters` needs: its parameters
/// are set one at a time, so a name that arrives twice is a client bug rather than
/// an ambiguity worth a resolution rule of its own.
#[must_use]
pub fn field(body: &str, name: &str) -> Option<String> {
    parse_form(body)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

/// Percent- and plus-decode one component. `None` if the result is not UTF-8.
fn decode(component: &str) -> Option<String> {
    if !component.contains(['%', '+']) {
        return Some(component.to_owned());
    }
    let bytes = component.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let hex = component.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    // `clippy::assert_is_empty` is new in clippy 1.99, which is the channel the
    // CI host gate runs on (`CC_RUST_TOOLCHAIN=stable`). It is silenced here, in
    // this `#[cfg(test)]` module, and nowhere else: the assertions below are on
    // COLLECTIONS, so the lint's suggestion (`assert_eq!(x, "")`) does not
    // typecheck, and `assert_eq!(x.len(), 0)` would print a count instead of
    // the contents. A `String` emptiness assertion elsewhere in this crate is
    // still linted -- those are written `assert_ne!(x, "")`.
    #![expect(
        clippy::assert_is_empty,
        reason = "the assertions are on collections, so the suggested \
                  assert_eq!(x, \"\") does not typecheck"
    )]
    use super::*;
    use alloc::format;
    use alloc::vec;

    /// `parse_form` returns owned pairs; the tests want to write `&str`
    /// literals, so compare through this.
    fn fields(body: &str) -> Vec<(&str, &str)> {
        parse_form(body)
            .into_iter()
            .map(|(k, v)| (leak(k), leak(v)))
            .collect()
    }

    fn leak(text: String) -> &'static str {
        alloc::boxed::Box::leak(text.into_boxed_str())
    }

    #[test]
    fn a_simple_field_round_trips() {
        assert_eq!(fields("value=94.5"), vec![("value", "94.5")]);
    }

    #[test]
    fn several_fields_are_all_parsed() {
        let body = "brew.setpoint=94.5&pid.enabled=1&steam.setpoint=120";
        assert_eq!(
            fields(body),
            vec![
                ("brew.setpoint", "94.5"),
                ("pid.enabled", "1"),
                ("steam.setpoint", "120")
            ]
        );
    }

    #[test]
    fn a_value_may_contain_an_equals_sign() {
        // Only the FIRST '=' separates. A base64 or a JWT-ish value would be
        // truncated at the second one otherwise.
        assert_eq!(fields("token=a=b=c"), vec![("token", "a=b=c")]);
    }

    #[test]
    fn a_plus_is_a_space() {
        assert_eq!(
            fields("name=brew+setpoint"),
            vec![("name", "brew setpoint")]
        );
    }

    #[test]
    fn percent_escapes_are_decoded() {
        assert_eq!(fields("msg=caf%C3%A9"), vec![("msg", "caf\u{e9}")]);
        assert_eq!(fields("a=%20b"), vec![("a", " b")]);
    }

    #[test]
    fn a_malformed_percent_escape_is_skipped_not_guessed() {
        // A truncated escape has no correct decoding. Emitting a replacement
        // character would let a mangled parameter name reach the lookup.
        assert!(fields("name=abc%").is_empty());
        assert!(fields("name=abc%ZZ").is_empty());
        assert!(fields("name=%2").is_empty());
    }

    #[test]
    fn a_name_with_no_value_is_an_empty_value() {
        // What a browser sends for a valueless input: the name is present, with
        // an empty value.
        assert_eq!(fields("flag"), vec![("flag", "")]);
    }

    #[test]
    fn a_lone_separator_is_skipped_rather_than_reported() {
        assert_eq!(fields("a=1&&b=2"), vec![("a", "1"), ("b", "2")]);
        assert_eq!(fields("&a=1&"), vec![("a", "1")]);
    }

    #[test]
    fn an_empty_body_is_no_fields() {
        assert!(fields("").is_empty());
    }

    #[test]
    fn field_finds_the_first_match() {
        let body = "value=1&value=2";
        assert_eq!(field(body, "value").as_deref(), Some("1"));
        assert!(field(body, "missing").is_none());
    }

    #[test]
    fn a_setpoint_of_zero_is_a_value_and_not_an_absence() {
        // The parser's contract, not the schema's: a field is present whether or
        // not its value is empty. Conflating `value=` with "no `value` field" is
        // how a zero becomes an absence.
        assert_eq!(field("value=0", "value").as_deref(), Some("0"));
        assert_eq!(field("value=", "value").as_deref(), Some(""));
    }

    #[test]
    fn a_paste_of_many_fields_parses_into_many_fields() {
        // The read limit is the *caller's* (`cc_net_esp32::web::drain_body`,
        // 256 bytes), and this asserts the parser is not a second limit: every
        // field in a 4 KB paste comes back, so a caller that forgets to bound
        // its read gets a big Vec rather than a truncated one. That is the
        // failure mode worth having -- visible and loud -- rather than a
        // silently short parameter list.
        let body: String = (0..512)
            .map(|i| format!("p{i}={i}"))
            .collect::<Vec<_>>()
            .join("&");
        assert!(body.len() > 4096);
        assert_eq!(fields(&body).len(), 512);
    }
}
