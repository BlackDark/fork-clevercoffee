# ADR-0004: Rust Migration — Platform and Concurrency Architecture

> **ARCHIVED — non-normative. Dated 2026-09/10, preserved for provenance.**
> The decision it records (`esp-idf` vs bare metal) was taken and shipped, and
> every consequence that still binds is now a numbered rule in `AGENTS.md`
> (`AG-RUST-*`) — this document is **not** the source of any of them. The part of
> ADR-0004 that is still an active contract is carried forward in
> [`docs/adr/0003`](../../adr/0003-state-machine-hardware-control-contract.md).
> Read it to understand *why* the port looks as it does; do not read it for what
> is true today. See [`docs/archive/README.md`](../README.md).

## Status

**Proposed** — conditional on spikes R1-01 … R1-07. Supersedes nothing; the C++ firmware
in `src/` remains the production system until a phase gate is passed.

## Context

CleverCoffee is an ESP32 (original, Xtensa LX6) coffee-machine controller written in
C++23 on the Arduino framework, ~28 kLoC across `src/` (11,147) and `include/clevercoffee/` (17,052), with
33 native test suites running 340 test cases. It drives safety-critical hardware (a 2 kW boiler through a relay,
a pump, and a three-way valve) from a single cooperative `loop()`.

The firmware has grown to depend on 12 C++ libraries (`platformio.ini:40-52`) plus 5 Arduino built-ins, several of
which are the only implementation of their protocol:

- **ZACwire** — proprietary TSIC-306 optical temperature protocol. No Rust crate exists.
- **U8g2** — 10 proprietary bitmap font families and 6 fully custom display templates.
- **HX711_ADC** and **AcaiaArduinoBLE** — both dead code, and the original ESP32 has no
  Bluetooth radio (it does — see the correction in 01 §3).
- **tzapu/WiFiManager** — a captive Wi-Fi portal.

Three properties make this migration unusually constrained, and none of them are about
Rust:

1. **Flash.** `firmware.bin` builds to **1,546,240 bytes** against a **1,703,936-byte**
   `app0` slot (`partitions_4M.csv`) — roughly **154 KB of headroom**. A Rust esp-idf
   image with `std` is typically 1.5–2.5 MB. A partition rebalance is probably mandatory.
2. **RAM.** ~320 KB, with a hard-won 30 KB heap-shed threshold documented in ADR-0002.
3. **Safety.** Eleven distinct control paths must prevent unexpected heating, pumping, or
   actuation ([01 §6](../../history/feature-inventory.md)). A
   regression here can boil water or run a dry pump.

A fourth, easily-missed property: **"ESP32 v4" means the AZ-Delivery DevKitC **PCB
revision 4**, not a chip variant.** There is no S3, C3, or C6 anywhere in the repository
or its 1875-commit history. The original ESP32 has **no USB peripheral** — the USB cable
is a CP210x-class bridge wired to UART0, and the `esp32_usb` PlatformIO environment name
is a misnomer. This has a direct consequence for the "provision Wi-Fi over USB" goal,
discussed in §5.

Evidence for all of the above is in
[01 — Feature inventory](../../history/feature-inventory.md); the ecosystem survey with links,
versions, and dates is in
[02 — Research matrix](../../history/dependency-evaluation.md).

---

## Options considered

### Option A — `esp-idf-svc` (ESP-IDF bindings, `std` available) ✅ **SELECTED**

Rust wrappers over Espressif's own ESP-IDF, running on FreeRTOS with `std` linked.
`esp-idf-svc` 0.53.0 / `esp-idf-hal` 0.47.0 / `esp-idf-sys` 0.38.1, defaulting to
**ESP-IDF v5.5.5**.

| Requirement | Coverage |
| --- | --- |
| Wi-Fi STA | ✅ `svc::wifi` |
| Web server + static SPA | ✅ `svc::http::server::EspHttpServer` |
| MQTT v3.1.1 + HA discovery | ✅ `svc::mqtt::EspMqttClient` |
| NVS config (96 registered params) | ✅ `svc::nvs::EspDefaultNvsPartition` |
| LittleFS web assets | ✅ `svc::fs::littlefs` (needs the `joltwallet/littlefs` component) |
| OTA | ✅ `svc::ota` |
| GPIO, I2C, ADC, LEDC, GPTimer, UART | ✅ `hal::*` |
| Task watchdog | ✅ `hal::task::watchdog::TWDTDriver` |
| Heap-constrained logging | ✅ `log` + `alloc`; ADR-0002's 30 KB shed is app-level |
| Async | ✅ native async wrappers + FreeRTOS; `std` also available |

**Pros**
- Covers every connectivity and storage requirement with a maintained (if community-run)
  crate. No stack has to be written.
- Uses the vendor's own Wi-Fi, TCP/IP, and flash drivers — the parts Espressif actually
  funds, and the parts with a decade of field hardening.
- FreeRTOS underneath means a real priority-ordered scheduler, which is what a
  safety-relevant control loop wants. The current C++ design's single priority-1
  `loopTask` versus priority-10 AsyncTCP inversion (ADR-0002) becomes an explicit,
  testable configuration instead of a build-flag side effect.
- NVS, the partition scheme, and `esp_app_format` all behave the way the C++ firmware
  already assumes. **NVS *contents* do not survive**: decided 2026-09-28, no backward
  compatibility — Rust owns its own `cc.`-prefixed keys and a C++-written NVS is
  overwritten with defaults. The `nvs` partition is still placed identically so the
  layout is familiar, not so old data is readable.
- `IDF_PATH`/self-provisioning means CI does not need ESP-IDF pre-installed.
- A `spiffs`-partition OTA and a `coredump` partition both map onto existing ESP-IDF
  concepts.

**Cons**
- The `esp-idf-*` crates are a **community effort** — the upstream README says explicitly
  that they lag ESP-IDF, have **no HIL tests**, and are thinly documented. Espressif's
  funded work is in `esp-hal`, not here.
- First `esp-idf-sys` build compiles all of ESP-IDF twice (std + no_std). Roughly 15–25
  minutes and multiple GB. This dominates CI cost.
- Requires the esp-rs **Xtensa Rust fork** via `espup`, which mise cannot manage — it
  writes to `~/.rustup/toolchains/esp`, outside mise's control. The most fragile part of
  the toolchain, and one mise cannot make hermetic.
- Ships C++ (FreeRTOS, lwIP, mbedTLS) into the image, so the "no C++" benefit is only
  about *our* code, not the binary.
- Binary size risk is real — see the flash constraint in Context.

**Risks**
- A community crate could lag or regress. Mitigation: pin exact versions, keep all
  hardware access behind our own traits, and re-verify on every minor bump.
- `EspHttpServer` SSE is a documented upstream problem (chunked encoding may break SSE;
  the "preamble only" API was requested and never landed). Spike R1-05.
- No HIL test coverage means library regressions will be discovered on our hardware.

### Option B — `esp-hal` + `esp-rtos` + `esp-radio` + embassy (bare metal) ❌ **DISQUALIFIED for this firmware**

`esp-hal` 1.2.0 (Espressif-funded, `no_std`), `esp-rtos` 0.4.0,
`esp-radio`, `embassy` 0.10, `embassy-net`. Chip support for ESP32, S3, and C6 is
confirmed.

**Pros**
- Officially funded by Espressif, with a real HIL test pipeline.
- `hal::mcpwm` hardware PWM for the heater — better than ESP-IDF's LEDC and better than
  today's 10 ms bit-banged ISR.
- Genuinely `no_std`; a much smaller, more predictable binary.
- `esp_rtos::InterruptExecutor` gives up to 4 ISR-mode tasks with low latency.
- RISC-V chips (C6/H2/C3) need **no** compiler fork at all.

**Cons / disqualifying gaps**
- **No HTTP server.** No maintained bare-metal HTTP server was found.
- **No OTA.** Absent.
- **No NVS.** Absent.
- **No filesystem.** Absent — so no LittleFS web assets and no `/config.json` seed.
- **No verified TCP/IP stack for `esp-radio`.** `esp-radio` is verified to provide the
  *radio*; whether it provides a TCP/IP stack or `embassy-net` glue is **UNVERIFIED**.
  Without it, MQTT and the web UI are unreachable.
- **No task watchdog.** `esp_hal::init()` *disables* the SWD/RWDT/TIMG watchdogs on boot
  and exposes no user-facing task WDT, so safety path S7 has no direct equivalent.
  Whether one can be reconstructed is **UNVERIFIED**.

Building an HTTP server, an OTA bootloader handshake, a filesystem, a TCP/IP stack, and a
watchdog from scratch is a multi-month project that dwarfs the coffee-machine firmware it
would serve. **Disqualifying.**

The Xtensa compiler fork is required for ESP32 in *both* options, so Option B offers no
tooling advantage on the board actually in use.

### Option C — Hybrid (`esp-hal` peripherals + `esp-idf-svc` connectivity) ❌ **REJECTED**

Attractive in theory: drive the bit-bang sensors from `esp-hal` for tighter ISR timing
while using `esp-idf-svc` for Wi-Fi. In practice it means two peripheral abstractions over
the same registers, two allocation strategies, two task models, and a linker
configuration with both ESP-IDF and bare-metal startup paths — to save a few microseconds
on a 10 ms tick that does not need them. Rejected on complexity, not on capability.

### Option D — Keep Arduino, add Rust via FFI ❌ **REJECTED**

Defeats the purpose and keeps the PlatformIO toolchain, the C++ libraries, and the
original compile-time risk.

---

## Decision

**Adopt Option A: `esp-idf-svc` 0.53.0 on ESP-IDF v5.5.5, target `xtensa-esp32-espidf`,
with `std` enabled and `esp-idf-hal` 0.47.0 for peripherals.**

Pinned versions: `esp-idf-svc = "=0.53.0"`, `esp-idf-hal = "=0.47.0"`,
`ESP_IDF_VERSION = "v5.5.5"`. Exact pins, no `^`, because these crates are
community-run and a minor bump is a real change.

This is the only option that can deliver the complete feature set within a sensible
budget, and it is the only one with a viable path to the flash-size constraint.

### Concurrency decision: FreeRTOS tasks + a single async control task

**Reject `embassy` as the primary execution model for now.** Reasons:

1. **No authoritative recommendation exists.** The `edge-executor` ISR-safety guidance
   survives only in the 0.43.0 CHANGELOG section; `edge-executor` 0.5.0 (2026-08-20) is
   newer than the "avoid `edge-executor`" advice in issue #630 (2026-01-21) and nobody
   has assessed it. Choosing a contested executor is not a decision we can make from
   documentation.
2. **`embassy-executor` is not ISR-safe on esp-idf-svc** — it synchronizes through the
   `critical-section` crate, and esp-idf-hal's implementation is a FreeRTOS recursive
   mutex, not `disable-all-interrupts`.
3. **FreeRTOS gives us what the machine actually needs**: explicit priorities, a
   priority-ordered scheduler, and independent watchdog subscription per task. The
   C++ firmware's biggest scheduling defect — a priority-10 network task preempting a
   priority-1 control loop — is fixable precisely because these priorities are ours to
   set.

**The architecture is deliberately executor-agnostic.** Every task boundary in
[04 — Target architecture](../../history/target-architecture.md) §3 is a plain "spawn this loop"
seam, so switching to `embassy` or `edge-executor` later is a change to the `spawn_*`
functions and nothing else. Spike R1-02 records the evidence so the choice can be revisited
with data.

**Concurrency is not introduced for its own sake.** Three activities genuinely need
independent execution contexts, and the reasoning is in
[04 — Target architecture](../../history/target-architecture.md) §2:

1. The **control loop** must keep its deadline even if the network stack stalls. This
   already happens today via `AsyncTCP`'s priority, but by accident.
2. The **heater PWM** must not be preemptible by anything, including a flash erase. LEDC
   hardware PWM (spike R1-07) removes the ISR entirely; the GPTimer ISR is the fallback.
3. The **HTTP handler** work (a full `AsyncJsonResponse` render, a LittleFS file read) is
   unbounded and must not sit in the control loop.

Everything else — sensors, state machine, display rendering, MQTT publishing — stays in
one control task, cooperatively, exactly as today. No channels are introduced for them.

### Flash-size mitigation (a decision, not a hope)

154 KB of headroom will not hold a Rust esp-idf image. The plan is:

1. Re-measure the actual LittleFS image in spike R1-01 to find the real spiffs usage.
2. Rebalance `partitions_4M.csv` to shrink `spiffs` and grow `app0`/`app1`, keeping the
   `nvs`, `otadata`, and `coredump` partitions identical so the NVS layout is unchanged and panics are
   still captured.
3. Embed the React bundle in the binary via `include_bytes!` (the Rust build) rather than
   requiring a separate filesystem upload, if measurement shows the filesystem is
   mostly empty.
4. **No NVS backward compatibility** (decided 2026-09-28): Rust owns its `cc.`-prefixed
   key namespace and a C++-written NVS is overwritten with defaults.

If the image still does not fit, the fallback is to accept a reduced app slot and drop
features deliberately — **never** by relaxing the safety paths.

### Decision is conditional

This ADR is **Proposed**, not Accepted. It becomes Accepted when R1-01 (build), R1-02
(executor), R1-03 (TSIC-306 decode), and R1-07 (heater PWM) all pass, because those four
determine whether Option A is merely the least-bad choice or a genuinely workable one. If
R1-03 fails and TSIC-306 cannot be decoded reliably in Rust, the decision must be
revisited: the machine would then run on the C++ firmware with Rust on a second core, or
the sensor would be replaced with hardware.

---

## Consequences

### Positive
- All connectivity, storage, and OTA requirements are met by maintained crates.
- Explicit task priorities make the ADR-0002 priority inversion a designed property.
- Hardware access sits behind our own traits, so a future Option B revisit is a
  reimplementation of one crate, not a rewrite.
- A Rust port of the safety paths gets the type system: a single `ActuatorState` that
  cannot represent "pump believed off while the relay is on".

### Negative
- We take on a community-maintained crate set with no HIL tests. Pinned versions and a
  thin hardware layer are the mitigation, not a cure.
- The Xtensa `esp` toolchain is the most fragile dependency and mise cannot contain it.
- CI cost rises sharply (ESP-IDF compiled twice, ~15-25 min cold).
- The image is larger, forcing a partition rebalance that touches the OTA path.

### Neutral
- Dropping scale support (F13/F14) removes work but is a **behaviour change** — brew by
  weight stops working. It is already dead code, so this is a documentation fix rather
  than a regression, but it must be stated in the release notes.
- The `safety.emergency_temp` / `safety.emergency_hysteresis` config bug found during
  the audit (defined and read but never registered in `getAllConfigParams()`) must be
  **fixed**, not replicated.

### Follow-up
- Re-evaluate `esp-hal` if Espressif ships an HTTP server, filesystem, and TCP/IP glue.
- Track `edge-executor` 0.5.x; revisit R1-02 when it has field history.
- An ESP32-S3 hardware revision would unlock native USB and Espressif's USB-serial Wi-Fi
  provisioning transport — a separate, hardware-level decision (see §5).
