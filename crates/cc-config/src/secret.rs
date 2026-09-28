//! A string that refuses to print itself.
//!
//! Skill rule 7: "Never put a credential anywhere it can leak. Not in source,
//! not in a command-line argument, not in a log, not in a commit, not in an
//! unencrypted example file." The cheapest place to enforce that is the type:
//! if a password's `Debug` and `Display` print `[redacted]`, then it cannot be
//! logged by accident, no matter how the log statement is written.
//!
//! `Secret<T>` is **transparent to serialisation** — it writes and reads the
//! inner value unchanged, so a `Config` containing secrets round-trips through
//! the store and the web API exactly as the plain values would. That is
//! deliberate and is the one thing this type does *not* protect against: the
//! stored blob and the `/api/config/download` response contain the plaintext
//! credentials, because the machine has to be able to use them. Redaction is
//! for *diagnostics*, not for storage.
//!
//! ```
//! extern crate alloc;
//! use alloc::{format, string::String};
//! use cc_config::Secret;
//!
//! let wifi_password = Secret::new(String::from("hunter2"));
//! assert_eq!(format!("{wifi_password:?}"), "[redacted]");
//! assert_eq!(format!("{wifi_password}"), "[redacted]");
//! assert!(!format!("{wifi_password:?}{wifi_password}").contains("hunter2"));
//! // …and the real value is still reachable, on purpose.
//! assert_eq!(wifi_password.expose(), "hunter2");
//! ```

use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The text every redacted value renders as.
pub const REDACTED: &str = "[redacted]";

/// A value that must not appear in diagnostics.
///
/// See the module documentation for what this does and does not protect.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wrap a value.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Read the value.
    ///
    /// Named `expose` rather than `get` or `value` so that every read is
    /// greppable. A reviewer can then find every place a credential leaves the
    /// `Secret`, and there are only a handful.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Read the value for the Wi-Fi / MQTT / HTTP client that needs it.
    pub fn expose_mut(&mut self) -> &mut T {
        &mut self.0
    }

    /// Replace the value.
    pub fn set(&mut self, value: T) {
        self.0 = value;
    }

    /// Discard the value and take the inner type.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Apply a function to the inner value.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Secret<U> {
        Secret(f(self.0))
    }
}

/// Always `[redacted]`, whatever `T` is.
impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Always `[redacted]`, whatever `T` is.
impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Transparent: the inner value is written as-is.
impl<T: Serialize> Serialize for Secret<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Transparent: the inner value is read as-is.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Secret<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    // `#![no_std]` means the prelude has no `format!`, `String` or `Vec`.
    use alloc::{format, string::String};

    use super::*;

    #[test]
    fn debug_and_display_are_redacted() {
        let s = Secret::new(String::from("hunter2"));
        assert_eq!(format!("{s:?}"), REDACTED);
        assert_eq!(format!("{s}"), REDACTED);
    }

    #[test]
    fn neither_debug_nor_display_contains_the_plaintext() {
        let s = Secret::new(String::from("hunter2"));
        let rendered = format!("{s:?} {s}");
        assert!(!rendered.contains("hunter2"), "leaked: {rendered}");
    }

    #[test]
    fn nested_debug_does_not_reach_through() {
        // A derived `Debug` on a containing struct prints the field, and the
        // field's `Debug` is the redacted one. This is the property that makes
        // `#[derive(Debug)]` on `Config` safe.
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Wrapper {
            user: String,
            password: Secret<String>,
        }
        let w = Wrapper {
            user: String::from("admin"),
            password: Secret::new(String::from("hunter2")),
        };
        let rendered = format!("{w:?}");
        assert!(rendered.contains("admin"), "non-secrets must stay visible");
        assert!(!rendered.contains("hunter2"), "leaked: {rendered}");
    }

    #[test]
    fn serialisation_is_transparent() {
        let s = Secret::new(String::from("hunter2"));
        let json = serde_json::to_string(&s).unwrap_or_default();
        assert_eq!(json, "\"hunter2\"");
        let back: Secret<String> = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back.expose(), "hunter2");
    }
}
