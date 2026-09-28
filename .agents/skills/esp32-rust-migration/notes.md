# Migration Notes

Running log for the C++ → Rust firmware migration. **Update this at the end of every
task and every phase gate.** Keep it factual: what was done, what was observed, what is
blocked.

---

## Current state

| Field | Value |
| --- | --- |
| Current phase | **Phase 1 (R1)** — R0-04 and R1-01 executed 2026-09-28 |
| Next task | **R1-02** (executor decision) — needs no hardware |
| Plan reviewed | 2026-09-28 by two adversarial subagents; 24 hard factual errors and 5 blocking tooling defects found and **fixed**. See 06 and 07. |
| ADR-0004 status | **Proposed** (becomes Accepted at Gate 1) |
| C++ baseline | `pio run -e esp32_usb` **succeeds**; `firmware.bin` = 1,546,240 B; `pio test -e native_test` = **340/340 pass** in 55 s |
| Rust workspace | **Exists** at the repo root (8 crates; 5 portable + 3 device). Builds, links, boots and runs. |
| Device image | **382,528 B** flashable app image, first measurement (07 §5). App slot 1,835,008 B → **+1,452,480 B headroom**. |
| Connected device | `/dev/cu.usbserial-204140` — `esp32` rev v3.0, 4 MB flash, dual core, WiFi+BT, MAC `ec:62:60:76:b5:3c`. Auto-reset works; a headless UART capture script is at `scripts/serial-log.py`. |

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
| ~~No ESP32 device attached.~~ **Resolved 2026-09-28**: `/dev/cu.usbserial-204140` is present and is an `esp32` rev v3.0, 4 MB, dual core. | — | — |
| **Flaky outbound TLS on this host** (same URL succeeds and fails minutes later) | Every provisioning step: `espup install`, `cargo fetch`, the ESP-IDF clone, `idf_tools.py` | Retries. Do not record a single failure as "no network". |
| `mise` tools declared but not installed | `pio run --target format`, frontend build | `mise install` |
| ~~Whether `cargo bloat` works on macOS arm64~~ **No** (0.12.1, no symtab) | — | Fallback in use: `xtensa-esp32-elf-size -A` + the final link map |
| ~~Rust toolchain not installed~~ **Installed** (`esp` 1.97.0.0) | — | — |

Build-only spikes (R1-02, R1-04 layout, R1-05) can proceed without hardware.

---

## Completed tasks

### R0-04 — C++ baseline (2026-09-28)

- `pio run -e esp32_usb` → success, `firmware.bin` = **1,546,240 B**.
- `pio test -e native_test` → **340/340** test cases pass in 22.5 s (the previously
  recorded 55 s is from a cold build).
- `pio run -t buildfs` **fails**: `Failed to fetch metadata from
  https://registry.npmjs.org/pnpm: error sending request`. `curl` to the same URL
  succeeds, so the registry is reachable and this is a **transient/flaky network
  path on this host**, not an outage. Do not record it as "npm is down".

### R1-01 — workspace + toolchain proof (2026-09-28) ✅

- `esp-idf-svc` 0.53.0 / `esp-idf-hal` 0.47.0 / `esp-idf-sys` 0.38.1 build, link, boot
  and run on this host for `xtensa-esp32-espidf` at ESP-IDF **v5.5.5**. **U1 did not
  materialise.**
- Flashed once and captured the boot log: ESP-IDF v5.5.5 banner, our partition table,
  `heater/valve/pump all inactive (LOW)` readback assertion, `esp_task_wdt_add -> 0`,
  1 Hz heartbeat.
- **First image measurement: 382,528 B** (see 07 §5 and `size-baseline.json`).
- `just fmt-check`, `just lint`, `just test`, `just lint-esp32` (`-D warnings`),
  `just build-esp32`, `just size`, `just size-check` all pass.

#### R1-01: the plan was wrong in six places (all fixed in the repo, re-verify before R2-01)

1. **The device binary crate needs a `build.rs`.** `esp-idf-sys` publishes its link
   args as `links` metadata, and Cargo does not forward a dependency's
   `cargo:rustc-link-arg` to the binary. Without
   `build.rs` → `embuild::espidf::sysenv::output()` (plus `[build-dependencies]
   embuild = "=0.33.5"`) the final link contains **no ESP-IDF archives** and dies with
   undefined references to `pthread_create`, `write`, `abort`, `sched_yield`, …
   04 §6 and 05 §2 do not mention this. The official esp-rs template has it.
2. **`.cargo/config.toml` must set the linker to `ldproxy`** for the device triple
   (`[target.xtensa-esp32-espidf] linker = "ldproxy"`), or the link fails with
   `unrecognized command-line option '--ldproxy-linker'`. Note the key is `linker`,
   **not** `rustc-linker`, under `[target.<triple>]` in cargo 1.97.
3. **The Xtensa GCC must be on `PATH` for the build**, or rustc fails with
   ``linker `xtensa-esp32-elf-gcc` not found``. 05 §2 covers this only implicitly via
   espup's `export-esp.sh`; `just env-file` now generates `.rust-esp-env.sh` from
   whichever toolchain is actually installed.
4. **The app image does NOT contain the partition table** (its first byte is the 0xE9
   app magic). The table is a separate 3,072 B image at 0x8000, so the flash recipe
   must pass `--partition-table` — 05 §3's rule that partition tables are "flashed
   explicitly" is correct and load-bearing, not a style preference.
5. **A virtual workspace has no "root crate"**, and `esp-idf-sys` takes
   `[[package.metadata.esp-idf-sys]]` **only from the root crate's** `Cargo.toml`
   (`esp-idf-sys/build/config.rs:92-122`). In this layout it prints
   `cargo:warning=could not identify the root crate and ESP_IDF_SYS_ROOT_CRATE not
   specified` and **silently ignores `extra_components`** — i.e. 04 §6's LittleFS
   component. `ESP_IDF_SYS_ROOT_CRATE=cc-firmware` is required in `.cargo/config.toml`
   `[env]`. **Verify the LittleFS component is actually present before using
   `svc::fs::littlefs`.**
6. **The binary is named `firmware`, so the artifact is
   `target/<triple>/release/firmware`** — 05 §6's CI upload path
   (`.../release/cc-firmware`) is wrong. 04 §6 says so; 05 §6 was not updated.

Plus one environment fact worth carrying: **this host's network path intermittently
drops outbound TLS connections** (same URL/second succeeds and fails minutes later;
`espup` failed 3× on `api.github.com`, the `esp` toolchain's cargo failed 4× on
`index.crates.io`, while `curl` and `git` succeeded throughout). Every provisioning
step — `espup install`, `cargo fetch`, the ESP-IDF git clone, `idf_tools.py install`
— needs retries. Do not conclude a host is blocked from a single failure.

#### R1-01: host state that differs from the plan

- `espup 0.17.1` is installed, but `espup install` **without `--toolchain-version`
  fails** in its first step (the GitHub "latest release" query). Use
  `espup install --targets esp32,esp32s2,esp32s3 --toolchain-version 1.97.0.0
  --skip-version-parse`. The `esp` toolchain installed is **1.97.0.0** (rustc
  1.97.0-nightly); it has `aarch64-apple-darwin` std and `rust-src`, and the Xtensa
  targets are built with `-Zbuild-std=std,panic_abort`.
- **The RUSTUP_HOME concern did not materialise**: `rustup run esp rustc --version`
  works, because `espup` and the system rustup share `~/.rustup`. `just doctor`
  asserts it.
- `ldproxy` 0.3.5, `espflash` 4.6.0, `cargo-espflash` 4.6.0, `cargo-binstall` 1.24.0,
  `cargo-bloat` 0.12.1, `just` 1.58.0 are installed.
- **`cargo bloat` does not work here** (0.12.1): `Error: parsing failed cause
  'symbols section is missing'`, because `[profile.release] strip = "symbols"` leaves
  no symtab and re-running with `--config 'profile.release.strip="none"'` does not
  help (it inspects its own artifact). Use `xtensa-esp32-elf-size -A` plus the final
  link map at
  `target/<triple>/release/build/esp-idf-sys-*/out/build/libespidf.map`.
- `espflash monitor` needs a TTY, so it cannot be used from an agent or CI. Use
  `just mon-headless <port>` (`scripts/serial-log.py`).
- `just --justfile just/size.just` changes the working directory to `just/`, so the
  root `justfile` delegates with `--working-directory .`.

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

### R2-08 — `cc-machine`: the state machine as a pure reducer (2026-09-28) ✅

- `crates/cc-machine/` is `no_std + alloc`, host-testable, and depends only on
  `cc-domain`, `cc-safety`, `cc-config` (`cargo tree` confirms no `esp_idf_*`).
- `reduce(&Machine, &Context, Event) -> (Machine, Vec<Effect>)`. `Machine` is a
  `Copy` value with no interior mutability; `Effect` is the only path to hardware
  and `applier::apply` is the only function that turns one into actuator calls.
- 18 states × 46 events × 5 machine flavours = **4140 pairs** in the exhaustive
  table, plus a 64-tick convergence drive of every pair.
- **255 tests** in `cc-machine` (47 unit + 208 integration, of which 27 are
  `#[ignore]`d records of C++ mock cases that have no Rust equivalent).
  Workspace total 165 → **420**.

#### R2-08: the C++ state machine is not what the docs say it is

1. **`test_state_machine` does not test the state machine.** All five cases are
   gMock plumbing; its own comment says "Full StateMachine tests require additional
   setup" (`test_state_machine/test_main.cpp:18-19`). Same for
   `test_pid_state_transitions` (mock states, not the real ones) and, in part, for
   `test_steam_water_injection` / `test_pid_mode_water_dispensing` (self-contained
   mock contexts, the real state files are never included). **The C++ has far less
   state-machine coverage than the 340-case count suggests.**
2. **ADR-0003 is violated by the code it was written for.**
   `BackflushFillingState::update` (`BackflushStates.cpp:71-76`) only logs, so the
   one backflush state that runs the pump never re-asserts it — while its four
   siblings all do. Pinned as `s13_…`.
3. **`SensorErrorState`'s recovery-clock reset is unreachable.**
   `ErrorStates.cpp:47-50` intends to measure the recovery delay from when the
   sensor error *clears*, but `BaseState::checkTransitions` returns `SENSOR_ERROR`
   (a discarded self-transition) before `checkSpecificTransitions` is ever
   reached, so the delay is measured from entry. Pinned as `s12_…`.
4. **Both pump watchdogs are dead.** `PumpTimer::start()` is never called, so
   `BrewHandler::checkPumpTimeout` and `HotWaterHandler::checkPumpTimeout` can
   never fire. Pinned as `s11_…`; the port keeps the check and makes it reachable.
5. **`hasUserActivity()` is a hard `return false`**
   (`MachineStateContext.cpp:419-423`), so the water switch cannot wake the machine
   from standby. Pinned as `s14_…`.
6. **`powerOff()` shuts the hardware down before setting the standby request**, so
   for one loop `PidNormalState::update` re-enables the pump. Pinned as `s15_…`.

All six are **preserved**, not fixed, each with a `s<N>_`-prefixed test.

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
