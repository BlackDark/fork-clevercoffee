//! The host side of the CleverCoffee USB provisioning channel.
//!
//! The protocol is line-oriented and human-readable, so a device that is refusing to accept
//! credentials can be diagnosed by hand from a terminal with nothing but `picocom`. That was a
//! deliberate choice when the protocol was designed, and it is why it is kept: an operator
//! migrating a machine off the C++ firmware is doing this once, possibly at the kitchen counter,
//! and a binary protocol would mean a second tool for the one moment that most needs help.
//!
//! # What never happens here
//!
//! A credential is read from `.env`, written to the port, and dropped. It is never printed, never
//! logged, never included in an error message, and never written to disk by this tool. The only
//! thing that reaches stdout is a status code and the device's own non-secret reply. That is not a
//! convention, it is why [`Link`] returns lines rather than exposing the writer, and why
//! [`Session`] has no method that returns a credential.

use std::fmt;
use std::path::Path;

pub mod chunk;
pub mod dotenv;
pub mod link;
pub mod session;

pub use chunk::{split, CHUNK_BYTES};
pub use dotenv::read_env;
pub use link::SerialLink;
pub use link::{Link, LinkError};
pub use session::{Outcome, Session};

/// The prompt the device prints when it is ready for a command.
pub const PROMPT: &str = "provision> ";

/// How long to wait for a device reply before giving up.
///
/// A device that has just booted on the original ESP32 can take several seconds to reach its
/// prompt. Ten seconds is generous rather than tight, because the cost of a false timeout is a
/// confusing failure and the cost of a long wait is a user who reads a status line.
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// A status line, which is the tool's entire output contract.
///
/// Both variants carry only non-secret text. There is deliberately no variant that can hold a
/// credential, so adding one would be a visible change rather than an easy slip.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Ok { code: &'static str, detail: String },
    Fail { code: &'static str, detail: String },
}

impl Status {
    pub fn is_ok(&self) -> bool {
        matches!(self, Status::Ok { .. })
    }

    /// The process exit code: success only for [`Status::Ok`].
    pub fn exit_code(&self) -> std::process::ExitCode {
        if self.is_ok() {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }

    /// The line to print: `OK <code> <detail>` or `FAIL <code> <detail>`.
    ///
    /// Always exactly one line. Whitespace in the detail is collapsed rather than passed through,
    /// because a detail can carry text the device chose, and a newline in it would let a device
    /// print a second line that a script reading this tool's output would parse as a success.
    pub fn line(&self) -> String {
        let (prefix, code, detail) = match self {
            Status::Ok { code, detail } => ("OK", code, detail),
            Status::Fail { code, detail } => ("FAIL", code, detail),
        };
        let detail = detail.split_whitespace().collect::<Vec<_>>().join(" ");
        if detail.is_empty() {
            format!("{prefix} {code}")
        } else {
            format!("{prefix} {code} {detail}")
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.line())
    }
}

pub fn ok(code: &'static str, detail: &str) -> Status {
    Status::Ok {
        code,
        detail: detail.to_string(),
    }
}

pub fn fail(code: &'static str, detail: &str) -> Status {
    Status::Fail {
        code,
        detail: detail.to_string(),
    }
}

/// Reads `WIFI_SSID` and `WIFI_PASS` out of a dotenv file.
///
/// Missing keys produce an empty string rather than an error: a device on an open network needs
/// only the SSID, and refusing to proceed without a password would block that case.
pub fn credentials(path: &Path) -> (String, String) {
    let env = read_env(path);
    let get = |key: &str| {
        env.iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    (get("WIFI_SSID"), get("WIFI_PASS"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ok_status_prints_one_line_and_exits_zero() {
        let s = ok("PING", "device answered");
        assert_eq!(s.line(), "OK PING device answered");
        assert!(s.is_ok());
        assert_eq!(s.exit_code(), std::process::ExitCode::SUCCESS);
    }

    #[test]
    fn a_fail_status_exits_non_zero() {
        let s = fail("OPEN_PORT", "cannot open /dev/nope");
        assert_eq!(s.line(), "FAIL OPEN_PORT cannot open /dev/nope");
        assert!(!s.is_ok());
        assert_eq!(s.exit_code(), std::process::ExitCode::FAILURE);
    }

    #[test]
    fn an_empty_detail_still_produces_a_parseable_line() {
        assert_eq!(ok("STATUS", "").line(), "OK STATUS");
        assert_eq!(fail("STATUS", "").line(), "FAIL STATUS");
    }

    #[test]
    fn a_device_supplied_detail_cannot_forge_a_second_status_line() {
        // The detail can carry text the device chose. A newline in it would make this tool print
        // two lines, and a script reading its output would take the second as a success.
        for forged in [
            "line one\nOK WIFI_SET",
            "line one\r\nOK WIFI_SET",
            "\nOK STATUS",
        ] {
            let s = fail("STATUS", forged);
            let line = s.line();
            assert_eq!(line.matches('\n').count(), 0, "{line:?}");
            assert_eq!(line.matches('\r').count(), 0, "{line:?}");
            assert!(line.starts_with("FAIL STATUS "), "{line:?}");
        }
    }

    #[test]
    fn credentials_are_read_from_a_dotenv_file() {
        let dir = std::env::temp_dir().join("provision-credentials-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        // A placeholder, so the repository's secret scanner does not flag the fixture.
        std::fs::write(
            &path,
            "WIFI_SSID=example-network\nWIFI_PASS=placeholder-value\n",
        )
        .unwrap();
        let (ssid, pass) = credentials(&path);
        assert_eq!(ssid, "example-network");
        assert_eq!(pass, "placeholder-value");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_file_yields_no_credentials_rather_than_an_error() {
        let (ssid, pass) = credentials(Path::new("/nonexistent/provision-test/.env"));
        assert!(ssid.is_empty());
        assert!(pass.is_empty());
    }
}
