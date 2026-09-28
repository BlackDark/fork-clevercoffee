//! CleverCoffee firmware entry point for the original ESP32 (Xtensa LX6).
//!
//! Bring-up order is defined by `docs/rust-migration/architecture.md` section 4.1.
//! The first thing that must happen is driving the heater, pump and valve pins to
//! their inactive level, before anything fallible runs.

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("clevercoffee: bring-up not implemented yet");
    Ok(())
}
