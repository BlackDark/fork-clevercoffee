# Architecture

Read time: 4 minutes. Purpose: state the target architecture for the Rust firmware and show why.

**TL;DR**

1. Keep `esp-idf-svc` + `std` on the original ESP32 (Xtensa LX6). It is the only stack built and run on a board.
2. Keep the four-layer split: portable logic, HAL, tasks, firmware. Never let a portable crate name `esp_idf_*`.
3. Drive the heater from a 10 ms timer ISR. Never from LEDC, never from hardware PWM.

## Layers

```text
  cc-firmware      tasks, bring-up, HTTP glue, telemetry seqlock
  cc-hal-esp32     ESP-IDF only: GPIO, relays, I2C, SPI, 1-Wire, NVS, Wi-Fi, httpd, TWDT
  ---------------------------------- unsafe / FFI boundary (unsafe_code = "deny")
  cc-domain        no_std: PID, DS18B20, TSIC-306, HX711, ABP2, state enums
  cc-machine       no_std: state machine as a pure reducer, guards, applier
  cc-safety        no_std: steam/water interlock, S1-S11 safety paths
  cc-config        no_std: 98-parameter schema, blob store, JSON import/export
  cc-display       no_std: framebuffer, templates, U8g2-compatible renderer
```

Rules that hold the split in place:

- `cc-domain`, `cc-machine`, `cc-safety`, `cc-config`, `cc-display` are host-testable and **must not** name `esp_idf_*` (CI grep).
- Only `cc-hal-esp32` and `cc-firmware` are device-only. They are outside `default-members`.
- The control task is the **only** task subscribed to the task watchdog.

## Task and memory model

| Item | Value | Evidence |
|---|---|---|
| Chip | ESP32 rev 3.0, Xtensa LX6, dual core, 4 MB flash, no PSRAM | `design:docs/adr/0004-…` |
| Runtime | FreeRTOS tasks, no async executor | `rewrite:crates/cc-firmware/src/main.rs` |
| Toolchain | `esp` fork via espup, `-Zbuild-std` | `rewrite:rust-toolchain.toml` |
| Target triple | `xtensa-esp32-espidf`, linker `ldproxy` | `rewrite:.cargo/config.toml` |
| Crates | `esp-idf-svc` 0.53.0, `esp-idf-hal` 0.47.0, `esp-idf-sys` 0.38.1, ESP-IDF v5.3.6 | `rewrite:Cargo.toml` |
| Allocator | ESP-IDF heap (tlsf). No Rust global allocator | `rewrite:crates/cc-hal-esp32/src/heap.rs` |
| Logging | `log` routed to UART0 by esp-idf-svc | `rewrite:Cargo.toml` |
| Error handling | `Result` everywhere. `panic = "abort"`; no unwinding on Xtensa | `rewrite:Cargo.toml` |
| Flashing | `espflash` for esp-hal images; the esp-idf path uses `cargo`/`ldproxy` | `design:docs/adr/0004-…`, `rewrite:docs/rust-migration/05-tooling-and-workflows.md` |

Stacks are sized from `--dwarf=frames`, not guessed. `app_main` is 3584 B and needs ~11 KB for bring-up, so bring-up runs on its own 16 KB thread.

## Decision table

| Choice | Why | Evidence | Rejected alternative |
|---|---|---|---|
| `esp-idf-svc` + `std` | HTTP, NVS, OTA, filesystem and TCP/IP already exist and run on the board | `rewrite` 111 device tests pass | `esp-hal` + `embassy` — 99 728 B image, but 600–900 lines of owned glue per subsystem and no compiled board crate (`space2`) |
| FreeRTOS tasks | Proven on device; `esp-idf-hal` is not ISR-safe for `embassy-executor` | `design:docs/adr/0004-…`, `rewrite` | `embassy-executor` — its critical section is a recursive mutex, not ISR-safe |
| 10 ms GPTimer ISR for the heater | Only mechanism that survives the 300 ms interrupt watchdog | `rewrite` commits `42be3578`, `51fa96ca` | LEDC at 1 Hz — panicked on every boot. 100 Hz software carrier — wrong by 100× |
| Fixed `1,835,008 B` app slots | Known-good table from the recovered firmware | `rewrite:rust/partitions_4M.csv`, `rewrite:docs/rust-migration/07-image-size-budget.md` | C++ `partitions_4M.csv` — 1,703,936 B will not hold a Rust image |
| Web UI embedded in the binary | Removes the filesystem-upload step | `rewrite:docs/rust-migration/07-image-size-budget.md` §2 | LittleFS — costs flash the image needs |
| Own framebuffer, not `embedded-graphics` | 42 722 B as RLE vs 177 723 B as `ImageRaw` | `rewrite` link map | `embedded-graphics` `ImageRaw` — 135 KB more on a size-constrained target |
| Bug-for-bug parity rejected | The C++ has blocking safety defects | `rewrite:docs/rust-migration/intentional-diffs.md`, `space2:docs/rust-migration/parity-report.md` | Faithful port — carries 5 broken safety paths |

## What to copy from `rewrite/rust`

- The workspace split and the CI grep that keeps portable crates ESP-free.
- The on-target test runner (`crates/cc-device-tests`) plus `just test-audit`. `cargo test` cannot run on this target.
- The host-only parity crate (`cc-parity`) with no GPIO in its tree, so actuator scenarios are structurally safe.
- `docs/rust-migration/intentional-diffs.md` and its runner that fails on an undeclared divergence.
- `just size`, `just size-check`, and the committed `size-baseline.json` at every gate.
- The telemetry seqlock. A contended mutex taken twice per tick was the first concurrency fix (`2b60de8`).

## Open questions

- **Heater relay polarity** — ❓ undetermined. A meter on the relay coil, boiler disconnected, settles it. Gates the whole application layer.
- **Target scope** — original ESP32 only, or also S3 and C6? The C6 pin budget does not fit (`space2`). The S3-DevKitC-1 v1.0 and v1.1 differ in LED pin.
- **BLE scale** — ⚠️ measured, not built. NimBLE costs +205 312 B flash and +40 124 B static RAM (`2013bda9`). A product call, not a size optimisation.
- **The 12 ms applier span** — ❓ unexplained. 15 of a 15 ms tick budget; split `apply` / `drain_scale` / reboot checks next.
- **HTTP OTA** — ❌ not implemented. Routes answer `501` (`web.rs:1810`).

See [FINDINGS.md](FINDINGS.md) for what works and what does not.
