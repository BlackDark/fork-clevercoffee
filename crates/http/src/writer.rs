//! A `core::fmt::Write` that appends into a caller-provided byte slice.
//!
//! Both the response head and the SSE encoder need to format text into a fixed buffer without
//! allocating, and both must stop cleanly at the end of that buffer rather than growing it. One
//! adapter serves both, so the "stop at the end" behaviour is written once.
//!
//! Truncation is silent here on purpose. The callers all have a bound on what they write (a
//! request line over the limit is refused before a response is built, a header value over the
//! limit is dropped), so reaching the end of the buffer means the caller's own limit fired, and
//! the callers that care check `n` against their expectation.

/// Appends into `buf`, counting what it wrote.
#[derive(Debug)]
pub struct SliceWriter<'a> {
    buf: &'a mut [u8],
    n: usize,
}

impl<'a> SliceWriter<'a> {
    pub const fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, n: 0 }
    }

    /// How many bytes were written.
    pub const fn written(&self) -> usize {
        self.n
    }

    /// The bytes written so far.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf[..self.n]
    }
}

impl core::fmt::Write for SliceWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = (self.n + s.len()).min(self.buf.len());
        let take = end - self.n;
        self.buf[self.n..end].copy_from_slice(&s.as_bytes()[..take]);
        self.n = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn it_writes_into_the_buffer_and_counts() {
        let mut buf = [0u8; 32];
        let mut w = SliceWriter::new(&mut buf);
        let _ = write!(w, "hello {}", 42);
        assert_eq!(w.written(), 8);
        assert_eq!(w.as_slice(), b"hello 42");
    }

    #[test]
    fn it_stops_at_the_end_of_the_buffer() {
        // The whole point: a format call on a short buffer stops rather than growing or
        // panicking, so a bounded response never needs an allocation to fail safely.
        let mut buf = [0u8; 5];
        let mut w = SliceWriter::new(&mut buf);
        let _ = write!(w, "this is much longer than the buffer");
        assert_eq!(w.written(), 5);
        assert_eq!(w.as_slice(), b"this ");
    }

    #[test]
    fn writing_into_a_zero_length_buffer_writes_nothing() {
        let mut buf = [0u8; 0];
        let mut w = SliceWriter::new(&mut buf);
        let _ = write!(w, "anything");
        assert_eq!(w.written(), 0);
    }

    #[test]
    fn successive_writes_append() {
        let mut buf = [0u8; 32];
        let mut w = SliceWriter::new(&mut buf);
        let _ = write!(w, "a");
        let _ = write!(w, "b");
        let _ = write!(w, "c");
        assert_eq!(w.as_slice(), b"abc");
    }
}
