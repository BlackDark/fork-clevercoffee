# Open work

Ordered by value. Remove an item when it lands.

## Needs the bench

- Remeasure heap and stack (`minFreeHeap`, `minFreeStack`) per image after the 2026-10-24 RAM changes; with the stack margin known, move part of `full`'s main stack to the heap.
- Firmware upload speed with the 2 KB TCP window (35–45 KB/s with 1 KB; flash erases cap it near 60–70 KB/s).
- Heap low-water mark with a Bluetooth scale connected (`full`, two home-network workers) and for `minimal`; `standard` and `full` without a scale are in docs/firmware.md.
- Control-loop worst case with `system.timing_debug.enabled` during a BLE scale connect (a firmware upload: 50 ms, docs/firmware.md).
- TSIC: confirm edge timing under WiFi load (parity error rate in the log). The bench has a DS18B20: no read errors under HTTP and MQTT load, a few CRC errors during firmware uploads.
- Fallback end to end: home network down for 2 min (portal opens), back (the device restarts once idle, the portal is gone).

## Found on the bench, not fixed yet

- `xtensa-lx-rt`: fixed upstream the same way (esp-hal PR #6497, merged 2026-10-15), not released yet (crates.io: 0.23.0). Drop `firmware/vendor/xtensa-lx-rt` with the first release after it; `just vendor-check` fails as soon as a bump stops using the vendored copy.
- smoltcp has no listen backlog. Upstream draft PR smoltcp-rs/smoltcp#1120 drops every unmatched SYN and stalled on that behaviour change; ours drops only on ports a socket serves. Share the variant and the bench data there, then drop `firmware/vendor/smoltcp` once smoltcp has an opt-in or a listener (embassy-net 0.9 is on smoltcp 0.13; 0.14 is out).
- WiFi setup in the web UI: a "Connect to WiFi" card on the portal's home page with a network scan, instead of SSID and password under Configuration → System. Needs a scan endpoint.
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
