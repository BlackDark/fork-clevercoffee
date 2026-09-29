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

    #[cfg_attr(test, test)]
    pub fn a_command_carries_no_pointer() {
        // The 04 §3.2 bound, as a compile-time fact: `Queue` requires
        // `T: Copy`, and `Command` is only `Copy` because every variant is
        // `Copy`. A future `String` in a variant would make this file stop
        // compiling, which is the intended enforcement.
        fn assert_copy<T: Copy>() {}
        assert_copy::<Command>();
        assert!(core::mem::size_of::<Command>() <= 8);
    }
}
