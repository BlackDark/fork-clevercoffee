---
title: "Subset #645: boot crash triage (resetReason, crashInfo) + chunked coredump download"
upstream_pr: 645
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/645
category: take
advise: include
stake: "Field panics are invisible without a serial cable; retained boot crash strings plus an authenticated chunked dump download close that gap without MQTT metric spam."
effort: M
risk: medium
priority: 6
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

# 0645 — diagnostics / coredump (subset)

## Verdict

**Include a subset. Skip the ESPHome-style MQTT telemetry dump.**

Take exactly three field-debug surfaces that survive a reboot:

1. **Retained MQTT `resetReason`** — why this boot happened (`power-on`, `panic or unhandled exception`, `task watchdog`, …).
2. **Retained MQTT `crashInfo`** — last stored core-dump summary (`task=… cause=… pc=… vaddr=…` or `none`). Pair with `resetReason`: dump is last panic, not necessarily last boot.
3. **Authenticated HTTP `GET /download/coredump`** — stream the raw flash partition in chunks for offline `esp-coredump`. Never load the dump into RAM.

Do **not** take: `freeHeap`, `maxAllocHeap`, `rssi`, `maxLoopTime` MQTT sensors, `state_class: measurement` on existing sensors, standby-path log-line extras, or `embeddedWebserver.h`.

## Upstream

PR is **open** (`irrwisch1`, +245/−8). Motivation: installed machines have no serial console; telnet has no boot backlog; a panic’s backtrace dies with the UART.

What it actually ships:

| Piece | Where | Keep? |
|---|---|---|
| `resetReason` text, retained, once per MQTT connect | `mqtt.h` discovery + `bootResetReason` in `main.cpp` | **yes** |
| `crashInfo` from `esp_core_dump_get_summary`, retained | same | **yes** |
| `GET /download/coredump` chunked from coredump partition, `authenticate()` | `embeddedWebserver.h` | **yes, ported** |
| Boot log `Reset reason:` / `Core dump found:` | `setup()` | **yes** (USB/telnet if attached) |
| MQTT `freeHeap`, `maxAllocHeap`, `rssi`, `maxLoopTime` (read-and-reset) | periodic sensors | **no** |
| HA `state_class` on temperature/weight/pressure/etc. | discovery | **no** |
| Empty-`device_class` omitted (HA rejects `""`) | `GenerateSensorDevice` | **yes, surgical** — required for the two text sensors |
| Standby log lines on power switch/button | `powerHandler.h` | **no** — fork already logs power on/off + “Entering standby mode” |
| Loop-peak tracker in `loop()` | `main.cpp` | **no** — fork `LoopManager` already tracks `maxLoopTime_` behind a debug flag |

Coredump path: `esp_core_dump_image_get` → `esp_partition_read` in `beginChunkedResponse`. Dump is **not** erased after read. Gated by `CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH`. Offline: `esp-coredump info_corefile --core dump.bin --core-format raw firmware.elf`.

## Current fork

| Need | State |
|---|---|
| Coredump partition | **Present.** `partitions_4M.csv`: `coredump, data, coredump, 0x3F0000, 0x10000` (64 KiB). Do not resize. |
| `CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH` | **Already on** in Arduino-ESP32 sdkconfig (`framework-arduinoespressif32/.../sdkconfig.h`). Confirm at compile with `#if`; do not add a custom `sdkconfig.defaults` unless a build proves it is off. |
| HTTP download | **Missing.** No `/download/coredump`. Routes live in `src/network/WebServerManager.cpp` (not `embeddedWebserver.h`). |
| Auth | Global `AsyncAuthenticationMiddleware` **only** when `system.auth.enabled` is true (**default false**). Factory username/password are `admin`/`admin` (`AUTH_USERNAME` / `AUTH_PASSWORD`). Config download is **not** per-route authenticated. "Auth always" on the dump route with those defaults means anyone on the LAN can download a RAM image. |
| MQTT sensors | `MQTTManager::registerSensor(topic, std::function<double()>)` — **numeric only**. Cannot carry `resetReason`/`crashInfo`. `publish(reading, payload, retain)` is private and used from `sendHASSIODiscoveryMsg()`. HA discovery every 5 min (`HASSIO_DISCOVERY_INTERVAL_MS`). |
| Empty `device_class` | `generateSensorDevice` **always writes** `device_class`, including `""`. HA already rejects that for existing empty-class sensors (`shotsSinceBackflush`, `backflushReminderDue`). Skip empty string when adding the two text sensors. |
| `freeHeap` | **Already exists.** `memoryUtils.h` logs free heap + largest block; `/api/nvs-debug` returns `metadata.free_heap` and `min_free_heap`; Logger sheds WiFi under heap pressure (ADR-0002). Do not MQTT it. `/api/status` has no heap field. `Config::stateFreeHeap` is commented out. |
| `maxLoopTime` | **Already exists internally.** `LoopManager::maxLoopTime_` when `performanceMonitoringEnabled_`. Different feature. Do not MQTT. |
| Boot reset / crash capture | **Missing.** No `esp_reset_reason()`, no `esp_core_dump_*`. |

ADR-0002: never serialize large payloads to `String` then `request->send()`. Use chunked / `AsyncJsonResponse`. Coredump **must** be `beginChunkedResponse` reading flash into the provided buffer only.

Native tests: `test_build_src=false`, source `.cpp` included directly. `esp_core_dump.h` in a header pulled by a stub **will** break `native_test`. IDF headers only in ESP32 `.cpp` files. `test/esp_system.h` is heap stubs only — **no** `esp_reset_reason()`. Do not include `BootDiagnostics.h` from `MQTTManager.h` / `WebServerManager.h` (native tests include those headers via stubs).

## Need it?

Yes, the subset. A machine that panics in the field currently leaves: a reboot, maybe `software restart` vs watchdog indistinguishable from the UI, and no dump. Telnet starts too late to see the boot line.

No, the rest. Heap is already in logs and `/api/nvs-debug`. RSSI is not a crash tool. `maxLoopTime` is a live stall metric, not a post-reboot artifact. HA long-term statistics on brew temperature are unrelated to this PR’s stated goal.

## Plan

### 0) Preconditions (no code)

- Confirm Arduino sdkconfig still has `CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH` and `CONFIG_ESP_COREDUMP_DATA_FORMAT_ELF` for this platform package.
- Confirm `partitions_4M.csv` coredump row unchanged.
- **Done when:** both are true; if flash coredump is off, stop and add an explicit board sdkconfig — do not ship a 501-only endpoint.

### 1) Boot capture (resetReason + crashInfo)

Add a tiny ESP32-only module, e.g. `include/clevercoffee/diagnostics/BootDiagnostics.h` + `src/diagnostics/BootDiagnostics.cpp`.

- Header: `capture()`, `resetReasonString()`, `crashInfoString()` (`"none"` if no dump). **No** `#include <esp_core_dump.h>` / `esp_partition.h` in the header. Native-safe `resetReasonToString(int)` (or equivalent) lives here so tests need no IDF types.
- `.cpp`: `esp_reset_reason()` + `resetReasonToString` (upstream switch table). Under `#if CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH && CONFIG_ESP_COREDUMP_DATA_FORMAT_ELF`: `esp_core_dump_image_check()`, then `esp_core_dump_get_summary` into a **short-lived** allocation (`std::unique_ptr` + `malloc`/`free`, or stack if the struct fits). Format into a **fixed** `char[176]` (MQTT payload budget). **Do not erase** the dump. **Do not** keep the summary allocated.
- Call `capture()` once from `SystemInitializer::initializeLogger()` immediately after `Logger::init` / `Logger::begin`, then `LOGF` the two lines.
- **Done when:** USB boot log shows `Reset reason: …` on every boot; after a panic, also `Core dump found: task=…`; clean boot with no dump logs crashInfo as none; native tests still compile because the header has no IDF includes.

### 2) MQTT: retained strings once per connect

`registerSensor` stays numeric. Do not add these to `mqttSensors_`.

- On **successful** `mqttClient_.connect(...)` in `MQTTManager::checkConnection()` (same function is first connect and reconnect), call the **private** `publish("resetReason", …, true)` / `publish("crashInfo", …, true)`. Do not promote `publish` to public. Include `BootDiagnostics.h` from `MQTTManager.cpp` only. Independent of `mqttHassioEnabled`.
- If HA discovery is enabled, also `generateSensorDevice("resetReason", …)` / `crashInfo` with **empty unit and empty device_class**.
- In `generateSensorDevice`: omit `device_class` when the string is empty (same guard as `generateBinarySensorDevice`). Do **not** add `state_class` to existing sensors in this change.
- Re-publish on each connect is enough; the 5-minute discovery timer may re-send identical retained payloads — acceptable, do not add a third publisher.
- **Done when:** a fresh `mosquitto_sub` after connect receives both retained messages without waiting for the next discovery interval; HA does not drop the entities for empty `device_class`.

### 3) HTTP `GET /download/coredump` — chunked + auth

Implement in `WebServerManager.cpp` only. Port the upstream handler, do not copy `embeddedWebserver.h`.

- Route: `GET /download/coredump` (keep upstream path so `esp-coredump` docs/scripts match).
- **Auth:** require `system.auth.enabled`. If false → **403**, do not stream (global middleware is off; dump must not be open on the LAN). If true: attach `AsyncAuthenticationMiddleware` to **this handler only** (ESPAsyncWebServer 3.12 `handler.addMiddleware`; do not enable global middleware for this). Empty username/password → **403**. Missing/wrong auth → **401**. Factory creds are `admin`/`admin` — enabling auth without changing the password still exposes dumps on the LAN; say so in the integration-test note / help text. Coredumps contain RAM (WiFi PSK, MQTT password).
- `#if CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH`: `esp_core_dump_image_get`; 404 `"No core dump stored"`; missing partition → 500; else `beginChunkedResponse("application/octet-stream", …)` reading `esp_partition_read(part, index, buffer, chunk)` for `min(maxLen, remaining)`. `Content-Disposition: attachment; filename="coredump.bin"`. Return 0 from the callback on read error or `index >= dumpSize`.
- `#else`: 501 `"Core dump to flash is not enabled in this build"`.
- **Never** `String`, `std::vector`, or `malloc(dumpSize)`. Chunk buffer is the one AsyncWebServer provides.
- **Done when:** `system.auth.enabled=false` → 403; enabled + unauthenticated curl → 401; empty partition → 404; a real dump downloads with heap drop ≪ dump size (telnet connected); `esp-coredump info_corefile` accepts the file.

### 4) Optional extras (only if the three above are done and still tiny)

- Add `resetReason` / `crashInfo` **strings** (not the dump) to `/api/status` via `AsyncJsonResponse` (that endpoint currently `serializeJson` → `String` — do not enlarge that pattern; if touched, switch it to `AsyncJsonResponse`). Skip if it pulls status into this PR’s auth/heap debate.
- Do **not** sneak in `maxLoopTime` MQTT, `freeHeap` MQTT, or `state_class`.

### 5) Integration-test note

Add a subsection to `docs/integration-tests.md` (new numbered section, not buried in smoke):

- `curl -I` / `GET /download/coredump` with `system.auth.enabled=false` → **403**, no body dump.
- Same with auth enabled, no credentials → **401**.
- Auth enabled, `-u admin:admin` (or configured creds) and no dump → 404, body `No core dump stored`. Document that factory `admin`/`admin` is well-known; change before leaving a machine on a shared LAN.
- With a dump (after a deliberate panic or a fixture): 200, `Content-Type: application/octet-stream`, file size matches `esp_core_dump_image_get`; device stays up; `/api/health` 200; free heap in `/api/nvs-debug` does not fall by ~dump size.
- Repeat the download with telnet (`nc <host> 23`) connected — ADR-0002 concurrency.
- `mosquitto_sub -t '<prefix>/<hostname>/resetReason' -C 1` and `…/crashInfo` receive retained values after MQTT connect (and after subscriber restart).
- Do not erase the dump as part of the test; a second download must still 200.

**Done when:** the checklist items exist and match the implemented status codes.

## Do not copy

- `src/embeddedWebserver.h` — fork route owner is `WebServerManager`.
- Whole-dump `String` / RAM buffer / `request->send(buf, len)` for the partition.
- `registerSensor` for text values, or encoding crashInfo as a double.
- MQTT `freeHeap`, `maxAllocHeap`, `rssi`, `maxLoopTime`.
- `state_class: measurement` on existing HA sensors.
- Power-switch standby log-line patch.
- Erase-after-download / erase-after-MQTT.
- Custom partition-table growth.
- New UI download button (optional later; dumps are secrets).
- Unauthenticated dump (`system.auth.enabled` false, or “auth always” with factory `admin`/`admin`).
- IDF headers (`esp_core_dump.h`, `esp_partition.h`) in `.h` files native tests include.

## Tests / verification

Firmware (after implementation, not this plan):

1. `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`
2. `~/.platformio/penv/bin/pio run -e esp32_usb -s`
3. `~/.platformio/penv/bin/pio test -e native_test` — must stay green; no `esp_core_dump.h` / `esp_partition.h` in headers native tests include.

Native (small, no hardware):

- `resetReasonToString` mapping for known reset-reason integers (pure helper; do not require `esp_reset_reason_t` from the native stub).
- Crash-info getter returns `"none"` when capture was not run / buffer empty.
- Do not try to unit-test `esp_partition_read`.
- Do not include `BootDiagnostics.h` from headers native tests already pull (`MQTTManager.h`, `WebServerManager.h`).

On device:

- Power-on boot: MQTT `resetReason=power-on`, `crashInfo=none` (or stale last panic — document that pairing).
- `ESP.restart()` / power-switch long-press reboot: `software restart`.
- After a panic: `crashInfo` non-`none`; download parses with `esp-coredump` against `.pio/build/esp32_usb/firmware.elf`.

## Risks

| Risk | Mitigation |
|---|---|
| **Auth default-off + factory `admin`/`admin`** | Require `system.auth.enabled`; 403 if off or creds empty. Per-handler middleware when on. Call out well-known defaults. |
| **Dump is a RAM image** — WiFi/MQTT secrets | No unauthenticated path; do not log the binary; no UI auto-fetch. |
| **64 KiB partition vs RAM** — allocating the dump OOMs (ADR-0002) | Chunked flash read only; verify heap during download with telnet up. |
| **AsyncTCP task** doing `esp_partition_read` | Keep chunks small (server `maxLen`); no extra alloc in the callback; abort callback with 0 on read fail. |
| **Stale crashInfo** after a clean reboot | Document: pair with `resetReason`; do not auto-erase (losing the only copy after OTA is worse). |
| **Flash wear** | HTTP is read-only. Panic handler writes once per crash. Do not add erase/rewrite on download. |
| **Native-test include leak** | IDF headers only in `BootDiagnostics.cpp` and `WebServerManager.cpp`. Do not include `BootDiagnostics.h` from `MQTTManager.h` / `WebServerManager.h`. |
| **HA rejection of `device_class: ""`** | Omit empty class; do not “fix” by inventing a fake class. |
| **501 in production** if sdkconfig ever disables flash coredump | Compile-time `#if` + boot log whether capture ran; fail the precondition rather than ship a dead route. |
