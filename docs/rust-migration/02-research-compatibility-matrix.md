# Rust Options — Research and Compatibility Matrix

**Research date:** 2026-09-28
**Researcher:** agent session, primary sources only (docs.rs, crates.io, `esp-rs` GitHub,
`docs.espressif.com/projects/rust/book`, vendor datasheets/app notes).
**Baseline:** [01 — Feature inventory](./01-feature-inventory.md)

Every version below was read from a primary source on the research date. Where a claim
could not be confirmed from a primary source it is marked **UNVERIFIED** and has a
corresponding proof-of-concept task in [06 — Task list](./06-migration-task-list.md).

Confidence key:

| Mark | Meaning |
| --- | --- |
| **VERIFIED** | Read directly in primary documentation or crate source on 2026-09-28 |
| **PLAUSIBLE** | Strongly implied by documentation, but not exercised end-to-end |
| **UNVERIFIED** | Not confirmed; a spike task exists |
| **ABSENT** | Confirmed not to exist |

> **Do not upgrade a VERIFIED verdict to "supported" without a phase gate.** A crate
> mentioning "ESP32" is not evidence that a specific chip works, and a crate existing is
> not evidence that a specific feature works on it.

---

## 1. Approach candidates

| ID | Approach | Status |
| --- | --- | --- |
| **A** | `esp-idf-svc` 0.53.0 — ESP-IDF bindings, `std` available, FreeRTOS underneath | **Selected** |
| **B** | `esp-hal` 1.2.0 + `esp-rtos` 0.4.0 + `esp-radio` + embassy — bare metal `no_std` | Strongest alternative, **disqualified for the full feature set** |
| **C** | Hybrid: `esp-hal` for peripherals + `esp-idf-svc` for connectivity | Rejected — two HALs, two allocators, two runtimes, no benefit |
| **D** | `arduino-hal` / C++ library reuse via FFI | Rejected — defeats the purpose, keeps the C++ toolchain |

**C** deserves a sentence of justification for the record: in theory, driving the bit-bang
sensors from `esp-hal` while using `esp-idf-svc` for Wi-Fi could reduce ISR-latency
concerns. In practice it means two peripheral abstractions over the same registers, two
allocation strategies, two task models, and a linker configuration with both ESP-IDF and
bare-metal startup paths. The measured benefit (a few microseconds on a 10 ms tick) does
not justify it. Rejected.

---

## 2. Version baseline

| Crate / tool | Version | Date checked | Source |
| --- | --- | --- | --- |
| `esp-idf-svc` | 0.53.0 | released 2026-09-15 (docs.rs) / 2026-09-25 (CHANGELOG) | [crates.io](https://crates.io/crates/esp-idf-svc) |
| `esp-idf-hal` | 0.47.0 | 2026-09-28 | `esp-idf/Cargo.toml` workspace |
| `esp-idf-sys` | 0.38.1 | 2026-09-28 | `esp-idf/Cargo.toml` workspace |
| ESP-IDF (default) | **v5.5.5** | 2026-09-28 | `esp-idf-sys/build/native/cargo_driver/config.rs` `DEFAULT_ESP_IDF_VERSION` |
| `esp-hal` | **1.2.2**, MSRV 1.95.0 | 2026-09-28 | `esp-hal/Cargo.toml` |
| `esp-rtos` | 0.4.0 | published 2026-08-26 | [crates.io](https://crates.io/crates/esp-rtos) |
| `espflash` / `cargo-espflash` | 4.6.0 | updated 2026-09-10 | [crates.io](https://crates.io/crates/espflash) |
| `ssd1306` | 0.10.0 | 2025-03-22 | [crates.io](https://crates.io/crates/ssd1306) |
| `embedded-graphics` | 0.8.x | 2026-09-28 | [crates.io](https://crates.io/crates/embedded-graphics) |
| `serde_json` | 1.0.151 | 2026-07-20 | [crates.io](https://crates.io/crates/serde_json) |
| `hx711` | 0.7.0 | 2025-11-11 | [crates.io](https://crates.io/crates/hx711) |
| `onecable` | 0.2.0 | 2025-03-07 | [crates.io](https://crates.io/crates/onecable) |
| `esp-wifi-provisioning` | 0.1.1 | 2026-04-09 | [crates.io](https://crates.io/crates/esp-wifi-provisioning) |
| `edge-executor` | 0.5.0 | 2026-08-20 | [crates.io](https://crates.io/crates/edge-executor) |

The `esp-idf-*` crates are a **community effort** (the upstream README says so explicitly:
they lag the latest ESP-IDF, have no HIL tests, and are thinly documented). `esp-hal` is
the Espressif-funded HAL. That trade-off is the crux of the decision.

`esp-idf-sys` 0.38 also tracks **ESP-IDF v6.0** compatibility as of 0.53.0 (v6.1 appears only under `[Unreleased]`).
Pin to v5.5.5 for the first migration; do not take a moving target.

---

## 3. Chip support matrix

Target chips: **ESP32** (in use today), **ESP32-S3**, **ESP32-C6** (requested future
variants). A cell marked verified for approach A means the target triple exists in the
documented `cargo espflash --target` table **and** the chip is not excluded by a `cfg` in
the relevant module.

| Chip | Rust target triple | Approach A (`esp-idf-svc`) | Approach B (`esp-hal`) | `esp-idf-svc` BLE | `esp-radio` |
| --- | --- | --- | --- | --- | --- |
| **ESP32** (Xtensa LX6) | `xtensa-esp32-espidf` | **VERIFIED** | **VERIFIED** | VERIFIED | WiFi+BLE+coex+ESP-NOW (VERIFIED) |
| ESP32-S2 (Xtensa LX7) | `xtensa-esp32s2-espidf` | VERIFIED | VERIFIED | **ABSENT** (cfg) | VERIFIED |
| **ESP32-S3** (Xtensa LX7) | `xtensa-esp32s3-espidf` | **VERIFIED** | **VERIFIED** | VERIFIED | WiFi+BLE+coex+ESP-NOW (VERIFIED) |
| ESP32-C2 | `riscv32imc-esp-espidf` | VERIFIED | VERIFIED | VERIFIED | — |
| ESP32-C3 | `riscv32imc-esp-espidf` | VERIFIED | VERIFIED | VERIFIED | — |
| **ESP32-C6** (RISC-V) | `riscv32imac-esp-espidf` | **VERIFIED** | **VERIFIED** | VERIFIED (fixed in 0.52.0, #556) | WiFi+BLE+coex+ESP-NOW+802.15.4 (VERIFIED) |
| ESP32-H2 | `riscv32imac-esp-espidf` | VERIFIED | VERIFIED | VERIFIED | — |
| ESP32-P4 | `riscv32imafc-esp-espidf` | VERIFIED | VERIFIED | **ABSENT** (cfg) | — |

Source: `cargo-espflash/README.md` MCU→target table and `esp-idf-svc/src/lib.rs` cfgs.

**The original ESP32 requires the esp-rs Rust fork.** `rustup target add
xtensa-esp32-espidf` does not exist on stable — the Rust on ESP Book states this outright:
*"ESP32, ESP32-S2 and ESP32-S3 are based on Xtensa architecture. If you're going to target
these chips you will need to use a fork of the Rust compiler for now."* The fork installs
as the rustup toolchain named **`esp`**, installed by `espup`. **RISC-V chips (C6, H2, C3)
use upstream `rustup` targets** and need no fork. This is a real operational asymmetry
between "port the existing board" and "move to a C6".

---

## 4. Feature coverage — the decisive table

Legend: ✅ VERIFIED available · ⚠️ available with a caveat · ❌ ABSENT · 🔬 spike required

| # | Feature needed | Approach A (`esp-idf-svc`) | Approach B (`esp-hal` + `esp-radio`) |
| --- | --- | --- | --- |
| F1 | PID control | ✅ port in-crate | ✅ port in-crate |
| F2 | State machine | ✅ port in-crate | ✅ port in-crate |
| F3 | Emergency stop | ✅ port in-crate | ✅ port in-crate |
| F4 | Relay/LED/switch facade | ✅ `hal::gpio` | ✅ `hal::gpio` |
| F5 | Heater PWM | ✅ `hal::ledc` **hardware PWM** (better), or `hal::timer` GPTimer | ✅ `hal::mcpwm` / `hal::ledc` (`unstable`) |
| F6 | Task watchdog | ✅ `hal::task::watchdog::{TWDTDriver, TWDTConfig, WatchdogSubscription}` | ❌ `esp_hal::init()` *disables* all watchdogs; no user-facing task WDT — **UNVERIFIED that one can be re-enabled** |
| F7 | Valve fail-safe | ✅ app code | ✅ app code |
| F8 | Water-tank interlock | ✅ app code | ✅ app code |
| F9 | TSIC-306 / ZACwire | ❌ ABSENT — hand-write | ❌ ABSENT — hand-write |
| F10 | DS18B20 1-Wire | ⚠️ `hal::onewire` exists (RMT/`espressif/onewire_bus`) but **no CRC checking and no command helpers** (both listed under a `todo:` item in the module doc-comment at `esp-idf-hal/src/onewire.rs:15`; there is no `todo!()` macro in the file) | 🔬 `onecable` 0.1.x, single author, unproven on hardware |
| F11 | Rate-of-change filter | ✅ app code | ✅ app code |
| F12 | ABP2 I2C pressure | ✅ `hal::i2c` | ✅ `hal::i2c` |
| F13 | HX711 ×2 | ❌ no driver in `esp-idf-hal` | ❌ no driver in `esp-hal` |
| F14 | Acaia BLE scale | ⚠️ `esp_idf_svc::ble` (NimBLE) exists, **new in 0.53.0** | ⚠️ `TrouBLE` via embassy |
| F15 | OLED SSD1306/SH1106 | ✅ `hal::i2c` + `ssd1306` 0.10.0 | ✅ same |
| F16 | 6 templates, 10 bitmap fonts | 🔬 port glyphs to `ImageRaw` | 🔬 same |
| F17 | Localization | ✅ app code | ✅ app code |
| F18/F19 | Switches / LEDs | ✅ `hal::gpio` | ✅ `hal::gpio` |
| F20 | Wi-Fi STA | ✅ `svc::wifi` | ✅ `esp-radio` |
| F21 | Captive portal | ⚠️ `esp-wifi-provisioning` 0.1 (pinned to `esp-idf-svc ^0.51`, needs a bump or vendoring) | ⚠️ `esp-wifi-caddy` 0.1.0 (43 downloads) / `provision32` 0.2 |
| F22 | MQTT v3.1.1 | ✅ `svc::mqtt::EspMqttClient` (MQTT5 added in 0.53.0) | 🔬 `mcutie` (embassy-net, HA-discovery aware) |
| F23 | REST + static SPA | ✅ `svc::http::server::EspHttpServer` | ❌ **no maintained bare-metal HTTP server found** |
| F24 | SSE `/events` | 🔬 **esp-idf chunked encoding may break SSE** (espressif/esp-idf#14121, IDFFGH-13182); the "preamble only, keep socket open" API was requested and never landed | ⚠️ `embassy-http` has a real `Sse` type — but only reachable via approach B |
| F25 | React SPA | ✅ embed with `include_bytes!` | ⚠️ same, needs a server |
| F26 | OTA | ✅ `svc::ota` | ❌ ABSENT |
| F27 | NVS config | ✅ `svc::nvs::EspDefaultNvsPartition` | ❌ ABSENT (no NVS) |
| F28 | LittleFS `/config.json` | ✅ `svc::fs::littlefs` — requires the `joltwallet/littlefs` managed component | ❌ ABSENT (no filesystem in `esp-hal`) |
| F29 | Telnet log server | ✅ `svc::io` + lwIP socket, or a small task | ⚠️ needs a TCP stack |
| F30 | Retry / circuit breaker | ✅ app code (or `resilience` crate) | ✅ app code |
| F31 | Standby | ✅ app code | ✅ app code |
| F32 | Backflush / maintenance | ✅ app code | ✅ app code |
| F33 | Sensor coordinator | ✅ app code | ✅ app code |

### The disqualifying gaps for Approach B

F23, F26, F27, and F28 are **absent from the bare-metal stack**: no HTTP server, no OTA, no
NVS, no filesystem. F29 needs a TCP/IP stack that was not verified to exist for
`esp-radio` (**UNVERIFIED**). Building all four from scratch — a TCP/IP stack, LittleFS,
an OTA bootloader handshake, and an HTTP server with TLS-free static file serving — is a
multi-month project that dwarfs the coffee-machine firmware itself. `esp-radio` is
VERIFIED to provide the **radio**; it is not verified to provide a **TCP/IP stack**, and
no `embassy-net` glue for it was confirmed.

Approach A is therefore the only candidate that can deliver the full feature set.

---

## 5. Concurrency model comparison

### Approach A — `esp-idf-svc`

The runtime is **FreeRTOS** (already present in ESP-IDF). `std` is available, so
`std::thread`, `std::sync::Mutex`, and `std::sync::mpsc` all work. The crate also ships
`async` wrappers.

Verified details:

- Async wrappers are implemented natively on the esp-idf-svc types (since 0.48.0), using
  `hal::interrupt::asynch::HalIsrNotification` and `hal::task::asynch::Notification`, plus
  `esp_idf_hal::task::block_on`. There is no `asyncify` shim.
- The `embassy-time-driver` Cargo feature provides an `embassy_time_driver::Driver` backed
  by the **ESP-IDF Timer Service** (`esp_timer_get_time`, 1 µs tick).
- The `embassy-time-isr-queue` feature **no longer exists** — removed in 0.48.0. Any
  tutorial that tells you to enable it is wrong.
- `esp-idf-hal/critical-section` is implemented with a **FreeRTOS recursive mutex**, not
  `disable-all-interrupts`. Therefore `embassy-executor`, which synchronizes through the
  `critical-section` crate, is **not ISR-safe** on this platform.
- `esp-idf-hal/wake-from-isr` is documented as *"Only enable if you plan to use the
  `edge-executor` crate"*.
- `hal::task::queue::Queue<T>` (`send_back` / `recv_front`, `T: Copy`) and
  `hal::task::notification::{Notification, Notifier}` provide ISR-safe signalling.

**Which executor is officially recommended in 2026: UNVERIFIED.** No current esp-rs
document names one. The `edge-executor` ISR-safety guidance survives only inside the
0.43.0 CHANGELOG section. The maintainer states in
[issue #630](https://github.com/esp-rs/esp-idf-svc/issues/630) (2026-01-21) that he
personally uses `embassy-executor` by default, and that `async-executor` and
`edge-executor` ≤ 0.4.1 had priority-inversion/hang reports. `edge-executor` 0.5.0 was
published 2026-08-20 with a new pluggable-queue design; whether it fixes that is
**UNVERIFIED**.

→ **Spike R1-02 settles this.** The architecture in
[04 — Target architecture](./04-target-architecture.md) §3 is designed to work with
*either* executor so the choice is not load-bearing.

### Approach B — `esp-hal` + `esp-rtos`

- `esp-rtos` 0.4.0 is a **cooperative, priority-less, non-preemptive** scheduler. It
  provides exactly three executor shapes, all via `esp_rtos::embassy`:
  1. `Executor` — thread-mode, wraps `embassy-executor` 0.10. Requires `esp_rtos::start(...)`
     first.
  2. `InterruptExecutor<const SWI: u8>` — driven from `FROM_CPU_INTR0..3`; hard limit of
     **4** tasks; lower latency; can run on core 2 without the scheduler.
  3. Multicore — `start_second_core(...)` or `start_on_second_core_only(...)`.
- The last mode's restrictions are severe: no `esp-rtos` API from core 0, no automatic
  light sleep, and **no `esp-radio` WiFi/BLE drivers**.
- Automatic light sleep is cfg-gated; whether **Xtensa ESP32** is included is **UNVERIFIED**.
- `esp_hal::init()` **disables** SWD/RWDT/TIMG watchdogs on boot. There is no user-facing
  task watchdog, so safety path S7 has no direct equivalent — **UNVERIFIED** whether a
  replacement can be constructed.
- esp-hal's `dev` profile is documented as potentially *"one or more orders of magnitude
  slower than release"* with *"issues with timing-sensitive peripherals"*. Any bit-bang
  sensor must be built `--release`.

---

## 6. Peripheral-driver findings

### TSIC-306 / ZACwire — **ABSENT, and the single highest-risk item**

No Rust crate exists. The protocol is documented in the IST AG app note
`ATTSic_E2.3.0.pdf`:

- 1-wire, Manchester-like, **duty-cycle encoded**: start = 50 %, `1` = 75 %, `0` = 25 %.
- Nominal **8 kHz**, 125 µs bit window, `Tstrobe` = 62.5 µs.
- Two packets: the first carries the MSB 3 bits (TSIC-306 = 11-bit resolution), the second
  the LSB 8 bits. Each is start + 8 data + **even parity**, with one stop bit between.
- `T = (DS / 2047) · (Th − Tl) + Tl`, with `Th = +150`, `Tl = −50` for TSIC 30x, so
  `T = DS / 2047 · 200 − 50`.
- The recommended implementation is a **falling-edge ISR**: measure `Tstrobe` on the start
  bit, then wait `Tstrobe` after each of the next 9 falling edges and sample.
- Update rate 10 Hz. ISR servicing ~2.7 ms worst case.

**Action:** hand-write from the app note, cross-checked against the existing
`ZACwire` C library and the current `TempSensorTSIC.cpp` (which additionally implements an
adaptive max-change-rate of 200 °C/sample → 5 °C/sample, and rejects `temp <= 0 || >= 180`
to stop glitches from tripping emergency stop — **both behaviours are safety-relevant and
must be preserved**). Spike R1-03.

**STATUS 2026-09-28 (R3-07): implemented, and 🔴 unverified on hardware.**
`cc_domain::sensor::tsic306` implements the app note's §1.3 decoder — strobe measured
from the start bit, `pulse > strobe` per bit, even parity per packet, the stop-bit gap
checked as a two-window interval between falling edges — and is host-tested against a
waveform synthesised from the spec's own duty cycles.
`cc_hal_esp32::zacwire` is the device capture, and is **not brought up**: the pin it
would use is carrying 1-Wire traffic from the DS18B20 that is actually fitted.
**No TSIC-306 has ever been attached to this machine**, so a green test run is evidence
about the arithmetic and about nothing else. Two things in the above are now known to be
wrong or misleading and are called out in the code: `temp >= 180` **cannot fire** on a
TSIC-306 (its span ends at 150 °C), and the change-rate constants are used in *two
different units* in two adjacent C++ files (see `intentional-diffs.md` #7).
Also learned: `esp-idf-hal` 0.47 has no `AtomicU64` on this target, so the edge ring
packs the level and a 31-bit timestamp into one `AtomicU32`; and there is no
safe timestamped per-edge GPIO callback anywhere in this HAL, so the capture is a
≥128 kHz **poller** (which is what the app note asks for anyway) rather than the
falling-edge ISR it suggests.

### DS18B20 / 1-Wire — no mature Rust driver

| Crate | Version | Date | Verdict |
| --- | --- | --- | --- |
| `one-wire-bus` | 0.1.1 | 2020-01-18 | **Dead** — 6 years stale, `embedded-hal 0.2` |
| `ds18b20` | 0.1.1 | 2020 | **Dead** — same author, `embedded-hal 0.2.3` |
| `kellerkindt/onewire` | unversioned | ~2018 | **Abandoned** — needs stm32f1xx-hal patches |
| `bartweber/one-wire-hal` | — | 2024-04 | **Abandoned** — README says "work in progress and might not yet ready" |
| `onecable` | 0.2.0 | 2025-03-07 | **Only real candidate** — `embedded-hal 1.0`, `DelayNs`, open-drain pin, has a `DS18B20` submodule. Single author, low adoption |
| `esp_idf_hal::onewire` | in 0.47 | 2026-09-28 | RMT-backed, wraps `espressif/onewire_bus`. **No CRC checking** (`todo!()` in source), no DS18B20 layer |

**Action:** port the existing C. The protocol is ~200 lines. **Critical timing
constraint:** `esp_idf_hal::delay::Ets` rounds `delay_ns` **up to 1 µs**. That is fine for
1-Wire (3–65 µs slots) but **not** fine for HX711.

### HX711 — no driver in either HAL

`hx711` 0.7.0 (jonas-hagen, 2025-11-11) is `embedded-hal 1.0` + `nb` and non-blocking, but:

- It does **not** expose raw bit-level primitives; `retrieve()` returns an assembled `i32`.
- Its README documents the exact problem we will hit: `embedded-hal` has no sub-µs delay,
  so 1 µs granularity makes a 24-bit read ≥ 48 µs versus 5 µs ideal, and it warns
  *"beware of interrupts during readout"*.
- It is tested on STM32F103 only.

`loadcell` 0.3.0 targets `esp-hal ^0.23.1` — a different ecosystem from `esp-idf-hal`.

**Recommendation: drop scale support entirely.** It is dead code in the current firmware
(see [01 §3 F13/F14](./01-feature-inventory.md#3-feature--source--hardware-matrix)), the
the code that drives it is never constructed, and the HX711 needs sub-µs timing that the
selected HAL cannot express. If scale support is later required, a dedicated FreeRTOS task
pinned to an isolated core is the only defensible implementation.

### OLED

| Crate | Version | Date | HAL | SH1106 | Verdict |
| --- | --- | --- | --- | --- | --- |
| `ssd1306` | 0.10.0 | 2025-03-22 | `embedded-hal 1.0` (+ async) | ❌ | **Selected** — `DisplaySize128x64`, 1 KiB framebuffer |
| `sh1106` | 0.5.0 | 2023-08-30 | `embedded-hal **0.2.3**` | ✅ | Stale; needs a port or fork |
| `oled-i2c` | 0.3.0 | 2026-09-08 | `embedded-hal 1.0` | ✅ | 13 lifetime downloads, single contributor — unproven |
| `embedded-graphics` | 0.8.x | current | — | — | **Selected** as the draw target; text is `mono_font` only |

**Font replacement cost: HIGH.** U8g2's `profont` and `fub` bitmap fonts have no Rust
equivalent. `embedded-graphics` offers `FONT_6X10`-class monospace fonts plus
`ImageRaw`. Two options:

1. **Port the `profont`/`fub` glyph atlases to XBM/`ImageRaw`** and render via
   `embedded-graphics::image::Image`. One-time conversion per glyph; preserves metrics
   exactly. **Recommended.**
2. Accept `FONT_6X10` and re-derive every layout. This invalidates the AGENTS.md rules
   about `getStrWidth`-probed fixed-width fields, bbox-height row anchoring, and
   bar+label vertical midlines. High regression risk.

Also note `ssd1306` 0.10 keeps the 1024-byte framebuffer private, so the fixed-width and
bar+label layout helpers must be written against `DrawTarget` primitives rather than by
reading back pixels. This is a **PLAUSIBLE** design; verify in spike R1-04.

### MQTT

| Option | `no_std`? | Verdict |
| --- | --- | --- |
| `rumqttc` 0.25.1 | ❌ requires tokio, flume, futures-util | Rejected |
| `esp_idf_svc::mqtt` | std (IDF) | **Selected** — `EspMqttClient`, blocking + async, LWT, 1024-byte+ buffer, payload is `&[u8]` so `serde_json` output drops straight in |
| `mcutie` | ✅ embassy-net | Fallback only |
| `mqttrust` | ✅ | **Version skew** — crates.io latest 0.6.0 (2022-09-22) while the GitHub README documents a 1.0 embassy rewrite. Do not depend without verifying. |

Home Assistant discovery is just a retained JSON blob published to
`homeassistant/.../config`. MQTT v3.1.1 with `retain` is entirely sufficient.

### HTTP + SSE — the biggest web-tier unknown

`esp_idf_svc::http::server::EspHttpServer` covers REST and static files (embed the React
bundle with `include_bytes!` and serve pre-gzipped). `axum` 0.8.9 is **not viable** on
`no_std`: its docs state *"axum is designed to work with tokio and hyper. Runtime and
transport layer independence is not a goal"*, and the `tokio` feature that gates `SSE` is
**default-on**.

**SSE on ESP-IDF is a documented problem.** The C layer has `httpd_resp_send_chunk()`, but
the WHATWG SSE specification warns that chunked transfer-coding may break SSE
([espressif/esp-idf#14121](https://github.com/espressif/esp-idf/issues/14121),
IDFFGH-13182). The upstream fix — a "send preamble only, keep socket open" API — was
**requested and never landed**; the maintainer's own SSE example remains an *example* only
and was re-opened as *"Not resolved… Commit only adds an example of using SSE with
`Transfer-Encoding: chunked`"*. The community workaround is to grab the socket fd with
`httpd_req_to_sockfd(req)` and write raw frames with `httpd_socket_send()` from another
task.

**UNVERIFIED:** whether `esp_idf_svc::http::server` exposes any chunked/streaming primitive
or an equivalent of `req_to_sockfd`. Spike R1-05. The mitigation is a ~20-line
`esp-idf-sys` FFI shim, or switching the UI's live channel from SSE to WebSocket
(`ws_handler` is VERIFIED present in `esp-idf-svc`).

### JSON

`serde` with `default-features = false, features = ["derive", "alloc"]` and `serde_json`
1.0.151 with `default-features = false` + `alloc` both work. `postcard` 1.1.3 is the
binary alternative, useful for a serial link; not needed for the SPA or HA discovery.

### Wi-Fi provisioning

Contrary to the usual assumption, Rust captive-portal crates do exist:

| Crate | Version | Stack | Verdict |
| --- | --- | --- | --- |
| `esp-wifi-provisioning` | 0.1 | **esp-idf-svc 0.51** + esp-idf-hal 0.45 | **Closest match.** Captive softAP, NVS persistence, form UI, auto-reconnect, `clear_credentials()`. **Needs a version bump or vendoring** for 0.53 |
| `esp-wifi-caddy` | 0.1.0 | esp-hal 1.0 + esp-radio + embassy-net | Young (43 downloads) |
| `provision32` | 0.2 | esp-hal 1.1 + esp-radio | i18n, SSID scan, flash persistence |

Caveat: these crates bind AP port 80 themselves, so they cannot share it with the SPA
server. See [05 §5](./05-tooling-and-workflows.md).

### BLE scales

No Rust crate exists. `tatemazer/AcaiaArduinoBLE`, `baettigp/Acaia_Felicita_ArduinoBLE`,
`Zer0-bit/esp-arduino-ble-scales` are all Arduino C++; `pyacaia` / `acaia-lunar-ble` are
Python. Combined with the fact that scale support is dead code in this firmware and the
the original ESP32 *does* have a BR/EDR + BLE radio (01 §3) — so this is **doubly** moot,
not triply. **Drop.**

---

## 7. Toolchain findings

### 7.1 What must be on the host — Approach A

`esp-idf-sys` downloads and configures everything itself. From its documentation: *"Build
is `cargo` driven and automatically downloads & configures everything by default; no need
to download the ESP IDF SDK manually, or set up a C toolchain."* The `esp-idf-template`
README is blunter: *"do **NOT** clone, install and activate the ESP-IDF… as it is not
necessary at all (though supported, but not for a beginner setup)."*

**ESP-IDF itself is NOT a prerequisite.** Host packages that *are* still needed
(ESP-IDF's own OS dependency list): `git wget flex bison gperf python3 python3-pip
python3-venv cmake ninja ccache libffi-dev libssl-dev dfu-util libusb-1.0-0`; Linux also
needs `libudev-dev`.

**`rustup` is required**, and the Rust on ESP Book explicitly warns against a
system-package Rust (Homebrew/apt/dnf).

`rustup target add xtensa-esp32-espidf` **does not exist on stable** — the esp-rs fork is
mandatory for ESP32/S2/S3. It installs as the rustup toolchain named `esp`, via `espup`,
which also brings in the espressif LLVM fork and the espressif crosstool-NG GCC linker.
`espup install` writes `~/export-esp.sh` which must be sourced (`LIBCLANG_PATH`,
`CLANG_PATH`, `PATH`).

**`ldproxy`** (via `cargo:ldproxy` from crates.io — NOT the `esp-rs/embuild` GitHub release,
whose latest is v0.3.2 from 2022 and whose asset names are Rust triples, not `uname`) is mandatory — it is in every esp-rs CI job
and in the template prerequisites. Do not skip it.

**`-Zbuild-std=std,panic_abort`** is passed on every esp-idf build in esp-rs CI, so the
`esp` toolchain behaves like nightly for our purposes. `RUSTFLAGS="--cfg espidf_time64"`
is also set on every CI job.

| Env var | Purpose | Required |
| --- | --- | --- |
| `ESP_IDF_VERSION` | primary knob, e.g. `v5.5.5` | **yes** |
| `RUSTFLAGS` | `--cfg espidf_time64` | **yes** |
| `MCU` | target chip | **yes** |
| `IDF_PATH` | use a locally installed IDF instead | no |
| `IDF_TOOLS_PATH` | **explicitly ignored by `esp-idf-sys`** | no |
| `ESP_IDF_TOOLS_INSTALL_DIR` | where `esp-idf-sys` installs its tools | no |
| `ESP_IDF_SYS_EXTRA_COMPONENTS_FILE` | components moved out of the v6.0 tree | if using MQTT on IDF 6.0+ |
| `ESP_IDF_SDKCONFIG_DEFAULTS` | sdkconfig fragment path | no |

**Partition tables cannot be set via `sdkconfig.defaults`.** The `esp-idf-sys` README's
"Known limitations" says do **not** set `CONFIG_PARTITION_TABLE_CUSTOM=y` — the build
ignores your CSV anyway. Flash it explicitly: `espflash flash --partition-table
partitions.csv`, or let `cargo espflash` auto-detect the table generated by the build
script. A relative-path `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` **breaks the build**
(`esp-idf-sys/README.md` "Known limitations"). The build only needs the
partition-table *offset*; content is a flashing-time concern.

### 7.2 `espflash` / `cargo-espflash` 4.6.0

- Chips: ESP32, C2, C3, C5, C6, C61, H2, H4, P4, S2, S3, S31. Bootloaders from IDF
  `release/v5.5`.
- **esp-idf (std) apps are detected automatically**: if the package depends on
  `esp-idf-sys`, the bootloader and partition table built by the build script are used.
  Override with `--bootloader` / `--partition-table`, or `.cargo/config.toml`:
  ```toml
  [idf]
  partition_table = "path/to/custom/partition-table.bin"
  ```
- `--monitor` on `flash`, plus a standalone `monitor` subcommand, `MONITOR_BAUD`,
  `no-reset`, separate connect vs. monitor baud.
- **Works over a plain USB-to-UART bridge on the original ESP32.** The ESP32 ROM
  bootloader serial protocol *is* the esptool protocol, driven by DTR/RTS auto-reset.
  Real-world caveat: cheap ESP32-DevKitC boards without the EN↔GND 100 nF cap need the
  manual BOOT+RST dance. This is a real risk for this board — see
  [05 §3](./05-tooling-and-workflows.md).
- Other subcommands: `board-info`, `erase-flash`, `read-flash`, `write-bin`,
  `partition-table` (CSV↔bin), `hold-in-reset`, `list-ports`.
- **Caveat:** espflash's monitor *"is currently unable to properly decode ESP-IDF
  stacktraces generated by the RISC-V Espressif MCUs"*. Irrelevant for the Xtensa ESP32,
  relevant if C6 is added later — use `esp-idf-monitor` there.

### 7.3 `mise` support — verified and partially absent

| Need | mise backend | Status |
| --- | --- | --- |
| `rust` (rustup-shaped) | built-in Rust backend (`/lang/rust.html`) | **VERIFIED** — installs rustup if absent, honours `MISE_RUSTUP_HOME` / `MISE_CARGO_HOME` to isolate from the system rustup, symlinks into its own installs dir, sets `RUSTUP_TOOLCHAIN`, and asks rustup to install configured components/targets on `mise install` |
| `just` | `aqua:casey/just` (146 versions; `asdf` and `cargo:just` as fallbacks) | **VERIFIED** |
| `espflash`, `cargo-espflash`, `ldproxy`, `espup` | `cargo:` backend | **VERIFIED** — `mise use -g cargo:espflash`; defaults to `cargo-binstall` for prebuilt binaries; **requires `rust` installed first** (`get_dependencies → vec!["rust"]`); setting `features` or `default-features=false` disables binstall |
| `clang-format` | existing entry in `.mise.toml` | already declared |
| **esp-rs `esp` (Xtensa) toolchain** | **none** | **ABSENT** — no `esp` or `esp-idf` backend exists in mise. `espup` writes to `~/.rustup/toolchains/esp`, outside mise's `RUSTUP_HOME` control. Must be driven by a mise task that runs `espup install` and sources `~/export-esp.sh` |
| ESP-IDF itself | none | **ABSENT** — and not needed, since `esp-idf-sys` self-provisions |

Documentation URL correction: the Rust page lives at
`https://mise.jdx.dev/lang/rust.html`; the old `dev-tools/rust.html` 404s, as does
`dev-tools/just.html`.

### 7.4 CI

The proven pattern is the esp-rs CI recipe
([`esp-rs/esp-idf/.github/workflows/ci.yml`](https://github.com/esp-rs/esp-idf/blob/master/.github/workflows/ci.yml)),
which deliberately avoids `dtolnay/rust-toolchain` and `esp-rs/xtensa-toolchain` actions
because their downloads are not retriable:

```yaml
env:
  ESP_IDF_VERSION: v5.5.5
  RUSTFLAGS: "--cfg espidf_time64"
# Xtensa targets:
- curl -sSfL https://github.com/esp-rs/espup/releases/latest/download/espup-$host -o ~/.cargo/bin/espup
- export ESPUP_EXPORT_FILE="$HOME/exports"
- espup install -l debug --targets esp32,esp32s2,esp32s3
- source "$ESPUP_EXPORT_FILE"
- rustup default esp
# ldproxy: `cargo install ldproxy` (crates.io 0.3.5)
# free disk: ESP-IDF is built TWICE (std + no_std)
```

The matrix is 6 targets × 5 IDF versions, plus separate NimBLE and legacy-sys jobs
(`ESP_IDF_SDKCONFIG_DEFAULTS=sdkconfig.defaults.nimble`, because Bluedroid and NimBLE are
a mutually-exclusive Kconfig `choice`).

`espressif/setup-idf` **does not exist**. The real actions are
`espressif/install-esp-idf-action@v1` (tiny, 5 stars, an EIM wrapper),
`espressif/esp-idf-ci-action@v1` (build-only, 105 stars), and
`espressif/build-esp-idf-projects-action@v1`. None are needed for a `cargo`-driven build.

**Recommendation: copy the esp-rs CI recipe verbatim.** It is the only configuration
proven to build these crates, and it does not require ESP-IDF to be pre-installed.

---

## 8. Summary of unverified assumptions

Each has a spike task in [06 — Task list](./06-migration-task-list.md).

| # | Assumption | Spike |
| --- | --- | --- |
| U1 | `esp-idf-svc` 0.53.0 builds on this macOS host with ESP-IDF v5.5.5 | R1-01 |
| U2 | Which executor (`embassy-executor` vs `edge-executor` 0.5.0) is correct and stable here | R1-02 |
| U3 | TSIC-306 / ZACwire can be decoded reliably from a Rust ISR at 10 Hz | R1-03 |
| U4 | `ssd1306` + `embedded-graphics` + ported `profont`/`fub` glyphs reproduce the six templates inside 128×64 with no overlap | R1-04 |
| U5 | `EspHttpServer` can deliver SSE (or the `esp-idf-sys` shim / WebSocket fallback works) | R1-05 |
| U6 | `esp-wifi-provisioning` 0.1 works with `esp-idf-svc` 0.53, or a hand-built portal is needed | R1-06 |
| U7 | `hal::ledc` can drive the heater relay as a hardware PWM, or a GPTimer ISR is required | R1-07 |
| U8 | The board can actually be flashed by `espflash` (auto-reset circuit present?) | **needs hardware** |
| U9 | The Xtensa `esp` toolchain installs and builds on this host | R1-01 |
| U10 | Whether a bare-metal `esp-radio` TCP/IP stack exists (only matters if Approach B is revisited) | not planned |

---

## 9. Sources

- <https://github.com/esp-rs/esp-idf> (monorepo; `esp-idf-svc`, `esp-idf-hal`, `esp-idf-sys`)
- <https://github.com/esp-rs/esp-idf-svc/CHANGELOG.md>
- <https://github.com/esp-rs/esp-idf-sys/BUILD-OPTIONS.md> and `README.md`
- <https://github.com/esp-rs/esp-hal> (`esp-hal`, `esp-rtos`, `esp-radio`)
- <https://docs.espressif.com/projects/rust/book/>
- <https://github.com/esp-rs/espflash>
- <https://github.com/espressif/esp-idf/issues/14121> (SSE / chunked encoding)
- <https://github.com/esp-rs/esp-idf/issues/630> and `#619` (executor guidance)
- <https://www.ist-ag.com/sites/default/files/downloads/ATTSic_E2.3.0.pdf> (TSIC protocol)
- <https://mise.jdx.dev/lang/rust.html>, <https://mise.jdx.dev/dev-tools/backends/cargo.html>
