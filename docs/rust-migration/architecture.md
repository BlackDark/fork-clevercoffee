# Rust firmware architecture

**Status:** Design accepted (A3)
**Last updated:** 2026-09-28
**Related:** [prior-implementation-findings.md](prior-implementation-findings.md) · [inventory.md](inventory.md) · [compatibility-matrix.md](compatibility-matrix.md) · [ADR 0004](../adr/0004-rust-migration-platform-selection.md) · [tooling.md](tooling.md) · [task-list.md](task-list.md) · [execution skill](../../.agents/skills/esp32-rust-migration/SKILL.md)

Platform is fixed by [ADR 0004](../adr/0004-rust-migration-platform-selection.md):
ESP-IDF / std on `xtensa-esp32-espidf`, ESP-IDF v5.3.6, `esp-idf-hal` 0.47 +
`esp-idf-svc` 0.53. Behaviour is to stay the same as the C++ firmware except for
the deliberate changes listed in that ADR.

This document defines the execution model, the component boundaries, the ownership
of each hardware resource, and the crate split. It exists so that concurrency and
safety decisions are made once, here, rather than per task.

---

## 1. Design constraints

These come from the inventory and are not negotiable.

| # | Constraint | Source |
|---|---|---|
| C1 | The heater must be physically incapable of being energised while an interlock is active. Today it is not — the ISR reads only `pidOutput`, and there is a window where a stale non-zero duty survives into `EMERGENCY_STOP`. | inventory §5.7 |
| C2 | Relay pins must be driven to their safe level **before anything else happens**, and before any fallible init step. Today they are set up at init step 8, and low-trigger relays are briefly energised between `pinMode()` and `off()`. | inventory §5.10 |
| C3 | The heater PWM period is **1000 ms with 10 ms resolution** (100 duty steps). Duty is expressed in milliseconds-on per window, 0..1000. Existing config and MQTT values depend on that unit. | inventory §5.5 |
| C4 | The pre-emptive safety transition order — emergency > sensor error > tank empty > PID disabled > state-specific — must be reproduced exactly, including the per-state exclusions. | inventory §5.3 |
| C5 | State ids keep their numeric values, or the range predicates (`isBrewState` 31..34, `isBackflushState` 60..63, LED eligibility ≤ 63) must be replaced by explicit sets. | inventory §5.1 |
| C6 | ~~NVS layout is frozen~~ **Struck by [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md).** NVS is ours to design: namespace `wifi` holds plain `ssid`/`pass` strings, namespace `cfg` holds `ver` (u16) and one `postcard` blob. | [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md) |
| C7 | ~~The partition table is frozen, `spiffs` label included~~ **Struck by ADR 0005.** The Rust firmware uses `partitions_rust_4m.csv` with the filesystem partition labelled `ccfs`, and **must refuse to boot without it** — see §3.1. | ADR 0005 |
| C8 | The HTTP surface in `docs/api/openapi.yaml` is the contract, with the four known mismatches fixed rather than reproduced. | inventory §6.2 |
| C9 | ~320 KB total RAM, shared with the Wi-Fi stack, the HTTP server and the display buffer. ADR 0002's memory budget still applies. | `docs/adr/0002-*` |
| C10 | All control logic must be testable on the host without hardware. It largely already is — 303 C++ test cases — and that must not regress. | inventory §2.5 |
| C11 | ADR 0003's hardware-control contract lists four regressions introduced by the last refactor. They are the known traps for this one. | `docs/adr/0003-*` |
| **C12** | **`config.json` as exported by the old web UI is the only compatibility surface.** The Rust firmware must import that nested dotted-path format, and its own export must stay the same shape. | ADR 0005 |
| **C13** | **A half-migrated device must be inert, not merely degraded.** The old firmware can OTA an arbitrary app image and ESP-IDF images are relocatable across slots, so the break has to be enforced at runtime. | ADR 0005 |

---

## 2. Execution model

### 2.1 Why not one loop

The C++ firmware is one free-running loop plus one 10 ms ISR. That works, but the
inventory shows the cost: sensor I/O, switch debouncing and **emergency detection**
all run at loop rate, so the emergency debounce counts iterations rather than
milliseconds and its time constant varies with load; the OTA path has to
`return` early out of the whole loop, skipping the state machine for the entire
session; and the network layer can block the control path.

The replacement is a small fixed set of FreeRTOS tasks, chosen so that **the safety
path does not share a deadline with anything that can block**. std threads via
`esp-idf-hal`'s `ThreadSpawnConfiguration` give name, priority, core pinning and
stack sizing, which is all that is needed.

### 2.2 Tasks

Five tasks and one ISR. Each entry states why it exists separately — if a task has
no timing reason to be its own task, it is not one.

| Task | Core | Prio | Period | Why separate | Stack |
|---|---|---|---|---|---|
| **`control`** | 0 | 18 | **10 ms, fixed** | Owns the safety path: sensor sampling cadence, interlock evaluation, PID, state machine, actuator commands. Must never wait on network, flash or I²C-display work. Highest application priority so it cannot be starved. | 8 KB |
| **`heater`** (ISR, not a task) | — | ISR | 10 ms | Hard real-time PWM edge generation. Must run even if `control` is late. | — |
| **`ui`** | 1 | 6 | 100 ms | Display rendering plus the I²C flush, which shares the bus with the pressure sensor and can take milliseconds. Lowest priority: a late frame is cosmetic. | 6 KB |
| **`net`** | 1 | 8 | event-driven | Wi-Fi supervision, MQTT publish/subscribe, SSE fan-out. Blocking and unbounded by nature. Pinned to core 1 with the Wi-Fi stack, matching the C++ build's `CONFIG_ASYNC_TCP_RUNNING_CORE=1`. | 8 KB |
| **`http`** | 1 | 8 | event-driven | Owned by ESP-IDF's `httpd`; we do not create it. Handlers must not block — same rule the C++ breaks in four places. | 8 KB (IDF config) |
| **`storage`** | 0 | 5 | event-driven | Serialises every NVS write and every flash erase. Exists specifically so that a config write never happens on an HTTP handler, which is what the C++ does today (one flash transaction per parameter, on the network task). | 4 KB |

`main` becomes bring-up only: safe the outputs, start tasks, then park.

### 2.3 How the tasks talk

One rule: **commands go down a queue, state comes back through a snapshot.** No
task reaches into another's data.

```
                    ┌──────────────────────────────────────────┐
   switches  ──────▶│  control  (core 0, prio 18, 10 ms tick)   │
   sensors   ──────▶│  sensors → interlocks → PID → FSM → act   │
                    └───┬───────────────┬──────────────────┬────┘
                        │               │                  │
            HeaterDuty  │       StateSnapshot        StorageCmd
            (atomic)    │       (seqlock, 1 writer)   (queue)
                        ▼               │                  ▼
                 ┌────────────┐         │           ┌─────────────┐
                 │ heater ISR │         │           │  storage    │
                 │  (10 ms)   │         │           │  NVS / flash│
                 └─────┬──────┘         │           └─────────────┘
                       │                │
                  heater relay          ├──────────▶ ui   (core 1, prio 6)
                                        └──────────▶ net  (core 1, prio 8)
                                                        ▲
   Command queue  ◀────────────────────────────────  http (IDF httpd)
   (bounded, 16)                                        MQTT inbound
```

Three primitives only, chosen to make the failure modes obvious:

- **`Command` queue** — bounded (16), `heapless::spsc` or an `esp-idf-svc` queue.
  Every external actor (HTTP, MQTT, and the UI's own button handling) submits
  commands; only `control` consumes them. Commands are *requests*, never direct
  actuator writes. This is how the C++ "action request flags" work already, made
  explicit.
- **`StateSnapshot`** — one `control`-written, many-reader seqlock holding
  temperature, setpoint, duty, state id, brew timer, error flags. Readers retry on a
  torn read. This replaces the C++ `SystemContext` shared-accessor pattern and
  removes the torn 8-byte `pidOutput` read the ISR performs today.
- **`HeaterDuty`** — a single `AtomicU16` (0..1000, milliseconds-on per window),
  the *only* thing the ISR reads. See §3.

### 2.4 Races, deadlocks, priority inversion

- **Deadlock** is prevented structurally: there is exactly one lock-taking
  direction, and the only mutex in the design is inside `storage`. `control` never
  blocks on a lock — it reads atomics and a seqlock and pushes to bounded queues.
- **Races** on control state are prevented by single-writer discipline: `control` is
  the only writer of `StateSnapshot` and `HeaterDuty`; `storage` is the only writer
  of NVS. Anything else that wants to change state sends a `Command`.
- **Priority inversion** is avoided by never letting `control` wait on a lower-priority
  task. `StorageCmd` submission is non-blocking; if the queue is full the command is
  rejected and reported, not awaited. `control` never reads from NVS at runtime — it
  gets a config snapshot at startup and on explicit reload.
- **ISR safety**: the ISR touches one atomic and one GPIO write. No allocation, no
  locks, no logging, no `esp_idf_svc` calls. `task::CriticalSection` is a FreeRTOS
  mutex and is **not** ISR-safe; `interrupt::IsrCriticalSection` is, and is not
  needed here because a single atomic suffices.
- **Backpressure**: every queue is bounded and every full-queue case has a named
  behaviour — `Command` full → reject with an error to the caller (HTTP 503, MQTT
  logged); `StorageCmd` full → reject and surface; log ring full → drop the message,
  as the C++ already does. Nothing blocks the control path to make room.

### 2.5 Timing guarantees and cancellation

`control` runs on a fixed 10 ms tick from `esp_timer`, and **measures its own
overrun**. Two consequences that the C++ lacks:

- The emergency debounce becomes time-based (N ticks × 10 ms) rather than
  iteration-based, so its behaviour no longer varies with system load. This is a
  deliberate change and must be calibrated to match the C++'s effective timing at
  its normal loop rate — see task list PARITY-2.
- A slow tick is detectable in release, not only under `#ifdef DEBUG`.

Long operations never run on `control`. OTA and Wi-Fi provisioning run on their own
tasks; the C++ approach of returning early out of the whole loop for an OTA session
is replaced by `control` continuing to run with the heater forced off by an
interlock. **That removes an entire class of "the state machine stopped" behaviour.**

Cancellation is cooperative and explicit: a `Command::Abort`-style request sets the
target state, and the state machine's existing stop-request mechanism handles it.
There are no detached futures to cancel because the design is threads, not async.

### 2.6 Watchdog

The ESP32 task watchdog (5 s, panic on trigger) subscribes **`control` and `ui`**,
not the idle tasks. That is a change from the C++ behaviour, which uses
`enableLoopWDT()` and feeds only from `loop()`.

- `control` feeds once per tick. A stalled control task is the condition that most
  needs a reset, and it is the one the C++ actually covers.
- `ui` feeds per frame, which catches an I²C bus hang.
- `net` is deliberately **not** subscribed — network operations are legitimately
  slow, and the C++ already has to `suspend()` the watchdog around them.
  Subscribing it would only reintroduce that dance.
- Bring-up runs **without** the watchdog armed, and arms it only once `control` is
  running and feeding. The C++ arms it across the whole of `initialize()` and never
  feeds it, surviving on scattered `yield()` calls. Recorded as a deliberate change.

---

## 3. The heater: making C1 structural

This is the single most important design decision, so it is stated on its own.

Today: the ISR reads `pidOutput` and nothing else. Every interlock in the system —
emergency stop, sensor error, standby, empty tank, backflush, brew PID delay —
works by racing to set that value to zero, and `computePID()` writes a fresh value
*before* `updatePIDState()` zeroes it.

The Rust design inverts the dependency:

```rust
/// Duty is milliseconds-on per 1000 ms window, 0..=1000.
/// The ISR reads ONLY this. Nothing else can energise the heater.
static HEATER_DUTY: AtomicU16 = AtomicU16::new(0);

/// Interlocks are evaluated before a duty can exist at all.
/// `HeaterCommand::duty()` is the only constructor, and it is fallible.
pub struct HeaterCommand(u16);

impl HeaterCommand {
    pub fn duty(requested: u16, interlocks: &Interlocks) -> Self {
        if interlocks.any_blocking() {
            Self(0)
        } else {
            Self(requested.min(WINDOW_MS))
        }
    }
}
```

Three properties follow, and each maps to a host test:

1. **A duty value cannot be produced without interlocks having been consulted**,
   because the only constructor takes them. The C++ ordering hazard disappears —
   not because the order was fixed, but because the unsafe order is unrepresentable.
2. **The ISR cannot read a torn value.** `AtomicU16` replaces a non-atomic `double`.
3. **Resolution and window are explicit constants**, so the 0..1000 unit that config,
   MQTT and the web UI all depend on stays one definition.

The ISR itself stays as small as today's:

```rust
// 10 ms tick. No allocation, no locks, no logging, no esp_idf_svc calls.
// The comparison MUST stay integer -- see the constraint below.
fn on_tick(counter: &mut u16, relay: &HeaterPin) {
    let duty = HEATER_DUTY.load(Ordering::Relaxed);
    if duty <= *counter { relay.off() } else { relay.on() }
    *counter = (*counter + TICK_MS) % WINDOW_MS;
}
```

**The integer comparison is a hard chip constraint, not a style preference.** A single
floating-point instruction anywhere in a level-1 ISR panics the original ESP32:
`Coprocessor exception`, `EXCCAUSE 0x4`. Xtensa never saves coprocessor state across an
interrupt, so the FP save area is null and the handler panics.
`CONFIG_FREERTOS_FPU_IN_ISR` is the opt-in and defaults to off. There is no
compile-time warning, and **the C++ escapes it only by accident** -- GCC lowers its
`double` compare to soft-float calls while the `esp` Rust target advertises
`target_feature="fp"`, so LLVM emits hardware FP from identical source. This was hit
and diagnosed on hardware by the parallel implementation; see
[prior-implementation-findings.md §1.1](prior-implementation-findings.md). Keep
`AtomicU16` and integer compares, and verify by reading the disassembly rather than the
source.

For the same reason `HEATER_DUTY` is milliseconds-on as a `u16` rather than a fraction:
there must be no float anywhere on the path the ISR reads.

`HEATER_DUTY` is written only by `control`, after interlocks. If `control` dies, the
watchdog resets the chip; until it does, the last duty persists for at most one
window — the same exposure as today, and the reason the watchdog subscribes
`control` specifically.

### 3.1 The layout guard — making C13 structural

The other way the heater can be energised wrongly is not a race but a
misconfiguration: the Rust firmware running against the C++ partition table, after
someone OTA'd the app from an old device. ESP-IDF images are relocatable across OTA
slots, so that boots. There is no `ccfs` partition, so there are no web assets and
no initialised configuration — and a heater, pump and valve are still wired up.

The guard is a startup precondition, not a feature:

```rust
/// Runs immediately after the actuators are driven safe, and before anything else.
/// Pure decision, so it is host-testable; the caller supplies the partition list.
pub fn layout_ok(parts: &[PartitionInfo]) -> Result<(), LayoutFault> {
    parts.iter()
        .find(|p| p.label == "ccfs" && p.subtype == SubType::LittleFs)
        .map(|_| ())
        .ok_or(LayoutFault::MissingCcfs)
}
```

On `Err`, the firmware logs one message naming the cause and the remedy, and
**halts**: no control task, no heater ISR, no Wi-Fi. Actuators are already safe
because this runs after startup step 1.

Three properties, each mapping to a test:

1. The check runs **after** the actuators are safe and **before** anything that
   could energise them, so a failed check cannot leave a hot machine.
2. It is a pure function over a partition list, so every case is host-testable
   without hardware.
3. The C++ firmware cannot produce a `ccfs` partition, and its filesystem-OTA path
   looks up the literal label `spiffs`, so it cannot create or write one either.

This is a safety interlock, not an anti-tamper measure. Someone with a USB cable can
flash whatever they like; the point is that the *accidental* half-migrated state is
inert rather than dangerous.

---

## 4. Startup and shutdown

### 4.1 Safe startup

Ordered to fix C2. The first thing that happens is that the actuators are made safe,
before any fallible step, any logging and any config load:

1. **Safe the outputs.** Configure `PIN_HEATER`, `PIN_PUMP`, `PIN_VALVE` and drive
   each to its configured inactive level. For a `LOW_TRIGGER` relay that means
   driving HIGH. This must be the first statement in `main`, with nothing fallible
   before it.
   - **Open hardware question:** ESP32 pins idle as inputs after reset, so the relay
     board decides the level between power-on and this line. That is a board-level
     property (pull resistors on the relay inputs) that the firmware cannot fix and
     that this project has not measured. Recorded in the task list as a hardware
     observation, not a code task.
   - The relay trigger polarity lives in config, which is not loaded yet. Resolution:
     read the three trigger-type keys from NVS **directly** in this first step, with
     a fail-safe default of `LOW_TRIGGER` (drive HIGH), before the full config
     registry initialises.
2. **Check the flash layout** (§3.1). If `ccfs` is missing, log once and **halt** —
   actuators are already safe from step 1.
3. Serial and logging.
4. Config load: read `cfg/ver` and `cfg/blob` from NVS. On a missing or unreadable
   blob, fall back to compiled-in defaults and log at ERROR. On first boot, also
   look for `/config.json` on `ccfs` and import it (the same path the user's manual
   migration uses).
5. Peripherals: I²C bus, temperature sensor, pressure sensor, water-tank input,
   switches, LEDs, display.
6. Start `control` — **in a safe state with the heater interlocked off**, and start
   the heater ISR only once `control` is ticking.
7. Arm the watchdog.
8. Start `storage`, `ui`.
9. Start `net` (Wi-Fi, then HTTP, then MQTT). **Non-fatal**, matching the C++
   offline-mode behaviour: a machine with no network still makes coffee. Wi-Fi
   credentials come from `wifi/ssid` and `wifi/pass`.
10. Release the startup interlock, letting `control` follow the state machine into
    `PID_NORMAL` or `PID_DISABLED` according to the power-switch type.

Failure policy, as a table, because the C++ has five inconsistent styles
(inventory §5.13):

| Failure | Behaviour |
|---|---|
| Output safing | Impossible to fail; it is direct register work with no allocation |
| **Flash layout check** | **Halt with actuators safe.** Not a fallback — a half-migrated device must be inert (C13) |
| Config load | Fall back to compiled-in defaults, log at ERROR, continue |
| Temperature sensor | Continue; `control` starts in `SENSOR_ERROR` with the heater off. **Change from C++**, which boots normally with no sensor. |
| Display | Continue without a display, log at WARN — same as C++ |
| Pressure sensor / scale | Continue, feature disabled |
| Wi-Fi / HTTP / MQTT | Continue in offline mode — same as C++ |
| `control` cannot start | The only *fatal-with-restart* case: log at FATAL, then `esp_restart()`. No `exit(0)`, no falling through with a null manager. |

Note the distinction between the two failure styles. The layout check **halts** —
retrying cannot help, and rebooting into the same wrong layout would be a reset
loop. `control` failing to start **restarts**, because that is plausibly transient.

That last row deliberately removes the C++ path where `isInitialized()` is false,
three FATALs are logged, and the firmware continues into `loop()` with
`loopManager == nullptr` and all actuators unmanaged.

### 4.2 Fail-safe actuator states

| Actuator | Safe state | Enforced by |
|---|---|---|
| Heater | off | `HEATER_DUTY = 0`; the ISR can do nothing else |
| Pump | off | `control` is the only writer; interlocks gate it |
| Valve | closed | `control`; plus the periodic valve-safety check, ported as-is |
| LEDs | off | cosmetic |

On fault, `control` writes all four to safe **unconditionally** — it does not
consult a cached "is it on?" flag first. That fixes the C++
`disableAllHardware()` behaviour, which skips any relay whose tracked flag says
it is already off, while the heater's flag is documented as unreliable.

### 4.3 Shutdown and reset

There is no clean shutdown path on this hardware — a reset does not run
destructors. So the design does not pretend to have one:

- Deliberate restart (OTA, factory reset, `/api/restart`): `control` drives all
  actuators safe, confirms, then `esp_restart()`.
- Panic: a panic hook drives the relay pins safe before aborting. **This is the one
  place where a global raw-GPIO handle is justified**, and it is written down here
  so it is not mistaken for sloppiness elsewhere.
- Brownout or external reset: nothing runs. Post-reset behaviour is a board
  property, same open question as §4.1 step 1.

---

## 5. Components and hardware ownership

Exactly one owner per resource. Everything else asks.

| Resource | Owner | Reached by others via |
|---|---|---|
| Heater relay GPIO | heater ISR | `HEATER_DUTY` |
| Pump, valve relay GPIO | `control` | `Command` |
| LED GPIO | `control` | `StateSnapshot` drives them |
| Switch GPIO | `control` | — |
| Temperature sensor + RMT channel | `control` | `StateSnapshot` |
| Water-tank GPIO | `control` | `StateSnapshot` |
| I²C bus | **shared**, `Mutex<I2cDriver>` | display (`ui`) and pressure (`control`) |
| Display | `ui` | `StateSnapshot` |
| NVS (`wifi` + `cfg`) | `storage` | `StorageCmd` |
| LittleFS `ccfs` (web UI, `/config.json`) | `http` | read-only |
| Wi-Fi, MQTT, SSE | `net` | `Command`, `StateSnapshot` |
| OTA partitions | OTA task | — |
| `esp_timer` (10 ms control tick) | `control` | — |
| GPTimer (heater PWM) | heater ISR | — |

The I²C mutex is the only shared-resource lock in the design, and it is the one
place a priority inversion is possible: `ui` (prio 6) holds it for a display flush
while `control` (prio 18) wants it for a 10 ms pressure read. Three mitigations,
in order of preference:

1. `control` uses `try_lock` for pressure and **skips the sample** if the bus is
   busy. Pressure is a 20 Hz non-safety signal; a dropped sample costs nothing.
   This makes the inversion structurally impossible.
2. FreeRTOS mutexes support priority inheritance, which bounds it anyway.
3. If measurement later shows a problem, move pressure reads onto `ui` and publish
   through the snapshot.

Option 1 is the design. Recorded because "share the I²C bus" is the kind of decision
that looks harmless and is not — and note the C++ has the same contention today,
unmanaged, with a **blocking 10 ms** pressure read at 20 Hz against 10 Hz display
flushes on a 100 kHz bus.

---

## 6. Crate layout

Split at real boundaries only: what must be host-testable, what is
platform-specific, what is board-specific. Five crates, not more.

```
Cargo.toml              host workspace
crates/
  cc-domain/     no_std  — pure logic. No HAL, no platform. Host-tested.
  cc-hal/        no_std  — traits the domain needs. No implementations.
  cc-drivers/    no_std  — device drivers over embedded-hal 1.0 + our HAL traits.
firmware/
  Cargo.toml            target workspace
  .cargo/config.toml    triple, ldproxy, build-std, MCU, ESP_IDF_VERSION
  esp32/         std     — the binary. Target + feature selection only.
  cc-board/      std     — esp-idf implementations of cc-hal, pin map, config store.
  cc-app/        std     — task wiring, HTTP, MQTT, OTA, provisioning.
```

Two workspaces, not one. The root workspace holds only the three host crates and
`exclude`s `firmware`, so `cargo test --workspace` never pulls in anything needing
the ESP toolchain. `firmware/` depends on `../crates/*` by path — dependencies, not
members, which is what lets them live outside its directory. The target-only crates
sit *under* `firmware/` because Cargo requires workspace members to be
hierarchically below the workspace root. See [tooling.md](tooling.md) §3.

Dependency direction is strictly downward. `cc-domain` depends on nothing in this
workspace; `cc-hal` depends on nothing; `cc-drivers` depends on `cc-hal`;
`cc-board` depends on `cc-hal` + `cc-drivers`; `cc-app` depends on all of them;
`esp32` depends on `cc-app` + `cc-board`. **Nothing depends upward, and
`cc-domain` never learns what platform it is on.**

### 6.1 `cc-domain` — the parity surface

Everything that the 303 existing C++ tests cover, and nothing else:

- `state`: the 18 state ids with their numeric values, the transition table, and
  `BaseState`'s pre-emptive safety chain (C4, C5).
- `control`: the `PID_v1` port, setpoint selection, tuning-set selection per state,
  brew PID delay.
- `interlocks`: `Interlocks::any_blocking()` and the `shouldPIDBeEnabled` rule set.
- `emergency`: the debounce and hysteresis state machine.
- `brew`, `backflush`, `steam`, `standby`, `maintenance`: the existing handler logic.
- `config`: the parameter schema, ranges, validation, `postcard` (de)serialisation
  of the versioned blob, and **`config.json` import/export in the old UI's nested
  dotted-path shape** (C12). All pure functions, so all host-testable.
- `layout`: the flash-layout precondition (C13, §3.1) as a pure function over a
  partition list.
- `display`: layout maths, template selection, the brew-timer state machine, and the
  U8g2 anchor shim. Renders into an `embedded-graphics` `DrawTarget`, so it is
  host-testable into a simulator buffer.
- `tsic`: the ZACwire decoder as a pure function over a symbol slice.

`cc-domain` is `no_std` + `alloc`, has **no** dependency on `esp-*` anything, and
`cargo test -p cc-domain` runs on the host with no toolchain gymnastics. This is
what makes C10 hold, and it is the reason the crate boundary exists at all.

### 6.2 `cc-hal` — the platform seam

Small traits, only what the domain and drivers actually need. This is also the seam
ADR 0004 relies on if the bare-metal platform is ever revisited:

```rust
pub trait DigitalOut  { fn set(&mut self, active: bool); }
pub trait DigitalIn   { fn is_active(&self) -> bool; }
pub trait Clock       { fn now_ms(&self) -> u64; }
pub trait HeaterPwm   { fn set_duty(&self, ms_on_per_window: u16); }
pub trait TempSource  { fn read(&mut self) -> Result<f32, SensorError>; }
pub trait KvStore     { /* get/set by key, typed, mirrors Preferences encoding */ }
pub trait SymbolCapture { /* RMT RX for ZACwire */ }
```

No trait here mentions `esp_idf`. `Clock` exists so state timeouts are testable
with a fake clock, exactly as `test/Arduino.h`'s `g_test_millis` does today — and it
resolves the C++'s two-clocks problem (`steady_clock` vs `millis()`) by having one.

### 6.3 `cc-drivers`

`ds18b20` (via `onewire` 0.4.0), `abp2` (ours, ~150 LOC), `oled` (`ssd1306` /
`oled_async` behind one `DrawTarget`), and the TSIC platform glue. Generic over
`embedded-hal` 1.0 and `cc-hal`, so each is host-testable against a mock bus.

### 6.4 `cc-board`

The only crate that knows about ESP-IDF. Pin map as constants (C1 from the
inventory: there are no board variants today), `cc-hal` implementations over
`esp-idf-hal`, the `KvStore` implementation over `EspNvs` (namespace `wifi` for the
two credential strings, namespace `cfg` for `ver` + the blob), and the GPTimer
heater PWM.

The board axis exists here as a feature, not a directory tree. Adding a variant
means adding a pin-map module and a feature; the shared logic does not move.
**A target or board counts as supported only once it has been built and validated
on that hardware** — see the task list's exit gates.

### 6.5 `cc-app`

Task wiring, the `Command`/`StateSnapshot` plumbing, the HTTP routes reproducing
`openapi.yaml` (C8), MQTT topics and Home Assistant discovery, OTA, provisioning,
and the logger. Depends on `std`; not host-testable, which is why it holds no
control logic.

### 6.6 What compiles and tests where

| Crate | `cargo check` on host | `cargo test` on host | Needs esp toolchain |
|---|---|---|---|
| `cc-domain` | yes | **yes — the main test surface** | no |
| `cc-hal` | yes | yes (trait-level only) | no |
| `cc-drivers` | yes | **yes — against mock buses** | no |
| `cc-board` | no | no | yes |
| `cc-app` | no | no | yes |
| `esp32` | no | no | yes |

The workspace is therefore split into two `cargo` invocations: a host workspace for
the first three crates and a target build for the rest. This is deliberate and it is
what makes `just test` fast — see [tooling.md](tooling.md).

---

## 7. Test strategy

| Layer | How | Runs on |
|---|---|---|
| State machine and transitions | table-driven tests against `docs/state-machine-architecture.md` | host |
| Safety chain (C4) | one test per state × per pre-emptive condition, asserting the target state and that the heater duty is 0 | host |
| Interlocks (C1) | `HeaterCommand::duty` is 0 for every blocking interlock; property test over the interlock powerset | host |
| PID parity | golden vectors captured from the C++ `PID_v1` under test, replayed against the Rust port | host |
| Config round-trip (C12) | a real `config.json` exported from the C++ UI imports, re-exports identically, and blob encode/decode round-trips | host |
| Layout guard (C13) | every partition-list case, including the C++ table, asserts halt | host |
| ZACwire decode | recorded symbol slices → expected °C, including parity failures and the sentinels 221/222 | host |
| Display | render into a buffer, compare against a golden image | host |
| Drivers | mock `embedded-hal` bus, including NAK and short-read paths | host |
| NVS round-trip | write and read back the config blob and credentials on the device | **device** |
| Timing | measure control-tick jitter and PWM edge accuracy on the device | **device** |
| Wi-Fi / MQTT / HTTP / OTA | on the device | **device** |

The C++ firmware stays buildable throughout and is the oracle: parity tests capture
its behaviour rather than restating the documentation.

---

## 8. Open design questions

Carried into the task list rather than resolved here, because each needs either
hardware or a user decision:

1. **Relay idle level between power-on and the first instruction** (§4.1). Board
   property, unmeasured.
2. **Water-tank switch polarity** — the C++ has it inverted relative to the other
   four switches and one of the two is wrong (inventory §7.1 item 11). Needs a user
   or hardware answer before the port can claim parity.
3. **Whether to add the missing manual-brew, steam and manual-flush timeouts.**
   Their absence is a genuine safety gap, but adding them is a behaviour change a
   user must approve. Default for this migration: **reproduce the current behaviour**
   and flag it.
4. **Whether `hasSensorError()` should keep conflating scale and temperature
   errors**, which currently lets a scale fault kill the heater — on a scale that is
   never instantiated.
5. **Whether provisioning uses a captive portal, a serial protocol, or the raw
   ESP-IDF `wifi_provisioning` component.** Decided by SPIKE-7; see
   [tooling.md](tooling.md).
