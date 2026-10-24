# Open work

Ordered by value. Remove an item when it lands.

## Needs the bench

- Heap low-water mark with a Bluetooth scale connected (`full`, two home-network workers) and for `minimal`; `standard` and `full` without a scale are in docs/firmware.md.
- Control-loop worst case with `system.timing_debug.enabled` during a BLE scale connect (a firmware upload: 50 ms, docs/firmware.md).
- TSIC: confirm edge timing under WiFi load (parity error rate in the log). The bench has a DS18B20: no read errors under HTTP and MQTT load, a few CRC errors during firmware uploads.
- Fallback end to end: home network down for 2 min (portal opens), back (the device restarts once idle, the portal is gone).

## Found on the bench, not fixed yet

- Report the `xtensa-lx-rt` `global_asm!` ordering upstream; drop `firmware/vendor/xtensa-lx-rt` once fixed.
- smoltcp has no listen backlog; propose the SYN-drop (ADR 0008) or a backlog upstream, then drop `firmware/vendor/smoltcp`.
- WiFi setup in the web UI: a "Connect to WiFi" card on the portal's home page with a network scan, instead of SSID and password under Configuration → System. Needs a scan endpoint.
- Firmware upload runs at 35–45 KB/s (1 KB TCP receive window per worker); a larger window for the upload would halve the time.
- The MQTT session drops during a firmware upload (flash stalls) and reconnects after it.

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
