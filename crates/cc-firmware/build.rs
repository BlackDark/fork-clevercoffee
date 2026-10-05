//! Build script for the Clever Coffee device binary.
//!
//! # Why this file exists
//!
//! `esp-idf-sys` builds ESP-IDF and then publishes the resulting link
//! arguments, `--cfg` flags and C include paths as *`links` metadata*
//! (`DEP_ESP_IDF_SYS_EMBUILD_LINK_ARGS`, `..._EMBUILD_CFG_ARGS`,
//! `..._EMBUILD_C_INCLUDE_ARGS`). Cargo does **not** forward a dependency's
//! `cargo:rustc-link-arg` directives to the packages that depend on it, so the
//! final link of the binary crate would contain no ESP-IDF archives at all and
//! fail with undefined references to `pthread_create`, `write`, `abort`,
//! `sched_yield` and the rest.
//!
//! `embuild::espidf::sysenv::output()` re-emits the propagated values as this
//! package's own build-script output, which is the only supported way to get
//! them onto the binary's link line. This is exactly what the official esp-rs
//! template does (`esp-idf-template/cargo/build.rs`).
//!
//! `embuild` must be the same version `esp-idf-sys` builds with — 0.33.5 for
//! esp-idf-sys 0.38.1. A mismatch shows up as missing/garbled link arguments,
//! not as a compile error, so the pin is exact (`=0.33.5`).

fn main() {
    embuild::espidf::sysenv::output();
}
