//! Forward ESP-IDF link arguments, cfg flags, and include paths to the binary.
//!
//! Cargo does not forward dependencies' `cargo:rustc-link-arg` directives.
//! `embuild::espidf::sysenv::output()` re-emits their `links` metadata, as the
//! official esp-rs template does. Keep `embuild` pinned to the version used by
//! `esp-idf-sys`; mismatches can corrupt link arguments without a compile error.

fn main() {
    embuild::espidf::sysenv::output();
}
