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
  `crates/*/Cargo.toml`, `crates/*/src/lib.rs`, `crates/fw/src/{lib,main,main_s3,main_c6,checks}.rs`,
  `tools/check-deps.py`, `tools/check-secrets.py`, `.github/workflows/rust.yml`, `justfile`
- **Steps:** create the workspace with the sixteen crates named in
  [architecture.md](architecture.md#2-crate-layout); add the compile-time feature guards in
  `crates/fw/src/checks.rs`; add the dependency-direction and committed-secret checks; add the
  three CI jobs with SHA-pinned actions and `permissions: contents: read`.
- **Acceptance:**
  - `just check` passes
  - `just check-fw esp32`, `just check-fw esp32s3` and `just check-fw esp32c6` all pass
  - `just spike` builds all eight spike configurations
  - `cargo tree -p clevercoffee-domain` shows no dependency at all
  - the C++ tree is untouched: `git diff --name-only -- src include lib test platformio.ini` is
    empty
  - three wrong feature combinations each fail to build, not merely warn
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
- **Acceptance:** `cargo test -p clevercoffee-domain` covers every state, every transition and
  its priority order, the actuator command for every state against the interlock lists, the
  emergency-stop evaluator across its whole range, the sensor fault latch, and the PID.
  82 tests, all passing.
- **Hardware:** no
- **Safety:** the transition table is the safety-critical part. Every test asserts the exact
  target state, not just that a transition happened, and one test walks all eighteen states
  asserting that each one's actuator command agrees with the interlock lists.
- **Rollback:** n/a, new crate
- **Open uncertainty:** none; the C++ table is fully specified in the source
- **Fixes:** D03 (a failed read is a fault, and a faulted sensor yields no value at all), D09,
  D18, D21, D33, D45, D46, D47, D49, D50, D51

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
- **Fixes:** D12 (unregistered parameters cannot exist), D23 (length limits enforced), D26
  (group filters mean what they say), D27 (typed values, no 2-decimal truncation), D41
  (deterministic order), D42 (no dead type variants), D52 (a zero calibration divisor is
  rejected)

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
- **Acceptance:** `cargo test -p storage`, 35 tests. A clean boot, a boot with one corrupt slot, a
  boot with both slots corrupt, a torn write leaving the older slot intact, a generation
  wraparound, a payload one byte too long, a length that does not fit the slot, and a version this
  firmware does not read in either direction.
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
- **Acceptance:** `cargo test -p clevercoffee-config`, 63 tests. The repository's own
  `config.json` is read from disk and must import with zero unknown keys, zero rejections and
  zero clamps; `docs/example_config.json` must fail with exactly one unknown field named, and
  must import cleanly once that one field is dropped. Further cases: a value above max, a value
  below min, a wrong type, an explicit null, a nested document that resolves to the same dotted
  key as a flat one, a partial document, an over-long text value, a string containing a control
  character, nesting past the depth limit, and a secret whose value must not appear in the
  report. The JSON parser is in `crates/config/src/json.rs`, separate from the validation rules
  so each can be tested without the other.
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
- **Acceptance:** `cargo test -p clevercoffee-http`, 86 tests. Request parsing for every method the
  frontend uses, a body arriving at the offset the parser reported, a request split across
  several reads, keep-alive from the version and the `Connection` header, a malformed request
  line, a request line and a header line and a header count over their limits, an obsolete folded
  header, a chunked body, an absolute-form target, SSE framing and its terminal chunk, and gzip
  negotiation including a `q=0` refusal. The connection loop runs over an injected stream, so
  every test exercises the real parse, route, guard and write path.
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
- **Acceptance:** 55 tests across `tools/provision`, run by `just check`. The dotenv parser, the
  chunking with a payload larger than one chunk and its reassembly, and every command and its reply
  handling against an in-memory fake device. The device-half is compiled out of a shipped build;
  the binary's own tests carry a twenty-line copy for the same reason. A port that cannot be opened
  exits non-zero with one status line and no secret.
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
  9 to 12 bits.
- **Acceptance:** `cargo test -p clevercoffee-onewire`, 24 tests: byte framing and its
  least-significant-bit-first wire order, the CRC against a corrupted byte at every position, an
  unconnected bus, a device that does not acknowledge, a ROM with a bad CRC being refused, the
  wire shape of a ROM search (64 bit pairs, 64 branch decisions, 8 CRC bits), and a search that
  cannot terminate being bounded. The DS18B20 half, with the scratchpad and the conversion wait at
  9 to 12 bits, is T-09 continued.
- **Hardware:** **yes**, ESP32, to confirm the real bit timings
- **Safety:** the driver must never return a plausible-looking value after a CRC failure. There
  is a test for exactly that.
- **Rollback:** n/a
- **Open uncertainty:** the actual DS18B20 conversion times (750/375/188/94 ms) were not
  verified from the datasheet in this run. The host test asserts the values the driver uses, and
  the device test confirms them.
- **Not verified:** multi-drop ROM *enumeration*. The search algorithm is Stoffregen's and its wire
  shape is asserted, but the simulated bus cannot yet model several devices driving the line
  simultaneously, so which devices a search returns is not proven. A real machine has one DS18B20
  on its bus, so this is a gap in the tests rather than a path the machine takes. Recorded here
  rather than covered by a weaker assertion that would read like more than it is.
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
- **Done:** `cargo test -p clevercoffee-drivers-pressure`, 13 tests. The conversion is a transaction
  and never a sleep, a stuck conversion is a short read rather than a stale value, an out-of-span
  count is refused, a NACK is reported, and both words' status bytes are checked before anything is
  converted.
- **Fixes:** D08 and D55 (no blocking delay anywhere in the read path), D54 (the frame is
  twelve bytes, not seven)

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
- **Done:** `cargo test -p clevercoffee-drivers-scale`, 18 tests. The C++ unbounded init loop is
  bounded and reported rather than hanging the boot, a tare with no samples fails instead of
  zeroing against nothing, a dual scale with a dead cell reports nothing rather than half the
  weight, and the weight is `None` until a conversion has completed.
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
- **Done:** `cargo test -p clevercoffee-display`, 63 tests across the crate and the layout suite.
  The font is one 5x7 table at two integer scales rather than the five U8G2 fonts, so every width
  in the crate is `cell_width * scale` and no layout can assume a proportional font. The
  framebuffer diffs against the last flushed frame, which costs a second kilobyte and is what makes
  D31's fix real: an unchanged re-render sends nothing, a one-degree change on a template with no
  thermometer sends one page. The six templates render in all eighteen states in all three
  languages with a zero clip count, and the tests assert the ink bands, the fixed-width numeric
  fields and the bar/label midline on pixels.
- **Not verified:** the physical rendering, and the font's legibility on a real panel. The metrics
  are a new input, which is what T-21 exists to check.
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
- **Open uncertainty:** none blocking. The three pin maps are settled in
  [board-pinouts.md](board-pinouts.md). The C6 has 14 usable pins against 17 needed, so by the
  user's decision of 2026-09-29 the three indicator LEDs and the second HX711 load cell are
  disabled on that board, behind a compile-time capability the shared logic already handles.
- **Partly done.** The pin maps, the capability sets and the per-chip unusable-pin lists moved into
  a new `crates/board-profiles` crate, because a pin map is data and a data error is invisible in
  review. Twelve host tests check each map: every essential signal is mapped, no two signals share
  a pin, nothing lands on the flash bus, USB, UART0 or JTAG, the ESP32's four switches are on
  input-only pins and say that they need an external pull, the C6 fits fourteen usable pins, and
  only the ESP32 lacks native USB. The three `bsp-*` crates take their numbers from that crate and
  add the pin construction, the relay owner, the switch inputs and the provisioning transport; the
  `fw` binaries run the boot order and the loop.
- **Blocked, and this is the one task in this phase that is not done:** the Xtensa toolchain. The
  host is aarch64 and `just espup-install` fetches an x86-64 `espup`, so it exits 126 and the `esp`
  rustc fork is never installed. **`just check-fw esp32c6` does pass**, because the C6 is RISC-V
  and needs only a `rustup target add`; the `check-fw` recipe now falls back to the stable
  toolchain when the fork is absent, which is how it was verified here. `check-fw esp32` and
  `esp32s3` have not been run, so those two board crates and binaries are **uncompiled**. Next
  action for whoever has an x86-64 host: `just setup`, then the two remaining `check-fw` targets.
- The pin numbers a board crate hands to the HAL are tied to the profile by `const _: () =
  assert!` blocks, so the data table and the hand-written macro cannot drift apart without a
  compile error. That is the check a host test could not give, and it is why the numbers appear
  twice.
- **Now wired, and compiled for the C6:** the boot order reads the `config` partition out of flash
  through the bootloader's partition table, hands it to the app's loader, and the machine is
  constructed from the result. The sensor aggregator is polled once per tick with each driver on
  its own period. A region that is absent, unparseable or rejected leaves the machine on its
  compiled defaults, which is a machine that brews rather than one that will not start.
- Deliberately **not** wired: the Wi-Fi stack, the HTTP socket, the 1-Wire temperature driver, the
  scale, the pressure sensor and the display bus. The first two are I/O no test here can exercise;
  the last three are drivers that exist and are host-tested but need pins and a bus the board
  crates do not construct yet. The parity report lists them in the order they should be closed.
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
- **Done:** `cargo test -p clevercoffee-app`, 123 tests, of which 21 are whole-machine scenarios
  driven against `RecordingActuators`: a normal brew, an abort in pre-infusion, an abort in the
  pause, a manual flush, a backflush run, a tank emptying mid-brew, an emergency stop from twelve
  states, standby and wake, a sensor fault and its recovery delay, a watchdog that is not fed, and
  a stuck switch held past the pump deadline. Three real defects surfaced while writing them and are
  fixed: the service mode was expressed as "the PID is off" in the transition input, which ejected
  the machine from whatever it was doing the moment provisioning started (D01's second half); the
  emergency stop was evaluated on the *filtered* temperature, so a genuine over-temperature took
  six seconds to trip a fifteen-sample mean; and a sensor fault was routed into the latched
  emergency stop, which turned a recoverable fault into a machine that needed unplugging.
- **Also done:** the configuration bridge (`config_rt`), which was the missing link between the
  99-parameter schema and the machine's thirty fields and had no tests because it did not exist.
  Its coverage test fails when a parameter is neither read nor listed as inert, which is how a
  setting that looks live and is not gets caught. The sensor aggregator (`sensors`) reads each
  driver on its own period. The storage loader (`store`) turns flash bytes into a machine
  configuration with four named failure modes.
- **Not verified:** on hardware. The task bodies are synchronous functions and the scheduling,
  priorities and watchdog timer are the firmware's, which is stated in the code rather than papered
  over.
- **Fixes:** D09, D22, D33, and D56 (found here: the valve interlock closed the hot-water valve)

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
- **Done:** `cargo test -p clevercoffee-app --test provisioning`, 14 tests: a full exchange from
  `PING` through the credentials to a chunked config, the actuator trace proving the machine is
  de-energised before the port is read and stays off for the session, a walk of every command
  asserting that no reply the device can produce contains the password, a bad CRC, an over-long
  document, an overrunning chunk, a malformed line, a closed port and a factory reset.
- **Not verified:** on a device, and the board crates' transports have not been compiled.
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
- **Done:** `cargo test -p clevercoffee-app --test api_routes`, 31 tests over all thirty routes: the
  success case and each documented error case, the 401 the C++ never produced, the 409 while
  brewing, the URL allow-list the C++ did not have, the extension check the firmware route was
  missing, the transactional upload with its counts, the redaction, and the "no handler awaits"
  check run against this crate's own source.
- **Done:** the handlers are pure functions over a `Backend`, and `app::net` joins them to a
  connection: a bridge of two bounded rings, the http crate's own router built from the same route
  table `dispatch` uses, and a connection task that pumps an async socket into it. An end-to-end
  test drives a real `GET /api/status` through the real parser, router and handler and asserts the
  bytes that come back. The `Handler` trait's reply lifetime was widened to `&mut self` so a
  handler can own its response buffer rather than leak one per request.
- **Not verified:** on a socket. The Wi-Fi association and the accept loop are not written; the
  bridge, the router and the exchange are tested against in-memory halves.
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
- **Open uncertainty:** which `no_std` MQTT client to use. `rumqttc` is a candidate. The generator
  and the parser are written against no client at all, so this decision does not block them.
- **Done:** the discovery generator and the inbound parser, with 11 host tests: every entity in
  every feature combination produces a document under the topic the C++ used, a number document
  carries its range and step, a sensor carries its unit and class and no command topic, the output
  is byte-identical across calls, and the parser refuses a non-numeric payload rather than acting
  on an uninitialised value.
- **Not verified:** the publish path. No broker and no client.
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
  Proposed: keep it simple, no RFC 2217 telnet negotiation. **Still open, not confirmed.**
- **Done:** the ring buffer and the line server, with 10 host tests: wraparound, a full ring
  dropping the oldest rather than blocking, a filtered level that is counted but not stored, an
  over-long line truncated rather than split, a tail of the newest lines, and the case that matters
  most, a client that stops reading being dropped in one write while the ring is left untouched.
  4 KB, statically sized, which is the same budget the C++ allocated at boot.
- **Not verified:** the socket. The server is a state machine over an injected transport, so the
  "never block on a stalled client" property is tested; the listener is not written.
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
- **Done, except the device legs.** `docs/rust-migration/parity-report.md` is the checklist: every
  C++ behaviour, what the port does, and a row of pass, fixed, deviation, gap or unverified, with
  the test that is the evidence. It names four deviations with their reasons, six gaps in the
  order they should be closed, and the rows that cannot be closed without hardware.
- **Not done:** the control legs on hardware, which need a device and a heater-disabled build, and
  therefore still need a device. Every row that touches a relay, a heater or a sensor is
  host-tested against a fake and nothing more.
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
- **Fixes:** D35, D53

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
| ESP32-C6 pin budget (P9) | 17 pins needed, 14 usable on the ESP32-C6-DevKitC-1. | **Resolved** (user, 2026-09-29): the three LEDs and the second HX711 cell are disabled on the C6. No expander, no different board. |

## Hardware-dependent tasks, in one place

T-09, T-11, T-12, T-13, T-14, T-15, T-16, T-17, T-18, T-19, T-20, T-21. All are blocked with no
board connected. T-01 to T-08 and T-09b are not.

T-13 is additionally blocked on a **decision**, not on hardware: the C6 pin map does not fit.
The ESP32 and S3 halves of T-13 can proceed while that is open.
