# 0008: Connections and memory on the chip

Status: accepted, 2026-10

## Context

The first run on a DevKitC (docs/bench.md) found the web server and the RAM budget wrong in ways the host tests could not show:

- smoltcp answers a SYN with RST when no socket listens on the port. picoserve ties one listening socket to each HTTP worker, so a connection that arrived while every worker was busy was refused. The web UI opens about eight connections on load and holds one per tab for its event stream; with two STA workers it lost its logo and its event stream, and the bench's three parallel downloads failed.
- A worker costs RAM: its static future (3.8 KB after this work, 5.6 KB before) plus 4.6 KB of heap for socket and request buffers. The `full` image had 94 KB of heap and 9 KB left with MQTT and two tabs; three parallel downloads ran it dry, and a failed allocation is a panic.
- The ESP32 Bluetooth controller owns 64 KB of main RAM at a fixed address in every `ble-scale` build, used or not.

## Decision

- **Listen backlog by dropping SYNs.** `firmware/vendor/smoltcp` (0.13.1) drops a SYN to a port that has sockets, all busy, instead of answering RST. The client retransmits after about 1 s and gets the next free worker. No RAM per waiting client; a busy device is slower, not unreachable. The patch is ~15 lines in `iface/interface/tcp.rs`.
- **Workers**: 4 on the home network, 3 on the access point. The access point's workers allocate their buffers only once the access point is up, so they cost no heap otherwise. Picked from the measurements in docs/firmware.md: 4 keeps a page load with two open tabs under a few seconds; more would cost 8.4 KB each.
- **Event streams are capped** (`cc_web::StreamSlots`): 2 of the 4 home-network workers, 1 of the 3 on the access point. A further `/events` gets 503; the UI retries it and polls the status meanwhile. Requests always find a worker within one SYN retry.
- **Bluetooth RAM joins the heap** when no Bluetooth scale is configured, as ESP-IDF's `esp_bt_mem_release` does: `full` without a BLE scale has 158 KB of heap instead of 94 KB. Scale settings need a restart, so the controller can never start on memory the heap owns.
- **WiFi driver buffers** come from the same heap: 4 static RX (default 10), at most 8 dynamic RX and 8 TX (default 32 each).
- **Big allocations in request handlers are fallible** (history copy, config upload): 503 instead of a reboot.

## Consequences

- Two vendored crates (`xtensa-lx-rt` for the boot fix, `smoltcp`), both patched in one place and marked; remove them when upstream has a fix or a listen backlog (docs/todo.md).
- A third UI tab on the home network gets no event stream; its temperatures update every 10 s.
- `full` with a Bluetooth scale stays tight (about 18 KB free with MQTT, no tabs); not measured with a scale connected.
- `tools/bench` checks more connections than workers and a heap floor.
