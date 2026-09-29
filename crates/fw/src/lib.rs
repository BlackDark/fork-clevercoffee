//! Shared firmware entry sequence.
//!
//! The three binaries differ only in which board they link, so the boot order lives here once.
//! The order is the safety property, not a convenience: see
//! docs/rust-migration/architecture.md section 1.4.

#![no_std]

/// Brings the machine up. Never returns.
pub async fn app_main() -> ! {
    // Placeholder until T-13 lands the board modules and T-14 the task graph. Kept as a
    // `!`-returning function so the call sites do not change when the body does.
    loop {
        core::future::pending::<()>().await
    }
}
