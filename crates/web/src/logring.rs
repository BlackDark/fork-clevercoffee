//! The newest log lines, kept in RAM so `GET /api/logs` can serve them over WiFi.

use alloc::vec::Vec;

/// A byte ring that remembers how many bytes ever went through it, so a reader can ask
/// for "everything since offset N" and notice when it missed some or the device restarted.
///
/// Pushing and reading copy whole slices: the firmware holds a critical section while
/// it calls them.
#[derive(Debug)]
pub struct LogRing {
    buf: Vec<u8>,
    /// Index the next byte goes to.
    head: usize,
    total: u64,
}

impl LogRing {
    /// `None` when the heap cannot spare `capacity` bytes.
    pub fn new(capacity: usize) -> Option<Self> {
        let mut buf = Vec::new();
        buf.try_reserve_exact(capacity).ok()?;
        buf.resize(capacity, 0);
        Some(Self {
            buf,
            head: 0,
            total: 0,
        })
    }

    /// Bytes ever pushed; the offset the next push gets.
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn push(&mut self, mut bytes: &[u8]) {
        let cap = self.buf.len();
        if cap == 0 {
            return;
        }
        self.total += bytes.len() as u64;
        // Only the last `cap` bytes can survive.
        if bytes.len() > cap {
            bytes = &bytes[bytes.len() - cap..];
            self.head = 0;
        }
        let first = bytes.len().min(cap - self.head);
        self.buf[self.head..self.head + first].copy_from_slice(&bytes[..first]);
        self.buf[..bytes.len() - first].copy_from_slice(&bytes[first..]);
        self.head = (self.head + bytes.len()) % cap;
    }

    /// Appends what was logged since offset `since` to `out` and returns the offset to ask
    /// for next. An offset the ring no longer holds (too old, or from before a restart)
    /// starts at the oldest byte kept, at the next line start.
    ///
    /// `None` when `out` cannot grow.
    pub fn read_since(&self, since: u64, out: &mut Vec<u8>) -> Option<u64> {
        let cap = self.buf.len();
        let oldest = self.total.saturating_sub(cap as u64);
        let lost = since < oldest || since > self.total;
        let start = if lost { oldest } else { since };
        let len = (self.total - start) as usize;
        out.try_reserve(len).ok()?;
        // Where `start` sits: the ring is full (oldest byte at `head`) or not yet wrapped.
        let at = if self.total > cap as u64 {
            (self.head + (start - oldest) as usize) % cap
        } else {
            start as usize
        };
        let first = len.min(cap - at);
        let (a, b) = (&self.buf[at..at + first], &self.buf[..len - first]);
        // A lost reader begins at a line start; the cut line is dropped.
        let skip = if lost && start > 0 {
            a.iter().chain(b).position(|&c| c == b'\n').map_or(len, |i| i + 1)
        } else {
            0
        };
        out.extend(a.iter().chain(b).skip(skip));
        Some(self.total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(r: &LogRing, since: u64) -> (u64, std::string::String) {
        let mut out = Vec::new();
        let next = r.read_since(since, &mut out).unwrap();
        (next, std::string::String::from_utf8(out).unwrap())
    }

    #[test]
    fn reads_everything_then_only_the_new_lines() {
        let mut r = LogRing::new(64).unwrap();
        r.push(b"one\ntwo\n");
        assert_eq!(read(&r, 0), (8, "one\ntwo\n".into()));
        r.push(b"three\n");
        assert_eq!(read(&r, 8), (14, "three\n".into()));
        assert_eq!(read(&r, 14), (14, std::string::String::new()));
    }

    #[test]
    fn a_reader_that_fell_behind_restarts_at_a_line() {
        let mut r = LogRing::new(16).unwrap();
        r.push(b"aaaa\nbbbb\ncccc\ndddd\n"); // 20 bytes, 16 kept: "a\nbbbb\ncccc\ndddd\n"
        let (next, text) = read(&r, 0);
        assert_eq!(next, 20);
        assert_eq!(text, "bbbb\ncccc\ndddd\n");
    }

    #[test]
    fn an_offset_from_before_a_restart_reads_what_is_there() {
        let mut r = LogRing::new(32).unwrap();
        r.push(b"fresh\n");
        let (next, text) = read(&r, 9_999);
        assert_eq!((next, text.as_str()), (6, "fresh\n"));
    }

    #[test]
    fn wraps_around_the_end_of_the_buffer() {
        let mut r = LogRing::new(8).unwrap();
        r.push(b"abc\n");
        r.push(b"defg\nh");
        assert_eq!(read(&r, 4), (10, "defg\nh".into()));
    }

    #[test]
    fn a_push_longer_than_the_ring_keeps_the_end() {
        let mut r = LogRing::new(8).unwrap();
        r.push(b"x\n");
        r.push(b"0123456789\nabc\n");
        assert_eq!(read(&r, 0), (17, "abc\n".into()));
        r.push(b"d\n");
        assert_eq!(read(&r, 17), (19, "d\n".into()));
    }

    #[test]
    fn an_exact_fill_and_the_push_after_it() {
        let mut r = LogRing::new(8).unwrap();
        r.push(b"abc\ndef\n");
        assert_eq!(read(&r, 0), (8, "abc\ndef\n".into()));
        r.push(b"gh\n");
        assert_eq!(read(&r, 8), (11, "gh\n".into()));
        assert_eq!(read(&r, 0), (11, "def\ngh\n".into()));
    }

    #[test]
    fn a_zero_capacity_ring_keeps_nothing() {
        let mut r = LogRing::new(0).unwrap();
        r.push(b"x");
        assert_eq!(read(&r, 0), (0, std::string::String::new()));
    }
}
