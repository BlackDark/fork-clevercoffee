//! Build script for the on-target test runner.
//!
//! Identical in purpose to `cc-firmware/build.rs` and for the same reason: the
//! ESP-IDF archives that `esp-idf-sys` builds are published as `links`
//! metadata, which Cargo does not forward to a package that merely depends on
//! it. `embuild::espidf::sysenv::output()` re-emits them as this package's own
//! link arguments.
//!
//! `embuild` is pinned to the version `esp-idf-sys` 0.38.1 builds with, for the
//! same reason it is pinned there: a mismatch shows up as missing or garbled
//! link arguments, not as a clean compile error.

fn main() {
    embuild::espidf::sysenv::output();
}
