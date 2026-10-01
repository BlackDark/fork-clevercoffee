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

/// The frame hand-off, and the control task's wake channel.
pub struct FrameSlot {
    /// Which half of [`Self::frames`] is current.
    index: AtomicUsize,
    /// The two frame buffers. Written only by the control task, into the half
    /// the index does not name; read only by the display task, from the half it
    /// does. See the `Sync` impl.
    frames: UnsafeCell<[FrameRequest; 2]>,
    /// Whether a frame has been published at all.
    published: AtomicUsize,
}

// SAFETY: the module's whole design. `frames[i]` is written only by the control
// task while `index` names `1 - i`, and read only by the display task while it
// names `i` — disjoint by construction, and `FrameRequest` is `Copy`, so a
// buffer is never half-written in a way a reader could observe. Nothing here is
// reachable from an interrupt. The compiler cannot see the disjointness, which
// is the only reason the `unsafe` is here.
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
            index: AtomicUsize::new(0),
            frames: UnsafeCell::new([FrameRequest::default(); 2]),
            published: AtomicUsize::new(0),
        }
    }

    /// Publish a frame. Called by the control task, and only it.
    pub fn publish(&self, request: FrameRequest) {
        let current = self.index.load(Ordering::Relaxed);
        // SAFETY: the control task is the only writer, and it writes the half the
        // index does not name — the one the display task is not reading.
        unsafe {
            (*self.frames.get())[1 - current] = request;
        }
        // `Release`: the buffer is complete before it is published.
        self.index.store(1 - current, Ordering::Release);
        self.published.store(1, Ordering::Release);
    }

    /// The current frame, or `None` before the control task has published one.
    ///
    /// Called by the display task, and only it. `None` is the honest answer at
    /// boot: the panel then shows the boot screen rather than a frame of zeroes
    /// that looks like a machine at 0 °C.
    #[must_use]
    pub fn frame(&self) -> Option<FrameRequest> {
        if self.published.load(Ordering::Acquire) == 0 {
            return None;
        }
        let index = self.index.load(Ordering::Acquire);
        // SAFETY: the display task is the only reader, and the `Acquire` above
        // pairs with the control task's `Release`, so this buffer is complete and
        // the control task has moved on to the other one.
        Some(unsafe { *(*self.frames.get()).get(index)? })
    }
}
impl core::fmt::Debug for FrameSlot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // A frame is 200 bytes of numbers and the handle is a pointer; neither
        // belongs in a log line.
        f.debug_struct("FrameSlot")
            .field("published", &self.frame().is_some())
            .finish()
    }
}

// The invariants of this module are **not** host-unit-tested, and the reason is
// structural rather than an omission: `cc-firmware` is a *binary* crate, so its
// `#[test]` functions cannot be reached by the on-target runner either — the
// `test-audit` lint is right to reject a bare `#[test]` here, and there is no
// registration site for a bin crate.
//
// The behaviour that matters is the machine's, and it is verified on hardware:
// the display task's boot screen, its Wi-Fi screen, and then a live frame drawn
// from a frame the control task published, all visible in the serial log and on
// the panel.
