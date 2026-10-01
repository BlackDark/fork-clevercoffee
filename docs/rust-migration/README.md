# Rust Migration

Plan for migrating the CleverCoffee ESP32 firmware from C++/Arduino to Rust.

**Status:** in progress, and the machine works end to end: the reducer runs on
hardware, the display lights up, the PID regulates, all 98 parameters are writable
over HTTP and survive a reboot, and the **web UI is on the device**. **Not done:**
OTA (R3-15), the Acaia BLE scale (R3-18 — measured, does not fit, needs a
decision), and any hand-pressed switch. See
["Where the migration actually is"](#where-the-migration-actually-is) before
planning work — several task IDs read as complete in the task list and are not.
**Started:** 2026-09-28.
**C++ baseline verified green:** `pio run -e esp32_usb` succeeds (`firmware.bin`
1,546,240 B); `pio test -e native_test` → 340/340 pass in 55 s. The C++ is the parity
baseline and is **never modified or flashed** during the port.

---

## The device

| | |
| --- | --- |
| Board | ESP32-DevKitC V4, **ESP32-WROOM-32E** (original ESP32, Xtensa LX6, rev v3.0) |
| MAC | `ec:62:60:76:b5:3c` |
| Serial | `/dev/cu.usbserial-204140` (WCH CH340, **not** CP2102N) |
| Flash | 4 MB, DIO @ 40 MHz, no PSRAM |
| USB bridge | CH340 — the chip has **no native USB**; the port is a UART bridge |
| **`system.hostname`** | **`test-cc-rust`** — `cc_config::schema::DEFAULT_HOSTNAME` |
| Link speed | 115200. **The port is unreliable above ~460800.** |

### The hostname is not the product name

The Rust firmware defaults to **`test-cc-rust`**, not the C++'s `silvia`
(`include/clevercoffee/defaults.h:14`). This is deliberate and load-bearing during the
migration: **both firmwares run on the same network and the C++ is not
interchangeable with the Rust** — the port diverges on pump timeouts, the steam-valve
whitelist and the PID divide. A hostname that says which firmware answered is worth more
than a brand-neutral one. The C++ is unchanged and still answers to `silvia`.

One definition: `cc_config::schema::DEFAULT_HOSTNAME`. Change the name there, and
`docs/example_config.json` with it — an existing import test parses that exact file, so
the two cannot drift apart. Full rationale in
[intentional-diffs.md §12](./intentional-diffs.md#12-the-devices-default-hostname-is-test-cc-rust-not-silvia-).

> `mqtt.password`'s default is *also* `"silvia"`. That is a **credential, not a name**,
> and it is deliberately left alone.

---

## Documents

Read in this order.

| # | Document | What it answers |
| --- | --- | --- |
| 01 | [Feature inventory](./01-feature-inventory.md) | What does the firmware do today, on what hardware, with which libraries — and which 11 control paths are safety-critical? |
| 02 | [Research and compatibility matrix](./02-research-compatibility-matrix.md) | Which Rust crates cover which feature, at which versions, with which gaps? What is verified vs. assumed? |
| 03 | [Decision record (ADR-0004)](./03-decision-record.md) | `esp-idf-svc` or bare metal? What was rejected, and what would make the decision wrong? |
| 04 | [Target architecture](./04-target-architecture.md) | Crate layout, task boundaries, priorities, single-ownership rules, startup and shutdown. |
| 05 | [Tooling and workflows](./05-tooling-and-workflows.md) | mise setup, the `just` recipes, flashing rules, Wi-Fi provisioning, CI. |
| 06 | [Migration task list](./06-migration-task-list.md) | R0–R4, one task at a time, with dependencies, gates, acceptance criteria, and the 33-suite test coverage map. |
| 07 | [Image size budget](./07-image-size-budget.md) | The 154 KiB problem, the rebalance arithmetic, the drop order, and the per-gate size report. |
| 08 | [Recovered oracle](./08-recovered-oracle.md) | A complete Rust firmware that previously ran on this board, recovered from a flash dump. Source is gone; design decisions, partition table, config schema and safety design are the only surviving record. |
| 09 | [C++ findings](./09-cpp-findings.md) | Every bug and ambiguity found in the C++ while porting it, each pinned by a named parity test — and which of them have since been closed on purpose. |
| 10 | [Parity scenario format](./10-scenario-format.md) | The declarative format `just parity` replays: the seven stimulus kinds, what is captured, and what an assertion can claim. Read this before adding a scenario. |
| — | [**Intentional divergences**](./intentional-diffs.md) | Where the Rust firmware **deliberately differs** from the C++ it replaces, why, and the test that pins each one. A parity diff here is expected, not a regression. Start here when a diff appears. Carries the machine-readable `ledger` blocks `just parity` classifies diffs against. |

Execution guidance lives in the agent skill:
[`../../.agents/skills/esp32-rust-migration/SKILL.md`](../../.agents/skills/esp32-rust-migration/SKILL.md).

---

## The short version

**Target.** An **ESP32-DevKitC V4** with an **ESP32-WROOM-32E** = the **original ESP32**
(Xtensa LX6). "v4" is the board's PCB revision, not a chip variant. It has WiFi +
Bluetooth/BLE and **no USB peripheral** — the Micro-USB port is a CP2102N UART bridge,
with Boot/EN buttons for the manual flash path. There is no S3, C3, or C6 in this
project. Migrate to **`esp-idf-svc` 0.53.0** on ESP-IDF v5.5.5 with `std`, using FreeRTOS
tasks and a functional-core control loop.

**Why.** Bare metal (`esp-hal` + `esp-radio` + embassy) has no HTTP server, no OTA, no
NVS, and no filesystem, and a verified TCP/IP stack for `esp-radio` does not exist.
Building those is a multi-month project that would dwarf the firmware it serves. The
`esp-idf-*` crates are community-maintained, but they are the only option that covers the
feature set.

**Decided 2026-09-28** — do not re-litigate: updates are a forced full flash over USB;
**no NVS backward compatibility** (Rust owns its own key namespace); **HTTP OTA is
optional** and is the first thing to drop if the image gets tight; the **React UI in
`ui/` stays as-is** unless R1-05 forces WebSocket instead of SSE.

**Biggest risks.**

1. **Flash.** The C++ image is 1,546,240 B against a 1,703,936 B app slot — ~154 KB
   headroom. A Rust esp-idf image will not fit. R0-02 and R2-03 rebalance the partition
   table.
2. **TSIC-306 temperature sensor.** No Rust crate exists; the protocol is proprietary and
   ISR-timing sensitive. R1-03 is the spike that can invalidate the whole approach.
3. **Toolchain.** The original ESP32 needs the esp-rs Xtensa compiler fork, which mise
   cannot fully manage. It is the most fragile dependency in the stack.
4. **`esp-wifi-provisioning` is a trap.** It forces `esp-idf-hal/rmt-legacy`, which
   removes `hal::onewire` and the GPTimer module from the whole graph. R1-06 checks this
   before adopting it.

**First task.** R0-01: confirm the physical board (module, silicon revision, flash size,
PSRAM, and whether the auto-reset circuit is present). It is blocked — **no ESP32 device
is currently attached to this machine.**

---

## Where the migration actually is

Recorded 2026-09-30, after R4-01 and R3-09 landed and were exercised on hardware.

### Verified working on the board

- **The reducer runs on hardware.** `cc_machine::reduce` is wired into the control
  task (`crates/cc-firmware/src/control.rs`); effects are applied through
  `cc-hal-esp32/src/actuators.rs` in the same tick. The machine boots to
  `PidNormal` and the PID drives the heater.
- **PID, and the specific check the human asked for.** At a **30 °C** target with
  a ~7 K error the duty settles at **~48 %** — P proportional to error, I ramping
  to its `i_max`, D decaying. At 95 °C with a 72 K error it correctly goes to
  **100 %**. Both measured, both reproducible from the web UI.
- **The display.** `present=true`, `frames=125` per 60 s, `failed=0`. The SSD1306
  is driven over an I²C bus **shared with the ABP2** behind a `Mutex`; the frame
  is chunked into 8 bus writes, not 64, so the pressure sensor is not starved.
  The flash the human reported was the panel being re-`INIT_SEQUENCE`'d every
  frame — its `0xAE`/`0xAF` pair switching the display off and on at 10 Hz.
- **All 98 parameters are writable and persist across a reboot** —
  `POST /api/parameters`, ported from `WebServerManager.cpp:813-886`. Verified for
  bool, int, float and text. This is what closed "parameters can be configured".
- NVS, Wi-Fi STA, UART provisioning (round trip survives a reboot), MQTT, HTTP + SSE
  (25 routes), the 10 ms heater ISR, DS18B20 and TSIC-306, the HX711 scale, and
  **131 device tests that actually run on hardware** via `just test-esp32`.
- **The web UI is on the device and it renders.** `GET /ui` serves the React SPA
  from **flash**, not from a mounted filesystem: `crates/cc-hal-esp32/build.rs`
  embeds the gzip build output with `include_bytes!`, which is why a 199,270 B
  bundle costs **0 B of RAM** (static RAM is 133,168 B before and after). One
  `/ui*` wildcard route serves the shell, the assets and the client-side routes,
  with the MIME types checked (`text/html`, `application/javascript`,
  `text/css`, `image/png`) and every asset served byte-for-byte identical to
  the Vite build. Verified in Chrome on the device: navigation, Machine Status,
  Machine Functions and Maintenance all render — and a **deep link to a
  client-side route** (`/ui/config/behavior`) boots the Configuration page with
  **104 live parameters**, which only works if the SPA fallback serves the shell
  *and* the MIME types let the JS and CSS actually execute. A `200` on `/ui` is
  not evidence of any of that. The size arithmetic and the embed-vs-mount
  decision are in [07 §13](./07-image-size-budget.md).

### Fixed on hardware 2026-10-01

Nine defects the human reported, all reproduced first and then fixed, all
re-verified on the board:

- **The control loop ran at 2.5 Hz, not the 100 Hz this document specifies.**
  `CONTROL_TICK_MS` was 400 ms and the display frame was written *inside* that
  tick, which is why a switch press took half a second to show up and why the
  tick overran its 10 ms budget in ~62 % of ticks. The control task now runs at
  **100 Hz** and the panel has **its own task** at its own 100 ms interval, which
  is the one task boundary 04 §2 gained. A sensor task was tried as well and
  **removed**: the DS18B20's bit-bang asserts inside the `FreeRTOS` kernel when
  it runs on a second task (09 §28, with the bisect).
- **`GET /api/history` was a stub.** It now answers a 600-point ring, one point
  every three seconds, oldest first — the C++'s `TemperatureHistory` exactly,
  including the skip interval the UI's x-axis assumes.
- **`POST /api/parameters` answered before the value was live**, so saving a
  parameter and refetching returned the old one and the toggle sprang back. The
  control task now publishes the values *immediately* after applying them, and
  the handler waits — bounded — for that acknowledgement instead of hoping.
- **The backflush reminder read `0/0`.** The threshold and the enabled flag were
  never published into `/api/status`; both are now, and the reminder's due
  computation is the C++'s.
- **There was no startup screen.** `displayLogo` is ported: the version, then the
  Wi-Fi address.
- **The panel blanked the instant standby began.** The ten-minute display-off
  countdown existed in `cc-machine` as a *declared but never ported* field; it is
  ported now, and the panel keeps showing the standby screen until it expires.
- **The header time lost its `m` and the degree `C` was off the panel** past a
  100-hour uptime — a fixed `x` and a minimum-width format. Both are laid out
  from the frame edge now ([intentional-diffs §14](./intentional-diffs.md)).
- **The log said `TSIC_306` next to a DS18B20.** It was never aliased — the
  driver is selected by a `const` that says `DallasDs18b20` — but the line read
  like the configuration had been ignored, and the configured value is `1`. The
  log now names the driver in use and says where the other setting lives.
- **The first `GET /api/history` and two on-target tests crashed the device**,
  because a 7.2 KB ring and a 7.2 KB return value were on 8 KB task stacks. Both
  are heap now; the on-target suite is back to 145 passing, 0 failing, 1 pre-existing
  LOST.

### Fixed on 2026-10-01, second pass — two more of the human's reports

- **"`hardware.sensors.temperature.type` does nothing."** It did nothing: the
  driver was chosen by a compile-time `const`, so selecting a `TSIC-306` on a
  board with a `DS18B20` carried on reading the 1-Wire probe. The configuration
  now chooses the driver, as it does in the C++, and the mismatch is *visible*:
  with `TSIC_306` selected the reading is `null`, the machine goes to
  `SENSOR_ERROR` with a zero duty, and the log says so once. The parameter is
  also in the reboot-required set, because the driver is constructed once at boot
  — without that, saving it answered `success` and nothing happened until a
  reboot.
- **A brew showed no timer.** The frame's `brew_timer` — the
  `Idle -> Running -> PostBrew` FSM that ADR-0001 §4 puts in `DisplayInput` — was
  **never stepped**, and `brew_time_ms` was never filled, so no template could
  ever show a timer. The control task now carries the `DisplayInput` between
  frames, feeds it `BrewProgress` and `BrewHandler::isBrewActive()`, and steps the
  FSM once per published frame. Each transition is logged, so a brew is provable
  from the console.
- The sensor's failure log was one line per read, which at the loop's rate is 50
  lines a second on a 115200-baud console. It is now one line per fault, re-armed
  by a good reading.

### Fixed on 2026-10-01, third pass — the language, and three flags that did nothing

- **The UI's language labels were swapped.** `display.language` shipped in
  `parameter-metadata.ts` as `0 = Deutsch, 1 = English` against the firmware's
  `English = 0, German = 1`, so choosing English in the UI wrote German and the
  panel came up German — reported as "I set the language to English and the OLED
  shows German". Fixed, and `cc-config`'s
  `the_ui_enum_labels_match_the_firmware_discriminants` now parses that table at
  compile time and pins all **seventeen** enum parameters against the firmware's
  discriminants, so a swapped pair, a renamed variant or a dropped option fails
  the build.
- **The fullscreen manual-flush and hot-water timers ignored their flags.** The
  C++ gates every fullscreen mode on `policy && config && state`
  (`DisplayTemplateBase.h:65-68`); the port had `policy && state`. Since the
  config side defaults to **false** in both firmwares, a stock machine showed two
  fullscreen screens the C++ never shows, and the hot-water one fires on
  ordinary hot-water and steam use. Both now take a `Config`, exactly as the brew
  timer always did.
- **Five `Config::default()` values disagreed with the C++'s `ParamDef`
  defaults** — all three fullscreen timers, `pid_off_logo` and
  `post_brew_timer_duration_s`. The type's doc comment claims they are the C++'s.
  This is what let the first two hide: `screen_matrix.rs`'s "everything off"
  config inherited them, so the checker meant to find the bug had the bug inside
  it. Twelve goldens changed as a result — and they were **wrong before**:
  `standard.ppm`, `scale.ppm` and `upright.ppm` were pictures of the fullscreen
  brew timer, so those three templates' normal layouts had never been
  golden-tested at all.
- **`hardware.oled.enabled` did nothing.** Now it gates the panel at bring-up —
  not the bus, because the ABP2 shares it — and "off" means the SSD1306 is never
  opened, which is what the C++ does by never setting the display pointer.
  Blank-on-stdby (`set_blank`, `0xAE`) is a different mechanism and is left
  alone.
- **The portrait sensor-error screen drew the landscape sentence.** The C++
  carries `langstring_error_tsensor_ur[5]` for the portrait screen
  (`languages.h:35,69-73`); the port had dropped it and fed the landscape strings
  to both, so a **64 logical pixel**-wide panel received 111 px of ink. Restored
  for all three languages.
- A false comment claimed the C++'s EEPROM screen "does not compile in C++ -- so
  this screen has never been reached". It does compile, the screen is reachable,
  and the port is faithful to it. Corrected, because a wrong note in a parity
  baseline is how correct code gets deleted later.

### Not done

- **R3-18, the Acaia BLE scale — measured, and it does not fit.** Enabling
  NimBLE and changing nothing else costs **+205,312 B flash** (headroom
  15.0 % → 3.8 %) and **+40,124 B static RAM** (133,168 → 173,292 B, i.e. 54 %
  of the ESP32's 320 KB). The RAM is the worse half, because ADR-0002 documents a
  production OOM from heap contention. Nothing was dropped to absorb it: the human
  said explicitly that both scales are kept, so this is a decision for them, not a
  default to apply. Two options that cost no feature are written up in
  [07 §14](./07-image-size-budget.md) — move the 199 KB web UI to the LittleFS
  partition, and shrink the font set, since **pixel parity is not a requirement**
  (the human's words: stay readable and in frame).
- **R3-15**, OTA: still a `unavailable_json` stub. The safety gap in 01 §6 — an
  OTA must leave pump and valve off — is therefore still open.
- **`/ui` is not done in one respect: the SSE stream.** The SPA itself is served
  from flash and renders on the device (see "Verified working" above), but
  `GET /events` answers `200 text/event-stream` and then **ends the response
  immediately with `Content-Length: 0`**, so the UI shows "Lost connection" and
  no live temperature. Cause, found 2026-09-30 and not yet fixed: when the
  `/events` handler returns, `esp-idf-svc`'s `EspHttpConnection::drop` calls
  `complete()` (`http/server.rs:1159-1166`), which — because `initiate_response`
  left `response_headers` pending — takes the `httpd_resp_send(len=0)` branch
  instead of the chunked one, and that *is* `Content-Length: 0`. The detached
  broadcaster's later `httpd_resp_send_chunk` writes then have nothing to write
  to. This is the async-detach path, it predates the UI work, and fixing it
  needs an ESP-IDF-level decision about how to keep `complete()` from firing.
  Until then the UI's live values come from polling, not the stream.
- The **telnet transport** that ADR-0002's heap shed is meant to protect. The
  shed logic is unit-tested but has no real client to shed.
- **Switch presses have still never been tested by hand.** The debounce and
  long-press are pinned by 17 host tests against a synthetic clock, and the
  switches are `enabled=true` by default at the human's request. Everything around
  a press is now fast — 10 ms loop, 20 ms debounce, 100 ms panel — but the press
  itself is still the human's to make.
- **The boot state after a reboot is still `PID_DISABLED` on this board**, and
  that is *not* a bug: `hardware.switches.power.type` is `Toggle`, and a toggle
  that reads off at boot starts the machine in `PID_DISABLED` in the C++ too
  (`SystemInitializer.cpp:606-641`, which the port matches line for line). The
  config's `pid.enabled` is honoured when no power switch is configured. The
  confusion is that the human expected the config to win; on a machine whose
  power toggle is off, the switch wins, and always has.
- **R1-08's C++ baseline**, deliberately absent. The harness works and 13
  scenarios report `BASELINE-MISSING` with exit 2. Capturing it means flashing
  the C++, which runs its own control loop on a powered, wired machine — the human
  declined, and nothing fabricated is better than a baseline never measured.

### Two things the harness taught us, both the hard way

**A LOST is not a pass.** `test-audit` checks that device tests are *registered*;
nothing checked that they *ran*. When the display tests' `Recorder` (2 KB, by
value) overflowed `main`'s **3584-byte** ESP-IDF task stack, the device reset and
the runner scored seven cases LOST while reporting "125 passed, 0 failed" — a
green run that had quietly stopped testing anything. Each case now runs on its
own 8 KB task. **No gate yet fails on an undeclared LOST**; that is a real gap.

**A test that has never run is not a test.** Those seven had been LOST since they
were written. Once they actually executed, two failed on assertions that had been
wrong the whole time — including a `last_payload` helper that scanned backwards
for a control byte and so found one *inside the rendered pixels*, because the test
ramp contains `0x40` and `0x00`. It now records the offset when the transfer
happens.

### The two measurements that will shape the later gates

Static RAM is **133,168 B — 42 % of the ESP32's 320 KB**, roughly double the
pre-network figure, so *RAM rather than flash is the binding constraint*, and
ADR-0002's 30 KB shed margin was tuned against a much smaller baseline. 94 KB of
it is IRAM belonging to the prebuilt Wi-Fi MAC, which is untouchable without
dropping Wi-Fi.

And the control tick already overruns its 10 ms budget in ~62 % of ticks,
independent of any scale ([09 §24](./09-cpp-findings.md)) — R4-01b's "zero ticks
over 10 ms" currently fails, and the fix must not be to relax the budget.

## How the migration runs

The C++ firmware stays in production the whole time. PlatformIO and Cargo coexist:

```
pio run -e esp32_usb     # C++ — unchanged, still the production build
just build-esp32         # Rust
just parity              # both, on hardware, diffed
```

PlatformIO is deprecated only at the very end (R4-10), and even then is kept for one
release cycle as a rollback path.

Phases: **0** preconditions → **1** feasibility spikes (Gate 1 confirms ADR-0004) →
**2** portable domain (no hardware) → **3** hardware abstraction → **4** integration,
provisioning, and parity.
