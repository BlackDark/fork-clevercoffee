# Compatibility Matrix

Three targets, three verification levels. Only **device-verified** counts as fully supported.

| Level | Meaning |
| --- | --- |
| `device-verified` | Built, flashed, and exercised on physical hardware. |
| `build-verified` | Compiled and linked for the target from this repository; behaviour not observed. |
| `unverified` | Not built, not compiled, or the evidence is indirect. |

**No hardware was connected for this run.** The original ESP32 is therefore `build-verified` and
not device-verified, and S3 and C6 are `build-verified`. Every task that needs a device is
blocked on hardware being available; see [task-list.md](task-list.md).

Cross-links: [decision-record.md](decision-record.md), [inventory.md](inventory.md),
[architecture.md](architecture.md), [tooling.md](tooling.md).

---

## Build matrix

Produced by the spikes in [`spikes/`](../../spikes) on 2026-09-28 with `esp-hal` 1.2.2,
`esp-rtos` 0.4.0, `esp-radio` 1.0.0-beta.1, `esp-storage` 0.10.0,
`esp-bootloader-esp-idf` 0.6.0, rustc 1.97.0-nightly (Espressif Xtensa fork) and
rustc 1.98.1 stable (RISC-V).

| Capability | ESP32 | ESP32-S3 | ESP32-C6 | Evidence |
| --- | --- | --- | --- | --- |
| Toolchain present | `esp` toolchain (Xtensa) | `esp` toolchain (Xtensa) | `stable` + `riscv32imac` target | `spikes/hal-smoke/rust-toolchain.toml`, `just espup-install` |
| `esp-hal` init, embassy executor, timer | build-verified | build-verified | build-verified | `spikes/hal-smoke` builds for all three targets |
| Flash access and partition table read | build-verified | build-verified | build-verified | `spikes/stack-smoke` calls `FlashStorage::new` and `read_partition_table` |
| I2C master (OLED) | build-verified, pins GPIO21/22 | build-verified, pins GPIO8/9 | build-verified, pins GPIO8/9 | `spikes/stack-smoke`; **the C++ pin map does not exist on S3/C6** |
| UART0 (original ESP32 provisioning channel) | build-verified | build-verified | build-verified | `spikes/stack-smoke` constructs `Uart` on `UART0` |
| USB Serial/JTAG CDC (S3/C6 provisioning channel) | **unavailable**, no native USB on ESP32 | build-verified | build-verified | `spikes/usb-smoke` builds for S3 and C6 only |
| Wi-Fi station via `esp-radio` | build-verified | build-verified | build-verified | `spikes/stack-smoke`; `esp-radio` support table lists all three |
| `embassy-net` TCP/IP stack | build-verified | build-verified | build-verified | `spikes/stack-smoke` links `embassy_net::new` |
| `espflash save-image` | verified locally | needs target hardware to flash | needs target hardware to flash | produced a 99 728-byte image from the ESP32 ELF |
| `espflash write-bin` at a fixed address | documented, not executed | documented, not executed | documented, not executed | espflash 4.6.0 CLI |

## Runtime and behaviour matrix

| Area | ESP32 | ESP32-S3 | ESP32-C6 | Blocker |
| --- | --- | --- | --- | --- |
| DS18B20 temperature reading | unverified | unverified | unverified | No device. Driver not written yet. |
| OLED rendering on SSD1306 / SH1106 | unverified | unverified | unverified | No device. Driver not written yet. |
| Switch debounce and long press | unverified | unverified | unverified | No device. |
| Relay actuation | unverified | unverified | unverified | No device, and no actuator hardware. |
| Heater PWM timing | unverified | unverified | unverified | No device. |
| Web API parity | host-testable | host-testable | host-testable | Not written yet. |
| Config import | host-testable | host-testable | host-testable | Not written yet. |
| `just wifi` and `just config-import` | unverified | unverified | unverified | No device. |

## Per-chip differences that affect the design

| Difference | ESP32 | ESP32-S3 | ESP32-C6 |
| --- | --- | --- | --- |
| Architecture | Xtensa LX6, needs the Espressif rustc fork | Xtensa LX7, same fork | RISC-V, uses stable rustup |
| Native USB | none | USB Serial/JTAG | USB Serial/JTAG |
| GPIO count | 0-39 | 0-21, 26-48 (**no GPIO22-25**) | 0-30 |
| PSRAM | none on this board | available | available |
| USB OTG device | no | no | no |
| OTA and provisioning over USB | UART0 through the USB-serial bridge | native CDC | native CDC |

The GPIO gap matters: the C++ pin map assigns SDA 21, SCL 22, valve 17, pump 27, heater 2,
switches 34, 35, 36, 39 and water tank 23. **None of GPIO22-25 exist on the S3**, and the C6 has
no input-only pins in the ESP32 sense. Any hardware configuration for S3 or C6 is therefore a
new board definition, not a port of the existing one. The task list treats per-board pin maps as
a separate, explicitly board-scoped task.

## Library-level compatibility

| C++ library | Rust replacement | Status |
| --- | --- | --- |
| Arduino-PID (vendored) | own PID, host-testable | not written |
| U8g2 | own framebuffer plus font metrics, or `ssd1306` | `ssd1306` support for SH1106 and partial updates is unverified |
| DallasTemperature + OneWire | `clevercoffee-onewire` + `clevercoffee-ds18b20`, ours | not written; decision recorded |
| ZACwire (TSIC) | own crate | not written; only needed if TSIC is kept |
| HX711_ADC | own crate | not written |
| AcaiaArduinoBLE | TrouBLE plus a client, or drop | deferred, problem feature P3 |
| PubSubClient | `rumqttc`, or own client | not started |
| ArduinoJson | `serde_json` with fixed-capacity types | not started |
| ESPAsyncWebServer | `clevercoffee-http`, ours | not written; decision recorded |
| NimBLE-Arduino | TrouBLE | only needed if P3 is kept |
| WiFiManager | dropped, replaced by USB provisioning | needs user confirmation |
| Preferences / NVS | own versioned config region | decision recorded |
| LittleFS | not needed, assets live in a dedicated flash region | decision recorded |
