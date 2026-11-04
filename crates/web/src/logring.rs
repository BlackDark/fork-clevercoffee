//! The newest log lines, kept in RAM so `GET /api/logs` can serve them over WiFi.

use alloc::vec::Vec;

/// A byte ring that remembers how many bytes ever went through it, so a reader can ask
/// for "everything since offset N" and notice when it missed some or the device restarted.
#[derive(Debug)]
pub struct LogRing {
    buf: Vec<u8>,
    total: u64,
}

impl LogRing {
    /// `None` when the heap cannot spare `capacity` bytes.
    pub fn new(capacity: usize) -> Option<Self> {
        let mut buf = Vec::new();
        buf.try_reserve_exact(capacity).ok()?;
        buf.resize(capacity, 0);
        Some(Self { buf, total: 0 })
    }

    /// Bytes ever pushed; the offset the next push gets.
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn push(&mut self, bytes: &[u8]) {
        let cap = self.buf.len();
        if cap == 0 {
            return;
        }
        for &b in bytes {
            self.buf[(self.total % cap as u64) as usize] = b;
            self.total += 1;
        }
    }

    /// Appends what was logged since offset `since` to `out` and returns the offset to ask
    /// for next. An offset the ring no longer holds (too old, or from before a restart)
    /// starts at the oldest byte kept, at the next line start.
    ///
    /// `None` when `out` cannot grow.
    pub fn read_since(&self, since: u64, out: &mut Vec<u8>) -> Option<u64> {
        let cap = self.buf.len() as u64;
        let oldest = self.total.saturating_sub(cap);
        let lost = since < oldest || since > self.total;
        let mut start = if lost { oldest } else { since };
        out.try_reserve((self.total - start) as usize).ok()?;
        let mut at_line_start = start == 0 || !lost;
        while start < self.total {
            let b = self.buf[(start % cap) as usize];
            start += 1;
            if !at_line_start {
                at_line_start = b == b'\n';
                continue;
            }
            out.push(b);
        }
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
    fn a_zero_capacity_ring_keeps_nothing() {
        let mut r = LogRing::new(0).unwrap();
        r.push(b"x");
        assert_eq!(read(&r, 0), (0, std::string::String::new()));
    }
}
