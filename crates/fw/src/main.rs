//! Firmware entry point for the ESP32.
//!
//! The board and the provisioning transport are selected at compile time, and selecting more
//! than one of either is a build error rather than a runtime surprise.

#![no_std]
#![no_main]

use esp_backtrace as _;

use clevercoffee_fw::app_main;
use embassy_executor::Spawner;

esp_bootloader_esp_idf::esp_app_desc!();

#[path = "checks.rs"]
mod checks;

#[esp_hal::main]
async fn main(_spawner: Spawner) -> ! {
    app_main().await
}
