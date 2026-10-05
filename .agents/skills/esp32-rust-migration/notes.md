# Migration Notes

Running log for the C++ → Rust firmware migration. **Update this at the end of every
task and every phase gate.** Keep it factual: what was done, what was observed, what is
blocked.

---

## Current state

> ⚠ **Updated 2026-09-29.** The table below supersedes the 2026-09-28 entry. If you are
> reading a task description and assuming it is done because it sounds finished, check
> [README §Where the migration actually is](../../../docs/rust-migration/README.md#where-the-migration-actually-is).

| Field | Value |
| --- | --- |
| Current phase | **Phase 4 (R4), with the control loop restructured on 2026-10-01.** R0, R1, R2 and most of R3 implemented. |
| **Critical path** | **R4-01 — the reducer is NOT wired to the hardware.** `cc_machine::` appears nowhere in `cc-firmware/src`; the control task is a heuristic that drops web commands. **No state machine, no PID, no brewing on the device yet.** |
| **Device hostname** | **`test-cc-rust`** (`cc_config::schema::DEFAULT_HOSTNAME`). The C++ default is `silvia` and the C++ is unchanged — the name is what distinguishes the two firmwares on one network. `mqtt.password`'s default is *also* `silvia`; that is a credential, leave it. See [intentional-diffs §12](../../../docs/rust-migration/intentional-diffs.md). |
| Plan reviewed | 2026-09-28 by two adversarial subagents; 24 hard factual errors and 5 blocking tooling defects found and **fixed**. See 06 and 07. |
| ADR-0004 status | **Accepted** in practice — `esp-idf-svc` 0.53.0 / ESP-IDF v5.5.5 is what is built. |
| C++ baseline | `pio run -e esp32_usb` **succeeds**; `firmware.bin` = 1,546,240 B; `pio test -e native_test` = **340/340 pass**. **The C++ is never modified or flashed** — the human has declined the C++ baseline capture for exactly that reason. |
| Rust workspace | 10 crates. Builds, links, boots, runs on hardware. |
| Host tests | **900+** passing. |
| **Device tests** | **145 passing, 0 failing, 1 pre-existing LOST** on real hardware via `just test-esp32` (measured 2026-10-01 against a 142/0/1 baseline on `f39858e`). This gate did not exist until R1-08's follow-up and its absence had already let three device bugs ship. |
| Device image | **1,580,224 B** of an 1,835,008 B slot (13.9 % headroom), measured 2026-10-01. Growth vs the `f25-web-ui-embedded` baseline: **+1.33 %**, inside the 10 % limit. |
| **Static RAM** | **131,688 B — 42 % of the ESP32's 320 KB**, roughly double the pre-network figure. **RAM, not flash, is now the binding constraint**, and ADR-0002's 30 KB shed margin was tuned against a much smaller baseline. |
| Connected device | `/dev/cu.usbserial-224140` — `esp32` rev v3.0, 4 MB, dual core, WiFi+BT, MAC `ec:62:60:76:b5:3c`. **WCH CH340**, not CP2102N. Link unreliable above ~460800. |
| Heater output | **10 ms GPTimer ISR**, not LEDC (LEDC cannot do a 1 Hz carrier on this chip — 09 §17). **Never energised** except in a deliberate, logged panic-probe. |
| Known regression | **Resolved 2026-10-01.** The control tick overran its 10 ms budget in ~62 % of ticks because a 1 KB display frame was written inside it (09 §24). The panel is now on its own task at 100 ms and the loop is at 100 Hz; the periodic `control tick:` line reports the worst tick and the over-budget count, and it is the check to watch. |

---

## Verified on this machine (2026-09-28)

- `~/.platformio/penv/bin/pio` → PlatformIO Core 6.2.0.
- `pio run -e esp32_usb` succeeds; produces `firmware.bin` (1,546,240 B),
  `bootloader.bin` (17,536 B), `partitions.bin` (3,072 B).
- `pio test -e native_test` → **340 test cases, 340 succeeded** in 55.3 s.
  (The 234 quoted by the now-deleted C++ cleanup tracker is gone with it; 340 is
  the number, and 01 §10.2.3 re-verifies it.)
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
| ~~No ESP32 device attached.~~ **Resolved 2026-09-28**: `/dev/cu.usbserial-224140` is present and is an `esp32` rev v3.0, 4 MB, dual core. | — | — |
| **Flaky outbound TLS on this host** (same URL succeeds and fails minutes later) | Every provisioning step: `espup install`, `cargo fetch`, the ESP-IDF clone, `idf_tools.py` | Retries. Do not record a single failure as "no network". |
| `mise` tools declared but not installed | `pio run --target format`, frontend build | `mise install` |
| ~~Whether `cargo bloat` works on macOS arm64~~ **No** (0.12.1, no symtab) | — | Fallback in use: `xtensa-esp32-elf-size -A` + the final link map |
| ~~Rust toolchain not installed~~ **Installed** (`esp` 1.97.0.0) | — | — |

Build-only spikes (R1-02, R1-04 layout, R1-05) can proceed without hardware.

---

## Completed tasks

### R1-08 (partially) — the parity harness (2026-09-29) ⚠ baseline NOT captured

The migration's correctness instrument now exists; the reference it measures against
does not.

**Built and tested:**

- **The scenario format**, specified in
  [`docs/rust-migration/10-scenario-format.md`](../../../docs/rust-migration/10-scenario-format.md)
  and implemented by `crates/cc-parity`. Seven stimulus kinds (`rest`, `wait`,
  `button`, `sensor`, `config`, `mqtt`, `ota`), a capture spec, and nine assertion kinds.
- **17 scenarios** in `docs/rust-migration/scenarios/`, covering S1–S11. Twelve of them
  would energise an actuator; all twelve are `dry_run`.
- **`scripts/parity/run.sh`**, which `just parity` calls. It runs each scenario, diffs
  the observation against `baseline/cpp/`, classifies every diff against the ledger, and
  exits non-zero on anything unexplained.
- **The divergence ledger** — five `ledger` blocks inside `intentional-diffs.md`, each
  naming a heading in the same document so the two cannot drift.
- **71 tests** in `cc-parity` (`just parity-test`), including the required proof that a
  synthetic **undeclared** diff makes the runner exit non-zero, and that a declared one
  does not.

**Not built, and it is the reason Gate 1 is not passable:**

- `docs/rust-migration/baseline/cpp/` is **empty**. `just parity` reports every scenario
  `BASELINE-MISSING` and exits **2**. Capturing one means flashing the **C++** image and
  letting its control loop run against a real boiler — a reviewed safe-test procedure
  and a human present, neither of which R1-08 had. **No baseline was fabricated.**
- The **C++ half of a `dry_run` scenario** — driving the same stimuli through the C++
  state machine — is R4-03's work. The Rust half, the format, the ledger and the runner
  are done.

**Two findings the harness produced before any baseline existed:**

1. **The safety monitor must run on the sensor cadence, not the control loop.** Run
   every 10 ms tick, S1's three-reading debounce trips on one reading repeated 30 times
   in 300 ms, and the most safety-relevant timing constant in the firmware is
   untestable. `overtemp_trip` now trips at 800 ms on the third reading, as the C++ does
   (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS` = 400 ms).
2. **`OpenSteamValve` is never emitted by the reducer** — the steam valve is a solenoid
   the machine cannot open by itself. The S5' whitelist therefore acts in the *close*
   direction, and `steam_on_off` asserts that. An assertion that the valve was opened
   would have been asserting something neither firmware does.

**Do not skip the actuator-safety note when picking this up.** The dry-run safety
property is *structural*, not a convention: `cc-parity` has no GPIO, no `cc-hal-esp32`
in its tree, and its `Actuators` is a `Vec` of call names. Keep it that way — adding a
device dependency to `cc-parity` would remove the only thing that lets twelve
actuator-energising scenarios run with no machine attached.

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

### R1-07 + safety-gap work (2026-09-28) ✅ for the host, ❌ for the hardware

Four findings closed on purpose (see [`docs/rust-migration/intentional-diffs.md`](../../../docs/rust-migration/intentional-diffs.md),
which is now **created** and is R1-08's deliverable):

1. **Pump timeouts armed** (09 §11) — on the pump-on edge, with
   `Effect::PumpTimeoutFired` carrying the C++'s own `logError` text.
2. **Steam-valve whitelist added** (09 §2) — `cc_safety::steam_flow_allowed`,
   `STEAM_RUNNING` only, `match` with no wildcard arm, plus a
   `steamValveSafetyShutdownCheck` in the reducer's tail.
3. **Water valve tank-gated** (09 §3).
4. **PID derivative over the real elapsed time** (09 §1).

Each replaced its `s<N>_` test with a `div<N>_` one. 420 → 443 host tests.

#### The finding that made the steam whitelist more than a formality

> "Steam and water valves share the same physical relay."
> — `include/clevercoffee/hardware/ValveState.h:8-11`

`rg -n openSteamValve src/ include/` finds **no call site at all** — only the
definition, the `MachineStateContext` pass-through and the interface declaration.
So in the C++ the gap is closed by an accident of the call graph, and an
ungated `openSteamValve()` is an ungated **water** valve. The whitelist derivation
is written out in `cc_safety::steam_flow_allowed`'s doc comment; the two
whitelists are asserted **disjoint** by a test, because a wider steam list would
re-open S5's hole from the other side.

#### PID parity, measured

Oracle scenarios A–C (the 1000 ms window) are **bit-identical, max |delta| 0.0**,
before and after. A new scenario E drives the 1000 ms window on ragged
timestamps to quantify the residual: **max |delta| = 1.0** on a ±1000 output,
arising only on steps that arrive *late*. Scenario D is **retained in the oracle
as the C++'s `NaN`** — do not "fix" the oracle to agree with the port, it is the
only evidence for the divergence.

#### Heater output: LEDC, and four things the plan got wrong

1. **The C++'s ISR rate is not its switching rate — this is the big one.** The
   10 ms ISR fires 100 times a second, but its predicate `pidOutput > counter`
   is monotone, so the relay **level** changes **twice** a second (one falling
   edge inside the window, one rising edge at the wrap) and not at all at duty 0
   or full duty. An `f` Hz square wave makes `2f`, so **`f ≤ 1 Hz`**. An earlier
   revision of this file, of 04 §5 and of both crate module docs specified
   **100 Hz** because it "reproduces the existing 10 ms-step / 1 Hz chopping
   exactly". **That was wrong** and has been corrected: 100 Hz would have switched
   a 2 kW contactor **200 times a second, a hundred times its mechanical duty**,
   while matching delivered power almost exactly. A carrier frequency is a
   *mechanical* duty, not a fidelity setting. The carrier is now **1 Hz** — one
   period per control window, which is also the only frequency at which the duty
   count can mean the same thing the C++'s millisecond duty meant.
2. **The low carrier has to be paid for in bits, and the divider is what
   limits them.** At 1 Hz, `div_param = (80e6 << 8) / (1 · 2^bits)` in
   ESP-IDF v5.5.5's `ledc_calculate_divisor`
   (`esp_driver_ledc/src/ledc.c:459-477`, `precision = 1 << duty_resolution` at
   `ledc.c:600`), and `LEDC_IS_DIV_INVALID` rejects `div_param ≤ 255` or
   `> 0x3FFFF` (`ledc.c:115,111`). At 1 Hz the reachable resolutions are
   **17, 18, 19, 20 and nothing coarser** — 16 bits already overflows the maximum
   divider. **`Bits17` chosen**: the coarsest that works, `div_param` 156 250,
   period exactly 80 000 000 APB clocks = **1.000 000 Hz**. Duty step 7.63 µs, so
   the C++ chopper is reproduced to **3.8 µs** — three orders of magnitude inside
   the 1 % bound. *(The discarded 100 Hz table was also wrong: at 100 Hz the valid
   resolutions are 10–19, not 8–10, and `div_param` at 100 Hz/`Bits10` is
   200 000, not 50 000.)*
3. **`Bits20` must be avoided — it is the resolution ESP-IDF says cannot reach
   100 % duty on the ESP32.** `ledc_channel_config`'s own comment: "due to a
   hardware bug, 100 % duty cycle (i.e. `2**duty_res`) is not reachable when the
   binded timer selects the maximum duty resolution", and 20 bits is the maximum
   — which is why `Resolution::max_duty` is `2^20 - 1` there. At 17 bits
   `max_duty` is a plain `131 072`, so full power is a *steady high level* and
   duty 0 a *steady low level*, i.e. "disabled" and "100 %" are different
   register values. `RESOLUTION` and `cc_domain::heater::CHOSEN_MAX_DUTY` are tied
   by a **`const` assert** in `cc-hal-esp32::heater`, so reaching `Bits20` is a
   compile error, not a review note.
4. **High-speed mode is not needed.** It exists for multi-MHz carriers. At 1 Hz
   low speed is six orders of magnitude inside its range, and high-speed timers
   are the scarce resource on this part. Decision recorded, not left implicit.

**The contactor itself is still unknown, and R1-07 is *not* finished until
somebody with the machine measures it.** Matching the C++'s transition rate is
necessary and not sufficient. Still open, recorded as measurements and not as
assumptions: the contactor's **minimum on/off time** (the software guarantees it
never requests a pulse narrower than the C++'s own 10 ms step, but whether 10 ms
is inside the contactor's ratings is a datasheet/measurement question);
**whether a hardware-PWM output is acceptable to the coil at all** at 1 Hz, a
frequency the C++ never *produced* even though its average was 1 Hz; the
**realised frequency and duty on the pin**; and whether 1 Hz is the best point on
the wear-versus-duty-resolution curve.

#### R1-07: what was NOT done

* **The device was not flashed.** Not needed: the duty arithmetic is host-tested,
  the divisor feasibility was read out of ESP-IDF's own source, and the firmware
  builds and links. Flashing to observe "duty 0" would have bought nothing over
  reading the code, and the board's boiler disconnection state is unconfirmed —
  which under skill §2 rule 4 means no energising test may run at all.
* **R1-07 steps 1 and 2 (drive a dummy load, measure with a scope) are NOT done.**
  There is no dummy load and no scope attached. Acceptance ("duty matching the PID
  output within 1 %") is therefore **unverified on hardware**, and R1-07's
  `HW: yes` is unsatisfied. Left un-run rather than claimed.
* **The GPTimer fallback is the only transport, and the seam is gone.** The
  `HeaterDuty` trait and its unbrought-up `LedcPwm` impl were deleted: LEDC
  panics this chip at every duty, so the second impl could never be built here.
  `HeaterOutput` is concrete over `TimerIsrPwm`. A chip without the spin gets a
  transport written for it, not one that has sat unbrought-up.

## R3 hardware findings (2026-09-29)

Four defects found on the device during the R3 storage-and-network slice, and one
process gap that let three of them through. All four are now fixed in the working
tree; **nothing is committed**.

1. **`app_main`'s stack is 3.5 KB and the startup sequence needs ~11 KB.** The
   symptom is not a stack-overflow report: it is
   `assert failed: block_trim_free tlsf_control_functions.h:548 (block must be free)`,
   an allocator assert on corrupted DRAM, with a backtrace that names
   `BlobConfigStore::load`. Measured with
   `xtensa-esp32-elf-objdump --dwarf=frames` on the `diagnostic` ELF (same codegen as
   `release`): `bring_up` 2400 B + `bring_up_config` 1584 B + the blob store's
   load/save 3088 B + five levels of `Deserialize` ~450 B each + `f64::from_str`
   1712 B.
   **Fix:** the startup sequence moved to a `bring-up` task with an explicit 16 KB
   stack (`BRING_UP_STACK_BYTES` in `crates/cc-firmware/src/main.rs`, with the
   derivation in its doc comment). `main` is now a trampoline.
   **Lesson worth more than the fix:** *any* task's stack must be sized from
   `.debug_frame`, not from a feeling. `CONTROL_STACK_BYTES` (8 KB) was a guess.
2. **The password window lasted zero milliseconds.**
   `Session::expire_window` tested `now.wrapping_sub(opened + WINDOW) >= WINDOW`.
   `wrapping_sub` of a *negative* difference is a number near `u32::MAX`, so the
   condition was true on the first poll after `wifi set`. Every password line was
   therefore parsed as a command and rejected, and UART provisioning could never
   complete — with a success-looking `ok ssid accepted` on the console.
   **Fix:** the rule moved to `cc_domain::provisioning::password_window_expired` and is
   host-tested, including across the 32-bit wrap.
3. **`/events` starves the whole HTTP server.** ESP-IDF's `httpd` is one task; the
   `/events` handler loops inside it writing a frame per second, so with one SSE
   client connected **every other endpoint stops answering** (measured: 150
   consecutive `/api/parameters?filter=all` requests all timed out, and answered in
   22–140 ms within a second of closing the stream). The C++ does **not** have this
   problem: `AsyncEventSource` (`WebServerManager.cpp:302-319`) returns from the
   handler on connect and pushes from the loop task, so httpd is never held.
   → **R3-14 is not at parity.** The Rust `/events` is a pull loop where the C++ is a
   push. `Sse::broadcast` and `cc_hal_esp32::web`'s client list are the push design and
   are **not wired**; `network::broadcast_temps` counts a push attempt and says so.
   Until it is, the UI and the API cannot be used at the same time.
4. **`esp_restart()` does not flush UART0, so the last two lines before a reboot are
   lost.** Both the provisioning task's `CCWIFI ok accepted …` reply and the control
   task's `config: a wifi credential from the console was stored` receipt were dropped
   by the reset that followed them. The write is queued to the UART ring; the ROM
   reset does not drain it. → every reboot path needs a `uart_wait_tx_done` (or an
   equivalent) before `esp_restart()`. `scripts/drive-provisioning.py` treats the
   absence of those two lines as inconclusive for that reason.
5. **There is no `just test-esp32`, so `cc-hal-esp32`'s unit tests never run.**
   `just lint-esp32` compiles them (≈50 of them) and `just test` cannot build the
   crate for a host target, so every one of them has been type-checked and never
   executed. That is the whole story of findings 2 and 4: the window arithmetic was
   *tested* — in a test that has never been run, by a machine that has never been
   asked. Either add a runner recipe or move the pure logic out of the device crate
   into `cc-domain`, where `just test` reaches it.

Also recorded, and *not* fixed:

* **The log mute (`LOG_MUTED`) has no consumer.** `cc_hal_esp32::provisioning` sets and
  clears it correctly and the module documentation used to claim a log facade read it.
  None does. On this board the framing is safe anyway — the log stream is written to
  UART0 TX and the parser reads UART0 RX, so the firmware cannot read its own log — but
  the moment a transport the machine can also *read* lands, a log stream it can
  read is a real hazard and that flag is the whole of rule 5.
* **`/api/parameters?filter=all` returns 5 of the C++'s 10 fields.** The C++
  (`Config.h:99-109,227-236`) sends `name`, `label`, `section`, `order`, `helpText`,
  `type`, `value`, `default`, `min`, `max`. The Rust sends `name`, `type`, `default`,
  `min`, `max` — **`value` is missing, and it is the field the UI renders.**
  `cc_config::schema::ParamSpec` carries no `label`/`section`/`order`/`helpText`, and
  the live `Config` needed for `value` is already in `Web::start`. → R3-14 parity work.
* **`Sse::sent` / `Sse::dropped` are counted but not exposed**, so the soak numbers in
  the R3 report are client-side. `/api/nvs-debug` is the obvious home.
* **`/api/status` cannot see the radio.** `telemetry_from(reading, uptime, wifi)` takes
  an `Option<&Sta>` and the control task always passes `None` — it has no `Sta`
  handle, which is the point of 04 §3.2. Measured: the machine was associated at
  RSSI −53 dBm, signal 3/4, IP `10.0.1.168`, and `/api/status` reported
  `wifiAssociated: false, wifiSignal: 0, ip: null`. The radio has to publish its own
  numbers into `Shared` (which `with_wifi` already knows how to merge); the
  alternative — handing the control task a `&Sta` — is exactly the coupling 04 §3.2
  forbids.
* **`objdump -d` is unusable for 09 §22 on this image.** `.flash.text` opens on a data
  table, objdump loses instruction sync and prints raw words for the next 275 KB —
  including `TimerDriver::handle_isr`, the heater's ISR body. A grep over that output
  reports "zero FP instructions" for a function it never decoded. Extract each symbol's
  own bytes and disassemble them as `-b binary` instead. **Re-run §22 with that method
  before trusting any future FPU result.**

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

See [02 §8](../../../docs/rust-migration/02-research-compatibility-matrix.md#8-summary-of-unverified-assumptions).
All ten (U1-U10) are still open. U3 (TSIC-306) is the one that can invalidate ADR-0004.

---

## HTTP contract pass (2026-09-30) — five UI-reported defects, all fixed and measured

The human drove the embedded React UI and reported five real defects. All are fixed,
flashed and measured on `ec:62:60:76:b5:3c`. **Note the serial port moved:**
`/dev/cu.usbserial-204140` (every older note) is gone; it is now
**`/dev/cu.usbserial-224140`**. Run `just identify` and do not trust the notes.

**1. `POST /api/pid|steam|backflush` 400 on a bare POST.** The C++ reads no field on
these three and computes `!current` (`WebServerManager.cpp:444,466,490`); the handler
here demanded `value`/`on`, which the UI never sends (`useMachineToggles.ts:22,31,40`).
New `register_toggle` + a `Toggle` struct. The negation happens **in the control task**
(`Command::TogglePid/Steam/Backflush`), because the C++ reads *live* machine state and
this web layer cannot without racing a snapshot. `?on=0`, `?on=1` and body `value=0`
still work.

**2. `POST /api/parameters` reported success without changing the machine.** `apply`
wrote `Config` and NVS, but the reducer **caches** `Machine::pid.mode_enabled` and
`Control::setpoint`, and nothing pushed the new value across — so `pid.enabled=1`
persisted and did nothing until a reboot. The control task now diffs before/after and
feeds `SetUserPidEnabled` / `set_setpoint`. Writes that genuinely **cannot** be live
(`hardware.switches.*`, the two sensor-fit flags — all read once by `SwitchBank::new`)
now answer `200` **with** `"requiresReboot":true` and the offending keys named, instead
of claiming success.

**3. `GET /api/config/download` 404.** Ported from `WebServerManager.cpp:706-723`,
including `Content-Disposition: attachment; filename="config.json"` — without it a
browser renders the JSON instead of saving it, which is why it is a separate route.

**4. The whole `/api/ota/*` group 404.** All four routes registered.
`/api/ota/status` returns the C++'s **real** status shape at idle values (the UI's
`OtaStatusSchema` *requires* `status`/`progress`/`updateInProgress`, so omitting them
left the page blank); `status` is sent as the **string** `"idle"` rather than the C++'s
integer, which `z.enum` rejects — intentional-diff §14. The three mutating routes answer
`501` + `unavailable_json("OTA","R3-15")`. **No OTA is implemented**, by design.

**5. `/events` sent two responses — the highest-value fix.** Every route goes through
`EspHttpServer::fn_handler`, which wraps the closure in `to_native_handler`
(`esp-idf-svc` `src/http/server.rs:660-685`) and calls **`complete()` after the handler
returns**. `complete()` (`:1158-1170`) sees `response_headers.is_some()` (set by
`initiate_response`) and takes the `httpd_resp_send(.., 0)` branch — writing a complete
`Content-Length: 0` 200 that **ends the response before any frame exists**. The detached
request then wrote the real chunked response as a **second** response on the same socket.
The broadcaster-task design was right and is unchanged; the **wrapping** was the bug.
`/events` is now registered with a **raw `httpd_uri_t`** (`web_async::register_raw_sse`),
so nothing calls `complete()`. Measured after: **one** `HTTP/1.1 200`,
`Transfer-Encoding: chunked`, **no `Content-Length`**, 13,408 B over 201 s.

### 🔴 A stack overflow found while testing standby — and the lesson is the old one

`SharedPanel::set_blank` existed, was unit-tested against a recorder, and was **called
from nowhere**. Wiring it into the standby path (the C++'s `LoopManager.cpp:330-334`)
crashed the device on the first transition to standby:

```
***ERROR*** A stack overflow in task pthread has been detected.
rst:0xc (SW_CPU_RESET)
```

`pthread` is the **control** task — a Rust `std::thread` name does not reach FreeRTOS, so
the panic handler reports the default. The cause: `set_blank` built
`Oled::new_initialised(..)` purely to send **one byte** (`0xAE`/`0xAF`), and `Oled`
embeds the 1 KB page buffer plus the `Ssd1306` wrapper, on an **8 KB** stack.

Fixed by adding `I2cPanel::send_command(byte)` and sending the byte directly.
`Oled::set_power_saved` is unchanged and still what the unit tests exercise.

**This is notes' finding 3 repeating itself, verbatim.** A green test for a function
nobody calls is not coverage. *Any* new call on the control task needs its frame size
measured from `.debug_frame` on an **unstripped** build (`just diag-build`) — the
release ELF has no frame info at all, because `strip = "symbols"`.

### Standby / display — tested, and it works

`PID_NORMAL → STANDBY → PID_NORMAL`, with the panel blanking and restoring:

```
blanked=false frames=0     → PID_NORMAL
blanked=true  frames=13    → STANDBY   (frames FROZEN: 0xAE sent, nothing written after)
blanked=false frames=125   → PID_NORMAL (0xAF, frames resume)
```

Note `POST /api/sleep` from `PID_DISABLED` is a **no-op** (state 95 is unreachable from
state 90) — the C++'s `requestStandby` behaves the same way, and it is not a defect, but
it surprised the test and will surprise an operator.

### Buttons enabled — default `true`, and the risk is stated not resolved

All four operator switches default to `true` on request (intentional-diff §13). **The
stored NVS blob still carries the old `false` values**, so on this machine they had to be
set once over HTTP before the new default took effect — `default=true` in
`/api/parameters` is the code change; `value` is the device's blob. Worth remembering
that a default change does not reach a device that has already persisted a config.

`Pull::Floating` is kept deliberately: GPIO34/35/36/39 have no internal pull and
ESP-IDF accepts a `Pull::Down` there while silently doing nothing. **Measured on this
board:** with all four enabled the machine sat 35 s in `PID_DISABLED` and started no
brew, so the pins settle LOW here. That is an observation about this board's wiring, not
a resolution of the risk — `intentional-diffs.md` §13 has the full argument.

### Safety during this work

Pump and valve were **never** energised. 186 samples across the standby/wake cycle all
read `refused pump=0 water=0 steam=0 heater=0  pins pump=false valve=false`. The heater
ran only as the ordinary PID output against the configured 95 °C setpoint.


---

## The kernel defect: a second task may not own the DS18B20 (2026-10-01)

**Do not try this again without reading 09 §28 first.**

The control loop was 400 ms and the display frame was inside it, so the tick
overran its budget in ~62 % of runs and a switch press took half a second to
show. The fix is the one 04 §2 already specifies: 100 Hz, and the display on a
task of its own. That works.

The **sensor** task does not. Bisected on hardware, one variable at a time:

| sensor task | display task | crashes per boot |
| --- | --- | --- |
| not started | not started | **0** |
| started, DS18B20 poll disabled | not started | **0** |
| started, DS18B20 poll enabled (any cadence) | not started | 3–4 |
| not started | started | **0** |

The DS18B20 is the only user of `esp_idf_hal::interrupt::free`, which on the
original ESP32 is `vPortEnterCritical` on a **process-global** cross-core
critical section. On one task that is what the C++ does with `noInterrupts()`; on
a second task this build asserts inside the kernel
(`xTaskRemoveFromEventList`, and a `LoadProhibited` in the lwIP `tcpip_thread`
that has nothing to do with the firmware).

**The same applies to every cross-task blocking hand-off.** A `Queue` with a
blocking receive, a `std::sync::Mutex`, and an ESP-IDF task notification were each
tried as the way to shorten the control loop's sleep, and each asserts the same
way. `CommandQueue::try_send` does not, because it never blocks. So:

* the frame hand-off to the display task is a **lock-free double buffer**
  (`cc_firmware::slots`), not a mutex;
* the control loop runs on its **10 ms deadline** and consumes every event at the
  top of the next period.

At 10 ms that is not a latency anyone can feel, and it is the honest description
of what the firmware does.

## Things this session got wrong, and what caught them

The notes already record that "a fix that only existed in a test that never ran"
is a real failure mode here. Three more, all from the same day:

1. **A stack overflow looks like a kernel bug.** A 7.2 KB `History` inside a
   by-value `Shared`, and a 7.2 KB return value from `history_json`, both reset
   the device — the second one as a `LoadProhibited` in the middle of an HTTP
   request. The first showed up only as the on-target runner reporting two tests
   **LOST**. Anything fixed-size and large is a `Box`.
2. **The first version of a tick-timing fix made the machine disappear.**
   `next_deadline.wrapping_sub(now)` after a tick that overran its period is a
   49-day sleep, not a short one. The device stopped answering HTTP while the panel
   kept working. Elapsed-time arithmetic, not deadline arithmetic.
3. **A 10 ms control period on a 100 Hz tick kernel is one tick.** A "10 ms"
   timeout written as ten ticks is 100 ms. `cc_hal_esp32::wake` reads
   `configTICK_RATE_HZ` from the generated bindings for exactly this reason — and
   its own on-target test then caught the first version of the conversion, which
   added one tick per remainder instead of dividing.

## Still not verified

* **No hand on a switch.** The loop is fast and the debounce is pinned, but the
  press itself has still never been made by a person. "Wakes from standby" and
  "the steam button reacts immediately" are therefore *unverified on hardware* —
  what is verified is that the code path is the C++'s and that the latency budget
  above it is 10 ms + 20 ms + 100 ms instead of 400 ms + 20 ms + 100 ms.
* **A reboot into `PID_DISABLED` is correct, not a bug.** `hardware.switches.power.type`
  is `Toggle` and a toggle that reads off at boot starts disabled — in the C++ too
  (`SystemInitializer.cpp:606-641`). `pid.enabled` wins only when no power switch
  is configured. If the human wants the config to win, the fix is a config
  default, not a code change, and it is their call.


## Wi-Fi recovery, 2026-10-01

The machine went unreachable and took **three** faults, none visible in the
firmware's own summary. Full write-up in `09-cpp-findings.md` §30 and the check
list in `docs/operations/integration-checklist.md`.

* `wifi_auth_mode_t` is a **sequence, not a bitmask** and the driver compares it
  for **equality**. `WPA2WPA3Personal` cannot join a WPA2-only AP. This port had
  the `embedded-svc` default `WPA2Personal` via `..Default::default()`, and the
  fix is to name **WPA2**, not to widen the mask. The C++ is immune because
  `WiFi.begin()` leaves `wifi_authmode` at 0.
* The UART console now **always** starts. 04 §3.2's "only when no valid
  credentials exist" made a wrong network unfixable over USB.
* A length-only boot log (`stored credential — ssid "..." (N bytes), password M
  bytes`) is what found a stored SSID that did not exist on the network. Keep it.


## Where everything from 2026-10-01 is written down

`docs/archive/migration/31-findings-2026-10-01.md` — the index. Every finding from
the day, with where the detail lives and what is still open.

The four documents it points into:

| document | what it holds |
| --- | --- |
| `09-cpp-findings.md` §28–§31 | the `FreeRTOS` blocking hazard, the tick-rate measurement, the Wi-Fi recovery, the 15 ms in the applier span |
| `intentional-diffs.md` §14–§17 | the layout divergences from the C++ (uptime, `°C`, Scale rows) and the two behaviour changes (S1 counting samples, the reboot shutdown) |
| `docs/operations/integration-checklist.md` | a runnable check per finding — Wi-Fi, the tick, the screen fit, the language columns |
| `notes.md` (this file) | the Wi-Fi recovery summary an agent needs before touching Wi-Fi again |

**Open, with the next step named, in §7 of the index.** The first is the control
tick: ~65 Hz rather than 100 Hz, with the **applier span** measured at 12 ms of
the 15 ms and nothing in it obviously blocking.
