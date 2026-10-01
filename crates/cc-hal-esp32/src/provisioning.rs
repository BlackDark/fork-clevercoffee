//! UART Wi-Fi provisioning: the line reader and the credential staging.
//!
//! Owner: **R3-12** (task C).
//!
//! # What this is
//!
//! The device half of the protocol whose grammar and framing are
//! [`cc_domain::provisioning`]. Three pieces:
//!
//! 1. [`LineReader`] — assembles UART0 bytes into whole lines.
//! 2. [`Session`] — the parser plus the pending credential, and the **log
//!    muting** that the framing requires.
//! 3. The replies, written to stdout with a prefix a script can match.
//!
//! # 🔴 Rule 5 — the password window, and what actually enforces it
//!
//! `wifi set <ssid>` arms the parser, and the password can then arrive **either
//! as an argument — `wifi pass <password>` — or as the next line, positionally**.
//! The argument form has no window at all, so the whole of rule 5 is about the
//! positional form, which exists because an operator typing by hand must keep
//! working.
//!
//! **`cc_domain::provisioning::PASSWORD_WINDOW_MS` (30 s) bounds the exposure,
//! and on this board nothing else needs to.** The log stream is written by
//! `esp_idf_svc::log` to UART0 *transmit*; the parser reads UART0 *receive*.
//! A log line the firmware emits is never bytes the firmware reads back, so the
//! two cannot collide here. The window is what keeps a mistyped `wifi set` from
//! swallowing the next command the operator types.
//!
//! The mute ([`log_muted`]) exists for the transport that *does* collide: a log
//! stream teed to a TCP client (`/events`, the telnet stream) is bytes the
//! machine can also read. The flag is set and cleared correctly and is
//! **currently read by nothing** — see [`log_muted`] — so on this build the
//! window is the only protection, and it is sufficient. When R3-11's telnet
//! pump lands, the pump is what must check the flag; this file's job is to keep
//! it correct so that adding the check is one `if`.
//!
//! The cost of the window is that up to 30 s of diagnostics can be lost if a
//! line *is* swallowed. The alternative — a keyword before the password — would
//! have to be typed before it, and so would be part of what a paste or a
//! shoulder-surfer captures.
//!
//! # Why there is no captive portal here
//!
//! The portal is evaluated and deferred; see the bottom of
//! [`crate::wifi`]. The short version: it is redundant with a line protocol
//! that is proven on this hardware, it costs heap this firmware does not have,
//! and every Rust crate for it binds port 80 itself so it could not share it
//! with the REST API (02 §6).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use cc_domain::provisioning::password_window_expired;
use cc_domain::provisioning::{Accepted, Parser, Reply, MAX_LINE_BYTES};
use cc_domain::secret::Secret;
use esp_idf_hal::gpio::Gpio1;
use esp_idf_hal::gpio::Gpio3;
use esp_idf_hal::uart::{UartDriver, UART0};
use esp_idf_hal::units::Hertz;
use esp_idf_svc::sys::EspError;
use log::info;

use crate::time::now_ms;

/// The baud rate of the shared UART0 log stream, and therefore of provisioning.
///
/// 115200, the same rate the C++ uses and the same rate `esp_idf_svc::log`
/// assumes. A mismatch would produce a log stream full of framing errors on the
/// operator's terminal while the firmware believed it was fine.
pub const BAUD: u32 = 115_200;

/// A UART0 port that both reads the operator's typing and writes replies.
///
/// One type owning one `UartDriver`, so there is exactly one owner of the pin
/// and no aliasing between a reader and a writer. `cc_hal_esp32::time` and the
/// ESP-IDF console keep writing the *log* stream to the same wire through
/// `stdout`; this is the other direction plus a direct write for the replies.
///
/// # Why replies do not go through `print!`
///
/// `print!` writes to the `stdout` `FILE*`, which is the ESP-IDF console, which
/// is the log stream — and the log stream is **muted for the password window**.
/// A reply written that way would be dropped exactly when it matters most: the
/// line that says "password accepted". A direct write to the driver is not
/// subject to the mute, which is the whole reason the mute is a flag the
/// logging path checks rather than a closed channel.
///
/// # Why installing a driver on UART0 is safe here
///
/// ESP-IDF's console writes through the ROM `uart_tx_chars`, which pushes into
/// the same TX FIFO this driver uses, and the console's *read* path polls the RX
/// FIFO — which is the contention risk. Nothing in this firmware reads `stdin`,
/// so there is nothing for the console's read path to race with, and the log
/// stream keeps working unchanged.
pub struct Serial<'d> {
    driver: UartDriver<'d>,
}

impl Serial<'_> {
    /// Install the driver on UART0 (GPIO1 TX, GPIO3 RX) at [`BAUD`].
    ///
    /// # Errors
    ///
    /// `EspError` if the driver cannot be installed — `ESP_ERR_INVALID_ARG` on
    /// a chip without the peripheral, `ESP_ERR_NO_MEM` if `FreeRTOS` cannot
    /// supply the queue. **Neither is fatal**: the caller warns and runs without
    /// provisioning, which is a machine with no network surface rather than a
    /// machine that will not boot.
    ///
    /// GPIO1 is `PIN_STEAMLED` in `pinmapping.h:44`, whose comment is
    /// *"Moved from pin 1 (UART TX - conflicts with serial logging)"*. This
    /// firmware does not drive that LED, so taking the pin costs nothing.
    pub fn new(
        uart: UART0<'static>,
        tx: Gpio1<'static>,
        rx: Gpio3<'static>,
    ) -> Result<Self, EspError> {
        let config = esp_idf_hal::uart::config::Config::default()
            .baudrate(Hertz(BAUD))
            // One entry, not the default five: the reader drains a 64-byte
            // buffer at a time and a deeper queue would only hold stale bytes
            // after the operator has typed the next command.
            .queue_size(1);
        // The `None`s are the flow-control pins, which this board does not
        // wire (`Gpio1`/`Gpio3` are the only pins the USB-to-UART cable uses).
        // They are typed explicitly because `None` alone leaves the two
        // `InputPin`/`OutputPin` parameters ambiguous.
        let driver = UartDriver::new(
            uart,
            tx,
            rx,
            None::<esp_idf_hal::gpio::Gpio4<'static>>,
            None::<esp_idf_hal::gpio::Gpio5<'static>>,
            &config,
        )?;
        Ok(Self { driver })
    }

    /// Read whatever has arrived, without blocking.
    ///
    /// Returns 0 when nothing is waiting. A timeout is not an error: the loop
    /// this feeds is a 20 ms poll, and there is nothing to do between bytes.
    pub fn read_available(&self, buf: &mut [u8]) -> usize {
        // `UartDriver::read` with a zero-tick timeout is the non-blocking form
        // (`esp-idf-hal` 0.47 `uart.rs:969-1000`); it returns
        // `ESP_ERR_TIMEOUT` rather than 0 when the ring is empty, and both mean
        // "nothing yet".
        self.driver
            .read(buf, esp_idf_hal::delay::NON_BLOCK)
            .unwrap_or(0)
    }

    /// Write a reply, bypassing the log mute.
    ///
    /// A short write is dropped rather than retried: the TX buffer is full
    /// because the operator's terminal is not draining, and blocking here would
    /// block the reader on the same driver.
    pub fn write_reply(&self, text: &str) {
        let _ = self.driver.write(text.as_bytes());
    }
}

/// The prefix every provisioning reply starts with.
///
/// A script reading the console needs to tell a reply from a log line, and it
/// cannot rely on the line's shape because the log stream is writing
/// interleaved lines. A fixed, distinctive, three-letter prefix is the cheapest
/// thing that works, and it is deliberately *not* something the log stream can
/// produce: ESP-IDF's log lines start with a level letter and a space.
pub const REPLY_PREFIX: &str = "CCWIFI ";

/// The process-wide "do not write log lines to a readable transport" flag.
///
/// **Read by nothing in this build, and that is a stated gap rather than an
/// oversight.** The `log` stream goes through `esp_idf_svc::log` straight to
/// UART0's transmit pin, so nothing the firmware logs can reach the parser; the
/// flag has nothing to protect against yet. It is set and cleared correctly
/// because the moment R3-11's telnet pump lands — a log stream the machine can
/// also *read* — the flag is the whole of rule 5, and a flag that is wrong on
/// arrival would not be noticed until a credential leaked.
///
/// An `AtomicBool` and not a `Mutex`: it is written twice per `wifi set` and
/// read on every line of every log stream, from any task, including ones that
/// must not block.
static LOG_MUTED: AtomicBool = AtomicBool::new(false);

/// Whether the log stream is currently muted for a password window.
///
/// ⚠️ **No consumer yet.** This is the flag R3-11's telnet pump and the SSE
/// broadcast must check before writing a line; see the module documentation for
/// why the window alone is sufficient until then.
#[must_use]
pub fn log_muted() -> bool {
    LOG_MUTED.load(Ordering::Relaxed)
}

fn set_log_muted(muted: bool) {
    LOG_MUTED.store(muted, Ordering::Relaxed);
}

/// Assembles UART0 bytes into whole lines.
///
/// A ring of one line plus a flag, so a paste that is longer than
/// [`MAX_LINE_BYTES`] is discarded whole rather than truncated — the parser's
/// rule 4, enforced here at the point where the bytes actually arrive.
///
/// `dropping` is what makes that true. A reader that simply appended until the
/// buffer was full and then started overwriting would turn a 200-byte paste
/// into two lines, and the second of them could be a command.
pub struct LineReader {
    buf: [u8; MAX_LINE_BYTES],
    len: usize,
    dropping: bool,
    /// Complete lines waiting to be consumed.
    ///
    /// A `Vec`, which is a heap allocation per session rather than per byte.
    /// The alternative -- a callback -- makes the reader and the parser borrow
    /// each other, which the borrow checker refuses and which would be a real
    /// aliasing hazard even if it compiled. One `String` per line, in a task
    /// that exists only while an operator is typing, is not a cost worth
    /// optimising away.
    lines: Vec<String>,
}

impl LineReader {
    /// An empty reader.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: [0; MAX_LINE_BYTES],
            len: 0,
            dropping: false,
            lines: Vec::new(),
        }
    }

    /// Take the next complete line, if there is one.
    pub fn pop_line(&mut self) -> Option<String> {
        if self.lines.is_empty() {
            None
        } else {
            Some(self.lines.remove(0))
        }
    }

    /// Feed received bytes, queueing every complete line.
    ///
    /// Lines come back out through [`LineReader::pop_line`] rather than through
    /// a callback, because the caller that owns the reader also owns the
    /// parser, and a closure over both is a borrow the compiler is right to
    /// refuse.
    ///
    /// Bytes that are not valid UTF-8 cannot become a line: the protocol is
    /// ASCII plus whatever a WPA passphrase contains, and a line that is not
    /// UTF-8 is dropped with the rest of its line rather than guessed at.
    pub fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte == b'\n' {
                if self.dropping {
                    // The tail of a line already rejected for length. It is
                    // discarded, and the reader is back in sync.
                    self.dropping = false;
                    self.len = 0;
                    continue;
                }
                if let Ok(line) = core::str::from_utf8(&self.buf[..self.len]) {
                    self.lines.push(String::from(line));
                }
                self.len = 0;
                continue;
            }

            if self.dropping {
                continue;
            }
            if byte == b'\r' {
                // Cooked-mode terminator. Skipped here rather than at the end
                // so the reader's buffer holds exactly the line's bytes, and
                // `cc_domain::provisioning` does not have to strip it.
                continue;
            }
            if self.len >= MAX_LINE_BYTES {
                self.dropping = true;
                self.len = 0;
                continue;
            }
            self.buf[self.len] = byte;
            self.len += 1;
        }
    }
}

impl Default for LineReader {
    fn default() -> Self {
        Self::new()
    }
}

/// A credential the operator has typed but not yet applied.
#[derive(Clone, PartialEq, Eq)]
pub struct Pending {
    /// `system.wifi.ssid`.
    pub ssid: Secret<String>,
    /// `system.wifi.password`. Empty means an open network.
    pub password: Secret<String>,
}

impl core::fmt::Debug for Pending {
    /// Both fields redacted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Pending { ssid: [redacted], password: [redacted] }")
    }
}

/// The provisioning session: the parser, the pending credential, and the mute.
///
/// One per task, and the task exists only while there are no stored credentials
/// (04 §3.2: *"The provisioning task is only spawned when no valid credentials
/// exist, and it exits after success"*). It is not a permanent task.
pub struct Session {
    parser: Parser,
    reader: LineReader,
    pending: Option<Pending>,
    /// When the password window opened, or `None` when no window is open.
    ///
    /// The *open* time, not the deadline: see
    /// [`cc_domain::provisioning::password_window_expired`].
    window_opened_ms: Option<u32>,
    /// `wifi clear` has been typed and is waiting for `wifi apply`.
    ///
    /// Session state rather than task state, because the decision it feeds is
    /// made here: `wifi apply` means "write the credential" or "forget it", and
    /// only the session knows which of the two the operator asked for.
    cleared: bool,
    /// What the session wants the caller to do, per command.
    action: Action,
}

/// What a completed command asks the caller to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do.
    #[default]
    None,
    /// `wifi set` then `wifi apply`: store the pending credential.
    Set,
    /// `wifi clear` then `wifi apply`: forget the stored credential.
    Clear,
}

/// The reply text for a [`Reply`], credential-free by construction.
///
/// `describe()` is the only source of the text, and it returns a `&'static str`
/// naming the command. The password is never part of it — see
/// `cc_domain::provisioning`'s module documentation for why that is a property
/// of the types rather than of this function.
#[must_use]
pub fn reply_text(reply: Reply<'_>) -> String {
    if reply.should_reply() {
        format!("{REPLY_PREFIX}ok {}", reply.describe())
    } else {
        String::new()
    }
}

impl Session {
    /// A session waiting for its first command.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            reader: LineReader::new(),
            pending: None,
            window_opened_ms: None,
            cleared: false,
            action: Action::None,
        }
    }

    /// Whether a password line is expected next.
    ///
    /// True exactly while the log stream is muted, so a caller that wants to
    /// know whether it is safe to log can ask this instead of reading the flag.
    #[must_use]
    pub const fn awaiting_password(&self) -> bool {
        self.parser.awaiting_password()
    }

    /// The pending credential, if one has been typed and not yet applied.
    #[must_use]
    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    /// Take the pending credential, so the caller can write it to the store.
    pub fn take_pending(&mut self) -> Option<Pending> {
        self.pending.take()
    }

    /// Feed received UART bytes, and collect the replies to send back.
    ///
    /// The replies are **returned**, not written. Two reasons: the caller owns
    /// the UART driver, and a `log`-moted session must still be able to say
    /// "password accepted" — writing through `log` would lose exactly the line
    /// that matters most, and writing from inside the parser would mean the
    /// session owns a peripheral it does not otherwise need.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.reader.push(bytes);
        let mut replies: Vec<String> = Vec::new();
        while let Some(line) = self.reader.pop_line() {
            self.feed_line(&line, &mut replies);
        }
        replies
    }

    /// Feed one already-assembled line. Used by [`Session::push`] and by tests.
    pub fn feed_line(&mut self, line: &str, replies: &mut Vec<String>) {
        let reply = self.parser.feed(line);
        match reply {
            Reply::Ignored => {}
            Reply::Rejected(reason) => {
                replies.push(format!("{REPLY_PREFIX}err {reason}"));
            }
            Reply::Accepted(Accepted::Password { password, ssid }) => {
                // Both come back from the parser, because it has just disarmed
                // and there is no later point at which the SSID could be asked
                // for. Either line form arrives here: `wifi pass <password>`,
                // or the password positionally in the window `wifi set` opened.
                // The window is closed either way, so a script that used the
                // argument form never had one open in the first place.
                self.pending = Some(Pending {
                    ssid: Secret::new(String::from(ssid)),
                    password: Secret::new(String::from(password)),
                });
                // A credential typed after `wifi clear` supersedes it: the
                // operator changed their mind, and the alternative is that
                // `wifi apply` stores nothing.
                self.cleared = false;
                self.close_window();
                replies.push(format!("{REPLY_PREFIX}ok password accepted"));
            }
            Reply::Accepted(Accepted::SetSsid) => {
                self.open_window();
                replies.push(format!(
                    "{REPLY_PREFIX}ok ssid accepted — now `wifi pass <password>`, or the \
                     password on the next line"
                ));
            }
            Reply::Accepted(Accepted::Clear) => {
                self.pending = None;
                self.cleared = true;
                self.close_window();
                replies.push(format!(
                    "{REPLY_PREFIX}ok credentials cleared, apply to persist"
                ));
            }
            Reply::Accepted(Accepted::Status) => {
                replies.push(format!("{REPLY_PREFIX}ok {}", self.status_text()));
            }
            Reply::Accepted(Accepted::Apply) => {
                self.close_window();
                if self.pending.is_some() {
                    self.action = Action::Set;
                } else if self.cleared {
                    self.action = Action::Clear;
                } else {
                    // Without this the command is swallowed: the session holds
                    // nothing, so there is no action to set, and an operator who
                    // typed `wifi apply` on an unprovisioned machine would get
                    // silence and conclude the machine had hung.
                    // `wifi pass` needs an armed SSID, which the parser
                    // enforces; this arm is the "what now?" that follows.
                    self.action = Action::None;
                    replies.push(format!(
                        "{REPLY_PREFIX}err nothing to apply — `wifi set <ssid>` then \
                         `wifi pass <password>`, or `wifi clear`, then `wifi apply`"
                    ));
                }
            }
        }
    }

    /// The action the last command asked for, and clear it.
    pub fn take_action(&mut self) -> Action {
        core::mem::take(&mut self.action)
    }

    /// Close the password window if it has expired.
    ///
    /// Called once per poll. The timeout is the escape hatch for a mistyped
    /// `wifi set`, without which every later line — including `wifi clear` —
    /// would be eaten as a password.
    ///
    /// The comparison is [`cc_domain::provisioning::password_window_expired`]
    /// and not a deadline, for the reason that function documents: `wrapping_sub`
    /// of a *negative* difference is a number near `u32::MAX`, so the deadline
    /// form of this test is true immediately and the window lasted zero
    /// milliseconds.
    pub fn expire_window(&mut self) {
        if let Some(opened_ms) = self.window_opened_ms {
            if password_window_expired(opened_ms, now_ms()) {
                self.parser.cancel();
                self.window_opened_ms = None;
                self.close_window();
            }
        }
    }

    fn open_window(&mut self) {
        self.window_opened_ms = Some(now_ms());
        set_log_muted(true);
    }

    fn close_window(&mut self) {
        self.window_opened_ms = None;
        set_log_muted(false);
    }

    /// The `wifi status` line.
    ///
    /// Reports what is *configured*, never what it is called. An SSID is half of
    /// a credential pair and the console is a shared wire, so the status line
    /// says `ssid_set=yes` and stops there. This is stricter than the C++, which
    /// logs the SSID on every connection attempt
    /// (`CleverCoffeeWiFiManager.cpp:96-98`).
    fn status_text(&self) -> String {
        let has_ssid = self.pending.is_some() || self.parser.awaiting_password();
        let has_password = self
            .pending
            .as_ref()
            .is_some_and(|p| !p.password.expose().is_empty());
        format!(
            "status: ssid_set={has_ssid} password_set={has_password}              pending={} log_unmuted={}",
            self.pending.is_some(),
            !log_muted(),
        )
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a provisioning line for the console.
///
/// Returns the text rather than writing it, because the caller owns the UART
/// driver. Keeping the prefix here means there is one place that knows it.
#[must_use]
pub fn line(text: &str) -> String {
    format!("{REPLY_PREFIX}{text}\n")
}

/// Report that the provisioning task is running, and why.
///
/// `wifi status` is useful even when credentials *are* stored, so this prints
/// unconditionally at boot — but it prints through `info!`, which is muted
/// during a password window, so it cannot corrupt one.
pub fn announce() {
    info!(
        "serial: WiFi commands available. `wifi set <ssid>` then \
         `wifi pass <password>` (or the password on the next line), \
         `wifi clear`, `wifi status`, `wifi apply`."
    );
}

/// Report a fatal error in the provisioning path.
///
#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in eight import lists
    // that would rot.
    #![allow(clippy::wildcard_imports)]

    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    fn session() -> (Session, Vec<String>) {
        (Session::new(), Vec::new())
    }

    /// Drive a session with a sequence of lines and collect the replies.
    fn run(lines: &[&str]) -> Vec<String> {
        let (mut session, mut replies) = session();
        for line in lines {
            session.feed_line(line, &mut replies);
        }
        replies
    }

    #[cfg_attr(test, test)]
    pub fn a_log_line_produces_no_reply_at_all() {
        // The console is shared. A log line must be invisible to the protocol,
        // not merely harmless.
        for line in [
            "I (12345) cc_firmware: heating",
            "W (6) cc_firmware::wifi: retrying",
            "D (99) cc_hal_esp32: tick",
        ] {
            assert!(run(&[line]).is_empty(), "{line} produced a reply");
        }
    }

    #[cfg_attr(test, test)]
    pub fn every_reply_is_prefixed_so_a_script_can_match_it() {
        for reply in run(&["wifi status", "wifi set", "wifi nonsense"]) {
            assert!(reply.starts_with(REPLY_PREFIX), "{reply}");
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_argument_form_stages_the_credential_with_no_next_line() {
        // The form the provisioning script uses. There is no window: the two
        // commands are adjacent, so nothing depends on the next line arriving
        // inside 30 s or on the console task's poll rate.
        let (mut session, mut replies) = session();
        session.feed_line("wifi set mynet", &mut replies);
        assert!(session.awaiting_password());
        session.feed_line("wifi pass hunter2", &mut replies);
        assert!(!session.awaiting_password());
        assert!(!log_muted(), "the log must come back after the password");
        assert!(
            replies
                .last()
                .is_some_and(|r| r.contains("password accepted")),
            "{replies:?}"
        );
        let pending = session.take_pending().expect("a pending credential");
        assert_eq!(pending.ssid.expose(), "mynet");
        assert_eq!(pending.password.expose(), "hunter2");
        session.feed_line("wifi apply", &mut replies);
        assert_eq!(session.take_action(), Action::Set);
    }

    #[cfg_attr(test, test)]
    pub fn the_next_line_form_still_works() {
        // The hand-typed form is not replaced by the argument form, only
        // complemented: an operator typing by hand must not be broken by a
        // change made to suit a script.
        let (mut session, mut replies) = session();
        session.feed_line("wifi set mynet", &mut replies);
        session.feed_line("hunter2", &mut replies);
        let pending = session.take_pending().expect("a pending credential");
        assert_eq!(pending.password.expose(), "hunter2");
    }

    #[cfg_attr(test, test)]
    pub fn the_password_window_opens_and_closes() {
        let (mut session, mut replies) = session();
        session.feed_line("wifi set mynet", &mut replies);
        assert!(session.awaiting_password());
        assert!(log_muted(), "the log must be muted for the password window");

        session.feed_line("hunter2", &mut replies);
        assert!(!session.awaiting_password());
        assert!(!log_muted(), "the log must come back after the password");
    }

    #[cfg_attr(test, test)]
    pub fn the_password_never_appears_in_a_reply() {
        let replies = run(&["wifi set myhomenetwork", "correcthorsebattery"]);
        for reply in &replies {
            assert!(!reply.contains("correcthorsebattery"), "{reply}");
            assert!(!reply.contains("myhomenetwork"), "{reply}");
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_ssid_is_never_echoed_even_in_the_set_reply() {
        let replies = run(&["wifi set myhomenetwork"]);
        assert!(!replies[0].contains("myhomenetwork"), "{}", replies[0]);
        assert!(replies[0].contains("ssid accepted"), "{}", replies[0]);
    }

    #[cfg_attr(test, test)]
    pub fn a_pending_credential_is_handed_over_not_printed() {
        let (mut session, mut replies) = session();
        session.feed_line("wifi set mynet", &mut replies);
        session.feed_line("hunter2", &mut replies);
        let pending = session.take_pending().expect("a pending credential");
        assert_eq!(pending.ssid.expose(), "mynet");
        assert_eq!(pending.password.expose(), "hunter2");
        assert!(!format!("{pending:?}").contains("hunter2"));
        assert!(!format!("{pending:?}").contains("mynet"));
    }

    #[cfg_attr(test, test)]
    pub fn apply_asks_for_a_set_when_there_is_a_credential() {
        let (mut session, mut replies) = session();
        // Nothing staged: refused *and* said so. Without the reply the operator
        // types `wifi apply` on an unprovisioned machine and gets silence, which
        // reads as a hang.
        session.feed_line("wifi apply", &mut replies);
        assert_eq!(session.take_action(), Action::None);
        assert!(
            replies
                .last()
                .is_some_and(|r| r.contains("nothing to apply")),
            "{replies:?}"
        );

        replies.clear();
        session.feed_line("wifi set mynet", &mut replies);
        session.feed_line("hunter2", &mut replies);
        session.feed_line("wifi apply", &mut replies);
        assert_eq!(session.take_action(), Action::Set);
        // And it is one-shot: a caller that does not act does not get it twice.
        assert_eq!(session.take_action(), Action::None);
    }

    #[cfg_attr(test, test)]
    pub fn clear_then_apply_asks_for_the_clear_and_not_the_set() {
        // The two-step form. `wifi clear` drops the staged credential and arms
        // the clear; `wifi apply` then has nothing to set, so the session's own
        // state is the only thing that can say which of the two was meant.
        let (mut session, mut replies) = session();
        session.feed_line("wifi set mynet", &mut replies);
        session.feed_line("hunter2", &mut replies);
        session.feed_line("wifi clear", &mut replies);
        assert!(session.pending().is_none());
        // `wifi clear` on its own does not act; the reply says so.
        assert_eq!(session.take_action(), Action::None);
        assert!(
            replies
                .last()
                .is_some_and(|r| r.contains("apply to persist")),
            "{replies:?}"
        );

        session.feed_line("wifi apply", &mut replies);
        assert_eq!(session.take_action(), Action::Clear);
    }

    #[cfg_attr(test, test)]
    pub fn a_new_credential_supersedes_an_armed_clear() {
        let (mut session, mut replies) = session();
        session.feed_line("wifi clear", &mut replies);
        session.feed_line("wifi set mynet", &mut replies);
        session.feed_line("hunter2", &mut replies);
        session.feed_line("wifi apply", &mut replies);
        assert_eq!(
            session.take_action(),
            Action::Set,
            "the operator typed a credential after the clear; the clear is stale"
        );
    }

    #[cfg_attr(test, test)]
    pub fn status_names_no_credential() {
        let replies = run(&["wifi set myhomenetwork", "hunter2", "wifi status"]);
        let status = replies.last().expect("a status reply");
        assert!(status.contains("ssid_set=true"), "{status}");
        assert!(status.contains("password_set=true"), "{status}");
        assert!(!status.contains("mynet"), "{status}");
        assert!(!status.contains("hunter2"), "{status}");
    }

    // ---- the line reader ------------------------------------------------

    #[cfg_attr(test, test)]
    pub fn a_reader_assembles_lines_from_arbitrary_chunk_boundaries() {
        let mut reader = LineReader::new();
        reader.push(b"wifi ");
        reader.push(b"status\nwifi clear\n");
        let mut seen = Vec::new();
        while let Some(line) = reader.pop_line() {
            seen.push(line);
        }
        assert_eq!(seen, vec!["wifi status", "wifi clear"]);
    }

    #[cfg_attr(test, test)]
    pub fn a_reader_discards_an_over_long_line_whole() {
        // The reader is where rule 4 is actually enforced, because this is where
        // the bytes arrive. A truncated paste would produce a short line that
        // looks like a valid command.
        let mut reader = LineReader::new();
        let mut paste = "x".repeat(MAX_LINE_BYTES + 20);
        paste.push_str("\nwifi clear\n");
        reader.push(paste.as_bytes());
        let mut seen = Vec::new();
        while let Some(line) = reader.pop_line() {
            seen.push(line);
        }
        assert_eq!(seen, vec!["wifi clear"], "the over-long line must vanish");
    }

    #[cfg_attr(test, test)]
    pub fn a_reader_strips_carriage_returns() {
        let mut reader = LineReader::new();
        reader.push(b"wifi status\r\n");
        assert_eq!(reader.pop_line().as_deref(), Some("wifi status"));
    }

    #[cfg_attr(test, test)]
    pub fn a_reader_keeps_a_passwords_own_spaces() {
        let mut reader = LineReader::new();
        reader.push(b"wifi set n\r\n  pass word  \r\n");
        let first = reader.pop_line();
        let second = reader.pop_line();
        assert_eq!(first.as_deref(), Some("wifi set n"));
        assert_eq!(second.as_deref(), Some("  pass word  "));
    }

    #[cfg_attr(test, test)]
    pub fn a_reader_drops_a_line_that_is_not_utf8() {
        let mut reader = LineReader::new();
        reader.push(&[
            0xff, 0xfe, b'\n', b'w', b'i', b'f', b'i', b' ', b's', b't', b'a', b't', b'u', b's',
            b'\n',
        ]);
        // The first line is not UTF-8 and is dropped with the rest of ITS line,
        // not with the next one.
        assert_eq!(reader.pop_line().as_deref(), Some("wifi status"));
    }
}
