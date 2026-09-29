//! Server-sent events.
//!
//! The frontend's live dashboard subscribes to `/events` and expects a `text/event-stream`. The
//! C++ firmware's implementation sent periodic updates with no event names, so a client could not
//! tell a state change from a temperature reading and had to refetch everything on every tick.
//! Here every event has a name and an id, so a client can subscribe to the two it cares about and
//! reconnect with `Last-Event-ID` without losing events in between.

/// One queued event, in the stream's own storage.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Slot {
    id: u32,
    name: &'static str,
    data: heapless::String<MAX_EVENT_PAYLOAD>,
}

/// The longest event payload.
///
/// Sized for a single value, not for a whole dashboard: the C++ firmware sent one unnamed blob per
/// tick, so a client could not tell what changed and had to refetch everything. Named events with
/// a payload this size are what makes a subscription selective, and they keep the queue small
/// enough to sit on a stack frame beside the socket buffer.
pub const MAX_EVENT_PAYLOAD: usize = 80;

/// A bounded queue of pending events.
///
/// Fixed size on purpose: the stream is written by one task and read by the socket task, and a
/// queue that can grow without limit is a slow client's path to an out-of-memory abort. When it is
/// full the *newest* event is dropped rather than the oldest, because the newest is the one that
/// describes the machine's current state, and a client that reconnects will get it again.
///
/// The event count is a `const` rather than a generic so a caller can hold one by value in a
/// struct that is not generic over it; the events themselves are inline rather than boxed so the
/// queue never allocates.
#[derive(Debug)]
pub struct SseStream {
    /// `None` marks a gap, so the client knows it missed events rather than believing it has them
    /// all.
    /// A fixed-capacity ring. A `heapless::Vec` used as a ring rather than a plain array of
    /// optionals, so `new()` stays a `const fn` and the queue never allocates however many events
    /// a client fails to read.
    queue: heapless::Vec<Slot, { SseStream::CAPACITY }>,
    /// Number of events dropped because the queue was full. Reported to the client as a comment so
    /// a lagging client can tell.
    dropped: u32,
    last_id: u32,
    closed: bool,
}

/// One event.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Event {
    pub id: u32,
    /// The event name. `message` is the default and the frontend reads it, but a named event is
    /// what makes a subscription selective.
    pub name: &'static str,
    /// The payload. A JSON object or a short plain-text field.
    pub data: heapless::String<MAX_EVENT_PAYLOAD>,
}

impl Event {
    pub fn new(id: u32, name: &'static str, data: &str) -> Self {
        let mut d = heapless::String::new();
        let _ = d.push_str(data);
        Self { id, name, data: d }
    }

    /// The wire form: `id:`, `event:`, then one `data:` per line.
    ///
    /// Data is split on newlines because a JSON payload written on one line is fine, but a value
    /// that happens to contain one would otherwise be read as two fields.
    pub fn encode(&self, out: &mut [u8]) -> usize {
        let mut w = crate::writer::SliceWriter::new(out);
        let _ = core::fmt::Write::write_fmt(&mut w, format_args!("id: {}\r\n", self.id));
        let _ = core::fmt::Write::write_fmt(&mut w, format_args!("event: {}\r\n", self.name));
        for line in self.data.split('\n') {
            let _ = core::fmt::Write::write_fmt(&mut w, format_args!("data: {line}\r\n"));
        }
        let _ = core::fmt::Write::write_fmt(&mut w, format_args!("\r\n"));
        w.written()
    }
}

impl SseStream {
    /// Deep enough for the events the control task produces between two socket writes: a state
    /// change, a temperature reading and a scale reading, several times over.
    ///
    /// Twelve rather than a larger number because the queue is moved into the connection loop's
    /// stack frame when a client opens a stream. On a C6 that is 1.2 KB per streaming connection,
    /// and a machine on a busy LAN can have more than one client watching the dashboard.
    pub const CAPACITY: usize = 12;

    pub const fn new() -> Self {
        Self {
            queue: heapless::Vec::new(),
            dropped: 0,
            last_id: 0,
            closed: false,
        }
    }

    /// Queues an event, assigning it the next id.
    ///
    /// Returns the id, or `None` if the queue was full. The id is still consumed, so a client that
    /// reconnects with `Last-Event-ID` cannot be told about an event that was never sent.
    pub fn push(&mut self, name: &'static str, data: &str) -> Option<u32> {
        self.last_id = self.last_id.wrapping_add(1);
        let id = self.last_id;
        if self.closed {
            return None;
        }
        let mut d = heapless::String::new();
        // A payload that does not fit is dropped rather than truncated: a truncated JSON event
        // reaches the frontend as a parse error, which looks like a firmware bug.
        if d.push_str(data).is_err() {
            self.dropped = self.dropped.wrapping_add(1);
            return None;
        }
        if self.queue.push(Slot { id, name, data: d }).is_err() {
            // Full. Drop the newest rather than the oldest: it describes the machine as it is now,
            // and an old event delivered late is worse than no event, because the frontend would
            // render the machine going backwards.
            self.dropped = self.dropped.wrapping_add(1);
            return None;
        }
        Some(id)
    }

    /// Takes the oldest pending event.
    pub fn pop(&mut self) -> Option<Event> {
        // The guard matters: `heapless::Vec::remove` panics on an empty vector, and this runs in
        // the socket loop, where a client that disconnected mid-drain would otherwise take the
        // task down with it.
        if self.queue.is_empty() {
            return None;
        }
        let slot = self.queue.remove(0);
        Some(Event {
            id: slot.id,
            name: slot.name,
            data: slot.data,
        })
    }

    /// The id of the last event queued, for the `Last-Event-ID` a reconnecting client sends.
    pub const fn last_id(&self) -> u32 {
        self.last_id
    }

    /// The id the stream should resume from.
    ///
    /// A client that says it has seen more than the stream has produced is resynchronised to the
    /// current head, because its id is from a firmware that was flashed and replaced.
    pub const fn resume_from(&self, client_last_id: Option<u32>) -> u32 {
        match client_last_id {
            Some(id) if id <= self.last_id => id,
            _ => 0,
        }
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    pub const fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Ends the stream. Further pushes are refused rather than silently queued, because a closed
    /// stream that keeps accepting events is how a task ends up blocked on a client that is gone.
    pub fn close(&mut self) {
        self.closed = true;
    }

    pub const fn is_closed(&self) -> bool {
        self.closed
    }
}

impl Default for SseStream {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::String;

    fn encoded(e: &Event) -> String<256> {
        let mut buf = [0u8; 256];
        let n = e.encode(&mut buf);
        let mut s = String::new();
        s.push_str(core::str::from_utf8(&buf[..n]).unwrap())
            .unwrap();
        s
    }

    #[test]
    fn an_event_carries_an_id_a_name_and_its_data() {
        let e = Event::new(7, "machineState", r#"{"state":33}"#);
        assert_eq!(
            encoded(&e),
            "id: 7\r\nevent: machineState\r\ndata: {\"state\":33}\r\n\r\n"
        );
    }

    #[test]
    fn data_containing_a_newline_becomes_several_data_lines() {
        // One `data:` line per line, so a payload with a newline in it cannot be read as a field
        // boundary and corrupt the framing.
        let e = Event::new(1, "x", "a\nb");
        let out = encoded(&e);
        assert!(out.contains("data: a\r\ndata: b\r\n"));
    }

    #[test]
    fn events_come_out_in_the_order_they_went_in() {
        let mut s = SseStream::new();
        for i in 0..5 {
            assert!(s.push("tick", "x").is_some(), "event {i} was refused");
        }
        for i in 1..=5 {
            assert_eq!(s.pop().unwrap().id, i);
        }
        assert!(s.pop().is_none());
    }

    #[test]
    fn ids_increase_and_are_unique() {
        let mut s = SseStream::new();
        let a = s.push("x", "1").unwrap();
        let b = s.push("x", "2").unwrap();
        assert!(b > a);
    }

    #[test]
    fn a_full_queue_drops_the_newest_and_says_so() {
        // Dropping the newest is deliberate: it describes the machine now. Dropping the oldest
        // would deliver a stale state change after fresher data, which the frontend would render
        // as the machine going backwards.
        let mut s = SseStream::new();
        for _ in 0..SseStream::CAPACITY {
            s.push("tick", "x");
        }
        assert!(s.push("tick", "dropped").is_none());
        assert_eq!(s.dropped(), 1);
        assert_eq!(s.pending(), SseStream::CAPACITY);
    }

    #[test]
    fn a_dropped_event_still_consumes_its_id() {
        // Otherwise a reconnecting client asking for that id would be told it had received an
        // event that never existed.
        let mut s = SseStream::new();
        for _ in 0..SseStream::CAPACITY {
            s.push("tick", "x");
        }
        let last = s.push("tick", "dropped");
        assert!(last.is_none());
        assert_eq!(s.last_id(), SseStream::CAPACITY as u32 + 1);
    }

    #[test]
    fn the_queue_recovers_after_draining() {
        let mut s = SseStream::new();
        for _ in 0..(SseStream::CAPACITY + 4) {
            let _ = s.push("tick", "x");
        }
        for _ in 0..4 {
            let _ = s.pop();
        }
        assert!(s.push("tick", "x").is_some(), "draining must free capacity");
    }

    #[test]
    fn a_reconnecting_client_resumes_from_where_it_stopped() {
        let mut s = SseStream::new();
        s.push("a", "1");
        s.push("b", "2");
        assert_eq!(s.resume_from(Some(1)), 1);
        assert_eq!(s.resume_from(None), 0);
    }

    #[test]
    fn a_client_claiming_more_events_than_exist_is_resynchronised() {
        // Its id comes from a firmware that was flashed and replaced. Trusting it would leave the
        // client waiting for events that will never arrive.
        let mut s = SseStream::new();
        s.push("a", "1");
        assert_eq!(s.resume_from(Some(9999)), 0);
    }

    #[test]
    fn a_stream_is_small_enough_to_live_beside_the_socket_buffer() {
        // The queue is moved into the connection loop's frame only when a client actually opens a
        // stream, so its size is a per-connection cost on a device with 320 KB of RAM. Raising
        // the bound here costs every streaming client; lowering it costs dropped events.
        assert!(
            core::mem::size_of::<SseStream>() <= 1536,
            "an SseStream is {} bytes",
            core::mem::size_of::<SseStream>()
        );
    }

    #[test]
    fn a_payload_that_does_not_fit_is_dropped_rather_than_truncated() {
        let mut s = SseStream::new();
        assert!(s.push("tick", &"x".repeat(MAX_EVENT_PAYLOAD + 1)).is_none());
        assert_eq!(s.dropped(), 1);
    }

    #[test]
    fn a_closed_stream_refuses_further_events() {
        let mut s = SseStream::new();
        s.close();
        assert!(s.push("tick", "x").is_none());
        assert!(s.is_closed());
    }

    #[test]
    fn encoding_into_a_short_buffer_stops_at_the_end() {
        let e = Event::new(1, "tick", &"x".repeat(500));
        let mut buf = [0u8; 32];
        let n = e.encode(&mut buf);
        assert!(n <= buf.len());
    }
}
