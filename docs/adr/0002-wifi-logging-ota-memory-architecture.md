# ADR 0002: WiFi logging, OTA, and memory management on ESP32

## Status

Accepted (2026-05-23)

## Context

The ESP32 has ~320 KB total RAM. Several subsystems compete for heap during normal operation:

- **AsyncWebServer** (ESPAsyncWebServer + AsyncTCP) — handles HTTP on a dedicated FreeRTOS task
- **WiFi telnet logger** — streams log output to a TCP client on port 23
- **OTA updates** — receives firmware over HTTP, writes to flash
- **ArduinoJson** — serializes large config/parameter responses (~19 KB)
- **LittleFS** — serves gzipped SPA frontend assets

When a user opens the web UI with the WiFi log monitor connected, the browser fires 6–10 parallel API requests within 2 seconds of boot. This caused reproducible `abort()` crashes from failed `operator new` in `AsyncWebServerRequest::beginResponse`.

### Root causes identified

1. **Logger ring buffer too large** — 64 entries × 576 bytes = 37 KB of static RAM in the Logger singleton, leaving insufficient heap for dynamic allocations.

2. **Double-copy JSON serialization** — API endpoints (`/api/parameters`, `/api/history`, `/api/nvs-debug`) serialized JSON into a `JsonDocument`, then into a `String`, then passed the String to `request->send()` which copied it again into a response object. Three heap allocations of ~19 KB each.

3. **`std::function` OTA callbacks** — heap-allocated captures for simple function pointers.

4. **`WiFiClient::availableForWrite()` returns 0** — ESP32's `WiFiClient` does not override `Print::availableForWrite()`, which defaults to 0. A pre-write guard `if (avail <= 0) return;` silently dropped every log message to WiFi while Serial kept working. Only the welcome message and heartbeat bypassed this check.

5. **No telnet heartbeat** — with no data flowing at INFO level on an idle machine, the TCP connection died silently. PlatformIO's socket monitor disconnected and reconnected in a loop.

6. **Ring buffer data races** — `log()` is called from multiple FreeRTOS tasks (main loop on core 1, AsyncTCP on core 0). The ring tail index was loaded and advanced non-atomically, allowing two tasks to claim the same slot.

7. **Shared mutable state** — `timestampBuffer_` and `logBuffer_` were instance members written from any task concurrently. `localtime()` returns a pointer to static storage, also unsafe across tasks.

## Decision

### 1. Reduce Logger static footprint

Ring buffer: 16 entries × 304 bytes ≈ 5 KB (down from 37 KB). Format buffer: 256 bytes (down from 512). These sizes are sufficient — the main loop flushes up to 8 entries per iteration at 10+ Hz, and individual log lines rarely exceed 200 characters.

### 2. Stream large JSON via AsyncJsonResponse

`AsyncJsonResponse` (part of ESPAsyncWebServer) serializes JSON in chunks through `_fillBuffer()`. The `JsonDocument` lives inside the response object; no intermediate `String` copy is needed. `request->send(response)` transfers ownership to the web server.

Applied to `/api/parameters`, `/api/history`, `/api/nvs-debug`. Peak heap usage for the parameters response dropped from ~57 KB (3 × 19 KB) to ~19 KB (one `JsonDocument`).

### 3. Function pointers for OTA callbacks

`OtaSessionCallbacks` changed from `std::function<void()>` to `void (*)() noexcept`. The callbacks are simple free functions — no captures needed. Eliminates the `std::function` heap allocation and its `<functional>` header dependency from `ota.h`.

### 4. Thread-safe logging

- **Ring tail**: `compare_exchange_weak` loop for atomic slot reservation (MPSC pattern)
- **Format buffers**: stack-local in `logf()` and `formatLogMessage()`
- **Timestamp**: `localtime_r()` with stack-local `struct tm`
- **Stats counters**: `std::atomic<size_t>` with relaxed ordering
- Consumer side (`flushRingBuffer`) remains single-consumer (only called from `update()` on the main loop)

### 5. Heap-aware WiFi shedding

When `ESP.getFreeHeap() < 30 KB`, `writeToOutputs()` skips WiFi writes (Serial still works). This is a soft shed — the telnet connection stays open and resumes when heap recovers. No active `client_.stop()` to avoid the "Connection reset by peer" problem.

### 6. Telnet heartbeat

`Logger::update()` sends `# heartbeat\r\n` every 30 seconds if no log data has been written. Keeps the TCP connection alive through idle periods.

### 7. Remove availableForWrite guard

`WiFiClient::write()` handles buffering internally on ESP32. The `availableForWrite()` check was actively harmful (always returned 0). Removed entirely.

## Consequences

### Positive

- Free heap during normal operation: ~95 KB (up from ~42 KB before crash)
- WiFi telnet shows the same logs as USB serial
- Telnet stays connected indefinitely (heartbeat + no aggressive disconnect)
- `/api/parameters?filter=all` returns 19 KB JSON reliably, even with telnet connected
- OTA uploads complete successfully with telnet monitor open
- No `abort()` / OOM crashes under concurrent UI + telnet load

### Negative

- Ring buffer reduced to 16 entries — under extreme log burst (>16 messages between main loop iterations), messages are dropped. Acceptable: the `messagesDropped` counter tracks this.
- `WiFiClient::write()` can theoretically block up to TCP ACK timeout under WiFi congestion. Mitigated by `setNoDelay(true)` and batched `flush()` after ring drain.
- 30 KB heap threshold is hardcoded. Could be made configurable if machines with heavy BLE/MQTT usage need a different value.

### Lessons learned

- **Always check base class defaults** — `Print::availableForWrite()` returning 0 silently broke WiFi logging with no compiler warning.
- **Calculate static RAM budgets** — on ESP32, a 37 KB ring buffer is 12% of total RAM. Size buffers for the actual throughput, not theoretical maximums.
- **Serialize directly to the response** — `AsyncJsonResponse` exists for exactly this purpose. Never build a `String` intermediate for large JSON.
- **Test with all outputs active** — USB-only testing missed the WiFi logging failure entirely. The integration test checklist (`docs/integration-tests.md`) now requires telnet + API concurrency testing.
