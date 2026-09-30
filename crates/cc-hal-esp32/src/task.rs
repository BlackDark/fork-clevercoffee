//! The cross-task command channel: network → control.
//!
//! Owner: **R3-14** (task E).
//!
//! # What this is, and why it is a wrapper
//!
//! 04 §3.2 is unambiguous: *"Commands into the control task (brew start,
//! setpoint change, factory reset) arrive on a bounded
//! `hal::task::queue::Queue<Command, 32>` drained at the top of every tick. Never
//! a direct call from a network task into control state — that is how the C++
//! code couples `LoopManager` to `MQTTManager` and `WebServerManager`."*
//!
//! [`hal::task::queue::Queue`] is bounded by `T: Copy`
//! (`esp-idf-hal` 0.47 `task.rs:980`), so [`crate::web::Command`] is `Copy` and
//! carries no `String`, no `Vec` and no `Box`. The wrapper exists to give the
//! two policies that matter names: **drop-newest** on a full queue, and a
//! `no_std`-compatible error that is not an ESP-IDF code.
//!
//! # Drop-newest, and why not drop-oldest
//!
//! 04 §3.2 says `Event` drops oldest (freshness wins) and `Command` drops
//! newest (the newest request is the most relevant). A `POST /api/setpoint`
//! arriving while the queue is full of older setpoints must win, or the UI
//! shows a value the machine never applied.
//!
//! # Why a parameter write does not travel as a [`Command`]
//!
//! `POST /api/parameters` carries a *list of key/value pairs*, and a `Command`
//! cannot: `Queue<T>` is bounded by `T: Copy` (`esp-idf-hal` 0.47 `task.rs:978`,
//! "ensures the contained elements are not `Drop`") and 04 §3.2 turns that into a
//! rule — *"No `String`, no `Vec`, no `Box` in a cross-task message."* A `Vec` of
//! pairs in a `Command` does not compile, and inlining the values into fixed
//! arrays would cap the value length at a number chosen by the queue's
//! ergonomics rather than by the schema.
//!
//! So the pairs travel in [`ParameterHandoff`], a bounded mailbox that is
//! drained at the same point in the tick as the queue. **The rule is still
//! applied in one place**: `cc_config::assign::apply` is what the control task
//! calls, and it is also what R3-13's inbound MQTT will call — MQTT being
//! *on* the control task already, so it hands over nothing.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use std::sync::Mutex;

use esp_idf_hal::task::queue::Queue;
use esp_idf_svc::sys::EspError;

use crate::web::Command;

/// The queue depth. 04 §3.2: `Queue<Command, 32>`.
pub const COMMAND_QUEUE_DEPTH: usize = 32;

/// A bounded, non-blocking, drop-newest channel from the network tasks to the
/// control task.
///
/// `Queue::send` takes a `TickType_t` timeout
/// (`esp-idf-hal` 0.47 `task.rs:1030`); [`CommandQueue::try_send`] passes 0, so
/// a producer never blocks. That is the whole point: a web request must not be
/// able to stall because the control loop is mid-tick.
pub struct CommandQueue {
    inner: Queue<Command>,
}

impl CommandQueue {
    /// Create the queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Queue::new(COMMAND_QUEUE_DEPTH),
        }
    }

    /// Offer a command, dropping it if the queue is full.
    ///
    /// Returns `true` if it was queued. A `false` is **not** an error the
    /// caller should surface: the queue being full means the control loop is
    /// behind, which is a condition to shed, not to report. The HTTP handler
    /// answers `202 Accepted` either way, because the request was understood.
    ///
    /// `&self`, not `&mut self`: `Queue::send_back` takes `&self`
    /// (`esp-idf-hal` 0.47 `task.rs:1030`) because the `FreeRTOS` queue is
    /// internally synchronised, so a producer needs no exclusive access. That is
    /// what lets a `&CommandQueue` be captured by a `Send + 'static` HTTP
    /// handler without a lock.
    #[must_use]
    pub fn try_send(&self, command: Command) -> bool {
        // The `bool` is "was a higher-priority task awoken", which in a
        // non-ISR context is always `false`; the *error* is the signal that the
        // queue was full. A zero timeout means the only way to fail is a full
        // queue, which is the drop-newest case.
        self.inner.send_back(command, 0).is_ok()
    }

    /// Take the next command, without blocking.
    #[must_use]
    pub fn recv(&self) -> Option<Command> {
        self.inner.recv_front(0).map(|(item, _)| item)
    }

    /// How many commands are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl core::fmt::Debug for CommandQueue {
    /// The depth, never the contents.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CommandQueue({} queued)", self.len())
    }
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a queue or fail.
///
/// # Errors
///
/// `ESP_ERR_NO_MEM` if `FreeRTOS` could not allocate the queue. It is a separate
/// function so `CommandQueue::new` can stay `const` and the failure is reported
/// at boot rather than at the first request.
///
/// `Queue::new` is infallible in `esp-idf-hal` 0.47 (`task.rs:981`), so this
/// never fails today; it exists so that when it does, the call site is already
/// written.
pub fn command_queue() -> Result<CommandQueue, EspError> {
    Ok(CommandQueue::new())
}

/// How many parameter-write requests may be waiting for the control task.
///
/// Four, and it is not a tuning knob: the control task drains the whole mailbox
/// at the top of every tick, so four requests is four ticks' worth of backlog
/// and the fifth is a client that is posting faster than 100 ms. The bound
/// exists so a client looping on `POST /api/parameters` cannot grow the heap; it
/// is reached by four browser tabs, not by a person.
pub const STAGED_PARAMETER_DEPTH: usize = 4;

/// One `POST /api/parameters`, already validated by the handler.
///
/// `pairs` is what survived [`crate::web::classify_parameters`], so the control
/// task re-applies rather than re-decides: the two go through the same
/// [`cc_config::assign::parse`] and the second is idempotent.
pub type ParameterRequest = Vec<(String, String)>;

/// A bounded mailbox for parameter writes, network tasks → control task.
///
/// The same shape as [`CommandQueue`] and for the same reason, with one
/// difference in policy. [`CommandQueue`] **drops the newest** command when it is
/// full, because a superseded setpoint is the least interesting thing in the
/// machine. A parameter write cannot be dropped: the HTTP response has already
/// been sent, and an operator who set six parameters and watched `200` has been
/// told they were set. So this **refuses** instead, and
/// [`ParameterHandoff::stage`] returning `false` is what the handler turns into
/// a `503` — a visible error rather than a silent loss.
///
/// A `std::sync::Mutex` and not a `FreeRTOS` one for the reason
/// `network::Handoff` gives: the critical section is a `VecDeque::push_back`, and
/// holding a `FreeRTOS` mutex across that is a priority inversion bought for
/// nothing. A poisoned mailbox returns empty to the control task and refuses to
/// the producer, so neither side can act on half a request.
#[derive(Clone, Default)]
pub struct ParameterHandoff(alloc::sync::Arc<Mutex<VecDeque<ParameterRequest>>>);

impl ParameterHandoff {
    /// A mailbox with nothing staged.
    #[must_use]
    pub fn new() -> Self {
        Self(alloc::sync::Arc::new(Mutex::new(VecDeque::new())))
    }

    /// Stage a request for the control task.
    ///
    /// Returns `false` when the mailbox is full or poisoned, in which case
    /// nothing is staged and the caller owes the client an error.
    #[must_use]
    pub fn stage(&self, request: ParameterRequest) -> bool {
        let Ok(mut slot) = self.0.lock() else {
            return false;
        };
        if slot.len() >= STAGED_PARAMETER_DEPTH {
            return false;
        }
        slot.push_back(request);
        true
    }

    /// Take everything staged, oldest first. Called by the control task.
    ///
    /// Draining all of them in one tick is deliberate: four `POST`s that arrived
    /// together are four independent requests, and answering them in a later tick
    /// than the one after would make the last one wait 400 ms for nothing.
    #[must_use]
    pub fn take_all(&self) -> Vec<ParameterRequest> {
        let Ok(mut slot) = self.0.lock() else {
            return Vec::new();
        };
        slot.drain(..).collect()
    }

    /// How many requests are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.lock().map_or(0, |slot| slot.len())
    }

    /// Whether nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl core::fmt::Debug for ParameterHandoff {
    /// The depth, never the contents — a parameter value may be a credential.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ParameterHandoff({} staged)", self.len())
    }
}

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

    use super::*;
    use alloc::vec;

    #[cfg_attr(test, test)]
    pub fn a_command_carries_no_pointer() {
        // The 04 §3.2 bound, as a compile-time fact: `Queue` requires
        // `T: Copy`, and `Command` is only `Copy` because every variant is
        // `Copy`. A future `String` in a variant would make this file stop
        // compiling, which is the intended enforcement — and it is why a
        // parameter write travels in `ParameterHandoff` instead.
        fn assert_copy<T: Copy>() {}
        assert_copy::<Command>();
        assert!(core::mem::size_of::<Command>() <= 8);
    }

    #[cfg_attr(test, test)]
    pub fn a_staged_parameter_request_reaches_the_control_task() {
        let handoff = ParameterHandoff::new();
        assert!(handoff.is_empty());
        assert!(handoff.stage(vec![("pid.enabled".into(), "1".into())]));
        assert_eq!(handoff.len(), 1);
        let taken = handoff.take_all();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0][0].0, "pid.enabled");
        assert!(handoff.is_empty(), "taking empties it");
        assert!(handoff.take_all().is_empty());
    }

    #[cfg_attr(test, test)]
    pub fn staged_requests_are_drained_in_order_and_all_at_once() {
        // Four posts that arrived together are four independent requests, and
        // answering the last one a tick later than it needed to would be a
        // latency the design does not owe anybody.
        let handoff = ParameterHandoff::new();
        for port in ["1883", "1884", "8883", "1885"] {
            assert!(handoff.stage(vec![("mqtt.port".into(), port.into())]));
        }
        let taken = handoff.take_all();
        let ports: Vec<&str> = taken.iter().map(|r| r[0].1.as_str()).collect();
        assert_eq!(ports, ["1883", "1884", "8883", "1885"]);
    }

    #[cfg_attr(test, test)]
    pub fn a_full_parameter_mailbox_refuses_rather_than_dropping() {
        // The policy difference from `CommandQueue`, and the reason for it: a
        // command that is dropped was never promised to anyone, but a parameter
        // write whose `200` has already been sent was. A `false` here is what
        // the handler turns into a `503`.
        let handoff = ParameterHandoff::new();
        for _ in 0..STAGED_PARAMETER_DEPTH {
            assert!(handoff.stage(vec![("pid.enabled".into(), "1".into())]));
        }
        assert!(!handoff.stage(vec![("pid.enabled".into(), "0".into())]));
        assert_eq!(handoff.len(), STAGED_PARAMETER_DEPTH);
        // And the refusal did not overwrite what was already there.
        let taken = handoff.take_all();
        assert_eq!(taken.len(), STAGED_PARAMETER_DEPTH);
        assert!(taken.iter().all(|r| r[0].1 == "1"));
    }

    #[cfg_attr(test, test)]
    pub fn the_parameter_mailbox_prints_its_depth_and_never_its_contents() {
        // A parameter value may be `system.wifi.password`. `Handoff` makes the
        // same promise about a staged credential (`network.rs:524-530`).
        let handoff = ParameterHandoff::new();
        assert_eq!(alloc::format!("{handoff:?}"), "ParameterHandoff(0 staged)");
        assert!(handoff.stage(vec![("system.wifi.password".into(), "hunter2".into())]));
        let printed = alloc::format!("{handoff:?}");
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(printed.contains("1 staged"), "{printed}");
    }
}
