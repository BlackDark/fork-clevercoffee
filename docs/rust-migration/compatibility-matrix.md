# Rust compatibility matrix

**Status:** Complete (A2)
**Last updated:** 2026-09-28
**Related:** [prior-implementation-findings.md](prior-implementation-findings.md) · [inventory.md](inventory.md) · [ADR 0004](../adr/0004-rust-migration-platform-selection.md) · [architecture.md](architecture.md) · [task-list.md](task-list.md) · [execution skill](../../.agents/skills/esp32-rust-migration/SKILL.md)

Per-capability evidence for the two candidate Rust platforms and for every driver
the firmware needs. All crates.io versions and dates were checked **2026-09-28**.
Raw downloads and the full research notes are under the gitignored `research/`
directory (`research/a2-research-raw.txt`, `research/idf/`,
`research/baremetal/`, `research/drivers/`).

Verdict vocabulary:

- **verified** — explicit support found in the crate's own docs, README support
  matrix, CI matrix or source at a version tag; or measured here.
- **needs prototype** — plausible but unproven for this chip or this workload.
- **unsupported / unknown** — no evidence of support found, or actively excluded.
- **write our own** — no usable crate exists.

A crate mentioning "ESP32" was not accepted as proof of support. The original
ESP32 (Xtensa LX6) is frequently second-class relative to the RISC-V parts, so
every row below was checked for *esp32* specifically.

---

## 1. Target and toolchain facts

All **device-verified on this host**:

| Fact | Value |
|---|---|
| Chip | `esp32` rev v3.0, Xtensa LX6, dual core, 4 MB flash, no PSRAM |
| Rust toolchain | `esp` channel via espup 0.17.1 → `rustc 1.97.0-nightly (8ea53bcd7 2026-07-08) (1.97.0.0)`, LLVM 21.1.3 |
| Targets present | `xtensa-esp32-espidf` (std), `xtensa-esp32-none-elf` (no_std) |
| Prebuilt std for the target | **none** — only `aarch64-apple-darwin` in `~/.rustup/toolchains/esp/lib/rustlib/`. `-Zbuild-std` is mandatory. |
| Stable Rust | **not an option.** Xtensa lives in the Espressif fork; the RISC-V ESP32s build on stable, this chip does not. |
| Link requirement | `xtensa-esp32-elf-gcc` from espup's `xtensa-esp-elf` toolchain must be on `PATH` |

**Incidental finding worth acting on:** `mise`'s `rust` tool installs a
non-rustup Rust whose `cargo` shim shadows rustup's. With it active,
`rust-toolchain.toml` is ignored and the build fails with
`can't find crate for core` plus `'esp32' is not a recognized processor`. mise
must not manage `rust` in this repo — see [tooling.md](tooling.md).

---

## 2. Platform A — ESP-IDF / std (`esp-idf-hal`, `esp-idf-svc`) — **SELECTED**

Repos are now the monorepo [esp-rs/esp-idf](https://github.com/esp-rs/esp-idf);
the old separate repos are read-only.

| Crate | Version | Published | Licence | Verdict |
|---|---|---|---|---|
| [esp-idf-sys](https://crates.io/crates/esp-idf-sys) | 0.38.1 | 2026-09-16 | MIT OR Apache-2.0 | verified |
| [esp-idf-hal](https://crates.io/crates/esp-idf-hal) | 0.47.0 | 2026-09-15 | MIT OR Apache-2.0 | verified |
| [esp-idf-svc](https://crates.io/crates/esp-idf-svc) | 0.53.0 | 2026-09-15 | MIT OR Apache-2.0 | verified |
| [embuild](https://crates.io/crates/embuild) | 0.33.5 | 2026-09-07 | MIT OR Apache-2.0 | verified |
| [ldproxy](https://crates.io/crates/ldproxy) | 0.3.5 | 2026-07-16 | MIT OR Apache-2.0 | verified |

ESP-IDF support comes from the monorepo
[`ci.yml`](https://github.com/esp-rs/esp-idf/blob/master/.github/workflows/ci.yml),
**not** from the crate READMEs, which state no version support despite the
monorepo README claiming they do. hal + svc + sys are tested against ESP-IDF
v5.3.6, v5.4.4, v5.5.5, v6.0.3 and v6.1, and **`xtensa-esp32-espidf` is a
first-class row in every job**, with `cargo clippy -Dwarnings`, plus an esp32-only
row for IDF 6.0 remote components. `esp-idf-sys` 0.38.0 was yanked; MSRV 1.82.

### 2.1 Peripherals

| Peripheral | Safe API | Verdict |
|---|---|---|
| GPIO | `PinDriver`, incl. `subscribe` ISR callbacks | verified |
| I²C | `I2cDriver` / `I2cSlaveDriver` | verified — **see the EOL risk below** |
| Hardware timers + ISR | GPTimer API (0.46.0+); `timer-legacy` for the old one | verified (**linked here**, §6) |
| ADC | oneshot + continuous | verified |
| LEDC | `LedcDriver` incl. fades | verified |
| RMT | new RMT API with `RxChannelDriver`, `receive()`, `receive_async()` | verified |
| PCNT | new PCNT API | verified |
| UART | `UartDriver`, `AsyncUartDriver` | verified |
| SPI | `SpiDriver` + `SpiDeviceDriver`, DMA, async | verified |
| Task watchdog | `task::watchdog::{TWDTDriver, TWDTConfig}` over `esp_task_wdt` | verified (**linked here**) |
| **MCPWM** | **none** — no `mcpwm.rs`; [issue #92](https://github.com/esp-rs/esp-idf-hal/issues/92) open since 2022-06-22 | unsupported (raw `esp-idf-sys` only) |

**I²C EOL risk, concrete.** `esp-idf-hal`'s `i2c.rs` wraps the *legacy*
`driver/i2c.h` (`i2c_param_config` + `i2c_driver_install` + command links). The
[IDF v6.0 peripherals migration guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/migration-guides/release-6.x/6.0/peripherals.html)
declares that driver **End-of-Life in v6.0, scheduled for removal in v7.0**, with
no timely bug or security fixes. The hal migrated ADC, timer, I²S, PCNT, RMT and
MCPWM to the new APIs and **skipped I²C**. No `i2c_master.h` wrapper exists.
Mitigation: pin ESP-IDF to ≤ 6.x, and treat writing an `i2c_master` wrapper as a
known future cost. Not a present blocker — we build against v5.3.6.

### 2.2 Wi-Fi, HTTP, MQTT, OTA, storage

| Capability | API | Verdict |
|---|---|---|
| STA / AP / **AP+STA** | `Configuration::{Client, AccessPoint, Mixed}` → `WIFI_MODE_APSTA` | verified (**linked here**) |
| scan, netif, static IP, **mDNS**, NAPT, ESP-NOW | modules present, built in CI | verified |
| captive portal | no helper; DNS hijack must be hand-rolled on a UDP socket | needs prototype |
| IDF `wifi_provisioning` component | no safe wrapper, **but** `wifi_provisioning/manager.h`, `scheme_ble.h`, `scheme_softap.h` are in `esp-idf-sys`'s bindings header | needs prototype (reachable via raw FFI) |
| HTTP server | `EspHttpServer` over `httpd_register_uri_handler`; `raw_connection()` exposes `httpd_req_t` | verified (**linked here**) |
| streaming request body | `EspHttpConnection::read` → `httpd_req_recv` | verified (**linked here**) |
| multipart parsing | not provided — raw bytes, parse ourselves | needs prototype |
| precompressed gzip static | `initiate_response` takes arbitrary headers | verified (**linked here**) |
| SSE | chunked `write` → `httpd_resp_send_chunk` works, but each stream pins a worker. **`max_open_sockets` defaults to 4 and `lru_purge_enable` defaults to true**, which will evict an idle SSE connection — both must be set explicitly. | needs prototype |
| MQTT | `EspMqttClient`; `LwtConfiguration{topic,payload,qos,retain}`, `publish(topic,qos,retain,payload)`, `subscribe(topic,qos)`, settable `buffer_size` | verified (**linked here**) |
| OTA | `EspOta::{initiate_update,write,finish,complete}`, abort on Drop, `mark_running_slot_valid`, `EspFirmwareInfoLoad` | verified (**linked here**) |
| raw partition by label | `EspPartition::find_by_label`, `read`/`write`/`erase`, `EspMemMappedPartition` | verified (**linked here**) |
| LittleFS / SPIFFS / FATFS | all three in `esp-idf-svc/src/fs/` | verified |
| Logging | `EspIdfLogger` implements `log::Log` with ESP-IDF-identical output; `init_from_esp_idf()` honours `CONFIG_LOG_*` | verified (**linked here**) |

### 2.3 The NVS question — no longer a requirement

> **Superseded by [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md).**
> Backward compatibility was dropped: NVS may be restructured, and migration happens
> by exporting `config.json` from the old web UI and importing it into the new
> firmware. Nothing below is needed any more. It is kept because it was the decisive
> argument in ADR 0004, so anyone re-reading that record needs to see what was
> removed — and because it would matter again if a compatible read is ever wanted.

**Reading an existing device's config is byte-exact, and provable.** Arduino's
`Preferences` has no encoding of its own; it is a direct `nvs_set_*` passthrough:

| Arduino | ESP-IDF NVS |
|---|---|
| `putChar/putUChar` | `nvs_set_i8` / `u8` |
| `putShort/putUShort` | `i16` / `u16` |
| `putInt/putUInt/putLong/putULong` | `i32` / `u32` |
| `putLong64/putULong64` | `i64` / `u64` |
| `putString` | `nvs_set_str` |
| `putBytes` | `nvs_set_blob` |
| `putBool` | `putUChar` → `u8` 0/1 |
| **`putFloat`** | **`putBytes(&value, 4)` — a BLOB, raw little-endian** |
| **`putDouble`** | **`putBytes(&value, 8)` — a BLOB** |

`esp_idf_svc::nvs::EspNvs` exposes matching `get_`/`set_` for every integer width,
`get_str`/`set_str`, `get_blob`/`set_blob`, `blob_len`, `str_len`, `erase_all`.
So the `config` namespace with its `"p" + FNV-1a-hex` keys maps 1:1.
**Verdict: verified, but no longer required.**

Two traps to carry into implementation:

1. Anything the C++ wrote with `putFloat`/`putDouble` must be read with
   `get_blob` into 4/8 bytes and `f32::from_le_bytes`/`f64::from_le_bytes` — **not**
   `get_u32`/`get_u64`. Given the config model uses `double` heavily, this applies
   to most numeric parameters.
2. NVS keys are capped at 15 characters, which is exactly why the C++ hashes them.

### 2.4 Task model and timing

| Item | Verdict |
|---|---|
| std threads | verified — pthread-backed, `ThreadSpawnConfiguration` for name, priority, core pinning, stack caps |
| `esp_timer` | verified — `EspTimer`, `EspAsyncTimer`, `EspTaskTimerService`, µs resolution |
| critical sections | verified — `task::CriticalSection` (FreeRTOS recursive mutex, **not ISR-safe**) vs `interrupt::IsrCriticalSection` / `interrupt::free` |
| `critical-section` / `embassy-sync` impls | verified, behind features |
| `embassy-time` | verified via the `embassy-time-driver` feature — **but see §6, it does not link on its own** |
| `embassy-executor` | needs prototype — no integration crate; `task::block_on`, `IsrReactor`, `edge-executor` are the documented paths |
| `tokio` | unsupported — only via an unstable `mio_unsupported_force_poll_poll` cfg |

**Threads-first, not async-first.** That matches how the existing firmware is
structured and is a point in this platform's favour.

### 2.5 Maturity and risk

- All crates MIT OR Apache-2.0. **No licence blocker.** The port also *removes* the
  LGPL-3.0 `ESPAsyncWebServer`/`AsyncTCP` dependency from a statically linked image.
- Last monorepo commit seen 2026-09-26; releases Sep 2026. Breaking minors land a
  few times a year (0.46.0 removed `Peripheral`/`PeripheralRef` and `prelude`,
  moved timer/pcnt/rmt to new APIs, raised MSRV to 1.82).
- **Bus factor ≈ 1** (`ivmarkov` publishes sys/hal/svc/embuild/ldproxy).
- **No hardware-in-the-loop tests anywhere in the stack.** All three READMEs say so.
  Green CI means "it linked". There has also been a breaking change in a *patch*
  release (0.42.5) to fix discovered UB in `subscribe` callbacks.
- All three READMEs state Espressif puts "little to no paid developer time" into
  these crates and redirect to `esp-hal`. The official esp-rs book has been
  rewritten with **no std/esp-idf chapters at all**. The funded direction is
  elsewhere — this is the strongest argument against, and it is a project-health
  argument, not a capability one.
- `docs.rs` cannot build these crates (they need the ESP-IDF toolchain), so there
  is **no rendered API reference**; read source at the version tag.
- Sharp edges: ESP-IDF cannot build on a filesystem without symlinks; and if
  `CARGO_TARGET_DIR` is moved without setting `CARGO_WORKSPACE_DIR`, the
  `sdkconfig.defaults` is **silently ignored**.

---

## 3. Platform B — bare metal (`esp-hal` + `esp-rtos` + `esp-radio`)

| Crate | Version | Published | Licence | Verdict for esp32 |
|---|---|---|---|---|
| [esp-hal](https://crates.io/crates/esp-hal) | 1.2.2 | 2026-09-18 | MIT OR Apache-2.0 | verified |
| [esp-rtos](https://crates.io/crates/esp-rtos) | 0.4.0 | 2026-08-26 | MIT OR Apache-2.0 | verified (replaces the EOL `esp-hal-embassy` 0.9.1) |
| [esp-radio](https://crates.io/crates/esp-radio) | 0.18.0 stable / 1.0.0-beta.1 | 2026-04-16 / 2026-09-16 | MIT OR Apache-2.0 + **closed blobs** | needs prototype |
| [esp-storage](https://crates.io/crates/esp-storage) | 0.10.0 | 2026-08-26 | MIT OR Apache-2.0 | verified |
| [esp-bootloader-esp-idf](https://crates.io/crates/esp-bootloader-esp-idf) | 0.6.0 | 2026-08-26 | MIT OR Apache-2.0 | verified |
| [esp-nvs](https://crates.io/crates/esp-nvs) | 0.5.0 | 2026-07-10 | MIT/Apache (crates.io) vs Apache-2.0 (repo) | needs prototype |
| [picoserve](https://crates.io/crates/picoserve) | 0.20.1 | 2026-09-21 | MIT | needs prototype |
| [rust-mqtt](https://crates.io/crates/rust-mqtt) | 0.6.0 | 2026-09-22 | MIT OR Apache-2.0 | needs prototype |
| [trouble-host](https://crates.io/crates/trouble-host) | 0.8.0 | 2026-08-25 | MIT OR Apache-2.0 | unsupported at latest (§3.4) |

`esp-hal`'s README states **ESP32 revisions below v3.0 are not supported** — our
device is exactly v3.0, so zero margin if that floor ever rises.

### 3.1 What "1.0 stable" actually covers

From `esp-hal/src/lib.rs` at v1.2.2, the semver-stable set is:
`clock, gpio, i2c, spi, uart, system, time, peripherals, rng, efuse, interrupt`.

Everything this project needs beyond that is inside `unstable_module!` /
`unstable_driver!` with **no semver protection**: `timer` (TIMG), `analog` (ADC),
`rtc_cntl` (RWDT), `dma`, `ledc`, `mcpwm`, `rmt`, `pcnt`, `i2s`, `delay`.
And `esp-radio` **requires** the `unstable` feature on `esp-hal`, so the stability
guarantee evaporates the moment Wi-Fi is enabled.

esp32-specific absences: **no SYSTIMER** (TIMG only, and `esp-rtos` consumes one
TIMG timer for the scheduler), no USB peripheral, no I²C slave, no ULP-FSM, no
SWD watchdog.

### 3.2 Wi-Fi — the key risk

Network stack is `embassy-net` + `smoltcp`; `blocking-network-stack` is git-only,
so async `embassy-net` is effectively the only maintained path. The driver itself
is a **closed Espressif blob** (`esp-wireless-drivers-3rdparty`).

esp32 appears in the support table with Wi-Fi ✓, BLE ✓, Coex ✓, ESP-NOW ✓, and
STA/AP/AP+STA examples build and link for `xtensa-esp32-none-elf`. But:

- **`esp-radio` is not 1.0.** Max stable is 0.18.0 (2026-04-16), the newest is
  1.0.0-beta.1, and **1.0.0-beta.2 was yanked one day after publication** with no
  reason in the CHANGELOG.
- **`ADC2` cannot be used simultaneously with the radio on ESP32** (esp-radio
  0.16.0 changelog, [#3876](https://github.com/esp-rs/esp-hal/pull/3876)) — a hard
  hardware constraint that would have to be checked against every pin.
- **Coexistence on ESP32 only started working in 0.14.0 (2025-06-03)**
  ([#3403](https://github.com/esp-rs/esp-hal/pull/3403)) — it was broken for about
  18 months. Thin track record.
- Open issues that matter for an appliance:
  [#1600](https://github.com/esp-rs/esp-hal/issues/1600) **no WPA3 station on
  ESP32**; [#5889](https://github.com/esp-rs/esp-hal/issues/5889) **after an
  `AuthenticationFailed` the next attempt also fails** — a machine that quietly
  stops rejoining the network; [#5309](https://github.com/esp-rs/esp-hal/issues/5309)
  buffer overflow in the Wi-Fi drivers; [#6390](https://github.com/esp-rs/esp-hal/issues/6390)
  ESP32 light-sleep with Wi-Fi.
- Scheduler bug [#6411](https://github.com/esp-rs/esp-hal/issues/6411)
  (`BorrowMutError` on first task switch) is reported on S3 Xtensa; whether the
  original esp32 is affected **cannot be determined without a hardware run**.

### 3.3 Size and RAM — measured, not quoted

No published esp32 size or RAM figures exist in `esp-hal`, the esp-rs book or
developer.espressif.com. The research pass therefore built and linked three
official examples here (esp-hal 1.2.2 + esp-radio 1.0.0-beta.0 + esp-rtos 0.4.0,
upstream release profile, image via `espflash save-image`):

| Variant | Flash image | % of 1.625 MiB | Static RAM | Stack | Heap | DRAM total |
|---|---|---|---|---|---|---|
| STA + embassy-net + DHCP + HTTP client | 675.0 KiB | 40.6 % | 136.3 K | 119.6 K | 64.0 K | 319.9 K |
| AP+STA | 674.9 KiB | 40.6 % | 136.8 K | 119.6 K | 64.0 K | 320.5 K |
| Wi-Fi + BLE coex (trouble) | 928.5 KiB | 55.8 % | 165.2 K | 54.4 K | 96.0 K | 315.6 K |

Flash is comfortable. **RAM is the wall**: DRAM is essentially fully allocated
(~320 KiB) in every variant because the linker hands `.stack` whatever is left, and
enabling BLE coex collapses stack from 119.6 KiB to 54.4 KiB. These examples
contain no display, no PID, no MQTT, no NVS, no OTA and no web-UI buffers. There is
no published `esp-radio` memory breakdown to budget against
([#6323](https://github.com/esp-rs/esp-hal/issues/6323) has no numbers).

### 3.4 Where bare metal runs out of road for *this* product

> **Re-scoped by [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md).**
> Both gaps below were about *preserving deployed state*. That requirement is gone,
> so neither is blocking any more. Gap 1 disappears entirely — a greenfield store
> like `sequential-storage` 8.0.1 was already rated verified. Gap 2 shrinks from
> "no path exists" to "needs a prototype": with the layout free, a bare-metal build
> could define its own asset format and stream it from flash. They are kept for the
> record because they are what ADR 0004 turned on.

Two gaps are unsolved by the vendor and both sat on the critical path to not
breaking existing machines:

1. **Reading the existing ESP-IDF NVS partition** has exactly one candidate,
   `esp-nvs` 0.5.0 — single maintainer, 22 stars, 4,616 total downloads, created
   2025-11-19, README still pinning `esp-storage` 0.8.1 while current is 0.10.0,
   and a licence discrepancy between crates.io and the repo. There is **no
   Espressif-official no_std NVS reader.** If it does not work, existing devices
   lose their settings on upgrade.
2. **Serving the existing 640 KB LittleFS web-UI partition has no crate path at
   all.** `littlefs2` 0.8.1 binds the C littlefs (same on-disk v2 format), but no
   glue to `esp-storage` exists and no instance of anyone mounting an ESP-IDF
   littlefs partition from no_std Rust was found. Compounding it, `picoserve`'s
   `fs` module serves compile-time `&'static [u8]`, **not a filesystem** — and a
   640 KB UI cannot be compiled into a 1.625 MB app partition alongside the
   firmware.

BLE also has a live version-skew blocker, though BLE is not on the critical path
because the scale is dead code:

| `trouble-host` | wants `bt-hci` | pairs with |
|---|---|---|
| 0.8.0 (2026-08-25) | ^0.10 | **nothing on ESP32 yet** |
| 0.7.0 (2026-06-16) | ^0.9 | `esp-radio` 1.0.0-beta.1 (pre-release) |
| 0.6.0 (2026-02-11) | ^0.8 | `esp-radio` 0.18.0 → `esp-hal ~1.1.0-rc.0`, **incompatible with 1.2.2** |

### 3.5 What bare metal is genuinely better at

Recorded honestly, because these are real and they are why this stays the
fallback rather than being discarded:

- **Espressif staffs `esp-hal`**, and the official book is now an esp-hal book.
  The long-term direction of the ecosystem is here.
- **OTA is officially solved**: `esp-bootloader-esp-idf` 0.6.0 reads and writes
  genuine ESP-IDF `otadata` (with CRC + state validation), partition tables and app
  descriptors, and ships an `esp_app_desc!` macro — so existing devices could be
  OTA'd to a Rust image rather than re-flashed by hand.
- **A real concurrency model**: embassy tasks, `start_second_core`, and
  priority-based `InterruptExecutor`s for timing-critical work, with preemption and
  stack-overflow checking.
- No ESP-IDF build system, crates.io-only dependency graph, and fast iteration
  (25–35 s full example builds, 4 s incremental, observed).
- Churn is the cost: 26 `esp-hal` releases, 8 breaking minors in 10 months
  pre-1.0, a 357-line migration guide for 1.1.0 *after* 1.0 shipped, and two
  whole-crate renames in the last year (`esp-wifi`→`esp-radio`,
  `esp-hal-embassy`→`esp-rtos`). Any tutorial older than ~12 months is wrong.

---

## 4. Drivers and libraries

Any crate on `embedded-hal` **1.0** works on both platforms; anything on 0.2 works
only on std/ESP-IDF, and only through the compat shim.

| # | Device / library | Today | Choice | Verdict | Risk |
|---|---|---|---|---|---|
| 1 | TSIC 306 / ZACwire | `ZACwire` 2.0.0 (MIT) | **write our own**, RMT RX capture | write our own, spec verified | **med** |
| 2 | DS18B20 / 1-Wire | DallasTemperature 4.0.6 + OneWire 2.3.8 | [`onewire`](https://crates.io/crates/onewire) 0.4.0 (2025-05-12, eh 1.0) | needs prototype | low |
| 3 | Honeywell ABP2 | hand-rolled in `pressureSensor.h` | **write our own** (no crate exists) | write our own, spec verified | low |
| 4a | SSD1306 128×64 I²C | U8g2 2.36.18 | [`ssd1306`](https://crates.io/crates/ssd1306) 0.10.0 + [`embedded-graphics`](https://crates.io/crates/embedded-graphics) 0.8.2 | verified | low |
| 4b | SH1106 128×64 I²C | U8g2 | [`oled_async`](https://crates.io/crates/oled_async) 0.2.1, else ~150 LOC of our own | needs prototype | **med** |
| 4c | U8g2 font parity | U8g2 fonts | [`u8g2-fonts`](https://crates.io/crates/u8g2-fonts) 0.8.0 + anchor shim | needs prototype | **med** |
| 5 | HX711 load cell | HX711_ADC 1.2.12 | [`loadcell`](https://crates.io/crates/loadcell) 0.3.0 — **dead code today** | deprioritise | low |
| 6 | PID | vendored `PID_v1` 1.2.1 | **hand-port**; reject the `pid` crate | write our own | **high** |
| 7a | Acaia / multi-scale BLE | `AcaiaArduinoBLE` v4.0.1 | **write our own** — **dead code today** | deprioritise | **high** if revived |
| 7b | BLE central stack | NimBLE-Arduino | [`esp32-nimble`](https://crates.io/crates/esp32-nimble) 0.13.0 (std) | verified (std) | low |
| 8 | MQTT | PubSubClient 2.8.0 | `esp-idf-svc::mqtt` | verified | low |
| 9 | JSON | ArduinoJson 7.4.3 | `serde` 1.0.229 + `serde_json` 1.0.151 | verified | low |
| 10 | HTTP server | ESPAsyncWebServer 3.12.1 (LGPL-3.0) | `esp-idf-svc::http::server` | verified | low |
| 11 | Wi-Fi provisioning portal | tzapu/WiFiManager 2.0.17 | hand-rolled AP + DNS hijack, or raw `wifi_provisioning` FFI | needs prototype | **med** |

### 4.1 TSIC 306 / ZACwire — the most interesting driver

**Reject [`tsic`](https://crates.io/crates/tsic) 0.2.1** (2020-10-12, last commit
2021-01-11, `embedded-hal` 0.2 `unproven`). The problem is not just the old HAL:
`Tsic::read()` is a pure busy-wait spin that blocks for the whole ~2.5 ms
transmission and re-measures the strobe by polling at 8 µs. With Wi-Fi active on an
ESP32 that will mis-decode, and the author's own commit message documents exactly
that failure ("strobe length of 0 … discovered in the wild").

Protocol, verified against IST AG app note ATTSic_E2.1.4
(`research/drivers/zacwire-appnote.txt`):

- One wire, idle high, duty-cycle (Manchester-like) encoding, a falling edge at
  every bit-window boundary.
- Nominal 8 kHz → **125 µs bit window**. Start bit 50 % duty → strobe ≈ 62.5 µs.
  Logic 1 = 75 % duty (low ≈ 31 µs); logic 0 = 25 % duty (low ≈ 94 µs).
- Packet = start + 8 data bits MSB-first + **even parity**. A reading is **two
  packets** with one idle-high window between them; packet 1 carries `00` +
  T[10:8], packet 2 carries T[7:0]. TSIC 306 updates at 10 Hz; servicing a read
  costs ~2.7 ms.

**Recommended capture mechanism: RMT RX**, available on both platforms
(`esp-idf-hal` 0.47 `rmt::RxChannelDriver::new(pin, &RxChannelConfig)` with
`receive()`/`receive_async()` and `signal_range_min`/`signal_range_max` at a 1 µs
default resolution). A frame is ~20 level+duration symbol pairs, far inside the
ESP32's 64-word RMT block. Set the glitch filter to ~2–5 µs and the idle threshold
to ~200–300 µs so the long idle gap terminates the frame. That gives **one
interrupt per reading instead of 20**, hardware timestamps, no Wi-Fi jitter
sensitivity, and — the important part — **the decoder becomes a pure function over
a symbol slice, so it is fully host-testable**.

GPIO edge interrupts work (it is what the C++ does) but the discrimination margin
is only ±31 µs around the 62.5 µs threshold and it costs 20 ISR entries per 100 ms.
**PCNT is unsuitable** — it counts edges and cannot measure pulse width, so it
cannot recover duty-cycle-encoded bits.

**Conversion formula is a deliberate choice.** The vendored library uses
`((temp * 250L >> 8) - 499) / 10.0`, which differs from the canonical datasheet
`raw * 200/2047 - 50` by up to ~0.1 °C. **Keep the library formula** to preserve
existing users' calibration offsets, and say so in the code.

### 4.2 Display font parity — quantified

`u8g2-fonts` 0.8.0 (code MIT/Apache-2.0, **font data under U8g2's own licence**,
hence `license = non-standard` on crates.io) reimplements U8g2's *renderer* over
U8g2's *original binary font data*, so glyph bitmaps will be identical. Both fonts
the firmware uses are bundled: `u8g2_font_fub20_tf` (5433 B) and
`u8g2_font_profont17_tf` (3137 B), out of 1999 fonts, each behind a unit struct
with `include_bytes!` so **only named fonts get linked** (~8.6 KB for the two).

**⚠️ CONTRADICTED, unresolved -- do not build on this section yet.** The parallel
implementation reports that `OledDriver::prepareDisplay` **never calls
`setFontPosTop`**, and that this made the Modern `fub20` readout at y=14 render at rows
-9..13 and be clipped off the panel. That came out of a pixel diff against real U8g2,
so it carries more weight than the read below. If it is right, this whole anchor
analysis needs redoing and the C++ display has a live clipping bug. Settle it against
`src/display/OledDriver.cpp` before trusting either -- see
[prior-implementation-findings.md §3.2](prior-implementation-findings.md).

**Anchor mismatch, as originally read.** `OledDriver.cpp` calls
`setFontRefHeightExtendedText()` *and* `setFontPosTop()`. U8g2's
`u8g2_font_calc_vref_top` returns `font_ref_ascent + 1`, and in XTEXT mode
`u8g2_UpdateRefHeight` raises `font_ref_ascent` to `max(ascent_A, ascent_para)`.
`u8g2-fonts`' `VerticalPosition::Top` is hard-coded to `ascent_A + 1`:

| Font | `ascent_A` | `ascent_para` | U8g2 Top | u8g2-fonts Top | Δ |
|---|---|---|---|---|---|
| `fub20_tf` | 20 | 20 | 21 | 21 | **0 px** |
| `profont17_tf` | 11 | 13 | 14 | 12 | **2 px** |
| `profont11_tf` | 7 | 8 | 9 | 8 | 1 px |
| `profont10_tf` | 6 | 7 | 8 | 7 | 1 px |

So the large temperature readout lands correctly and every profont label lands
1–2 px too high. **Fix: a ~30-line shim** that renders with
`VerticalPosition::Baseline` at `y + max(ascent_A, ascent_para) + 1`. Display
parity is achievable. Separately, `ModernTemplate.h`'s `kFontHeightFub20 = 23` and
`kFontHeightProfont17 = 15` are hand-measured for the drawn glyph subset, not font
metrics (`max_char_h` is 36 and 17) — **port the literals, do not recompute them**.

Caveat: the crate's own tests are golden PNGs of its *own* output, not a
cross-check against U8g2's C output. "Pixel-identical" is credible, not proven —
hence *needs prototype*.

### 4.3 PID — the highest-risk port

**Reject [`pid`](https://crates.io/crates/pid) 4.0.0.** It has no time input at
all, no proportional-on-measurement, no sample-time gating and no manual/automatic
mode, and its per-term `p_limit`/`i_limit`/`d_limit` clamps are a different
construct from `SetIntegratorLimits`. (4.1.0 was yanked 2025-04-08.) No
alternative crate reproduces `PID_v1` semantics.

**Hand-port the vendored library into a host-testable `no_std` crate.** Existing
users' tuning numbers only stay meaningful if these are reproduced, and the
vendored copy is **not stock Beauregard 1.2.1**:

1. Conditional integration — `integrator += ki*error` is skipped when the output is
   already saturated, in `P_ON_E` mode only.
2. An EWMA input filter (`SetSmoothingFactor`) used for the derivative in `P_ON_E`
   mode only; `P_ON_M` deliberately uses the unfiltered input.
3. Double anti-windup — integrator clamped to `[outMin, outMax]` always, then to
   `[integratorMin, integratorMax]` in `P_ON_E` mode only.
4. `SetTunings` folds sample time in (`ki = Ki·dt`, `kd = Kd/dt`); `SetSampleTime`
   rescales both by ratio; `Initialize()` does bumpless transfer by seeding
   `integrator = *myOutput`.
5. **Latent hazard**: in `P_ON_E` mode the derivative divides by
   `SampleTime / 1000` using **integer** division on `unsigned long`. Since
   `windowSize_` is exactly 1000 the divisor is 1 and it is a no-op — but any value
   below 1000 divides by zero and 1500 truncates to 1. A Rust port using a float
   `dt` is numerically identical at the shipped 1000 ms and better elsewhere.
   **That is a deliberate behaviour change to declare, not to smuggle in.**

Blocking prerequisite: **`P_ON_M`/`P_ON_E` are inverted between the firmware header
and the host-test stub** (inventory §7.2). The existing tests cannot serve as the
port's reference oracle until that is fixed.

### 4.4 Honeywell ABP2 — cheapest item on the list

No crate exists (`honeywell_mpr` and `mprls` are a different part and protocol).
The protocol is four I²C operations, so this is ~120–180 LOC on `embedded-hal` 1.0
`I2c` + `DelayNs`, host-testable against a mock bus.

The datasheet confirms the **pressure** maths in the C++ is correct: transfer
function A spans 10 %–90 % of 2²⁴ counts, i.e. 1677722 to 15099494, and
`ABP2LANT010BG2A3XX` decodes to 10 bar gauge, function A, 3.3 V. The **temperature**
maths is wrong — see inventory §4.2. Fix it in the port and note the change.

### 4.5 Write-our-own list

| Item | Est. Rust LOC | Host-testable? |
|---|---|---|
| ZACwire / TSIC 306 decoder (symbols → 2 packets → parity → °C, incl. the change-rate guard) | 250–400 | **yes** — pure function over a symbol slice |
| └ RMT platform glue | ~80 | no |
| Honeywell ABP2 driver | 120–180 | **yes** |
| `PID_v1` port (P_ON_M/P_ON_E, EWMA, dual clamps, bumpless transfer, sample-time folding) | 300–400 + ~200 tests | **yes** |
| U8g2 `setFontPosTop` + XTEXT ref-ascent shim | ~30 | **yes** |
| Wi-Fi provisioning (AP + DNS hijack + portal, or raw `wifi_provisioning` FFI) | 200–400 | partly |
| Multipart parser for OTA upload | 100–150 | **yes** |
| SH1106 page-writer, only if `oled_async` disappoints | ~150 | partly |
| HX711 averaging / outlier-rejection / tare state machine — only if the scale is revived | ~200 | **yes** |
| Acaia BLE protocol codec — only if the scale is revived | 250 (Acaia only) – 1000 (6 families) | **yes** (codec) |

---

## 5. Everything still unverified

Carried into [task-list.md](task-list.md) as explicit spikes or as open risk.

| # | Item | Why it matters | Where it is handled |
|---|---|---|---|
| U1 | **Nothing has been flashed.** Every runtime claim is compile/link-level only. | Wi-Fi stability, timing, reconnect behaviour and heap use are all unproven. | SPIKE-2, and every phase gate |
| ~~U2~~ | ~~Reading a real device's NVS `config` namespace~~ **Dropped** — ADR 0005 removed the requirement. Replaced by U12. | — | SPIKE-3 deleted |
| U3 | ZACwire decode via RMT against real TSIC hardware. | The only timing-critical driver. | SPIKE-4 |
| U4 | Pixel parity of `u8g2-fonts` vs U8g2's C renderer. | Visible regression if wrong. | SPIKE-5 |
| U5 | SSE behaviour under `EspHttpServer`'s socket limits. | The web UI's live telemetry channel. | SPIKE-6 |
| U6 | Wi-Fi provisioning mechanism feasibility on-device. | Decides the provisioning design. | SPIKE-7 |
| U7 | `exit(0)` semantics on Arduino-ESP32 (inventory §3.2). | Affects what "init failed" means for parity. | Observation during SPIKE-2 |
| U8 | Whether simultaneous BLE + Wi-Fi + MQTT ever worked in the C++ firmware. | Only matters if the scale is revived. | Deferred — out of scope while the scale is dead code |
| U9 | Licences for 11 of 13 third-party libraries. | Only affects what we may copy from them. | Resolved by not copying; ADR 0004 §Licence |
| U10 | `esp-idf-hal`'s I²C wrapping an EOL IDF driver. | Forces ESP-IDF ≤ 6.x, or a wrapper we write. | Pinned IDF; recorded risk |
| U11 | Whether the water-tank switch polarity or the other four is wrong. | A real behavioural question for parity. | Needs a user/hardware decision |
| **U12** | **Does `config.json` as exported by the old web UI round-trip into the new firmware?** This replaces U2 and is now the *only* data-compatibility surface. | If wrong, users cannot migrate their settings at all. | ORACLE-4, DOMAIN-8 |
| **U13** | Does the boot layout guard actually fire on a device carrying the C++ partition table? | It is what makes a half-migrated machine inert rather than dangerous. | BOOT-1, SPIKE-8 |

---

## 6. What this host actually proved

Beyond the research above, the following were **executed here** rather than read:

1. **The C++ baseline builds**: `pio run -e esp32_usb` → SUCCESS, 65 s, flash
   90.4 %, RAM 14.1 %. This also resolved the Arduino 2.x-vs-3.x timer-API
   contradiction (platform 7.1.3 → Arduino core 2.0.17 → ESP-IDF 4.4).
2. **The device was identified**: `espflash board-info` → esp32 rev v3.0.
3. **The Xtensa Rust toolchain works here**, with both targets available.
4. **SPIKE-1: a Rust + ESP-IDF image carrying every needed subsystem links and
   fits.** Full write-up in `research/spike-size/RESULT.md`.

   | Build | App image | % of 1.625 MiB | Headroom |
   |---|---|---|---|
   | **Rust + ESP-IDF** | **997,648 B (974.3 KiB)** | **58.5 %** | **689.7 KiB** |
   | C++ baseline | 1,539,657 B (1503.6 KiB) | 90.4 % | 160.4 KiB |

   Static DRAM 34,092 B for the Rust spike vs 75,240 B reported for the C++ build.
   Linked and reachable from `main`: Wi-Fi STA+AP, `EspHttpServer` with JSON,
   gzip-static and streaming-OTA handlers, `EspMqttClient` with LWT+retain,
   `EspNvs` with `get_blob`, `EspOta`, `EspPartition`, `I2cDriver`, a GPTimer with
   a subscribed ISR, `TWDTDriver`, `PinDriver`, `serde_json`, `EspLogger`.

   It was **never flashed** and holds no credentials. It contains no display,
   fonts, PID, config registry or state machine — the 690 KiB of headroom is what
   absorbs those.

5. **Three API facts that contradict older documentation**, found by compiling:
   - `esp-idf-hal` 0.47 removed `prelude` and `Peripheral`/`PeripheralRef`. Timers
     moved to GPTimer: `TimerDriver::new(&TimerConfig)` takes **no peripheral**,
     config types live in `esp_idf_hal::timer::config`, and the alarm is
     `set_alarm_action(Some(&AlarmConfig{..}))` then `enable()` + `start()`.
   - `esp-idf-svc` 0.53's `embassy-time-driver` feature **does not link on its own**
     — `undefined reference to __embassy_time_queue_item_from_waker`. It needs
     `embassy-time-queue-utils` wired up too. Do not enable it casually.
   - On this nightly the `panic_immediate_abort` build-std feature is a hard error;
     the replacement is `panic = "immediate-abort"` with `-Zunstable-options`. It
     remains an unused size lever.
