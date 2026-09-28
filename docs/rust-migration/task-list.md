# Rust migration task list

**Status:** Active (A5). Supersedes `docs/plan/task-list.md`.
**Last updated:** 2026-09-28
**Related:** [inventory.md](inventory.md) · [compatibility-matrix.md](compatibility-matrix.md) · [architecture.md](architecture.md) · [tooling.md](tooling.md) · [ADR 0004](../adr/0004-rust-migration-platform-selection.md) · [skill](../../.agents/skills/esp32-rust-migration/SKILL.md)

Read [ADR 0004](../adr/0004-rust-migration-platform-selection.md) and
[architecture.md](architecture.md) before starting any task. The execution rules
for an agent working through this list are in the
[migration skill](../../.agents/skills/esp32-rust-migration/SKILL.md).

## Conventions

Every task carries: **ID**, objective, prerequisites, files, steps, acceptance
criteria with exact commands, hardware requirement, safety notes, rollback, open
uncertainty, and **kind** — `research` or `implementation`, never both. A task that
discovers it needs research stops and files the research task.

`HW: no` means the task is complete without hardware. `HW: esp32` means it needs
the attached ESP32 v3.0 board.

Commit messages start with the task ID. **Commit only after validation passes.**

### Standing safety rules

1. **No actuator is energised by any task in this list without a written safe test
   procedure in the task itself.** Tasks that would energise the heater, pump or
   valve are marked **ACTUATOR** and carry that procedure.
2. The heater, pump and valve must be safe before anything fallible runs — see
   [architecture.md §4.1](architecture.md).
3. Deliberate behaviour changes are declared in the commit message, never smuggled in.
4. Secrets never appear in output, logs, commits or examples.

---

## Prerequisite P0 — device availability (BLOCKING, needs the user)

**The attached device is not in a re-flashable state without a decision.** See
[tooling.md §7](tooling.md) and `research/device/FINDINGS.md`.

It currently runs a Rust/ESP-IDF binary (`project_name: libespidf`, ESP-IDF
v5.5.5, built 2026-09-28 21:48 local), its partition table does not match
`partitions_4M.csv` (1792 KB app slots and a 384 KB partition labelled `littlefs`,
versus 1664 KB slots and a 640 KB `spiffs`), and its NVS is completely blank.

Questions for the user, in priority order:

1. **May this device be re-flashed?** Flashing this repo's partition table changes
   the layout and erases the 384 KB `littlefs` region.
2. **Is a device with the C++ firmware and real stored config available?** Needed
   for SPIKE-3 (NVS continuity) and all parity checks. The current device has
   nothing to preserve.
3. **Which partition table is authoritative** — the repo's `partitions_4M.csv`, or
   the device's? The label difference (`spiffs` vs `littlefs`) changes the
   filesystem-OTA path.

Until P0 is answered, every `HW: esp32` task below is blocked. Every `HW: no` task
can proceed.

---

## Phase 0 — foundations (done)

| ID | Kind | Objective | Status |
|---|---|---|---|
| **F-1** | research | Inventory the C++ firmware; identify the device | done → [inventory.md](inventory.md) |
| **F-2** | research | Compare both Rust platforms per capability | done → [compatibility-matrix.md](compatibility-matrix.md) |
| **SPIKE-1** | research | Does a Rust + ESP-IDF image fit the app partition? | **done — 58.5 %, closed** → `research/spike-size/RESULT.md` |
| **F-3** | implementation | Architecture and crate split | done → [architecture.md](architecture.md) |
| **F-4** | implementation | Tooling: mise, justfile, CI, provisioning | done → [tooling.md](tooling.md) |
| **F-5** | implementation | Decision record | done → [ADR 0004](../adr/0004-rust-migration-platform-selection.md) |

**Phase 0 exit gate — met:** `just doctor`, `just fmt-check`, `just lint`,
`just test`, `just build esp32` and `just size esp32` all pass on this host; the
C++ baseline builds. No device flash (blocked by P0).

---

## Phase 1 — parity oracle and pure logic (`HW: no`)

Purpose: get the control logic into `cc-domain` with the C++ as the reference,
before any hardware is involved. **Every task in this phase is hardware-free**, so
Phase 1 can complete regardless of P0.

### ORACLE-1 — fix the `P_ON_M`/`P_ON_E` inversion in the C++ test stub
- **Kind:** implementation. **HW:** no. **Blocks:** PID-1, PARITY-1.
- **Objective:** `lib/Arduino-PID-Library/PID_v1.h` defines `P_ON_M 0`/`P_ON_E 1`
  while `test/PID_v1.h` defines them inverted. Any existing host test passing a
  mode constant exercises the opposite branch from the firmware, so the C++ tests
  cannot be used as the port's reference oracle until this is fixed.
- **Files:** `test/PID_v1.h`; any test asserting on the mode.
- **Steps:** align the stub with the firmware header; re-run the C++ suite; record
  which tests change result.
- **Acceptance:** `just cpp-test` passes; a note in the commit listing any test
  whose outcome changed and why.
- **Safety:** none — host only. **Rollback:** revert the header.
- **Uncertainty:** some tests may have been written against the inverted meaning
  and will need their expectations corrected. If any *firmware* behaviour turns out
  to depend on the inversion, stop and report.

### ORACLE-2 — capture PID golden vectors from the C++
- **Kind:** implementation. **HW:** no. **Prereq:** ORACLE-1.
- **Objective:** produce input→output vectors from the real `PID_v1` so the Rust
  port can be proven numerically equivalent.
- **Files:** new `test/test_pid_golden/test_main.cpp`; `research/pid-golden.csv`.
- **Steps:** drive the vendored PID through both `P_ON_M` and `P_ON_E`, saturation,
  integrator clamping, `SetSampleTime` changes and MANUAL↔AUTOMATIC transitions at
  the shipped `windowSize = 1000`; dump `(setpoint, input, output, kp, ki, kd, mode)`
  per step.
- **Acceptance:** CSV committed; `just cpp-test` passes; ≥200 steps covering every
  listed branch.
- **Uncertainty:** the EWMA filter's warm-up makes the first samples
  initial-condition dependent — capture from a defined reset.

### ORACLE-3 — capture NVS key vectors from the C++
- **Kind:** implementation. **HW:** no.
- **Objective:** prove the Rust `"p" + FNV-1a` derivation matches the C++ for every
  registered parameter.
- **Files:** new C++ test dumping `dotted.path → nvs key` for all ~97 registered
  parameters; `research/nvs-keys.csv`.
- **Acceptance:** CSV committed; **key collisions explicitly checked and reported**
  (the C++ has no collision detection on a 32-bit hash).
- **Uncertainty:** if two parameters collide, that is a live bug in the shipped
  firmware. Stop and report rather than working around it.

### DOMAIN-1 — state ids and the pre-emptive safety chain
- **Kind:** implementation. **HW:** no. **Prereq:** none.
- **Objective:** port the 18 state ids with their numeric values and
  `BaseState::checkTransitions` exactly, per [architecture.md](architecture.md) C4/C5.
- **Files:** `crates/cc-domain/src/state/{mod,ids,transitions}.rs`.
- **Steps:** ids as `#[repr(u8)]` with explicit discriminants; replace the range
  predicates with **explicit sets** and assert they agree with the ranges; port the
  chain in order (emergency > sensor error > tank empty > PID disabled > specific)
  with the per-state exclusions.
- **Acceptance:** `just test` passes; one test per state × per pre-emptive
  condition; a test asserting each category predicate matches the C++ numeric range.
- **Safety:** this is the contract that makes every state fail safe. Review the diff
  against `docs/state-machine-architecture.md` and ADR 0003 line by line.
- **Rollback:** the crate is new; revert the commit.
- **Uncertainty:** none expected — this is the best-documented part of the firmware.

### DOMAIN-2 — interlocks and `HeaterCommand`
- **Kind:** implementation. **HW:** no. **Prereq:** DOMAIN-1.
- **Objective:** make C1 structural — a heater duty cannot exist without interlocks
  having been consulted.
- **Files:** `crates/cc-domain/src/interlocks.rs`, `src/heater.rs`.
- **Steps:** port the `shouldPIDBeEnabled` rule set (PID disabled, sensor error,
  emergency, EEPROM error, standby, **all** backflush states including
  `BACKFLUSH_IDLE`, brew PID delay, and tank-empty unless `keepHeaterOnEmpty`); make
  `HeaterCommand::duty(requested, &Interlocks)` the only constructor.
- **Acceptance:** `just test`; a property test over the interlock powerset asserting
  duty is 0 whenever any blocking interlock is set; a test that `HeaterCommand`
  cannot be constructed another way (compile-fail test).
- **Safety:** **this is the most safety-relevant task in the list.** It closes the
  C++ window where a stale non-zero duty survives into `EMERGENCY_STOP`.
- **Declared change:** interlocks move from "race to zero the output" to structural.

### DOMAIN-3 — PID port
- **Kind:** implementation. **HW:** no. **Prereq:** ORACLE-2, DOMAIN-2.
- **Objective:** port the vendored `PID_v1` preserving semantics so existing users'
  tuning numbers stay meaningful.
- **Files:** `crates/cc-domain/src/control/pid.rs`.
- **Steps:** reproduce conditional integration when saturated (`P_ON_E` only), the
  EWMA derivative filter (`P_ON_E` only), double anti-windup, `SetTunings` folding
  sample time in, `SetSampleTime` rescaling, and bumpless transfer.
- **Acceptance:** replay `research/pid-golden.csv`; every step within 1e-9 of the
  C++ output at `windowSize = 1000`. `just test` passes.
- **Declared change:** the derivative uses a float `dt` instead of integer
  `SampleTime / 1000` — identical at 1000 ms, and removes a division by zero below it.
- **Uncertainty:** if any golden vector cannot be matched, stop and report rather
  than adjusting the tolerance.

### DOMAIN-4 — config schema, validation and NVS key derivation
- **Kind:** implementation. **HW:** no. **Prereq:** ORACLE-3.
- **Objective:** the ~97 registered parameters with ranges, defaults and the exact
  key derivation.
- **Files:** `crates/cc-domain/src/config/`.
- **Steps:** port the schema; implement `nvs_key()` as a pure function; **register
  `emergencyStopTemp` and `emergencyStopHysteresis`**, which the C++ declares but
  omits from its registry.
- **Acceptance:** every row of `research/nvs-keys.csv` reproduced exactly; range
  validation tested at both bounds; `just test` passes.
- **Declared change:** the two emergency parameters become persisted and exported.
- **Uncertainty:** string length limits are currently unenforced in the C++.
  Reproduce that for now and note it; changing it is a separate decision.

### DOMAIN-5 — emergency stop, brew, steam, backflush, standby, maintenance
- **Kind:** implementation. **HW:** no. **Prereq:** DOMAIN-1, DOMAIN-4.
- **Objective:** the remaining handler and coordinator logic.
- **Acceptance:** behavioural tests mirroring the existing C++ suites;
  `just test` passes.
- **Declared change:** the emergency debounce becomes time-based (N × 10 ms ticks)
  rather than counting loop iterations. Calibrate to match the C++'s effective
  timing at its normal loop rate and **record the measured equivalence** (PARITY-2).
- **Uncertainty:** the missing manual-brew, steam and manual-flush timeouts are
  reproduced as-is. Adding them is a user decision — see
  [architecture.md §8](architecture.md).

### DOMAIN-6 — ZACwire/TSIC decoder as a pure function
- **Kind:** implementation. **HW:** no. **Prereq:** none.
- **Objective:** decode a symbol slice (level + duration pairs) into °C.
- **Files:** `crates/cc-domain/src/tsic.rs`.
- **Steps:** classify each low pulse against the ~62.5 µs strobe threshold; two
  9-bit packets; even parity; the 221/222 sentinels; the 0–180 °C reject; the
  two-stage change-rate gate (200 then 5).
- **Acceptance:** synthetic symbol slices decode correctly, including parity
  failures and out-of-range rejects; `just test` passes.
- **Declared change:** none — **keep the vendored library's conversion**
  `((raw * 250) >> 8 - 499) / 10.0`, not the datasheet's, to preserve users'
  calibration offsets. Say so in a one-line comment.

### DOMAIN-7 — display layout and the U8g2 anchor shim
- **Kind:** implementation. **HW:** no. **Prereq:** SPIKE-5.
- **Objective:** port the render pipeline onto `embedded-graphics` with pixel parity.
- **Steps:** port the layout maths and `ModernTemplate` literals **as literals**
  (`kFontHeightFub20 = 23`, `kFontHeightProfont17 = 15` are hand-measured, not font
  metrics); implement the anchor shim — render at
  `VerticalPosition::Baseline`, `y + max(ascent_A, ascent_para) + 1`.
- **Acceptance:** golden-image tests; the shim verified against the measured
  deltas (0 px `fub20`, 2 px `profont17`, 1 px `profont11/10`).

**Phase 1 exit gate:** `just fmt-check`, `just lint`, `just test` all pass; the PID
golden vectors and NVS key vectors both reproduce exactly; `just build esp32`
still succeeds. No hardware needed.

---

## Phase 2 — feasibility spikes (research; mostly `HW: esp32`)

These run early because they are the tasks that can invalidate the design. Each is
**research only** — it produces a finding, not production code.

### SPIKE-2 — first flash and runtime smoke test
- **Kind:** research. **HW:** esp32. **Prereq:** **P0**.
- **Objective:** prove a Rust image boots on this device and that the toolchain's
  flash path works. Everything in ADR 0004 is compile-level until this passes.
- **Safe test procedure — no actuator is energised:**
  1. **Physically disconnect the heater, pump and valve relay board**, or confirm
     the relay board is unpowered. Record which.
  2. Confirm the chip: `just board-info <port>` must report `esp32` rev v3.0.
  3. Back up the device first: read and save the partition table (`0x8000`, 0x1000),
     NVS (`0x9000`, 0x5000) and `otadata` (`0xe000`, 0x2000).
  4. Flash a firmware whose `main` **only** safes the outputs and logs — no PID, no
     state machine, no pump or valve command.
  5. `just monitor <port>`; confirm boot, no panic, no reset loop for 5 minutes.
  6. Measure the heater, pump and valve pins with a meter and confirm each sits at
     its inactive level.
- **Acceptance:** recorded boot log; measured pin levels; free-heap figure.
- **Rollback:** re-flash the saved images.
- **Uncertainty:** `exit(0)` semantics (inventory §3.2) can be observed here as a
  side note. Also observe whether the relay pins float between reset and first
  instruction — [architecture.md §4.1](architecture.md) open question 1.

### SPIKE-3 — NVS config continuity against a real device
- **Kind:** research. **HW:** esp32 **with the C++ firmware and real config**.
- **Prereq:** P0 question 2.
- **Objective:** prove the Rust firmware reads config written by the C++ firmware.
  This is the claim that protects deployed machines.
- **Steps:** flash the C++ firmware; set a range of parameters through the web UI
  covering `bool`, `int`, `double` and `String`; `just nvs-report <port>` and keep
  the digests; flash a Rust image that reads and logs every parameter; compare.
- **Acceptance:** every parameter matches. **Doubles and floats must be read via
  `get_blob` + `from_le_bytes`** — `get_u32`/`get_u64` will silently give garbage.
- **Safety:** read-only on the Rust side; no actuator involved.
- **Uncertainty:** currently **blocked** — the attached device's NVS is blank, so
  there is nothing to read back. This is the single most important unverified claim
  in the whole plan.

### SPIKE-4 — ZACwire decode via RMT against real TSIC hardware
- **Kind:** research. **HW:** esp32 + TSIC 306 on GPIO 16.
- **Objective:** confirm RMT RX capture recovers the waveform reliably with Wi-Fi
  active. This is the only timing-critical driver.
- **Steps:** configure `RxChannelDriver` at 1 µs resolution, glitch filter ~2–5 µs,
  idle threshold ~200–300 µs; log raw symbol slices; decode with DOMAIN-6; run for
  an hour with Wi-Fi connected and traffic flowing; count parity and range failures.
- **Acceptance:** ≥99.9 % of readings decode; values agree with the C++ firmware's
  reported temperature within 0.2 °C.
- **Safety:** sensor read only; heater interlocked off for the whole run.
- **Uncertainty:** if RMT proves unreliable, the fallback is GPIO edge interrupts
  (what the C++ does) with a ±31 µs margin. **PCNT cannot work** — it counts edges
  and cannot measure pulse width.

### SPIKE-5 — u8g2-fonts pixel parity
- **Kind:** research. **HW:** no (device optional for a visual check).
- **Objective:** confirm `u8g2-fonts` renders identically to U8g2's C renderer for
  the two fonts in use.
- **Steps:** render the same strings with both and diff the bitmaps; verify the
  quantified anchor deltas; confirm only the named fonts get linked.
- **Acceptance:** byte-identical glyph bitmaps, or a documented shim that makes the
  rendered output identical.
- **Uncertainty:** the crate's own tests are golden PNGs of its own output, not a
  cross-check against U8g2. If parity fails, options are a hand-rolled renderer or
  accepting a visual change — a user decision.

### SPIKE-6 — SSE under `EspHttpServer` socket limits
- **Kind:** research. **HW:** esp32. **Prereq:** SPIKE-2.
- **Objective:** the web UI's live telemetry depends on SSE. `max_open_sockets`
  defaults to 4 and `lru_purge_enable` defaults to true, which will evict an idle
  SSE stream.
- **Steps:** hold an SSE stream open while issuing the 6–10 parallel API requests
  ADR 0002 describes; vary both settings; measure heap.
- **Acceptance:** a stream survives a full UI load; a recorded socket budget.
- **Uncertainty:** may force a different telemetry transport (polling, or WebSocket
  — noting `esp-idf-svc#666` affects only the WS *client*).

### SPIKE-7 — provisioning on-device
- **Kind:** research. **HW:** esp32. **Prereq:** SPIKE-2, P0.
- **Objective:** confirm the USB NVS-write path actually results in a Wi-Fi
  connection, and decide the long-term mechanism.
- **Steps:** `just provision <port>` against a blank-NVS device; boot; confirm
  association. Then evaluate reprovisioning and the merge problem.
- **Acceptance:** device associates using credentials never printed or committed.
- **Safety:** credentials from `.env` only; nothing echoed.
- **Uncertainty:** the write half of the transport is unverified (see
  [tooling.md §6.2](tooling.md)). A true NVS merge or the NET-5 serial command is
  the better long-term answer.

**Phase 2 exit gate:** every spike has a recorded verdict. Any spike that fails
updates [compatibility-matrix.md](compatibility-matrix.md) and this list **before**
Phase 3 starts.

---

## Phase 3 — board layer and drivers

### BOARD-1 — pin map and `cc-hal` implementations
- **Kind:** implementation. **HW:** no to build, esp32 to validate. **Prereq:** DOMAIN-1.
- **Files:** `firmware/cc-board/src/{pins,gpio,clock}.rs`.
- **Steps:** pin map as constants from `pinmapping.h`; `DigitalOut`/`DigitalIn`/
  `Clock` over `esp-idf-hal`. **Do not port the C++'s silent no-op on writing to a
  non-output pin** — make it a type error.
- **Acceptance:** `just build esp32`, `just lint` clean.
- **Safety notes to carry into the code as one-line comments: `PIN_HEATER = 2` is a
  strapping pin; `PIN_STEAMLED = 1` is UART0 TX and collides with the console.**
- **Uncertainty:** the water-tank switch polarity is inverted relative to the other
  four switches in the C++ and one of them is wrong. **Stop and ask** rather than
  picking.

### BOARD-2 — safe output initialisation
- **Kind:** implementation. **HW:** esp32 to validate. **Prereq:** BOARD-1.
- **Objective:** [architecture.md §4.1](architecture.md) step 1 — heater, pump and
  valve driven inactive as the first statement in `main`, before anything fallible.
- **Steps:** read the three relay trigger-type keys straight from NVS, defaulting to
  `LOW_TRIGGER` (drive HIGH) if absent, before the config registry initialises.
- **Acceptance (ACTUATOR-adjacent, so measured):** with the relay board
  **disconnected**, measure each pin from reset and confirm it reaches its inactive
  level; record the time to safe.
- **Safety:** **safe test procedure = relay board disconnected for the measurement.**
  This task exists to *prevent* unexpected actuation; it must never be validated
  with actuators connected.
- **Declared change:** fixes the C++ window where a low-trigger relay is briefly
  energised between `pinMode()` and `off()`.

### BOARD-3 — heater PWM and the ISR
- **Kind:** implementation. **HW:** esp32. **Prereq:** BOARD-2, DOMAIN-2.
- **Objective:** GPTimer at 10 ms driving the relay from a single `AtomicU16`.
- **Steps:** `TimerDriver::new(&TimerConfig)` (no peripheral argument in
  `esp-idf-hal` 0.47), `set_alarm_action(Some(&AlarmConfig{ alarm_count: 10_000,
  auto_reload_on_alarm: true, .. }))`, `enable()`, `start()`. The ISR touches one
  atomic and one GPIO write — no allocation, no locks, no logging.
- **Acceptance (ACTUATOR):** see the safe procedure below. Verify with a scope or
  logic analyser: 1000 ms period, 10 ms resolution, duty within one tick.
- **Safe test procedure:**
  1. **Heater relay output disconnected from the heating element.** The element must
     be physically isolated — verify, do not assume.
  2. Measure at the relay *input* pin, or use an LED on the relay output.
  3. Set duty from software only, starting at 0, then 100, 500, 1000, back to 0.
  4. Confirm duty 0 holds the pin inactive indefinitely.
  5. Reconnect the element only after an explicit user decision, which is outside
     this task.
- **Safety:** this is the task that can energise a heating element. It does not
  proceed without the element isolated.
- **Rollback:** the ISR is the only writer of the heater pin; reverting removes it.

### DRV-1 — DS18B20 via `onewire` 0.4.0
- **Kind:** implementation. **HW:** esp32 + DS18B20 to validate.
- **Acceptance:** host tests against a mock bus; on-device reading within 0.2 °C of
  the C++ firmware's.
- **Uncertainty:** the crate does bit timing with `DelayNs`; confirm it holds under
  Wi-Fi load. Low risk — DS18B20 is the fallback sensor and 1-Wire is forgiving.

### DRV-2 — Honeywell ABP2 (ours)
- **Kind:** implementation. **HW:** esp32 + sensor to validate.
- **Steps:** `embedded-hal` 1.0 `I2c` + `DelayNs`; write `{0xAA,0x00,0x00}`, wait
  10 ms, read 7 bytes; **check the status byte** and every read result, which the
  C++ ignores (a NAK currently yields 0xFF counts ≈ 10 bar).
- **Acceptance:** host tests including NAK and short-read paths; on-device pressure
  matching the C++ within 0.05 bar.
- **Declared change:** the temperature transfer function is corrected to
  `counts * 200 / 16777215 - 50`; the C++'s `* 270 / … - 40` is wrong.
- **Note:** `control` reads pressure with `try_lock` on the I²C bus and **skips the
  sample** if busy — see [architecture.md §5](architecture.md).

### DRV-3 — TSIC RMT glue
- **Kind:** implementation. **HW:** esp32 + TSIC. **Prereq:** SPIKE-4, DOMAIN-6.

### DRV-4 — OLED
- **Kind:** implementation. **HW:** esp32 + OLED. **Prereq:** DOMAIN-7, SPIKE-5.
- **Steps:** `ssd1306` 0.10.0 for SSD1306, `oled_async` for SH1106, both behind one
  `DrawTarget`; runtime device selection from config as today.
- **Uncertainty:** if `oled_async` disappoints, a hand-rolled SH1106 page writer is
  ~150 LOC (132-column RAM, 2-column offset, no horizontal addressing).

**Phase 3 exit gate:** `just lint`, `just test`, `just build esp32`, `just size
esp32` under 85 %; and **on-device**: boot, sensor readings matching the C++ within
tolerance, PWM verified with the heating element isolated.

---

## Phase 4 — application layer

### APP-1 — task wiring
- **Kind:** implementation. **HW:** esp32. **Prereq:** Phase 3.
- **Objective:** the five tasks and three primitives from
  [architecture.md §2](architecture.md).
- **Acceptance:** control-tick jitter measured and recorded; watchdog subscribes
  `control` and `ui` only; a deliberately stalled `control` causes a reset.
- **Safety:** heater interlocked off for the whole task.

### APP-2 — config store over NVS
- **Kind:** implementation. **Prereq:** DOMAIN-4, SPIKE-3.
- **Objective:** `KvStore` over `EspNvs` with the Preferences encoding, floats and
  doubles as little-endian blobs.
- **Declared change:** writes are batched through the `storage` task instead of one
  open/write/close per parameter on the network task.

### NET-1 — Wi-Fi supervision
- **Kind:** implementation. **Prereq:** APP-1.
- **Declared change:** fix the C++ wart where `WiFi.status()` is checked
  synchronously right after an async `begin()`, over-counting circuit-breaker failures.

### NET-2 — HTTP server
- **Kind:** implementation. **Prereq:** NET-1, SPIKE-6.
- **Objective:** reproduce `docs/api/openapi.yaml` exactly, with the four known
  mismatches fixed: implement `POST /api/config`; document `/api/wake`,
  `/api/sleep`, `/events`; fix the `servers` URL; document the auth scheme.
- **Steps:** gzip-precompressed static serving from the filesystem partition; a
  multipart parser for uploads (~100–150 LOC, host-testable); **no blocking calls in
  handlers** — the C++ breaks this in four places.
- **Acceptance:** every endpoint exercised with `curl`; a spec-conformance test.
- **Safety:** endpoints that command actuators go through the `Command` queue and
  are subject to interlocks. **Do not reintroduce auth-off-by-default with wildcard
  CORS on firmware upload and factory reset** — flag it as a product decision.

### NET-3 — MQTT
- **Kind:** implementation. **Prereq:** NET-1.
- **Objective:** the topic scheme, all published topics, the `+/set` subscription
  and Home Assistant discovery.
- **Declared changes to propose (each needs a nod, not a silent fix):** normalise
  the topic prefix's trailing slash; make retain consistent across numeric sensors;
  stop coercing all inbound payloads to `double`, which currently makes string
  parameters unsettable and leaves a value uninitialised on garbage input.

### NET-4 — OTA
- **Kind:** implementation. **HW:** esp32. **Prereq:** NET-2.
- **Objective:** `EspOta` for the app, `EspPartition` for the filesystem image by
  label, and A/B rollback.
- **Acceptance:** OTA from the running Rust firmware to a new Rust firmware, then a
  deliberate bad image to prove rollback.
- **Safety:** all actuators forced safe for the whole session — the C++ disables the
  timer and heater first and so must this. **Unlike the C++, `control` keeps running**
  with the heater interlocked rather than the whole loop returning early.
- **Uncertainty:** the filesystem partition label is a P0 question (`spiffs` vs
  `littlefs`).

### NET-5 — serial provisioning command
- **Kind:** implementation. **HW:** esp32. **Prereq:** SPIKE-7.
- **Objective:** a console command so reprovisioning does not need download mode or
  a whole-partition NVS rewrite.
- **Safety:** the command must never echo the password.

### UI-1 — display rendering on-device
- **Kind:** implementation. **HW:** esp32 + OLED. **Prereq:** DRV-4, APP-1.

**Phase 4 exit gate:** `just lint`, `just test`, `just build esp32`, size under
85 %; on-device: Wi-Fi connects, the web UI loads from the filesystem partition,
MQTT publishes and accepts commands, OTA round-trips with rollback, the display
matches the C++ side by side.

---

## Phase 5 — parity and cutover

### PARITY-1 — PID equivalence on-device
- **Kind:** research. **HW:** esp32. **Prereq:** DOMAIN-3, BOARD-3.
- **Objective:** same setpoint and tunings produce the same duty trace on both
  firmwares.
- **Safe procedure:** heating element isolated; compare computed duty from logs, not
  actual heating. A real heat-up comparison is a separate, user-approved step.

### PARITY-2 — emergency debounce equivalence
- **Kind:** research. **HW:** esp32. **Prereq:** DOMAIN-5.
- **Objective:** the time-based debounce must fire at the same wall-clock time as
  the C++'s iteration-based one at its normal loop rate.
- **Steps:** measure the C++ loop rate on-device; derive the tick count; inject a
  synthetic over-temperature through the sensor abstraction and compare timings.
- **Safety:** injection at the sensor abstraction, **not** by actually overheating.

### PARITY-3 — state machine trace equivalence
- **Kind:** research. **HW:** esp32. **Prereq:** Phase 4.
- **Objective:** an identical input sequence (switch presses, tank events, timeouts)
  produces an identical state-transition trace on both firmwares.
- **Safe procedure:** pump and valve **disconnected**; compare traces from logs.
  Brew and steam states are exercised without water or pressure.
- **Acceptance:** identical traces, or every difference explained and accepted.

### PARITY-4 — web API and MQTT conformance
- **Kind:** research. **HW:** esp32. **Prereq:** NET-2, NET-3.
- **Objective:** diff every endpoint response and every MQTT topic between the two
  firmwares. Differences must be either bugs to fix or declared changes.

### CUT-1 — full-function validation with actuators (**needs user approval**)
- **Kind:** research. **HW:** esp32 + complete machine.
- **Objective:** the first run that actually heats water and pulls a shot.
- **This task does not begin without the user's explicit go-ahead and a written
  procedure agreed with them.** It requires a real machine, water in the tank,
  supervision, and a physical means of cutting power. It is listed so it is not
  forgotten, not so an agent can decide to do it.

### CUT-2 — release workflow
- **Kind:** implementation. **Prereq:** PARITY-1..4.
- **Objective:** extend `release.yml` to publish the Rust artefacts alongside the
  C++ ones during the transition; keep flash offsets and the merge command correct.
- **Note:** fix `README.md`'s merge command, which omits `littlefs.bin` and leaves
  the device with no web UI.

**Phase 5 exit gate:** every parity task recorded; the C++ firmware remains
buildable; a documented list of accepted behaviour differences.

---

## Cross-cutting cleanups

Small, independently valuable, and safe to do any time. Each references its
inventory finding.

| ID | Kind | Task | Finding |
|---|---|---|---|
| **CLEAN-1** | implementation | Remove `test/.pioignore` entries for the two non-empty suites (~460 lines of tests never run) | §7.4 32 |
| **CLEAN-2** | implementation | Reconcile the test-count claims: 280 vs 234 vs the actual 303 | §7.4 |
| **CLEAN-3** | implementation | Fix `test/TESTING_GUIDE.md`'s example, which names an ignored suite | §7.4 |
| **CLEAN-4** | implementation | Remove or create `platformio_extra.ini`; four CI cache keys hash a missing file | §7.4 31 |
| **CLEAN-5** | implementation | Settle on one clang-format version (three are in play) | §7.4 33 |
| **CLEAN-6** | implementation | Fix the `-std=gnu++2a # use C++23` comment — it is C++20 | §7.4 34 |
| **CLEAN-7** | implementation | Remove the stale `#include <os.h>` from `main.cpp` | §7.4 39 |
| **CLEAN-8** | implementation | Fix `README.md`'s merge command to include `littlefs.bin` | §7.4 36 |
| **CLEAN-9** | implementation | Reconcile `CONTRIBUTING.md`'s `develop` branch with CI, which only uses `main` | §7.4 37 |
| **CLEAN-10** | implementation | Fix `DEBUG_GUIDE.md`'s contradictory ISR rate (~100/s is right) | §7.4 38 |
| **CLEAN-11** | implementation | Mark `docs/plan/task-list.md` superseded; merge `docs/plan/` and `docs/plans/` | §7.4 40, 42 |
| **CLEAN-12** | implementation | Normalise ADR 0003's heading to `# ADR 0003:` | §7.4 42 |
| **CLEAN-13** | research | Verify the licences of the 11 unverified libraries and the vendored PID library, which declares none | §7.4 41 |

---

## Deferred

Recorded so they are not silently dropped.

| Item | Why deferred |
|---|---|
| HX711 scale driver | The scale is **dead code** in the C++ — never instantiated, `getWeight()` returns 0.0. Reviving it is a feature decision, not a migration task. |
| Acaia BLE scale | Same. Also needs a protocol codec written from scratch (no crate) and has a `trouble-host`/`esp-radio` version-skew problem on the no_std path. |
| Wi-Fi + BLE coexistence | Only matters if the scale is revived. Unhandled in the C++ too, and there is no evidence it was ever tested. |
| Captive-portal provisioning | v1 provisions over USB. The portal is a user-facing nicety once the core works. |
| Flash encryption / secure boot | A one-way efuse burn and a product decision. Currently `Security features: None`. |
| `i2c_master` wrapper | Only needed when moving past ESP-IDF 6.x, where the legacy I²C driver `esp-idf-hal` wraps is removed. |
| Rotary encoder, dimmer zero-cross | Pins are declared in `pinmapping.h` and referenced nowhere. Not implemented in the C++ either. |
