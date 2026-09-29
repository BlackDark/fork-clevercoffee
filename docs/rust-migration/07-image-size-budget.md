# Image Size Budget

Every phase gate must account for where the flash went. This document defines the
policy; `just/size.just` (created by 06 R1-09) and `just size` produce the numbers.

Related: [03 — Decision record](./03-decision-record.md) (the 154 KB problem) ·
[05 — Tooling](./05-tooling-and-workflows.md#4-justfile) ·
[06 — Task list](./06-migration-task-list.md)

---

## 1. The problem, stated once

| Quantity | Value | Source |
| --- | --- | --- |
| C++ `firmware.bin` | **1,546,240 B** | measured, `pio run -e esp32_usb` 2026-09-28 |
| `app0` slot (current table) | **1,703,936 B** (`0x1A0000`) | `partitions_4M.csv` |
| **Headroom** | **157,696 B (154.0 KiB)** | arithmetic, verified |
| A Rust esp-idf image with `std`, measured | **≥ 1,835,008 B — it filled a 1,835,008 B slot** | [08 §2](./08-recovered-oracle.md#2-partition-table-it-used): the recovered firmware's app0 image occupied its slot to the last non-`0xFF` byte |
| **Our own minimal image, measured 2026-09-28 (R1-01)** | **382,528 B** — blink-equivalent: `std`, logging, three GPIO pins, a TWDT-fed control task, and the whole default ESP-IDF component set | `just size`; recorded in `size-baseline.json` as label `r1-01-minimal` |

**154 KiB will not hold a Rust esp-idf image.** The partition table must change. This
document tracks *how much* room we won and *what* is spending it.

> **The rebalance is no longer a hypothesis.** The recovered firmware (08 §2) used
> `app0/app1 = 0x1C0000` (1,835,008 B) and `littlefs = 0x60000` (393,216 B) — +128 KB per
> app slot, taken from 256 KB of filesystem. That is exactly what the formula below
> produces, and it is the **known-good target** for R0-02 / R2-03.

### The arithmetic R0-02 must respect

The region between `0x10000` and the `coredump` partition at `0x3F0000` is
**4,063,232 B** total for `app0` + `app1` + `spiffs`. An earlier draft asked for "≥ 2 MB
per app slot"; **that is arithmetically impossible** — two 2 MiB slots need 4,194,304 B,
which is 131,072 B *more than the whole region*, before any filesystem.

The correct target is a formula, not a number:

> Maximise `min(app0, app1)` subject to `nvs`, `otadata`, and `coredump` staying
> **byte-identical**, and `spiffs ≥ S`, where `S` is the measured SPA size from R0-02 —
> or **64 KB** if the SPA is embedded in the binary instead.

Sanity check for the agent: with a 64 KB `spiffs`, the maximum is
`(4,063,232 − 65,536) / 2` = **1,998,848 B ≈ 1.906 MiB per slot**. If your computed `app0`
exceeds that, you have made an arithmetic error.

---

## 2. What the region is for

Decided 2026-09-28:

| Decision | Rationale |
| --- | --- |
| **No NVS backward compatibility.** Rust owns its key namespace (`cc.` prefix). | Frees the design from replicating the C++ FNV-1a key hashing. Config starts from defaults once. |
| **HTTP OTA is optional; `espota` is desirable if cheap.** | A forced full flash over USB is the primary update path. If the image gets tight, HTTP OTA is the first thing to drop — it is the single largest optional feature. |
| **The React SPA is embedded in the binary** (`include_bytes!`) unless measurement says otherwise. | Removes the filesystem-upload step, the separate `buildfs` target, and a whole class of "UI didn't update" bugs. Also means `spiffs` can shrink to near zero. |
| **The UI in `ui/` stays as-is** except if R1-05 forces WebSocket instead of SSE. | Per the human, 2026-09-28. The firmware must serve the existing bundle; only the live-event transport is negotiable. |

---

## 3. Drop order — decided in advance, not in a panic

If the image is too large, features are removed **in this order**. Do not re-litigate
mid-phase; do not remove a safety path under any circumstance.

| # | Candidate | Est. saving | Why it is safe to drop |
| --- | --- | --- | --- |
| 1 | **HTTP OTA** (`POST /api/ota/*`, URL update) | large | The human confirmed it is not a hard requirement. Keeps `espota` if it fits. |
| 2 | **MQTT + HA discovery** (`svc::mqtt`, all discovery payloads) | large | ~900 lines of C++ today. Only affects the HA integration, not machine operation. |
| 3 | **Telnet log server** | medium | Serial logging over USB remains. ADR-0002 exists because of it. |
| 4 | **DS18B20 support** (`temp-ds18b20` feature) | small | TSIC-306 is the default and the wired sensor. One feature flag. |
| 5 | **Pressure sensor** (`sensors-pressure` feature) | small | Off by default today (`hardware.sensors.pressure.enabled` = false). |
| 6 | **Water-tank sensor** (`sensors-watertank` feature) | small | Off by default today. **Note:** dropping it removes an interlock — see S4. |
| 7 | **BLE** (if ever enabled) | large | Already dead code. Dropped at R2-07. |
| — | **NEVER**: emergency stop, water-tank interlock, valve fail-safe, watchdog, actuator ownership, the 18-state machine, the heater output path. | — | Safety. Excluded by policy, not by budget. |

Anything dropped goes into `intentional-diffs.md` (created at R1-08) and the release notes.

---

## 4. The measurement

`just size` must report, and the report is committed at every gate:

```
target:      xtensa-esp32-espidf
image:       target/xtensa-esp32-espidf/release/firmware      1,9xx,xxx B
app slot:    rust/partitions_4M.csv app0                      2,037,xxx B
headroom:                                          + 1xx,xxx B (x.x %)
baseline:    docs/rust-migration/size-baseline.json   (previous gate)
delta:                                       +  xxx,xxx B   <-- attributed below
```

Attribution is by **crate** and by **largest symbol**, so an increase is never
unexplained:

- `cargo bloat --release -p cc-firmware --target xtensa-esp32-espidf` → top 25 symbols
  by size, grouped by crate.
- `xtensa-esp32-elf-size -A` on the `.map` file → per-section totals (`.text`, `.rodata`,
  `.data`), which separates **code** growth from **embedded-asset** growth.
- Feature-flag matrix: build once per optional feature and record the delta. This is the
  only way to price a drop from §3 before you actually drop it.

`size-baseline.json` lives in the repo and is updated by `just size-record mcu=esp32
label=gate-2`. A phase that grows the image by > 10 % without a recorded justification
fails `just size-check`.

---

## 5. Gate criteria (added to Phases 1-4)

At **every** phase gate, in addition to that phase's own criteria:

- [ ] `just size` output pasted into the gate record, with the delta attributed per crate.
- [ ] `just size-check` passes (image fits the slot, delta < 10 % or justified).
- [ ] The measured LittleFS / embedded-SPA size is recorded (R0-02 and every later phase).
- [ ] If any feature from §3 was dropped to fit, it is in `intentional-diffs.md` and the
      release notes.
- [ ] Static RAM (`data` + `bss` from the same `.map`) is recorded — ADR-0002's 30 KB
      heap-shed threshold depends on it.

Phase 1 measures the **minimal** image (blink-equivalent). That number is the reference
every later phase is compared against, and it is the one that tells us whether the whole
migration fits at all.

---

## 6. Size report template

Copy into the phase-gate record:

```markdown
### Image size — <phase> (<date>)

| | Bytes | Δ vs previous |
|---|---:|---:|
| image (firmware) | | |
| app slot | | (0 after rebalance) |
| headroom | | |
| .text | | |
| .rodata (incl. embedded SPA) | | |
| .data + .bss | | |
| embedded SPA | | |
| LittleFS image | | |

Attribution of Δ:
- <crate/symbol>: +N B — <why>
- <crate/symbol>: +N B — <why>

Verdict: fits / does not fit. Action: <none | drop §3 item N>.
```

---

## 6b. Recorded: image size — Phase 1 minimal image (2026-09-28, R1-01)

```
target:      xtensa-esp32-espidf (esp32), ESP-IDF v5.5.5, release + lto=fat, strip=symbols
image:       target/xtensa-esp32-espidf/release/firmware  ->  382,528 B flashable app image
app slot:    rust/partitions_4M.csv app0  0x1C0000  ->  1,835,008 B
headroom:                                       +  1,452,480 B (79.15 %)
baseline:    docs/rust-migration/size-baseline.json  (label r1-01-minimal)
delta:                                        n/a — this is the first datapoint
```

| Section | Bytes | Where it goes |
| --- | ---: | --- |
| `.flash.text` | 251,380 | code: `std` + `esp-idf-svc` + the firmware |
| `.flash.rodata` | 71,264 | strings, vtables, panic metadata |
| `.flash.appdesc` | 256 | `esp_app_desc` |
| `.iram0.vectors` + `.iram0.text` | 47,451 | code that must run from IRAM (ISR vectors, `__init` paths) |
| `.dram0.data` | 11,492 | static RAM, initialised |
| `.dram0.bss` | 2,400 | static RAM, zeroed |
| **Static RAM total** | **61,343** | ADR-0002's 30 KB heap-shed threshold is measured against this |
| ELF total (incl. `.comment`, `.xtensa.info`) | 384,530 | the flashable image is smaller after segment padding |

**What this number means.** 382,528 B for a blink-equivalent is dominated by ESP-IDF
itself (the default component set: wifi, lwip, mbedtls, fatfs, spiffs, mqtt, nvs,
http_server, …) — *not* by anything in `cc-*`. `cargo bloat` cannot attribute it
(`Error: parsing failed cause 'symbols section is missing'`, because the release
profile strips symbols), so the per-crate attribution method in §4 has to come from
the final link map
(`target/<triple>/release/build/esp-idf-sys-*/out/build/libespidf.map`) until that
is automated.

**The oracle filled a 1,835,008 B slot; we use 382,528 B.** Either the oracle
embedded a large asset (a React SPA is 300 KB–1 MB built, so this is the likely
explanation — see §2), or it linked far more of ESP-IDF than the default component
set. Until R2-03 measures the embedded SPA, assume **the SPA is the dominant term**
and size the partition table around it, not around this number.

**Still unknown (07 §7, unchanged by R1-01):** the embedded SPA size, the price of
each §3 drop, and whether `rust/partitions_4M.csv`'s 0x60000 filesystem is enough if
the SPA stays on LittleFS.

## 7. Open items

| Item | Owner | Notes |
| --- | --- | --- |
| The exact rebalanced `app0`/`app1`/`spiffs` split | R0-02 | Apply at R2-03, into `rust/partitions_4M.csv` — **not** the root `partitions_4M.csv`, which stays C++-owned until R4-10 |
| The embedded SPA size | R0-02 | Drives whether `spiffs` can shrink to 64 KB |
| The price of each §3 drop | R2-09b | Feature-matrix build, one run per candidate |
| `cargo bloat` availability on macOS arm64 | R1-01 | If unavailable, use `.map` + `xtensa-esp32-elf-size` only |

---

## 8. Measured growth at R3, and what it was

The first real content milestone — NVS, Wi-Fi, UART provisioning, MQTT, HTTP+SSE —
moved the image from **382,528 B** to **1,349,344 B** (+966,816 B, +252 %).
`just size-check` correctly failed it (>10 % growth). The 252 % is not mostly *our*
code. Measured with `just diag-build` + `nm --size-sort` (the release ELF is stripped,
so symbols come from the `diagnostic` profile, which has identical codegen):

| bucket | bytes | symbols |
| --- | ---: | ---: |
| `std::backtrace` + `addr2line` | 108,382 | 113 |
| `gimli` / `object_rs` DWARF reader | 101,147 | 147 |
| `serde` (derive + json) | 84,264 | 167 |
| mbedTLS / TLS | 82,916 | 606 |
| `core::fmt` | 25,576 | 236 |
| `printf` family | 23,952 | 16 |
| http parser | 9,253 | 7 |
| **all symbols** | **1,124,321** | |

Per-section at that point: `.flash.text` 1,019,528 B, `.flash.rodata` 215,212 B,
static RAM 133,128 B.

### What is avoidable

1. **`std::backtrace` + `gimli` ≈ 210 KB — the single biggest item.** This is *panic
   backtrace symbolization*: it links a DWARF reader and the `addr2line` machinery so a
   panic can print symbol names. It is **useless on the device** — there is no ELF on
   flash, so it can only ever produce addresses. Decoding happens host-side with
   `just diag-addr2line` against the saved core dump. **This should be removed.**
2. **mbedTLS ≈ 83 KB** is linked but the MQTT client is plain TCP and the HTTP server is
   plain HTTP. Verify whether anything actually needs TLS; if not, drop the component.
3. `serde` at 84 KB is load-bearing (the config blob) — keep.
4. Static RAM **133,128 B** is 42 % of the ESP32's 320 KB and has roughly doubled from
   62 KB. ADR-0002's 30 KB heap-shed threshold was tuned against ~75 KB of static use, so
   the margin is now much thinner. **This is a bigger risk than the flash number.**

### Decision

Re-baseline the `r1-01-minimal` point to a **post-R3 record** and open a new task to
remove the backtrace machinery before the image is accepted. Growing 252 % and then
moving the goalposts is how a 1.8 MB slot gets exhausted silently, so the removal is
tracked as a task rather than absorbed.

**Flash is not the current binding constraint — static RAM is.**
