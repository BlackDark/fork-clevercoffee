# Open work

Ordered by value. Remove an item when it lands.

## Needs the bench

- Measure the runtime heap low-water mark per profile (portal + station + MQTT + two UI tabs + BLE scale) and set a floor in `tools/bench`.
- Measure the control-loop worst case with `system.timing_debug.enabled` during an OTA upload and a BLE connect.
- TSIC: confirm edge timing under WiFi load (parity error rate in the log).
- Confirm the access-point fallback and the return to station-only after reconnecting (today the portal stays open until restart).

## Features

- Rotary encoder (GPIO 3/4/5) as an optional feature: menu navigation on the display.
- Zero-crossing input (GPIO 18) for phase-controlled pump pressure profiling.
- Observability endpoints behind a feature: `/api/metrics` (heap, loop timing, WiFi, uptime), `/api/logs` (ring buffer).
- Shared API types with `ts-rs`: generate the UI's TypeScript types from `cc-api`.
- Bootloader rollback: build with an ESP-IDF bootloader that has rollback enabled; the firmware already confirms images at boot.
- ESP32-S3 and C6 boards: chip features exist in `firmware/Cargo.toml`, pins need a per-board table.
- Web UI: drop the remaining C++-era fields the firmware does not send; size budget once it moves to Svelte (agent-presets `stack/embedded-ui.md`).

## Display

- Desktop simulator (embedded-graphics-simulator) for interactive layout work next to the PNG contact sheet (`just screens`).
- Fonts: the image uses the smallest u8g2 glyph set per font (~16 KB). German/Spanish need `_tf`; if more languages come, generate one subset font from the used strings instead.
