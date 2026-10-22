//! Re-export [`cc_domain::secret::Secret`] with transparent serialization enabled.
//!
//! Diagnostics redact credentials; storage and config downloads retain their values.
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

pub use cc_domain::secret::{Secret, REDACTED};

#[cfg(test)]
mod tests {
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
        // This is the one property the re-export does NOT inherit for free: the
        // `serde` feature of `cc-domain` has to be on, or the config blob would
        // not round-trip its four credential fields.
        let s = Secret::new(String::from("hunter2"));
        let json = serde_json::to_string(&s).unwrap_or_default();
        assert_eq!(json, "\"hunter2\"");
        let back: Secret<String> = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back.expose(), "hunter2");
    }
}
