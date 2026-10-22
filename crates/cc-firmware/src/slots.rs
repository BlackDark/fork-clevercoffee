//! Bounded hand-off from control to display; machine and configuration keep one owner.
//!
//! A critical section protects one frame-input copy. A double buffer is unsafe
//! when the 10 ms producer laps the 100 ms consumer; a seqlock still permits
//! non-atomic Rust data races even if the reader discards a torn copy.
//!
//! Do not block control on an unbounded critical section or a higher-priority holder.
//! The control task and httpd both run at priority 5; `esp-idf-svc` hard-codes httpd's
//! priority. Provisioning runs below control and benefits from priority inheritance.
//!
//! # Control-task lock budget
//!
//! Host-scaled estimates below assume 240 MHz Xtensa with 8x pessimism; they are
//! not device measurements:
//!
//! * Parameter mailbox, every tick: 12-byte pop, <= 0.04 µs; holder push, <= 0.5 µs.
//! * Provisioning hand-off, every tick: at most 28 bytes, <= 0.1 µs;
//!   string bodies stay outside.
//! * History: one 12-byte point per second. HTTP copies out 7,200 bytes under the
//!   lock only for `/api/history`: 225 cold 32-byte lines at 100 ns each,
//!   about 22.5 µs. Host copy: 425–550 ns.
//!
//! Equal-priority time slicing can still add one 10 ms scheduler tick on collision.
//! The loop subtracts elapsed work from its sleep to avoid cumulative drift.
//! Moving history into an interrupt-masked cell or double buffer would move the
//! large copy onto control without improving this budget.
//!
//! The kernel assert in [`crate::sensor_task`] was reproduced with cross-task
//! critical sections and blocking wake primitives, not isolated to these contended
//! mutexes. Keep that distinction when evaluating another synchronisation scheme.

#![allow(
    unsafe_code,
    reason = "single-producer/single-consumer frame buffer shared between the \
              control and display tasks; the only `unsafe` left is the Sync \
              impl, and its justification is on the impl"
)]

use core::cell::Cell as StdCell;

use cc_display::model::DisplayInput;
use esp_idf_hal::interrupt;

/// What the display task needs, published by the control task.
///
/// The [`DisplayInput`] is the whole of a frame's inputs; the two flags are the
/// two decisions the display task cannot make for itself, because both are
/// functions of control state it does not own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameRequest {
    /// The inputs for one frame.
    pub input: DisplayInput,
    /// Whether to blank the panel while keeping its last frame in RAM.
    pub blank: bool,
    /// The display's own view of the configuration.
    ///
    /// Published rather than shared because the configuration has exactly one
    /// owner — the control task, which also owns the store — and fifteen of
    /// these flags (`display.pid_off_logo`, `display.heating_logo`, the
    /// fullscreen switches, the scale and pressure enables) change at runtime
    /// through `POST /api/parameters`. A display task holding its own copy would
    /// be a second copy that goes stale; holding a `&Config` would be a second
    /// owner.
    pub config: cc_display::model::Config,
}

/// The frame hand-off.
pub struct FrameSlot {
    /// The frame itself. One writer (the control task), one reader (the display
    /// task), and every access inside `interrupt::free` -- see [`Self::publish`].
    frame: StdCell<FrameRequest>,
    /// Set once the control task has published a frame, so a display task that
    /// wakes before the first tick gets `None` rather than a blank default it
    /// might mistake for a real frame.
    ///
    /// The payload is `Copy`, so reading it is a plain copy with no `Drop`; the
    /// flag is therefore only ever cleared by a reset, never by a reader.
    published: core::sync::atomic::AtomicBool,
}

// SAFETY: the only ways to reach `frame` are `publish` and `frame`, and both run
// their whole copy inside `interrupt::free` (`portENTER_CRITICAL` /
// `portEXIT_CRITICAL`). On this single-core target that is mutual exclusion
// between the control task and the display task, and it excludes the ISR too, so
// no access can overlap another.
//
// This is a *different* answer from the one this module used to give, and the
// difference matters. The previous version was a seqlock: an `AtomicUsize`
// stamped odd before the payload and even after, with readers retrying if it
// moved. That is **not** sound. A seqlock gives you atomicity of *observation*,
// not of *access* -- the reader still reads a non-atomic location while the
// writer may be overwriting it, and discarding the copy afterwards does not
// retroactively make the read defined. Under Rust's memory model that is a data
// race, and `FrameRequest` embeds a `cc_display::model::Config` whose fields a
// reader would act on, so a torn read here is a control decision made from
// garbage, not a cosmetic glitch.
//
// `frame` is module-private, `StdCell::as_ptr` is never called (grep for it: if
// that grep ever hits, this `Sync` is void), and neither method calls out to
// anything that can block inside the critical section.
unsafe impl Sync for FrameSlot {}
// SAFETY: `&FrameSlot` exposes no interior mutability outside the critical
// sections documented above, and `&mut FrameSlot` is exclusive by Rust's own
// rules.
unsafe impl Send for FrameSlot {}

impl Default for FrameSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameSlot {
    /// A slot with no frame in it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: StdCell::new(FrameRequest::default()),
            published: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Publish a frame. Called by the control task, and only it.
    ///
    /// **Why a critical section, and not the two obvious alternatives.**
    ///
    /// *Not a mutex.* A `std::sync::Mutex` on this build is a `pthread` mutex and
    /// so a `FreeRTOS` one; contending for it is a queue operation, and the
    /// device asserted in a loop:
    ///
    /// ```text
    /// assert failed: xTaskRemoveFromEventList tasks.c:3894 (pxUnblockedTCB)
    /// ```
    ///
    /// Beyond that kernel bug, there is a reason that is not about kernels and
    /// is the one that decides the design: **a control loop must not block on a
    /// lock whose critical section it cannot bound.** A 10 ms period that can
    /// become "however long the display task takes to finish a frame" is not a
    /// 10 ms period, and the safety paths (S1-S5) run inside it. The rule's two
    /// halves, the priorities that settle the second one, and the measurement
    /// of every lock the control task does take are in the module docs under
    /// "The rule, and every lock on the control task measured against it".
    ///
    /// *Not a double buffer.* Two buffers and an index are only "disjoint by
    /// construction" until the producer wraps onto the slot the consumer is
    /// reading. This producer publishes every **10 ms** and this consumer reads
    /// every **100 ms**, so the producer laps the consumer nine times per frame
    /// *by design*. The cadences divide exactly, which also puts the display
    /// task's wake on a control-task tick boundary, maximising the chance of
    /// being preempted mid-copy. That is a data race with no synchronisation
    /// object at all.
    ///
    /// *Not a seqlock.* It is lock-free and it is still a data race; see the
    /// `Sync` impl above.
    ///
    /// `interrupt::free` is `portENTER_CRITICAL`/`portEXIT_CRITICAL`: not a
    /// blocking primitive (nothing lands on a semaphore event list, so the assert
    /// above is unreachable), and the section is one ~200-byte store, over in
    /// well under a microsecond against a 10 ms period. A display task that
    /// happens to hold the section delays the control tick by that much and not
    /// by "however long it takes to finish a frame".
    ///
    /// The trade is that a reader can no longer be told "not right now" — the
    /// old seqlock returned `None` when a write did not settle within four
    /// attempts. It never needed to: a critical section cannot be "not ready",
    /// and a frame is a snapshot by definition, so a display task a few
    /// milliseconds behind the machine is showing a slightly old screen, which is
    /// what a 100 ms refresh means anyway.
    pub fn publish(&self, request: FrameRequest) {
        interrupt::free(|| {
            self.frame.set(request);
            self.published
                .store(true, core::sync::atomic::Ordering::Release);
        });
    }

    /// The current frame, or `None` before the control task has published one.
    ///
    /// Called by the display task, and only it.
    ///
    /// **A peek, not a consume.** `FrameRequest` is `Copy`, so this is
    /// `Cell::get` -- a copy that leaves the slot holding what it held. It was
    /// `Cell::take`, which is move-and-reset, so it left `FrameRequest::default()`
    /// behind: the control task publishes every 10 ms and the display reads
    /// every 100 ms, so a second read landing before the next publish rendered
    /// a blank frame over a live screen. The `published` field's doc promises
    /// the flag is "never cleared by a reader", and with `take` the payload was
    /// cleared by every reader anyway.
    #[must_use]
    pub fn frame(&self) -> Option<FrameRequest> {
        if !self.published.load(core::sync::atomic::Ordering::Acquire) {
            return None;
        }
        Some(interrupt::free(|| self.frame.get()))
    }
}
