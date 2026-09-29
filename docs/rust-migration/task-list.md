# Task list

Phases, in dependency order. Every task states its objective, prerequisites, expected files,
acceptance criteria with exact commands, whether it needs hardware and on which board, safety
notes, rollback, open uncertainty, whether it researches or implements, and the defect IDs it
fixes.

**A task either researches or implements, never both.**

**Hardware status: no board is connected.** Every task marked `hardware: yes` is blocked until a
board is available. The original ESP32 is the bench target. S3 and C6 are build-verified until
hardware exists. See [compatibility-matrix.md](compatibility-matrix.md).

**Parity scope:** functional, control and API behaviour. Storage format, partition layout and OTA
are out of parity scope by design, because the Rust firmware is a clean break. Deviations
documented as defect fixes are exempt from parity.

Cross-links: [architecture.md](architecture.md), [decision-record.md](decision-record.md),
[defects-register.md](defects-register.md), [tooling.md](tooling.md),
[.agents/skills/esp32-rust-migration/SKILL.md](../../.agents/skills/esp32-rust-migration/SKILL.md).

---

## Phase 0: Foundation

Exit gate: `just check` passes, the three spikes build for all three targets, the CI workflow
runs.

### T-01. Rust workspace skeleton and CI
- **Type:** implement
- **Prereqs:** none
- **Files:** `Cargo.toml` (workspace), `rust-toolchain.toml`, `.cargo/config.toml`,
  `crates/*/Cargo.toml`, `crates/*/src/lib.rs`, `.github/workflows/rust.yml`
- **Steps:** create the workspace with the eleven crates named in
  [architecture.md](architecture.md#2-crate-layout); add the dependency-direction CI check; add
  the secrets-grep CI check; add fmt, clippy, host-test and three build jobs with SHA-pinned
  actions and `permissions: contents: read`.
- **Acceptance:**
  - `just check` passes
  - `just spike` builds all three targets
  - `cargo tree -p domain` shows no `esp-hal`
- **Hardware:** no
- **Safety:** none
- **Rollback:** delete the workspace files
- **Open uncertainty:** whether `-Zbuild-std=core,alloc` is needed for all three targets or only
  the Xtensa ones. Observed: needed for all three in this environment.
- **Fixes:** none

### T-02. Domain crate: states, transitions, timing constants
- **Type:** implement
- **Prereqs:** T-01
- **Files:** `crates/domain/**`
- **Steps:** port `State` and the transition table from
  `include/clevercoffee/state/MachineStateIds.h` and `BaseState::checkTransitions`, keeping the
  integer ids so the frontend's state rendering keeps working; port the timing constants from
  `include/clevercoffee/constants/Timing.h`; port the PID from `lib/Arduino-PID-Library` with
  the same output scaling (0-1000) and window (1000 ms).
- **Acceptance:** `cargo test -p domain` covers every state, every transition and its priority
  order, and the PID against three hand-computed step responses.
- **Hardware:** no
- **Safety:** the transition table is the safety-critical part. Every test asserts the exact
  target state, not just that a transition happened.
- **Rollback:** n/a, new crate
- **Open uncertainty:** none; the C++ table is fully specified in the source
- **Fixes:** D21 (a state with no hardware action is now a compile error, because the state
  carries its own command)

### T-03. HAL traits and the mock actuator mode
- **Type:** implement
- **Prereqs:** T-01
- **Files:** `crates/hal-traits/**`
- **Steps:** define `Actuators`, `TemperatureSensor`, `PressureSensor`, `Display`, `Switch`,
  `Scale`, `Storage`, `ProvisioningTransport`, `Clock`; define a `RecordingActuators` fake that
  records the exact command sequence, used by every control test and by the on-device
  `mock-actuators` build.
- **Acceptance:** `cargo test -p hal-traits`; a test asserts that a recorded command sequence for
  "brew started then aborted" is exactly `[pump_on, valve_open, pump_off, valve_close]`.
- **Hardware:** no
- **Safety:** `mock-actuators` must be a cargo feature that **removes** the real pin
  construction, not a runtime flag. Verified by a test that the mock build has no GPIO output in
  its binary.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** D02 (no shadow boolean), D19 (one owner, re-asserted every tick)

### T-04. Config schema crate
- **Type:** implement
- **Prereqs:** T-01
- **Files:** `crates/config/src/schema.rs`, `crates/config/src/defaults.rs`
- **Steps:** transcribe the 96 parameters from
  [config-export-schema.md](config-export-schema.md) into a declarative table with key, type,
  unit, default, min, max and a `secret` flag. Add `format_version`, `safety.emergency_temp`,
  `safety.emergency_hysteresis`, `hardware.board`. Order derived from declaration index.
- **Acceptance:** a test asserts the table has exactly the expected key set, that no key appears
  twice, and that every numeric default lies inside its own range.
- **Hardware:** no
- **Safety:** every range is a safety limit. The test above is the gate.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** D12 (unregistered parameters cannot exist), D23 (length limits enforced), D41
  (deterministic order), D42 (no dead type variants)

---

## Phase 1: Storage and provisioning on the host

Exit gate: host tests green, `just check` passes, both phases build for all three targets.

### T-05. Config region format
- **Type:** implement
- **Prereqs:** T-04
- **Files:** `crates/storage/**`
- **Steps:** implement the A/B slot region described in
  [architecture.md](architecture.md#32-config-region-format): magic, format version, slot
  generation, length, CRC-32, a SHA-256 prefix, the payload, read-back verification on write.
- **Acceptance:** `cargo test -p storage` covers: a clean boot, a boot with one corrupt slot, a
  boot with both slots corrupt, a torn write leaving the older slot intact, a generation
  wraparound, and a payload one byte too long.
- **Hardware:** no
- **Safety:** the "both slots corrupt falls back to defaults" test is a safety case, because a
  corrupt setpoint must never reach the PID.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** D11 (every field is range-checked on decode, before use)

### T-06. Config import and export
- **Type:** implement
- **Prereqs:** T-04, T-05
- **Files:** `crates/config/src/import.rs`, `crates/config/src/export.rs`
- **Steps:** implement the three accepted input shapes and the validation and reporting rules in
  [architecture.md](architecture.md#5-config-import).
- **Acceptance:** `cargo test -p config` imports the repository's `config.json` and
  `docs/example_config.json` and asserts the exact report for each, including the
  `display.blescale_brew_timer` unknown field and the missing `mqtt` keys. Further cases: a
  value above max, a value below min, a wrong type, an explicit null, a flat dotted key at depth
  two, a partial file, and a file with a secret whose value must not appear in the report.
- **Hardware:** no
- **Safety:** the "nothing is applied when anything is rejected" test is the gate.
- **Rollback:** n/a
- **Open uncertainty:** whether users want `?mode=partial` at all. Proposed: keep it, off by
  default.
- **Fixes:** D13 (transactional, structured report), D14 (secrets never echoed), D27 (typed
  parse, no 2-decimal truncation)

### T-07. HTTP server crate
- **Type:** implement
- **Prereqs:** T-01
- **Files:** `crates/http/**`
- **Steps:** a minimal HTTP/1.1 server over a `Read + Write` stream: request parsing, routing,
  chunked responses, SSE, `Accept-Encoding` negotiation, static asset serving from a byte
  source, and an authentication hook. The socket layer is injected so the whole thing runs on
  the host.
- **Acceptance:** `cargo test -p http` covers request parsing for every method the frontend uses,
  chunked and content-length bodies, multipart upload, keep-alive, a malformed request line, a
  request larger than the limit, SSE framing, and gzip negotiation.
- **Hardware:** no
- **Safety:** the request size limit and the header count limit must have tests, because an
  unbounded allocation on an embedded target is an OOM.
- **Rollback:** n/a
- **Open uncertainty:** none; this is our code
- **Fixes:** D34 (the contract becomes what the tests assert)

### T-08. Host-side provisioning tool
- **Type:** implement
- **Prereqs:** T-06
- **Files:** `tools/provision/**`
- **Steps:** the line protocol client: `PING`, `WIFI SSID`, `WIFI PASS`, `WIFI COMMIT`,
  `CONFIG BEGIN/CHUNK/END`, `FACTORY RESET`, `STATUS`. Reads `.env`, never echoes a secret,
  base64-chunks the config with a CRC-32 prefix.
- **Acceptance:** `cargo test` in `tools/provision` covers the dotenv parser, the chunking with
  a payload larger than one chunk, and, against an in-memory fake device, every command and its
  reply handling. `just status <a port we cannot open>` exits non-zero with a status line and
  prints no secret.
- **Hardware:** no
- **Safety:** an automated check that the binary's source contains no `println!` of a credential
  variable.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** n/a

---

## Phase 2: Drivers

Exit gate: all three targets build; each driver has host tests.

### T-09. One-wire and DS18B20
- **Type:** implement
- **Prereqs:** T-01, T-03
- **Files:** `crates/onewire/**`, `crates/ds18b20/**`
- **Steps:** the bit-bang transport with an explicit timing model, ROM search, the CRC-8 Dallas
  family, and the command layer: `CONVERT_TEMP`, wait, `READ_SCRATCHPAD`, resolution handling for
  9 to 12 bits, and the error sentinels.
- **Acceptance:** host tests run the driver against a simulated bus that models the slave's
  response timing, covering: a single-drop and a multi-drop ROM search, CRC failure on a
  corrupted byte, a disconnected bus, a stale scratchpad read at each resolution, and the
  conversion wait at 9, 10, 11 and 12 bits.
- **Hardware:** **yes**, ESP32, to confirm the real bit timings
- **Safety:** the driver must never return a plausible-looking value after a CRC failure. There
  is a test for exactly that.
- **Rollback:** n/a
- **Open uncertainty:** the actual DS18B20 conversion times (750/375/188/94 ms) were not
  verified from the datasheet in this run. The host test asserts the values the driver uses, and
  the device test confirms them.
- **Fixes:** D03 (CRC failure and out-of-range are a fault, not a value)

### T-09b. TSIC 306 driver
- **Type:** implement
- **Prereqs:** T-01, T-03
- **Files:** `crates/drivers-tsic/**`
- **Steps:** the pulse-train protocol behind the C++ `ZACwire` library, as a second
  implementation of the `TemperatureSensor` trait. The selection between DS18B20 and TSIC is
  `hardware.sensors.temperature.type`, resolved once at boot. **Both sensors are kept**, per the
  user on 2026-09-29.
- **Acceptance:** host tests drive the driver from a simulated pulse train, covering a normal
  reading, the 222 read-failed and 221 not-connected sentinels, a reading at or below 0 C, a
  reading at or above 180 C, and the initial and runtime change rates. A test asserts that
  selecting an unavailable sensor type fails at boot with a named error rather than reading zero.
- **Hardware:** **no** for the driver; the bench has a DS18B20, so this path stays
  build-verified until a TSIC-equipped machine exists
- **Safety:** the same D03 rule applies: a failed read is a fault, not a value
- **Rollback:** n/a
- **Open uncertainty:** the exact TSIC timing is taken from the C++ `ZACwire` configuration
  (`INITIAL_CHANGERATE` 200, `RUNTIME_CHANGERATE` 5,
  `src/hardware/tempsensors/TempSensorTSIC.cpp:11-12`), not from a TSIC datasheet
- **Fixes:** D03, D43 (the dead `testEmergencyStop` path is not ported)

### T-10. Pressure sensor driver
- **Type:** implement
- **Prereqs:** T-03
- **Files:** `crates/drivers-pressure/**`
- **Steps:** the ABP2 I2C transaction as a future: start the conversion, return, read 10 ms
  later. 24-bit counts, the 0-10 bar span, the temperature conversion.
- **Acceptance:** host tests against a scripted I2C bus covering a normal reading, a stuck
  conversion, an out-of-span count, and a NACK. The test asserts the driver never sleeps.
- **Hardware:** no, the device has no pressure sensor fitted
- **Safety:** n/a
- **Rollback:** n/a
- **Open uncertainty:** whether the fitted hardware is the ABP2 variant the C++ code assumes.
  The C++ header says ABP2-LANT010BG2A3XX. **needs confirmation** from the user.
- **Fixes:** D08 (no blocking delay in the loop)

### T-11. HX711 scale driver
- **Type:** implement
- **Prereqs:** T-03
- **Files:** `crates/drivers-scale/**`
- **Steps:** single and dual load cell, bounded init retries, the calibration divisor, the
  moving average over `samples`.
- **Acceptance:** host tests against a scripted bit waveform covering init timeout, a read
  timeout, both cells present, one cell absent, and a negative calibration divisor.
- **Hardware:** **yes**, ESP32, for the timing
- **Safety:** init must be bounded, so a missing load cell cannot hang the boot.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** D29 (bounded retries), D30 (a configured sensor that is absent is an error)

### T-12. Display: framebuffer, font metrics, six templates
- **Type:** implement
- **Prereqs:** T-01, T-03
- **Files:** `crates/display/**`
- **Steps:** a 1024-byte framebuffer, a font-metrics table so glyph bounding boxes are computable
  on the host, dirty-page tracking, the six templates and the three languages, and the layout
  rules in `CLAUDE.md`: fixed pixel width for every numeric field, computed Y positions from
  glyph boxes, and bar-plus-label pairs sharing a vertical midline.
- **Acceptance:** host tests render every template in every state into a framebuffer and assert
  pixel content: nothing outside 128x64, no two rows overlapping, a numeric field's pixels
  identical in width when the value changes from 9 to 10, and a bar and its label sharing a
  midline.
- **Hardware:** **yes**, all three, to confirm the physical rendering
- **Safety:** none
- **Rollback:** n/a
- **Open uncertainty:** the exact font metrics. The C++ U8G2 fonts cannot be measured from the
  repository, so the metrics table is a new input. This is the single largest unknown in the
  display port and it is why the task is sized at 1200 to 1800 lines.
- **Fixes:** D31 (dirty pages only, 400 kHz bus)

---

## Phase 3: Board support and the firmware

Exit gate: all three targets build; the mock-actuator image boots on the bench device.

### T-13. Board profiles and the firmware binary
- **Type:** implement
- **Prereqs:** T-02, T-03, T-09, T-12
- **Files:** `crates/bsp-esp32/**`, `crates/bsp-esp32s3/**`, `crates/bsp-esp32c6/**`,
  `crates/fw/**`, `partitions-rust.csv`
- **Steps:** three board modules behind three mutually exclusive features, each defining its own
  pin map, its `Actuators` impl, its display init and its provisioning transport; the `fw`
  binaries with the panic handler, the partition table, the config region load, the sensor
  probe, the task spawns and the provisioning task.
- **Acceptance:** `just check-fw esp32`, `just check-fw esp32s3` and `just check-fw esp32c6`
  all pass. Enabling two board features is a compile error.
- **Hardware:** **yes**, ESP32, to flash the mock-actuator image and confirm it boots and reports
  over the provisioning channel
- **Safety:** the boot order drives the actuators to their inactive state before anything else
  is configured. Verified by reading the startup path, and by the mock image on device.
- **Rollback:** `just flash` reverts; the old C++ firmware is recoverable by flashing it again
  over USB, which is why the migration guide says to export the config first
- **Open uncertainty:** the ESP32 and S3 pin maps are settled in
  [board-pinouts.md](board-pinouts.md#5-proposed-pin-maps). **The C6 map does not fit**: the
  project needs 17 pins and the ESP32-C6-DevKitC-1 exposes 16. This task cannot complete the C6
  board module until the user picks a different C6 board, an I2C IO expander, or a reduced C6
  feature set. The ESP32 and S3 modules are unblocked by this. **This is the first blocking
  question in the plan.**
- **Fixes:** D01, D04, D05

---

## Phase 4: Application

Exit gate: host tests green, all three targets build, the API is complete.

### T-14. Control tasks and the state machine wiring
- **Type:** implement
- **Prereqs:** T-02, T-03, T-13
- **Files:** `crates/app/src/tasks/**`, `crates/app/src/machine/**`
- **Steps:** the seven tasks from
  [architecture.md](architecture.md#11-tasks); the `control` task driving the state machine
  every millisecond; the `safety` task feeding the watchdog and enforcing the interlocks; every
  pump command carrying a deadline.
- **Acceptance:** host tests run the whole task graph against `RecordingActuators` and assert
  the command sequence for: normal brew, brew aborted during preinfusion, brew aborted during
  preinfusion pause, manual flush, backflush to completion, water tank emptying mid-brew,
  emergency stop from every state, standby entry and wake, sensor fault, watchdog trip, and a
  stuck switch held for longer than the pump deadline.
- **Hardware:** **yes**, ESP32, in mock-actuator mode
- **Safety:** every test asserts the actuator sequence, so a missing shutdown is a test failure
  rather than a field failure. A stuck switch must not produce more than the pump deadline worth
  of `pump_on` commands; that assertion is explicit.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** D09, D22, D33

### T-15. Provisioning protocol on device
- **Type:** implement
- **Prereqs:** T-06, T-08, T-13
- **Files:** `crates/app/src/provision/**`
- **Steps:** the device side of the protocol, over UART0 on ESP32 and USB Serial/JTAG on S3 and
  C6. Provisioning forces actuators off for its duration.
- **Acceptance:** `cargo test -p app` runs a full host-side exchange against the device-side
  parser. On device: `just wifi <port>` then `just status <port>` shows the device connected;
  `just config-import <port> config.json` then `just status <port>` shows the imported setpoint.
- **Hardware:** **yes**, all three boards
- **Safety:** the first thing provisioning does is `force_off()`. Tested.
- **Rollback:** `just factory-reset <port>`
- **Open uncertainty:** whether the USB Serial/JTAG CDC path works at 115200 baud on both S3 and
  C6. The C++ firmware had a note that the USB console stops during light sleep, which affects
  the C6. **needs confirmation** on device.
- **Fixes:** n/a

### T-16. Web API implementation
- **Type:** implement
- **Prereqs:** T-07, T-14
- **Files:** `crates/app/src/api/**`
- **Steps:** all 30 routes from [api-contract.md](api-contract.md) as pure handler functions,
  including the four OTA routes, which are **kept** and corrected rather than removed: every OTA
  path requires the configured password, refuses to start unless the machine is idle, and the URL
  variant gains a scheme and host allow-list plus the extension check it was missing. The
  frontend is therefore unchanged.
- **Acceptance:** `cargo test -p app` asserts, for every route, the exact status code and the
  exact response body shape from the contract, in the success case and in each error case. Then,
  on device: `GET /api/status`, `GET /api/parameters`, `GET /api/config`, `GET /api/history`,
  `GET /api/temperatures`, and the UI loads and renders. The OTA tests must include: an
  unauthenticated upload is rejected, an upload while brewing is rejected with 409, a URL
  outside the allow-list is rejected, and a valid update reaches a reboot.
- **Hardware:** **yes**, ESP32
- **Safety:** handlers must not block. A test asserts no handler contains an await.
- **Rollback:** n/a
- **Open uncertainty:** none; the user confirmed OTA stays, so the frontend is unchanged
- **Fixes:** D01, D10, D15, D16, D17, D24, D25, D26, D28, D32

### T-17. MQTT and Home Assistant discovery
- **Type:** implement
- **Prereqs:** T-14
- **Files:** `crates/app/src/mqtt/**`
- **Steps:** the MQTT client and the discovery-document generator, ported from
  `src/network/MQTTManager.cpp`. The generator is a pure function of the schema, so it is
  host-testable against golden JSON.
- **Acceptance:** `cargo test -p app` compares the generated discovery documents against golden
  files. On device, a real broker is needed to verify the publish path, which is a separate
  device task.
- **Hardware:** yes, and a broker
- **Safety:** the `sscanf` return-value bug in the C++ inbound path (uninitialised value on a
  non-numeric payload) must not be reproduced: the parser is typed.
- **Rollback:** the feature is behind a config flag
- **Open uncertainty:** which `no_std` MQTT client to use. `rumqttc` is a candidate.
- **Fixes:** D43 (dead code not ported)

### T-18. Telnet logging
- **Type:** implement
- **Prereqs:** T-14
- **Files:** `crates/app/src/log/**`
- **Steps:** a bounded ring buffer and a line server on port 23, plus the USB and UART
  transports.
- **Acceptance:** host tests for ring wraparound and for a full ring dropping the oldest entry
  rather than blocking. On device, `nc <host> 23` shows the log stream.
- **Hardware:** yes, ESP32
- **Safety:** the buffer is statically sized and the count is in the heap budget.
- **Rollback:** n/a
- **Open uncertainty:** whether to keep this at all. It was RFC 2217-adjacent in the C++ code.
  Proposed: keep it simple, no RFC 2217 telnet negotiation.
- **Fixes:** D38, D39

---

## Phase 5: Parity and migration

Exit gate: the end-to-end import check passes on device; the migration guide is written.

### T-19. End-to-end config import on device
- **Type:** implement
- **Prereqs:** T-15, T-16
- **Files:** `tests/e2e/**`, `docs/user-migration-guide.md`
- **Steps:** the user migration guide (export, flash over USB, import); an end-to-end check that
  imports the repository's `config.json` and a real old C++ export and verifies the device
  reports the imported values.
- **Acceptance:** on device, `just config-import <port> config.json` succeeds;
  `just config-import <port> <old export>.json` succeeds; `just status <port>` reports the
  imported setpoint, PID gains and relay trigger types; a deliberately out-of-range setpoint is
  rejected with a named field.
- **Hardware:** **yes**, ESP32
- **Safety:** this is the task that must pass before anyone is told to flash. The guide is not
  published until it does.
- **Rollback:** `just factory-reset <port>`
- **Open uncertainty:** a real old C++ export is needed as a test fixture. The repository's
  `config.json` is a sample, not a real export. **needs confirmation** from the user.
- **Fixes:** n/a

### T-20. Parity check against the C++ firmware
- **Type:** implement
- **Prereqs:** T-16, T-17
- **Files:** `tests/parity/**`
- **Steps:** a checklist-driven parity run: functional (brew, steam, backflush, manual flush,
  standby), control (the state table, the interlocks, the emergency stop) and API (every route's
  status codes and body shapes). Document every deviation and its reason.
- **Acceptance:** the parity report is written, every row is pass or an explained deviation, and
  every deviation that is not a defect fix is either fixed or escalated to the user.
- **Hardware:** **yes**, ESP32, for the control legs
- **Safety:** parity runs with the heater **disabled** by default, so no test energizes a load.
  A load-energizing test needs the user's explicit approval and a written safe procedure.
- **Rollback:** n/a
- **Open uncertainty:** none
- **Fixes:** n/a

### T-21. Display layout verification on device
- **Type:** implement
- **Prereqs:** T-12
- **Files:** `tests/display/**`
- **Steps:** photograph or dump every template in every state on the real panel and compare
  against the host framebuffer assertions.
- **Acceptance:** no clipped text, no overlapping rows, no shifting numeric fields, no
  misaligned bar and label pairs.
- **Hardware:** **yes**, all three
- **Safety:** none
- **Rollback:** n/a
- **Open uncertainty:** the font metrics table is unverified against real fonts until this task
- **Fixes:** n/a

---

## Phase 6: Removal

### T-22. Remove the C++ firmware
- **Type:** implement
- **Prereqs:** every parity gate in T-20 passes
- **Files:** delete `src/`, `include/`, `lib/`, `test/`, `platformio.ini`,
  `partitions_4M.csv`, `wokwi.toml`, `diagram.json`, `tools/platformio_wokwi.py`,
  `tools/wokwi_flasher_args.py`, `scripts/auto_compression.py`, `scripts/create_flash_package.py`,
  `examples/`; update `README.md`, `CONTRIBUTING.md`, `REPOSITORY_SUMMARY.md`,
  `DEBUG_GUIDE.md`, `CLAUDE.md`, `.clang-format`, `.pre-commit-config.yaml`, `platformio.ini`
  removal from CI, and the `docs/` tree
- **Steps:** delete the C++ tree; rewrite the root docs for the Rust firmware; regenerate
  `docs/api/openapi.yaml` from [api-contract.md](api-contract.md); update
  `docs/integration-tests.md` to the new checklist; drop PlatformIO, node, pnpm, python and
  clang-format from `.mise.toml`; remove the C++ CI workflows.
- **Acceptance:** `just check` and `just spike` pass; `rg --files src include lib test` returns
  nothing; the repo has no PlatformIO reference; every doc in `docs/` describes the Rust
  firmware; `CONFIG_REFERENCE.md` is regenerated from the schema.
- **Hardware:** no
- **Safety:** none
- **Rollback:** the whole phase is one commit on its own branch, so reverting is a single
  `git revert`. This is why it is its own task with its own review.
- **Open uncertainty:** none
- **Fixes:** D35, D43

---

## Deferred and dropped

| Item | Reason | Options presented to the user |
| --- | --- | --- |
| Acaia Bluetooth scale (P3) | The scale is dead code in the C++ firmware, so there is no parity pressure. A BLE stack swap plus a vendor protocol is 600 to 1000 lines with no test bench. | **Deferred** (user, 2026-09-29). The HX711 path covers brew-by-weight. |
| Wi-Fi captive portal (P7) | USB provisioning replaces it, and it blocks the loop for up to 60 seconds. | **Dropped** (user, 2026-09-29). |
| Wokwi simulation (P8) | Wokwi runs the PlatformIO build; the Rust firmware cannot run there. | **Dropped** (user, 2026-09-29), with the rest of the C++ tree in T-22. |
| HTTP OTA and URL OTA | The plan proposed deleting them. The user wants OTA kept. | **Kept and corrected** (user, 2026-09-29): authenticated, idle-only, URL allow-list, plus a USB path. |
| TSIC 306 temperature sensor | No Rust equivalent for `ZACwire`. | **Both sensors kept** (user, 2026-09-29). The TSIC driver is task T-09b, build-verified until a TSIC machine exists. |
| NVS encryption at rest | The config region is a single self-describing blob, so this is addable later without a format change. | Accept as a documented limitation. **Still open, not confirmed by the user.** |
| Telnet log server RFC 2217 features (P6) | The C++ implementation is not actually RFC 2217. | Ship a plain line server. **Still open, not confirmed by the user.** |
| ESP32-C6 pin budget (P9) | 17 pins needed, 16 exposed on the ESP32-C6-DevKitC-1. | **Needs a user decision**: a different C6 board, an I2C IO expander, or a reduced C6 feature set. Evidence in [board-pinouts.md](board-pinouts.md). |

## Hardware-dependent tasks, in one place

T-09, T-11, T-12, T-13, T-14, T-15, T-16, T-17, T-18, T-19, T-20, T-21. All are blocked with no
board connected. T-01 to T-08 and T-09b are not.

T-13 is additionally blocked on a **decision**, not on hardware: the C6 pin map does not fit.
The ESP32 and S3 halves of T-13 can proceed while that is open.
