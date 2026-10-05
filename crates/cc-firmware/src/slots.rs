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
//! that decides the design: **a control loop must not block on a lock whose
//! critical section it cannot bound.** A 10 ms period that can become "however
//! long the display task takes to finish a frame" is not a 10 ms period, and the
//! safety paths (S1-S5) run inside it.
//!
//! "Cannot bound" is the whole of it. The rule is **not** "no mutex on the
//! control task": the control task takes three, on every tick for two of them,
//! and the next section is the measurement that says why each is affordable.
//!
//! So the hand-off is a **critical section**: one ~200-byte copy guarded by
//! `portENTER_CRITICAL`/`portEXIT_CRITICAL`, which is mutual exclusion between
//! the two tasks without either of them ever *waiting* on the other.
//!
//! Two lock-free designs were tried first and both are wrong, for reasons worth
//! keeping because they look right:
//!
//! * A **double buffer with an index** is only "disjoint by construction" until
//!   the producer laps the consumer -- which it does nine times per frame here,
//!   because 10 ms publishing divides 100 ms reading exactly.
//! * A **seqlock** is lock-free but is not sound in Rust: it gives atomicity of
//!   *observation*, not of *access*. The reader still reads a non-atomic
//!   location the writer may be overwriting, and throwing the copy away
//!   afterwards does not retroactively define the read.
//!
//! A critical section is the answer that is actually sound, and its cost is one
//! ~200-byte copy under interrupt masking: well under a microsecond against a
//! 10 ms period. A frame is a snapshot by definition, so a display task a few
//! milliseconds behind the machine is showing a slightly old screen -- which is
//! what a 100 ms display refresh means anyway.
//!
//! # 🔴 The rule, and every lock on the control task measured against it
//!
//! Stated exactly, the rule the control loop keeps is **do not block on a lock
//! whose critical section is unbounded, and do not block on a lock a
//! higher-priority task holds.** The two halves are separate and both are
//! needed; the second half is a number, and the first is arithmetic.
//!
//! ## The priorities, because the second half is a lookup
//!
//! | task | prio | where the number comes from |
//! | --- | --- | --- |
//! | lwIP `tcpip` | 18 | IDF's own default; not this firmware's to change |
//! | scale sampler | 6 | [`cc_hal_esp32::scale::SAMPLER_PRIO`] |
//! | **control** | **5** | [`cc_hal_esp32::task::CONTROL_PRIO`] |
//! | **httpd** | **5** | `esp-idf-svc-0.53.0/src/http/server.rs:161` |
//! | esp-mqtt | 4 | `cc-hal-esp32/src/mqtt.rs:142`, `MQTT_TASK_PRIO` (module-private) |
//! | provisioning | 4 | [`cc_hal_esp32::task::PROVISION_PRIO`] |
//! | display | 3 | [`cc_hal_esp32::task::DISPLAY_PRIO`] |
//! | telnet | 2 | [`cc_hal_esp32::task::TELNET_PRIO`] |
//!
//! **httpd runs at the same priority as the control task and
//! [`cc_hal_esp32::web::configuration`] cannot move it.** `esp-idf-svc`'s
//! `Configuration` has no priority field at all; its
//! `From<&Configuration> for Newtype<httpd_config_t>` writes `task_priority: 5`
//! as a literal. That settles the rule's second half in httpd's favour — it is
//! not a *higher*-priority holder, so none of the three locks below is a
//! priority inversion — but it also means priority inheritance is a no-op on
//! them, so the wait is bounded by the holder's own critical section and
//! nothing else. The only holder that is genuinely lower is provisioning, and
//! there inheritance boosts it to 5 and it releases immediately.
//!
//! ## What each of the three actually holds
//!
//! Scaled to a 240 MHz Xtensa at **8x pessimism** — no Xtensa figure here is a
//! device measurement, because measuring one means flashing the machine, which
//! the migration forbids. The scaling is a deliberately generous multiple of
//! the host/core-clock ratio, so each figure is an upper bound and not a guess
//! dressed as a number.
//!
//! ### [`cc_hal_esp32::task::ParameterHandoff::take_one`] — **every tick**
//!
//! Guards a `VecDeque` of 4 `Vec` fat pointers; the key/value pairs themselves
//! are on the heap, outside the lock. Control side is one `pop_front` of 12 B,
//! **<= 0.04 us**. Holder side is one `len` and one `push_back`,
//! **<= 0.5 us**.
//!
//! ### [`crate::network::Handoff::take`] — **every tick**
//!
//! Guards an `Option<Staged>`; both `String` bodies are on the heap, outside the
//! lock. Control side is one `Option::take` of at most 28 B, **<= 0.1 us**.
//! Holder side is one `is_some` and a 28 B store.
//!
//! ### [`cc_hal_esp32::web::Shared::push_history`] — **once a second**
//!
//! Guards a `Box<History>`, a 600-point ring of 12 B points. Control side is a
//! single 12 B point store, **< 0.01 us**. Holder side is the 7,200 B copy-out,
//! bounded below.
//!
//! The first two are the per-tick pair, and they are the reason this section
//! exists: they are taken 100 times a second and each costs under a microsecond
//! on **both** sides, so neither can be what makes the loop miss its period.
//!
//! The third is the one with a genuinely large critical section, and it is
//! **once a second, not once per tick** — it sits inside the
//! `SSE_INTERVAL_MS` gate (1000 ms, `main.rs:SSE_INTERVAL_MS`), which is
//! the C++'s `sendTempEvent` cadence and the same gate the SSE broadcast uses.
//! Finding 4.2 counted it as one of three per-tick locks; it is one in a hundred.
//!
//! ## The 7,200 B copy, which is the largest number in this file
//!
//! `history_json` (`cc-hal-esp32/src/web.rs`) holds the ring lock across a
//! 7,200-byte copy-out: 225 cold 32-byte DRAM lines. At a pessimistic 100 ns per
//! uncached line on this part that is **<= 22 us** — 0.2% of one tick, once a
//! second, and only while a browser is actually on `/api/history`.
//!
//! The host measurement of that same loop is **425-550 ns** (`size_of` on the
//! guarded types: `Point` 12 B, `History` 7,224 B). The 40x gap between the two
//! is the DRAM, and it is the reason the copy is written out here rather than
//! dismissed: on this chip the guarded value is real memory and not a register.
//!
//! ## The worst case, which is not any of the above
//!
//! Because `configUSE_TIME_SLICING` is 1 and `CONFIG_FREERTOS_HZ` is 100, an
//! **equal**-priority task switch happens on the 10 ms tick. So a control tick
//! that collides with an httpd critical section waits for the remainder of that
//! section and then for httpd to block — bounded by one 10 ms tick, not by the
//! section.
//!
//! That bound is reached only on collision, and the collision probability is the
//! hold time over the 10 ms period: **~5e-5 per tick** for the parameter mailbox,
//! **~1e-5..1e-4 per second** for the history ring. The loop absorbs a lost tick
//! rather than drifting, because step 9 sleeps
//! `CONTROL_PERIOD_MS.saturating_sub(elapsed)` (step 9) — a slow tick
//! shortens the next one instead of adding to it.
//!
//! ## What this rules out, and what it does not
//!
//! Ruled out by the numbers above: a `Cell` under `interrupt::free` for the
//! history ring, or a double buffer published by pointer swap. The ring is
//! appended once a second and read by a task that must be able to ask for it at
//! any moment, so a pointer swap buys a second lock (the swap) to replace a
//! section already bounded at <= 22 us — and it would put the 7,200 B on the
//! *control* side of the swap, which is the direction the numbers say is the
//! cheap one. Deletion over addition: the mutex stays.
//!
//! **Not established here, and not claimed:** that a contended `std::sync::Mutex`
//! between these particular tasks can trip the `xTaskRemoveFromEventList` assert.
//! The assert reproduced against `interrupt::free` from a second task
//! ([`crate::sensor_task`]), and against three blocking wake primitives tried in
//! the same experiment; whether it is reachable through a plain contended mutex
//! *between these two tasks* was not measured, because measuring it means
//! flashing. What is measured is that these locks have run, contended, on the
//! device through every 10 ms-loop build.
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
