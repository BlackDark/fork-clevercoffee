# ADR 0004: Rust migration platform selection

## Status

Accepted (2026-09-28)

**Related:** [inventory.md](../rust-migration/inventory.md) · [compatibility-matrix.md](../rust-migration/compatibility-matrix.md) · [architecture.md](../rust-migration/architecture.md) · [task-list.md](../rust-migration/task-list.md)

## Context

The firmware is to be rewritten in Rust with behaviour unchanged. The target
hardware is fixed and was identified from the attached device rather than assumed:
**ESP32 revision v3.0, Xtensa LX6, dual core, 4 MB flash, no PSRAM**, on an
`az-delivery-devkit-v4` board. "Old ESP32 v4" in the project's language refers to
the board revision, not the chip.

Xtensa is the constraint that frames everything. Unlike the RISC-V ESP32 parts,
this chip has no upstream stable Rust support; it requires the Espressif toolchain
fork installed via `espup`, and `-Zbuild-std` for any std target. That is a
permanent condition of the project, not a transitional one.

Two viable Rust platforms exist for this chip, and both were researched against
the same capability list:

- **A — ESP-IDF / std**: `esp-idf-sys` 0.38.1 + `esp-idf-hal` 0.47.0 +
  `esp-idf-svc` 0.53.0, target `xtensa-esp32-espidf`.
- **B — bare metal**: `esp-hal` 1.2.2 + `esp-rtos` 0.4.0 + `esp-radio` (0.18.0
  stable / 1.0.0-beta.1) + embassy, target `xtensa-esp32-none-elf`.

The existing C++ build sets the constraint that dominated the analysis: it consumes
**90.4 % of the 1.625 MB app partition** (1,539,657 of 1,703,936 bytes), leaving
160 KiB of headroom. Binary size was therefore treated as the gating risk, because
Rust `std` plus `build-std` is exactly the kind of addition that could eat that
margin — and no published figure existed for a Wi-Fi + HTTP `esp-idf-svc`
application on the original ESP32.

The product also has two properties that turned out to be decisive, both of which
come from the inventory rather than from the platforms:

1. Configuration for every deployed machine lives in the **ESP-IDF NVS partition**,
   namespace `config`, under keys derived as `"p" + FNV-1a(dotted path)` in hex,
   with no schema version and no migration path. New firmware can only read it by
   reproducing that derivation *and* the exact Arduino `Preferences` type encoding.
2. The web UI is a **gzip-only React bundle served from a 640 KB LittleFS
   partition**. It is not compiled into the firmware and cannot be: 640 KB does not
   fit in a 1.625 MB app partition alongside the application.

## Decision

**Adopt platform A: ESP-IDF / std, via `esp-idf-hal` and `esp-idf-svc`, on target
`xtensa-esp32-espidf`, pinned to ESP-IDF v5.3.6.**

The selection is **not conditional**. The one gap that would have made it
conditional — binary size — was closed by measurement before this record was
written.

### SPIKE-1: the size question, closed

A spike was built on this host that links every subsystem the firmware needs and
reaches all of them from `main`: Wi-Fi STA + AP, `EspHttpServer` with a JSON
handler, a gzip-precompressed static handler and a streaming OTA upload handler,
`EspMqttClient` with LWT and retain, `EspNvs` including `get_blob`, `EspOta`,
`EspPartition` raw read by label, `I2cDriver`, a GPTimer with a subscribed ISR
closure, `TWDTDriver`, `PinDriver`, `serde_json` and `EspLogger`. Release profile
`opt-level = "z"`, fat LTO, one codegen unit, `panic = "abort"`, stripped, with a
size-tuned `sdkconfig.defaults` including `CONFIG_ESP32_REV_MIN_3=y`.

| Build | App image | % of 1.625 MiB app partition | Headroom |
|---|---|---|---|
| **Rust + ESP-IDF (this spike)** | **997,648 B (974.3 KiB)** | **58.5 %** | **689.7 KiB** |
| C++ baseline (`pio run -e esp32_usb`) | 1,539,657 B (1503.6 KiB) | 90.4 % | 160.4 KiB |

Static DRAM was 34,092 B for the spike against 75,240 B reported for the C++ build.

The Rust image is roughly **532 KiB smaller** than the current C++ one in the same
partition, because it drops the Arduino core, ESPAsyncWebServer/AsyncTCP, U8g2,
WiFiManager, PubSubClient, ArduinoJson and NimBLE in exchange for ESP-IDF
components that are already in the image. Full write-up, caveats included, in
`research/spike-size/RESULT.md`.

The spike was **never flashed** and holds no credentials. It contains no display
rendering, no font data, no PID, no config registry and no state machine; the
690 KiB of headroom is what absorbs those.

### Why A over B

1. **Config continuity is provable on A and speculative on B.** Arduino
   `Preferences` has no encoding of its own — it is a direct `nvs_set_*`
   passthrough — and `esp_idf_svc::nvs::EspNvs` covers every type it uses, so the
   existing `config` namespace maps 1:1. On B, reading that partition depends
   entirely on `esp-nvs` 0.5.0: one maintainer, 22 stars, 4,616 downloads, created
   2025-11-19, README still pinning an older `esp-storage`, and a licence
   discrepancy between crates.io and its repo. There is no Espressif-official
   no_std NVS reader. If it fails, deployed machines lose their settings.
2. **The web UI has no path on B at all.** `littlefs2` binds the C littlefs, but no
   glue to `esp-storage` exists and no instance was found of anyone mounting an
   ESP-IDF littlefs partition from no_std Rust. `picoserve` serves compile-time
   `&'static [u8]`, not a filesystem. On A, `esp-idf-svc` ships LittleFS, SPIFFS and
   FATFS, and `EspPartition` gives raw access by label. This is the closest thing to
   a disqualifying gap found in the whole analysis.
3. **Wi-Fi on A is the stack the device is already proven on.** The ESP-IDF
   Wi-Fi/lwIP/mbedTLS/httpd paths are identical to today's. On B, `esp-radio` is
   pre-1.0 with open issues that are hostile to an appliance: no WPA3 station on
   ESP32 ([#1600](https://github.com/esp-rs/esp-hal/issues/1600)), a reconnect that
   **stays broken after an authentication failure**
   ([#5889](https://github.com/esp-rs/esp-hal/issues/5889)), a driver buffer
   overflow ([#5309](https://github.com/esp-rs/esp-hal/issues/5309)), and
   coexistence that only began working on this chip in mid-2025. A coffee machine
   that quietly stops rejoining the network is the worst failure mode for support.
4. **RAM, measured, favours A.** B's own official Wi-Fi examples — measured during
   research, because no figures are published — allocate essentially all 320 KiB of
   DRAM before any application code, with stack collapsing from 119.6 KiB to
   54.4 KiB once BLE coexistence is enabled. The A spike used 34 KiB of static
   DRAM. RAM, not flash, is B's binding constraint, and the display, PID, MQTT and
   web buffers all still have to fit.
5. **The architecture ports structurally on A.** The existing firmware is a single
   loop plus one ISR, with threads available for anything that must not block. std
   + threads + `log` maps onto that directly; B would mean an async-first rewrite
   on top of an already large behavioural port, compounding two risks at once.
6. **Everything needed already has a wrapper on A.** Wi-Fi STA/AP/APSTA, HTTP with
   streaming bodies and arbitrary response headers, MQTT with LWT and retain, OTA
   with slot management and rollback, NVS, LittleFS, raw partitions by label,
   `esp_timer`, the task watchdog, and mDNS — which the current firmware lacks and
   gets for free. Nothing has to be built from scratch at the platform layer.

### What we are accepting by choosing A

Recorded plainly, because these are real and B is better at each of them:

1. **Espressif does not fund the `esp-idf-*` crates.** All three READMEs say so and
   redirect to `esp-hal`, and the official esp-rs book has been rewritten with no
   std chapters at all. The funded direction of the ecosystem is B. Bus factor on A
   is approximately one maintainer.
2. **No hardware-in-the-loop testing anywhere in the A stack.** Green CI means "it
   linked". There has been a breaking change in a patch release (0.42.5) to fix
   discovered UB in `subscribe` callbacks.
3. **`esp-idf-hal`'s I²C wraps a driver Espressif has declared End-of-Life in
   IDF v6.0 and scheduled for removal in v7.0.** The hal migrated ADC, timer, I²S,
   PCNT, RMT and MCPWM to the new APIs and skipped I²C. Consequence: pin ESP-IDF to
   ≤ 6.x, and treat writing an `i2c_master` wrapper as a known future cost. Not a
   present blocker.
4. **No MCPWM wrapper.** Only reachable through raw `esp-idf-sys`. The firmware does
   not use MCPWM today, and the heater PWM design deliberately avoids needing it.
5. **Permanent toolchain friction**: a nightly fork, `-Zbuild-std` forever, ESP-IDF
   compiled from source, no `menuconfig` from Cargo, and `cargo check` impossible on
   the host for anything touching the HAL. Mitigated by the crate split in
   [architecture.md](../rust-migration/architecture.md), which keeps all control
   logic in host-checkable crates.
6. **An ESP-IDF major-version jump**, from 4.4 under Arduino core 2.0.17 to 5.3.6.

### Revisit conditions

This decision should be reopened if any of these becomes true:

- `esp-nvs` (or an Espressif-official equivalent) reaches production maturity **and**
  a no_std path to mount the existing LittleFS partition appears. Those are the two
  gaps that decided against B.
- `esp-radio` reaches 1.0 with [#5889](https://github.com/esp-rs/esp-hal/issues/5889)
  and [#1600](https://github.com/esp-rs/esp-hal/issues/1600) closed.
- The `esp-idf-*` crates go unmaintained for two or more release cycles.
- The product moves to an ESP32-S3 or -C6, which would remove the Xtensa nightly
  constraint and change B's risk profile substantially.

Because the two platforms are isolated behind the HAL-trait crate described in
[architecture.md](../rust-migration/architecture.md), switching later means writing
a second adapter crate, not rewriting the firmware. That is the main reason the
seam exists.

## Consequences

### Immediate

- Toolchain comes from **rustup + espup**, pinned in repo config.
  **mise must not manage `rust`** — its `cargo` shim shadows rustup's, silently
  ignoring `rust-toolchain.toml` and failing with `can't find crate for core`. This
  was hit and diagnosed during SPIKE-1.
- ESP-IDF is pinned to **v5.3.6** via `ESP_IDF_VERSION`.
- The C++ firmware stays in the tree and buildable for the whole migration. It is
  the parity oracle: every behavioural claim is checked against it, not against the
  documentation.
- The partition table is unchanged, **including the `spiffs` label**, so the
  existing filesystem-OTA path and the deployed web UI keep working.
- NVS keys keep the `"p" + FNV-1a(dotted path)` derivation. Floats and doubles must
  be read with `get_blob` + `from_le_bytes`, **not** `get_u32`/`get_u64`.

### Deliberate behaviour changes

Each must be declared in its commit message and in the task list, never smuggled in:

- **Heater interlocks move into the PWM driver.** Today the ISR consults only
  `pidOutput`, so every interlock works by forcing that to zero — and
  `computePID()` runs before `updatePIDState()` zeroes it, leaving a window in
  which the heater can be driven from a live duty cycle during an emergency. The
  Rust design makes the interlock structural. See
  [architecture.md](../rust-migration/architecture.md).
- **The PID derivative uses a float `dt`** instead of the integer
  `SampleTime / 1000`, which is numerically identical at the shipped 1000 ms window
  and removes a division-by-zero for any smaller window.
- **The ABP2 temperature transfer function is corrected** to the datasheet's
  `counts * 200 / 16777215 - 50`. The current `* 270 / … - 40` is wrong; it only
  feeds a TRACE log line today.
- **`emergencyStopTemp` and `emergencyStopHysteresis` are registered** in the
  config registry. They are currently declared but absent from it, so they never
  persist, never export and are skipped by factory reset — on a safety parameter.
- **The TSIC conversion keeps the vendored library's formula**
  (`((raw * 250) >> 8 - 499) / 10.0`), not the datasheet's, to preserve existing
  users' calibration offsets. This is a decision to keep a deviation, recorded so it
  is not "corrected" later by accident.

### Known-open

Everything still unverified is listed in
[compatibility-matrix.md §5](../rust-migration/compatibility-matrix.md#5-everything-still-unverified)
and carried as named spikes in [task-list.md](../rust-migration/task-list.md).
The most important: **nothing has been flashed yet**, so every runtime claim in
this record is compile- and link-level only.

### Licence

The chosen crates are MIT OR Apache-2.0 throughout. The port also **removes the
LGPL-3.0 `ESPAsyncWebServer`/`AsyncTCP` dependency** from a statically linked
firmware image, which resolves a pre-existing licensing wrinkle. Note that
`u8g2-fonts` ships font data under U8g2's own licence (crates.io reports
`license = non-standard`); that is an addition to review before release.
