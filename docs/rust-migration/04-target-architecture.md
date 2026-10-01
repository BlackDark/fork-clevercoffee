# Target Architecture — Rust Port

Companion to [03 — Decision record](./03-decision-record.md). Read the inventory
([01](./01-feature-inventory.md)) first; every boundary below traces back to a row there.

**Platform:** `esp-idf-svc` 0.53.0 / `esp-idf-hal` 0.47.0, ESP-IDF v5.5.5, target
`xtensa-esp32-espidf`, `std` enabled.
**Concurrency:** FreeRTOS tasks, explicit priorities, one async-capable control task.

---

## 1. Principles

1. **Safety paths are a separate crate with no I/O.** `cc-safety` is `no_std`
   and depends on nothing. It cannot accidentally acquire a peripheral. It is a pure
   reducer: `(telemetry, config, state) -> SafetyVerdict + Vec<Effect>`.
2. **The control loop is functional core, imperative shell** (§3.1). Pure reducers decide;
   exactly one applier acts. Nothing else may touch hardware, and that is a type
   constraint rather than a convention.
3. **A relay is only ever driven by one owner, through a state that cannot lie.** The
   C++ code's `heaterEnabled_` is a `std::atomic<bool>` that the ISR deliberately does not
   update ([01 §4](./01-feature-inventory.md#4-execution-model-today)). In Rust the
   heater output has exactly one owner, so the flag cannot drift.
4. **Board configuration is data, not `#[cfg]` spaghetti.** One `Board` trait, one impl per
   board, one `PinMap` const. A pin that does not exist on the chip is a compile error.
5. **Host-testable by construction.** Domain logic depends on traits, never on
   `esp-idf-svc`. `cargo test` on the host must cover the state machine, the emergency
   logic, the config layer, and the layout engine.
6. **No concurrency for its own sake.** Three task boundaries, each justified below.
   Everything else stays cooperative in one task.
7. **No blanket lint suppression.** `#![deny(warnings)]` and
   `-D clippy::all -D clippy::pedantic` with **named, justified** `#[allow]`s, each with a
   comment saying why. A blanket `#![allow(clippy::pedantic)]` is a CI failure.

---

## 2. Why each activity needs the execution model it gets

The C++ firmware runs everything in one `loopTask` at priority 1, while `AsyncTCP` runs
at priority 10 (`platformio.ini:19-22`). Network work preempts control work. That has
already caused production incidents (ADR-0002). The Rust design makes the split explicit
and, more importantly, makes the *priority relationship* a testable property.

| Activity | Execution model | Why it cannot be anything else |
| --- | --- | --- |
| **Heater output** | **LEDC hardware PWM** (preferred) or a **dedicated GPTimer ISR** (fallback) | The only safety-critical timing in the machine. Must be independent of the scheduler, of heap allocation, and of flash erases. Hardware PWM has no CPU involvement at all. A fallback ISR must be the highest-priority ISR on the chip and must do nothing but one GPIO write and a counter increment. |
| **Control loop** (sensors, state machine, PID, interlocks, MQTT publish) | **One FreeRTOS task**, priority 5, 100 Hz | Needs a hard 10 ms period. Its work is CPU-cheap and highly interdependent — splitting it across tasks would add message passing for no benefit. Sensors are already async (`start_read` / `try_get`), so the loop never blocks. |
| **Display** (render, flush) | **One FreeRTOS task**, priority 3, 100 ms | 🔴 **Added 2026-10-01, and the only addition to this table.** A 1 KB frame goes out as eight I²C writes — tens of milliseconds of bus time — and it was being paid *inside* the control tick, which is the whole of "the control tick overruns its 10 ms budget in 62 % of ticks" (09 §24). Moving it is what makes the 100 Hz control period achievable, and it is a slow consumer, which 04 §3.2 already says is the case for a boundary. The hand-off is lock-free (a double buffer), never a queue: see [`intentional-diffs`](../intentional-diffs.md) and 09 §28 for the two cross-task blocking primitives that assert on this toolchain. |
| **Sensors** (DS18B20, switches, ABP2) | **stays on the control task** | 🔴 **Tried and removed.** 04 §2's "sensors are already async" is true of the *drivers* and false of the *bus*: the DS18B20's bit-bang is the only user of `esp_idf_hal::interrupt::free`, which on this chip is a process-global cross-core critical section, and running it from a second task asserts inside the `FreeRTOS` kernel on every boot. Measured, bisected and recorded in 09 §28. |
| **Network / HTTP** | **FreeRTOS task(s) inside lwIP/esp-idf-svc**, priority 3 | A JSON render, a LittleFS read, or an OTA flash write is unbounded in duration. The C++ code already accepts this (AsyncTCP is a separate task); the fix is that our control loop at priority 5 now *outranks* it, and the network task is explicitly low priority. |
| **Config / NVS writes** | **Synchronous, inside the control task, rate-limited** | NVS writes are slow (erase + program). The C++ code already does this. Introduce an async writer only if measurement shows it blocks the loop > 5 ms. |
| **Watchdog feed** | **Control task only** | A single feed point means a hang anywhere in the control path trips the TWDT. Network stalls must *not* feed it — otherwise a network deadlock hides a control fault. |

Explicitly **not** separate tasks: display rendering, MQTT publishing, sensor polling.
The C++ firmware interleaves these in one loop; keeping them there is a deliberate choice
to minimise migration risk, and their combined cost (10 ms budget) is measured in
R2-09 before anything is split.

### Priority table

| Task | Priority | Core | Stack | Watchdog |
| --- | --- | --- | --- | --- |
| `control` | 5 | 1 | 8 KB | subscribed (5 s) |
| `heater_isr` (fallback only) | ISR prio 3 (max) | any | — | n/a |
| `provisioning` (only during provisioning) | 4 | 1 | 6 KB | not subscribed |
| `housekeeping` (log flush, maintenance) | 2 | 0 | 4 KB | not subscribed |
| lwIP / AsyncTCP equivalent | 3 (per IDF defaults) | 0 | per IDF | not subscribed |

Rationale for core assignment: the control task is pinned to core 1 so it shares nothing
with the Wi-Fi ISR-heavy work that IDF keeps on core 0. The hardware PWM timer is
independent of both.

---

## 3. Task boundaries as seams

Every task is spawned through a small function so the executor can change without touching
call sites. The `spawn_*` functions are the **only** place that knows about FreeRTOS
threads.

```
                       ┌──────────────────────────────────────────┐
                       │            control task (prio 5)         │
                       │                                          │
  sensors ───────────► │  SensorAggregator ──► StateMachine      │
  switches ─────────►  │        │                 │              │
  config (read) ────►  │        ▼                 ▼              │
                       │   Actuators ◄──── ProcessControl         │
  displays ◄─────────  │        ▲                                 │
  mqtt ◄────────────►  │        │                                 │
  commands ◄─────────  │  SafetyMonitor (every tick)             │
                       └───────────┬──────────────────────────────┘
                                   │ events (bounded queue, drop-oldest)
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
      ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
      │ housekeeping │    │  http server │    │  provisioner │
      │   (prio 2)   │    │  (lwIP task) │    │  (prio 4)    │
      └──────────────┘    └──────────────┘    └──────────────┘
```

Command and event flow, in full:

### 3.1 Internal structure: functional core, imperative shell

The task boundaries above say nothing about the shape of the work *inside* the control
task. That is where the C++ design's real problem lives: `LoopManager::update()` is a
god-function that reaches into ten-plus subsystems in eight fixed steps
(`src/core/LoopManager.cpp:90-253`; the coupling is catalogued in
[01 §4](./01-feature-inventory.md#4-execution-model-today)). Every new feature adds a step,
and the order becomes load-bearing and untestable.

The control task is therefore structured as an **Elm-style reducer with a polling shell**:

```rust
// Pure. No I/O, no time, no allocation. Host-testable, exhaustively.
pub fn reduce(state: &MachineState, ctx: &Context, ev: Event) -> (MachineState, Vec<Effect>);

// The tick, in the shell. Single-threaded. This is the only clock.
fn tick(&mut self) {
    // 1. SENSE — read hardware into plain values (start_read / try_get, never blocking)
    let samples = self.sample();
    // 2. DECIDE — fold events through pure reducers. No hardware access in here.
    let effects = self.reducer.step(&samples, self.inbox.drain());
    // 3. ACT — the only place hardware is written. Synchronous, same tick.
    self.applier.apply(effects);
    // 4. NOTIFY — hand slow consumers their copies. Never awaited.
    self.outbox.publish();
}
```

Four properties fall out of this, and they are the reason to prefer it:

1. **`MachineState` is the single source of truth for control**, and the reducers are
   pure functions of it. This is *already* the shape of `StateMachine::update()` →
   `checkTransitions()` → `onEntry`/`onExit`; the port makes it explicit and total rather
   than re-deriving a looser version of it.
2. **The safety rules become one auditable list.** `SafetyMonitor::reduce` returns a
   `SafetyVerdict` *and* the `Effect::DisableHeater` / `DisablePump` / `CloseValve` that
   enforce it. The C++ version scatters S1-S11 across `EmergencyStopManager`, the
   `HardwareManager` guard clauses, the `BaseState` constexpr exclusion lists, and
   `valveSafetyShutdownCheck`. One reducer, one place, one test table.
3. **Effects are the only path to hardware.** `applier.apply()` is the sole caller of
   `Actuators` and `HeaterOutput`. A new state cannot accidentally poke a relay, because
   it has no way to reach one — it can only return an `Effect`.
4. **The god-function becomes a fixed four-step tick.** Adding a feature means adding an
   `Event` variant and a `reduce` arm, not a seventh step in a 250-line function.

### 3.2 Where the queues are, and where they deliberately are not

This is the part that makes it an event architecture *and* keeps it safe. The line is
drawn at the tick boundary.

| Path | Mechanism | Why |
| --- | --- | --- |
| Hardware → `Event` | **direct call, same tick** | Sensing is cheap and already non-blocking (`start_read` / `try_get`) |
| `Event` → `Effect` → actuator | **direct call, same tick** | **The safety latency budget.** Today the overtemp trip is same-loop immediate. Putting a queue here adds a scheduling hop on the most safety-critical path in the machine, and the consumer can be preempted. Not a trade worth making. |
| Control → display / MQTT / web | bounded queue, drop-oldest | These are the genuinely slow consumers. A 5 s MQTT publish, a full `AsyncJsonResponse` render, a LittleFS read — none of them may sit in the tick. |
| Network task → `Event` | bounded `Queue`, drop-newest | A `POST /api/...` becomes a `Command`, never a direct call into control state. This is what breaks the `LoopManager` ↔ `MQTTManager` ↔ `WebServerManager` coupling. |

**The cross-task primitive must be `hal::task::queue::Queue<T>`, not a `heapless::Deque`.**
> **Correction (2026-09-28).** An earlier draft specified `heapless::Deque` and then
> forbade locking the control task's data — a contradiction. A `heapless::Deque` has no
> interior mutability, so a producer in another task **cannot** push to a deque the
> control task owns without a lock; and `hal::task::queue::Queue<T>` is bounded by
> **`T: Copy`** (`task.rs:980`). So `Command` and `Event` must be `Copy`: fixed-size
> payloads only — `heapless::String<N>`, a config key index, a small `Copy` struct. **No
> `String`, no `Vec`, no `Box` in a cross-task message.** State this as a bound in the enum
> definitions so the compiler enforces it.

**No broker, no actors, no per-subsystem mailboxes inside the control task.** FreeRTOS is
already a scheduler; a second one built out of actor mailboxes would add priority
inversion problems without adding capability. A cross-task `Event` is a plain enum in a
`heapless::Deque`, not a message-passing fabric.

**Events are also not free.** An `Event` queued while the consumer is stalled must have
drop-oldest semantics or a single noisy sensor will flood the queue; and a pure event log
is *harder* to debug live than an imperative tick. Keep the per-tick `LOGF(DEBUG, ...)`
state snapshot alongside the event stream so a monitor can still show
`tick 4231: state=BREW_PREINFUSION, pump=on` without a trace viewer.

- **Commands into the control task** (brew start, setpoint change, factory reset) arrive on
  a bounded `hal::task::queue::Queue<Command, 32>` drained at the top of every tick. Never a
  direct call from a network task into control state — that is how the C++ code couples
  `LoopManager` to `MQTTManager` and `WebServerManager`.
- **Events out of the control task** (temperature changed, state entered, telemetry
  ready) go to a bounded `Queue<Event, 32>` with **drop-oldest** semantics.
  A slow consumer degrades telemetry freshness; it must never back-pressure the control
  loop. The display is a consumer and *must not* be able to stall safety logic.
- **Shared state** is either (a) an `AtomicU32`/`AtomicBool` for the handful of values
  genuinely read from another task (`water_tank_full`, `emergency_latched`, a
  temperature `AtomicU32` bit-cast from `f32`), or (b) a command/event message. **No
  `Mutex` around a data structure that the control task owns** — a lock there is a
  deadlock waiting to happen.
- **Errors** are `Result<_, AppError>`. Inside a tick, errors are logged and converted
  into state-machine input (e.g. a sensor read failure becomes a `SensorError` event), not
  propagated up. There is no `?` in the control loop.

### Deadlock, race, and priority-inversion avoidance

- No lock is held across an `await` or across a FreeRTOS yield point. The only lock in
  the design is the config store, held for a bounded `NVS` read/write and never across
  anything else.
- No task waits on another task. Queues are the only cross-task channel, and every
  producer is non-blocking (`try_send`).
- The control task is the **highest-priority non-ISR task**, so nothing can preempt it. A
  network task that blocks on a full queue drops the event rather than waiting.
- `critical-section` (the `critical-section` crate, used by esp-idf-hal) is a FreeRTOS
  recursive mutex and therefore **is not ISR-safe**. ISR context must therefore use only
  `hal::task::queue::Queue` and `hal::task::notification` — and the design gives ISRs
  nothing to do beyond one GPIO write.

### Cancellation and backpressure

- The control task runs until `esp-idf-svc` shuts down; it has no cancellation path. This
  is intentional — a running control loop must not be cancellable.
- Every queue is bounded. `Command` drops-newest (the newest request is the most
  relevant); `Event` drops-oldest (freshness wins). Both are `hal::task::queue::Queue`,
  so `Command` and `Event` are `Copy` — no `String`, no `Vec`, no `Box` in a message.
- The provisioning task is **only** spawned when no valid credentials exist, and it exits
  after success. It is never a permanent task.

### Watchdog

`hal::task::watchdog::TWDTDriver` with a 5 s period and `panic_on_trigger = true`,
matching the current `esp_task_wdt_init(5, true)` (`Resilience.h:420-449`). The control
task subscribes and is the **only** subscriber and the only feeder. This preserves the C++
semantics exactly: a hang in control logic reboots the machine into a relays-off state;
a network stall does not, but also does not get to hide a control fault.

> **Correction (2026-09-28).** An earlier draft said `main()` owns the driver and passes
> `&TWDTDriver` to the control task. **That does not compile:**
> `esp-idf-hal/src/task.rs:755` has `unsafe impl Send for TWDTDriver<'_>` and **no `Sync`
> impl**, so a shared reference cannot cross a thread boundary. `WatchdogSubscription`
> (`task.rs:757`) is `PhantomData<&'s mut ()>` with only `feed(&mut self)`, so it is not
> shareable either. The workable — and better — design is the inverse:
> **`TWDTDriver` is *moved* into the control task**, which calls `watch_current_task()`
> itself. `main` never touches it. The subscription is created and dropped inside the
> control task and `feed()` is a method on it, so **no other task can feed the watchdog by
> construction.**

Also note `TWDTConfig` has a third required field, `subscribed_idle_tasks: EnumSet<Core>`,
defaulting from `CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU0/1`. So "the control task is the
only subscriber" is **not automatic** — IDF also subscribes idle tasks unless those are
set to `n`. Set them explicitly. And `TWDTDriver::new` calls `esp_task_wdt_reconfigure`
(not `init`) when the TWDT is already up. `feed()` returns `Result`.

---

## 4. Startup, shutdown, and fault handling

### Startup sequence (must be equivalent to `SystemInitializer::initialize()`)

Actuators are driven to a **known-safe state before anything else runs**, and this is
asserted, not assumed:

1. `esp_idf_svc::init()`.
2. Configure the pin map, and **write every output pin to its inactive level** before
   configuring any driver. On the original ESP32 the boot state of GPIOs 2, 17, 27, 1, 19,
   26 is undefined; the C++ code handles this with
   `HardwareManager::initializeRelays()` calling `->off()` per relay with a `yield()`
   between (`HardwareManager.cpp:70-93`). The Rust version does the same and adds a
   **post-init readback assertion** that the three relay pins are at the inactive level.
3. Bring up the watchdog and **move** the `TWDTDriver` into the control task; `main`
   keeps no handle and never feeds it (see §3.4). **Do not** feed it yet.
4. Display (so errors are visible), then config/NVS, then LittleFS, then sensors, then
   relays' owning state machine, then network.
5. **Actuate the emergency-stop path**: trip emergency stop, verify the heater command is
   zero, then clear. If the heater command is not zero, abort boot into
   `HardwareFault` and stay there with the heater physically off. This is a new, stronger
   check than the C++ firmware performs.
6. Start the control task, then feed the watchdog.

If any step before 4 fails, log over UART, drive the relays off, and halt. Never
`exit(0)` as the C++ code does at `main.cpp:122-126` — that leaves relays in whatever
state they were.

### Shutdown

Three levels, matching the C++ semantics but stronger:

| Level | Trigger | Action |
| --- | --- | --- |
| `safe_hardware_shutdown` | standby, OTA start, error state entry | heater off, pump off, valve closed, LEDs off. **Not** latched — hardware may be re-enabled. |
| `emergency_shutdown` | overtemp, invalid reading, hardware fault | same, **plus** latch `emergency_latched`; every energising call is refused until explicitly cleared. |
| `boot_halt` | init failure before the display exists | relays off, loop forever, no reboot. |

### Fail-safe behaviour

- **Watchdog reset** is the outermost fail-safe. Relays are de-energised by the ESP32's
  own reset state, which is why the pin map puts actuators on outputs that default to
  inactive.
- **Emergency latch** is checked inside `Actuators::enable_pump`, `enable_heater`,
  `open_water_valve`, `open_steam_valve` — **not** at the call sites. In the C++ code the
  check lives in `HardwareManager` and relies on every state going through it; in Rust it
  is a precondition of the method, so it cannot be bypassed by a new state.
- **`valve_safety_check`** runs every tick with an explicit whitelist, exactly as
  `BrewHandler.h:105-122`. The Rust version is a `const fn water_flow_allowed(state) -> bool`
  so the whitelist is a compile-time-visible match, and **adding a water-flow state is a
  compile error until the whitelist is updated** (via a `match` with no `_` arm).
- **OTA** must call `safe_hardware_shutdown`, not just `disable_heater` — this fixes the
  gap in `SystemInitializer.cpp:54-59` recorded in
  [01 §6](./01-feature-inventory.md#6-safety-critical-control-paths).

---

## 5. Heater output — the one hard real-time path

Preferred: **LEDC hardware PWM** (`hal::ledc`), with the PID output mapped to duty cycle.
Zero CPU, zero jitter, immune to scheduler stalls.

> **This is a behaviour change and must be recorded as one.** The C++ implementation is
> a **1 Hz / 100-step chopper**: `windowSize_ = 1000` ms (`context/ProcessState.h:183`)
> with `ISR_COUNTER_INCREMENT = 10` per 10 ms tick. R1-07 must decide the target frequency,
> record it, and add it to `intentional-diffs.md`. Two more caveats
> from `esp-idf-hal/src/ledc.rs`: the duty must not exceed `2^N - 1` at max resolution
> (20-bit on the original ESP32, 14-bit elsewhere), and the **original ESP32 is the only
> chip with LEDC high-speed mode**. Driving a *relay* coil is defensible — the machine
> already chops it today — but the contactor's minimum on/off time constrains the choice, and
> that constraint is **still unmeasured**. `HardwareManager::setHeaterPower`
already models a percentage that is currently a TODO (`HardwareManager.cpp:305-318`), so
this also un-finishes a known stub.
>
> ⚠ **"The contactor already chops it today" bounds the *duty cycle*, not the *rate*.
> Read the ISR rate as the switching rate and the target frequency comes out a hundred
> times too high.** See the R1-07 decision below.

Fallback: a **GPTimer** (`hal::timer`) with `auto_reload_on_alarm` at 10 ms, whose ISR
only does `pin.set_level()` and `counter += 10; if counter >= window { counter = 0 }`. This
is the direct translation of `isr.h:96-118`, and the RTL-style test in
[06 — Task list](./06-migration-task-list.md) R1-07 is a table-driven unit test over
`(pid_output, counter) -> level` run on the host.

> ### R1-07 decision, recorded 2026-09-28 — **corrected 2026-09-28**
>
> **LEDC hardware PWM, 1 Hz carrier, `Bits17` resolution, low-speed mode.** The
> **1 Hz chopper window is kept**, so the PID's control law and every gain in
> `defaults.h` are unchanged; only the delivery mechanism moves. The window
> arithmetic, the duty→count mapping and the deadman gate are in
> `cc_domain::heater` (host-testable); the pin is in `cc-hal-esp32::heater`, behind
> the `HeaterDuty` seam so the GPTimer fallback stays swappable.
>
> **The carrier is low, and that is a hardware requirement, not a rounding
> argument.** The C++ ISR fires 100 times a second, but its predicate
> `pidOutput > counter` is monotone, so the relay level changes **twice** a second
> (one falling edge inside the window, one rising edge at the wrap) — and not at
> all at duty 0 or at full duty. A square wave makes `2f` changes per second, so
> `f ≤ 1 Hz`, and 1 Hz is also the only frequency at which one carrier period *is*
> one control window. An earlier revision of this document specified **100 Hz** on
> the reasoning that it "reproduces the existing 10 ms-step / 1 Hz chopping
> exactly". **That was wrong** — 100 Hz would switch a 2 kW boiler contactor
> **200 times a second, a hundred times the C++'s mechanical duty** — and it has
> been corrected here, in `intentional-diffs.md` #5, and in both crate module docs.
>
> Three facts about the pair, read out of ESP-IDF v5.5.5's own source rather than
> guessed (`ledc_calculate_divisor`, `esp_driver_ledc/src/ledc.c:459-477`;
> `LEDC_IS_DIV_INVALID`, `ledc.c:115,111`; `precision = 1 << duty_resolution`,
> `ledc.c:600`):
>
> 1. **At 1 Hz the reachable resolutions on the original ESP32 are 17, 18, 19 and
>    20, and nothing coarser** — `div_param = (80e6 << 8) / (1 · 2^bits)` exceeds
>    `0x3FFFF` at 16 bits and below. `Bits17` is the coarsest that works
>    (`div_param` 156 250, period exactly 80 000 000 APB clocks = **1.000 000 Hz**),
>    and coarsest-that-works leaves the most margin against the divider maths
>    being wrong. The C++ chopper is reproduced to **3.8 µs** — 0.0004 % of the
>    window, three orders of magnitude inside the 1 % acceptance bound.
>    *For the record, the discarded 100 Hz analysis was also wrong in its table: at
>    100 Hz the valid resolutions are 10–19 bits, not 8–10, and `div_param` at
>    100 Hz / `Bits10` is 200 000, not 50 000.*
> 2. **`Bits20` must be avoided, and a `const` assert keeps it out.** ESP-IDF's own
>    comment in `ledc_channel_config` says that on the ESP32 "100 % duty cycle
>    (i.e. `2**duty_res`) is not reachable when the binded timer selects the
>    maximum duty resolution", and 20 bits is the maximum — which is also why
>    `Resolution::max_duty` is `2^20 - 1` there. At 17 bits, `max_duty` is a plain
>    `131 072`, so duty `max_duty` is a *steady high level* and duty 0 a *steady
>    low level*, and the two are distinguishable — which is what makes "disabled"
>    a different register value from "100 %".
> 3. **High-speed mode is not needed.** It exists for multi-MHz carriers; at 1 Hz
>    low speed is six orders of magnitude inside its range, and high-speed timers
>    are the scarce resource on this part.
>
> **Not yet verified on hardware, and this is why R1-07 is still open.** Matching
> the C++'s transition rate is *necessary and not sufficient*. Still unknown, and
> **not guessed here**:
>
> * the contactor's **minimum on-time and off-time** — the software guarantees it
>   never requests a pulse narrower than the C++'s own 10 ms step, but whether
>   10 ms is inside the contactor's ratings is a datasheet/measurement question;
> * **whether a hardware-PWM output is acceptable to the coil at all** at 1 Hz, a
>   frequency the C++ never *produced* even though its average was 1 Hz;
> * the **realised frequency and duty on the pin** — no scope has been attached;
> * whether 1 Hz is the *best* point on the wear-versus-resolution curve, which is
>   a decision for someone with the machine.
>
> R1-07 steps 1 and 2 (dummy load, scope) are **not run**: no scope is attached, the
> board's boiler-disconnection state is unconfirmed, and skill §2 rule 4 forbids an
> energising test without a reviewed procedure. The hardware acceptance criterion
> is unverified and is left that way. See
> [`intentional-diffs.md` #5](./intentional-diffs.md#5-the-heater-is-driven-by-ledc-not-a-10-ms-isr-🔴-changed).

Either way, `HardwareActuator` owns `pin` and `window` and nothing else touches them. The
`heater_enabled` boolean the C++ code maintains is **deleted** — the actuator's own
command register is the truth, and there is exactly one writer.

---

## 6. Crate layout

**The workspace is at the repository root**, not in a subdirectory. The `justfile`,
`scripts/`, and `ui/` all already live there, and a subdirectory would put the recipes'
relative paths in the wrong place.

> **The root `partitions_4M.csv` is C++-owned and must not be modified until R4-10.**
> The Rust table is a **new file** at `rust/partitions_4M.csv`. An earlier draft of 04 put
> the workspace in a `clevercoffee/` subdirectory with its own `partitions_4M.csv`, which
> made it easy to clobber the production table. The distinct name is deliberate.

```
.                                    # repository root = Cargo workspace
├── rust-toolchain.toml             # channel = "esp"
├── .cargo/config.toml              # target = "xtensa-esp32-espidf", [idf] partition_table
├── Cargo.toml                      # [workspace] members + [profile.release] + [workspace.lints]
├── justfile                        # developer recipes (root, next to ui/ and scripts/)
├── just/size.just                  # image-size reporting (07)
├── mise.toml                       # host tools (merged into the existing .mise.toml)
├── partitions_4M.csv               # C++-OWNED. Do not touch until R4-10.
├── rust/
│   └── partitions_4M.csv           # Rust table (rebalanced, R2-03)
├── crates/
│   ├── cc-domain/                  # ── portable, no_std, host-testable ──
│   │   Units, Temperature, Pressure, Weight, Duration
│   │   BrewMode, MachineState (18 variants), ErrorCode
│   │   Pid (controller; port of lib/Arduino-PID-Library)
│   │   EmergencyPolicy  (S1/S3)
│   │   WaterFlowPolicy  (S5 — compile-time whitelist)
│   │   InterlockPolicy  (S2/S4)
│   │   Shot, MaintenanceCounter    (F32)
│   │
│   ├── cc-safety/                  # ── portable, no_std, zero I/O ──
│   │   SafetyMonitor: the one place that decides
│   │     "may the heater / pump / valve be energised right now?"
│   │   Consumes telemetry, produces a SafetyVerdict.
│   │   No dependencies outside cc-domain. The whole crate is a pure reducer
│   │     of (telemetry, config, state) -> verdict, so every safety rule in 01 §6
│   │     is a host unit test.
│   │
│   ├── cc-config/                  # ── portable model + trait-bounded store ──
│   │   ConfigSchema (96 registered params, typed, const-validated)
│   │   Config: the in-memory value
│   │   trait ConfigStore { load, save, erase_all }
│   │   JsonImport / JsonExport via serde_json
│   │   Host tests: round-trip, defaults, the emergency-temp registration bug
│   │
│   ├── cc-machine/                 # ── portable orchestration ──
│   │   StateMachine (18 states; per ADR-0003)
│   │   Handlers: Brew, Steam, HotWater, Power, Standby, Backflush
│   │   Reducer { step(samples, events) -> Vec<Effect> }   # pure; see §3.1
│   │   enum Event { SensorRead, ButtonPressed, CommandReceived, Tick, … }
│   │   enum Effect { EnablePump, OpenWaterValve, SetHeaterDuty, … }
│   │   trait Hardware { the ports below }
│   │   trait Clock { now_ms }
│   │   trait TelemetrySource { temperature, pressure, water_tank_full, … }
│   │   trait CommandSink, trait EventSink
│   │   Sensors: TempSensor trait + rate-of-change filter (F11)
│   │   Host tests: the C++ suites in test/ ported 1:1, plus an
│   │               exhaustive state x event table over the reducer

│   │
│   ├── cc-display/                 # ── portable layout, no_std ──
│   │   Framebuffer ([u8; 1024]) + DrawTarget impl
│   │   Font: profont/fub glyph atlases as ImageRaw data
│   │   DisplayLayoutUtils port (fixed-width fields, bar+label clusters)
│   │   6 templates behind a trait, mirroring docs/display-architecture.md
│   │   Host tests: render to a PPM, assert against golden images;
│   │               assert 128x64 fit and no row overlap
│   │
│   ├── cc-hal-esp32/               # ── the ONLY crate that imports esp-idf-* ──
│   │   Board trait + Esp32DevkitC impl (01 §2 pin map)
│   │   GpioIn (debounce, long-press), GpioOut, Relay, Led
│   │   HeaterOutput: LedcPwm | TimerIsr   (§5)
│   │   I2cBus, Abp2Pressure, OneWireDs18b20, Tsic306, WaterTankSwitch
│   │   Actuators — the single owner of pump/valve/heater
│   │   Watchdog (TWDTDriver)
│   │   NvsStore: impl ConfigStore
│   │   Wifi, Ota, Logger (telnet + heap shed), Mqtt
│   │
│   ├── cc-provisioning/            # ── captive portal, ESP-IDF specific ──
│   │   SoftAp, DnsIntercept, PortalHttp, CredentialValidation
│   │   (see 05 §5 for the USB discussion)
│   │
│   └── cc-firmware/                # ── the device binary ──
│       main.rs: startup order, spawn tasks, feed watchdog
│       ONE binary named `firmware`, selected by the `board-*` feature.
│       NOT bin/esp32 + bin/esp32s3 + bin/esp32c6: `cargo espflash` cannot tell
│       which binary to flash from `--target` alone, and the artifact path
│       `target/<triple>/release/cc-firmware` would then be wrong.
│
│   Required managed component (added at R1-01 — without it `svc::fs::littlefs`
│   does not compile, because it is gated on esp_idf_comp_joltwallet__littlefs_enabled):
│
│       [[package.metadata.esp-idf-sys.extra_components]]
│       remote_component = { name = "joltwallet/littlefs", version = "^1.22" }
│
│   (For ESP-IDF v6.0+ the same mechanism is used for `mqtt`, via
│    ESP_IDF_SYS_EXTRA_COMPONENTS_FILE — see 02 §7.1.)
└── ui/                              # unchanged React app
```

### Why these boundaries and not others

- **`cc-safety` is separate from `cc-domain`** because it is the crate that must be
  auditable on its own. It has no I/O, no `alloc`, and no dependency that can grow. A
  reviewer can read it in one sitting and be certain nothing else influences the verdict.
- **`cc-machine` and `cc-display` are separate** because display work is the only
  sizeable chunk of the control loop that is not control logic; keeping it out means the
  state-machine crate stays readable and its host tests stay fast.
- **`cc-hal-esp32` is one crate, not seven.** Splitting it per-peripheral would create
  seven crates whose only shared content is the pin map, and the pin map is the thing that
  must be edited in one place. One crate, one `Board` impl, one place to look up a pin.
- **There is no `cc-utils` crate.** Anything two crates need is either domain vocabulary
  or it belongs in one of the above.
- **There is no `cc-web`.** The REST surface is thin; it is a translation of commands and
  telemetry into the existing `cc-machine` ports, and it lives with the HAL's HTTP
  plumbing.

### Dependency direction

```
cc-domain  ←  cc-safety  ←  cc-machine  ←  cc-hal-esp32  ←  cc-firmware
                    ↑                                            ↑
              cc-config ←──────────────────────────────────────── ┤
                                                           cc-display
                              cc-provisioning ──────────────────── ┘
```

Rules, enforced by CI:
- `cc-domain`, `cc-safety`, `cc-machine`, `cc-display`, `cc-config` must not name
  `esp_idf_svc`, `esp_idf_hal`, or `esp_idf_sys` anywhere. A CI grep enforces this, so a
  hardware dependency cannot leak into portable logic.
- Only `cc-hal-esp32` and `cc-provisioning` may depend on `esp-idf-svc`.
- `cc-firmware` depends on everything; nothing depends on `cc-firmware`.

### What compiles and tests on the host

`cargo test --workspace` on macOS/Linux runs `cc-domain`, `cc-safety`, `cc-machine`,
`cc-display`, and `cc-config` — the five crates covering **F1, F2, F3, F7, F11, F15, F16,
F17, F27, F30, F32, F33** and safety paths **S1, S2, S3, S5**. That is the majority of the
behavioural surface, and it is where the existing 340 C++ tests are ported to.

`cc-hal-esp32`, `cc-provisioning`, and `cc-firmware` build only for the ESP targets and
are validated by `cargo clippy --target xtensa-esp32-espidf`.

### Feature flags and target selection

- Cargo features select the **board**: `board-esp32-devkitc` (default), and later
  `board-esp32s3-devkitc-1`, `board-esp32c6-devkitc-1`.
- Cargo features select **optional hardware**: `sensors-pressure`, `sensors-watertank`,
  `temp-tsic306`, `temp-ds18b20`, `display-sh1106`, `network`, `mqtt`, `telemetry`.
- A build-time `const` assertion (`Board::PINS.assert_valid()`) fails compilation on a
  pin that does not exist on the target chip — the Rust equivalent of
  `pinmapping.h:57-101`'s 21 `static_assert`s, but chip-aware, so porting to S3 or C6
  becomes a compile error to fix rather than a silent miswiring.
- `cfg(target_arch)` is used **only** for chip-capability facts (input-only pin set, PSRAM
  presence, native USB). Never for business logic.

---

## 7. Component ownership of hardware

| Resource | Sole owner | Others may |
| --- | --- | --- |
| Heater pin | `HeaterOutput` | read its commanded duty; never write it |
| Pump relay | `Actuators` | call `enable_pump` / `disable_pump` |
| Valve relay | `Actuators` (`ValveState` enum) | call `open_water` / `open_steam` / `close_*` |
| I2C bus | `I2cBus` — a mutex-guarded shared bus | take the bus for a bounded transaction |
| Temperature pin | `TempSensor` impl | nothing |
| NVS | `NvsStore` | call `load` / `save` |
| Watchdog | `main` | the control task holds a subscription and feeds it |

The C++ `ValveState` enum (`CLOSED` / `STEAM_OPEN` / `WATER_OPEN` / `BOTH_OPEN`) is
preserved exactly, because the single valve relay is multiplexed between steam and water
and the bookkeeping bug it prevents is real
(`HardwareManager.h:215-217`, `BrewHandler.h:116-120`).

---

## 8. Testing strategy

| Level | What | Where it runs |
| --- | --- | --- |
| Unit | `cc-safety` verdicts for every S1-S5 branch | host, `cargo test` |
| Unit | State machine transitions — the C++ `test_state_machine`, `test_pid_state_transitions`, `test_brew_preinfusion_pause`, `test_steam_water_injection`, `test_backflush_states` suites ported 1:1 | host |
| Unit | Heater PWM table `(pid_output, counter) -> level` | host |
| Unit | `cc-display` golden-image render of all 6 templates at 128×64, asserting no clipping and no row overlap (per the AGENTS.md OLED rules) | host, PPM snapshots |
| Unit | Config round-trip, defaults, and the emergency-temp registration bug | host |
| Integration | Startup order, watchdog, OTA suspend/resume, actuator off on every error path | **hardware** |
| Parity | Side-by-side `/api/status`, `/api/parameters?filter=all`, MQTT discovery payloads, and the state-transition log against the C++ build | **hardware** |
| Simulation | Wokwi (`diagram.json` already exists) | CI, optional |

The parity harness is the key migration tool: run both firmwares against the same scripted
input, capture the transition log and API responses, and diff. Parity is checked **per
feature**, not once at the end.

## 9. What stays in C++ during the migration

`src/` and `include/clevercoffee/` are untouched until the Rust firmware passes its phase
gates. PlatformIO remains the C++ build; Cargo is added alongside it. The two coexist:

```
pio run -e esp32_usb     # C++ (unchanged, still the production build)
just build-esp32        # Rust
just parity             # both, on hardware
```

Only after the Rust firmware passes the final phase gate is the PlatformIO build
deprecated, and even then it is kept for one release cycle as a rollback path.
