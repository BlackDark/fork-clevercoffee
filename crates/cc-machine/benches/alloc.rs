//! A counting global allocator, shared by the bench and the test.
//!
//! Included by the bench (which reports a number for a human to watch over time)
//! and by the matching `tests/` file (which gates the build). Keeping it in ONE
//! place matters: a gate that measures something subtly different from the
//! number a human watches is worse than no gate at all.
//!
//! Why allocations and not nanoseconds is argued on the bench's own header.

#![allow(
    unsafe_code,
    reason = "a GlobalAlloc impl is unsafe by definition and there is no safe \
              way to count allocations. Every block forwards the call and its \
              arguments verbatim to std::alloc::System -- the allocator that \
              would otherwise have run -- so the counters are a side effect and \
              not a change of behaviour."
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "a benchmark's per-iteration ratio; iteration and allocation \
              counts are far below the 2^53 where u64 stops being exact in f64"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

/// The counting allocator.
pub struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: forwarded verbatim to the system allocator with the same layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded verbatim to the system allocator with the same layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Allocations and bytes so far. Cumulative: callers read either side of a
/// window and subtract with [`delta_since`].
#[must_use]
pub fn counters() -> (u64, u64) {
    (
        ALLOCATIONS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    )
}

/// `(allocations, bytes)` since `mark`.
#[must_use]
pub fn delta_since(mark: (u64, u64)) -> (u64, u64) {
    let (allocs, bytes) = counters();
    (allocs - mark.0, bytes - mark.1)
}

/// `numerator / denominator` as an `f64`, for a per-iteration rate.
///
/// Only the bench prints ratios; the test asserts on the raw count. Both live in
/// this module so they cannot drift, so the unused-in-one-of-them half has to be
/// declared rather than deleted.
#[must_use]
#[allow(
    dead_code,
    reason = "used by the bench; this module is shared with the test, which only \
              needs the counts"
)]
pub fn per(numerator: u64, denominator: u64) -> f64 {
    numerator as f64 / denominator as f64
}

/// Nanoseconds per iteration. Bench-only, same reason as [`per`].
#[must_use]
#[allow(
    dead_code,
    reason = "used by the bench; this module is shared with the test, which only \
              needs the counts"
)]
pub fn nanos_per(elapsed: Duration, iterations: u64) -> f64 {
    elapsed.as_nanos() as f64 / iterations as f64
}

/// A measured quantity divided by an iteration count, for reporting a rate in
/// whatever unit the caller already has (seconds here, so a share of a refresh
/// budget can be printed without a second cast). Bench-only, same reason as
/// [`per`].
#[must_use]
#[allow(
    dead_code,
    reason = "used by the bench; this module is shared with the test, which only \
              needs the counts"
)]
pub fn measured_per(quantity: f64, iterations: u64) -> f64 {
    quantity / iterations as f64
}
