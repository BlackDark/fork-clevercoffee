# 0001: no_std on Embassy, not ESP-IDF std

Status: accepted, 2026-10

## Context

The firmware needs WiFi (station + access point), an HTTP server, captive DNS/DHCP, mDNS, MQTT and optionally BLE, on a classic ESP32 (~320 KB data RAM). Rust offers ESP-IDF with `std` (FreeRTOS, C components) or bare-metal `esp-hal` + Embassy. We had no data on whether bare metal fits everything into RAM.

## Decision

Bare metal: `esp-hal`, `esp-rtos` (Embassy executor), `esp-radio` (WiFi, BLE), `embassy-net`, `picoserve`, `rust-mqtt`, `edge-*`, `trouble-host`. Fallback was `std` if the RAM test failed.

A test build with everything enabled linked with a 28 KB main stack and ~118 KB heap. The real firmware:

- `standard`: heap 94 KB (reclaimed bootloader RAM) + 48 KB, main stack ~57 KB.
- `full`: the Bluetooth controller reserves 64 KB of main RAM at a fixed address. Network buffers and the 7 KB history live on the heap, not in static task futures; no second heap region. Main stack ~27 KB.

## Consequences

- One language and toolchain from drivers to UI server; everything above the HAL is testable on the host.
- No C build, no sdkconfig; image 1.15–1.54 MB.
- Some crates are young (esp-radio 1.0 beta). Pinned exactly in `firmware/Cargo.toml`; `firmware/ld/rom-fixups.x` works around missing BT ROM symbols.
- RAM is tight in `full`: new static buffers must go to the heap (AGENTS.md).
- Runtime heap use is still to be measured on the bench (docs/todo.md).
