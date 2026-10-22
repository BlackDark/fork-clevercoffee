//! Thread-local allocation counters shared by the benchmark and regression gate.
//!
//! Process-wide counting includes libtest's concurrent allocations. Measure only
//! the calling thread; the reducer does not spawn work. Const-initialized,
//! destructor-free cells avoid allocator re-entry during counter access.

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
use std::cell::Cell;
use std::time::Duration;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
}

fn count(size: usize) {
    ALLOCATIONS.with(|n| n.set(n.get() + 1));
    BYTES.with(|n| n.set(n.get() + size as u64));
}

/// The counting allocator.
pub struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: forwarded verbatim to the system allocator with the same layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded verbatim to the system allocator with the same layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Allocations and bytes the calling thread has made so far. Cumulative:
/// callers read either side of a window and subtract with [`delta_since`].
///
/// Counting per thread, not per process — see the module header for the
/// measurement that forced it.
#[must_use]
pub fn counters() -> (u64, u64) {
    let allocs = ALLOCATIONS.with(Cell::get);
    let bytes = BYTES.with(Cell::get);
    (allocs, bytes)
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
