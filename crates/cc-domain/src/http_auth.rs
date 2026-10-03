//! HTTP Basic authentication: header parsing, base64, constant-time compare.
//!
//! Owner: the `/api/config/upload` + auth work on the Rust HTTP surface.
//!
//! # Why this is in `cc-domain` and not `cc-hal-esp32`
//!
//! **Because it is the only way to test it.** `cc-hal-esp32` names
//! `esp_idf_hal`, so it cannot be built for a host target and its `#[test]`
//! functions are executable *only* by flashing the device
//! (`crates/cc-hal-esp32/src/device_tests.rs`). A base64 decoder and a
//! credential comparison are pure string logic with no reason to need a
//! microcontroller, and shipping them untested would mean shipping the one
//! piece of this work that a mistake in silently opens or silently closes a
//! machine's web interface. Here they are `no_std`, allocation-free and covered
//! by `just test` on the host.
//!
//! # What the C++ does, and what is therefore the same here
//!
//! `WebServerManager::setupMiddleware` (`src/network/WebServerManager.cpp:272-296`)
//! installs `ESPAsyncWebServer`'s `AsyncAuthenticationMiddleware` when
//! `system.auth.enabled` is set, with the realm `"CleverCoffee"`:
//!
//! ```cpp
//! if (Config::getInstance().systemAuthEnabled.get()) {
//!     String username = Config::getInstance().systemAuthUsername.get();
//!     String password = Config::getInstance().systemAuthPassword.get();
//!     if (!username.isEmpty() && !password.isEmpty()) {
//!         authMiddleware_ = std::make_unique<AsyncAuthenticationMiddleware>();
//!         authMiddleware_->setRealm("CleverCoffee");
//!         server_->addMiddleware(authMiddleware_.get());
//!     } else {
//!         LOG(WARNING, "Web authentication enabled but credentials not set");
//!     }
//! }
//! ```
//!
//! Two properties of that block are reproduced rather than improved, and both
//! are decisions the caller has to know about:
//!
//! * **The middleware is installed once, at `WebServerManager::initialize`.**
//!   Enabling `system.auth.enabled` therefore protects nothing until the next
//!   boot. `cc_hal_esp32::web::needs_reboot` says so in the
//!   `POST /api/parameters` answer for the same reason.
//! * **Empty credentials mean no middleware at all** — the C++ logs a warning
//!   and serves the whole API unauthenticated. That is fail-open, and it is
//!   preserved because it is the C++'s behaviour and because the alternative
//!   (locking an operator out of their own machine because they enabled auth and
//!   then did not finish) is worse on a device whose only other console is a
//!   UART. It is called out in `docs/rust-migration/intentional-diffs.md`.
//!
//! # Why there is no ESP-IDF auth hook to use instead
//!
//! There is not one. `esp-idf-svc` 0.53.0's `src/http/server.rs` has no
//! `auth`, `realm` or `WWW-Authenticate` anywhere, and neither does ESP-IDF
//! v5.5.5's `components/esp_http_server` — `esp_http_server` has no
//! authentication concept at all, which is why the C++ needed a middleware at
//! all. So this is hand-rolled, and hand-rolled credential handling is exactly
//! the code that must be constant-time and must be tested.
//!
//! # What this deliberately does NOT do
//!
//! * **No `Authorization` header is ever logged.** The caller gets a `bool`.
//! * **No retry limiting, no lockout, no timing-safe *length*.** The compare is
//!   constant-time over the bytes it compares; the *lengths* are compared
//!   first, because a username length is not the secret and because a
//!   fixed-width compare would mean padding a credential to a size an attacker
//!   chooses.
//! * **No TLS, and no digest.** Basic auth over plain HTTP sends the credential
//!   base64-encoded, which is encoding, not encryption. That is the C++'s
//!   posture (there is no HTTPS listener in `WebServerManager` either) and it
//!   is why this is "the C++'s control, implemented", not "a secure interface".

/// The realm the C++ challenges with (`WebServerManager.cpp:286`).
///
/// The whole header rather than the realm, because the realm is a constant and
/// this crate is `no_std` + no `alloc`: a `fn realm() -> String` would be the
/// only allocation in the module, for a string that never varies.
pub const WWW_AUTHENTICATE: &str = "Basic realm=\"CleverCoffee\"";

/// How many bytes a decoded `user:password` may be.
///
/// A bound, because the decode target is a caller-supplied buffer and the
/// header length is attacker-controlled. 256 is far above any credential a
/// person sets (`MAX_TEXT_LEN` in `cc-config` is 4096, and the C++ checks no
/// length at all) and far below the point where a request could make the
/// httpd task do meaningful work — the decode is O(n) in the *header*, which
/// `CONFIG_HTTPD_MAX_URI_LEN`'s sibling, `CONFIG_HTTPD_MAX_HEADER_LEN`,
/// already bounds at 512.
pub const MAX_CREDENTIAL_BYTES: usize = 256;

/// Whether `header` carries credentials matching `username` and `password`.
///
/// `header` is the raw `Authorization` header value, or `None` when the request
/// carried none. `scratch` is the decode buffer; its length bounds the decoded
/// credential, so a caller that passes a short buffer rejects long credentials
/// rather than overflowing.
///
/// Returns `false` for every failure — absent header, wrong scheme, malformed
/// base64, no `:`, wrong username, wrong password — so a caller cannot
/// distinguish "you are wrong" from "you are not formatted correctly" by
/// timing the branch it takes afterwards.
///
/// # Examples
///
/// ```
/// use cc_domain::http_auth::authorized;
///
/// let mut scratch = [0u8; 64];
/// // base64("admin:admin")
/// assert!(authorized(Some("Basic YWRtaW46YWRtaW4="), "admin", "admin", &mut scratch));
/// assert!(!authorized(Some("Basic YWRtaW46YWRtaW4="), "admin", "wrong", &mut scratch));
/// assert!(!authorized(None, "admin", "admin", &mut scratch));
/// ```
#[must_use]
pub fn authorized(
    header: Option<&str>,
    username: &str,
    password: &str,
    scratch: &mut [u8],
) -> bool {
    let Some((user, pass)) = decode_basic(header, scratch) else {
        return false;
    };
    // Both compares always run. `&&` would short-circuit, and the time taken
    // would then say whether the *username* was right, which is half the
    // credential.
    let user_ok = constant_time_eq(user.as_bytes(), username.as_bytes());
    let pass_ok = constant_time_eq(pass.as_bytes(), password.as_bytes());
    user_ok & pass_ok
}

/// Split a decoded Basic credential at its first `:`.
///
/// `None` for a header that is not `Basic <base64>`, whose base64 is
/// malformed, or that carries no `:`. RFC 7617 §2 puts the colon at the *first*
/// one, so a password containing `:` survives.
///
/// Returns borrows into `scratch`, which is why the decode target is the
/// caller's and not a local: `no_std` + no `alloc` means there is nowhere else
/// for the bytes to land.
fn decode_basic<'a>(header: Option<&str>, scratch: &'a mut [u8]) -> Option<(&'a str, &'a str)> {
    // RFC 7235 §2.1: the scheme is case-insensitive. `ESPAsyncWebServer` is
    // case-sensitive, so accepting `basic` is a superset of the C++ — a client
    // that the C++ would reject is one that works here, never the reverse.
    let rest = header?.strip_prefix_ignore_ascii_case("Basic")?;
    let rest = rest.strip_prefix(' ')?;

    let len = base64_decode(rest.trim(), scratch)?;
    let decoded = scratch.get(..len)?;
    let text = core::str::from_utf8(decoded).ok()?;
    let colon = text.find(':')?;
    Some((&text[..colon], &text[colon + 1..]))
}

/// Decode standard base64 into `out`, returning the byte count.
///
/// `None` on any malformed input: a character outside the alphabet, a `=` in the
/// wrong place, a length that is not a multiple of four, or output that does not
/// fit `out`.
///
/// Whitespace is **not** skipped, unlike [`base64_decode`]'s usual
/// implementations. RFC 7617 §2 says the credentials are `base64(user-pass)`,
/// and every client that produces this header produces it without whitespace;
/// accepting it would mean accepting two encodings of one credential.
fn base64_decode(input: &str, out: &mut [u8]) -> Option<usize> {
    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut written = 0usize;
    for (chunk_index, chunk) in bytes.chunks(4).enumerate() {
        let is_last = chunk_index == bytes.len() / 4 - 1;
        // Left-aligned in 24 bits, the layout every base64 description uses:
        // four 6-bit symbols, most significant first. `=` contributes zero
        // rather than being absent, so the shifts below are the same for every
        // quantum and only the *count* of emitted bytes varies.
        let mut accumulator = 0u32;
        // How many bytes this quantum carries: 3 for four symbols, and one
        // fewer per `=`.
        let mut bytes = 3usize;
        for (i, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                // Padding is legal only in the final quantum's last two
                // positions, and everything after it must be padding too.
                if !is_last || i < 2 {
                    return None;
                }
                bytes = i - 1;
                if chunk[i + 1..].iter().any(|rest| *rest != b'=') {
                    return None;
                }
                break;
            }
            accumulator |= base64_value(*byte)? << (18 - i * 6);
        }
        for i in 0..bytes {
            let byte = ((accumulator >> (16 - i * 8)) & 0xff) as u8;
            *out.get_mut(written)? = byte;
            written += 1;
        }
    }
    Some(written)
}

/// The six-bit value of one base64 character, or `None` if it is not one.
///
/// `encode64` (`esp_http_server`'s own, and therefore the C++'s) is
/// `A-Za-z0-9+/`; RFC 4648 §4 also permits `-_` as the URL-safe alphabet, and
/// that is **not** accepted here. Accepting both would make `a+b` and `a-b` two
/// spellings of one credential, and the C++ accepts only the first.
const fn base64_value(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some((byte - b'A') as u32),
        b'a'..=b'z' => Some((byte - b'a') as u32 + 26),
        b'0'..=b'9' => Some((byte - b'0') as u32 + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Whether two byte strings are equal, in time that does not depend on *where*
/// they first differ.
///
/// The XOR of every pair is accumulated and only examined once at the end, so
/// the loop runs to completion for every input. `len_eq` short-circuits the
/// length comparison, because a length is visible in the header anyway and
/// pretending otherwise would only cost a fixed-size buffer per request.
///
/// This is the comparison a credential needs. `==` on `&[u8]` is memcmp, which
/// returns at the first differing byte, and the number of bytes it got through
/// is exactly the prefix length of the guess — which is how a byte-at-a-time
/// search recovers a password.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

/// `str::strip_prefix`, case-insensitively. `no_std`'s `str` has no such method.
trait StripPrefixIgnoreAsciiCase {
    /// The remainder after `prefix`, or `None`.
    fn strip_prefix_ignore_ascii_case(&self, prefix: &str) -> Option<&str>;
}

impl StripPrefixIgnoreAsciiCase for str {
    fn strip_prefix_ignore_ascii_case(&self, prefix: &str) -> Option<&str> {
        if self.len() >= prefix.len()
            && self.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
        {
            Some(&self[prefix.len()..])
        } else {
            None
        }
    }
}

/// Base64-encode, for a test that needs to *produce* a credential.
///
/// Not part of the firmware's surface: nothing here encodes, only decodes. It
/// exists because `cc-hal-esp32`'s authentication tests need to build an
/// `Authorization` header and **that crate's tests are executable only by
/// flashing a device**, so a copy of the encoder here is what lets them state a
/// credential rather than a transcribed base64 blob.
#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests_support {
    extern crate alloc;
    use alloc::string::String;

    /// Standard base64 (`encode64`, RFC 4648 §4).
    #[must_use]
    pub fn encode(plain: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in plain.chunks(3) {
            let b0 = u32::from(chunk[0]);
            let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
            let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
            let n = (b0 << 16) | (b1 << 8) | b2;
            for i in 0..=chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - i * 6)) & 63) as usize]));
            }
            for _ in 0..3 - chunk.len() {
                out.push('=');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::encode;
    use super::*;
    use alloc::format;
    use alloc::string::String;

    use alloc::vec::Vec;

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Base64-encode, the way a client does.
    ///
    /// Deliberately a **second** implementation rather than a call into
    /// [`tests_support::encode`]: the decoder is checked against an encoder
    /// written separately from it, so the two cannot be wrong in the same way and
    /// cancel out. `the_shared_encoder_agrees_with_this_reference_one` holds the
    /// two together, because `cc-hal-esp32`'s authentication tests build their
    /// headers with the shared one.
    fn encode_reference(plain: &[u8]) -> String {
        let mut out = String::new();
        for chunk in plain.chunks(3) {
            let b0 = u32::from(chunk[0]);
            let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
            let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
            let n = (b0 << 16) | (b1 << 8) | b2;
            // A chunk of n bytes carries n+1 data symbols and 4-(n+1) pads.
            for i in 0..=chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - i * 6)) & 63) as usize]));
            }
            for _ in 0..3 - chunk.len() {
                out.push('=');
            }
        }
        out
    }

    fn header(user: &str, pass: &str) -> String {
        let mut credential = String::from(user);
        credential.push(':');
        credential.push_str(pass);
        format!("Basic {}", encode(credential.as_bytes()))
    }

    // ---------------------------------------------------------- the happy path

    #[test]
    fn the_right_credentials_are_accepted() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(authorized(
            Some(&header("admin", "s3cret")),
            "admin",
            "s3cret",
            &mut scratch
        ));
    }

    #[test]
    fn the_default_credentials_from_the_schema_are_accepted() {
        // `cc-config`'s `SystemAuth::default()` is `admin`/`admin`
        // (`config.rs:1133`), which is what an operator gets before changing
        // anything. If this test's encoding of `admin:admin` were wrong, this
        // is the case that would notice.
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(authorized(
            Some("Basic YWRtaW46YWRtaW4="),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn the_scheme_is_case_insensitive() {
        // RFC 7235 §2.1. A superset of the C++, deliberately: see
        // `decode_basic`.
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(authorized(
            Some("basic YWRtaW46YWRtaW4="),
            "admin",
            "admin",
            &mut scratch
        ));
        assert!(authorized(
            Some("BASIC YWRtaW46YWRtaW4="),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn trailing_whitespace_after_the_scheme_is_tolerated() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(authorized(
            Some("Basic  YWRtaW46YWRtaW4= "),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    // ------------------------------------------------------------ the refusals

    #[test]
    fn an_absent_header_is_refused() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(!authorized(None, "admin", "admin", &mut scratch));
    }

    #[test]
    fn a_wrong_password_is_refused() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(!authorized(
            Some(&header("admin", "nope")),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn a_wrong_username_is_refused() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(!authorized(
            Some(&header("root", "admin")),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn a_username_that_is_a_prefix_of_the_real_one_is_refused() {
        // The case a naive `starts_with` would pass.
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(!authorized(
            Some(&header("adm", "admin")),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn an_empty_password_does_not_match_a_non_empty_one() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(!authorized(
            Some(&header("admin", "")),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn a_credential_with_no_colon_is_refused() {
        // base64("adminadmin")
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(!authorized(
            Some("Basic YWRtaW5hZG1pbg=="),
            "admin",
            "admin",
            &mut scratch
        ));
    }

    #[test]
    fn a_password_containing_a_colon_survives() {
        // RFC 7617 §2: the split is at the FIRST colon, so everything after it
        // is the password including any further colons.
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(authorized(
            Some(&header("admin", "a:b:c")),
            "admin",
            "a:b:c",
            &mut scratch
        ));
    }

    #[test]
    fn a_non_ascii_credential_round_trips() {
        // `String::from_utf8_lossy` on the decoded bytes would corrupt this and
        // could match a mangled configured password; the decode rejects
        // non-UTF-8 instead.
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        assert!(authorized(
            Some(&header("café", "naïve")),
            "café",
            "naïve",
            &mut scratch
        ));
    }

    #[test]
    fn a_malformed_scheme_is_refused() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        for bad in [
            "",
            "YWRtaW46YWRtaW4=",
            "Bearer YWRtaW46YWRtaW4=",
            "Basic",
            "BasicYWRtaW46YWRtaW4=",
            "Digest username=\"admin\"",
        ] {
            assert!(
                !authorized(Some(bad), "admin", "admin", &mut scratch),
                "{bad:?} must not authenticate"
            );
        }
    }

    #[test]
    fn malformed_base64_is_refused() {
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        for bad in [
            "YWJ",   // length is not a multiple of four
            "YWJj=", // five characters
            "YW=Jj", // padding in the middle
            "====",  // nothing but padding
            "YWJ$",  // `$` is not in the alphabet
            "YW Jj", // whitespace is not skipped
            "-_-_",  // the URL-safe alphabet is deliberately not accepted
        ] {
            assert!(
                !authorized(
                    Some(&format!("Basic {bad}")),
                    "admin",
                    "admin",
                    &mut scratch
                ),
                "{bad:?} must not authenticate"
            );
        }
    }

    #[test]
    fn a_credential_longer_than_the_buffer_is_refused_and_does_not_overflow() {
        // The header is attacker-controlled and `out` is the caller's, so this
        // is the boundary that matters: a long credential is a refusal, never a
        // write past the end.
        let long = "x".repeat(400);
        let mut scratch = [0u8; 16];
        assert!(!authorized(
            Some(&header("admin", &long)),
            "admin",
            &long,
            &mut scratch
        ));
    }

    #[test]
    fn an_empty_buffer_refuses_even_a_one_byte_credential() {
        let mut scratch = [];
        assert!(!authorized(Some("Basic QQ=="), "a", "", &mut scratch));
    }

    // --------------------------------------------------- constant-time compare

    #[test]
    fn the_compare_agrees_with_equality_on_every_length() {
        for len in 0..24usize {
            let left = "a".repeat(len);
            assert!(constant_time_eq(left.as_bytes(), left.as_bytes()));
            for differ_at in 0..len {
                let mut right = left.clone().into_bytes();
                right[differ_at] = b'b';
                assert!(
                    !constant_time_eq(left.as_bytes(), &right),
                    "len {len}, differing at {differ_at}"
                );
            }
            let longer = "a".repeat(len + 1);
            assert!(!constant_time_eq(left.as_bytes(), longer.as_bytes()));
        }
    }

    // -------------------------------------------------------------- base64 unit

    #[test]
    fn base64_decodes_every_padding_length() {
        // The three cases that a hand-rolled decoder gets wrong.
        let cases: [(&str, &str); 6] = [
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ];
        for (encoded, plain) in cases {
            let mut out = [0u8; 16];
            assert_eq!(
                base64_decode(encoded, &mut out),
                Some(plain.len()),
                "{encoded}"
            );
            assert_eq!(&out[..plain.len()], plain.as_bytes(), "{encoded}");
        }
    }

    #[test]
    fn base64_agrees_with_a_full_alphabet_round_trip() {
        // Every byte value, through both directions, so a wrong constant in
        // `base64_value` cannot hide behind the hand-written cases above. Both
        // chunk lengths that produce padding are included, and 256 is not a
        // multiple of three so the final quantum is the two-character one.
        for len in 0..=256usize {
            let plain: Vec<u8> = (0..len)
                .map(|i| u8::try_from(i % 256).unwrap_or(0))
                .collect();
            let encoded = encode_reference(&plain);
            assert_eq!(encoded.len() % 4, 0, "len {len}");
            let mut decoded = [0u8; 256];
            assert_eq!(
                base64_decode(&encoded, &mut decoded),
                Some(len),
                "len {len}"
            );
            assert_eq!(&decoded[..len], &plain[..], "len {len}");
        }
    }

    #[test]
    fn base64_encodes_what_rfc_4648_says_it_does() {
        // The test vectors from RFC 4648 §10, which is the specification the
        // decoder implements. `encode` is checked against them so that the
        // round-trip above cannot pass by both halves being wrong together.
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_reference(plain.as_bytes()), encoded, "{plain:?}");
        }
    }

    #[test]
    fn the_shared_encoder_agrees_with_this_reference_one() {
        // `cc-hal-esp32`'s authentication tests build their `Authorization`
        // headers with `tests_support::encode` and run them on a device, so the
        // two encoders must produce identical bytes -- otherwise those tests
        // would be asserting against something other than what a client sends.
        for len in 0..=64usize {
            let plain: Vec<u8> = (0..len)
                .map(|i| u8::try_from(i * 3 % 256).unwrap_or(0))
                .collect();
            assert_eq!(encode(&plain), encode_reference(&plain), "len {len}");
        }
    }

    #[test]
    fn the_challenge_is_the_cpp_realm() {
        // `WebServerManager.cpp:286` — `authMiddleware_->setRealm("CleverCoffee")`.
        assert_eq!(WWW_AUTHENTICATE, "Basic realm=\"CleverCoffee\"");
    }
}
