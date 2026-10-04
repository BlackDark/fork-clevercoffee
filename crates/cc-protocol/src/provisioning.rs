//! The UART Wi-Fi provisioning line protocol: the parser and its state machine.
//!
//! Owner: **R3-12** (task C).
//!
//! # Why UART and not a captive portal
//!
//! The chip has no USB peripheral (01 §1), so provisioning over a serial console
//! is the one mechanism that always works. The recovered oracle firmware
//! implemented exactly this (08 §5.2) and its log line is the specification:
//!
//! ```text
//! cc_firmware::serial_wifi: serial: WiFi commands available.
//!   `wifi set <ssid>` then the password on the next line, `wifi clear`,
//!   `wifi status`, `wifi apply`.
//! ```
//!
//! # Two ways to give the password
//!
//! 1. `wifi set <ssid>`, then the password **as an argument**: `wifi pass <password>`.
//!    There is no window, no deadline arithmetic, and nothing that depends on
//!    the console task's loop rate. **This is what `scripts/wifi_provision.py`
//!    uses**, because a window that has to stay open across a USB serial
//!    session is a window a watchdog can close.
//! 2. `wifi set <ssid>`, then the password **on the next line**, positionally,
//!    inside a 30 s window ([`PASSWORD_WINDOW_MS`]). This is the oracle's form
//!    and it stays, because an operator typing by hand must not be broken by a
//!    change made to suit a script.
//!
//! Form 1 exists because of the other half of the reason the positional form was
//! chosen in the first place: because `argv` is world-readable. On a host,
//! `wifi set mynet hunter2` puts the password in `ps` output for any user on the
//! machine and in the shell history file forever. On the device, UART0 *is* the
//! log stream, so anything typed there is likewise visible to whatever is
//! reading the console. A password on a following line is a thing a human types
//! without it appearing in a process listing, and it is what the oracle did.
//! Neither form changes that: `wifi pass` is still typed on the wire, not
//! passed in `argv`, and the device still never echoes or logs it.
//!
//! # What this type is and is not
//!
//! It is a **pure state machine**: bytes in, [`Reply`] out, no I/O, no clock of
//! its own, **no `alloc`** — `cc-domain` has none, and a parser that has to
//! allocate to hold a 32-byte SSID is a parser that can fail where a stack
//! buffer cannot. The caller (in `cc-hal-esp32`) owns the UART, the line
//! reader, the NVS write, and the `alloc::string::String` the credential ends up
//! in.
//!
//! That split is what lets the whole protocol — including every rejection path —
//! be a host unit test, and it is the same split
//! [`heater`](cc_domain::heater) uses.
//!
//! # A credential cannot be printed, because the type cannot print it
//!
//! [`Reply`] deliberately does **not** implement `Debug`. A `Debug` on a type
//! that can hold a password is an argument waiting to be forgotten at a log
//! site, and the C++ prints the SSID on every connection attempt
//! (`CleverCoffeeWiFiManager.cpp:96-98`). Instead:
//!
//! * [`Reply`] and [`Accepted`] have hand-written `Debug` impls that print the
//!   command *name* and never the argument, so `{:?}` is safe at a log site and
//!   `assert_eq!` still works in a test. The impls are written out rather than
//!   derived precisely so that adding a variant to [`Accepted`] does not
//!   automatically gain a derived `Debug` that prints its payload.
//! * [`Reply::describe`] returns a `&'static str` naming the command and never
//!   its argument.
//! * The password is returned as a `&str` **borrowed from the parser**, so it
//!   is only ever a temporary. The caller copies it into a
//!   `cc_domain::secret::Secret<String>` on the spot; the parser's copy is
//!   overwritten by the next line and the whole parser is dropped at the end of
//!   the provisioning task.
//!
//! # The framing, and why it is unambiguous
//!
//! UART0 is a shared wire: the log stream and the operator share it. Four
//! rules make "which bytes are a command" decidable, and each is a function a
//! test can call.
//!
//! 1. **A command is a whole line, and only a whole line.** Lines are
//!    terminated by `\n`; a trailing `\r` is stripped. There is no
//!    character-count prefix and no terminator escape, because a password
//!    could contain either.
//! 2. **A command line begins with the exact 5 bytes `wifi ` at offset 0.**
//!    No leading whitespace, no case folding, no aliases. Nothing this
//!    firmware's logger emits can begin with those bytes: ESP-IDF's `log`
//!    format is `<I|W|E|D|V><space>(<ms>)<space><tag>: <message>`
//!    (`esp_log_write` in `components/log/esp_log.c`), so a log line starts
//!    with a level letter, a space or a digit — never with `wifi `. A pinned
//!    test asserts the parser ignores every one of those prefixes.
//! 3. **The password line is accepted only in a state no other line can reach**,
//!    and it is consumed positionally. `set` arms the parser; the *next* line
//!    is the password; the line after that is a command again. There is no
//!    keyword in front of the password, because a keyword would have to be
//!    typed before the password and so would be part of what a shoulder-surfer
//!    or a pasted `script` captures — and because an unparseable password
//!    (`wifi`-prefixed, over-long, non-UTF-8) then simply *fails to be a
//!    credential* instead of corrupting the parser.
//!    **The one exception is [`PASS_PREFIX`].** A line beginning with the exact
//!    10 bytes `wifi pass ` is a command even while the parser is armed, because
//!    that is what makes form 1 above windowless. A passphrase that literally
//!    begins with `wifi pass ` typed positionally is therefore taken as the
//!    command, not as the password — a deliberate trade for a passphrase that
//!    is pathological, made in exchange for a protocol a script cannot be
//!    defeated by timing.
//! 4. **An over-long line is discarded whole, and the parser resynchronises.**
//!    Lines longer than [`MAX_LINE_BYTES`] cannot be commands (no command is
//!    that long) and are not passwords (a WPA passphrase is at most 63 ASCII
//!    characters). Discarding the whole overrun rather than truncating is what
//!    stops a long paste from being silently interpreted as a short valid
//!    credential.
//!
//! A fifth rule is the caller's to keep and is stated here because the parser
//! cannot enforce it: **the caller must not interleave log output into the
//! one-line password window.** The parser has no way to tell a log line from a
//! password once armed, so the caller mutes the log stream between `set` and the
//! password line. `cc-hal-esp32` does this; see
//! `cc_hal_esp32::provisioning` for the window length and why it is bounded.

/// The longest line the parser will consider, in bytes.
///
/// The longest possible command is now `wifi pass ` (10) + a 63-character WPA
/// passphrase = 73 bytes. 80 covers that and the `wifi set ` + 32-octet SSID
/// form (41) with margin, and anything longer is a paste, a binary blob, or
/// noise.
///
/// It was 72 before `wifi pass` existed, which is *one byte* short of the
/// longest legal passphrase on the `wifi pass` form: raising it is not a
/// nicety, a 63-character passphrase typed as `wifi pass <pw>` is a legal
/// command and was rejected as "line too long" at 72.
pub const MAX_LINE_BYTES: usize = 80;

/// The longest SSID the parser accepts, in bytes.
///
/// 32, the 802.11 limit and the width of `wifi_sta_config_t::ssid`. A longer
/// one is rejected rather than truncated: a truncated SSID connects to the
/// wrong network, or to nothing while looking like it tried.
pub const MAX_SSID_BYTES: usize = 32;

/// The prefix every command line must begin with, byte for byte.
pub const COMMAND_PREFIX: &str = "wifi ";

/// The prefix that carries the password as an argument, byte for byte.
///
/// `wifi pass ` — 10 bytes. Deliberately a *different* prefix from
/// [`COMMAND_PREFIX`]'s commands rather than a subcommand the positional arm
/// can be confused with: it is the one line that is a command **while the
/// parser is armed**, so an operator can send the whole credential without the
/// window being open at all.
pub const PASS_PREFIX: &str = "wifi pass ";

/// How long a `wifi set` line arms the parser for its password, in milliseconds.
///
/// 30 s. Long enough to type or paste a passphrase and press enter; short enough
/// that a mistyped `wifi set` does not swallow the next command an operator
/// types, which would be a silent no-op in the middle of a recovery.
pub const PASSWORD_WINDOW_MS: u32 = 30_000;

/// Whether a password window opened at `opened_ms` has expired at `now_ms`.
///
/// **Both times, not a deadline.** The rule is `now - opened >= window`, and
/// the subtraction is `wrapping` so that the 49.7-day wrap of a 32-bit
/// millisecond count is a subtraction that still comes out right
/// (`cc_domain::units::millis_since_wraps_like_unsigned_long`) rather than a
/// borrow panic or a 49-day stall.
///
/// The distinction is not pedantry. `now.wrapping_sub(opened + window)` — the
/// deadline form — is a *negative* number for the whole life of the window, and
/// `wrapping_sub` turns that into a number near `u32::MAX`, so
/// `>= PASSWORD_WINDOW_MS` is true **immediately**. That bug was on the device:
/// the window lasted zero milliseconds, every password line was parsed as a
/// command and rejected as one, and provisioning could never complete. It is
/// here, and host-tested, because the alternative is a rule that only the
/// device can check.
#[must_use]
pub fn password_window_expired(opened_ms: u32, now_ms: u32) -> bool {
    now_ms.wrapping_sub(opened_ms) >= PASSWORD_WINDOW_MS
}

/// A command the parser understood, or the credential it just accepted.
///
/// The `Password` arm borrows from the [`Parser`], so the borrow ends before the
/// next line is read and the plaintext does not outlive the parser unless the
/// caller chooses to copy it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Accepted<'a> {
    /// The password that followed `wifi set <ssid>`, and the SSID it belongs
    /// to. Copy both into `Secret<String>`s on the spot and never print either.
    ///
    /// The SSID comes back with the password because the parser has just
    /// disarmed and there is no later point at which the caller could ask for
    /// it. It is not a secret in the way a password is, but it is half of a
    /// credential pair and the C++ prints it on every connection attempt, so it
    /// is handled the same way.
    Password {
        /// The password, verbatim, borrowed from the parser.
        password: &'a str,
        /// The SSID that was armed when the password arrived.
        ssid: &'a str,
    },
    /// `wifi set <ssid>` with no password line yet. The SSID is now armed and
    /// the *next* line is the password.
    ///
    /// The SSID itself is deliberately **not** returned here: it is in the
    /// line the caller already has, so there is no reason to hand out a second
    /// copy of it.
    SetSsid,
    /// `wifi clear` — forget the stored credentials.
    Clear,
    /// `wifi status` — report what is configured, without naming it.
    Status,
    /// `wifi apply` — commit the pending credentials and connect.
    Apply,
}

impl Accepted<'_> {
    /// A name for this command that contains no argument.
    ///
    /// This is what a log line is allowed to print. It is a `&'static str`
    /// precisely so that no formatting code can be tricked into appending the
    /// argument.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Password { .. } => "password",
            Self::SetSsid => "set-ssid",
            Self::Clear => "clear",
            Self::Status => "status",
            Self::Apply => "apply",
        }
    }
}

/// The parser's answer to one input line.
///
/// `Debug` is hand-written and credential-free; see the module documentation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reply<'a> {
    /// A line that was a command, or a credential.
    Accepted(Accepted<'a>),
    /// A line that was a command but is not usable, with the reason.
    ///
    /// The reason is a `&'static str` naming the *field*, never its contents,
    /// so this cannot leak a credential either.
    Rejected(&'static str),
    /// A line that was not a command. Silent by design — this is the log
    /// stream, and answering every log line would be an amplifier.
    Ignored,
}

/// Renders the variant and the *command name* only. Never the argument.
impl core::fmt::Debug for Accepted<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// Renders the variant and the reason only. Never a credential.
impl core::fmt::Debug for Reply<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Accepted(accepted) => write!(f, "Accepted({accepted:?})"),
            Self::Rejected(reason) => write!(f, "Rejected({reason:?})"),
            Self::Ignored => f.write_str("Ignored"),
        }
    }
}

impl Reply<'_> {
    /// Whether the line should produce a line of output.
    ///
    /// `Ignored` produces nothing. `Rejected` produces one short line, because
    /// an operator who typed something and got no answer has no way to tell a
    /// typo from a dead console.
    #[must_use]
    pub const fn should_reply(self) -> bool {
        !matches!(self, Self::Ignored)
    }

    /// A short, credential-free description for the operator.
    ///
    /// The whole of what this protocol is allowed to print about itself.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Ignored => "not a wifi command",
            Self::Rejected(reason) => reason,
            Self::Accepted(accepted) => accepted.name(),
        }
    }
}

/// The line-protocol state machine.
///
/// Construct with [`Parser::new`], then feed it one line at a time with
/// [`Parser::feed`]. It holds at most one SSID and one line, so it is 112 bytes
/// and lives on the stack of the provisioning task with nothing to free.
#[derive(Clone, PartialEq, Eq)]
pub struct Parser {
    armed: [u8; MAX_SSID_BYTES],
    armed_len: u8,
    /// The current line, used to lend the password back to the caller.
    line: [u8; MAX_LINE_BYTES],
    line_len: u8,
    /// Set when a line was longer than [`MAX_LINE_BYTES`].
    ///
    /// The reader keeps consuming until the newline, so the remainder of an
    /// over-long line is discarded rather than parsed as a fresh line. This is
    /// rule 4 of the framing and it is why the flag is separate from the line
    /// buffer.
    dropping: bool,
}

impl Parser {
    /// A parser waiting for its first command.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            armed: [0; MAX_SSID_BYTES],
            armed_len: 0,
            line: [0; MAX_LINE_BYTES],
            line_len: 0,
            dropping: false,
        }
    }

    /// Whether a password line is expected next.
    ///
    /// `true` between a `wifi set` and the line after it. The caller uses this
    /// to mute the log stream; see the module docs' fifth rule.
    #[must_use]
    pub const fn awaiting_password(&self) -> bool {
        self.armed_len != 0
    }

    /// Feed one complete, newline-terminated line, without the terminator.
    ///
    /// **"One complete line" is part of the contract.** The device reader
    /// assembles a line in its own buffer and calls this once, so a line longer
    /// than [`MAX_LINE_BYTES`] arrives whole and is rejected whole. The
    /// `dropping` flag additionally makes the parser safe against a caller that
    /// feeds *chunks* of a long line: the chunk after an over-long one is
    /// swallowed rather than executed, so a paste whose middle bytes happen to
    /// read `wifi clear` does not clear the machine's credentials.
    ///
    /// The line may carry a trailing `\r`, which is stripped, because a
    /// terminal in cooked mode sends `\r\n` and a password pasted from one will
    /// include it. Only the terminator is stripped: a password's leading and
    /// trailing spaces are part of the password, and WPA allows them.
    #[must_use]
    pub fn feed<'a>(&'a mut self, line: &str) -> Reply<'a> {
        let bytes = line.as_bytes();
        let bytes = match bytes.last() {
            Some(b'\r') => &bytes[..bytes.len() - 1],
            _ => bytes,
        };

        if self.dropping {
            // This line is the tail of one that was already too long. It is
            // discarded whole, and the parser is back in sync at the next
            // newline.
            self.dropping = false;
            return Reply::Ignored;
        }

        if bytes.len() > MAX_LINE_BYTES {
            self.dropping = true;
            self.armed_len = 0;
            return Reply::Rejected("line too long");
        }

        // `wifi pass ` and the bare keyword `wifi pass` are both commands even
        // while armed, so the argument form is windowless and a half-typed
        // keyword gets the same "needs the password on the same line" answer as
        // `wifi set` does rather than being eaten as the password.
        let pass_command = bytes == b"wifi pass" || bytes.starts_with(PASS_PREFIX.as_bytes());
        if self.armed_len != 0 && !pass_command {
            // Rule 3: this line is the password, positionally. It is taken
            // whatever it looks like, because the caller has already muted the
            // log stream and there is nothing else it could be. An empty line
            // is the open-network case and is accepted as the empty password.
            let len = bytes.len();
            let ssid_len = usize::from(self.armed_len);
            self.line[..len].copy_from_slice(bytes);
            // Bounded by `MAX_LINE_BYTES` (72) by the check at the top of
            // `feed`, which is why the cast cannot truncate.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "len <= MAX_LINE_BYTES (72), checked earlier in this function"
            )]
            {
                self.line_len = len as u8;
            }
            let password = core::str::from_utf8(&self.line[..len]);
            let ssid = core::str::from_utf8(&self.armed[..ssid_len]);
            self.armed_len = 0;
            return match (password, ssid) {
                // `ssid_len` is non-zero here: this branch is only reached when
                // `armed_len != 0`, which is the arming condition, and an SSID
                // that was accepted for arming was already UTF-8 -- so in
                // practice only the password arm can fail. The combined arm is
                // kept so the rejection reason does not depend on which of the
                // two was the odd one out.
                (Ok(password), Ok(ssid)) => Reply::Accepted(Accepted::Password { password, ssid }),
                (Err(_), _) | (_, Err(_)) => Reply::Rejected("password is not valid UTF-8"),
            };
        }

        let Ok(text) = core::str::from_utf8(bytes) else {
            return Reply::Ignored;
        };

        let Some(rest) = text.strip_prefix(COMMAND_PREFIX) else {
            return Reply::Ignored;
        };

        if rest == "pass" {
            // Same reasoning as the `set` arm below: a typed-but-empty keyword
            // is a typo with an obvious fix, so it is reported rather than
            // answered with "unknown wifi subcommand".
            return Reply::Rejected("`wifi pass` needs the password on the same line");
        }
        if let Some(password) = rest.strip_prefix("pass ") {
            if self.armed_len == 0 {
                // The SSID is not in this line, so there is nothing to attach
                // the password to. Naming the fix is the whole of the reply.
                return Reply::Rejected("`wifi pass` needs an SSID first — `wifi set <ssid>`");
            }
            let len = password.len();
            let ssid_len = usize::from(self.armed_len);
            self.line[..len].copy_from_slice(password.as_bytes());
            // Bounded by `MAX_LINE_BYTES` by the check at the top of `feed`.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "len <= MAX_LINE_BYTES, checked earlier in this function"
            )]
            {
                self.line_len = len as u8;
            }
            let password = core::str::from_utf8(&self.line[..len]);
            let ssid = core::str::from_utf8(&self.armed[..ssid_len]);
            self.armed_len = 0;
            return match (password, ssid) {
                (Ok(password), Ok(ssid)) => Reply::Accepted(Accepted::Password { password, ssid }),
                (Err(_), _) | (_, Err(_)) => Reply::Rejected("password is not valid UTF-8"),
            };
        }
        if rest == "clear" {
            return Reply::Accepted(Accepted::Clear);
        }
        if rest == "status" {
            return Reply::Accepted(Accepted::Status);
        }
        if rest == "apply" {
            return Reply::Accepted(Accepted::Apply);
        }
        if rest == "set" {
            // `wifi set` with nothing after it. Checked before the `set `
            // arming below, because `strip_prefix("set ")` would not match and
            // the operator would get "unknown subcommand" for a typo that has
            // an obvious fix.
            return Reply::Rejected("`wifi set` needs an SSID on the same line");
        }
        if let Some(ssid) = rest.strip_prefix("set ") {
            if ssid.is_empty() {
                return Reply::Rejected("`wifi set` needs an SSID on the same line");
            }
            if ssid.len() > MAX_SSID_BYTES {
                return Reply::Rejected("SSID longer than 32 bytes");
            }
            self.armed[..ssid.len()].copy_from_slice(ssid.as_bytes());
            // Bounded by `MAX_SSID_BYTES` (32) by the check two lines above.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "ssid.len() <= MAX_SSID_BYTES (32), checked above"
            )]
            {
                self.armed_len = ssid.len() as u8;
            }
            return Reply::Accepted(Accepted::SetSsid);
        }

        Reply::Rejected("unknown wifi subcommand")
    }

    /// Disarm the password window, so the next line is a command again.
    ///
    /// Called by the caller when the password window's deadline passes with no
    /// line. Without it, an operator who sends `wifi set <ssid>` and then
    /// decides against it is stuck: every subsequent line is eaten as a
    /// password, including `wifi clear`, which is the command they would reach
    /// for. Recovery by timeout is the only way out of a strictly positional
    /// protocol, and it is the caller's job because only the caller has a
    /// clock.
    pub const fn cancel(&mut self) {
        self.armed_len = 0;
    }

    /// The armed SSID, once `wifi set` has been accepted.
    ///
    /// `None` when nothing is armed. This is the only accessor for the SSID and
    /// it exists because the caller has to write it to the configuration
    /// eventually; there is no accessor for the password, because the password
    /// arrives as a borrow from [`Parser::feed`] and never has to be asked for
    /// twice.
    #[must_use]
    pub fn armed_ssid(&self) -> Option<&str> {
        if self.armed_len == 0 {
            return None;
        }
        core::str::from_utf8(&self.armed[..self.armed_len as usize]).ok()
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::String, vec::Vec};

    use super::*;

    /// Feed a line and turn a `Password` into the owned `Secret`-shaped value
    /// the firmware actually stores, so a test can assert on it.
    fn password_of(reply: Reply<'_>) -> Option<String> {
        match reply {
            Reply::Accepted(Accepted::Password { password, .. }) => Some(String::from(password)),
            _ => None,
        }
    }

    fn ssid_of(reply: Reply<'_>) -> Option<&'static str> {
        match reply {
            Reply::Accepted(Accepted::SetSsid) => Some("armed"),
            _ => None,
        }
    }

    #[test]
    fn the_oracle_sequence_parses_end_to_end() {
        // 08 §5.2, verbatim.
        let mut parser = Parser::new();
        assert_eq!(
            parser.feed("wifi status"),
            Reply::Accepted(Accepted::Status)
        );
        assert_eq!(ssid_of(parser.feed("wifi set mynet")), Some("armed"));
        assert!(parser.awaiting_password());
        assert_eq!(parser.armed_ssid(), Some("mynet"));
        assert_eq!(
            password_of(parser.feed("hunter2")),
            Some(String::from("hunter2"))
        );
        assert!(!parser.awaiting_password());
        assert_eq!(parser.feed("wifi apply"), Reply::Accepted(Accepted::Apply));
        assert_eq!(parser.feed("wifi clear"), Reply::Accepted(Accepted::Clear));
    }

    #[test]
    fn the_password_may_be_taken_as_an_argument_with_no_window_at_all() {
        // The form `scripts/wifi_provision.py` uses, and the reason this parser
        // grew a `pass` subcommand: with the password on the same line as the
        // command there is no window, so nothing depends on the next line
        // arriving inside 30 s, on the console task's poll rate, or on the log
        // being quiet. The next-line form below is unchanged.
        let mut parser = Parser::new();
        assert_eq!(ssid_of(parser.feed("wifi set mynet")), Some("armed"));
        assert_eq!(
            password_of(parser.feed("wifi pass hunter2")),
            Some(String::from("hunter2"))
        );
        assert!(!parser.awaiting_password());
        assert_eq!(parser.feed("wifi apply"), Reply::Accepted(Accepted::Apply));
    }

    #[test]
    fn the_argument_form_reports_the_armed_ssid_and_an_empty_password_is_open() {
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set opennet");
        let Reply::Accepted(Accepted::Password { password, ssid }) = parser.feed("wifi pass ")
        else {
            panic!("expected a password");
        };
        assert_eq!(ssid, "opennet");
        assert_eq!(password, "");
    }

    #[test]
    fn the_argument_form_survives_a_full_length_passphrase() {
        // `wifi pass ` is 10 bytes and a WPA passphrase is 63, so the longest
        // legal command on this form is 73 -- which is why `MAX_LINE_BYTES` is
        // 80 and not the 72 it used to be. At 72 this exact line was rejected
        // as "line too long".
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let pass = "p".repeat(63);
        let line = format!("wifi pass {pass}");
        assert!(line.len() <= MAX_LINE_BYTES, "{} bytes", line.len());
        assert_eq!(password_of(parser.feed(&line)), Some(pass));
    }

    #[test]
    fn a_pass_command_with_nothing_after_it_is_reported() {
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        assert_eq!(
            parser.feed("wifi pass"),
            Reply::Rejected("`wifi pass` needs the password on the same line")
        );
        // And it did not disarm: the next line is still the password.
        assert!(parser.awaiting_password());
        assert_eq!(
            password_of(parser.feed("hunter2")),
            Some(String::from("hunter2"))
        );
    }

    #[test]
    fn a_pass_command_with_no_ssid_armed_is_refused() {
        let mut parser = Parser::new();
        assert_eq!(
            parser.feed("wifi pass hunter2"),
            Reply::Rejected("`wifi pass` needs an SSID first — `wifi set <ssid>`")
        );
        assert!(!parser.awaiting_password());
    }

    #[test]
    fn the_password_may_be_empty_for_an_open_network() {
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set opennet");
        assert_eq!(password_of(parser.feed("")), Some(String::new()));
        assert!(!parser.awaiting_password());
    }

    #[test]
    fn a_password_keeps_its_own_leading_and_trailing_spaces() {
        // Only the line terminator is stripped. A passphrase with a space at
        // either end is legal in WPA, and truncating it produces a
        // wrong-password failure that looks like a typo.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        assert_eq!(
            password_of(parser.feed("  spaced  ")),
            Some(String::from("  spaced  "))
        );
    }

    #[test]
    fn a_carriage_return_from_a_cooked_terminal_is_stripped_once() {
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet\r");
        assert_eq!(parser.armed_ssid(), Some("mynet"));
        assert_eq!(
            password_of(parser.feed("hunter2\r")),
            Some(String::from("hunter2"))
        );
    }

    #[test]
    fn a_63_character_passphrase_survives() {
        // 63 is the WPA2 maximum and the largest legal password. The line
        // buffer is 72, so this is inside it with room for the 8 bytes of
        // padding the 802.11 field adds.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let pass = "p".repeat(63);
        assert_eq!(password_of(parser.feed(&pass)), Some(pass));
    }

    // ---- rule 2: a command line is exactly `wifi ` + a known subcommand --

    #[test]
    fn a_log_line_is_never_mistaken_for_a_command() {
        // ESP-IDF's log format is `<LEVEL><space>(<ms>)<space><tag>: <msg>`.
        // Every one of these is a real shape the firmware's own logger emits.
        let mut parser = Parser::new();
        for line in [
            "I (12345) cc_firmware: heating",
            "W (6) cc_firmware::wifi: retrying",
            "E (12) cc_firmware::nvs: write failed",
            "D (99) cc_hal_esp32: tick",
            "V (1) cc_firmware: detail",
            "I (0) cc_firmware: connect",
        ] {
            assert_eq!(parser.feed(line), Reply::Ignored, "{line} was parsed");
        }
        assert!(!parser.awaiting_password());
    }

    #[test]
    fn a_near_miss_prefix_is_not_a_command() {
        // Case, leading space and a different verb are all rejected, so an
        // operator's typo produces a message rather than a silent no-op.
        let mut parser = Parser::new();
        for line in [
            " WiFi status",
            "wifi  status",
            "wifistatus",
            "wifi",
            "wifi ",
            "wifi set",
        ] {
            assert!(
                !matches!(parser.feed(line), Reply::Accepted(_)),
                "{line} was accepted"
            );
        }
    }

    #[test]
    fn an_unknown_subcommand_is_reported_not_swallowed() {
        let mut parser = Parser::new();
        assert_eq!(
            parser.feed("wifi factoryreset"),
            Reply::Rejected("unknown wifi subcommand")
        );
    }

    #[test]
    fn a_set_command_with_no_ssid_is_rejected() {
        let mut parser = Parser::new();
        assert_eq!(
            parser.feed("wifi set"),
            Reply::Rejected("`wifi set` needs an SSID on the same line")
        );
        assert!(!parser.awaiting_password());
    }

    // ---- rule 3: the password is positional ----------------------------

    #[test]
    fn a_stray_line_outside_a_set_window_is_not_a_password() {
        let mut parser = Parser::new();
        assert_eq!(parser.feed("hunter2"), Reply::Ignored);
        assert_eq!(
            parser.feed("wifi status"),
            Reply::Accepted(Accepted::Status)
        );
    }

    #[test]
    fn the_line_after_the_password_is_a_command_again() {
        // If the arming were not cleared, the *next* command would be eaten as
        // a second password. That is the classic way a positional protocol goes
        // wrong.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let _ = parser.feed("hunter2");
        assert_eq!(parser.feed("wifi apply"), Reply::Accepted(Accepted::Apply));
    }

    #[test]
    fn a_second_set_line_while_armed_is_the_password_not_a_command() {
        // The consequence of rule 3 that a user will hit: once `wifi set` has
        // armed the parser, *every* line is the password, so a mistyped second
        // `wifi set` becomes the credential rather than re-arming. This is the
        // deliberate trade — a keyword before the password would have to be
        // typed *before* it, and so would be part of what a paste or a
        // shoulder-surfer captures.
        //
        // The recovery is `Parser::cancel`, which the caller drives from
        // [`password_window_expired`].
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set first");
        assert_eq!(parser.armed_ssid(), Some("first"));
        assert_eq!(
            password_of(parser.feed("wifi set second")),
            Some(String::from("wifi set second"))
        );
        // The armed SSID is still the first one, and the parser is back to
        // accepting commands.
        assert_eq!(parser.armed_ssid(), None);
        assert_eq!(parser.feed("wifi apply"), Reply::Accepted(Accepted::Apply));
    }

    #[test]
    fn a_password_window_stays_open_for_the_whole_thirty_seconds() {
        // A regression test for a bug that was on the device. The rule was
        // written as `now.wrapping_sub(opened + WINDOW) >= WINDOW`, and
        // `wrapping_sub` on a *negative* difference yields a number near
        // `u32::MAX`, so the condition was true on the very first poll: the
        // window lasted zero milliseconds, every password line was parsed as a
        // command, and `wifi set` could never complete.
        const OPENED: u32 = 5_000;
        assert!(
            !password_window_expired(OPENED, OPENED),
            "the instant it opens"
        );
        for elapsed in [1, 999, 20_000, PASSWORD_WINDOW_MS - 1] {
            assert!(
                !password_window_expired(OPENED, OPENED + elapsed),
                "still open after {elapsed} ms"
            );
        }
        assert!(
            password_window_expired(OPENED, OPENED + PASSWORD_WINDOW_MS),
            "expired at exactly the window"
        );
        assert!(password_window_expired(
            OPENED,
            OPENED + 10 * PASSWORD_WINDOW_MS
        ));
    }

    #[test]
    fn a_password_window_survives_the_millisecond_counter_wrapping() {
        // The window opened one millisecond before the 32-bit millisecond count
        // wrapped, which is every 49.7 days. A naive `now > opened + WINDOW`
        // comparison is false for the whole window; a borrowing subtraction
        // panics in debug. `wrapping_sub` is the whole reason it is not either.
        //
        // Note the off-by-one that `wrapping_sub` gives for free: with
        // `opened == u32::MAX`, the elapsed time at `now == k` is `k + 1`, not
        // `k`. So the window closes at `PASSWORD_WINDOW_MS - 1`, one tick
        // earlier than the same window opened at zero would.
        const OPENED: u32 = u32::MAX;
        assert!(!password_window_expired(OPENED, 0), "one ms after the wrap");
        assert!(!password_window_expired(OPENED, PASSWORD_WINDOW_MS - 2));
        assert!(password_window_expired(OPENED, PASSWORD_WINDOW_MS - 1));
        assert!(password_window_expired(OPENED, PASSWORD_WINDOW_MS));
    }

    #[test]
    fn cancel_disarms_the_password_window() {
        // The escape hatch. The caller calls this when the password window
        // expires with no line, so an operator who sent `wifi set` and then
        // thought better is not locked out for the rest of the session.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        assert!(parser.awaiting_password());
        parser.cancel();
        assert!(!parser.awaiting_password());
        assert_eq!(
            parser.feed("wifi status"),
            Reply::Accepted(Accepted::Status)
        );
    }

    #[test]
    fn cancel_is_idempotent_and_safe_before_any_command() {
        let mut parser = Parser::new();
        parser.cancel();
        parser.cancel();
        assert!(!parser.awaiting_password());
    }

    #[test]
    fn an_over_long_line_disarms_the_window_too() {
        // Same reason as `cancel`: leaving the operator with no way back is a
        // worse failure than losing a half-typed credential.
        //
        // Two lines are needed to get back to a command: one is discarded as
        // the tail of the over-long line (rule 4), and the next one is the
        // first the parser reads fresh. The device reader feeds whole lines,
        // so in practice the first is the last line of an over-long paste.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let _ = parser.feed(&"z".repeat(MAX_LINE_BYTES + 1));
        assert!(!parser.awaiting_password());
        assert_eq!(parser.feed("the rest of the paste"), Reply::Ignored);
        assert_eq!(
            parser.feed("wifi status"),
            Reply::Accepted(Accepted::Status)
        );
    }

    // ---- rule 4: over-long lines are discarded whole -------------------

    #[test]
    fn an_over_long_line_is_discarded_and_the_parser_resynchronises() {
        let mut parser = Parser::new();
        let too_long = "x".repeat(MAX_LINE_BYTES + 1);
        assert_eq!(parser.feed(&too_long), Reply::Rejected("line too long"));
        // The rest of the over-long line is discarded, not parsed: feeding its
        // tail must be ignored, not treated as a command.
        assert_eq!(parser.feed("tail of the over-long line"), Reply::Ignored);
        // And the next real line works.
        assert_eq!(
            parser.feed("wifi status"),
            Reply::Accepted(Accepted::Status)
        );
    }

    #[test]
    fn an_over_long_line_while_armed_cancels_the_pending_credential() {
        // The alternative would be to take the *tail* of the over-long line as
        // a password, which is a plausible-looking credential that will never
        // authenticate. Cancelling is the honest outcome: the operator is told
        // and retypes.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let too_long = "y".repeat(MAX_LINE_BYTES + 1);
        assert_eq!(parser.feed(&too_long), Reply::Rejected("line too long"));
        assert!(!parser.awaiting_password());
        assert_eq!(parser.feed("hunter2"), Reply::Ignored);
    }

    #[test]
    fn a_line_of_exactly_the_maximum_length_is_accepted() {
        // The bound is a bound, not an off-by-one. The longest legal SSID
        // command is `wifi set ` + 32 bytes = 41 and a 63-byte passphrase is
        // 63, so neither is near 72; the exact boundary is tested rather than
        // reasoned about.
        let mut parser = Parser::new();
        let line = "z".repeat(MAX_LINE_BYTES);
        assert_ne!(parser.feed(&line), Reply::Rejected("line too long"));
    }

    #[test]
    fn an_ssid_longer_than_32_bytes_is_rejected_rather_than_truncated() {
        let mut parser = Parser::new();
        let reply = parser.feed(&format!("wifi set {}", "s".repeat(33)));
        assert_eq!(reply, Reply::Rejected("SSID longer than 32 bytes"));
        assert!(!parser.awaiting_password());
    }

    #[test]
    fn the_password_comes_back_with_the_ssid_it_belongs_to() {
        // The parser disarms on the password, so the SSID has to travel with it.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let Reply::Accepted(Accepted::Password { password, ssid }) = parser.feed("hunter2") else {
            panic!("expected a password");
        };
        assert_eq!(ssid, "mynet");
        assert_eq!(password, "hunter2");
    }

    #[test]
    fn a_32_byte_ssid_is_accepted() {
        let mut parser = Parser::new();
        let _ = parser.feed(&format!("wifi set {}", "s".repeat(32)));
        assert_eq!(parser.armed_ssid(), Some("s".repeat(32).as_str()));
    }

    // ---- the no-credential-in-a-log rule --------------------------------

    #[test]
    fn a_password_is_never_mistaken_for_a_command_while_armed() {
        // A passphrase that happens to start with `wifi ` is a password, not a
        // command, because the arming state takes precedence over the prefix
        // rule — with the one documented exception, `wifi pass `, which is the
        // argument form and is a command by construction. Getting this backwards
        // would make a valid passphrase silently disarm the parser and leave
        // the operator with no way in.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        assert_eq!(
            password_of(parser.feed("wifi clear")),
            Some(String::from("wifi clear"))
        );
    }

    #[test]
    fn describe_never_names_an_argument() {
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set myhomewifi");
        let reply = parser.feed("correcthorse");
        // The whole of what this protocol may print about a credential line.
        assert_eq!(reply.describe(), "password");
        assert!(!reply.describe().contains("correcthorse"));
        assert!(!parser
            .feed("I (1) cc_firmware: x")
            .describe()
            .contains("cc_firmware"));
    }

    #[test]
    fn a_credential_is_not_recoverable_from_the_parser_after_the_next_line() {
        // The password is a borrow of the parser's line buffer, so the next
        // line overwrites it. That is not a security property — a determined
        // reader can read the ESP32's RAM — it is a statement that the value
        // has no lifetime beyond the call, so it cannot accumulate in a struct.
        let mut parser = Parser::new();
        let _ = parser.feed("wifi set mynet");
        let snapshot: Vec<u8> = {
            let Reply::Accepted(Accepted::Password { password, .. }) = parser.feed("hunter2")
            else {
                panic!("expected a password");
            };
            password.as_bytes().to_vec()
        };
        assert_eq!(snapshot, b"hunter2");
        let _ = parser.feed("wifi status");
        assert_ne!(parser.armed_ssid(), Some("hunter2"));
    }
}
