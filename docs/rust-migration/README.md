# Rust Migration

Plan for migrating the CleverCoffee ESP32 firmware from C++/Arduino to Rust.

**Status:** in progress, and the machine now works: the reducer runs on hardware,
the display lights up, the PID regulates, and all 98 parameters are writable over
HTTP and survive a reboot. **Not done:** OTA (R3-15), the Acaia BLE scale
(R3-18), the `/ui` SPA mount, and any hand-pressed switch. See
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
- **All 98 parameters are writable and persist across a reboot** —
  `POST /api/parameters`, ported from `WebServerManager.cpp:813-886`. Verified for
  bool, int, float and text. This is what closed "parameters can be configured".
- NVS, Wi-Fi STA, UART provisioning (round trip survives a reboot), MQTT, HTTP + SSE
  (25 routes), the 10 ms heater ISR, DS18B20 and TSIC-306, the HX711 scale, and
  **125 device tests that actually run on hardware** via `just test-esp32`.
- **The web UI is on the device and it renders.** `GET /ui` serves the React SPA
  from **flash**, not from a mounted filesystem: `crates/cc-hal-esp32/build.rs`
  embeds the gzip build output with `include_bytes!`, which is why a 199,270 B
  bundle costs **0 B of RAM** (static RAM is 133,168 B before and after). One
  `/ui*` wildcard route serves the shell, the assets and the client-side routes,
  with the MIME types checked (`text/html`, `application/javascript`,
  `text/css`, `image/png`) and every asset served byte-for-byte identical to
  the Vite build. Verified in Chrome on the device: navigation, Machine Status,
  Machine Functions and Maintenance all render. The size arithmetic and the
  embed-vs-mount decision are in [07 §13](./07-image-size-budget.md).

### Not done

- **R3-18**, the Acaia BLE scale (NimBLE; the flash/RAM cost is real and needs a
  gate decision, not a silent drop — see 07 §3).
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
- **Switch presses have never been tested by hand.** The debounce and long-press
  are pinned by 17 host tests against a synthetic clock, and the four switches
  are `enabled=false` by default (faithful to the C++). The human has to press one.
- **R1-08's C++ baseline**, deliberately absent. The harness works and 13
  scenarios report `BASELINE-MISSING` with exit 2. Capturing it means flashing
  the C++, which runs its own control loop on a powered, wired machine — the human
  declined, and nothing fabricated is better than a baseline never measured.

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
