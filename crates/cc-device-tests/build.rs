//! Forward ESP-IDF `links` metadata to the test binary's final link.
//!
//! Cargo does not propagate dependency link-argument directives. Keep this in
//! step with `cc-firmware/build.rs` and `esp-idf-sys`'s `embuild` version.

fn main() {
    embuild::espidf::sysenv::output();
}
