# Migration Notes

Running log for the C++ → Rust firmware migration. **Update this at the end of every
task and every phase gate.** Keep it factual: what was done, what was observed, what is
blocked.

---

## Current state

| Field | Value |
| --- | --- |
| Current phase | **Not started** — plan delivered, no Rust code written yet |
| Next task | **R0-01** (confirm the physical board) — blocked on hardware |
| Plan reviewed | 2026-09-28 by two adversarial subagents; 24 hard factual errors and 5 blocking tooling defects found and **fixed**. See 06 and 07. |
| ADR-0004 status | **Proposed** (becomes Accepted at Gate 1) |
| C++ baseline | `pio run -e esp32_usb` **succeeds**; `firmware.bin` = 1,546,240 B; `pio test -e native_test` = **340/340 pass** in 55 s |
| Rust workspace | **Does not exist yet** (created by R1-01) |
| Connected device | **None** (verified 2026-09-28) |

---

## Verified on this machine (2026-09-28)

- `~/.platformio/penv/bin/pio` → PlatformIO Core 6.2.0.
- `pio run -e esp32_usb` succeeds; produces `firmware.bin` (1,546,240 B),
  `bootloader.bin` (17,536 B), `partitions.bin` (3,072 B).
- `pio test -e native_test` → **340 test cases, 340 succeeded** in 55.3 s.
  (`docs/plan/task-list.md` still says 234 — that document is stale.)
- The `espressif32` platform and all 12 C++ libraries were downloaded and installed by
  that build.
- `.mise.toml` existed but needed `mise trust`; after trusting, `mise ls` works and shows
  node 24, pnpm, python 3.14.7, clang-format 23.1.1 all **missing** (declared, not
  installed).
- No `rustup`, `cargo`, `just`, `espflash`, or ESP-IDF on the host.
- No ESP-IDF is needed — `esp-idf-sys` self-provisions.

**Flash budget:** `app0` is 1,703,936 B; the C++ image is 1,546,240 B. **~154 KB
headroom.** A Rust esp-idf image with `std` will not fit. R0-02 and R2-03 address this.

---

## Blocked

| Blocker | Blocks | Needs |
| --- | --- | --- |
| **No ESP32 device attached.** `ioreg -p IOUSB` and `/dev/cu.*` show no Espressif device; `DEBUG_GUIDE.md` expects `/dev/ttyUSB0`, which does not exist. | Every HW task: R0-01, R1-01, R1-03, R1-04 (on-device), R1-06, R1-07, all of R3, all of R4 | A DevKitC-V4 board plugged in |
| `mise` tools declared but not installed | `pio run --target format`, frontend build | `mise install` |
| Whether `cargo bloat` works on macOS arm64 | `just size` attribution detail | R1-01; fallback is `.map` + `xtensa-esp32-elf-size` |
| Rust toolchain not installed | R1-01 | `just setup` |

Build-only spikes (R1-02, R1-04 layout, R1-05) can proceed without hardware.

---

## Completed tasks

_None yet._

---

## Open questions for a human

| Question | Blocks | Default if unanswered | Answer |
| --- | --- | --- | --- |
| Drop HX711 + Acaia BLE scale support? | R0-03 → R2-07 | Drop (dead code) | |
| Rebalance partitions, or embed the SPA in the binary? | R0-02 → R2-03 | Measure, then rebalance; embed the SPA | |
| Keep SH1106 support or drop it? | R2-10 | Drop and document (`ssd1306` has no SH1106; `sh1106` 0.5.0 is on `embedded-hal 0.2`) | |
| SSE or WebSocket for the UI's live channel? | R1-05 → R3-14 | SSE via `EspHttpConnection::write` / `raw_connection()` (both ship in esp-idf-svc 0.53); WebSocket only if both fail | |
| Encrypt NVS credentials during the port? | R2-06 | No — parity; record as a follow-up | |
| Target ESP32-S3 / C6 at all? | R4-07, R4-08 | No — the board in use is the original ESP32 | |

---

## Corrections made 2026-09-28 (do not reintroduce)

Found by adversarial review. Each was a real error in an earlier draft of these docs.

1. **"The original ESP32 has no Bluetooth radio"** — **false**. It has BR/EDR + BLE. The
   right reason to drop scale support is that it is dead code.
2. **Counts were stale**, all in the same direction: 18 states (not 19), 96 registered
   config params (not 108), 10 bitmap fonts (not 11), 21 `static_assert`s (not 22),
   33 test suites (not 50), 12 `lib_deps` (not 13), ~28 kLoC (not 19). Re-derive from the
   repo; do not hand-edit counts.
3. **`ldproxy` IS on crates.io (0.3.5).** The embuild GitHub release tops out at v0.3.2
   (2022). The prescribed curl also 404'd (assets are named by Rust triple, not `uname`).
4. **The `justfile` did not parse** — `{{ fn(args) }}` does not exist in just, a recipe
   dependency cannot take arguments, and `KEY=value` is make syntax. All three rewritten;
   positional arguments only.
5. **`just setup` failed on its last line** (`doctor` read an unbound shell var) and the
   `mise` `ldproxy` task 404'd.
6. **`&TWDTDriver` does not compile** — `Send` but not `Sync` in esp-idf-hal 0.47. The
   driver must be *moved* into the control task.
7. **`heapless::Deque` cannot be a cross-task channel** (no interior mutability), and
   `hal::task::queue::Queue<T>` requires `T: Copy`. Cross-task `Command`/`Event` must be
   `Copy` — no `String`, no `Vec`.
8. **The SSE question is already answered**: `EspHttpConnection::write()` (chunked) and
   `raw_connection().write_all()` (raw fd) both ship in `esp-idf-svc` 0.53.0. The planned
   ~20-line FFI shim is unnecessary.
9. **`esp-wifi-provisioning` forces `esp-idf-hal/rmt-legacy`**, which removes
   `hal::onewire` and the GPTimer module from the whole graph — it would break R3-06 and
   the R1-07 fallback. Check this in R1-06 step 0.
10. **"≥ 2 MB per app slot" is arithmetically impossible** — two 2 MiB slots exceed the
    whole 4,063,232 B region by 131,072 B. The real max with a 64 KB `spiffs` is
    (4,063,232 − 65,536)/2 = **1,998,848 B ≈ 1.906 MiB/slot**.
15. **R3-17 was a dependency deadlock** — it depended on R4-01, but Gate 3 required every
    `HW: yes` R3 task to pass, and R4-01 depends on Gate 3. Renamed **R4-01b** and moved to
    Phase 4.
16. **`just/size.just` had no owning task** while Gates 1-4 all gate on `just size`.
    Added **R1-09**.
17. **The CI portable-crate gate failed open** — `cargo tree … 2>/dev/null` into `grep -q`
    means a failing `cargo tree` silently *passes* the gate. Removed the redirect.
18. **`wifi-reset` leaked the auth password into `curl`'s argv** (world-readable in `ps`).
    Now piped via `--config -` on stdin, and the recipe is declared `#!/usr/bin/env bash`
    because `read -s` is a bash builtin, not POSIX `sh`.
11. **Moving to LEDC changes the PWM period.** The C++ heater is a 1 Hz / 100-step chopper
    (`ProcessState.h:183`). This is a behaviour change and is now in
    `intentional-diffs.md`.
12. **`esp-idf-svc#395` is about `SpiBusDriver: Send`**, not partition tables. The advice
    is still right, sourced from `esp-idf-sys/README.md`.
13. **The CI portable-crate grep could not catch a real `esp-idf-svc` dependency** (it
    matched underscores, not hyphens). Replaced with `cargo tree`.
14. **The CI firmware job had no `env:` block**, so it silently used the default ESP-IDF
    and omitted `--cfg espidf_time64`.

## Findings to carry forward

Recorded during the initial audit; these are **bugs in the C++ firmware that must be
fixed in Rust, not replicated**:

1. **`safety.emergency_temp` and `safety.emergency_hysteresis` never persist.** Defined
   at `Config.h:813, 822` and read at `EmergencyStopManager.cpp:18-19`, but absent from
   `getAllConfigParams()` (`src/Config.cpp:438-563`). Never NVS-loaded, never saved,
   never exported to the UI, never importable. **They silently reset to the compiled
   default on every reboot** — a live over-temperature-settings bug on safety path S1.
   → R2-06 must fix this, with a test that fails against the old behaviour.
2. **OTA leaves the pump and valve uncommanded.** `LoopManager::update()` returns early
   while `OTA::isActive()` (`LoopManager.cpp:128-132`), so the state machine never runs;
   `otaPrepareHardware()` only disables the heater (`SystemInitializer.cpp:54-59`).
   → R3-15 must call `safe_hardware_shutdown`.
3. **The pressure sensor blocks the control loop 20 % of the time.**
   `pressureSensor.h:35` does `delay(10)` every 50 ms.
   → R3-05 must make it non-blocking.
4. **`enterSafeMode()` / `exitSafeMode()` are log-only no-ops**
   (`MachineStateContext.cpp:393-402`) and `hasUserActivity()` hard-returns `false`
   (`:419-423`).
   → R2-08 must decide and implement these deliberately.

Documentation drift, for the record: `CONFIG_REFERENCE.md:114` documents
`display.blescale_brew_timer` and `:165-172` documents `display.blinking.mode` — neither
exists in the code.

---

## Unverified assumptions status

See [02 §8](../../docs/rust-migration/02-research-compatibility-matrix.md#8-summary-of-unverified-assumptions).
All ten (U1-U10) are still open. U3 (TSIC-306) is the one that can invalidate ADR-0004.
