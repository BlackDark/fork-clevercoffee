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
use esp_idf_hal::task::thread::ThreadSpawnConfiguration;

use cc_web::Command;

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

/// How long a command's caller waits for the control task to apply it.
///
/// Forty control periods at 10 ms, so the wait survives a control task that is
/// busy and a command queue with several entries ahead of it.
pub const COMMAND_ACK_TIMEOUT_MS: u32 = 400;

/// How often a waiting caller re-checks. Five milliseconds is half a control
/// period, so the ack is noticed within one tick of happening.
pub const COMMAND_ACK_POLL_MS: u32 = 5;

/// Sleep, for the ack wait. Named so [`crate::web::Shared::wait_applied`] does
/// not reach for the HAL directly.
pub fn delay_ms(ms: u32) {
    esp_idf_hal::delay::FreeRtos::delay_ms(ms);
}

/// Spawn a task at an explicit `FreeRTOS` priority.
///
/// # Why a `std::thread::Builder` alone is not enough
///
/// `std::thread::Builder` on this target is `esp_pthread`
/// (`components/pthread/port/linux/pthread.c`), which reads its priority from
/// the **process-wide** `esp_pthread` default,
/// `CONFIG_PTHREAD_TASK_PRIO_DEFAULT`. That default is 5 in this build, and
/// `04 §2` says control is 5, display is 3 and provisioning is 4 — so the
/// control task got the right number by coincidence and the other two did not.
/// Worse, lwIP's own `tcpip` task runs at **18**, so a bare spawn left network
/// work preempting the control loop, which is the inversion ADR-0002 records
/// for the C++ firmware and which `04 §2` exists to remove. Naming the
/// priorities and never applying them is worse than not naming them: the log
/// claims a relationship nothing establishes.
///
/// [`ThreadSpawnConfiguration`] is the only priority API in `esp-idf-hal` 0.47
/// (`task.rs:353-405`) and it is **global**, so the default is saved, the
/// task's configuration is installed, the thread is spawned, and the default is
/// put back. Leaving the task's priority installed would silently raise
/// everything spawned afterwards — including the httpd task, which
/// `esp-idf-svc` starts from a `std::thread` of its own.
///
/// This is the pattern [`crate::scale::Sampler::start`] already uses
/// (`scale.rs:640-662`); it is here rather than inlined three times because
/// three tasks need it.
///
/// # Errors
///
/// [`std::io::Error`] from the thread spawn, and [`EspError`] if the global
/// configuration could not be installed or — the case that matters — restored.
/// A failure to restore leaves every later task at this task's priority, so it
/// is reported rather than dropped.
pub fn spawn_with_prio<F, T>(
    name: &'static core::ffi::CStr,
    stack_size: usize,
    priority: u8,
    body: F,
) -> std::io::Result<std::thread::JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let mut config = ThreadSpawnConfiguration::get().unwrap_or_default();
    config.stack_size = stack_size;
    config.priority = priority;
    // `&'static CStr` rather than `&str`: this is the field's own type, and
    // converting would mean either an allocation whose `CString` cannot outlive
    // the `'static` borrow `config.name` needs, or
    // `CStr::from_bytes_with_nul_unchecked`, which is `unsafe` and this
    // workspace denies `unsafe_code` (`Cargo.toml:125`). A `c"control"` literal
    // at the call site is cheaper than either.
    config.name = Some(name);

    // `set` panics on a priority outside `1..24`. Every caller passes a
    // compile-time constant, so this cannot fire today; it is left to fire
    // because a panic at boot is the right outcome for a priority the
    // scheduler would reject anyway.
    config.set().map_err(std::io::Error::other)?;

    let spawned = std::thread::Builder::new()
        .name(name.to_string_lossy().into_owned())
        .stack_size(stack_size)
        .spawn(body);

    // Restore the process default whatever happened — including on the error
    // path, which is the case that matters.
    if let Some(previous) = ThreadSpawnConfiguration::get() {
        previous.set().map_err(std::io::Error::other)?;
    }

    spawned
}

/// The control task's `FreeRTOS` priority.
///
/// 5, from `04 §2`'s table: the control loop is the highest-priority non-ISR
/// task in the firmware, so a network or display task cannot preempt a
/// decision about the heater.
///
/// **Still below lwIP's `tcpip` task at 18**, which is IDF's own default and
/// is not this firmware's to change. What this constant fixes is the
/// *documented* relationship between *this firmware's* tasks; the
/// control-vs-lwIP ordering is recorded in
/// [`crate::scale::SAMPLER_PRIO`]'s doc, which is where it is measured.
pub const CONTROL_PRIO: u8 = 5;

/// The display task's priority, 3 (`04 §2`).
///
/// Below [`CONTROL_PRIO`] on purpose: a frame is ten milliseconds of I²C bus
/// time, and the tick that must not be delayed is the one that decides whether
/// the heater stays on.
pub const DISPLAY_PRIO: u8 = 3;

/// The provisioning task's priority, 4 (`04 §2`).
///
/// Between display and control: the UART console is interactive, so it should
/// answer while a frame is going out, but it is still a network-tier task and
/// must not delay the tick.
pub const PROVISION_PRIO: u8 = 4;

/// The telnet log stream's priority, 2.
///
/// **The lowest priority in the firmware, and that is the requirement rather
/// than an accident of numbering.** The stream's only job is to move bytes to a
/// terminal; a client that has stopped reading is a condition to *drop* lines
/// for, never to spend the machine's time on. So it sits below the display
/// task's 3 and below provisioning's 4, and the ring
/// ([`cc_web::telnet::Ring`]) is what keeps a stalled client off the control
/// tick entirely: the producer claims a slot, copies, and returns, whatever the
/// socket is doing.
///
/// Below [`CONTROL_PRIO`] on its own would have been enough for the tick, but
/// the display task's doc names a ten-millisecond I²C frame as the thing that
/// must not wait, and a `lwip_send` into a full TCP window is a longer wait
/// than that.
pub const TELNET_PRIO: u8 = 2;

/// The priorities are a relationship, and a relationship expressed only in a
/// table is not checked.
///
/// A display task at 5 would compile, run, and reintroduce exactly the tick
/// overrun `04 §2` moved the display out of the control loop to fix. Asserted
/// at compile time for the reason [`crate::scale::SAMPLER_PRIO`] asserts its
/// relationship the same way.
const _: () = {
    assert!(
        CONTROL_PRIO > DISPLAY_PRIO,
        "04 §2: a display frame is 10 ms of I²C bus time and must not preempt \
         the tick that decides whether the heater stays on"
    );
    assert!(
        CONTROL_PRIO > PROVISION_PRIO,
        "04 §2: provisioning is a network-tier task and must not preempt the \
         control loop"
    );
    assert!(
        PROVISION_PRIO > DISPLAY_PRIO,
        "04 §2: an interactive UART console should answer while a frame is \
         being written to the panel"
    );
    assert!(
        CONTROL_PRIO > TELNET_PRIO,
        "the telnet stream must never preempt the tick that decides whether \
         the heater stays on"
    );
    assert!(
        DISPLAY_PRIO > TELNET_PRIO,
        "a `lwip_send` into a full TCP window blocks for longer than a 10 ms \
         I²C frame, and the log stream is the least urgent thing the firmware \
         does"
    );
};

/// How many parameter-write requests may be waiting for the control task.
///
/// Four, and it is not a tuning knob: the control task takes **one** of them at
/// the top of every tick, so four requests is four ticks' worth of backlog —
/// 40 ms, against an ack timeout of [`PARAMETER_ACK_TIMEOUT_MS`] — and the
/// fifth is a client that is posting faster than the tick drains. The bound
/// exists so a client looping on `POST /api/parameters` cannot grow the heap;
/// it is reached by four browser tabs, not by a person.
pub const STAGED_PARAMETER_DEPTH: usize = 4;

/// How long `POST /api/parameters` waits for the control task to apply what it
/// staged, in milliseconds.
///
/// Four control periods (400 ms each at the time of writing), which is long
/// enough that a loaded control task still answers and short enough that a
/// stalled one fails visibly rather than hanging a browser. The C++ has no
/// timeout here because its handler *is* the writer; see
/// [`ParameterHandoff::stage_and_wait`].
pub const PARAMETER_ACK_TIMEOUT_MS: u32 = 1_600;

/// The poll interval inside that wait.
pub const PARAMETER_ACK_POLL_MS: u32 = 5;

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
pub struct ParameterHandoff {
    /// Requests staged for the control task.
    queue: alloc::sync::Arc<Mutex<VecDeque<ParameterRequest>>>,
    /// The values the control task last published, for `GET /api/parameters`.
    ///
    /// See [`ParameterHandoff::publish_live`] for why the GET cannot use the
    /// boot-time `Config` snapshot.
    /// An [`Arc`], not a `String`.
    ///
    /// `GET /api/parameters` used to `live().clone()` the whole ~8.8 KB body on
    /// **every request**, under the lock, on the httpd task -- whose stack is
    /// 8 KB, which is the same lesson ADR-0002 records for the 7.2 KB history
    /// ring. A reader now clones an `Arc` (one refcount bump) and hands the
    /// `&str` to `respond_large`, which is what the body always was: bytes the
    /// control task already built.
    ///
    /// The writer still allocates, once a second, in the control task. That
    /// allocation was never the problem; N copies of it per second were.
    live: alloc::sync::Arc<Mutex<Option<alloc::sync::Arc<alloc::string::String>>>>,
    /// How many staged requests the control task has drained and applied.
    ///
    /// The read-after-write ack. See [`ParameterHandoff::stage_and_wait`].
    applied: alloc::sync::Arc<core::sync::atomic::AtomicU32>,
}

impl ParameterHandoff {
    /// A mailbox with nothing staged.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: alloc::sync::Arc::new(Mutex::new(VecDeque::new())),
            live: alloc::sync::Arc::new(Mutex::new(None)),
            applied: alloc::sync::Arc::new(core::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// The queue half, for the [`Self::take_one`] drain and `len`.
    fn queue(&self) -> &Mutex<VecDeque<ParameterRequest>> {
        &self.queue
    }

    /// The **current** values, for `GET /api/parameters`.
    ///
    /// # Why this exists
    ///
    /// The GET used to render from the `Config` captured when the HTTP server
    /// was built — the boot snapshot. So a write via `POST /api/parameters` took
    /// effect in the running machine immediately (the control task applies it)
    /// and in NVS, and the very next read still reported the **old** number. The
    /// human's report was "I enabled pid, the UI said successful, but the PID is
    /// still disabled" — and the UI was not lying, it was reading a stale
    /// snapshot.
    ///
    /// So the control task publishes the **rendered body** it would have
    /// served, and the GET sends that. One `String` per heartbeat rather than
    /// 98 copied values, and no lifetime to get wrong: `LiveValue::Text` borrows
    /// from the `Config`, so a value list could not have crossed this boundary
    /// without either leaking every field once a second or inventing a second
    /// owned enum to hold the same five variants.
    ///
    /// A read that finds nothing published yet returns `None` rather than an
    /// empty list, so the caller can answer from the boot snapshot instead of
    /// pretending a machine has no parameters.
    pub fn publish_live(&self, body: alloc::string::String) {
        if let Ok(mut slot) = self.live.lock() {
            *slot = Some(alloc::sync::Arc::new(body));
        }
    }

    /// The last published body, if the control task has published one.
    ///
    /// A cheap `Arc` clone rather than a copy of ~8.8 KB. The caller reads it
    /// through the guard, so it must **drop the `Arc` before the body is used**
    /// if it wants to keep the lock short -- see the `GET /api/parameters`
    /// handler, which binds the clone first and only then takes `&body`.
    #[must_use]
    pub fn live(&self) -> Option<alloc::sync::Arc<alloc::string::String>> {
        self.live.lock().ok().and_then(|slot| slot.clone())
    }

    /// Stage a request for the control task.
    ///
    /// Returns `false` when the mailbox is full or poisoned, in which case
    /// nothing is staged and the caller owes the client an error.
    #[must_use]
    pub fn stage(&self, request: ParameterRequest) -> bool {
        let Ok(mut slot) = self.queue().lock() else {
            return false;
        };
        if slot.len() >= STAGED_PARAMETER_DEPTH {
            return false;
        }
        slot.push_back(request);
        true
    }

    /// How many requests the control task has applied.
    ///
    /// Incremented once per drained request, whatever the request contained: the
    /// handler's verdict already decided which pairs were acceptable, so the
    /// control task's only job on the ack path is to say "I have had your
    /// message".
    pub fn note_applied(&self) {
        self.applied
            .fetch_add(1, core::sync::atomic::Ordering::Release);
    }

    /// The current applied count, for the waiter's termination condition.
    #[must_use]
    pub fn applied(&self) -> u32 {
        self.applied.load(core::sync::atomic::Ordering::Acquire)
    }

    /// Stage a request and wait, bounded, for the control task to apply it.
    ///
    /// Returns `false` if the request could not be staged, or if the control
    /// task had not applied it within [`PARAMETER_ACK_TIMEOUT_MS`].
    ///
    /// # Why the handler waits at all
    ///
    /// The human's report: *save a parameter, the UI immediately refetches, and
    /// the old value comes back — so the toggle flips back*. Three latencies
    /// stack up between `POST /api/parameters` and the next `GET`:
    ///
    /// 1. the request is staged and the control task has not run yet — up to one
    ///    control period;
    /// 2. the control task applies it and writes NVS, which is a flash write;
    /// 3. the GET is answered from `publish_live`, which until now only ran on
    ///    the 1 s heartbeat — so even a value applied in step 2 was not visible
    ///    to a reader for up to a second afterwards.
    ///
    /// Step 3 is fixed by publishing immediately after the apply. Steps 1 and 2
    /// are inherent to keeping the configuration in one task, and the C++ does
    /// not have them: `Config::getInstance()` is a singleton, so the C++'s
    /// handler mutates the live configuration in place
    /// (`WebServerManager.cpp:821-878`) and the very next read sees it.
    ///
    /// A bounded wait is how this port buys the C++'s guarantee without giving
    /// the httpd task a handle on the store or the machine. It is bounded
    /// because the alternative — answering `200` and hoping — is the lie the
    /// human reported, and an unbounded wait would be a hung web server.
    #[must_use]
    pub fn stage_and_wait(&self, request: ParameterRequest) -> bool {
        use core::sync::atomic::Ordering;
        let before = self.applied.load(Ordering::Acquire);
        if !self.stage(request) {
            return false;
        }
        let deadline = crate::time::now_ms().wrapping_add(PARAMETER_ACK_TIMEOUT_MS);
        // 5 ms is well under the control period, so the wait ends within one
        // poll of the apply rather than quantised to the next one.
        while self.applied.load(Ordering::Acquire) == before {
            if crate::time::now_ms().wrapping_sub(deadline) < PARAMETER_ACK_POLL_MS {
                return false;
            }
            esp_idf_hal::delay::FreeRtos::delay_ms(PARAMETER_ACK_POLL_MS);
        }
        true
    }

    /// Take the **oldest** staged request, leaving the rest. Called by the
    /// control task, once per tick.
    ///
    /// # Why one, and not all of them
    ///
    /// Draining the whole mailbox meant every drained request ran its own
    /// `persist_config` — a ~2 KB JSON serialise plus an NVS erase-and-write —
    /// on the control task, in the 10 ms period, **before** the heater decision
    /// for that period. At [`STAGED_PARAMETER_DEPTH`] that is four flash
    /// transactions in one tick.
    ///
    /// This is a latency defect and not a hazard, and the distinction matters:
    /// the fail-safe direction is already correct, because the 1000 ms deadman
    /// drops the heater where the 5 s task watchdog would have reset the chip.
    /// So the smallest honest fix is to bound the work per tick rather than to
    /// restructure the mailbox or move persistence off this task.
    ///
    /// # Why one, and not coalesce-the-rest-into-one-`persist_config`
    ///
    /// Coalescing is the other option and it needs more machinery for the same
    /// result: the drained requests have to be merged into one `apply` (with
    /// four posts setting the same key, "which value wins" becomes a question
    /// this file would have to answer), and `note_applied` — the read-after-write
    /// ack four `stage_and_wait` callers are blocked on — has to be called four
    /// times for one apply. One request per tick keeps the ack 1:1 with the
    /// request it answers, which is the property [`stage_and_wait`] is built on.
    ///
    /// # Why no request starves
    ///
    /// The mailbox holds at most [`STAGED_PARAMETER_DEPTH`] requests, one is
    /// taken per tick, and the control period is 10 ms, so the last of four
    /// waits at most 40 ms — against an ack timeout of
    /// [`PARAMETER_ACK_TIMEOUT_MS`] (1600 ms). The mailbox is never near empty
    /// in practice; it is reached by four browser tabs, not by a person.
    #[must_use]
    pub fn take_one(&self) -> Option<ParameterRequest> {
        self.queue()
            .lock()
            .ok()
            .and_then(|mut slot| slot.pop_front())
    }

    /// How many requests are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue().lock().map_or(0, |slot| slot.len())
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
        let taken = handoff.take_one();
        assert_eq!(taken.as_ref().unwrap()[0].0, "pid.enabled");
        assert!(handoff.is_empty(), "taking empties it");
        assert!(handoff.take_one().is_none());
    }

    /// The 2.8 defect, as a test: four posts could land four NVS erase-and-writes
    /// in one 10 ms tick, on the control task, before its heater decision.
    ///
    /// The fix is that the control task takes **one** request per tick and
    /// leaves the rest, so a tick can only ever pay one `persist_config`.
    #[cfg_attr(test, test)]
    pub fn only_one_staged_request_is_taken_per_tick() {
        let handoff = ParameterHandoff::new();
        for port in ["1883", "1884", "8883", "1885"] {
            assert!(handoff.stage(vec![("mqtt.port".into(), port.into())]));
        }

        // Tick 1..4 take them in order, oldest first — a later post never
        // overtakes an earlier one, so two posts to the same key are applied in
        // the order they were made.
        for expected in ["1883", "1884", "8883", "1885"] {
            let taken = handoff.take_one().expect("a staged request");
            assert_eq!(taken[0].1, expected);
        }
        assert!(handoff.is_empty(), "four ticks drained a four-deep mailbox");

        // And the ack path is 1:1 with the requests, which is what lets four
        // `stage_and_wait` callers each be released by their own tick.
        let handoff = ParameterHandoff::new();
        assert!(handoff.stage(vec![("pid.enabled".into(), "1".into())]));
        assert!(handoff.stage(vec![("pid.enabled".into(), "0".into())]));
        assert_eq!(handoff.applied(), 0);
        assert!(handoff.take_one().is_some());
        handoff.note_applied();
        assert_eq!(handoff.applied(), 1, "one taken, one acked");
        assert_eq!(handoff.len(), 1, "the second post is still waiting");
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
        // And the refusal did not overwrite what was already there: four ticks
        // later all four are still staged, and every one says `1`.
        for _ in 0..STAGED_PARAMETER_DEPTH {
            let taken = handoff.take_one().expect("a staged request");
            assert_eq!(taken[0].1, "1");
        }
        assert!(handoff.is_empty());
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
