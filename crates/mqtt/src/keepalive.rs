//! MQTT keep-alive. The broker drops a client that sends nothing for one and a half
//! keep-alive intervals. Only packets actually sent count: a publish round where no
//! value changed sends nothing, and on a cold machine that is every round.

/// When the client last sent a packet, and so when it has to ping.
#[derive(Debug, Clone, Copy)]
pub struct KeepAlive {
    interval_ms: u64,
    last_sent_ms: u64,
}

impl KeepAlive {
    pub const fn new(keep_alive_s: u16, now_ms: u64) -> Self {
        Self {
            interval_ms: keep_alive_s as u64 * 1000,
            last_sent_ms: now_ms,
        }
    }

    /// A packet went out.
    pub fn sent(&mut self, now_ms: u64) {
        self.last_sent_ms = now_ms;
    }

    /// Half an interval after the last packet: the client must wake up by then.
    pub const fn ping_at_ms(&self) -> u64 {
        self.last_sent_ms + self.interval_ms / 2
    }

    pub const fn ping_due(&self, now_ms: u64) -> bool {
        now_ms >= self.ping_at_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_is_due_half_an_interval_after_the_last_packet() {
        let mut k = KeepAlive::new(30, 1_000);
        assert_eq!(k.ping_at_ms(), 16_000);
        assert!(!k.ping_due(15_999));
        assert!(k.ping_due(16_000));
        k.sent(16_000);
        assert!(!k.ping_due(30_000));
    }

    // The firmware used to count a publish round as traffic even when nothing changed:
    // with a steady temperature it went silent and the broker dropped it every minute.
    #[test]
    fn only_packets_sent_reset_the_deadline() {
        let k = KeepAlive::new(30, 0);
        // Publish rounds at 5 s, 10 s, 15 s sent nothing: no `sent` calls.
        assert!(k.ping_due(15_000));
    }
}
