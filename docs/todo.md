# Open work

Ordered by value. Remove an item when it lands.

## Needs the bench

- Heap and stack with a Bluetooth scale connected (`full`, two home-network workers); the other images are in docs/firmware.md. Then move part of `full`'s main stack (22 KB never used without a scale) to the heap.
- Control-loop worst case with `system.timing_debug.enabled` during a BLE scale connect (a firmware upload: 50 ms, docs/firmware.md).
- TSIC: confirm edge timing under WiFi load (parity error rate in the log). The bench has a DS18B20: no read errors under HTTP and MQTT load; during a firmware upload enough CRC errors for a brief SENSOR_ERROR (the relays are off then anyway).
- DS18B20 on the bench: about 2 % of readings fail their CRC at idle (34 per 10 min over a 4 h soak, evenly spread; none were logged on 2026-10-23). Check the 4.7 kΩ pull-up and wire length, then the bit timing under the network load of the listener build.
- Fallback while brewing: the restart after the home network returns must wait for the shot (host-tested; on the bench only the idle case ran: down 2 min → portal, back → restart 55 s later, station only).

## Found on the bench, not fixed yet

- `xtensa-lx-rt`: fixed upstream the same way (esp-hal PR #6497, merged 2026-10-15), not released yet (crates.io: 0.23.0). Drop `firmware/vendor/xtensa-lx-rt` with the first release after it; `just vendor-check` fails as soon as a bump stops using the vendored copy.
- smoltcp has no listen backlog. Ours (`tcp::Listener` queues SYNs, `Socket::accept` replays one into an idle socket) follows the design a maintainer proposed on smoltcp-rs/smoltcp#1133 (closed); propose it upstream with the bench data, together with `TcpListener` for embassy-net, then drop `firmware/vendor/{smoltcp,embassy-net}` (embassy-net 0.9 is on smoltcp 0.13; 0.14 is out). The drafts written for #1120 describe the older SYN-drop variant.
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
