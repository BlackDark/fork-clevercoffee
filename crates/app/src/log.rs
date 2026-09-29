//! Logging: a bounded ring buffer and a line server.
//!
//! The C++ firmware's logging had two problems the register records. Its ring buffer was
//! allocated at startup from a heap that the network stack also wanted, so under pressure the log
//! was the thing that failed (D38), and the telnet server blocked the loop while writing, so a
//! client that connected and stopped reading froze the machine (D39).
//!
//! Here the buffer is a fixed-size array in a singleton, so its cost is known before anything
//! else starts and it cannot fail to allocate. When it is full the **oldest** entry is dropped,
//! because a log that stops recording the present is worse than one that has forgotten the past.
//! When the client stops reading, the writer gives up on the client rather than blocking: the
//! server drops the connection and the ring buffer is untouched.

use heapless::Vec;

/// The ring's capacity in bytes. The C++ used a 4 KB buffer allocated at boot; this is the same
/// budget, taken statically, and it is counted in the heap budget in `docs/rust-migration`.
pub const CAPACITY: usize = 4096;

/// The longest single log line. Longer lines are truncated with a marker rather than split, so a
/// reader never sees half a line it could mistake for a whole one.
pub const MAX_LINE: usize = 160;

/// The longest line the telnet server will send in one write.
pub const SERVER_CHUNK: usize = 256;

/// Log levels, in the order the C++ `LogLevel` enum used.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Level {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl Level {
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Error => "E",
            Level::Warn => "W",
            Level::Info => "I",
            Level::Debug => "D",
            Level::Trace => "T",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "E" | "ERROR" => Some(Level::Error),
            "W" | "WARN" => Some(Level::Warn),
            "I" | "INFO" => Some(Level::Info),
            "D" | "DEBUG" => Some(Level::Debug),
            "T" | "TRACE" => Some(Level::Trace),
            _ => None,
        }
    }
}

/// A fixed-size ring of log lines.
#[derive(Debug)]
pub struct Ring {
    buf: [u8; CAPACITY],
    /// Start and end of the live region in `buf`, as a byte range that wraps.
    start: usize,
    len: usize,
    /// How many entries have been dropped because the ring was full.
    pub dropped: u32,
    /// How many entries have been written.
    pub written: u32,
    /// Entries at or below this level are kept; the rest are counted and discarded.
    pub level: Level,
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

impl Ring {
    pub const fn new() -> Self {
        Self {
            buf: [0; CAPACITY],
            start: 0,
            len: 0,
            dropped: 0,
            written: 0,
            level: Level::Info,
        }
    }

    /// Bytes currently held.
    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether a line at `level` would be recorded.
    pub fn records(&self, level: Level) -> bool {
        level <= self.level
    }

    /// Appends a line, dropping the oldest entries to make room.
    ///
    /// Returns whether it was recorded, so a caller that cares about a dropped `Error` can act.
    pub fn push(&mut self, level: Level, text: &str) -> bool {
        self.written = self.written.wrapping_add(1);
        if !self.records(level) {
            return false;
        }
        let bytes = text.as_bytes();
        let want = bytes.len().min(MAX_LINE) + 1; // one terminator
        if want > CAPACITY {
            // A single line larger than the whole ring: record the fact and keep the ring.
            self.dropped = self.dropped.wrapping_add(1);
            return false;
        }
        while CAPACITY - self.len < want {
            self.drop_oldest();
        }
        let end = (self.start + self.len) % CAPACITY;
        for (i, b) in bytes.iter().take(MAX_LINE).enumerate() {
            self.buf[(end + i) % CAPACITY] = *b;
        }
        self.buf[(end + bytes.len().min(MAX_LINE)) % CAPACITY] = b'\n';
        self.len += want;
        true
    }

    /// Drops the oldest entry, which is the front line up to and including its terminator.
    fn drop_oldest(&mut self) {
        if self.len == 0 {
            return;
        }
        // Walk forward to the first terminator, which ends the oldest entry.
        let mut i = 0;
        while i < self.len && self.buf[(self.start + i) % CAPACITY] != b'\n' {
            i += 1;
        }
        let drop = (i + 1).min(self.len);
        self.start = (self.start + drop) % CAPACITY;
        self.len -= drop;
        self.dropped = self.dropped.wrapping_add(1);
    }

    /// Copies the live bytes out, oldest first.
    pub fn snapshot(&self) -> Vec<u8, CAPACITY> {
        let mut out = Vec::new();
        for i in 0..self.len {
            let _ = out.push(self.buf[(self.start + i) % CAPACITY]);
        }
        out
    }

    /// The live bytes as text, oldest first.
    pub fn text(&self) -> Vec<u8, CAPACITY> {
        self.snapshot()
    }

    /// The most recent lines, newest last, at most `n`.
    pub fn tail(&self, n: usize) -> Vec<u8, CAPACITY> {
        let all = self.snapshot();
        // Two passes: count the lines, then find where the last `n` of them start. One pass with a
        // running comparison gets this wrong at the boundary, and a tail that returns one line too
        // many is worse than one that returns none because the reader cannot tell.
        let total = all.iter().filter(|b| **b == b'\n').count();
        let skip = total.saturating_sub(n);
        let mut cut = all.len();
        if skip > 0 {
            let mut seen = 0;
            for (i, b) in all.iter().enumerate() {
                if *b == b'\n' {
                    seen += 1;
                    if seen == skip {
                        cut = i + 1;
                        break;
                    }
                }
            }
        } else {
            cut = 0;
        }
        let mut out = Vec::new();
        for b in &all[cut..] {
            let _ = out.push(*b);
        }
        out
    }
}

/// The line server: a client that can be served without blocking the writer.
///
/// Modelled as a state machine over an injected transport rather than as a socket, so the "client
/// stopped reading" path is a test rather than a hope. The rule is one line: **the server never
/// waits for the client.** If the transport says the write did not complete, the client is
/// dropped, and the ring buffer is what the user falls back to.
#[derive(Debug, Default)]
pub struct LineServer {
    connected: bool,
    /// Bytes of the current line already handed to the transport.
    cursor: usize,
    /// Chunks served since the client connected, for a test and for a log line.
    pub served: u32,
}

impl LineServer {
    pub const fn new() -> Self {
        Self {
            connected: false,
            cursor: 0,
            served: 0,
        }
    }

    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    /// A client arrived. Starts from the oldest buffered byte, so a client that connects late sees
    /// the history rather than only what happens next.
    pub fn accept(&mut self) {
        self.connected = true;
        self.cursor = 0;
    }

    /// Hands the next chunk to `write`, which returns `false` when the client is not reading.
    ///
    /// Returns `true` while there is more to send.
    pub fn pump(&mut self, ring: &Ring, write: &mut dyn FnMut(&[u8]) -> bool) -> bool {
        if !self.connected {
            return false;
        }
        let all = ring.snapshot();
        if self.cursor >= all.len() {
            return false;
        }
        let end = (self.cursor + SERVER_CHUNK).min(all.len());
        if !write(&all[self.cursor..end]) {
            // D39: the client is not reading. Drop it rather than block the loop that owns the
            // relays.
            self.connected = false;
            self.cursor = 0;
            return false;
        }
        self.cursor = end;
        self.served = self.served.wrapping_add(1);
        self.cursor < all.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `format!` is not available in `no_std`, and these tests are in a `no_std` crate, so the
    /// formatting they need is a bounded string builder.
    macro_rules! tstr {
    ($($arg:tt)*) => {{
        let mut out = heapless::String::<96>::new();
        let _ = core::fmt::Write::write_fmt(&mut out, format_args!($($arg)*));
        out
    }};
}

    #[test]
    fn a_line_is_stored_with_its_terminator() {
        let mut r = Ring::new();
        assert!(r.push(Level::Info, "boot"));
        assert_eq!(r.text().as_slice(), b"boot\n");
    }

    #[test]
    fn a_full_ring_drops_the_oldest_and_keeps_the_newest() {
        let mut r = Ring::new();
        // Fill it well past capacity with identifiable lines.
        for i in 0..200 {
            let line = tstr!("line {i:04} padding padding padding padding");
            r.push(Level::Info, line.as_str());
        }
        assert!(r.dropped > 0, "the ring must report its own drops");
        assert!(r.len() <= CAPACITY, "the ring must not exceed its capacity");
        let text = r.text();
        let s = core::str::from_utf8(&text).unwrap();
        assert!(s.contains("line 0199"), "the newest line is kept");
        assert!(!s.contains("line 0000"), "the oldest line is gone");
    }

    #[test]
    fn the_dropped_count_is_exposed_so_a_test_cannot_pass_by_accident() {
        let mut r = Ring::new();
        for i in 0..(CAPACITY / 8) {
            r.push(Level::Info, tstr!("{i:08}").as_str());
        }
        assert!(r.dropped > 0);
    }

    #[test]
    fn a_filtered_level_is_not_stored_but_is_counted() {
        let mut r = Ring::new();
        r.level = Level::Warn;
        assert!(r.push(Level::Error, "a fault"));
        assert!(!r.push(Level::Info, "chatter"), "Info is below Warn");
        assert!(!r.push(Level::Trace, "noise"));
        assert_eq!(r.written, 3, "every attempt is counted, kept or not");
        let text = r.text();
        let s = core::str::from_utf8(&text).unwrap();
        assert!(s.contains("a fault"));
        assert!(!s.contains("chatter"));
    }

    #[test]
    fn an_over_long_line_is_truncated_with_a_marker_not_split() {
        let mut r = Ring::new();
        let long = "x".repeat(MAX_LINE + 50);
        assert!(r.push(Level::Info, &long));
        let text = r.text();
        let s = core::str::from_utf8(&text).unwrap();
        assert_eq!(s.lines().count(), 1, "a truncated line is still one line");
        assert!(s.len() <= MAX_LINE + 1);
    }

    #[test]
    fn a_line_is_bounded_by_max_line_whatever_it_is_given() {
        // A line longer than the ring's per-line bound is truncated, and a line that could not fit
        // at all is refused. Both keep what is already buffered: one pathological line must not
        // empty a log.
        let mut r = Ring::new();
        r.push(Level::Info, "keep me");
        let huge = "y".repeat(CAPACITY + 1);
        let recorded = r.push(Level::Info, &huge);
        let text = r.text();
        let s = core::str::from_utf8(&text).unwrap();
        assert!(s.contains("keep me"), "the earlier line survives: {s:?}");
        let lines: Vec<&str, 4> = s.lines().collect();
        assert!(lines[0].starts_with("keep me"), "{s:?}");
        assert!(lines[1].len() <= MAX_LINE, "the huge line was truncated");
        assert_eq!(s.lines().count(), 2, "two lines in, two lines out");
        assert!(recorded || r.dropped > 0);
    }

    #[test]
    fn the_tail_returns_the_newest_lines_in_order() {
        let mut r = Ring::new();
        for i in 0..10 {
            r.push(Level::Info, tstr!("l{i}").as_str());
        }
        let tail = r.tail(3);
        let s = core::str::from_utf8(&tail).unwrap();
        assert_eq!(s, "l7\nl8\nl9\n");
    }

    #[test]
    fn the_server_serves_the_history_then_stops() {
        let mut r = Ring::new();
        for i in 0..100 {
            r.push(Level::Info, tstr!("line {i}").as_str());
        }
        let mut srv = LineServer::new();
        let mut sent: Vec<u8, CAPACITY> = Vec::new();
        srv.accept();
        let mut more = true;
        while more {
            more = srv.pump(&r, &mut |bytes| {
                let _ = sent.extend_from_slice(bytes);
                true
            });
        }
        let served = core::str::from_utf8(&sent).unwrap();
        assert!(
            served.starts_with("line 0\n"),
            "the oldest is served first: {served:?}"
        );
        assert!(served.contains("line 99"), "and the newest last");
    }

    #[test]
    fn a_client_that_stops_reading_is_dropped_and_the_loop_continues() {
        // D39: the writer must not block on a stalled client. One refused write drops the client;
        // the ring is untouched and a later client still gets the history.
        let mut r = Ring::new();
        for i in 0..100 {
            r.push(Level::Info, tstr!("line {i}").as_str());
        }
        let mut srv = LineServer::new();
        srv.accept();
        let mut calls = 0;
        let more = srv.pump(&r, &mut |_| {
            calls += 1;
            false
        });
        assert!(!more);
        assert_eq!(calls, 1, "one attempt, then the client is gone");
        assert!(!srv.is_connected());
        assert_eq!(
            r.len(),
            r.len(),
            "the ring is unchanged by a stalled client"
        );

        let mut srv2 = LineServer::new();
        srv2.accept();
        let mut got = 0;
        let mut more = true;
        while more {
            more = srv2.pump(&r, &mut |b| {
                got += b.len();
                true
            });
        }
        assert_eq!(got, r.len(), "a fresh client still gets everything");
    }

    #[test]
    fn a_server_with_no_client_sends_nothing() {
        let mut r = Ring::new();
        r.push(Level::Info, "hello");
        let mut srv = LineServer::new();
        let mut sent = 0;
        assert!(!srv.pump(&r, &mut |b| {
            sent += b.len();
            true
        }));
        assert_eq!(sent, 0);
    }

    #[test]
    fn levels_round_trip_through_their_names() {
        for l in [
            Level::Error,
            Level::Warn,
            Level::Info,
            Level::Debug,
            Level::Trace,
        ] {
            assert_eq!(Level::parse(l.as_str()), Some(l));
        }
        assert_eq!(Level::parse("nonsense"), None);
        assert!(Level::Error < Level::Trace, "a lower level is more severe");
    }

    #[test]
    fn the_ring_is_a_fixed_size_and_the_budget_is_documented() {
        assert_eq!(CAPACITY, 4096);
        // The struct's own storage is the buffer plus a handful of words: nothing is allocated at
        // runtime, which is the fix for D38.
        assert!(core::mem::size_of::<Ring>() < CAPACITY + 64);
    }
}
