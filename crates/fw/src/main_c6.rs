//! Firmware entry point for the ESP32-C6 on the ESP32-C6-DevKitC-1 v1.2.
//!
//! This board has 14 usable GPIO against the 17 the machine needs, so the three indicator LEDs
//! and the second HX711 load cell are not available here. See
//! docs/rust-migration/board-pinouts.md.

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
