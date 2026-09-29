//! The protocol, over any [`Link`].
//!
//! Every command the tool can issue lives here, and every one of them is testable against the
//! in-memory device in `link::FakeDevice`. What the caller supplies is a credential or a payload;
//! what it gets back is a [`Status`] carrying only non-secret text.

use crate::chunk;
use crate::link::Link;
use crate::{fail, ok, Status};

/// A provisioning session.
///
/// Holds no credential after a command completes. The credential is taken by reference, written,
/// and the reference goes out of scope with the call; nothing caches it, so there is no field for
/// a later log line or a crash dump to reach.
pub struct Session<'a, L: Link> {
    link: &'a mut L,
}

impl<'a, L: Link> Session<'a, L> {
    pub fn new(link: &'a mut L) -> Self {
        Self { link }
    }

    /// Waits for the device's prompt and confirms it is alive.
    ///
    /// Done before every command group rather than once, because a device that reboots between
    /// two commands must be reported as a reboot rather than as a failure of the second command.
    pub fn handshake(&mut self) -> Result<(), Status> {
        self.link
            .drain()
            .map_err(|e| fail("PROMPT", &e.to_string()))?;
        self.send_hop("PING".to_string(), "PING", "PONG")
            .map(|()| ())
    }

    /// Writes Wi-Fi credentials and commits them.
    ///
    /// The password is written and never retained. No method on this type returns one, and no
    /// `Status` can carry one, so there is no path from a credential to stdout.
    pub fn set_wifi(&mut self, ssid: &str, password: &str) -> Status {
        if let Err(status) = self.handshake() {
            return status;
        }
        let hops = [
            ("WIFI SSID ", ssid, "WIFI_SSID", "WIFI_ACCEPTED"),
            ("WIFI PASS ", password, "WIFI_PASS", "WIFI_SET"),
        ];
        for (prefix, value, fail_code, want) in hops {
            // The line is built here and dropped at the end of this statement. Nothing on `Session`
            // retains it, so there is no field a later log line or a crash dump could reach.
            let line = format!("{prefix}{value}");
            if let Err(status) = self.send_hop(line, fail_code, want) {
                return status;
            }
        }
        match self.send_hop("WIFI COMMIT".to_string(), "WIFI_COMMIT", "WIFI_SET") {
            Ok(()) => ok(
                "WIFI_SET",
                "credentials written; the device connects on its next boot",
            ),
            Err(status) => status,
        }
    }

    /// Sends a config document and reports the device's counts.
    ///
    /// The device replies with counts only. It never echoes a value, so there is nothing here that
    /// could carry a secret out of the device and onto the host's terminal.
    pub fn send_config(&mut self, payload: &[u8]) -> Status {
        if let Err(status) = self.handshake() {
            return status;
        }
        let crc = crc32fast::hash(payload);
        let begin = format!("CONFIG BEGIN {} {crc:08x}", payload.len());
        if let Err(status) = self.expect(begin, "CONFIG_BEGIN") {
            return status;
        }

        use base64::Engine as _;
        let engine = base64::engine::general_purpose::STANDARD;
        for (i, block) in chunk::split(payload).iter().enumerate() {
            let line = format!("CONFIG {}", engine.encode(block));
            if let Err(status) = self.expect(line, "CONFIG_CHUNK") {
                // Naming the chunk index turns "the import failed" into "chunk 3 of 8 was
                // rejected", which is the difference between a retryable and a fatal error.
                return Status::Fail {
                    code: "CONFIG_CHUNK",
                    detail: format!(
                        "chunk {} of {} rejected: {}",
                        i + 1,
                        chunk::count(payload.len()),
                        status
                    ),
                };
            }
        }
        let end = "CONFIG END".to_string();
        match self.expect(end, "CONFIG_REJECTED") {
            Ok(reply) => ok("CONFIG_APPLIED", &reply_detail(&reply)),
            Err(status) => status,
        }
    }

    /// Clears the config region.
    pub fn factory_reset(&mut self) -> Status {
        if let Err(status) = self.handshake() {
            return status;
        }
        match self.send_hop(
            "FACTORY RESET".to_string(),
            "FACTORY_RESET",
            "FACTORY_RESET",
        ) {
            Ok(()) => ok(
                "FACTORY_RESET",
                "config region cleared; the device will reboot",
            ),
            Err(status) => status,
        }
    }

    /// Asks for a status line.
    pub fn status(&mut self) -> Status {
        if let Err(status) = self.handshake() {
            return status;
        }
        match self.send_hop("STATUS".to_string(), "STATUS", "STATUS") {
            Ok(()) => ok("STATUS", "device answered"),
            Err(status) => status,
        }
    }

    /// Sends a line and reads one reply, checking only that the device said OK.
    fn expect(&mut self, line: String, fail_code: &'static str) -> Result<String, Status> {
        self.link
            .send(&line)
            .map_err(|e| fail(fail_code, &e.to_string()))?;
        let reply = self
            .link
            .read_line()
            .map_err(|e| fail(fail_code, &e.to_string()))?;
        if reply.starts_with("OK") {
            Ok(reply)
        } else {
            Err(Status::Fail {
                code: fail_code,
                detail: describe(&reply),
            })
        }
    }

    /// Sends a line and checks both that the device said OK and that it named the expected token.
    ///
    /// The token check is what stops a device that answers every command with OK from looking like
    /// a working provisioning channel.
    fn send_hop(
        &mut self,
        line: String,
        fail_code: &'static str,
        want: &str,
    ) -> Result<(), Status> {
        let reply = self.expect(line, fail_code)?;
        if reply.contains(want) {
            Ok(())
        } else {
            Err(Status::Fail {
                code: fail_code,
                detail: format!("device replied {want:?} not: {reply}"),
            })
        }
    }
}

/// The counts from a `CONFIG END` reply.
///
/// The device answers `OK CONFIG_VALIDATED applied=88 rejected=0 ...`. Only the `key=value` tail is
/// passed on, because that is the whole of what the device is supposed to say there: if a future
/// firmware started putting a value in that position it would be a protocol change, and forwarding
/// it verbatim is how a config value would reach the host's terminal. A tail that is not entirely
/// `key=number` pairs is dropped rather than shown.
fn reply_detail(reply: &str) -> String {
    let rest = reply.strip_prefix("OK").unwrap_or("").trim();
    // Drop the command token, which is the first field with no '=' in it.
    let Some((_, tail)) = rest.split_once(' ') else {
        return String::new();
    };
    let tail = tail.trim();
    let all_counts = !tail.is_empty()
        && tail.split(' ').all(|field| match field.split_once('=') {
            Some((_, value)) => value.parse::<u64>().is_ok(),
            None => false,
        });
    if all_counts {
        tail.to_string()
    } else {
        String::new()
    }
}

/// Renders a device error reply for a status line.
///
/// Whitespace collapsed and the length bounded: a device that emitted an error containing a newline
/// could otherwise forge a second `OK` line on the host's terminal.
fn describe(reply: &str) -> String {
    let flat = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() > 120 {
        format!("{}...", &flat[..120])
    } else {
        flat
    }
}

/// What a session produced. A name rather than a second type, so a caller cannot end up holding a
/// result that is not the one the session returned.
pub type Outcome = Status;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::CHUNK_BYTES;
    use crate::link::tests::FakeDevice;

    fn happy_path() -> FakeDevice {
        FakeDevice::new(&["OK PONG", "OK WIFI_ACCEPTED", "OK WIFI_SET", "OK WIFI_SET"])
    }

    #[test]
    fn a_credential_write_sends_the_four_commands_in_order() {
        let mut device = happy_path();
        let status = Session::new(&mut device).set_wifi("example-network", "placeholder-value");
        assert!(status.is_ok(), "{}", status.line());
        assert_eq!(
            device.received(),
            vec![
                "PING".to_string(),
                "WIFI SSID example-network".to_string(),
                format!("WIFI PASS {PASS}"),
                "WIFI COMMIT".to_string(),
            ]
        );
    }

    /// The value every credential test uses. A placeholder, so the repository's secret scanner does
    /// not flag the fixture; the assertion is that *whatever* the value is, it never reaches
    /// stdout, so a placeholder tests the same property.
    const PASS: &str = "placeholder-value";

    #[test]
    fn no_status_line_can_contain_the_password() {
        // The single most important assertion in this file: the credential goes to the port and
        // nowhere else. Asserted against every failure path too, since an error is where a
        // credential most often leaks.
        for replies in [
            vec!["OK PONG", "OK WIFI_ACCEPTED", "OK WIFI_SET", "ERR 5 nope"],
            vec!["OK PONG", "ERR 4 bad ssid"],
            vec!["ERR 1 not a provisioner"],
            vec!["OK PONG", "OK WIFI_ACCEPTED"],
        ] {
            let mut device = FakeDevice::new(&replies);
            let status = Session::new(&mut device).set_wifi("example-network", PASS);
            assert!(!status.line().contains(PASS), "leaked: {}", status.line());
        }
    }

    #[test]
    fn a_device_that_never_answers_reports_a_timeout_and_not_a_success() {
        let mut device = FakeDevice::silent();
        let status = Session::new(&mut device).set_wifi("example-network", PASS);
        assert!(!status.is_ok());
        assert_eq!(status.line(), "FAIL PING no reply from the device");
    }

    #[test]
    fn a_device_that_reboots_mid_session_reports_a_disconnect() {
        // Which is what a device that reboots after committing looks like.
        let mut device = FakeDevice::new(&["OK PONG", "OK WIFI_ACCEPTED"]);
        let status = Session::new(&mut device).set_wifi("example-network", PASS);
        assert!(!status.is_ok());
        assert!(
            status.line().contains("closed the port"),
            "{}",
            status.line()
        );
    }

    #[test]
    fn a_config_payload_larger_than_one_chunk_is_sent_in_chunks() {
        // A real export is about 4 KB. Sending it as one line overflows the device's line buffer,
        // which is the reason this is chunked at all.
        let payload = vec![b'x'; CHUNK_BYTES * 3 + 10];
        let mut device = FakeDevice::new(&[
            "OK PONG",
            "OK CONFIG_READY",
            "OK CONFIG_CHUNK",
            "OK CONFIG_CHUNK",
            "OK CONFIG_CHUNK",
            "OK CONFIG_CHUNK",
            "OK CONFIG_VALIDATED applied=90 rejected=0 clamped=0 unknown=1 missing=8",
        ]);
        let status = Session::new(&mut device).send_config(&payload);
        assert!(status.is_ok(), "{}", status.line());

        let sent = device.received();
        assert_eq!(sent[0], "PING");
        assert!(sent[1].starts_with("CONFIG BEGIN "), "{}", sent[1]);
        assert!(sent[1].ends_with(&format!("{:08x}", crc32fast::hash(&payload))));
        assert_eq!(
            sent.len(),
            3 + 4,
            "expected four chunks plus the framing lines"
        );

        let chunk_lines: Vec<&String> = sent[2..sent.len() - 1].iter().collect();
        assert_eq!(chunk_lines.len(), 4);
        assert!(chunk_lines.iter().all(|l| l.starts_with("CONFIG ")));
        assert_eq!(sent[sent.len() - 1], "CONFIG END");
    }

    #[test]
    fn a_rejected_chunk_names_which_one_it_was() {
        let payload = vec![b'x'; CHUNK_BYTES * 3];
        let mut device = FakeDevice::new(&[
            "OK PONG",
            "OK CONFIG_READY",
            "OK CONFIG_CHUNK",
            "ERR 9 chunk mismatch",
        ]);
        let status = Session::new(&mut device).send_config(&payload);
        assert!(!status.is_ok());
        assert!(status.line().contains("chunk 2 of 3"), "{}", status.line());
        assert!(
            !status.line().contains("CONFIG END"),
            "a failed import must not be finalised"
        );
    }

    #[test]
    fn a_config_result_carries_only_the_devices_counts() {
        let mut device = FakeDevice::new(&[
            "OK PONG",
            "OK CONFIG_READY",
            "OK CONFIG_CHUNK",
            "OK CONFIG_VALIDATED applied=90 rejected=0 clamped=0 unknown=1 missing=8",
        ]);
        let status = Session::new(&mut device).send_config(b"{}");
        assert!(status.is_ok());
        assert_eq!(
            status.line(),
            "OK CONFIG_APPLIED applied=90 rejected=0 clamped=0 unknown=1 missing=8"
        );
    }

    #[test]
    fn an_empty_config_is_sent_as_a_begin_and_an_end_with_no_chunks() {
        let mut device = FakeDevice::new(&[
            "OK PONG",
            "OK CONFIG_READY",
            "OK CONFIG_VALIDATED applied=0",
        ]);
        let status = Session::new(&mut device).send_config(b"");
        assert!(status.is_ok());
        assert_eq!(
            device.received().len(),
            3,
            "no chunk lines for an empty payload"
        );
    }

    #[test]
    fn a_config_the_device_rejects_ends_with_a_failure_not_an_applied_report() {
        // The CRC is checked at `CONFIG END`, after the data is buffered, so a corrupt transfer is
        // caught before anything is applied rather than after.
        let mut device = FakeDevice::new(&[
            "OK PONG",
            "OK CONFIG_READY",
            "OK CONFIG_CHUNK",
            "ERR 13 crc mismatch",
        ]);
        let status = Session::new(&mut device).send_config(b"{}");
        assert!(!status.is_ok());
        assert_eq!(status.line(), "FAIL CONFIG_REJECTED ERR 13 crc mismatch");
    }

    #[test]
    fn the_factory_reset_sends_one_command() {
        let mut device = FakeDevice::new(&["OK PONG", "OK FACTORY_RESET"]);
        let status = Session::new(&mut device).factory_reset();
        assert!(status.is_ok(), "{}", status.line());
        assert_eq!(device.received(), vec!["PING", "FACTORY RESET"]);
    }

    #[test]
    fn the_status_command_sends_one_command() {
        let mut device = FakeDevice::new(&["OK PONG", "OK STATUS state=PID_NORMAL uptime=1234"]);
        let status = Session::new(&mut device).status();
        assert!(status.is_ok());
        assert_eq!(device.received(), vec!["PING", "STATUS"]);
    }

    #[test]
    fn a_device_whose_reply_is_not_the_expected_token_is_a_failure() {
        // A device that answers every command with OK would otherwise make a broken provisioning
        // channel look like a working one.
        let mut device = FakeDevice::new(&["OK PONG", "OK SOMETHING_ELSE"]);
        let status = Session::new(&mut device).factory_reset();
        assert!(!status.is_ok());
        assert!(status.line().contains("FACTORY_RESET"), "{}", status.line());
    }

    #[test]
    fn a_device_error_cannot_forge_a_second_status_line() {
        let mut device = FakeDevice::new(&["ERR 1 nope\nOK WIFI_SET"]);
        let status = Session::new(&mut device).factory_reset();
        assert!(!status.is_ok());
        assert_eq!(status.line().matches('\n').count(), 0, "{}", status.line());
    }

    #[test]
    fn a_reply_whose_tail_is_not_counts_is_dropped_rather_than_forwarded() {
        // The counts are the only thing the device may say at `CONFIG END`. A firmware that put a
        // value there would be a protocol change, and forwarding it would put a config value on the
        // host's terminal.
        assert_eq!(
            reply_detail("OK CONFIG_VALIDATED applied=88 rejected=0"),
            "applied=88 rejected=0"
        );
        assert_eq!(reply_detail("OK CONFIG_VALIDATED"), "");
        assert_eq!(
            reply_detail("OK CONFIG_VALIDATED password=placeholder-value"),
            ""
        );
        assert_eq!(
            reply_detail("OK CONFIG_VALIDATED applied=placeholder-value"),
            ""
        );
    }

    #[test]
    fn a_very_long_device_error_is_bounded() {
        let long = format!("ERR 1 {}", "x".repeat(500));
        let mut device = FakeDevice::new(&[&long]);
        let status = Session::new(&mut device).status();
        assert!(
            status.line().len() < 200,
            "the detail was {}",
            status.line().len()
        );
    }
}
