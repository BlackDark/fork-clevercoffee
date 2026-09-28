# Rust Migration

Plan for migrating the CleverCoffee ESP32 firmware from C++/Arduino to Rust.

**Status:** plan complete, migration not started. ADR-0004 is **Proposed**, pending the
Phase 1 spikes.
**Started:** 2026-09-28.
**C++ baseline verified green:** `pio run -e esp32_usb` succeeds (`firmware.bin`
1,546,240 B); `pio test -e native_test` → 340/340 pass in 55 s.

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
| — | [**Intentional divergences**](./intentional-diffs.md) | Where the Rust firmware **deliberately differs** from the C++ it replaces, why, and the test that pins each one. A parity diff here is expected, not a regression. Start here when a diff appears. |

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
