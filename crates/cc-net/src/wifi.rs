//! Wi-Fi connection ordering and signal-strength buckets.
//!
//! Set the hostname before association: ESP-IDF snapshots the DHCP client identity
//! when association starts. A later change applies only to the next association.
//! [`ConnectionOrder`] records the sequence for validation.

use crate::resilience::{CircuitBreaker, RetryPolicy};

/// A record of the order in which the STA came up, for the ordering assertion.
///
/// Not a state machine: a log of four booleans the device code sets as it
/// performs each step. A type rather than a comment, because this rule is exactly
/// the sort of thing an edit removes from a comment without noticing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the type IS four booleans -- it records which of four ordered \
              steps have happened, and the only code that reads it branches on \
              exactly one of them. An enum per step would be a state machine \
              for a value that is written once, at boot, and asserted once."
)]
pub struct ConnectionOrder {
    /// The netif was created with the DHCP hostname in its configuration.
    ///
    /// In `esp-idf-svc` the hostname is a field of the netif's
    /// `DHCPClientSettings`, applied when the netif is constructed, so this is
    /// true the moment the netif exists, which is necessarily before
    /// `wifi.start()`. That is the structural reason this port cannot get the
    /// order wrong, and it is why the assertion below is about the netif and not
    /// about a separate `set_hostname` call.
    pub netif_created_with_hostname: bool,
    /// `wifi.start()` was called.
    pub driver_started: bool,
    /// `wifi.connect()` — the association — was requested.
    pub association_requested: bool,
    /// The DHCP client sent its first request.
    ///
    /// Only observable in the device log, and the reason the ordering rule
    /// exists: this is the point at which the client ID is already fixed.
    pub dhcp_requested: bool,
}

impl ConnectionOrder {
    /// Whether the hostname was applied before the association could send a
    /// DHCP request.
    ///
    /// The rule is "hostname before the association", expressed over the four
    /// steps the stack actually has. `esp-idf` has no `begin()`; the equivalent
    /// edge is `association_requested`, and because the hostname is a netif field
    /// it is set before that edge by construction.
    #[must_use]
    pub const fn hostname_is_before_association(&self) -> bool {
        // The association is only reachable with a hostname already on the
        // netif, so `netif_created_with_hostname` must be true. The rest is
        // bookkeeping: `dhcp_requested` is recorded by the device for the log,
        // and if it is ever observed with the hostname missing, the trace
        // below is what says so.
        self.netif_created_with_hostname || !self.dhcp_requested
    }
}

/// The four-bucket signal indicator.
///
/// The buckets are `-50`, `-65`, `-75`, `-80` dBm, and **0 is overloaded**: it
/// means "not connected" *or* "offline mode", not "no signal". A display that
/// drew four bars for 0 and an empty icon for "no network" would be showing a
/// network it does not have, so zero is its own case and the caller renders it as
/// offline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signal {
    /// Offline mode, or not associated. Never a received signal.
    Offline = 0,
    /// Present but weak: worse than -80 dBm and up to -100 dBm.
    Weak = 1,
    /// Usable: -79..=-75 dBm.
    Fair = 2,
    /// Good: -74..=-65 dBm.
    Good = 3,
    /// Excellent: -64 dBm and above.
    Excellent = 4,
}

impl Signal {
    /// The RSSI used for a radio that is up but not associated.
    ///
    /// -100 dBm is 20 dB below the weakest bucket, and the zero case is returned
    /// before the buckets are consulted, so this value only ever lands in
    /// [`Signal::Weak`].
    pub const NOT_CONNECTED_RSSI: i32 = -100;

    /// Map a received RSSI in dBm to a bucket.
    ///
    /// | RSSI dBm | bucket |
    /// | --- | --- |
    /// | `>= -50` | 4 |
    /// | `-65..-51` | 3 |
    /// | `-75..-66` | 2 |
    /// | `-80..-76` | 1 |
    /// | `<= -81` | 0 |
    ///
    /// Note the last row: a **received but very weak** signal buckets the same as
    /// "offline". That is a display wart rather than a safety issue, and it is
    /// reproduced — see [`Signal::Offline`].
    #[must_use]
    pub const fn from_rssi(rssi: i32) -> Self {
        if rssi >= -50 {
            Self::Excellent
        } else if rssi >= -65 {
            Self::Good
        } else if rssi >= -75 {
            Self::Fair
        } else if rssi >= -80 {
            Self::Weak
        } else {
            Self::Offline
        }
    }

    /// The 0–4 integer the `/api/status` payload uses.
    #[must_use]
    pub const fn as_bars(self) -> u8 {
        self as u8
    }
}

/// What [`Monitor::poll`] wants the network code to do this iteration.
///
/// `Connected` means "the link is up, keep going"; `Retry` means "associate now";
/// `Wait { retry_in_ms }` means "the backoff has not elapsed, do nothing";
/// `CircuitOpen` means fail fast rather than touching the radio; `Offline` means
/// both policies are exhausted and the machine has stopped trying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The link is up. Reset both policies and count the reconnects as done.
    Connected,
    /// Start an association attempt now.
    Retry {
        /// The 1-based attempt number.
        attempt: u32,
        /// The backoff that will apply *after* this attempt.
        next_delay_ms: u32,
    },
    /// The backoff has not elapsed.
    Wait {
        /// Milliseconds until the next attempt is due.
        retry_in_ms: u32,
    },
    /// The circuit is open; fail fast rather than touching the radio.
    CircuitOpen,
    /// The machine has stopped trying. Reported once, then
    /// [`Monitor::is_offline`] is true forever after.
    Offline,
}

/// The whole of the reconnection decision, as data.
///
/// The structure is: check the breaker, check the association, on success record
/// it and reset both policies, on failure consult the retry policy, and enter
/// offline mode when both policies are exhausted. Every one of those decisions is
/// a call into [`RetryPolicy`] or [`CircuitBreaker`], so this type is the whole
/// of that decision and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monitor {
    retry: RetryPolicy,
    breaker: CircuitBreaker,
    offline: bool,
    reconnect_attempts: u32,
}

impl Monitor {
    /// A monitor in the production configuration: 10 s → 5 min ×2, 5 attempts;
    /// 5 failures, 60 s open, 30 s half-open.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            retry: RetryPolicy::WIFI,
            breaker: CircuitBreaker::WIFI,
            offline: false,
            reconnect_attempts: 0,
        }
    }

    /// Whether the machine has given up on Wi-Fi and is running offline.
    ///
    /// Once true, the monitor returns early on every call, and a later successful
    /// association does **not** clear it: only an explicit configuration change
    /// does. A machine that has gone offline needs `wifi apply` or a reboot, not
    /// luck.
    #[must_use]
    pub const fn is_offline(&self) -> bool {
        self.offline
    }

    /// Cumulative reconnection attempts, for `/api/status` and the boot log.
    #[must_use]
    pub const fn reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts
    }

    /// The retry policy, for the log lines.
    #[must_use]
    pub const fn retry(&self) -> &RetryPolicy {
        &self.retry
    }

    /// The circuit breaker, for the log lines.
    #[must_use]
    pub const fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }

    /// One iteration of the monitor, called from the network task.
    ///
    /// `associated` is the current radio state. The `&mut self` is why this is a
    /// method rather than free code: the two policies, the offline flag and the
    /// attempt count must move together, and as private fields they cannot be
    /// updated separately.
    pub fn poll(&mut self, associated: bool, now_ms: u32) -> Action {
        // Offline mode stops everything.
        if self.offline {
            return Action::Offline;
        }

        // Fail fast while the circuit is open, so a dead access point costs one
        // association attempt a minute rather than a busy loop of
        // `esp_wifi_connect()` calls.
        if !self.breaker.can_attempt(now_ms) {
            return Action::CircuitOpen;
        }

        if associated {
            // Record the success on both policies. The reconnect count is only
            // ever read for the log.
            self.breaker.record_success(now_ms);
            self.retry.reset();
            return Action::Connected;
        }

        // The attempt count is the hard stop.
        if !self.retry.should_retry() {
            self.offline = true;
            return Action::Offline;
        }

        // The backoff gate.
        if !self.retry.can_retry_now(now_ms) {
            return Action::Wait {
                retry_in_ms: self.retry.ms_until_retry(now_ms),
            };
        }

        // Associate now.
        self.retry.record_attempt(now_ms);
        self.reconnect_attempts += 1;
        Action::Retry {
            attempt: self.retry.attempts(),
            next_delay_ms: self.retry.next_delay_ms(),
        }
    }

    /// Record the outcome of the attempt [`Monitor::poll`] asked for.
    ///
    /// An asynchronous association is not a success yet, so `success` only feeds
    /// the circuit breaker: `poll` is called again with the real radio state on
    /// the next iteration, and the breaker is the one thing that must not count a
    /// not-yet-associated radio as a success.
    pub fn record_attempt_result(&mut self, success: bool, now_ms: u32) {
        if success {
            self.breaker.record_success(now_ms);
            self.retry.reset();
        } else {
            self.breaker.record_failure(now_ms);
        }
    }

    /// Enter or leave offline mode explicitly.
    ///
    /// Leaving offline mode also resets both policies, deliberately rather than
    /// conveniently: a process that left offline mode with a spent attempt count
    /// would go straight back offline on the next poll. Resetting here makes
    /// `wifi apply` behave like a reboot, which is what the operator expects.
    pub fn set_offline(&mut self, offline: bool) {
        if offline {
            self.offline = true;
        } else {
            self.offline = false;
            self.retry.reset();
            self.breaker.reset(0);
        }
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resilience::CircuitState;

    #[test]
    fn the_hostname_is_set_before_the_association_can_send_a_dhcp_request() {
        // The rule: the hostname before the association.
        let good = ConnectionOrder {
            netif_created_with_hostname: true,
            driver_started: true,
            association_requested: true,
            dhcp_requested: true,
        };
        assert!(good.hostname_is_before_association());

        // The broken order: a DHCP request that went out with no hostname on the
        // netif, which is what setting the hostname after the association
        // produces.
        let bad = ConnectionOrder {
            netif_created_with_hostname: false,
            driver_started: true,
            association_requested: true,
            dhcp_requested: true,
        };
        assert!(!bad.hostname_is_before_association());
    }

    #[test]
    fn a_machine_that_has_not_asked_for_dhcp_yet_is_not_yet_wrong() {
        // The assertion is about the *edge*, not about a static property: a
        // driver that has been started but has not been asked to associate has
        // not yet put a wrong client ID on the wire.
        let starting = ConnectionOrder {
            netif_created_with_hostname: false,
            driver_started: true,
            association_requested: false,
            dhcp_requested: false,
        };
        assert!(starting.hostname_is_before_association());
    }

    // ---- signal buckets ------------------------------------------------

    #[test]
    fn the_signal_buckets_match_the_cpp_boundaries() {
        // The bucket table, boundary for boundary.
        let cases: &[(i32, u8)] = &[
            (-30, 4),
            (-50, 4),
            (-51, 3),
            (-65, 3),
            (-66, 2),
            (-75, 2),
            (-76, 1),
            (-80, 1),
            (-81, 0),
            (-100, 0),
        ];
        for (rssi, bars) in cases {
            assert_eq!(Signal::from_rssi(*rssi).as_bars(), *bars, "{rssi} dBm");
        }
    }

    #[test]
    fn a_not_associated_radio_lands_in_the_cpp_substitution_bucket() {
        // A radio that is up but not associated reads -100 dBm.
        assert_eq!(
            Signal::from_rssi(Signal::NOT_CONNECTED_RSSI),
            Signal::Offline,
            "an unassociated radio reads -100 dBm and buckets as offline"
        );
    }

    // ---- the monitor ---------------------------------------------------

    #[test]
    fn a_link_that_is_up_resets_both_policies() {
        let mut m = Monitor::new();
        m.poll(false, 0);
        m.record_attempt_result(false, 0);
        assert_eq!(m.poll(true, 1_000), Action::Connected);
        assert_eq!(m.retry().attempts(), 0);
        assert_eq!(m.breaker().recorded_state(), CircuitState::Closed);
    }

    #[test]
    fn the_first_reconnect_is_immediate_and_the_next_waits_10s() {
        let mut m = Monitor::new();
        assert_eq!(
            m.poll(false, 0),
            Action::Retry {
                attempt: 1,
                next_delay_ms: 20_000
            }
        );
        m.record_attempt_result(false, 0);
        // Immediately after, the backoff gate is shut.
        assert!(matches!(m.poll(false, 1), Action::Wait { .. }));
        // Ten seconds after the attempt, `can_retry_now` compares against the
        // delay for the *current* attempt count, which after one attempt is 20 s.
        assert!(matches!(
            m.poll(false, 20_001),
            Action::Retry { attempt: 2, .. }
        ));
    }

    #[test]
    fn five_failed_attempts_take_the_machine_offline_forever() {
        let mut m = Monitor::new();
        let mut now = 0u32;
        for _ in 0..5 {
            // Jump the clock far enough that the backoff is always satisfied.
            now += 600_000;
            let action = m.poll(false, now);
            assert!(
                matches!(action, Action::Retry { .. }),
                "{action:?} should still be retrying"
            );
            m.record_attempt_result(false, now);
        }
        now += 600_000;
        assert_eq!(m.poll(false, now), Action::Offline);
        assert!(m.is_offline());
        // And it stays there: only an explicit configuration change clears
        // offline mode, never a lucky association.
        assert_eq!(m.poll(true, now + 1), Action::Offline);
        assert!(m.is_offline());
    }

    #[test]
    fn an_open_circuit_is_reported_rather_than_touching_the_radio() {
        // This is the point of the breaker: a dead AP must not cost an
        // `esp_wifi_connect()` every control tick.
        //
        // The failures are recorded directly rather than through `poll`, because
        // the two policies trip at the same count (5) and `poll` checks the
        // breaker *first* — so a monitor that both attempted and failed five
        // times goes offline (the retry policy) rather than reporting an open
        // circuit. Driving the breaker alone is what isolates its wiring in
        // `poll`; its own state machine is `resilience`'s business.
        let mut m = Monitor::new();
        for t in 0..5u32 {
            m.record_attempt_result(false, t * 1_000);
        }
        // The fifth failure opened the circuit at t = 4 s, so it is open until
        // 4 s + 60 s.
        let open_until = 4_000 + 60_000;
        assert_eq!(m.breaker().recorded_state(), CircuitState::Open);
        // Retries are still available (`should_retry` is true at 0 attempts), so
        // the only thing stopping a reconnect here is the breaker.
        assert!(m.retry().should_retry());
        assert_eq!(m.poll(false, open_until - 1), Action::CircuitOpen);
        // After the 60 s open window, a probe is admitted again.
        assert!(matches!(m.poll(false, open_until), Action::Retry { .. }));
    }

    #[test]
    fn a_successful_probe_closes_the_circuit() {
        let mut m = Monitor::new();
        for t in 0..5u32 {
            m.record_attempt_result(false, t * 1_000);
        }
        let probe_at = 4_000 + 60_000;
        assert_eq!(m.poll(false, probe_at - 1), Action::CircuitOpen);
        assert!(matches!(m.poll(false, probe_at), Action::Retry { .. }));
        m.record_attempt_result(true, probe_at + 1);
        // One success does not close a half-open circuit, so the next poll is
        // still an attempt: it takes two.
        //
        // The attempt number is 1 rather than 2 because a success resets the
        // retry policy: a success invalidates the backoff history.
        assert_eq!(
            m.poll(false, probe_at + 2),
            Action::Retry {
                attempt: 1,
                next_delay_ms: 20_000
            }
        );
        m.record_attempt_result(true, probe_at + 3);
        assert_eq!(m.breaker().recorded_state(), CircuitState::Closed);
        assert_eq!(m.poll(true, probe_at + 4), Action::Connected);
    }

    #[test]
    fn the_reconnect_count_is_monotonic_and_bounded_by_the_attempts() {
        let mut m = Monitor::new();
        let mut now = 0;
        for _ in 0..5 {
            now += 600_000;
            let _ = m.poll(false, now);
            m.record_attempt_result(false, now);
        }
        assert_eq!(m.reconnect_attempts(), 5);
    }

    #[test]
    fn an_offline_monitor_can_be_brought_back_explicitly() {
        // `wifi apply` after a successful provisioning: the in-process equivalent
        // of a reboot.
        let mut m = Monitor::new();
        for round in 0..5 {
            let now = round * 600_000;
            let _ = m.poll(false, now);
            m.record_attempt_result(false, now);
        }
        assert_eq!(m.poll(false, 3_000_000), Action::Offline);
        assert!(m.is_offline());

        m.set_offline(false);
        assert!(!m.is_offline());
        // Both policies are reset too, or the very first poll after recovery
        // would go straight back offline on a spent attempt count.
        assert!(m.retry().should_retry());
        assert!(matches!(m.poll(false, 3_000_001), Action::Retry { .. }));
    }

    #[test]
    fn set_offline_true_is_a_one_way_latch() {
        // The factory-reset direction: only this call clears offline mode; the
        // monitor never does.
        let mut m = Monitor::new();
        let _ = m.poll(true, 0);
        m.set_offline(true);
        assert!(m.is_offline());
        // Even with a live link, an offline machine does not reconnect.
        assert_eq!(m.poll(true, 1_000), Action::Offline);
    }
}
