//! The transport, as a trait.
//!
//! Splitting this out is what makes the protocol testable. Everything above this trait is the
//! logic that decides what to send and what a reply means; everything below it is a serial port.
//! The tests drive the real logic against an in-memory device, so there is no "tested up to the
//! serial port" gap in the part that decides whether a migration succeeded.

use std::fmt;
use std::time::{Duration, Instant};

// `set_timeout` and `try_clone` are trait methods; without this in scope the calls look like
// inherent methods and do not resolve.
use serialport::SerialPort as _;

/// The link to a device.
pub trait Link {
    /// Writes one protocol line, terminator included.
    fn send(&mut self, line: &str) -> Result<(), LinkError>;

    /// Reads one reply line, without its terminator.
    fn read_line(&mut self) -> Result<String, LinkError>;

    /// Discards whatever the device printed before its prompt: a boot banner, a reset message, or
    /// the residue of a previous session. Without this, a device that prints a firmware version on
    /// boot makes the first `read_line` return the banner and every command appear to fail.
    fn drain(&mut self) -> Result<(), LinkError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// The port could not be opened.
    Open(String),
    /// The port was opened but a read or write failed. Usually the cable moved.
    Io(String),
    /// No reply arrived within the timeout.
    Timeout,
    /// The device closed the port, which for a provisioning session usually means it rebooted.
    Disconnected,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Open(e) => write!(f, "cannot open the port: {e}"),
            LinkError::Io(e) => write!(f, "port error: {e}"),
            LinkError::Timeout => write!(f, "no reply from the device"),
            LinkError::Disconnected => write!(f, "the device closed the port"),
        }
    }
}

/// A deadline-based read loop, shared by both link implementations.
///
/// A serial read with a timeout returns "nothing yet" rather than "done", so a caller needs this
/// loop rather than a single read. A device mid-flash or mid-reboot is silent for seconds, and the
/// wait is the caller's decision, not the port library's.
pub fn read_with_deadline<F>(mut read_once: F, timeout: Duration) -> Result<String, LinkError>
where
    F: FnMut(&mut String) -> Result<usize, LinkError>,
{
    let deadline = Instant::now() + timeout;
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        match read_once(&mut line)? {
            0 => continue,
            _ => return Ok(line.trim_end().to_string()),
        }
    }
    Err(LinkError::Timeout)
}

/// A serial port, as a `Link`.
///
/// The port and its reader are both trait objects rather than a concrete `TTYPort`, because the
/// device half needs a clone and the reader half needs `get_mut`, and a `Box<dyn SerialPort>`
/// carries neither method. Holding the concrete type is the simplest thing that does.
pub struct SerialLink {
    port: serialport::TTYPort,
    reader: Box<dyn std::io::BufRead + Send>,
    timeout: Duration,
}

impl std::fmt::Debug for SerialLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The port name is not stored, so there is nothing to print and nothing to leak.
        f.write_str("SerialLink")
    }
}

impl SerialLink {
    pub fn open(name: &str, timeout: Duration) -> Result<Self, LinkError> {
        let mut port = serialport::TTYPort::open(&serialport::new(name, 115_200))
            .map_err(|e| LinkError::Open(e.to_string()))?;
        port.set_timeout(Duration::from_millis(200))
            .map_err(|e| LinkError::Open(e.to_string()))?;
        let cloned = port
            .try_clone()
            .map_err(|e| LinkError::Open(e.to_string()))?;
        Ok(Self {
            port,
            reader: Box::new(std::io::BufReader::new(cloned)),
            timeout,
        })
    }

    /// Reads whatever is available without waiting for a terminator.
    ///
    /// Used by [`Link::drain`] to look for the prompt. Blocking on a whole line would wait for one
    /// that a device mid-boot never sends, which is exactly the case drain exists to survive.
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, LinkError> {
        match self.reader.as_mut().read(buf) {
            Ok(n) => Ok(n),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            Err(e) => Err(LinkError::Io(e.to_string())),
        }
    }
}

impl Link for SerialLink {
    fn send(&mut self, line: &str) -> Result<(), LinkError> {
        use std::io::Write;
        self.port
            .write_all(line.as_bytes())
            .and_then(|()| self.port.write_all(b"\r\n"))
            .and_then(|()| self.port.flush())
            .map_err(|e| LinkError::Io(e.to_string()))
    }

    fn read_line(&mut self) -> Result<String, LinkError> {
        let timeout = self.timeout;
        read_with_deadline(
            |line| match self.reader.as_mut().read_line(line) {
                Ok(n) => Ok(n),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    Ok(0)
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    Err(LinkError::Disconnected)
                }
                Err(e) => Err(LinkError::Io(e.to_string())),
            },
            timeout,
        )
    }

    fn drain(&mut self) -> Result<(), LinkError> {
        // Discard until the prompt, so a boot banner is not mistaken for a reply. Bounded by the
        // same deadline as a read: a device that never presents a prompt is a failure, not a hang.
        let deadline = Instant::now() + self.timeout;
        let mut scratch = [0u8; 256];
        while Instant::now() < deadline {
            let n = self.read_available(&mut scratch)?;
            if n == 0 {
                continue;
            }
            let tail = String::from_utf8_lossy(&scratch[..n]);
            if tail.trim_end().ends_with(crate::PROMPT.trim_end()) {
                return Ok(());
            }
        }
        Err(LinkError::Timeout)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A device that replies from a script.
    ///
    /// Public so the protocol tests can drive the real session logic against it, which is the only
    /// way to know the command sequence is right without a board.
    ///
    /// Records what it was sent and answers from a queue, so a test can assert on the exact
    /// conversation and can make the device misbehave in one specific way at a time.
    #[derive(Debug, Default)]
    pub struct FakeDevice {
        sent: RefCell<Vec<String>>,
        replies: RefCell<Vec<String>>,
        /// When true, every read times out. A device that has just been plugged in and has not
        /// reached its prompt behaves this way.
        pub silent: bool,
    }

    impl FakeDevice {
        pub fn new(replies: &[&str]) -> Self {
            Self {
                sent: RefCell::new(Vec::new()),
                replies: RefCell::new(replies.iter().map(|s| s.to_string()).collect()),
                silent: false,
            }
        }

        pub fn silent() -> Self {
            Self {
                silent: true,
                ..Default::default()
            }
        }

        /// The lines the host sent, in order.
        pub fn received(&self) -> Vec<String> {
            self.sent.borrow().clone()
        }
    }

    impl Link for FakeDevice {
        fn send(&mut self, line: &str) -> Result<(), LinkError> {
            self.sent.borrow_mut().push(line.to_string());
            Ok(())
        }

        fn read_line(&mut self) -> Result<String, LinkError> {
            if self.silent {
                return Err(LinkError::Timeout);
            }
            let mut replies = self.replies.borrow_mut();
            if replies.is_empty() {
                return Err(LinkError::Disconnected);
            }
            Ok(replies.remove(0))
        }

        fn drain(&mut self) -> Result<(), LinkError> {
            Ok(())
        }
    }

    #[test]
    fn a_read_that_returns_nothing_yet_is_retried_until_the_deadline() {
        // A serial read with a timeout returns "nothing yet", not "done". Treating that as a reply
        // would make every command fail on a device that is merely slow.
        let mut calls = 0;
        let result = read_with_deadline(
            |line| {
                calls += 1;
                line.push_str("OK PONG");
                Ok(if calls < 3 { 0 } else { line.len() })
            },
            Duration::from_millis(500),
        );
        assert_eq!(result, Ok("OK PONG".to_string()));
        assert_eq!(calls, 3);
    }

    #[test]
    fn a_device_that_never_answers_times_out() {
        let result = read_with_deadline(|_| Ok(0), Duration::from_millis(50));
        assert_eq!(result, Err(LinkError::Timeout));
    }

    #[test]
    fn a_trailing_carriage_return_is_trimmed() {
        // The host sends CRLF, so every reply the device produces ends with CRLF too.
        let result = read_with_deadline(
            |line| {
                line.push_str("OK PONG\r\n");
                Ok(line.len())
            },
            Duration::from_millis(100),
        );
        assert_eq!(result, Ok("OK PONG".to_string()));
    }

    #[test]
    fn a_read_error_is_propagated_rather_than_retried_forever() {
        let result =
            read_with_deadline(|_| Err(LinkError::Disconnected), Duration::from_millis(100));
        assert_eq!(result, Err(LinkError::Disconnected));
    }

    #[test]
    fn the_fake_records_the_exact_conversation() {
        let mut device = FakeDevice::new(&["OK PONG"]);
        device.send("PING").unwrap();
        assert_eq!(device.received(), vec!["PING".to_string()]);
        assert_eq!(device.read_line().unwrap(), "OK PONG");
    }

    #[test]
    fn a_fake_with_no_replies_left_reports_a_disconnect() {
        // Which is what a device that reboots mid-session looks like.
        let mut device = FakeDevice::new(&[]);
        assert_eq!(device.read_line(), Err(LinkError::Disconnected));
    }

    #[test]
    fn a_link_error_renders_without_quoting_a_credential() {
        // The message reaches stdout, so it must be safe to print whatever the port library said.
        assert_eq!(LinkError::Timeout.to_string(), "no reply from the device");
        assert_eq!(
            LinkError::Disconnected.to_string(),
            "the device closed the port"
        );
    }

    #[test]
    fn opening_a_port_that_does_not_exist_is_an_open_error() {
        // The case the user hits when they pass the wrong port name, and the reason `status` must
        // exit non-zero rather than printing an empty status.
        let result = SerialLink::open(
            "/nonexistent/provision-test-port",
            Duration::from_millis(50),
        );
        assert!(matches!(result, Err(LinkError::Open(_))));
    }
}
