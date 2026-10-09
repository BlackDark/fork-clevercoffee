# Migration Task List — Phased, Executable

> **ARCHIVED — non-normative. Dated 2026-09/10, preserved for provenance.**
> Most of these tasks are done, and a task list that is 90 % complete reads as
> current while describing a port that no longer exists. What is *not* done is
> written down, dated and owned in [`docs/status.md`](../../status.md), which is
> the only page permitted to claim anything. Read this for the sequencing history
> and the acceptance criteria that were designed, not for the state of the work.
> See [`docs/archive/README.md`](../README.md).

The execution skill this was a companion to is gone; the port it governed is
closed. Every task has an ID, prerequisites, acceptance criteria, and an exact
validation command where one can already be determined.

**Read first:** [01 — Feature inventory](../../history/feature-inventory.md) ·
[02 — Research matrix](../../history/dependency-evaluation.md) ·
[03 — Decision record](./03-decision-record.md) ·
[04 — Target architecture](../../history/target-architecture.md) ·
[05 — Tooling](./05-tooling-and-workflows.md)

---

## Definitions

Two terms carry most of the weight in this plan and are used in more than 20 places.
Defining them here removes the largest source of agent ambiguity.

### Parity

> Behavioural equivalence with the C++ firmware **on the committed scenario set**
> (`docs/history/scenarios/`, created by R1-08), **excluding the divergences listed
> in `docs/history/divergences.md`**.

Operationally: `just parity <port> <host>` replays every scenario against both
firmwares, captures the state-transition log plus the scenario's endpoint snapshots, and
diffs against `docs/history/baseline/`. It exits **non-zero on any diff not
explained by `intentional-diffs.md`**.

`intentional-diffs.md` is created at R1-08 and seeded with the known C++ bugs the port
**fixes**, so a diff on those is expected rather than a regression to chase.

### Known-safe state

> Heater duty **0**, pump **off**, water valve **closed**, steam valve **closed**,
> solenoid **closed**, all three LEDs **off**, and the emergency latch **not set**.

This is `safe_hardware_shutdown` in 04 §4 — *not* `emergency_shutdown`, which is the same
actuator state **plus** the latch. Every "drive to a known-safe state" instruction in this
plan means the former, and the latch must not be set by a routine shutdown.

---

## Conventions

- **Validation commands** are the `just` recipes defined in
  [05 §4](./05-tooling-and-workflows.md#4-justfile). If a recipe does not exist yet, the
  task that needs it creates it.
- **HW** = hardware required. Tasks marked HW are **blocked** while no device is attached
  (see [01 §10](../../history/feature-inventory.md)).
- **Safety** — any task touching pump, valve, or heater lists its safe test procedure.
  A future agent must not improvise one.
- **Commit** — one task, one commit, message prefixed with the task ID:
  `R1-01: bootstrap rust workspace for xtensa-esp32-espidf`.
- A task is **done** only when its validation command has actually been run and passed.
  See the skill's "Recording results" section.

---


## C++ test-suite coverage map

**33 suites, 340 test cases.** This table gives every suite an owner. A suite with no owner
is a silent regression waiting to happen — this is the concrete form of "do not regress
safety". Verify with `ls -d test/test_*`.

| Suite | Safety path | Owner task | Notes |
| --- | --- | --- | --- |
| `test_state_machine` | — | R2-08 | core transitions |
| `test_state_classification` | — | R2-08 | `isBrewState` / `isSteamState` / etc. |
| `test_state_flow_integration` | — | R2-08 | multi-step flows |
| `test_pid_state_transitions` | S11 | R2-08 | stale-flag draining |
| `test_brew_preinfusion_pause` | S5 | R2-08 | valve whitelist during pause |
| `test_pid_mode_water_dispensing` | — | R2-08 | water switch under PID |
| `test_process_controller` | S1, S2 | R2-08 | emergency before PID compute |
| `test_emergency_stop_manager` | **S1, S3** | **R2-05** | 3-count debounce; immediate trip out of range; recovery < 100 °C |
| `test_hardware_water_tank` | **S4** | **R3-03** | pump kill on tank-empty edge |
| `test_sensor_coordinator_water_tank` | S4 | R3-02 | debounce + `TOGGLE` mode |
| `test_water_tank_empty_state` | S4 | R2-08 | `WATER_TANK_EMPTY` transitions |
| `test_sensor_error_state` | — | R2-08 | `SENSOR_ERROR` entry/exit |
| `test_isr_initialization` | **S6** | **R1-07** | heater PWM table + counter wrap |
| `test_temp_sensor_tsic` | S1 | R1-03 | range reject, sentinels 221/222 |
| `test_system_initialization` | S10 | R3-16 | init order, partial-init unwind |
| `test_brew_handler` | S5 | R2-08 | `valveSafetyShutdownCheck` |
| `test_steam_handler` | — | R2-08 | steam valve vs water valve |
| `test_hot_water_handler` | — | R2-08 | hot-water path |
| `test_power_handler` | — | R2-08 | reboot, safe shutdown |
| `test_steam_water_injection` | — | R2-08 | injection during steam |
| `test_backflush_states` | S5 | R2-08 | filling/flushing valve whitelist |
| `test_backflush_mode` | — | R2-08 | cycle logic |
| `test_config` | **S1** | **R2-06** | **must include the `safety.emergency_*` regression test** |
| `test_config_json` | — | R2-06 | import/export round-trip |
| `test_coordinators` | — | R2-08 | coordinator contracts |
| `test_network_coordinator` | — | R3-12 | retry policy |
| `test_ui_coordinator` | — | R2-10 | buffer-ready handshake |
| `test_maintenance_coordinator` | — | R2-08 | shot counter, backflush reminder |
| `test_display_brew_timer` | — | R2-10 | brew timer state machine |
| `test_display_helpers` | — | R2-10 | layout helpers, fixed-width fields |
| `test_wifi_sta_hostname` | S9 | R3-12 | hostname before `begin()` |
| `test_utils` | S7, S8 | R2-04 / R3-10 | retry policy, circuit breaker, watchdog |
| `test_support.h` | S7 | R3-10 | TWDT stub semantics |
| `test/` (root `main.cpp`) | — | R2-04 | harness entry |

**Every suite is accounted for.** Phase 2 (R2-*) owns the portable ones; the hardware-coupled
ones move to Phase 3 with their driver task. No suite is silently dropped — if a suite is
decided to be not-ported, remove its row here and say why in `intentional-diffs.md`.

## Phase 0 — Preconditions (no code)

These are not Rust tasks. They unblock everything else.

| ID | Objective | Depends on | HW | Acceptance |
| --- | --- | --- | --- | --- |
| **R0-01** | Confirm the physical board: exact module (WROOM-32E vs WROVER-32E), silicon revision, flash size, PSRAM presence, and whether the EN↔GND 100 nF auto-reset cap is present. Photograph the module and the board. | — | **yes** | Findings written into [01 §10](../../history/feature-inventory.md), replacing the "still to confirm" table. |
| **R0-02** | Decide the partition rebalance and the asset strategy. Build the current frontend, measure the real LittleFS image size, then compute a new split. **Target is a formula, not a number** (see [07 §1](./07-image-size-budget.md#1-the-problem-stated-once)): maximise `min(app0, app1)` subject to `nvs`/`otadata`/`coredump` byte-identical and `spiffs ≥ S`. An earlier draft asked for "≥ 2 MB per slot" — **that is impossible**: two 2 MiB slots exceed the entire 4,063,232 B region by 131,072 B. Sanity check: max = (4,063,232 − 65,536)/2 = **1,998,848 B ≈ 1.906 MiB/slot** with a 64 KB `spiffs`. | R0-01 | no | A proposed `rust/partitions_4M.csv` checked in with the arithmetic shown, plus the measured SPA size and the embed-vs-mount decision. **Do not** overwrite the root `partitions_4M.csv` — that file stays C++-owned until R4-10. |
| **R0-03** | Decide the scale-support question: formally drop F13 (HX711) and F14 (Acaia BLE), or keep them. Recommendation: drop. | — | no | A one-line decision in [01 §3](../../history/feature-inventory.md) plus a note for the release notes. |
| **R0-04** | Baseline the C++ firmware: run the C++ build + native tests and record the numbers, so parity has a reference. | — | no | `pio run -e esp32_usb` succeeds; `pio test -e native_test` result recorded. **Already verified on 2026-09-28: build succeeds (`firmware.bin` = 1,546,240 B) and 340/340 native tests pass in 55 s.** |

---

## Phase 1 — Feasibility spikes (R1)

**Gate 1 is the point at which ADR-0004 moves from Proposed to Accepted.** Do not start
Phase 2 until R1-01, R1-02, R1-03, and R1-07 have all passed. R1-04, R1-05, R1-06 are
required before the features they gate are started, but they may run in parallel with
Phase 2.

Each spike is a **throwaway `examples/` binary in the workspace**, deleted or moved to
`docs/history/spikes/` afterwards. A spike is not production code.

### R1-01 — Workspace bootstrap and toolchain proof ⚠ CRITICAL PATH

- **Objective:** prove that `esp-idf-svc` 0.53.0 builds, links, boots, and runs on this
  host for `xtensa-esp32-espidf` at ESP-IDF v5.5.5. Record the binary size.
- **Prereqs:** R0-04.
- **Files:** `Cargo.toml` (workspace), `rust-toolchain.toml`, `.cargo/config.toml`,
  `mise.toml` (extended), `justfile`, `crates/cc-firmware/` (minimal), `partitions_4M.csv`.
- **Steps:**
  1. `rust-toolchain.toml` with `channel = "esp"`.
  2. `.cargo/config.toml` with `[build] target = "xtensa-esp32-espidf"`,
     `[unstable] build-std = ["std", "panic_abort"]`, and
     `[idf] partition_table = "rust/partitions_4M.csv"`. Do **not** port the C++ `sdkconfig.defaults` partition settings, and do **not** copy the upstream `esp-idf` repo's own `partitions.csv` (it declares `nvs 0x6000`, `phy_init 0x1000`, `factory 3M` and would lose the OTA layout).
  3. Workspace with `esp-idf-svc = "=0.53.0"`, `esp-idf-hal = "=0.47.0"`,
     `esp-idf-sys = "=0.38.1"`.
  4. Extend `mise.toml` per [05 §2](./05-tooling-and-workflows.md#2-misetoml-additions);
     add the `setup`, `doctor`, `fmt`, `fmt-check`, `lint`, `test`, `build-*`, `flash`,
     `mon`, `list-ports`, `identify` recipes.
  5. Minimal `main`: init, configure GPIO 2/17/27 as outputs, write them inactive, log
     over UART at 115200, and **read the pin levels back and assert they are inactive**.
  6. Measure `target/xtensa-esp32-espidf/release/cc-firmware` size against the app slot.
  7. If the image exceeds the slot, apply R0-02's rebalance and re-measure.
- **Acceptance:**
  - `just setup` succeeds on a clean machine.
  - `just build-esp32` succeeds.
  - `just lint-esp32` succeeds with `-D warnings`.
  - `just flash <explicit-port>` succeeds.
  - `just mon <explicit-port>` shows the boot log and the pin readback.
  - **Recorded in the task comment:** binary size, app slot size, headroom.
- **HW:** yes (flash + monitor). Build-only validation possible without.
- **Rollback:** delete the workspace files; the C++ firmware is untouched.
- **Uncertainty:** the Xtensa `esp` toolchain may not install cleanly on macOS arm64
  (U1, U9). If `espup install` fails, **stop and report** — do not work around it by
  switching to a RISC-V target, because the production board is Xtensa.

### R1-02 — Executor decision

- **Objective:** determine whether to use `embassy-executor`, `edge-executor` 0.5.0, or
  plain FreeRTOS tasks, and record the evidence.
- **Prereqs:** R1-01.
- **Files:** a spike example; the ADR-0004 §"Concurrency decision" section.
- **Steps:**
  1. Build the same three-task skeleton (control / network / housekeeping) three ways.
  2. `embassy-executor`: measure wake latency and check for hangs under load.
  3. `edge-executor` 0.5.0: same, plus an ISR-wakeup test.
  4. Plain `std::thread` on FreeRTOS tasks: same.
  5. Read the CHANGELOG for `edge-executor` 0.5.0 and check whether issues #619/#630 are
     resolved.
- **Acceptance:** a short ADR addendum with measurements and a decision. **The
  architecture is designed to be executor-agnostic**, so the worst case is "keep FreeRTOS
  tasks" and nothing else changes.
- **HW:** no.
- **Uncertainty:** U2 — no authoritative recommendation exists. This is a
  document-the-evidence task as much as a benchmark.

### R1-03 — DS18B20 / 1-Wire driver (⚠ re-scoped 2026-09-28)

- **RE-SCOPED 2026-09-28.** This was "TSIC-306 / ZACwire, the highest-risk spike that can
  invalidate the whole approach". Two measurements invalidate that framing:
  1. **A DS18B20 is physically fitted** (family `0x28`, 11-bit, ROM
     `0x41af78cdaa376928`, live readings 22.9–23.3 °C). Not a TSIC-306.
  2. **The recovered firmware shipped 1-Wire only** and logged *"config asks for Tsic306
     but only the DS18B20 driver exists; reading the 1-Wire bus anyway"* (08 §3).
  So TSIC-306 is a **config option that was never implemented**, not the shipping sensor.
  This task is now: **port DS18B20 correctly, and decide what
  `hardware.sensors.temperature.type = TSIC_306` does** (implement, or reject at
  config-validation time — the oracle's fail-closed config validation is the precedent,
  08 §4.1).
- **Objective:** read a DS18B20 reliably enough for temperature control, including every
  safety behaviour the C++ version has, plus the TSIC-306 decision above.
- **Prereqs:** R1-01. **HW required.**
- **Reference material:** the IST AG app note `ATTSic_E2.3.0.pdf`; the existing
  `ZACwire` C library; `src/hardware/tempsensors/TempSensorTSIC.cpp`.
- **Steps:**
  1. **1-Wire first.** Port the C++ DS18B20 path: reset/presence, `0xCC` skip-ROM,
     `0x44` convert, `0xBE` read scratchpad, CRC8, 11-bit resolution (~375 ms),
     non-blocking, 400 ms cadence. Note `esp_idf_hal::delay` rounds `delay_ns` up to 1 µs,
     which is adequate for 1-Wire (3–65 µs slots) but **not** for HX711.
  2. **TSIC-306, only if the decision is "implement".** The protocol, from the IST app
     note: 8 kHz, 125 µs bit window, `Tstrobe` = 62.5 µs, duty-cycle encoding (start 50 %,
     `1` 75 %, `0` 25 %), two packets with even parity, `T = DS/2047 · 200 − 50`, and
     **a ≥ 128 kHz sampling rate for acquiring the start bit** (app note; the plan
     previously omitted this, the most concrete timing constraint in the protocol).
  2. Implement a **falling-edge ISR** that measures `Tstrobe` on the start bit then
     samples after each of the next 9 falling edges. Verify the ISR stays within budget
     (~2.7 ms worst case per the app note).
  3. Port the **adaptive max-change-rate** logic: 200 °C/sample initially, latching to
     5 °C/sample after two consecutive in-range readings within 5 °C of each other.
  4. Port the sentinels: `222` = read failed, `221` = not connected.
  5. **Port the range reject `temp <= 0.0 || temp >= 180.0`.** This is safety-relevant:
     it exists to stop a −2.9 °C glitch from tripping emergency stop.
  6. **Do NOT try to read the sensor from both firmwares at once** — there is one
     TSIC-306 on GPIO 16 and only one chip is flashed at a time. Instead: flash the
     **C++** firmware, record a **10-minute** reference log to
     `docs/history/spikes/tsic-reference.csv`
     (`t_ms, raw_ds, temp_c, accepted, reject_reason`); then flash Rust and record the
     same log; then diff offline.
  7. Record the deliberate divergence: the app note and this task specify a
     **falling-edge** ISR, but the production `ZACwire` library times on **rising**
     edges (`ZACwire.cpp:39`). Note it in `intentional-diffs.md`.
  8. **ISR architecture, reconciled with 04 §2.** 04 requires ISRs to do "nothing
     beyond one GPIO write". A 2.7 ms bit-sampling ISR violates that. So: the ISR
     **timestamps edges into a fixed-size ring and signals via
     `hal::task::notification`; the decode happens in the control task.** The app
     note's 2.7 ms figure is the worst case for a callback-style driver and is the
     right thing to *compare against*, not to build.
- **Acceptance** (measurable):
  - Over the recorded 10-minute window, **≥ 99 % of Rust samples within ±1.5 °C** of
    the interpolated C++ reference.
  - The accept/reject decision agrees on **≥ 99.5 %** of samples.
  - The adaptive change-rate latch (200 → 5 °C/sample) transitions at the same sample
    index as the C++ reference; both thresholds are asserted as named constants.
  - `just test` covers the decode as a table-driven host test over **captured edge
    timestamps** committed as test data.
  - ISR worst-case duration measured and recorded; the control task is not starved.
- **HW:** yes.
- **Rollback:** the C++ firmware is unchanged; if the spike fails, the decision in
  ADR-0004 must be revisited (keep the C++ sensor path, or change the sensor).
- **Uncertainty:** U3. **This is the one spike that can invalidate the whole approach.**

### R1-04 — OLED rendering fidelity

- **Objective:** prove the 128×64 layout can be reproduced without U8g2.
- **Prereqs:** R1-01 (build only; the render test is host-only).
- **Steps:**
  1. `ssd1306` 0.10.0 + `embedded-graphics` 0.8 over `hal::i2c`.
  2. Convert the `profont` and `fub` glyph atlases from U8g2 to `ImageRaw` data.
  3. Port `DisplayLayoutUtils.h`: `draw_str_right_in_box`, `draw_str_centered_in_box`,
     `layout_bar_label_cluster`.
  4. Render all 6 templates plus the system screens into a host-side framebuffer.
  5. **Assert against the AGENTS.md OLED rules:** everything fits in 128×64; no
     overlapping rows; counting fields use a fixed pixel width so digits do not shift;
     bar and label share a vertical midline; bottom rows anchored from `DISPLAY_HEIGHT`.
  6. Produce PPM goldens and diff them against screenshots from the C++ firmware.
- **Acceptance:** zero clipping, zero overlap, and a visual diff of each template that
  the reviewer signs off. `just snapshot-display` regenerates the goldens.
- **HW:** no for the layout work; yes for the final on-device comparison.
- **Uncertainty:** U4. `ssd1306` keeps the framebuffer private, so the fixed-width
  helpers must be written against `DrawTarget` primitives — plausible, not verified.
- **Note:** SH1106 (config `hardware.oled.type = 1`) needs a separate path; `ssd1306` does
  not support it and `sh1106` 0.5.0 is stuck on `embedded-hal 0.2`. **Decide: port
  `sh1106` to `embedded-hal 1.0`, or drop SH1106 support and document it.** Do not leave
  this dangling.

### R1-05 — HTTP server + SSE

- **Objective:** determine how the `/events` SSE stream is delivered.
- **Prereqs:** R1-01.
- **Much of this is now answered** (2026-09-28, verified against `esp-idf-svc` 0.53.0
  `src/http/server.rs`). An earlier draft budgeted a ~20-line `esp-idf-sys` FFI shim; that
  work **already ships in the dependency**:
  - `EspHttpConnection::write(&[u8])` → `httpd_resp_send_chunk` (`server.rs:1126`),
    terminated by the zero-length chunk in `complete()` (`:1164`). **Chunked streaming is
    already a public API.**
  - `EspHttpConnection::raw_connection() -> &mut EspHttpRawConnection` → `write`/`write_all`
    call `httpd_req_to_sockfd` (`server.rs:815, 826`) then a raw `write(2)`. **That is
    exactly the "equivalent of `req_to_sockfd`" the earlier draft wanted shimmed.**
  - `ws_handler` is present (`server.rs:1623`).
  So U5 is downgraded to the *only* genuinely open question below.
- **Steps:**
  1. Stand up `EspHttpServer` with one JSON endpoint and one static file.
  2. **Q1:** does a browser `EventSource` accept chunked frames from `connection.write()`?
  3. **Q2:** does `raw_connection().write_all()` let you emit SSE frames with no
     `Transfer-Encoding` at all, sidestepping the WHATWG chunked-SSE problem
     ([espressif/esp-idf#14121](https://github.com/espressif/esp-idf/issues/14121) — the
     "preamble only" API there was requested and never landed)?
  4. If both fail, fall back to a **WebSocket** and accept the `ui/` change.
- **Acceptance:** a browser-equivalent client receives events for **10 minutes with zero
  drops**; heap stays flat (watch for the ADR-0002 pattern); the chosen mechanism is
  recorded with the Q1/Q2 answers.
- **HW:** recommended (a laptop is enough; the device is not strictly required).
- **Uncertainty:** U5.

### R1-06 — Wi-Fi provisioning feasibility

- **Objective:** determine the provisioning mechanism (P1/P2/P3 in
  [05 §5](./05-tooling-and-workflows.md#5-wi-fi-provisioning)).
- **Prereqs:** R1-01.
- **⚠ Landmine, found 2026-09-28 and not previously considered.** `esp-wifi-provisioning`
  depends on `esp-idf-hal` with `features = ["rmt-legacy"]` (non-optional). Cargo
  feature unification turns that **on for the whole graph**, and `not(feature =
  "rmt-legacy")` is exactly what gates `pub mod onewire`
  (`esp-idf-hal/src/lib.rs:54-59`) and the new `pub mod timer` (`:131-144`).
  **Adopting the captive-portal crate therefore removes `hal::onewire` and the GPTimer
  module from the entire dependency graph.** That would break R3-06 (DS18B20) and the
  R1-07 GPTimer fallback.
  The check must be step 0, not step 1.
- **Steps:**
  0. **Verify the feature-unification consequence** by adding the crate to a scratch
     project and running `cargo tree -e features -p esp-idf-hal | grep rmt`. If
     `rmt-legacy` is enabled, **do not use the crate** — hand-build the portal instead
     and record why.
  1. Otherwise try `esp-wifi-provisioning` 0.1.1 against `esp-idf-svc` 0.53 (it pins
     `^0.51`; patch bump, then vendoring).
  2. If it fails, assess hand-building on `svc::wifi` + softAP + DNS interception.
  3. Confirm credentials land in NVS and that `clear_credentials()` works.
  4. Confirm the portal **does not** collide with the SPA server for port 80 — they both
     want it. Resolve: the portal only runs when there are no credentials, so the
     sequence is portal → reboot → SPA.
- **Acceptance:** a phone can join the AP and provision; the device reconnects; a
  reboot does not re-enter the portal. **No credential appears in any log at any level.**
- **HW:** yes.
- **Uncertainty:** U6. **P3 (USB) is unavailable on this hardware** — do not plan around
  it.

### R1-07 — Heater output method

- **Objective:** decide between LEDC hardware PWM and a GPTimer ISR.
- **Prereqs:** R1-01.
- **Steps:**
  1. Drive a **dummy load** (an LED, or a resistive heater with the boiler disconnected)
     from LEDC. Measure the duty cycle with a scope or a logic analyser.
  2. Drive the same from a GPTimer with `auto_reload_on_alarm` at 10 ms, ISR doing only
     `set_level` + counter increment. Measure jitter and worst-case ISR latency.
  3. Unit-test the translation `(pid_output, counter) -> level` on the host, covering
     `pid_output = 0`, `pid_output = window`, and every wrap boundary.
  4. **Confirm the relay board's active-high/active-low behaviour in the field** (R0-01)
     and honour `hardware.relays.*.trigger_type`.
- **Acceptance:** the chosen method produces a duty cycle matching the PID output within
  1 % across the full range, and the host table test passes.
- **HW:** yes, but **only with the boiler safely disconnected** — see the safe test
  procedure below.
- **Uncertainty:** U7.

> ### ⚠ Safe test procedure for R1-07
>
> This task energises the heater output. Until the boiler is confirmed disconnected:
>
> 1. **Physically disconnect the boiler** at the machine. Do not rely on a switch.
> 2. Put the ESP32 in `PidDisabledState` and confirm the heater command is zero.
> 3. Drive the output with a dummy load only.
> 4. Never run a brew, backflush, or manual-flush cycle during this spike.
> 5. If a test requires the real boiler, it needs a written procedure reviewed before
>    it starts, a person present, and a hand on the power switch.
>
> R1-03 has the same constraint: a temperature sensor on a **live boiler** can act on a
> real heater. Do not connect a sensor spike to a live boiler.

##### R1-07 progress, 2026-09-28 — the decision is made, the hardware test is not done

| Step | Status |
| --- | --- |
| 0. Verify `hal::ledc`'s real API from the installed source | **done** — `esp-idf-hal-0.47.0/src/ledc.rs`; the signatures used are `LedcTimerDriver::new(timer, &TimerConfig)`, `LedcDriver::new(channel, &timer_driver, pin)`, `set_duty(u32)`, `get_duty()`, `get_max_duty()`. `set_duty` clamps to `max_duty` **silently**. |
| 0b. Derive the carrier from the C++'s **transition rate**, not its ISR rate | **done, and it changed the answer.** The ISR fires 100×/s but the level changes 0 or 2×/s; `cc_domain::heater::cpp_transitions_per_second` computes that from the transcribed predicate. `f ≤ 1 Hz`. The earlier 100 Hz specification is withdrawn — see 04 §5 and `intentional-diffs.md` #5. |
| 1. Drive a dummy load from LEDC, measure with a scope | **NOT DONE.** No dummy load and no scope are attached to this machine, and the board's boiler-disconnection state is unconfirmed. Left un-run rather than claimed. |
| 2. Same from a GPTimer, measure jitter | **NOT DONE.** The `GPTimer` ISR is what the firmware runs, but nothing has been measured against a dummy load with a scope. The second transport (`LedcPwm`) and the `HeaterDuty` seam it needed were **deleted** — zero construction sites, and LEDC panics this chip — so there is no half-written implementation standing in the way. |
| 3. Host-test the `(pid_output, counter) -> level` translation at 0, at window, and every wrap boundary | **done** — `cc_domain::heater`, 16 tests: `the_tick_table_is_the_cpp_isr` walks all 100 counters at duty 0 and at full duty; `the_ten_millisecond_quantisation` pins both off-by-ones; `duty_counts_at_the_chosen_resolution` pins monotonicity; `the_chosen_carrier_reproduces_the_cpp_on_time` pins the 10 µs on-time tolerance and `…_at_the_analytic_bound` the 3.8 µs floor. |
| 3b. Host-test that the carrier does not switch the contactor more than the C++ does | **done** — `the_carrier_does_not_switch_the_contactor_more_than_the_cpp_does` computes both sides and requires them **equal** at every half-millisecond of the PID range, and never above 2/s. Also `the_minimum_pulse_is_the_cpp_ten_millisecond_step` and `full_duty_is_distinguishable_from_disabled`. |
| 4. Honour `hardware.relays.*.trigger_type` | **partly** — `cc_safety::validate_config` refuses a `LOW_TRIGGER` **heater** relay outright (08 §4.1). R3-03 applies the trigger type to the pin. |

**Decision: LEDC, 1 Hz, `Bits17`, low-speed mode; the 1 Hz chopper window is
kept so the PID is unchanged.** Recorded in
[`intentional-diffs.md` #5](../../history/divergences.md)
and in 04 §5.

**R1-07 is NOT complete.** Its acceptance criterion is measured duty within 1 %
across the full range, and that requires steps 1 and 2. What *is* established: the
arithmetic is host-tested, the frequency/resolution pair was derived from
ESP-IDF's own divider maths rather than guessed, the firmware builds and links
with the carrier at duty 0, and the latching gate plus deadman are implemented
from the recovered oracle (08 §3, §4).

**Open, and it is a hardware question that nobody has answered.** The carrier now
matches the C++'s contactor duty, which removes the specific worry that motivated
the 100 Hz design, but three things remain **unmeasured** and are recorded as
measurements to take rather than assumptions to make:

* the contactor's **minimum on/off time** — the software guarantees it never
  requests a pulse narrower than the C++'s own 10 ms step, but whether 10 ms is
  inside the contactor's ratings is a datasheet/measurement question;
* **whether a hardware-PWM output is acceptable to the coil at all** at 1 Hz, a
  frequency the C++ never *produced* even though its average was 1 Hz;
* the **realised frequency and duty on the pin** — no scope has been attached;
  and, relatedly, whether 1 Hz is the best point on the
  wear-versus-duty-resolution curve, which is a decision for someone with the
  machine in front of them.

**R1-07 stays open until steps 1 and 2 are run with the boiler disconnected.**

### R1-08 — Parity baseline capture (required before Gate 1)

- **Objective:** create the reference the migration is measured against. Currently
  `just parity` is required at **every** gate but `scripts/parity/run.sh` is not created
  until R4-03 — in Phase 4. Gates 1, 2 and 3 are un-passable as written.
- **Prereqs:** R0-04 (a working C++ build), R0-01 (a connected device).
- **Files:** `scripts/parity/run.sh`, `docs/history/scenarios/*.yaml`,
  `docs/history/baseline/` (captured C++ reference output),
  `docs/history/divergences.md`.
- **Steps:**
  1. **Define the scenario format** — it is currently undefined, which is the real hole.
     A declarative YAML file of timed stimuli:
     `[{ at_ms, kind: rest|mqtt|button|ota, method, path, body, expect }]`, plus a capture
     spec listing the state-transition log and the endpoints to snapshot.
  2. **Author the minimum scenario set**, one per behaviour that must not regress:
     `cold_boot`, `brew_by_time`, `brew_aborted_mid_preinfusion`,
     `brew_aborted_mid_flow`, `overtemp_trip`, `overtemp_recovery`,
     `water_tank_empty_mid_brew`, `backflush_full_cycle`, `steam_on_off`,
     `standby_wake`, `ota_start_from_idle`, `ota_start_during_brew`.
     Every safety path S1-S11 has at least one scenario.
  3. Run each against the **C++** firmware and commit the captured reference to
     `docs/history/baseline/`.
  4. Write `intentional-diffs.md`, seeded with the four known C++ bugs that the Rust port
     **fixes** (so a diff there is expected, not a regression):
     the unregistered `safety.emergency_temp`/`hysteresis`; the OTA actuator gap; the
     blocking pressure read; the log-only `enterSafeMode`/`exitSafeMode`. Add the
     LEDC period change from R1-07 and the TSIC rising→falling edge change from R1-03.
- **Acceptance:** `just parity <port> <host>` runs the scenarios against the C++ firmware
  and produces an **empty diff against the committed baseline**. `intentional-diffs.md`
  exists and is non-empty. `run.sh` exits non-zero on any unexplained diff.
- **HW:** yes.
- **Why Phase 1:** without this, "parity" is an unmeasurable word, and it is the
  migration's primary correctness instrument.

#### R1-08 status, 2026-09-29 — the harness exists; the C++ baseline does not

| Step | Status |
| --- | --- |
| 1. Scenario format | **done** — [`10-scenario-format.md`](../../history/scenario-format.md), implemented by `crates/cc-parity/src/scenario.rs`. Seven stimulus kinds, a capture spec, and nine assertion kinds. |
| 2. Scenario set | **done** — 17 scenarios in `docs/history/scenarios/`, covering all twelve named here plus S1's no-debounce path, S4's refill, S7's reboot path, S6's heater bound and S9's watchdog. Every dry_run scenario is **executed and its assertions checked** by `cc-parity`'s own tests, so a broken scenario fails `just parity-test` rather than a phase gate. |
| 3. C++ baseline | **NOT DONE.** `docs/history/baseline/cpp/` is empty and `just parity` reports every scenario `BASELINE-MISSING` and exits **2**. Not a skip: nothing has been compared with anything. See the reasoning in [`baseline/README.md`](../../history/baseline/README.md) — capturing one means flashing the C++ image and running its control loop against a real boiler, which needs the reviewed safe-test procedure and a human present. **No baseline was fabricated.** |
| 4. `intentional-diffs.md` | **done** — the file existed; R1-08 added the five machine-readable `ledger` blocks the runner classifies against, each naming a heading in the same document so the prose and the ledger cannot drift. |

**The safety problem, and how it was solved.** Twelve of the seventeen scenarios would
energise a pump, a valve or a heater. They are `dry_run`: `cc-parity` drives the **real**
reducer and the **real** safety monitor in process on the host, and the `Actuators` it
hands them is a `Vec` of call names. There is no GPIO in the crate, no `cc-hal-esp32` in
its dependency tree, and no path from a scenario file to a pin — so `brew_by_time` really
does emit `Effect::EnablePump`, the harness really does assert on it, and nothing is
energised. That is what makes the set runnable with no device attached.

**Two findings the harness produced before any baseline existed**, both now pinned:

* The safety monitor has to run on the **sensor cadence** (400 ms,
  `constants/Timing.h:42`), not the 10 ms control loop. Running it every tick made
  S1's three-reading debounce trip on one reading repeated 30 times in 300 ms, so the
  most safety-relevant timing constant in the firmware was untestable. `overtemp_trip`
  now trips at 800 ms, on the third reading, as the C++ does.
* **`OpenSteamValve` is never emitted by the reducer**, because the steam valve is a
  solenoid the machine cannot open by itself. The S5' whitelist therefore acts in the
  *close* direction, and `steam_on_off` asserts that. Asserting an open would have been
  asserting something neither firmware does.

**Gate 1 is still not passable**, and the runner is what says so: `just parity` exits
non-zero with `INCOMPLETE: 13 scenario(s) have no C++ baseline`. The C++ half of the
harness — driving a `dry_run` scenario through the C++ state machine — is R4-03's work
and is not built.

### Gate 1

- **All four of R1-01, R1-02, R1-03, R1-07 pass, and R1-08 has produced a committed
  baseline.**
- ADR-0004 status changes from *Proposed* to *Accepted*, with the R1-02 and R1-07 results
  filled in.
- If R1-03 fails: **stop.** Do not proceed. Escalate per ADR-0004 §"Decision is
  conditional".

---

## Phase 2 — Portable domain (no hardware)

Nothing here touches hardware. All of it is host-testable, and all of it is the majority
of the behavioural surface.

| ID | Objective | Depends on | HW | Acceptance |
| --- | --- | --- | --- | --- |
| **R2-01** | Pin the exact `ldproxy` / `embuild` version matching `esp-idf-sys` 0.38.1 and record it in `mise.toml` + CI. | R1-01 | no | `just setup` works from a clean machine with a pinned version. |
| **R2-02** | Add the CI lint-hygiene greps (no `#![allow(`, no bare `esp_idf_` in portable crates) and enable `clippy::pedantic` as deny. | R1-01 | no | The greps are in `.github/workflows/rust.yml` and fail on a deliberately added violation. |
| **R2-03** | Apply the R0-02 partition rebalance. Keep `nvs`, `otadata`, `coredump` byte-identical. | R0-02, R1-01 | no | `just build-esp32` produces an image that fits with recorded headroom; `cargo espflash` flashes without error. |
| **R2-04** | `cc-domain`: units, enums, `MachineState` (18 variants), `ErrorCode`, and a port of the Arduino PID library. | Gate 1 | no | `cargo test -p cc-domain`; PID output matches the C++ for a fixed input sequence. |
| **R2-05** | `cc-safety`: the `SafetyMonitor` — S1 (3-count debounce, immediate trip on out-of-range), S2 (emergency latch), S3 (recovery below 100 °C), S4 (water tank), S5 (`water_flow_allowed` with a `match` that has no `_` arm, so a new water-flow state is a compile error). **Plus the two fail-closed rules recovered from the oracle (08 §4.1), which the plan did not have:** (a) **cross-parameter validation** — `safety.emergency_temp` must exceed `steam.setpoint + safety.emergency_hysteresis`, else the machine trips itself during normal steam use; (b) **refuse a stored config that fails validation** — discard it, run defaults, and refuse to store an unsafe one. | Gate 1 | no | `cargo test -p cc-safety`; every branch of 01 §6 is a named test; adding a water-flow state without updating the whitelist **fails to compile** (tested by a `trybuild`-style case or a documented manual check). |
| **R2-06** | `cc-config`: the 96-parameter registered schema (98 after the `safety.emergency_*` fix), the `ConfigStore` trait, NVS-independent JSON import/export, and a `Secret<T>` wrapper whose `Debug`/`Display` redact. | Gate 1 | no | `cargo test -p cc-config`; round-trip; defaults; the `safety.emergency_temp` registration bug from [01 §10](../../history/feature-inventory.md) is **fixed**, with a test that fails against the old behaviour. |
| **R2-07** | Drop scale support (F13/F14) per R0-03. | R0-03 | no | No HX711 or BLE code in the Rust tree; documented in the release notes. |
| **R2-08** | `cc-machine`: the state machine as an **Elm-style reducer** ([04 §3.1](../../history/target-architecture.md)) — `reduce(state, ctx, event) -> (state, Vec<Effect>)`, 18 states, per-state `on_entry`/`on_exit`/`update` per ADR-0003, the global guards, and the handlers. **Do not** port `LoopManager::update()` as-is: its eight ordered steps reaching into ten-plus subsystems ([01 §4](../../history/feature-inventory.md)) is the defect being fixed. | R2-04, R2-05, R2-06 | no | The C++ suites `test_state_machine`, `test_pid_state_transitions`, `test_brew_preinfusion_pause`, `test_steam_water_injection`, `test_backflush_states`, `test_backflush_mode`, `test_state_flow_integration`, `test_power_handler`, `test_brew_handler`, `test_hot_water_handler`, `test_steam_handler` are ported and pass. Plus an **exhaustive `state × event` table** over the reducer — every pair reaches a named verdict. Every state that energises hardware disables it in `on_exit` **and** re-asserts it in `update`. `applier.apply()` is the only function that calls `Actuators`. |
| **R2-09** | **Host micro-benchmark** of the reducer in `cc-machine`. This is pure CPU, so it is legitimately hardware-free. | R2-08 | no | `cargo bench --bench reducers -p cc-machine`: reducer `step()` ≤ **50 µs at p99** across the full 18-state × N-event table. Committed as a recorded baseline. **Do not optimise before measuring.** |
| **R2-09b** | Decide the display/SH1106/optional-feature **feature-flag set** from the first size measurement — see [07 — Image size budget](./07-image-size-budget.md). | R2-09 | no | `rust/partitions_4M.csv` and the feature set are agreed, and `intentional-diffs.md` lists anything dropped to fit. |
| **R2-10** | `cc-display`: framebuffer, `DrawTarget`, ported glyphs, the layout helpers, 6 templates, and the golden-image harness with the AGENTS.md fit/spacing assertions. | R1-04 | no | `cargo test -p cc-display`; goldens regenerated with `just snapshot-display`; reviewer signs off on the diff against the C++ render. |

**R2-10 status: engine parity and the golden harness are done; the
template-layout sign-off is not.** See
[`docs/display-parity.md`](../../display/parity.md) for what is proven and
what is not. Three gaps, in the order they should be closed:

1. **The goldens record the port, not the C++.** All 48 images pass, and
   `just test-display-parity` reports zero differing pixels against real U8g2
   across eleven scenarios. But the *normal* layouts are not compared against a
   rendered C++ frame, which is the task's own exit criterion. Closing it needs a
   `SystemContext` stub small enough to be obviously faithful.
2. **The goldens have already earned their keep.** The first `modern` render
   showed the large `fub20` temperature clipped off the top of the panel, because
   nothing called `OledDriver::prepareDisplay`'s `setFontPosTop` /
   `setFontRefHeightExtendedText`. Fixed in `Display::prepare_display`. Expect
   more of the same in the Modern row map and the bottom-row bar.
3. **`DrawTarget` is not implemented** and the deviation is recorded as *open*,
   not decided — see [`intentional-diffs.md` §6](../../history/divergences.md). It
   should be resolved before the templates are finished, because the resolution
   may change the drawing path.

### Gate 2

- **Size gate** (see [07 — Image size budget](./07-image-size-budget.md) §5): `just size`
  recorded with per-crate attribution; `just size-check` passes.
- `cargo test` over the five portable crates green.
- `just fmt-check`, `just lint`, `just lint-esp32` green.
- CI greps in place and demonstrated to fail on a violation.
- R2-08's ported suites all pass.

---

## Phase 3 — Hardware abstraction (hardware required for most)

| ID | Objective | Depends on | HW | Acceptance |
| --- | --- | --- | --- | --- |
| **R3-01** | `cc-hal-esp32`: `Board` trait + the ESP32-DevKitC impl, the full pin map from [01 §2](../../history/feature-inventory.md), and a `const` pin assertion. | Gate 2 | no | Compiles; a deliberately wrong pin fails to compile with a clear message. |
| **R3-02** | `GpioIn` with 20 ms debounce and 500 ms long-press, matching `IOSwitch.cpp`. The water-tank switch is a `GpioIn` too. | R3-01 | yes | Debounce and long-press match the C++ on hardware; water-tank-empty kills the pump within one tick. |
| **R3-03** | `Actuators`: the single owner of pump, valve, heater. `ValveState` enum preserved. Emergency latch and water-tank interlock checked **inside** the methods, not at call sites. **Plus the deadman heartbeat (08 §4): the heater output is gated by a latching heartbeat — the plan currently has no equivalent and relies on a 5 s watchdog reset, which leaves the heater energised far too long.** Also **reject `LOW_TRIGGER` for the heater relay**: an undriven GPIO at reset would energise it, so a `LOW_TRIGGER` heater config must be refused at config-validation time (08 §4.1) — this is a real hole in the current C++ firmware, which honours the setting. | R3-01, R2-05 | yes | Every method refuses when the latch is set; `close_*` always works. A test that tries to bypass via a state cannot. |
| **R3-04** | `HeaterOutput` per the R1-07 decision. `heater_enabled` boolean **deleted**. | R1-07, R3-03 | yes | ⚠ Safe test procedure. Duty cycle correct; no other task may write the pin. |
| **R3-05** | `Abp2Pressure` over I2C with the 10 ms conversion wait made **non-blocking** (the C++ version blocks the loop 20 % of the time). | R3-01 | yes | Pressure matches the C++ reading; the control loop is no longer stalled. |
| **R3-06** | `OneWireDs18b20` — port the C, non-blocking, 11-bit, 400 ms cadence. | R3-01 | yes | Matches the C++ reading; disconnected/short/open faults are detected. Only needed if `temp-ds18b20` is enabled. |
| **R3-07** | `Tsic306` — the R1-03 spike promoted to a real driver. | R1-03, R3-01 | yes | The R1-03 acceptance criteria hold in the real crate. |
| **R3-08** | `NvsStore`: `impl ConfigStore`. **No C++ key compatibility** (decided 2026-09-28). Rust defines its own `cc.`-prefixed key namespace. A C++-written NVS is *expected* to be ignored and overwritten with defaults. | R2-06, R3-01 | yes | A NVS blob written by the C++ firmware is ignored and all 96 registered parameters fall back to defaults, **and that is the expected result** (a test asserts it). Rust's own keys round-trip. Add the "config resets on upgrade" line to the release notes. |
| **R3-09** | Display driver: `ssd1306` over I2C, power save, rotation, and the 100 ms refresh. | R1-04, R2-10, R3-01 | yes | On-device render matches the golden; standby power-save blanks the panel. |
| **R3-10** | Watchdog: `TWDTDriver`, 5 s, panic on trigger, subscribed and fed by the control task only. | R1-01 | yes | A deliberate infinite loop in the control task reboots the device; a network stall does not. |
| **R3-11** | Logger: ring buffer, telnet server, and the ADR-0002 **30 KB heap shed**. | R1-01 | no | Under heap pressure the telnet client is disconnected rather than the device crashing; static RAM is counted. |
| **R3-12** | Wi-Fi STA, hostname-before-connect ordering, and the retry/circuit-breaker policy from `CleverCoffeeWiFiManager.cpp`. | R1-01 | yes | Connects, retries, and falls back to offline mode exactly as the C++ does. |
| **R3-13** | MQTT: `EspMqttClient`, will, subscribe, incremental publishing within a time budget, and HA discovery every 300 s. | R3-12 | yes | HA sees the same entities as the C++ firmware; a brewery in progress does not publish. |
| **R3-14** | HTTP: all 24 route registrations (20 `/api/*`, the `/` redirect, `/ui`), the 6 `serveStatic` mounts, and the `/events` SSE stream, static SPA from an embedded bundle, CORS and auth, `AsyncJsonResponse`-equivalent streaming for the large responses. | R1-05, R3-12 | yes | Endpoint-by-endpoint parity with the C++ `/api` responses; `/api/parameters?filter=all` returns complete JSON **with a telnet client connected** (the ADR-0002 crash). |
| **R3-15** | OTA: espota equivalent + HTTP upload + URL update, with `safe_hardware_shutdown` (not just `disable_heater`) and watchdog suspend/resume. **PARTIAL, 2026-10-04** — the two upload endpoints and `/api/ota/status` are implemented and stream to flash in 4 KiB chunks (`a7722161`, `cbfeb091`); `/api/ota/url` and espota/port 3232 are **not**. S8 is closed and stricter than the C++ (refused while water or steam flows; full safe shutdown through the applier on the control task). **Not verified on hardware**, and rollback is off — both stated in [`intentional-diffs.md` §28](../../history/divergences.md). The remainder needs an HTTP client and a second long-lived task. | R3-12, R3-03 | yes | An OTA started from every state leaves pump and valve off. **This fixes the gap in [01 §6](../../history/feature-inventory.md).** |
| **R3-16** | Startup sequence per [04 §4](../../history/target-architecture.md), including the pin readback assertion and the post-boot heater-command-zero check. | R3-01, R3-02, R3-03, R3-04, R3-05, R3-08, R3-10, R3-12 | yes | Startup is a **host-testable boot state machine with injected failure points**, so every failure path before the display exists is exercised without hardware: relays off, halts, no reboot. The on-device end-to-end run happens at R4-01 (the control task does not exist until then). Do **not** depend on R3-13/R3-14/R3-15 — they start *after* the sequence R3-16 orchestrates. |

| **R3-17** | **HX711 scale driver (KEEP — decided 2026-09-29).** `hx711` 0.7.0 is
  `embedded-hal 1.0` + `nb` and non-blocking, but 02 §HX711 records it cannot be driven from
  the control task: the 25/100/128 conversions need sub-µs-stable clock edges. **Run it on
  its own FreeRTOS task at a high priority**, and hand samples to the machine through a
  queue — do not reintroduce a blocking read into the tick. Port
  `HX711Scale.cpp` faithfully: two channels (GPIO 32/25 data, GPIO 33 shared clock),
  calibration factor, `known_weight`, tare, and the **bounded spin loops at `:44,51` that
  01 §5 lists as unbounded** — those get real timeouts in Rust rather than a copy.
  **Construct it at boot.** The C++ never does (09 §23), so there is no behaviour to match
  and no C++ baseline for this task; it is new, working functionality. | R3-01, R3-16 | yes | With a scale on GPIO 32/25/33 the reported weight is stable and within the device's documented accuracy; tare survives a reboot; the control tick is **unaffected** while sampling (measure both); a disconnected data line reports a fault instead of blocking. |
| **R3-18** | **Acaia BLE scale driver (KEEP — decided 2026-09-29).** NimBLE via
  `esp_idf_svc::ble`, which 02 §BLE-scales confirms is available in 0.53.0. `BluetoothScale`
  and the `updateConnection` reconnect loop are the reference. **Note the earlier open
  question is closed the other way from what several agents assumed: the original ESP32
  *does* have a Bluetooth + BLE radio** — this was asserted wrongly more than once during
  the migration and the assumption was what made "drop it" look safe. NimBLE costs flash
  and RAM, both of which are tight (§3 of 07); if the image cannot absorb it, that is a
  gate decision to raise, **not** a licence to drop the feature silently. | R3-12, R3-16 | yes | A BLE scale connects, reports weight, and reconnects after the radio drops; the machine's radio use does not starve the control task; with no scale paired, nothing else regresses. |

### Gate 3

- **Size gate**: `just size` recorded with per-crate attribution; `just size-check` passes.
  This is the gate most likely to force a §3 drop.
- Every R3 task **marked `HW: yes`** validated on hardware; R3-11 validated on host.
- The four known C++ config defects are fixed in Rust, not replicated:
  `safety.emergency_temp`/`hysteresis` registration; the OTA hardware gap; the
  blocking pressure read; the missing panic backtrace decode.
- A `docs/history/parity.md` recording every observed difference from C++.

---

## Phase 4 — Integration and provisioning (hardware required)

| ID | Objective | Depends on | HW | Acceptance |
| --- | --- | --- | --- | --- |
| **R4-01** | Wire the control task, the executor per R1-02, the queues, and the watchdog feed. | Gate 3 | yes | 10 ms tick held; no task can starve the control task. |
| **R4-01b** | **Performance: measure and prove the improvement over C++.** The human's ask is
"improve performance and better use code". The known wins, in expected order of magnitude:
(a) the ABP2 pressure read goes non-blocking — today `pressureSensor.h:35` does
`delay(10)` every 50 ms, **20 % of wall clock**
([01 §4](../../history/feature-inventory.md)); (b) the 10 ms software-PWM
ISR is replaced by LEDC hardware PWM, so the heater costs **zero CPU**; (c) the tick is a
pure reducer, so `cc-machine` becomes host-benchmarkable (R2-09). | R4-01, R3-05, R3-04 | yes | Control-loop budget over a **24 h soak**: worst-case tick **≤ 5 ms**, mean **≤ 2 ms**, **zero ticks > 10 ms**, compared against the per-iteration histogram recorded from C++ at R0-04. Heater duty error ≤ 1 %. Publish a before/after table. **If no improvement is demonstrated, say so** — a regression here is a finding, not a failure. |
| **R4-02** | Run the full [integration checklist](../../operations/runbook.md) against the Rust firmware. | R4-01 | yes | Every section PASSes; failures recorded, not skipped. |
| **R4-03** | Parity harness: `scripts/parity/run.sh` runs both firmwares against the same scripted input and diffs `/api/status`, `/api/parameters?filter=all`, the MQTT discovery payloads, and the state-transition log. | R4-01 | yes | **Zero unexplained diffs.** Any difference is either a documented intentional change or a bug. |
| **R4-04** | Safety-path validation: overtemp trip, emergency latch and recovery, water-tank-empty pump kill, valve fail-safe, watchdog reboot, OTA actuator-off. | R4-01 | yes | Each behaves as in [01 §6](../../history/feature-inventory.md). **Every one of these needs a written safe test procedure reviewed before it runs.** |
| **R4-05** | Soak: 24 h unattended with periodic API and MQTT polling. Watch heap and the ADR-0002 pattern. | R4-01 | yes | No reboot, no leak, heap stable. |
| **R4-06** | Flash-size and RAM final measurement; document the final partition table. | R3-16 | yes | Image fits with recorded headroom; static RAM budget documented. |
| **R4-07** | ESP32-S3 spike **only if** desired: same workspace, `board-esp32s3-devkitc-1` feature, and the `LEDC`/pin differences resolved. | R4-06 | yes | Builds and boots. **A successful build is not support** — see the explicit rule below. |
| **R4-08** | ESP32-C6 spike **only if** desired: the same, with `riscv32imac-esp-espidf`. Note C6 needs no compiler fork, but has a different GPIO map and no GPIO 34-39. | R4-06 | yes | Builds and boots. |
| **R4-09** | Document the intentional behaviour changes for the release notes: scale support dropped; config key names for safety params now actually persist; OTA is safer. | R4-03 | no | In the release notes. |
| **R4-10** | Deprecate the PlatformIO build — **but do not remove it.** Keep it for one release cycle as a rollback path. | R4-06 | no | `pio run -e esp32_usb` still works and still produces a flashable image. |
| **R4-11** | Implement provisioning per the R1-06 decision. | R1-06, R3-12 | yes | A phone provisions the device; credentials are not in any log; `clear_credentials` works. |

> ### Rule: a target is not supported because it builds
>
> `build-esp32s3` and `build-esp32c6` exist but are **excluded from `build-all`**. A
> target becomes supported only after it has been **flashed and exercised on real
> hardware**, and its entry in [01 §1](../../history/feature-inventory.md)
> is updated with what was actually verified. The C6 in particular needs no compiler fork
> — which makes it *look* easy — but it has a completely different GPIO map, no
> GPIO 34-39 input-only block, and no native USB on some DevKitC variants.

### Gate 4 (final)

- **Size gate**: final `just size` recorded; the image fits with headroom to spare, and
  R4-01b's performance comparison is in the same report.
- R4-02, R4-03, R4-04, R4-05 all pass on hardware.
- Zero unexplained parity diffs.
- The release notes list every intentional change.
- The C++ build still works, for rollback.

---

## Dependency graph

```
R0-01 ──┬─> R0-02 ─┐
R0-03 ──┤           ├─> Gate 1 ─> R2-04 ─┐
R0-04 ──┴─> R1-01 ─┤                     ├─> R2-08 ─> R2-09 ─> Gate 2
                  ├─> R1-02 ─────────────┘            │
                  ├─> R1-03 ─────────────────────────┤
                  ├─> R1-04 ─> R2-10 ────────────────┤
                  ├─> R1-05 ─────────────────────────┤
                  ├─> R1-06 ─────────────────────────┤
                  └─> R1-07 ─────────────────────────┘
                                                            v
                          R3-01 … R3-16 ──> Gate 3 ──> R4-01 ──> R4-02/03/04/05 ──> Gate 4
```

## Open decisions blocking Phase 2+ (need a human)

| Question | Blocks | Default if unanswered |
| --- | --- | --- |
| ~~Drop HX711 / Acaia BLE scale support?~~ **DECIDED 2026-09-29: KEEP BOTH.** The human
  is the owner of the hardware and answered that the code being dead is a **bug on their
  side**, not a reason to discard the feature. Not a divergence to be tolerated either —
  Rust is expected to do the job *properly*, which the C++ never does. See R3-17/R3-18
  and [09 §23](../../history/cpp-findings.md). | R2-07 | resolved |
| Reduce app slots to grow the filesystem, or embed the SPA in the binary? | R2-03 | Measure first, then rebalance; embed the SPA |
| Keep SH1106 support or drop it? | R2-10 | Drop it and document; `ssd1306` does not support SH1106 and `sh1106` 0.5.0 is stuck on `embedded-hal 0.2` |
| SSE or WebSocket for the UI's live channel? | R3-14 | SSE via `EspHttpConnection::write` / `raw_connection()` (both ship in esp-idf-svc 0.53); WebSocket only if both fail |
| Is the C++/Rust coexistence acceptable, or must the Rust build replace PlatformIO immediately? | R2-01 | Coexist; deprecate PlatformIO at R4-10 |
| Should NVS credentials be encrypted during the port? | R2-06 | No — parity. Record as a follow-up |
