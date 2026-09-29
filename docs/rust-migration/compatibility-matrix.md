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

**The Xtensa toolchain is unavailable in the current checkout.** `just espup-install` fetches an
x86-64 `espup` binary and this host is aarch64, so it exits 126 and the `esp` rustc fork is never
installed. `just check-fw esp32` and `just check-fw esp32s3` therefore **cannot be run here**, and
those two targets' board crates and binaries are `unverified` rather than `build-verified`.

**The C6 is a different story and is now build-verified.** It is RISC-V, so it needs only a
`rustup target add`, which works on any host. `rustup target add --toolchain stable
riscv32imac-unknown-none-elf` followed by the `check-fw` command with `RUSTUP_TOOLCHAIN=stable`
compiles and lints `clevercoffee-fw` for the C6 with `-D warnings` and links a 1.76 MB image. The
`just check-fw esp32c6` recipe needs the same one-line change to be runnable on a host without the
Xtensa fork, and that change is the obvious next thing to make.

The host test suite passes in full, and it is the only behavioural evidence in this checkout.
Whoever picks this up on an x86-64 host should run `just setup` and then all three `just check-fw`
targets before trusting the ESP32 and S3 board crates.

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
| DS18B20 temperature reading | host-tested, unverified on device | host-tested, unverified on device | host-tested, unverified on device | No device. |
| OLED rendering on SSD1306 / SH1106 | framebuffer host-tested, no panel driver | same | same | No device, and no SSD1306 bus driver is written. |
| Switch debounce and long press | host-tested, unverified on device | same | host-tested, input pins compiled, unverified on device | No device. |
| Relay actuation | host-tested against a recorder; pin driver written but uncompiled | same | host-tested against a recorder; **pin driver compiled**, not flashed | No device. |
| Web API over a connection | host-tested end to end over the bridge | same | same | No socket has carried a byte; the Wi-Fi association is not written. |
| Heater power control | host-tested policy, **compiled** for the C6 only | same, uncompiled | same, compiled | Hardware PWM at a 10 ms window, no interrupt. The ESP32 and S3 pins are uncompiled. |
| Web API parity | host-tested, 31 route tests | same | same | The socket layer is not written. |
| Config import | host-tested | host-tested | host-tested | The device half is written and host-tested; not run on a device. |
| Control scenarios (brew, backflush, faults, deadlines) | host-tested, 21 scenarios | same | host-tested **and compiled** | The pin drivers behind them are build-unverified. |
| Configuration load from flash | host-tested, including the region format and the export round trip | same | **compiled and linked** | The partition read is compiled; no flash has been read. |
| Home Assistant discovery documents | host-tested | same | same | No broker, and the client is not chosen. |
| MQTT and Home Assistant discovery | host-tested against generated documents | same | same | No broker, and the client is not chosen. |
| `just wifi` and `just config-import` | host-tested protocol, unverified on a device | same | same | No device. |
| Boot pin map on a real board | host-tested as data, **board crate uncompiled** | host-tested as data, **board crate uncompiled** | host-tested as data and **board crate compiled and linked**, not flashed | No device. The C6 compile is the only chip-level evidence in this checkout. |

## Per-chip differences that affect the design

| Difference | ESP32 | ESP32-S3 | ESP32-C6 |
| --- | --- | --- | --- |
| Architecture | Xtensa LX6, needs the Espressif rustc fork | Xtensa LX7, same fork | RISC-V, uses stable rustup |
| Native USB | none | USB Serial/JTAG | USB Serial/JTAG |
| GPIO count | 0-39, of which 34-39 are input-only | 0-21 and 26-48 (**no GPIO22-25**), none input-only | 0-30, none input-only, but the dev board exposes only 16 |
| Pins this project needs | 17, fits | 17, fits | 17 needed, **14 usable on ESP32-C6-DevKitC-1** |
| Dev board | ESP32-DevKitC V4 | ESP32-S3-DevKitC-1 v1.1 | ESP32-C6-DevKitC-1 v1.2 |
| PSRAM | none on this board | available | available |
| USB OTG device | no | no | no |
| OTA and provisioning over USB | UART0 through the USB-serial bridge | native CDC | native CDC |

The GPIO gap matters, and the evidence is in
[board-pinouts.md](board-pinouts.md). Three findings:

1. **The C++ pin map cannot be reused on S3 or C6.** S3 has no GPIO22-25, so the SDA 21 / SCL 22
   assignment is gone. The full map for each board is in
   [board-pinouts.md](board-pinouts.md#5-proposed-pin-maps).
2. **The input-only pin constraint is ESP32-only.** The four panel switches sit on GPIO34, 35, 36
   and 39 with no internal pull, so the board must supply external resistors. S3 and C6 have no
   input-only pins, so the same signals can use internal pulls there.
3. **The C6 dev board does not have enough pins for the full feature set.** 23 GPIOs are exposed,
   and 14 are usable after removing the flash bus, USB and the RGB LED. The essential 11 signals
   plus a single-cell HX711 fit, with one spare; the three indicator LEDs and the second load
   cell do not. Resolved by the user on 2026-09-29: those two features are disabled on the C6.

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
