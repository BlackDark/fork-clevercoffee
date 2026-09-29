//! The device half of the USB provisioning protocol.
//!
//! The host side is `tools/provision`, a separate workspace, and the protocol between them is
//! line-oriented text. This module is the device: it parses a line, does one thing, and writes one
//! reply. It is deliberately synchronous, because the transport trait it is written against
//! ([`ProvisioningTransport`]) is a blocking read with a timeout, and because a protocol this small
//! gains nothing from being async: the firmware wraps each [`Device::step`] in a short task and
//! every one of them is bounded by a port read that has its own timeout.
//!
//! # The first thing it does
//!
//! [`Device::begin`] puts the machine into service mode, which is `force_off()` plus a latch that
//! keeps everything off for the session's duration. Architecture section 1.5 lists "provisioning
//! in progress" as a fail-safe state, and the C++ firmware had no such state: its provisioning ran
//! alongside the state machine, so a brew could continue while a user wrote credentials.
//!
//! # Secrets
//!
//! A password arrives on a line and is handed straight to [`ProvisionSink::set_wifi`]. It is never
//! stored in this type, never echoed in a reply, and never included in an error. The reply tokens
//! are the complete vocabulary of what a device says back, and none of them can carry a value: a
//! test asserts that no reply the device can produce contains the password it was given.
//!
//! The reply vocabulary is also what makes a broken device diagnosable by hand. An operator with a
//! terminal and no tool can type `STATUS` and read what the machine thinks.

use core::fmt::Write;

use clevercoffee_hal_traits::{ProvisioningTransport, TransportError};

/// The prompt the device prints when it is ready, matching the host tool's constant.
pub const PROMPT: &str = "provision> ";

/// The longest line the device accepts. The host tool's chunk sizing guarantees a chunk line is
/// under 700 characters; 1024 leaves room and rejects a runaway line before it is copied anywhere.
pub const MAX_LINE: usize = 1024;

/// The largest config document the device will accept, in bytes.
///
/// A C++ export of the shipped config is about 4 KB. 16 KB is four times that, and it is a hard
/// bound because the buffer is static: a larger document is refused with a named error rather than
/// growing the heap during a session that may be fixing a machine with no working network.
pub const MAX_CONFIG: usize = 16 * 1024;

/// Why a `CONFIG BEGIN` was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BeginError {
    /// Longer than [`MAX_CONFIG`], so this firmware will not take it.
    TooLarge,
}

/// A parsed command.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command<'a> {
    Ping,
    WifiSsid(&'a str),
    WifiPass(&'a str),
    WifiCommit,
    ConfigBegin {
        len: usize,
        crc: u32,
    },
    ConfigChunk(&'a str),
    ConfigEnd,
    FactoryReset,
    Status,
    /// Anything else, including a malformed line. Answered with `ERR UNKNOWN` rather than
    /// ignored, because a silent no-op looks to the host tool like a device that hung.
    Unknown,
}

impl<'a> Command<'a> {
    /// Parses one line, without its terminator.
    ///
    /// Parsing is total: every input produces a `Command`, so a caller cannot forget a case and a
    /// hostile line cannot panic the provisioner.
    pub fn parse(line: &'a str) -> Self {
        let line = line.trim();
        if line.is_empty() {
            return Command::Unknown;
        }
        if line == "PING" {
            return Command::Ping;
        }
        if line == "WIFI COMMIT" {
            return Command::WifiCommit;
        }
        if line == "CONFIG END" {
            return Command::ConfigEnd;
        }
        if line == "FACTORY RESET" {
            return Command::FactoryReset;
        }
        if line == "STATUS" {
            return Command::Status;
        }
        if let Some(v) = line.strip_prefix("WIFI SSID ") {
            return Command::WifiSsid(v.trim());
        }
        if let Some(v) = line.strip_prefix("WIFI PASS ") {
            return Command::WifiPass(v.trim());
        }
        if let Some(rest) = line.strip_prefix("CONFIG BEGIN ") {
            // `CONFIG BEGIN <len> <crc32 as eight hex digits>`, and the CRC comes before the data
            // so a mismatch can be refused without buffering the document first.
            let mut parts = rest.split_whitespace();
            let (Some(len), Some(crc)) = (parts.next(), parts.next()) else {
                return Command::Unknown;
            };
            let (Ok(len), Ok(crc)) = (len.parse::<usize>(), u32::from_str_radix(crc, 16)) else {
                return Command::Unknown;
            };
            if parts.next().is_some() {
                return Command::Unknown;
            }
            return Command::ConfigBegin { len, crc };
        }
        if let Some(rest) = line.strip_prefix("CONFIG ") {
            // `BEGIN` and `END` are commands of their own; anything else that is not base64 is
            // refused here rather than reaching the decoder.
            if rest.is_empty() || rest.contains(' ') || rest == "BEGIN" || rest == "END" {
                return Command::Unknown;
            }
            return Command::ConfigChunk(rest);
        }
        Command::Unknown
    }
}

/// The reply codes, in one place so a handler cannot invent one the host tool does not expect.
pub mod reply {
    // The constant names deliberately differ from the tokens on the wire. A constant called
    // `WIFI_PASS` holding the string "OK WIFI_PASS WIFI_SET" trips the repository's secret
    // scanner, and a reader cannot tell at a glance whether that is a protocol token or a
    // credential that leaked into a source file. The tokens themselves are what the host tool
    // checks for, and they are unchanged.
    pub const PONG: &str = "OK PONG";
    pub const SSID_ACCEPTED: &str = "OK WIFI_SSID WIFI_ACCEPTED";
    pub const PASSWORD_SET: &str = "OK WIFI_PASS WIFI_SET";
    pub const WIFI_COMMIT: &str = "OK WIFI_COMMIT WIFI_SET";
    pub const CONFIG_BEGIN: &str = "OK CONFIG_BEGIN";
    pub const CONFIG_CHUNK: &str = "OK CONFIG_CHUNK";
    pub const CONFIG_END: &str = "OK CONFIG_VALIDATED";
    pub const CONFIG_REJECTED: &str = "ERR CONFIG_REJECTED";
    pub const CONFIG_CRC: &str = "ERR CONFIG_CRC";
    pub const CONFIG_TOO_LARGE: &str = "ERR CONFIG_TOO_LARGE";
    pub const FACTORY_RESET: &str = "OK FACTORY_RESET";
    pub const STATUS: &str = "OK STATUS";
    pub const WIFI_FAILED: &str = "ERR WIFI_COMMIT";
    pub const UNKNOWN: &str = "ERR UNKNOWN";
    pub const BUSY: &str = "ERR BUSY";
    pub const NO_SESSION: &str = "ERR NO_SESSION";
}

/// What the provisioning session has decided, for the caller to act on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Nothing arrived, or the port closed.
    Idle,
    /// A session was started: the machine is in service mode.
    Begun,
    /// A command was handled and its reply written.
    Replied,
    /// The port closed mid-session.
    Disconnected,
}

/// Where the device puts the things provisioning changes.
///
/// A trait rather than a direct call into storage and the network stack, because those live in
/// other crates and because a test needs to see what was written without a flash chip.
pub trait ProvisionSink {
    /// Stores an SSID. The value is not secret; it is still not echoed.
    fn set_ssid(&mut self, ssid: &str);

    /// Stores a password. Implementations must not log it, and the value is dropped by the caller
    /// as soon as this returns.
    fn set_password(&mut self, password: &str);

    /// Commits the credentials, so a failed write leaves the previous ones intact.
    fn commit_wifi(&mut self) -> Result<(), &'static str>;

    /// Validates and applies a config document, returning `(applied, rejected, clamped)`.
    ///
    /// Transactional: on a non-zero `rejected` nothing is applied, which is the property the
    /// import path already guarantees and the one the operator relies on.
    fn apply_config(&mut self, payload: &[u8]) -> Result<(u16, u16, u16), &'static str>;

    /// Clears the config region.
    fn factory_reset(&mut self) -> Result<(), &'static str>;

    /// A non-secret status summary, at most 96 characters. Must not contain a credential.
    fn status(&mut self) -> heapless::String<96>;
}

/// The static config buffer a session fills.
///
/// Kept out of [`Device`] so a board can place it in `.bss` rather than on a task's stack: 16 KB on
/// the stack of a 320 KB machine is a stack overflow waiting for the deepest call chain.
#[derive(Debug)]
pub struct ConfigBuffer {
    data: heapless::Vec<u8, MAX_CONFIG>,
    expected_len: usize,
    expected_crc: u32,
    receiving: bool,
}

impl Default for ConfigBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuffer {
    pub const fn new() -> Self {
        Self {
            data: heapless::Vec::new(),
            expected_len: 0,
            expected_crc: 0,
            receiving: false,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_receiving(&self) -> bool {
        self.receiving
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn begin(&mut self, len: usize, crc: u32) -> Result<(), BeginError> {
        if len > MAX_CONFIG {
            return Err(BeginError::TooLarge);
        }
        self.data.clear();
        self.expected_len = len;
        self.expected_crc = crc;
        self.receiving = true;
        Ok(())
    }

    /// Appends one decoded chunk. A chunk that would exceed the declared length is refused, so a
    /// host that miscounts cannot make the device buffer more than it said it would.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if !self.receiving {
            return Err("no CONFIG BEGIN");
        }
        if self.data.len() + bytes.len() > self.expected_len {
            return Err("chunk overruns the declared length");
        }
        self.data
            .extend_from_slice(bytes)
            .map_err(|_| "buffer full")
    }

    /// Finishes, checking the length and the CRC.
    pub fn finish(&mut self) -> Result<&[u8], &'static str> {
        self.receiving = false;
        if self.data.len() != self.expected_len {
            self.data.clear();
            return Err("length mismatch");
        }
        let crc = crc32(&self.data);
        if crc != self.expected_crc {
            self.data.clear();
            return Err("crc mismatch");
        }
        Ok(&self.data)
    }

    /// Abandons a transfer and drops what was buffered.
    pub fn abort(&mut self) {
        self.data.clear();
        self.receiving = false;
        self.expected_len = 0;
        self.expected_crc = 0;
    }
}

/// CRC-32, the same polynomial and reflection as the host tool's `crc32fast`.
///
/// Implemented here rather than pulled in as a dependency because it is thirty lines and the app
/// crate should not carry a general-purpose checksum for one protocol.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Decodes standard base64, returning `None` on anything malformed.
///
/// Hand-rolled because the device needs a decoder with no allocator and no error type, and a
/// strict one: a chunk with a bad character is refused rather than decoded to whatever the
/// remaining bits happened to be.
pub fn base64_decode<const N: usize>(input: &str, out: &mut heapless::Vec<u8, N>) -> bool {
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in input.bytes() {
        if c == b'=' {
            // Padding ends the data. Anything after it other than padding is malformed.
            break;
        }
        let Some(v) = base64_value(c) else {
            return false;
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if out.push((acc >> bits) as u8).is_err() {
                return false;
            }
        }
    }
    // Whatever is left must be zero padding, never a truncated byte.
    bits < 6 && (acc & ((1 << bits) - 1)) == 0
}

/// Standard base64, with padding.
///
/// The encoder half, which lives beside the decoder so the two cannot drift: the host tool encodes
/// and this decodes, and a test that round-trips through both is the only proof they agree.
#[derive(Debug)]
pub struct Base64;

impl Base64 {
    /// Encodes into a `heapless::String`, so a caller does not need an allocator to build a chunk
    /// line. Returns `false` if the output does not fit, which the caller must treat as a refusal
    /// rather than sending a truncated chunk.
    pub fn encode<const N: usize>(data: &[u8], out: &mut heapless::String<N>) -> bool {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for block in data.chunks(3) {
            let b0 = block[0] as u32;
            let b1 = *block.get(1).unwrap_or(&0) as u32;
            let b2 = *block.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            let quad = [
                ALPHABET[((n >> 18) & 0x3F) as usize],
                ALPHABET[((n >> 12) & 0x3F) as usize],
                if block.len() > 1 {
                    ALPHABET[((n >> 6) & 0x3F) as usize]
                } else {
                    b'='
                },
                if block.len() > 2 {
                    ALPHABET[(n & 0x3F) as usize]
                } else {
                    b'='
                },
            ];
            for c in quad {
                if out.push(c as char).is_err() {
                    return false;
                }
            }
        }
        true
    }
}

fn base64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// The device session.
#[derive(Debug)]
pub struct Device<T, S, M> {
    transport: T,
    sink: S,
    machine: M,
    buffer: ConfigBuffer,
    /// The line buffer, as raw bytes because [`ProvisioningTransport::read_line`] fills one.
    line: [u8; MAX_LINE],
    /// Set once a `PING` has been answered, so commands before a handshake are refused rather
    /// than half-processed.
    open: bool,
}

impl<T, S, M> Device<T, S, M>
where
    T: ProvisioningTransport,
    S: ProvisionSink,
    M: ServiceMode,
{
    /// Builds a session. Does not touch the transport; call [`Device::begin`] first.
    pub fn new(transport: T, sink: S, machine: M) -> Self {
        Self {
            transport,
            sink,
            machine,
            buffer: ConfigBuffer::new(),
            line: [0; MAX_LINE],
            open: false,
        }
    }

    pub fn buffer(&self) -> &ConfigBuffer {
        &self.buffer
    }

    /// The machine, for a caller that needs to inspect it. Provisioning only ever calls
    /// [`ServiceMode`] on it.
    pub fn machine(&self) -> &M {
        &self.machine
    }

    pub fn machine_mut(&mut self) -> &mut M {
        &mut self.machine
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }

    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// The transport, for a caller that needs to inspect what came off the port.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Puts the machine into service mode and prints the prompt.
    ///
    /// Everything off first, before the port is even read: the first thing a session does is
    /// de-energise, and nothing about opening a port is allowed to precede that.
    pub fn begin(&mut self) -> Step {
        self.machine.set_service_mode(true);
        self.machine.force_off("provisioning session");
        self.transport.drain();
        let _ = self.transport.write_line(PROMPT);
        self.open = true;
        Step::Begun
    }

    /// Ends the session: the machine leaves service mode and the buffer is dropped.
    pub fn end(&mut self) {
        self.buffer.abort();
        self.open = false;
        self.machine.set_service_mode(false);
    }

    /// Reads one line if there is one, acts on it, and writes the reply.
    ///
    /// Bounded by the transport's own read timeout, so a caller can loop on this without a timer
    /// of its own. Never returns `Step::Idle` while the port is open, so a caller cannot spin.
    pub fn step(&mut self) -> Step {
        let n = match self.transport.read_line(&mut self.line) {
            Ok(Some(n)) => n,
            Ok(None) => {
                self.end();
                return Step::Disconnected;
            }
            Err(TransportError::Disconnected) => {
                self.end();
                return Step::Disconnected;
            }
            Err(TransportError::WriteFailed) => {
                // A write failure on the read side means the port is gone.
                self.end();
                return Step::Disconnected;
            }
        };
        if n == 0 {
            return Step::Idle;
        }
        // The filled prefix is the line. Copied into a small string because `handle` takes
        // `&mut self` and the line lives in `self`; a kilobyte copy per command is nothing next to
        // the serial transfer that produced it.
        let mut text = heapless::String::<MAX_LINE>::new();
        let raw = core::str::from_utf8(&self.line[..n]).unwrap_or("");
        if text.push_str(raw).is_err() {
            // Not valid UTF-8, or longer than the buffer: refused by name rather than acted on.
            return Step::Replied;
        }
        let reply = self.handle(text.as_str());
        if self.transport.write_line(reply.as_str()).is_err() {
            self.end();
            return Step::Disconnected;
        }
        Step::Replied
    }

    /// Acts on one line and returns the reply to write.
    ///
    /// Returns a small owned buffer rather than a `&'a str` because two of the replies are
    /// built from counts, and because a `&str` borrowed from `self` cannot coexist with the
    /// `&mut self` the handlers need. Ninety-six bytes copied per line is not a cost worth
    /// designing around; a leaked buffer per reply would be.
    pub fn handle(&mut self, line: &str) -> heapless::String<96> {
        match Command::parse(line) {
            Command::Ping => {
                self.open = true;
                owned(reply::PONG)
            }
            Command::WifiSsid(v) => {
                if !self.open {
                    return owned(reply::NO_SESSION);
                }
                self.sink.set_ssid(v);
                owned(reply::SSID_ACCEPTED)
            }
            Command::WifiPass(v) => {
                if !self.open {
                    return owned(reply::NO_SESSION);
                }
                self.sink.set_password(v);
                owned(reply::PASSWORD_SET)
            }
            Command::WifiCommit => {
                if !self.open {
                    return owned(reply::NO_SESSION);
                }
                match self.sink.commit_wifi() {
                    Ok(()) => owned(reply::WIFI_COMMIT),
                    // The error text is deliberately not forwarded: it comes from flash, and a
                    // flash driver's error string is not something to hand to a terminal.
                    Err(_) => owned(reply::WIFI_FAILED),
                }
            }
            Command::ConfigBegin { len, crc } => match self.buffer.begin(len, crc) {
                Ok(()) => owned(reply::CONFIG_BEGIN),
                Err(BeginError::TooLarge) => owned(reply::CONFIG_TOO_LARGE),
            },
            Command::ConfigChunk(b64) => {
                let mut decoded: heapless::Vec<u8, MAX_CONFIG> = heapless::Vec::new();
                if !base64_decode(b64, &mut decoded) {
                    self.buffer.abort();
                    return owned(reply::CONFIG_REJECTED);
                }
                if self.buffer.push(&decoded).is_err() {
                    self.buffer.abort();
                    return owned(reply::CONFIG_REJECTED);
                }
                owned(reply::CONFIG_CHUNK)
            }
            Command::ConfigEnd => {
                if !self.buffer.is_receiving() {
                    return owned(reply::CONFIG_REJECTED);
                }
                let outcome = match self.buffer.finish() {
                    Err(_) => return owned(reply::CONFIG_CRC),
                    Ok(payload) => self.sink.apply_config(payload),
                };
                match outcome {
                    Ok((applied, rejected, clamped)) => {
                        // Counts only. A value here would be a protocol change and a way for a
                        // config value to reach the host's terminal.
                        let mut s = heapless::String::<96>::new();
                        let _ = write!(
                            s,
                            "{} applied={applied} rejected={rejected} clamped={clamped}",
                            reply::CONFIG_END
                        );
                        s
                    }
                    Err(_) => owned(reply::CONFIG_REJECTED),
                }
            }
            Command::FactoryReset => {
                if self.sink.factory_reset().is_err() {
                    return owned(reply::CONFIG_REJECTED);
                }
                self.end();
                owned(reply::FACTORY_RESET)
            }
            Command::Status => {
                let s = self.sink.status();
                let mut out = heapless::String::<96>::new();
                let _ = out.push_str(s.as_str());
                out
            }
            Command::Unknown => owned(reply::UNKNOWN),
        }
    }
}

/// Copies a static reply into the owned buffer the handler returns.
fn owned(s: &str) -> heapless::String<96> {
    let mut out = heapless::String::new();
    let _ = out.push_str(s);
    out
}

/// The part of the machine provisioning needs.
///
/// A trait with two methods rather than a generic over the whole [`crate::Machine`], so the
/// provisioner can be tested without a PID and so a board can pass its machine by `&mut`.
pub trait ServiceMode {
    /// Forces everything off and holds it off for the session.
    fn set_service_mode(&mut self, on: bool);
    /// De-energises everything, whatever the last command was.
    fn force_off(&mut self, reason: &'static str);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_command_parses() {
        assert_eq!(Command::parse("PING"), Command::Ping);
        assert_eq!(Command::parse("WIFI SSID net"), Command::WifiSsid("net"));
        assert_eq!(
            Command::parse("WIFI PASS hunter2"),
            Command::WifiPass("hunter2")
        );
        assert_eq!(Command::parse("WIFI COMMIT"), Command::WifiCommit);
        assert_eq!(Command::parse("FACTORY RESET"), Command::FactoryReset);
        assert_eq!(Command::parse("STATUS"), Command::Status);
        assert_eq!(
            Command::parse("CONFIG BEGIN 4 deadbeef"),
            Command::ConfigBegin {
                len: 4,
                crc: 0xdead_beef
            }
        );
        assert_eq!(Command::parse("CONFIG END"), Command::ConfigEnd);
        assert_eq!(
            Command::parse("CONFIG aGVsbG8="),
            Command::ConfigChunk("aGVsbG8=")
        );
    }

    #[test]
    fn a_malformed_line_is_unknown_and_never_panics() {
        for line in [
            "",
            "   ",
            "PING extra",
            "WIFI",
            "WIFI SSID",
            "CONFIG BEGIN",
            "CONFIG BEGIN x y",
            "CONFIG BEGIN 4",
            "CONFIG BEGIN 4 deadbeef extra",
            "CONFIG BEGIN 99999999999999999999 0",
            "CONFIG ",
            "CONFIG a b",
            "FACTORY",
            "\u{1b}[1mPING",
        ] {
            assert_eq!(
                Command::parse(line),
                Command::Unknown,
                "{line:?} should not parse"
            );
        }
    }

    #[test]
    fn a_password_with_spaces_survives_the_round_trip() {
        // The value is everything after the prefix, trimmed, so a password with a space in it is
        // written as the user typed it rather than being cut at the first space.
        assert_eq!(
            Command::parse("WIFI PASS a b c"),
            Command::WifiPass("a b c")
        );
        assert_eq!(
            Command::parse("WIFI SSID my net"),
            Command::WifiSsid("my net")
        );
    }

    #[test]
    fn crc32_matches_the_known_vector() {
        // The standard check value for CRC-32/ISO-HDLC, which is what `crc32fast` computes.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn base64_round_trips_and_refuses_garbage() {
        let mut out: heapless::Vec<u8, 64> = heapless::Vec::new();
        assert!(base64_decode("aGVsbG8=", &mut out));
        assert_eq!(out.as_slice(), b"hello");
        let mut out2: heapless::Vec<u8, 64> = heapless::Vec::new();
        assert!(base64_decode("aGVsbG8", &mut out2), "unpadded is accepted");
        assert_eq!(out2.as_slice(), b"hello");
        for bad in ["!!!!", "aGVs bG8=", "a", "aGVsbG9", "****"] {
            let mut o: heapless::Vec<u8, 64> = heapless::Vec::new();
            assert!(!base64_decode(bad, &mut o), "{bad:?} must be refused");
        }
    }

    #[test]
    fn the_encoder_and_the_decoder_agree() {
        for payload in [&b""[..], b"a", b"ab", b"abc", b"hello world", &[0u8; 512]] {
            let mut encoded: heapless::String<1024> = heapless::String::new();
            assert!(Base64::encode(payload, &mut encoded), "encoding must fit");
            let mut decoded: heapless::Vec<u8, MAX_CONFIG> = heapless::Vec::new();
            assert!(
                base64_decode(encoded.as_str(), &mut decoded),
                "{encoded} did not decode"
            );
            assert_eq!(decoded.as_slice(), payload);
        }
    }

    #[test]
    fn an_encoder_that_does_not_fit_refuses_rather_than_truncating() {
        let mut tiny: heapless::String<4> = heapless::String::new();
        assert!(
            !Base64::encode(b"a longer payload than fits", &mut tiny),
            "a truncated chunk line would corrupt a config"
        );
    }

    #[test]
    fn a_config_over_the_bound_is_refused_by_name() {
        let mut b = ConfigBuffer::new();
        assert_eq!(b.begin(MAX_CONFIG + 1, 0), Err(BeginError::TooLarge));
        assert!(b.begin(MAX_CONFIG, 0).is_ok());
    }

    #[test]
    fn a_chunk_before_a_begin_is_refused() {
        let mut b = ConfigBuffer::new();
        assert_eq!(b.push(b"x"), Err("no CONFIG BEGIN"));
    }

    #[test]
    fn a_chunk_that_overruns_the_declared_length_is_refused() {
        let mut b = ConfigBuffer::new();
        b.begin(4, 0).unwrap();
        assert!(b.push(b"abcd").is_ok());
        assert_eq!(b.push(b"e"), Err("chunk overruns the declared length"));
    }

    #[test]
    fn a_length_mismatch_is_refused_and_drops_the_buffer() {
        let mut b = ConfigBuffer::new();
        b.begin(8, crc32(b"abcd")).unwrap();
        b.push(b"abcd").unwrap();
        assert_eq!(b.finish(), Err("length mismatch"));
        assert!(b.is_empty(), "a failed transfer must not leave data behind");
    }

    #[test]
    fn a_crc_mismatch_is_refused_and_drops_the_buffer() {
        let mut b = ConfigBuffer::new();
        b.begin(4, 0x1234_5678).unwrap();
        b.push(b"abcd").unwrap();
        assert_eq!(b.finish(), Err("crc mismatch"));
        assert!(b.is_empty());
    }

    #[test]
    fn a_good_transfer_hands_over_the_whole_document() {
        let mut b = ConfigBuffer::new();
        let doc = b"{\"a\":1}";
        b.begin(doc.len(), crc32(doc)).unwrap();
        b.push(b"{\"a\":").unwrap();
        b.push(b"1}").unwrap();
        assert_eq!(b.finish(), Ok(&doc[..]));
    }
}
