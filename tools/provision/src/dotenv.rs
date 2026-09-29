//! A dotenv reader.
//!
//! Deliberately small: no variable expansion, no `export` prefix handling beyond the common case,
//! no multiline values, no shell quoting rules. The file holds two values that a person typed, and
//! a parser that interprets escapes would silently rewrite a password containing a backslash,
//! which is exactly the class of bug that leaves a user unable to join their own network and
//! unable to see why.
//!
//! What it does handle is the quoting people actually write by hand, because `WIFI_PASS="a b"` is
//! what a password with a space looks like.

use std::collections::BTreeMap;
use std::path::Path;

/// Reads `KEY=VALUE` pairs.
///
/// Later keys win, so a file that sets a value twice takes the last one, which is what a person
/// editing the bottom of the file expects.
pub fn read_env(path: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    parse(&text)
}

/// Parses dotenv text. Split out from the file read so the rules are testable without a filesystem.
pub fn parse(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        // Blank lines and comments. A comment marker inside a value is not a comment, which is why
        // the check is on the first non-space character rather than a search.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // An `export ` prefix is common enough in a shell-flavoured file to accept.
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        out.insert(key.to_string(), unquote(value.trim()));
    }
    out
}

/// Strips one layer of matching quotes.
///
/// Only matching pairs. `"a'` is left alone rather than having one character removed, because a
/// half-quoted password is a typo and silently repairing it is how the wrong password gets stored.
fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_pair_is_read() {
        let env = parse("WIFI_SSID=example-network\n");
        assert_eq!(
            env.get("WIFI_SSID").map(String::as_str),
            Some("example-network")
        );
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let env = parse(
            "# a comment\n\n   \nWIFI_SSID=example-network\n# WIFI_PASS=placeholder-commented-out",
        );
        assert_eq!(env.len(), 1);
        assert_eq!(
            env.get("WIFI_SSID").map(String::as_str),
            Some("example-network")
        );
    }

    #[test]
    fn a_hash_inside_a_value_is_not_a_comment() {
        // A password containing a hash is a real password, not a truncated one.
        let env = parse("WIFI_PASS=placeholder#value");
        assert_eq!(
            env.get("WIFI_PASS").map(String::as_str),
            Some("placeholder#value")
        );
    }

    #[test]
    fn matching_quotes_are_stripped_so_a_password_may_contain_spaces() {
        let env = parse("WIFI_SSID=\"example network\"\nWIFI_PASS='hunter 2'");
        assert_eq!(
            env.get("WIFI_SSID").map(String::as_str),
            Some("example network")
        );
        assert_eq!(env.get("WIFI_PASS").map(String::as_str), Some("hunter 2"));
    }

    #[test]
    fn half_quotes_are_left_alone() {
        // Repairing a typo would store the wrong password, which is worse than storing the typo.
        let env = parse("WIFI_PASS=\"placeholder-value");
        assert_eq!(
            env.get("WIFI_PASS").map(String::as_str),
            Some("\"placeholder-value")
        );
        let env = parse("WIFI_PASS=placeholder-value'");
        assert_eq!(
            env.get("WIFI_PASS").map(String::as_str),
            Some("placeholder-value'")
        );
    }

    #[test]
    fn an_empty_value_is_kept_rather_than_dropped() {
        // An open network has an empty password, and the difference between "absent" and "empty"
        // is the difference between a clear error and a confusing one.
        let env = parse("WIFI_PASS=");
        assert_eq!(env.get("WIFI_PASS").map(String::as_str), Some(""));
    }

    #[test]
    fn a_value_may_contain_equals_signs() {
        let env = parse("MQTT_TOPIC=a=b=c");
        assert_eq!(env.get("MQTT_TOPIC").map(String::as_str), Some("a=b=c"));
    }

    #[test]
    fn an_export_prefix_is_accepted() {
        let env = parse("export WIFI_SSID=example-network");
        assert_eq!(
            env.get("WIFI_SSID").map(String::as_str),
            Some("example-network")
        );
    }

    #[test]
    fn a_line_with_no_equals_sign_is_skipped() {
        let env = parse("WIFI_SSID=example-network\ngarbage\nWIFI_PASS=placeholder-value");
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn a_later_value_wins() {
        // A person editing the bottom of the file expects their edit to take effect.
        let env = parse("WIFI_SSID=old\nWIFI_SSID=new");
        assert_eq!(env.get("WIFI_SSID").map(String::as_str), Some("new"));
    }

    #[test]
    fn backslashes_are_not_interpreted() {
        // Escape processing would silently rewrite a Windows-style password.
        let env = parse(r"WIFI_PASS=a\nb");
        assert_eq!(env.get("WIFI_PASS").map(String::as_str), Some(r"a\nb"));
    }

    #[test]
    fn a_missing_file_is_an_empty_map_not_a_panic() {
        assert!(read_env(Path::new("/nonexistent/provision-test/.env")).is_empty());
    }

    #[test]
    fn a_crlf_file_parses() {
        // A file edited on Windows is the common case, not an edge case.
        let env = parse("WIFI_SSID=example-network\r\nWIFI_PASS=placeholder-value\r\n");
        assert_eq!(
            env.get("WIFI_PASS").map(String::as_str),
            Some("placeholder-value")
        );
    }
}
