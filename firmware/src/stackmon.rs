//! Main-stack high-water mark. The thread-mode executor and every interrupt run on
//! the main stack; how much of it they use decides how much RAM can go to the heap
//! instead (ADR 0008). At boot the unused part is filled with a pattern; the lowest
//! overwritten word is the deepest the stack has been.

use core::sync::atomic::{AtomicU32, Ordering};

const PATTERN: u32 = 0xA5A5_A5A5;
/// Left untouched next to the current stack pointer while painting.
const MARGIN: usize = 512;
/// esp-hal's stack guard word sits this far above the stack's end.
const GUARD_OFFSET: usize = 60;

unsafe extern "C" {
    static _stack_start: u32;
    static _stack_end: u32;
}

/// Last measurement; 0 until the first. Painted words only ever get overwritten, so
/// a later scan never reports more free stack than an earlier one.
static LOWEST: AtomicU32 = AtomicU32::new(0);

fn bounds() -> (usize, usize) {
    // Linker symbols: only their addresses are used.
    (
        core::ptr::addr_of!(_stack_end) as usize + GUARD_OFFSET + 4,
        core::ptr::addr_of!(_stack_start) as usize,
    )
}

/// Size of the main stack in bytes.
pub fn size() -> usize {
    let (bottom, top) = bounds();
    top - bottom
}

/// Fills the free stack below the current frame. Call once, early in `main`.
pub fn paint() {
    let (bottom, _) = bounds();
    let here = 0u8;
    let sp = core::ptr::addr_of!(here) as usize;
    critical_section::with(|_| {
        let mut p = bottom;
        while p + 4 <= sp - MARGIN {
            // SAFETY: [bottom, sp - MARGIN) is main-stack memory below the deepest
            // live frame; interrupts are off, so nothing else uses it meanwhile.
            unsafe { (p as *mut u32).write_volatile(PATTERN) };
            p += 4;
        }
    });
}

/// The value of the last [`measure`], for readers that must not scan.
pub fn last() -> u32 {
    LOWEST.load(Ordering::Relaxed)
}

/// Bytes of the main stack never touched since boot (scans the stack).
pub fn measure() -> u32 {
    let (bottom, top) = bounds();
    let mut p = bottom;
    // SAFETY: reads within the main stack; a word still holding the pattern was never
    // written since `paint`.
    while p < top && unsafe { (p as *const u32).read_volatile() } == PATTERN {
        p += 4;
    }
    let free = (p - bottom) as u32;
    LOWEST.store(free, Ordering::Relaxed);
    free
}
