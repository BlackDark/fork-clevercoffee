//! A value that refuses to print itself.
//!
//! Skill rule 7: "Never put a credential anywhere it can leak. Not in source,
//! not in a command-line argument, not in a log, not in a commit, not in an
//! unencrypted example file." The cheapest place to enforce that is the type:
//! if a password's `Debug` and `Display` print `[redacted]`, then it cannot be
//! logged by accident, no matter how the log statement is written.
//!
//! # Why this lives in `cc-domain` and not in `cc-config`
//!
//! It started in `cc-config`, which is where the first four credential-bearing
//! fields live (`system.wifi.ssid`, `system.wifi.password`, `mqtt.password`,
//! `system.auth.password`). R3-12 added a fifth user: the UART provisioning
//! parser in [`crate::provisioning`], which has to hand a password to the Wi-Fi
//! stack and must not be printable in the process. `cc-domain` is the crate
//! both depend on, and it is the one that can be read on its own, which is the
//! property that makes redaction auditable.
//!
//! `cc_config::Secret` is a re-export of this type, so the configuration
//! crate's public API is unchanged.
//!
//! # `serde` is behind a feature, and off by default
//!
//! `Secret<T>` is **transparent to serialisation** — it writes and reads the
//! inner value unchanged, so a `Config` containing secrets round-trips through
//! the store and the web API exactly as the plain values would. That is
//! deliberate and is the one thing this type does *not* protect against: the
//! stored blob and the `/api/config/download` response contain the plaintext
//! credentials, because the machine has to be able to use them. Redaction is
//! for *diagnostics*, not for storage.
//!
//! The `Serialize`/`Deserialize` impls need `serde`, and `cc-domain` has no
//! dependencies — a crate with no dependencies is a crate a reviewer can be
//! certain nothing else influences. So the impls are behind the off-by-default
//! `serde` feature and `cc-config`, which already depends on `serde`, turns it
//! on. The provisioning parser needs nothing but `new` and `expose`, so the
//! default build of `cc-domain` stays dependency-free.
//!
//! ```
//! extern crate alloc;
//! use alloc::{format, string::String};
//! use cc_domain::secret::Secret;
//!
//! let wifi_password = Secret::new(String::from("hunter2"));
//! assert_eq!(format!("{wifi_password:?}"), "[redacted]");
//! assert_eq!(format!("{wifi_password}"), "[redacted]");
//! assert!(!format!("{wifi_password:?}{wifi_password}").contains("hunter2"));
//! // …and the real value is still reachable, on purpose.
//! assert_eq!(wifi_password.expose(), "hunter2");
//! ```

use core::fmt;

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

    /// Whether the wrapped value is empty.
    ///
    /// For text credentials an empty SSID means "not configured" and an empty
    /// password means "open network", so callers need to ask without unwrapping
    /// into a form a log line could reach.
    pub fn is_empty(&self) -> bool
    where
        T: AsRef<str>,
    {
        self.0.as_ref().is_empty()
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
#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for Secret<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Transparent: the inner value is read as-is.
#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Secret<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}

#[cfg(all(test, not(feature = "serde")))]
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
    fn an_empty_credential_is_reported_as_empty() {
        // The Wi-Fi and MQTT code asks "is this configured?" on every boot, and
        // must be able to ask without a value that a log could reach.
        assert!(Secret::new(String::new()).is_empty());
        assert!(!Secret::new(String::from("x")).is_empty());
    }
}
