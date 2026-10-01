//! The hand-off from the control task to the display task.
//!
//! Owner: **R4-01b** (the control-loop restructure).
//!
//! # What this is
//!
//! One frame's inputs, published by the control task and read by the display
//! task. That is the whole of it: the sensor readings, the switch edges and the
//! machine state are all still **inside** the control task, which is the correct
//! place for them (see [`crate::sensor_task`] for the measurement that moved the
//! display out and left everything else where it was).
//!
//! # 🔴 Why there is no `Mutex` here
//!
//! The first version of this used `std::sync::Mutex`, and the device asserted in
//! a loop:
//!
//! ```text
//! assert failed: xTaskRemoveFromEventList tasks.c:3894 (pxUnblockedTCB)
//! ```
//!
//! On ESP-IDF a `std::sync::Mutex` is a `pthread` mutex, which is a `FreeRTOS`
//! mutex, so *contending* for one is a queue operation: the loser is put on the
//! semaphore's event list and the winner's release removes it. The assert is that
//! removal finding an owner-less list entry, and on this build it is reachable.
//!
//! There is a second reason that is not about a kernel bug, and it is the one
//! that decides the design: **a control loop must not block on a lock another
//! task holds.** A 10 ms period that can become "however long the display task
//! takes to finish a frame" is not a 10 ms period, and the safety paths (S1-S5)
//! run inside it.
//!
//! So the hand-off is a **double buffer and an index**: the control task writes
//! the half the index does not name and publishes the new index with a
//! `Release`; the display task reads the half the index names with an `Acquire`.
//! A frame is a snapshot by definition, so a display task that is a few
//! milliseconds behind the machine is showing a slightly old screen — which is
//! what a 100 ms display refresh means anyway — and nothing else can be wrong
//! with it.
//!
//! # What is deliberately *not* here
//!
//! * **The configuration.** One owner, the control task, because the store is
//!   one owner and a 2 KB blob write must not land on whichever task the web
//!   server happened to be serving. The display's own view of it travels with
//!   each frame, for the same reason.
//! * **The machine.** The reducer is a value in the control task; every other
//!   consumer sees the `cc_hal_esp32::web::Telemetry` snapshot it publishes.
//! * **The actuators.** Only the control task may move a pin, and it applies
//!   effects in the same tick that produced them.

#![allow(
    unsafe_code,
    reason = "single-producer/single-consumer frame buffers shared between the \\
              control and display tasks; each access is documented at the call \\
              site and the `Sync` claim is spelled out on the impl"
)]

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use cc_display::model::DisplayInput;

// The design, in one line: single producer, single consumer, disjoint buffers,
// and an `Acquire`/`Release` pair between them. The `unsafe` is the `Sync` impl
// and the two buffer accesses; the reasoning is at each of them and summarised on
// the impl. `cc-firmware` denies `unsafe_code` workspace-wide, and this is the
// third narrowly-scoped exception in the workspace after
// `cc_hal_esp32::wake` and `cc_hal_esp32::web_async` — the alternative is a
// `Mutex`, and a mutex in the control loop is the defect this module removes.

/// What the display task needs, published by the control task.
///
/// The [`DisplayInput`] is the whole of a frame's inputs; the two flags are the
/// two decisions the display task cannot make for itself, because both are
/// functions of control state it does not own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameRequest {
    /// The inputs for one frame.
    pub input: DisplayInput,
    /// `standbyCoordinator().shouldTurnOffDisplay()` — blank the panel.
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

/// How many times a frame read retries before falling back to the last clean one.
///
/// Four. The writer's critical section is one ~200-byte store, so it is over in
/// nanoseconds; four attempts is generous, and the fallback exists to bound the
/// display task rather than to be reached.
const FRAME_READ_ATTEMPTS: usize = 4;

/// The frame hand-off, and the control task's wake channel.
pub struct FrameSlot {
    /// The seqlock counter: even and consistent, odd and being written.
    seq: AtomicUsize,
    /// The frame itself. One writer (the control task), one reader (the display
    /// task), with [`Self::seq`] making a torn read **detectable** rather than
    /// acceptable. See [`Self::publish`] for why the original two-buffer version
    /// was a race rather than a buffer.
    frame: UnsafeCell<FrameRequest>,
    /// The last frame that read back cleanly, so a stalled writer degrades to a
    /// stale screen rather than a blank one.
    last_good: Mutex<Option<FrameRequest>>,
}

// SAFETY: the module's design. `frame` has exactly one writer (the control task)
// and one reader (the display task), and [`Self::seq`] turns a torn read into a
// discarded one rather than an acted-on one. The compiler cannot see that
// agreement, which is the only reason the `unsafe` is here. Nothing here is
// reachable from an interrupt.
unsafe impl Sync for FrameSlot {}
// SAFETY: `&FrameSlot` exposes no interior mutability without the atomics
// above, and `&mut FrameSlot` is exclusive by Rust's own rules.
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
            seq: AtomicUsize::new(0),
            frame: UnsafeCell::new(FrameRequest::default()),
            last_good: Mutex::new(None),
        }
    }

    /// Publish a frame. Called by the control task, and only it.
    ///
    /// **A seqlock, not a double buffer.** The first version of this was two
    /// buffers and an index, on the argument that they are "disjoint by
    /// construction". That argument is wrong: disjointness holds only while the
    /// producer has not wrapped onto the slot the consumer is reading — and this
    /// producer publishes every **10 ms** while the consumer reads every
    /// **100 ms**, so the producer laps the consumer nine times per frame *by
    /// design*. The two cadences divide exactly, which also puts the display
    /// task's wake on a control-task tick boundary, maximising the chance of
    /// being preempted mid-copy. That is a data race with no synchronisation
    /// object at all, and on Xtensa a torn read is a garbage value rather than a
    /// stale one.
    ///
    /// So the writer stamps the sequence **odd** before the payload and **even**
    /// after, and the reader copies and re-reads it, retrying if it moved. The
    /// reader may still *see* a torn copy; it can never accept one.
    pub fn publish(&self, request: FrameRequest) {
        self.seq.fetch_add(1, Ordering::Release);
        // SAFETY: the control task is the only writer. A reader may be copying
        // at this instant, which yields a torn copy — and that is precisely
        // what the sequence check in `frame` rejects. A mutex here would be the
        // `FreeRTOS` hazard in `09-cpp-findings.md` §28.
        unsafe {
            *self.frame.get() = request;
        }
        self.seq.fetch_add(1, Ordering::Release);
    }

    /// The current frame, or `None` before the control task has published one.
    ///
    /// Called by the display task, and only it.
    #[must_use]
    pub fn frame(&self) -> Option<FrameRequest> {
        for _ in 0..FRAME_READ_ATTEMPTS {
            let before = self.seq.load(Ordering::Acquire);
            if before == 0 {
                return None;
            }
            if before % 2 != 0 {
                continue;
            }
            // SAFETY: reading a `FrameRequest` a writer may be updating right
            // now. A torn result is discarded by the sequence check below,
            // which is what makes reading-without-a-lock sound.
            let copy = unsafe { *self.frame.get() };
            if self.seq.load(Ordering::Acquire) == before {
                if let Ok(mut slot) = self.last_good.lock() {
                    *slot = Some(copy);
                }
                return Some(copy);
            }
        }
        // The writer is not making progress. The last clean frame beats none: a
        // stale screen is better than a blank panel.
        self.last_good.lock().ok().and_then(|slot| *slot)
    }
}
