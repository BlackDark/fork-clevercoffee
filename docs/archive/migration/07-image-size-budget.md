# Image Size Budget

> **ARCHIVED — non-normative. Dated 2026-09/10, preserved for provenance.**
> **Measured numbers through §14 are from 2026-09 and have moved. §15 is the
> 2026-10-08 re-baseline.**
> The policy it argues for is still enforced, and enforced in code, by
> `just/size.just` reading [`size-baseline.json`](../../history/size-baseline.json)
> and appending to [`size-records.jsonl`](../../history/size-records.jsonl)
> — both live, both still in `docs/history/`. The current image size and
> its headroom are in [`docs/status.md`](../../status.md). See
> [`docs/archive/README.md`](../README.md).

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
| A Rust esp-idf image with `std`, measured | **≥ 1,835,008 B — it filled a 1,835,008 B slot** | [08 §2](../../history/recovered-oracle.md): the recovered firmware's app0 image occupied its slot to the last non-`0xFF` byte |
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
baseline:    docs/history/size-baseline.json   (previous gate)
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
baseline:    docs/history/size-baseline.json  (label r1-01-minimal)
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

> **Resolved 2026-09-29 — see §9–§12.** Item 1 is done (−162,656 B, see §9).
> Item 2 is investigated and the answer is *something does need TLS*, namely
> `esp-mqtt` and the Wi-Fi supplicant, neither of which is our code (§11). Item 3
> stands. Item 4 stands and got relatively worse, because the flash win did
> almost nothing for RAM (§12). The table above is kept as the *before* column of
> §9.4 rather than deleted, so the two are comparable.

### What is avoidable

1. **`std::backtrace` + `gimli` ≈ 210 KB — the single biggest item.** This is *panic
   backtrace symbolization*: it links a DWARF reader and the `addr2line` machinery so a
   panic can print symbol names. It is **useless on the device** — the app image
   carries only its 256-byte descriptor, not a symbol table, so it can only ever
   produce addresses. Decoding happens host-side with `just diag-addr2line` against
   the saved core dump. **Removed; see §9.**
2. **mbedTLS ≈ 83 KB** is linked but the MQTT client is plain TCP and the HTTP server is
   plain HTTP. Verify whether anything actually needs TLS; if not, drop the component.
   **Answered in §11: something does, and it is not our code.**
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

---

## 9. The backtrace removal (2026-09-29) — done, and what it bought

### 9.1 What actually pulled it in

Not `esp-idf-svc`'s `panic_handler`, and not `esp-idf-hal`'s. Both of those are
gated `#![cfg(all(not(feature = "std"), feature = "panic_handler"))]`
(`esp-idf-sys-0.38.1/src/panic.rs:1`) and neither is enabled on this build
anyway. The mechanism is **`std` itself**:

- `std::panicking::default_hook` unconditionally calls
  `crate::sys::backtrace::lock().print(...)` (`library/std/src/panicking.rs:260`),
  and `default_hook` is reachable from every `panic!` through
  `rust_panic_with_hook`. There is no way to keep `std` and lose that call.
- `std::sys::backtrace::_print_fmt` then calls
  `backtrace_rs::resolve_frame_unsynchronized` (`library/std/src/sys/backtrace.rs:105`),
  and `backtrace_rs` is the vendored `backtrace` crate, `#[path]`-included at
  `library/std/src/lib.rs:717`. Its `symbolize` module pulls in `addr2line`,
  `object` and `miniz_oxide` — that is the `gimli` DWARF reader.
- Cargo's `-Zbuild-std` turns std's `backtrace` feature on by default, which is
  what makes those three optional dependencies exist. Confirmed from the build
  fingerprint (`target/<triple>/release/.fingerprint/std-*/lib-std.json`):
  `features: ["addr2line", "backtrace", "miniz_oxide", "object", "panic-unwind"]`.

### 9.2 The fix — one line, and it is upstream's

`std` has had a feature for exactly this since the symbolisation code was split
out, documented in `library/std/Cargo.toml:111` as *"Disable symbolization in
backtraces. For use with `-Zbuild-std`"*. It is enabled from `.cargo/config.toml`:

```toml
[unstable]
build-std-features = ["std/backtrace-trace-only"]
```

`-Zbuild-std-features` **replaces** the sysroot feature set rather than adding to
it (verified in the fingerprint: the resulting `std` is built with
`features: ["backtrace-trace-only"]`, and `addr2line`/`object`/`miniz_oxide` are
gone). `_print_fmt` then takes its `cfg!(feature = "backtrace-trace-only")`
branch and prints each frame's raw PC instead of resolving it.

No profile was touched. `opt-level`, `lto = "fat"`, `codegen-units = 1`,
`panic = "abort"` and `overflow-checks` are unchanged, and `just size` still
measures the release profile.

### 9.3 It is really gone — symbol evidence, not a size delta

`just diag-build` then, on the unstripped diagnostic ELF:

```
$ xtensa-esp32-elf-nm -S firmware | grep -ciE 'gimli|addr2line|object_rs|miniz'
0
$ xtensa-esp32-elf-nm -S firmware | grep -c backtrace
10
```

Ten `backtrace` matches remain, and all ten are accounted for: four are
`std::sys::backtrace::{BacktraceLock::drop_glue, __rust_end_short_backtrace ×2, lock::LOCK}`
(15 + 13 + 11 + 4 bytes), and six are ESP-IDF's own C backtrace printer
(`esp_backtrace_print`, `esp_backtrace_print_from_frame`, `esp_backtrace_get_start`,
`esp_backtrace_get_next_frame`, `esp_crosscore_int_send_print_backtrace`,
`panic_print_backtrace`) — which is the thing that has to stay. The link map has
zero occurrences of `gimli`, `addr2line` or `miniz_oxide`.

Before, for contrast, the same `nm` on the same target found 148 `gimli`/
`addr2line`/`object`/`miniz` symbols.

### 9.4 Before and after, by bucket

`scripts/size-buckets.py` (new) is the automated form of the `nm --size-sort`
method this section used. Both columns are the same recipe, same profile, same
method, on the same target.

| bucket | before (B) | after (B) | Δ | before (syms) | after (syms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `std::backtrace` + `addr2line` | 108,382 | **442** | −107,940 | 113 | 9 |
| `gimli` / `object` / `miniz_oxide` | 29,827 | **0** | −29,827 | 66 | 0 |
| mbedTLS / TLS (incl. `wpa_supplicant`) | 94,545 | 94,545 | 0 | 680 | 680 |
| `serde` (derive + `serde_json`) | 83,235 | 83,235 | 0 | 118 | 118 |
| `core::fmt` | 24,800 | 19,944 | −4,856 | 230 | 217 |
| `printf` family | 24,087 | 24,087 | 0 | 18 | 18 |
| http parser | 9,191 | 9,191 | 0 | 6 | 6 |
| lwIP | 14,845 | 14,836 | −9 | 111 | 111 |
| **all symbols** | **1,124,373** | **980,416** | **−143,957** | 7,464 | 7,184 |

Note the `gimli` row is 29,827 B here and was recorded as 101,147 B in §8. Same
symbols, narrower pattern: the pattern that produced §8's number also matched
`object_rs`' internals under names this script attributes elsewhere. The
authoritative figure is the **all symbols** delta, −143,957 B, and the image
delta, which agree.

### 9.5 Image and RAM

| | before | after | Δ |
| --- | ---: | ---: | ---: |
| flashable app image | 1,349,424 B | **1,186,768 B** | **−162,656 B** (−12.05 %) |
| headroom in the 1,835,008 B slot | 485,584 B (26.46 %) | **648,240 B (35.33 %)** | +162,656 B |
| `.flash.text` | 1,019,604 B | 872,644 B | −146,960 B |
| `.flash.rodata` | 215,212 B | 200,956 B | −14,256 B |
| static RAM (`iram0` + `dram0.data` + `dram0.bss`) | 133,128 B | **131,688 B** | **−1,440 B** |
| — of which `.iram0.vectors` | 1,028 B | 1,028 B | 0 |
| — of which `.iram0.text` | 94,155 B | 94,155 B | 0 |
| — of which `.dram0.data` | 19,024 B | 17,584 B | −1,440 B |
| — of which `.dram0.bss` | 18,920 B | 18,920 B | 0 |

`just size-check` now passes: −12.05 % against the `r3-storage-network`
baseline, inside the ±10 % band in the other direction.

**The RAM win is small, and it should have been bigger.** The backtrace machinery
held no large static buffer — the frame arrays are on the panic path's stack, not
in `.bss`. The 1,440 B that did go is the three tables behind
`std::sys::backtrace::{output_filename, set_image_base, _print_fmt}`'s dead half.
The claim in §8 item 1's framing that a backtrace buffer would be a large static
RAM consumer is **not borne out**: 1.1 % of static RAM, not 10 %.

---

## 10. A panic is still diagnosable — the evidence

The requirement is that removing the symboliser must not cost the ability to
diagnose a crash. It does not, and nothing about the *shape* of the diagnosis
changed; only where the decoding happens, and that was already host-side.

What the device still does, in the order an operator meets it:

1. **ESP-IDF's panic handler.** `esp_panic_handler`, `esp_panic_handler_disable_timg_wdts`,
   `esp_panic_handler_feed_wdts`, `esp_panic_handler_increment_entry_count` and
   `esp_backtrace_print*` are all in the image (verified with `nm`). The chip
   prints `Guru Meditation Error`, the faulting PC, `MEIP`/`MEPS` and the register
   dump, then reboots. That path is C, in `esp_system`/`esp_hw_support`, and is
   untouched by this change.
2. **`std`'s panic hook.** `std::panicking::default_hook` (624 B) and
   `std::sys::backtrace::BacktraceLock` are still linked, so the Rust panic
   message, the source location and the `stack backtrace:` frame list are still
   printed. What changed is that each frame is a raw PC (`0: 0x4008f2a4`) instead
   of a symbol — which, on a target with no ELF in flash, is **all it could ever
   have printed usefully anyway**.
3. **The next boot's reset reason.** `rst:0x…` on the following power-on tells you
   whether the chip died from a panic, an interrupt WDT, a task WDT or a brownout.
4. **Host-side resolution.** `just diag-addr2line 0x4008f2a4 …` against the
   `diagnostic` ELF names the frames. This is the same step that already existed
   for the addresses the C++ firmware's `Panic` handler prints.

What was **not** lost, and is worth being explicit about: with the symboliser
gone, a panic on the device no longer prints a function name on the wire. It
never could have printed a *line number* or a *file* (no DWARF on the device), and
before this change the symbol it would have printed came from the same
`just diag-addr2line` lookup, one step later. The trade is a slightly longer
host-side step in exchange for 162 KB of flash.

### 10.1 Measured, on the board

A temporary `panic!("TEMP panic-probe: backtrace diagnosis check")` was placed on
a spawned task in `bring_up` (8 s after boot, three frames deep), built with the
**release** profile, flashed, and the console captured. The probe is **not** in the
shipped image; §9.5's numbers are all from the image without it. Verbatim, trimmed:

```
I (681) firmware: water valve driven inactive
I (681) firmware: pump driven inactive
I (681) firmware: heater driven inactive
I (741) firmware: pin readback OK: heater=GPIO2 (10 ms GPTimer ISR, disarmed) \
       valve=GPIO17 pump=GPIO27 all inactive
I (4121) firmware: control heartbeat 1 — watchdog fed, duty 0 of 1, gate true, ...
I (8751) firmware: TEMP panic-probe firing

thread '<unnamed>' (3) panicked at crates/cc-firmware/src/main.rs:437:17:
TEMP panic-probe: backtrace diagnosis check

abort() was called at PC 0x401099f6 on core 0

Backtrace: 0x40088ccd:0x3ffc0dc0 0x40088c95:0x3ffc0de0 0x4009016e:0x3ffc0e00
 0x401099f6:0x3ffc0e70 0x401086ca:0x3ffc0e90 0x4010acba:0x3ffc0eb0
 0x40106c8e:0x3ffc0ed0 0x401086d6:0x3ffc0ef0 0x401083c6:0x3ffc0f10
 0x4010aa8b:0x3ffc0f80 0x4010aa40:0x3ffc0fb0 0x4010acd3:0x3ffc0fd0
 0x400f985d:0x3ffc1000 0x400eb71e:0x3ffc1040 0x400e406f:0x3ffc1060
 0x400d49a7:0x3ffc10a0 0x400e6890:0x3ffc10d0 0x4010aec1:0x3ffc10f0
 0x40111c40:0x3ffc1120

ELF file SHA256: 9ed674562

Rebooting...
rst:0xc (SW_CPU_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)
```

Read that against §10's four requirements:

| requirement | where it is in the transcript |
| --- | --- |
| actuators inactive | `pin readback OK: … all inactive` at t=741 ms, and every heartbeat to the panic reports `duty 0 of 1` |
| the faulting PC | `abort() was called at PC 0x401099f6 on core 0`, and frame 4 of the backtrace is the same address |
| registers | ESP-IDF's `Guru Meditation` register dump path is what `esp_backtrace_print` drives; the PC/SP pairs are on the wire |
| reset reason | the next boot prints `rst:0xc (SW_CPU_RESET)` |
| the frames resolve | `xtensa-esp32-elf-addr2line -pfiaC` on the diagnostic ELF — this is `just diag-addr2line`, unchanged |

`abort() was called at PC 0x401099f6` resolves to
`<std::io::default_write_fmt::Adapter<Stderr> as core::fmt::Write>::write_str` —
which is the write the panic hook is in the middle of, so the abort lands in the
middle of printing the message. Two frames further down,
`0x400e406f` resolves to `firmware::bring_up::{closure#0} at
crates/cc-firmware/src/main.rs:433`, which is the probe task's own closure, and
`0x400e6890` to `firmware::bring_up` / `firmware::main` — so the whole chain from
the spawned task up through `bring_up` and `main`'s thread is recovered, from raw
addresses, with no symbolisation on the device at all.

The `ELF file SHA256: 9ed674562` line is worth being precise about, because it
looks like evidence that there *is* an ELF in flash. It is not: that line is
`esp_elf_find_description()` reading the 256-byte `esp_app_desc_t` in
`.flash.appdesc` and printing the image's identity, so a coredump or a gdbstub
session can tell *which* build crashed. The symbol table is in the separate `.elf`
and is not in the app image, so the device could never have symbolised a frame
from it. The SHA is a version tag, not a symbol source.

Two findings from building that probe, both worth keeping:

- A `panic!` reached **inline** from `bring_up` is not usable as a probe. The
  optimiser proves the call never returns, so everything behind it in `bring_up`
  becomes unreachable and `lto = "fat"` drops the whole R3 bring-up: the image
  fell to **220,768 B**. `black_box` and a wide modulus do not help — the
  modulus alone bounds the depth, which is all the unroller needs. A **spawned
  task** has neither problem, because the spawn returns.
- A probe at the *end* of `bring_up` is also wrong: the last statement is
  `control.join()`, which does not return while the control task is alive, so
  the probe never ran and the chip simply looped the heartbeat forever.

### 10.2 The coredump partition is present but nothing writes to it

`rust/partitions_4M.csv` reserves `coredump, data, coredump, 0x3F0000, 0x10000`,
and the plan keeps it. But the Rust build's `sdkconfig` has
`CONFIG_ESP_COREDUMP_ENABLE_TO_NONE=y`, so **no coredump is captured** — the
partition is allocated and never written. This is pre-existing and unchanged by
this work; §8's framing ("a coredump must still be captured") described an
intent the build does not currently meet.

Turning it on is a one-line change (`CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH=y`,
via `[[package.metadata.esp-idf-sys]] esp_idf_sdkconfig_defaults`, which today
only sets `CONFIG_COMPILER_OPTIMIZATION_SIZE`). It is **deliberately not done
here**: it changes crash-time behaviour of the shipped image and costs flash, and
that is a decision for the phase gate, not a side effect of a size reduction.
Flagged as an open item in §7.

---

## 11. mbedTLS — investigated, and it is not removable

§8 item 2 asked whether anything needs TLS. Answer: **yes, and it is not our
code.** 94,545 B of mapped symbols, 680 of them, in two ESP-IDF C components.

### 11.1 Root cause 1 — `mqtt` → `tcp_transport` → `esp-tls` → `mbedtls`

`esp-tls` hard-requires mbedtls (`components/esp-tls/CMakeLists.txt:22`:
`REQUIRES mbedtls`). It is reached from the MQTT client, which requires
`tcp_transport` (`components/mqtt/esp-mqtt/CMakeLists.txt:11`). The link map names
the reference exactly:

```
libesp-tls.a(esp_tls.c.obj)  <-  libmqtt.a(mqtt_client.c.obj)  (esp_tls_get_and_clear_last_error)
libesp-tls.a(esp_tls_crypto.c.obj)  <-  libtcp_transport.a(transport_ws.c.obj)  (esp_crypto_sha1)
```

The MQTT client calls `esp_tls_get_and_clear_last_error()` unconditionally — even
for a plain-TCP broker — to clear a stale error, so `esp-tls` cannot be dropped
while `mqtt` is in the build. And `tcp_transport`'s WebSocket handshake needs
`esp_crypto_sha1` from the same library. Both are ESP-IDF C; removing them means
patching ESP-IDF, not configuring the Rust build.

### 11.2 Root cause 2 — `esp_wifi` → `wpa_supplicant` → mbedTLS, for WPA3 and Enterprise

This is the "some IDF Wi-Fi builds link mbedTLS for EAP/SAE" case, and it is
present. `esp_wifi` always pulls `wpa_supplicant`, and 77 of the 81 mbedTLS archive
members come in through it:

```
libwpa_supplicant.a(esp_eap_client.c.obj)  (tls_init)                  -> libmbedtls
libwpa_supplicant.a(tls_mbedtls.c.obj)      (mbedtls_ssl_tls_prf)       -> libmbedtls
libwpa_supplicant.a(esp_owe.c.obj)          (crypto_ecdh_deinit)        -> libmbedcrypto
libwpa_supplicant.a(crypto_mbedtls-ec.c.obj)(crypto_ecdh_gen_public)    -> libmbedcrypto
```

The `sdkconfig` switches that turn this on, and what each costs:

| `sdkconfig` symbol | effect | what it buys |
| --- | --- | --- |
| `CONFIG_ESP_WIFI_MBEDTLS_CRYPTO=y` | `select`s `MBEDTLS_{AES,ECP,ECDH,ECDSA,CMAC}_C` (`esp_wifi/Kconfig:582`) | the supplicant's ECC/AES. **Do not disable** — its own help text says the supplicant's internal crypto "may not suffice … including WPA3". |
| `CONFIG_ESP_WIFI_MBEDTLS_TLS_CLIENT=y` | `select MBEDTLS_TLS_ENABLED` (`esp_wifi/Kconfig:601`), `depends on ESP_WIFI_ENTERPRISE_SUPPORT` | the whole libmbedtls + libmbedx509 TLS stack, for WPA2-Enterprise |
| `CONFIG_ESP_WIFI_ENTERPRISE_SUPPORT=y` | the dependency above | EAP |
| `CONFIG_ESP_WIFI_ENABLE_WPA3_SAE=y`, `..._SAE_PK`, `..._SAE_H2E`, `..._WPA3_OWE_STA` | `sae.c`, `esp_owe.c` in wpa_supplicant | WPA3 |

So the plain-MQTT / plain-HTTP observation in §8 is correct about *our* code and
irrelevant to the linker: neither our MQTT client nor our HTTP server asks for
TLS, and the TLS still gets linked, by the Wi-Fi supplicant and by esp-mqtt's
unconditional error call.

### 11.3 What could be cut, and why it was not

`CONFIG_ESP_WIFI_ENTERPRISE_SUPPORT=n` and `CONFIG_ESP_WIFI_ENABLE_WPA3_{SAE,OWE_STA}=n`
would remove most of `libmbedtls` and the `wpa_supplicant` EC code. **Not done
here**, because it changes *which access points the machine can join in the field*,
and the plan's Wi-Fi provisioning being plain says nothing about the network a
deployed machine is pointed at. That is a product decision for the phase gate, not
a size optimisation to be taken silently. It is recorded in §7.

Also checked and **not** a cost: `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` is `y` with
`DEFAULT_FULL`, which looks like a 30–40 KB liability, but nothing references it —
`strings -n 40 firmware | grep -c 'BEGIN CERTIFICATE'` is **0** and the only bundle
symbol in the image is a 14-byte `esp_transport_ssl_crt_bundle_attach` stub.

---

## 12. Static RAM — the bigger problem, measured

§8 item 4 called this "a bigger risk than the flash number". It is, and the
backtrace removal did almost nothing for it: **131,688 B**, down only 1,440 B.

Per section, from `just size`:

| section | bytes | share of the ESP32's 320 KB |
| --- | ---: | ---: |
| `.iram0.vectors` | 1,028 | 0.3 % |
| `.iram0.text` | 94,155 | 29.4 % |
| `.dram0.data` | 17,584 | 5.5 % |
| `.dram0.bss` | 18,920 | 5.9 % |
| **total static** | **131,688** | **41.1 %** |

### 12.1 The dominant term is IRAM, and it is the Wi-Fi MAC

**94,155 B of the 131,688 B is instruction RAM, and it is dominated by the
prebuilt Wi-Fi MAC.** The link map's IRAM input sections, by named-symbol size:

| input section | bytes | archive |
| --- | ---: | --- |
| `.wifi.slp.rx.iram` | 7,825 | `libnet80211.a` / `libpp.a` |
| `.wifi.extra.iram` | 5,842 | `libnet80211.a` |
| `.wifi.rx.iram` | 4,973 | `libnet80211.a` |
| `.wifi.slp.iram` | 1,742 | `libnet80211.a` |
| `.iram1` (everything else) | 1,292 | various |
| `.wifi.orp.slp.iram` | 47 | `libpp.a` |

Those account for 21,739 B of *named* symbols; the remaining ~72 KB of
`.iram0.text` is unnamed padding and literal pools inside the same Wi-Fi
archives, which is why the per-symbol total (21,739 B) is far below the section
size (94,155 B). Every one of the 12 largest IRAM objects is from `libpp.a` or
`libnet80211.a` — `pm.o` 6,117 B, `pp.o` 3,784 B, `wdev.o` 3,207 B,
`ieee80211_sta.o` 1,966 B.

`CONFIG_ESP_WIFI_IRAM_OPT=y` and `CONFIG_ESP_WIFI_RX_IRAM_OPT=y` put the whole
receive path in IRAM. That is a fixed cost of `esp_wifi` on this chip and **no
Rust-side change touches it** short of not using Wi-Fi, which the machine needs.
`.iram0.text` was byte-identical before and after this work.

### 12.2 The largest named `.data` / `.bss` consumers

| bytes | symbol | component | note |
| ---: | --- | --- | --- |
| 10,416 | `core::num::dec2flt::table::POWER_OF_FIVE_128` | `core` | **7.9 % of all static RAM, for one lookup table** |
| 3,880 | `g_cnxMgr` | lwIP | connection pool |
| 3,496 | `serde_json::{de::POW10, read::HEX0, read::HEX1}` | `serde_json` | float + `\u` parsing tables |
| 3,072 | `port_IntStack` | FreeRTOS | the ISR stack |
| 1,816 | `esp_err_msg_table` | `esp_system` | |
| 1,296 | `core::flt2dec::grisu::CACHED_POW10` | `core` | float formatting |
| 1,308 + 5,004 | `s_wifi_nvs`, `gWpaSm`, `g_ic`, `gChmCxt`, `s_dp`, `g_pm`, `gScanStruct`, `destination_cache` | `wpa_supplicant` | **6,312 B, a direct consequence of §11.2** |
| 1,184 | `dns_table` | lwIP | |
| 1,024 | `d_mult_table` | mbedTLS | |
| 1,008 | `rtc_io_desc` | `hal` | |
| 972 | `TxRxCxt` | `esp_driver_uart` | console driver |
| 944 + 744 + 384 | `ciphersuite_definitions`, `ciphersuite_preference`, `mbedtls_cipher_definitions` | mbedTLS | 2,072 B of cipher tables |
| 860 | `soc_memory_regions` | `soc` | |
| 767 | `core::unicode::grapheme_extend::OFFSETS` | `core` | |
| 564 | `HAL_ISR_REACTOR` | `esp-idf-hal` | |

### 12.3 The one that is worth a task

`POWER_OF_FIVE_128` is a `static` (not `const`) array, so rustc emits it into
**`.dram0.data` — 10,416 B copied from flash into DRAM at boot** — for a
`f64::from_str` that the configuration blob genuinely needs
(`crates/cc-firmware/src/main.rs:315` documents `f64::from_str` as the leaf of
every float parameter, and `cc-web/src/request.rs` (`parse_setpoint`) parses a float from the
setpoint endpoint). It is load-bearing *while the config schema has float
parameters*; it is pure overhead the day the schema is all integers.

The 6,312 B of `wpa_supplicant` statics and the 2,072 B of mbedTLS cipher tables
are the RAM face of §11.2 — the same decision, priced in DRAM instead of flash.

Neither is addressed here. Both are recorded in §7 as follow-up work, and both
belong to a task that is allowed to change the config schema or the Wi-Fi
feature set, which this one is not.

---

## 13. Open items after §9–§12

| Item | Owner | Notes |
| --- | --- | --- |
| Enable `CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH` (the `coredump` partition is allocated and unused) | next phase gate | §10.2. One line in `esp_idf_sdkconfig_defaults`; costs flash and changes crash-time behaviour, so it is a gate decision |
| Decide whether WPA3-SAE / WPA2-Enterprise are worth ~94 KB of flash and ~8 KB of RAM | next phase gate | §11.3. The single largest remaining flash item, and the only one that is a *product* decision rather than dead weight |
| Move `POWER_OF_FIVE_128` out of `.dram0.data` (10,416 B) | R4 config work | §12.3. Either the config schema stops having float parameters, or the table is relocated to flash |
| `scripts/size-buckets.py` into the gate | R1-09 follow-up | Written for this work; §9.4 is the first thing it produced. It is not yet wired into `just size` |
| Whether `.iram0.text`'s 94,155 B can be reduced at all | — | §12.1. Almost certainly not without dropping Wi-Fi. Recorded so the next reader does not re-derive it |

### 13.1 How the numbers in §9 and §10 were taken, for the next reader

Two things about this measurement that cost time and are worth not rediscovering.

**A panic probe cannot be a `panic!` statement in the function you are probing.**
In `bring_up` it made the optimiser prove the call diverges, `lto = "fat"` then
marked every line behind it unreachable, and the whole R3 bring-up left the image:
1,186,768 B → 220,768 B. A **spawned task** does not have this problem, because the
spawn returns and the task body is not on `bring_up`'s control-flow path.

**The size numbers in §9.5 are from an image with no probe in it.** The probe build
measured 1,187,984 B; the shipped one is 1,186,768 B. §9.5's per-section table is
the shipped image's.

The whole of §9–§13 was built and measured in a `git worktree` at `ad0f81e` with
only `.cargo/config.toml` applied, because another session had uncompilable work in
flight in the shared tree. That worktree reproduced the shared tree's measurements
byte for byte (`.iram0.text` 94,155, `.dram0.data` 17,584, `.dram0.bss` 18,920,
image 1,186,768 B), which is the cross-check that the numbers are not contaminated
by whatever else is changing. `just size-record` also has to be run from a tree
whose `target/xtensa-esp32-espidf/release/firmware` is the build you are claiming:
run against a stale ELF it silently records that ELF's size, which is how a
1,197,712 B record for this work got written and then had to be taken back out.

---

## 11. R3-09's cost: the display, and why it is 64 KB

Bringing the OLED up (R3-09) plus wiring the reducer into the control loop
(R4-01) moved the image from 1,216,816 B to **1,338,816 B** (+122,000 B, +10.0 %).
`just size-check` failed it at exactly the 10 % limit, which is the gate working.

Attributed with `just diag-build` + `nm --size-sort`, the same method as §8:

| bucket | bytes | symbols |
| --- | ---: | ---: |
| `cc-display` (fonts + renderer) | **63,831** | 69 |
| `serde` | 84,264 | 167 |
| mbedTLS / TLS | 75,333 | 542 |
| mbedTLS certificates | 6,420 | 33 |
| backtrace / gimli | 4,765 | 11 |
| ssd1306 + display-interface | 834 | 1 |
| all symbols | 1,109,558 | |

Per-section: `.flash.text` 964,968 B, `.flash.rodata` **259,208 B** (up 43,996 B
from 215,212 B), static RAM **133,168 B** (up 1,480 B).

**The 64 KB is the ten U8G2 fonts, and it is not waste.** 01 §3 F15 rates the
display *High* difficulty precisely because "U8G2 fonts have no Rust
equivalent" — the C++ embeds its own font tables in `bitmaps.h`/`font.h` and
renders from them. The Rust port carries the same tables so the rendering is
pixel-identical, which is what the two-sided oracle against the real U8g2
verifies. A smaller font set would be a smaller image and a **layout regression
the moment a glyph is missing**, so the trade is not available.

**Static RAM barely moved** (+1.5 KB), which is the important number: 94 KB of
the 133 KB is IRAM belonging to the prebuilt Wi-Fi MAC and is untouchable
without dropping Wi-Fi. The display's cost is flash, not RAM, and RAM is the
binding constraint — so the display did not make the real problem worse.

**Nothing was dropped to absorb this.** §3's drop order was not used, and in
particular the scale was not: see `intentional-diffs.md` §11 and the human's
explicit decision to keep it.

Re-baselined as `r4-01-reducer-and-display`.

## 13. The web UI is embedded (2026-09-30) — +220,608 B, and 199,270 B of it is the SPA

### 13.1 The open question, and the number that answered it

The 06 task list left F25's transport open: embed the SPA in the binary, or
serve it from the `littlefs` partition. Nothing had ever built the bundle, so
the decision had no number behind it. Built with Vite 8.2.2
(`pnpm --filter @clevercoffee/frontend build`, `VITE_BASE_PATH=/ui/`):

| file | raw | gzip |
|---|---:|---:|
| `assets/index-DTmvHJP_.js` | 594.20 KB | **182,878 B** |
| `assets/index-B4vm-kEh.css` | 59.97 KB | **11,205 B** |
| `index.html` | 0.46 KB | **303 B** |
| `logo.png` | 4,884 B | — (already compressed) |
| **total `dist/`** | **~715 KB** | **199,270 B** |

That table decides it, because the two budgets are different sizes:

| destination | budget | fits the 199 KB gzip bundle? |
|---|---:|---|
| app0 slot (was) | 496,192 B free | yes, with 296,922 B to spare |
| `littlefs` (0x60000) | 393,216 B | yes |
| app0 slot, **uncompressed** | 496,192 B free | **no** — 715 KB does not fit |
| `littlefs`, uncompressed | 393,216 B | **no** |

**Chosen: embed, via `include_bytes!` in a `build.rs`.** Three reasons, in
order of weight:

1. **The identity form does not fit anywhere.** The `littlefs` partition is
   393,216 B and the raw bundle is ~715 KB. A filesystem could only ever have
   held the gzipped form, so "grow the filesystem" was not on the table without
   a partition rebalance that would take bytes from `app0`/`app1` and so make
   the *code* budget worse. Embedding avoids trading code for assets.
2. **There is no filesystem to fail.** A mounted-LittleFS `/ui` has a failure
   mode the C++ genuinely has: a second flash step (`uploadfs`) that can drift
   from the app image, leaving a firmware whose `/ui` 404s or serves a stale
   bundle. Embedded, the UI on the device is by construction the bundle in the
   tree.
3. **It costs zero RAM.** The bytes are `&'static [u8]` in `.flash.rodata` and
   the handler streams them through the httpd send buffer. Static RAM is
   **133,168 B before and after** — the binding constraint on this machine
   (§12) is untouched by a 199 KB feature.

### 13.2 What the growth actually is

| | before | after |
|---|---:|---:|
| image | 1,338,816 B | 1,559,520 B |
| headroom | 496,192 B (27.05 %) | **275,488 B (15.01 %)** |
| static RAM | 133,168 B | **133,168 B** |

`+220,704 B` total, of which **199,270 B (90 %) is the bundle itself**. The
remaining ~21 KB is the handler: the MIME table, `resolve_ui`, `write_ui_file`,
and the `build.rs` module. So this is a feature paid for with the feature's own
bytes, not code growth — which is the distinction §4's per-section split exists
to make, and `.flash.rodata` is where it landed (259,208 → 460,792 B, +201,584 B
against a +199,270 B bundle).

### 13.3 §3's drop order was not used

Nothing was dropped to absorb this. The bundle is not "waste to be trimmed
later" — it is the UI the human needs to test anything at all, and it is
already the minimal form (gzip, hashed, no sourcemaps, no legacy polyfill
target configured).

### 13.4 The ceiling, and what would replace it

The embedded form has one real limit: **only the gzip variant exists**, so it is
sent with `Content-Encoding: gzip` unconditionally and a client that cannot
decode gzip cannot load the UI (`curl` needs `--compressed`; every browser
advertises gzip). If the bundle ever needs an identity fallback — an old
`curl`, a raw HTTP client, an MQTT-driven asset fetcher — that is the trigger to
move to `littlefs`, which can hold both forms, and it should come with a
partition rebalance, not with an app-slot squeeze.

Re-baselined as `f25-web-ui-embedded`.

---

## 14. R3-18's blocker: NimBLE does not fit, and that is a decision not a default

Measured 2026-09-30 by enabling NimBLE and building, changing **nothing else**:

| | before | with NimBLE | delta |
| --- | ---: | ---: | ---: |
| image | 1,559,520 B | **1,764,832 B** | **+205,312 B** |
| flash headroom | +275,488 B (15.0 %) | **+70,176 B (3.8 %)** | −205,312 B |
| `.iram0.text` | 94,155 B | 126,131 B | +31,976 B |
| static RAM | 133,168 B | **173,292 B** | **+40,124 B** |
| `.dram0.data` | 17,584 B | 24,292 B | +6,708 B |
| `.dram0.bss` | 18,928 B | 21,840 B | +2,912 B |

`just size-check` fails it at +13.17 % against a 10 % limit, which is the gate
working.

**The RAM is the worse half.** 173,292 B is **54 % of the ESP32's 320 KB**, and
~95 KB of that is the Wi-Fi MAC's IRAM. The remaining heap is what ADR-0002's
30 KB heap-shed threshold and the display's frame buffer and the HTTP server's
JSON responses all compete for — and ADR-0002 documents a **production OOM abort**
that happened when several API requests overlapped the telnet logger. Cutting
free heap by 40 KB to add a scale that **no Acaia device is paired to** is the
wrong side of that trade.

### Why it is not simply "drop something"

07 §3 has a drop order, and §11 (intentional-diffs) records the human's explicit
instruction that the scales are **kept**. Those two together mean the only
legitimate ways forward are:

1. **The human decides the trade.** 70 KB of headroom is thin but not negative;
   173 KB of RAM is survivable *if* the heap report stays comfortable. This is a
   product judgement about what the machine is for, and it is not mine to take.
2. **Free flash first.** 199,270 B of the image is the embedded web UI and
   63.8 KB is U8G2 fonts. The UI could move to LittleFS (which is what the
   partition is *for*) and would free ~199 KB — enough for NimBLE several times
   over. That is a real option, not a dodge, and it costs no feature.
3. **Shrink the fonts.** Pixel parity is **not** a requirement — the human said so
   explicitly ("if you think some other fonts are better feel free to use them …
   it is just important that we stay readable and in frame"). A smaller glyph
   set is 30-40 KB back with no behavioural cost.

### What is NOT claimed here

The NimBLE stack **initialises and builds**. No scan, no pairing, no weight, and
no Acaia hardware is present, so nothing about the feature is verified. The
measurement is the deliverable of this section; the driver is not.

---

## 15. Re-baselined 2026-10-08 — the web bundle crossed 10 %

`just size-check` failed the growth gate. The slot still fit.

| | |
| --- | ---: |
| image that crossed | 1,720,032 B (+10.29 % vs 1,559,520 B) |
| app0 slot | 1,835,008 B |
| headroom then | +114,976 B |

Already 1,710,208 B (+9.66 %) before this rebuild. The step over 10 % is the web bundle after the dependency update (`1e8bc315`), +9,824 B. Nothing from §3 was dropped. Re-recorded as `ui-bundle-2026-10-08`. The 10 % limit is unchanged. Live figure: [`docs/status.md`](../../status.md).
