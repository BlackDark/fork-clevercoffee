# 0008: Connections and memory on the chip

Status: accepted, 2026-10

## Context

The first run on a DevKitC (docs/bench.md) found the web server and the RAM budget wrong in ways the host tests could not show:

- smoltcp answers a SYN with RST when no socket listens on the port. picoserve ties one listening socket to each HTTP worker, so a connection that arrived while every worker was busy was refused. The web UI opens about eight connections on load and holds one per tab for its event stream; with two STA workers it lost its logo and its event stream, and the bench's three parallel downloads failed.
- A worker costs RAM: its static future (3.8 KB after this work, 5.6 KB before) plus 4.6 KB of heap for socket and request buffers. The `full` image had 94 KB of heap and 9 KB left with MQTT and two tabs; three parallel downloads ran it dry, and a failed allocation is a panic.
- The ESP32 Bluetooth controller owns 64 KB of main RAM at a fixed address in every `ble-scale` build, used or not.

## Decision

- **Listen backlog by dropping SYNs.** `firmware/vendor/smoltcp` (0.13.1) drops a SYN to a port that has sockets, all busy, instead of answering RST. The client retransmits after about 1 s and gets the next free worker. No RAM per waiting client; a busy device is slower, not unreachable. The patch is ~15 lines in `iface/interface/tcp.rs`.
- **Workers**: 4 (2 with a Bluetooth scale), shared by the home network and the access point: each worker serves the network whose link is up and re-picks when it goes down. A boot uses one network at a time because the device restarts when the home network comes back after the fallback (once idle: `Status::idle_for_restart`). Picked from the measurements in docs/firmware.md: 4 keeps a page load with two open tabs under a few seconds. Until 2026-10-24 the access point had 3 workers of its own: 11 KB of static RAM a machine on its home network never used.
- **Event streams are capped** (`cc_web::StreamSlots`): half the workers. A further `/events` gets 503; the UI retries it and polls the status meanwhile. Requests always find a worker within one SYN retry.
- **Bluetooth RAM joins the heap** when no Bluetooth scale is configured, as ESP-IDF's `esp_bt_mem_release` does: `full` without a BLE scale has 158 KB of heap instead of 94 KB. Scale settings need a restart, so the controller can never start on memory the heap owns.
- **WiFi driver buffers** come from the same heap: 4 static RX (default 10), at most 8 dynamic RX and 8 TX (default 32 each).
- **Big allocations in request handlers are fallible** (history copy, config upload): 503 instead of a reboot. Every request but `/api/health` starts only with 8 KB of heap free (`cc_web::MIN_FREE_HEAP`), else 503 with `Retry-After`: the per-request allocations stay small and infallible, and the WiFi driver keeps room for its packet buffers.

## Consequences

- Two vendored crates (`xtensa-lx-rt` for the boot fix, `smoltcp`), both patched in one place and marked; remove them when upstream has a fix or a listen backlog (docs/todo.md).
- A third UI tab on the home network gets no event stream; its temperatures update every 10 s.
- `full` with a Bluetooth scale stays tight (about 18 KB free with MQTT, no tabs); not measured with a scale connected.
- `tools/bench` checks more connections than workers and a heap floor.

## Revision 2026-10-24

- One worker pool for both networks (above); `full`'s main stack grew from 26 to 38 KB.
- History points in hundredths (`i16`/`u16`): 3.6 KB instead of 7.2 KB, for the history and for each `/api/history` copy.
- Without `ble-scale` the main-RAM heap is 80 KB instead of 48: `full` served every bench test with a 26 KB main stack, `standard` keeps 36 KB. The firmware reports the stack's high-water mark (`minFreeStack`, `firmware/src/stackmon.rs`); the bench fails below 4 KB. With measurements from a Bluetooth scale connected, `full` can move part of its 38 KB stack to the heap the same way.
- TCP receive buffer 2 KB per worker (was 1 KB): it is the window a firmware upload gets.
