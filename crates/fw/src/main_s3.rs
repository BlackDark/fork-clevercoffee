//! Firmware entry point for the ESP32-S3 on the ESP32-S3-DevKitC-1 v1.1.

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
