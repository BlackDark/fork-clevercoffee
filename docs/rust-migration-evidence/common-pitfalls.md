# Common pitfalls — the C++ → Rust migration, as three agents independently found them

Companion to `common-design.md`. Sources: `origin/feat/rust-migration-design` (`D`),
`origin/refactor/space2` (`S`), `origin/rewrite/rust` (`R`). Per-branch digests with file
citations in `digests/`.

**A finding marked ●●● was found independently by all three. Those are almost certainly real and
should be treated as blocking. ●● = two branches. ● = one branch, but mechanistically obvious or
corroborated by a fourth source (a recovered oracle, the actual C++ source, or a build failure).**

> ### Read `implementation-evidence.md` as well
>
> **Every marker in this document counts branches reading *documents*.** After
> `rewrite/rust` was built and run on a real board, the evidence tier of many items here rose
> from "predicted" to "reproduced on hardware", and a set of **new** problems appeared that no
> planning document predicted — including two independent bugs that meant **the machine could not
> heat at all**.
>
> The three branches are not peers: `design` is 51 lines of crate skeletons, `space2` has 30 201
> lines that have **never seen a compiler**, and `rewrite` has 72 644 lines with 111 device tests
> running. Only one branch has empirical findings.

---

## 1. The C++ is not correct, and most of the migration is a repair

This is the single most important shared conclusion. The C++ firmware has **blocking safety
defects**, and a bug-for-bug port would faithfully carry them over. All three branches decided to
fix rather than replicate — but they disagree on the boundary, which is itself a decision worth
making explicitly.

### 1.1 Dead safety code (●●●)

> **Both of the headline items below were found by all three branches from source reading — and
> `space2` then *found them again by porting*, as D09 and D59.** The port's job is not to
> rediscover them; it is to not reintroduce them. It reintroduced one: the first HX711 build armed
> its watchdog on the first conversion and reproduced the C++'s "healthy scale measuring nothing"
> defect in fresh code. See `implementation-evidence.md` §2.6.

**`PumpTimer::start()` is never called anywhere in the tree.** `isExpired()` returns false unless
`isRunning_`, which only `start()` sets. Therefore the advertised **5-minute brew limit and
60-second hot-water limit can never fire.** Hold the water switch on a machine with a 2 kW boiler
and the pump runs indefinitely. `MANUAL_FLUSH_RUNNING` and the backflush fill/flush phases are not
covered even if the timer were armed. *(D, S, R — three independent traces of the same six
references.)*

**`heaterEnabled_` is never set.** `enableHeater()` has zero call sites, so `disableHeater()`,
`safeShutdown()`, `disableAllHardware()` and the partial-init cleanup all early-return on
`if (!heaterEnabled_)`. **Every heater shutdown path is inert.** The heater is driven only by the
10 ms ISR. *(S)* — and note the consequence elsewhere: an active OTA leaves the pump and valve
energized *because* the OTA's `disableHeater()` is a no-op. *(S)*

### 1.2 The safety paths that were never implemented (●●●)

**The steam valve has no safety whitelist at all.** `openSteamValve()` checks only `emergencyMode_`;
there is no `steamSafetyShutdownCheck` anywhere. Steam and water **share one physical relay**
(GPIO17; `ValveState.h:8-11` says so explicitly), so an ungated steam open is an ungated water
valve. Closed in the C++ only by the accident that `openSteamValve` has no call site. *(D, S, R)*

**`safety.emergency_temp` / `emergency_hysteresis` are consumed but never registered** in
`getAllConfigParams()`. They are never persisted, exported, imported, or returned by
`/api/parameters`, and are silently reset to the compiled default on every reboot — a live
over-temperature bug on the primary safety path. *(D, S, R — all three found it independently;
`design` confirmed it against a real `config.json` export, 96 leaves, key absent.)*

**Three conflicting emergency-stop thresholds coexist**: a `GlobalTypes` constant of 145, a
`Temperature.h` pair of 145 and 120 (read nowhere), and the live config default of 150 with a 100
clear. `testEmergencyStop()` is dead code. Any port "fixing" this to the datasheet number changes
behaviour. *(D, S, R)*

### 1.3 Actuator-state defects found *while porting* (●●)

These are the ones that only appear when you actually build the thing, and they are the strongest
argument for porting early:

- **`setHeaterDuty` was a no-op — every heater duty was 100 %.** The PID's 0–1000 duty went to a
  variable nobody read, while the state machine's actuator command drove the pin high. The loop
  still converges via the integral term, so it is invisible in normal operation and obvious on a
  bench. *(S)*
- **Hot water pumps with the three-way valve shut.** The valve interlock whitelists brew / manual
  flush / backflush, but hot-water dispense runs *inside* `PID_NORMAL` and `STEAM_RUNNING` with no
  state of its own — so the check closed the valve every tick while the pump kept running, with the
  heater on throughout. *(S, found while porting; D independently identifies the same
  structural gap — no state of its own — from the other direction)*
- **`enterSafeMode` / `exitSafeMode` / `disableWaterOperations` are log-only stubs**, and
  `SensorErrorState::onEntryImpl` calls `enterSafeMode()` believing it disables hardware. *(S)*
- **An active OTA leaves the pump and valve energized.** `LoopManager::update()` returns early
  before the state machine, `valveSafetyShutdownCheck()` and the PID; `beginSession()` is
  additionally called from the network task. *(S)*
- **Backflush states never re-assert hardware in `update()`.** `Filling` opens pump+valve in
  `onEntry` and nothing re-asserts; `Flushing` fails inverted. *(D, S, R — all three. This is
  exactly the ADR-0003 class the project already knows about.)*
- **The water valve is not gated on an empty tank** — only `enablePump()` is. *(D)*
- **The "safe mode" / steam mode is flippable over HTTP from any state** and moves the boiler
  setpoint, unauthenticated behind wildcard CORS with auth off by default. *(D)*

### 1.4 The scale stack is structurally absent (●●)

Nothing calls `HardwareContext::setScale()`, so `scale_` is always null, `getWeight()` always
returns 0.0, the display's null check is always false, and MQTT/web tare + calibration commands
"return success and have no effect". `main.cpp:145-150` logs that support is present. *(D, R)*

`design` and `space2` use this to **defer** HX711 and Acaia BLE entirely. `rewrite` **keeps and
fixes** them (the human owner called the deadness their bug) — and therefore has **no C++ parity
baseline for that stack by construction**. Note the original ESP32 *does* have a Bluetooth radio;
`rewrite`'s docs initially asserted the opposite, and that false belief is what made "drop the
scales" look safe.

---

## 2. `config.json` is the whole migration, and it is where the migration dies (●●●)

All three severed NVS, partitions, LittleFS and OTA. The **only** surviving bridge is:
**export `config.json` from the old UI → flash over USB → import into the new firmware.**

That makes one JSON round-trip the highest-risk artifact in the entire project. Each branch found a
distinct way it breaks:

- **The old importer returns success if ≥ 1 parameter updated.** A mostly-garbage file reports HTTP
  200 "Configuration validated and applied successfully." *(D, S)*
- **The exporter wrote keys its own importer rejects** — it emitted each parameter's *full dotted
  key as a leaf name inside its group object* (`"pid": {"pid.regular.kp": 50.0}`) while the import
  format nests. **Every export the port produced was refused by its own stricter importer.** A
  comma-state bug in the first attempt was caught only by the round-trip test. *(S)*
- **`format_version` was written by the exporter and rejected by the importer** — so **every real
  C++ export, the only file a migrating user has, was refused outright.** It survived two review
  passes because the repo's own `config.json` happens not to carry the key. *(S)*
- **The old export contains 16 keys per real export that the schema does not have.** *(S)*
- **`safety.emergency_*` never appear in the export at all**, so the one safety setting a user
  might most want to change is the one that cannot survive. *(D, S, R)*
- **The schema has no version marker anywhere in the C++.** The only marker is a `_seeded` boolean.
  *(S)*

**Actionable rules that follow:**
1. The import must return a **report** — accepted / rejected-with-reason / unknown-key /
   out-of-range — never a bool. *(D)*
2. Refuse an unread `format_version` **by name and in full, before any field is looked at.**
   Half-applying a newer document is how a downgrade destroys a configuration. *(S)*
3. Validate **cross-parameter**, not per-parameter: `steam.setpoint = 140` with
   `safety.emergency_temp = 120` is a machine that cannot be steamed, and the C++ accepts it. *(S, R)*
4. **Reject unknown fields.** That is what caught D57/D58. *(S)*
5. The round-trip test is mandatory and must use a **real C++ export fixture**, not the repo's
   `config.json` — the repo file is precisely what hides the bugs. *(S, R)*
6. The two repository JSON files **disagree with each other and with the code**: different sensor
   type (TSIC vs DS18B20), different calibrations (−1750.05 vs 1000.0). The root `config.json` is
   *not* the LittleFS seed file. *(S)*

### 2.1 Config schema landmines (●●)

- **Range validation exists on `set()` but not on the NVS load path** — a corrupt or hand-edited
  blob loads straight into live control state including setpoints and PID gains. *(S)*
- **String fields have no length limit and all eight `*_MAX_LENGTH` constants are dead.**
  `CONFIG_REFERENCE.md` disagrees with `defaults.h` on five fields. *(D, S, R)*
- **`String::toInt()` / `toDouble()` return 0 on garbage**, and routing a double through Arduino
  `String(double)` turns `0.005` into `0.01`. *(D, S)*
- **The scale calibration is a *divisor* whose range includes zero, and the negative half is
  legitimate** — the shipped config has −1750.05 because an inverted load cell really reads
  negative. The range **cannot** be narrowed to exclude zero without excluding real devices; the
  fix is a separate `forbid_zero` flag. *(S)*
- **Parameters that look live and are not.** `trigger_type` is registered but *inert* because the
  port wires the relays active-low. `space2` ships a `config_rt` test that **fails when a schema
  key is neither read by the machine nor listed as inert** — the mechanism for catching this class
  of lie. *(S)*

### 2.2 API contract (●●)

- **The auth middleware never calls `setAuthType()`**, so `allowed()` returns true for every
  request — including factory-reset, restart, wifi-reset and all OTA routes — even with
  `system.auth.enabled` set. *(S)*
- **Four plaintext secrets are exported verbatim**, with CORS reflecting any origin; and the
  shipped defaults are **real credentials** (`mqtt.password` = `silvia`,
  `system.ota_password` = `otapass`). *(S)*
- **`docs/api/openapi.yaml` disagrees with the shipped code in 26 places**, including a
  `POST /api/config` that does not exist, a `/api/scale/calibrate` that never existed, and **no
  `securitySchemes` at all**. *(S)*
- **`/api/steam`, `/api/pid`, `/api/backflush` are toggles that ignore the request body** —
  callers cannot set an absolute state. Reproduce the semantics even though it looks like a bug. *(D)*
- **`/api/setpoint` validates 0–150, applies it live, then calls `set()` whose real bound is
  20–110 with the result discarded as `(void)`.** *(S)*
- **`POST /api/parameters` silently skips empty values**, so a string cannot be cleared. *(S)*

---

## 3. Hardware landmines on the original ESP32

### 3.1 The two that will bite first (●●●)

Both of these were **predicted by all three branches and then actually reproduced on the device**,
where each caused a boot panic. See `implementation-evidence.md` §2.2.

**A floating-point instruction anywhere in a level-1 ISR panics the original ESP32.** Xtensa does
not save coprocessor state across an interrupt; `CONFIG_FREERTOS_FPU_IN_ISR` defaults off. The C++
is safe **only by accident** — GCC soft-floats the same source that the `esp` target compiles to
hardware FP. In Rust, `AtomicChopper::tick`'s `self.duty_ms() as f32` was redundant and fatal.
**There is no compile-time warning**: the check is `objdump` plus a careful FP-instruction grep, and
a naive `.s` grep also matches `divn.s`, `un.s`, `moveqz.s`. *(D, R)*

**LEDC cannot produce a contactor-friendly carrier — it panics at boot.**
`ledc_ll_set_duty_start` (esp32's `hal/ledc_ll.h:483-489` only) spins on `conf1.duty_start` inside
`portENTER_CRITICAL` with interrupts masked, up to one full carrier period. At 1 Hz that is ~1 s
against a **300 ms** interrupt watchdog — **and it panics at duty 0 too**, so `just flash` was
landing images that died before the control task started. *(D, R)* **Confirmed by device test:**
Guru Meditation `interrupt wdt timeout` on every boot with the LEDC heater; reverted to the 10 ms
GPTimer ISR. The commit that introduced LEDC and the commit that reverted it are **two commits
apart** on `rewrite/rust`.

### 3.2 The heater switches ~2×/s, not 100×/s (●●)

The 10 ms ISR predicate `pidOutput > counter` is monotone, so the relay level changes about twice a
second and **not at all** at duty 0 or full duty. Anyone reading the ISR rate as the switching rate
would spec a 2 kW contactor at 200 changes/s — 100× the C++ mechanical duty. Contactor min on/off
time, realised pin frequency and duty are **unmeasured by any of the three efforts**. *(D, R)*

### 3.3 The blocking 10 ms pressure read (●●●)

`pressureSensor.h:35` does `delay(10)` every 50 ms — **20 % of the control loop's wall clock,
permanently** — and both the class comment and ADR-0002 describe the path as non-blocking. A naive
split-phase port silently turns the 50 ms period into 60 ms. *(D, S, R)*

It also **checks nothing**: `stat` is computed and never read (a NAK is invisible), and
`requestFrom`'s return is discarded, so a short read converts the **previous** sample as if fresh.
And `counts_to_percentage` divides by full scale while `counts_to_bar` divides by the output span,
so **10 bar reads as 90 %**. *(S, R)*

### 3.4 The ABP2 frame is 12 bytes, not 7 (●)

The C++ reads seven bytes and builds its temperature count from
`data[6] + data[5]·256 + data[4]·65536`, **mixing the second word's status byte with two data
bytes**. It never checks either status byte for a diagnostic fault, and converts out-of-span counts
into negative pressures the over-pressure logic accepts. *(S)*

### 3.5 I²C contention (●●)

The bus runs at the **Arduino default 100 kHz** (`Wire.setClock()` never called) and is shared
between a 10 ms priority-18 control read and priority-6 display flushes. The display is in U8g2
**full-page mode**, so every 10 Hz render pushes 1024 bytes and blocks ~90 ms — exceeding the
project's own 100 ms slow-loop threshold. The C++ has this contention unmanaged. *(D, S)*

### 3.6 The temp sensor fitted is not the one configured (●●)

A **DS18B20** is fitted (ROM `0x41af...7928`, family `0x28`, answering 1-Wire) while
`Config.h:1087` defaults to `TSIC_306`, and nothing detects the mismatch. A spike written to
validate ZACwire decode against real TSIC hardware **cannot run as written**. *(D)*

### 3.7 DS18B20 conversion defects (●●)

`rawToCelsius` folds every raw `≤ DEVICE_DISCONNECTED_RAW (−7040)` to −127, and **−7040 is exactly
−55 °C in 1/128 units** — the bottom of the sensor's range is reported as disconnected. All six
faults are in fact rejected, but all six report as "not connected", so a probe power-on-reset sends
an operator to look at the wiring. And `TempSensor::isValidTemperature` (−50..150 °C) is **dead
code**, so a 165 °C reading is cached, averaged and handed to the PID. *(D, S, R)*

### 3.8 TSIC-306 / ZACwire is the least verified thing in the firmware (●●)

- **The change-rate filter's time base is a frame counter, not time** — it divides by a
  `volatile uint8_t heartbeat` incremented per ISR frame and reset only on an accepted read, so
  every rejected frame makes the next one easier to accept; the counter wraps at 256. A port that
  divides by elapsed time is a *different filter*. *(D)*
- **The same constant is used in two different units in two adjacent files** — raw counts in
  `ZACwire.cpp:58`, degrees in `TempSensorTSIC.cpp:39`, with a `//grad is [°C/s]` comment in the
  counts code. Under the count reading the effective limits are ≈19.5 and ≈0.49 °C/sample. Flagged
  unresolved by both branches that found it. *(D, R)*
- **The no-signal timeout is 100 ms against a 10 Hz sensor** — one transmission period, zero
  margin, on a safety input, and the field is `uint8_t` so nothing above 255 ms is expressible. *(D)*
- **It has never been verified on hardware.** The pure decoder is host-tested against a
  *synthesised* waveform, which proves the arithmetic and nothing about a real sensor. *(R)*
- **On the `esp` target there is no `AtomicU64` and no safe timestamped per-edge callback**, so
  capture must be a ≥128 kHz **poller**, not the falling-edge ISR the app note suggests. *(R)*

### 3.9 Relays and pins (●●)

- **`GPIO2` is a boot-mode strapping pin** and the heater relay is driven **late** — relays are
  created and driven off only after logger, LittleFS, NVS, I²C and the display are up, so the
  strapping sample is not guaranteed correct. The steam LED is on **`GPIO1` = UART0 TX**. *(D, S)*
- **`PIN_HEATER = 2` collides with the strapping requirement**; `design` and `space2` both move the
  heater to GPIO4 and the LED to GPIO21 in their maps. *(D, S)*
- **The heater relay polarity is unresolved and is a hardware fact, not a design decision.** Both
  branches that reached it **refuse a `LOW_TRIGGER` heater outright**, because an undriven GPIO
  during reset energises a 2 kW heater and no firmware can prevent it. `design` escalated this as
  BLOCKED / "must not be implemented as written". `rewrite` adopted the refusal from the recovered
  oracle. **This gates the heater PWM, which gates the whole application layer.** *(D, R)*
- **Four panel switches sit on input-only pins 34/35/36/39 with `pinMode(INPUT)` and no internal
  pull** — the board *must* supply external pull-ups. This does not carry to S3/C6. *(S)*
- **Relay coils must not be powered from the dev board's 3.3 V rail** — the datasheet asks for
  ≥500 mA and three coils plus an SSR exceed it. *(S)*

### 3.10 If the C6 or S3 is ever a target (S)

- **The ESP32-C6-DevKitC-1 does not have enough pins**: 23 exposed → 21 less USB → 15 less the
  module's 6 SDIO flash pins → 14 usable, vs 17 needed. The vendor guide **contradicts its own J3
  table** about whether the flash pins are broken out.
- **The S3-DevKitC-1 v1.0 and v1.1 differ**: RGB LED is GPIO48 on v1.0, GPIO38 on v1.1. A pin map
  written against one hits the LED on the other.
- **Neither chip has a 1-Wire peripheral** (C6 datasheet v1.5 lists none; S3 not retrieved). A claim
  to the contrary in the original task brief was **refuted**.

---

## 4. Concurrency and Rust-platform traps

### 4.1 The watchdog cannot be shared (●●)

`TWDTDriver` is `Send` but **not `Sync`**, and `WatchdogSubscription` is `PhantomData<&'s mut ()>`
with a `&mut self` feed. So control + UI subscription **cannot be built** — two branches designed
it, hit the compile error, and revised to *move* the driver into the control task. Worse, **"only
the control task subscribes" is not automatic**: IDF subscribes idle tasks unless
`CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU0/1` are set to `n`. A silently wrong watchdog. *(D, R)*

### 4.2 Queues cannot carry what you want (●●)

`hal::task::queue::Queue<T>` requires **`T: Copy`**, so no `String`, `Vec` or `Box` in a
cross-task message — use a `heapless::String<N>` or an index. And **`heapless::Deque` has no
interior mutability, so it cannot be a cross-task channel at all.** Both branches specified a deque
in an early draft and then forbade locking, which is a direct self-contradiction. *(D, R)*

### 4.3 The ISR calls flash-resident code (●)

`onTimer` is `IRAM_ATTR` but `Relay::on/off()` and `GPIOPin::write()` are ordinary `.cpp`
functions — and `Relay.h:17-25` **explicitly claims they are ISR-safe**. During a flash erase the
icache can be bypassed → illegal-instruction panic. The ISR also re-programs an already
auto-reloading alarm every tick. *(S)*

### 4.4 `esp-wifi-provisioning` is a feature-graph landmine (●●)

It depends on `esp-idf-hal` with a **non-optional `rmt-legacy`**, and `not(feature="rmt-legacy")`
is exactly what gates `hal::onewire` and `hal::timer`. Cargo feature unification turns it on for
the whole graph, **silently removing the 1-Wire and GPTimer modules** — breaking the heater PWM
and the temp sensor at once. Check `cargo tree -e features -p esp-idf-hal | grep rmt` **before**
adopting. *(D, R)*

### 4.5 Build-system traps that cost real cycles (●●)

- **`-D warnings` on the device target cannot pass** — `esp-idf-sys` emits ~1300 `esp_idf_*` cfgs
  without matching `rustc-check-cfg`, so every use is an `unexpected_cfgs` warning. *(D)*
- **Cargo does not forward a dependency's `cargo:rustc-link-arg` to the binary package.** Without
  `embuild::espidf::sysenv::output()` in the binary's `build.rs`, the final link has no ESP-IDF
  archives and fails on undefined `pthread_create` / `write` / `abort`. *(D)*
- **`ESP_IDF_SYS_ROOT_CRATE` must be exported for a virtual workspace**, or `esp-idf-sys` silently
  ignores `extra_components` — including LittleFS. *(D)*
- **`--cfg espidf_time64` is mandatory** or the build silently uses 32-bit `time_t`. `IDF_TOOLS_PATH`
  is ignored; the knob is `ESP_IDF_TOOLS_INSTALL_DIR`. *(D)*
- **Partition tables cannot be set from `sdkconfig.defaults`** — the knob is
  `[idf] partition_table` in `.cargo/config.toml` plus `--partition-table` at flash time, and **the
  flashable app image does not contain the table**. That is exactly why an app-only flash can never
  fix a layout mismatch. *(D)*
- **`.cargo/config.toml` is read from the cwd upward, not `--manifest-path`** — target recipes must
  `cd firmware/` first or fail with a misleading `can't find crate for core`. *(D)*
- **`build-std` must be set per recipe, not in `.cargo/config.toml`** — a global one also applies to
  host builds and collides with the host's prebuilt `core`. *(S)*
- **Cargo unifies features across a `--workspace` build**, so including the `bsp-*` crates enables
  three chips at once and fails inside `esp-metadata-generated` with a duplicate-macro error. Host
  jobs must exclude exactly the chip crates. *(S)*
- **`espflash monitor` needs a TTY** and is unusable from CI or from an agent. `cargo bloat` cannot
  work against a stripped release profile. *(D)*
- **`esp_restart()` does not flush UART0** — the last two lines before a reboot are always lost;
  every reboot path needs a `uart_wait_tx_done`. *(R)*
- **`esp-idf-hal` 0.47's `hal::onewire` is unusable** (CRC/command helpers still `todo!()`, no
  `attach_interrupt`); bit-banging was used instead. `gptimer_set_alarm_action` rejects
  `alarm_count == reload_count`, so `reload_count: 0` is the correct periodic config.
  `esp-idf-svc` 0.53's `embassy-time-driver` **does not link on its own**. *(D)*

### 4.6 Tooling traps that will bite an agent specifically (●●)

- **`KEY=value` is make syntax.** A `just` recipe written that way passes the literal string
  `MCU=esp32` through to `espflash` — at the exact moment an agent touches real hardware. Use
  positional args. *(R)*
- **A `cargo tree … | grep -q` CI gate fails open.** A text grep matched `esp_idf_svc` but not the
  `esp-idf-svc` dependency name, so the gate passed while enforcing nothing. *(R)*
- **A CI job with no `env:` block silently uses the default ESP-IDF version.** *(R)*
- **A `wifi-reset` recipe leaked the auth password into curl's world-readable argv.** *(R)*
- **A flash dump must never enter the repository** — the device's NVS contains Wi-Fi credentials in
  plaintext. `rewrite` found a foreign Rust firmware already on the bench device with credentials
  in its NVS and a partition table differing from the repo's. *(D, R)*

---

## 5. Size and memory budgets

### 5.1 The C++ starts with almost no headroom (●●●)

`firmware.bin` is **1,539,657 – 1,546,240 B against a 1,703,936 B app slot — 90.4 % used, ~154 KB
real headroom.** A Rust esp-idf `std` image is typically 1.5–2.5 MB, so a **partition rebalance is
mandatory** on that stack. The formula is `(4,063,232 − S) / 2`; the `rewrite` target of 1,835,008 B
slots is the max-`min(app0, app1)` solution. "≥ 2 MB per slot" is **arithmetically impossible** by
131,072 B. *(D, R)*

The one measured counter-example: `design` linked every needed subsystem at **997,648 B = 58.5 %**
of the slot, closing the flash question by measurement rather than argument. `space2`'s bare-metal
build is **99,728 B**.

### 5.2 Static RAM, not flash, is the real constraint (●)

**131,688 B = 41 % of the ESP32's 320 KB**, roughly double the pre-network figure — and ADR-0002's
30 KB heap-shed threshold was tuned against ~75 KB. **94,155 B of it is `.iram0.text` dominated by
the prebuilt Wi-Fi MAC** (`libpp.a` / `libnet80211.a`, `CONFIG_ESP_WIFI_IRAM_OPT=y`), and **no
Rust-side change touches it.** *(R)*

`space2` adds two rules the others do not: no driver may sleep, and the framebuffer must **diff
against the last flushed frame** (an extra 1 KB static) so an unchanged re-render sends nothing. The
log ring is a fixed 4 KB allocated at boot; logger nesting costs ~576 B per nested `LOGF` and can
overflow the 8 KB loop-task stack. PSRAM is absent.

### 5.3 Two measured size wins worth copying (●)

- **`std` pulls in 210 KB of backtrace symbolisation** (`default_hook` → `backtrace_rs` →
  addr2line/gimli/object/miniz_oxide) — useless on a device with no symbol table. One line —
  `build-std-features = ["std/backtrace-trace-only"]` — gave **−162,656 B (−12.05 %)**. The RAM win
  was 1,440 B, **not the 10 % implied**. *(R)*
- **Fonts cost 135 KB more as `embedded-graphics` `ImageRaw` (177,723 B) than as raw U8g2 RLE
  (42,722 B).** Decisive on a size-constrained target, and the reason `embedded-graphics` was
  rejected as the draw target. *(D, R)*

`mbedTLS` (~94 KB flash, ~8 KB RAM) is **not removable and is not our code** — it comes in via
`mqtt` → `tcp_transport` → `esp-tls`, and via `esp_wifi` → `wpa_supplicant` for WPA3/Enterprise.
Cutting it is a **product** decision, not a size optimisation. *(R)*

---

## 6. The PID and the state machine (●●)

- **`PID_v1`'s output clamp lets `NaN` through** — `if (output > outMax) … else if (output < outMin)`
  fails both comparisons for `NaN`, so the divide's `NaN` reaches `*myOutput` unclamped. *(D, R)*
  Note the port is therefore a *parity decision*: `f64::clamp` **panics** on `NaN`; `min`/`max`
  differ again. *(D)*
- **Integer division by zero**: `PID_v1.cpp:85` divides by `SampleTime / 1000` as `unsigned long`, so
  any window < 1000 ms makes the divisor 0 and the output `NaN`. The shipped firmware escapes only
  because sample time == window == 1000. **Any heater-output change trips it.** *(R)*
- **The shipped gains make the controller behave like bang-bang control** (`kd = Tv·Kp = 713`; a
  3.1 °C change moves the D term by ~22,000 against a 1000-wide window; output alternates
  1000 → 0 → 0). Preserved faithfully, flagged for a human. *(R)*
- **The brew-detection gains always overwrite the regular gains.** `calculateDerivedValues()`
  computes `aggKi`/`aggKd` from `pid.regular.*`, stores them, then immediately recomputes from
  `pid.bd.*` and stores over the top. In the shipped config the BD `tn` is 0, so **the brew PID has
  no integral action at all.** *(S)*
- **Anti-windup's dead band exists only in `P_ON_E`.** In `P_ON_M` — the mode the brew-detection
  tuning uses — there is no conditional integration at all and the integrator accumulates while
  saturated. A port with one unconditional gate is wrong in one mode or the other. *(D)*
- **Window size, PID sample time and output limits are one number in three roles**, set from
  `processWindowSize()`: changing the chop window changes the derivative divisor and the duty scale
  at once. *(D)*
- **The Arduino-PID input filter starts at 0**, so the first `Compute()` sees `dInput ≈ −14 000`
  counts; output clamps to 0 and the heater does not start for a full sample period — a symptom
  that reads as a sensor fault. *(S)*

### State machine (●●)

- **The C++ state-machine test coverage is far thinner than the suite count suggests.**
  `test_state_machine` exercises gMock plumbing, `test_pid_state_transitions` tests hand-written
  mock states, and two suites never include the real state sources. The counts are also
  unreconciled (303 macros / 340 cases / 280 / 234 / 50 / 33). **This undercuts treating "303 tests"
  as a safety net** — and it is why all three replace it with an exhaustive table (rewrite:
  18 states × 46 events × 5 flavours = **4,140 pairs**, every pair reaching a named verdict). *(D, R)*
- **`P_ON_M`/`P_ON_E` are inverted between the firmware header and the host-test stub**, so any
  existing host test exercises the **opposite** branch. The existing suite cannot serve as the
  oracle until this is fixed — and every PID golden vector captured before the fix certifies the
  wrong branch. *(D)*
- **`SENSOR_ERROR`'s recovery clock is measured from *entry*, not from when the error clears** —
  the guard has no exclusion list, so `checkSpecificTransitions()` is never reached and the reset
  is unreachable. The source comment claims the opposite, and one inventory initially believed the
  comment. *(D, R)*
- **`hasUserActivity()` and `shouldExitStandby()` are hard `return false` stubs** — the water switch
  **cannot** wake the machine from standby. One inventory had described a wake path that does not
  exist. *(D, R)*
- **The numeric gaps in the state ids are load-bearing** (`isBrewState` 31..34, `isBackflushState`
  60..63, `state <= 63` for LED eligibility), and so is `executeTransition`'s **self-transition
  skip** — it is what makes the `SENSOR_ERROR` clock bug work as it does. Both must be reproduced,
  not refactored away. *(D)*
- **Emergency stop keeps heating through its ~800 ms debounce window** (three readings at a 400 ms
  cadence), and `clearEmergencyMode` requires a valid reading **and** `temp <= 100 °C`. Exactly
  `0.0 °C` counts as valid. Preserved on purpose, flagged "real exposure", needs a human. *(D, R)*

---

## 7. Display parity (●●)

Only found by an oracle, never by reading the C++:

- **`U8G2_BALANCED_STR_WIDTH_CALCULATION` is defined unconditionally**, so `getStrWidth` adds the
  first glyph's x-offset back in; every string starting with an inset glyph measures 1–2 px narrow
  and **every centred/right-aligned label moves**. *(D, R)*
- **U8g2's unsigned coordinate wrap means a glyph box above the display is *clipped*, not
  dropped** — `u8g2_is_intersection_decision_tree` has `if (v0 > v1) return 1` in both branches. A
  Rust port using signed coordinates with "reject if off-screen" renders **blank** where C++
  renders a partial glyph. *(D, R)*
- **The degree sign is one Latin-1 byte and Rust `&str` cannot hold a lone `0xB0`** — `"\u{b0}"`
  is two bytes and the byte walk measures 30 px instead of 25 in `profont10`. **Every temperature
  readout is affected** and nothing in the compiler flags it. *(D, R)*
- **`drawCircle`/`drawDisc` draw 8 pixels/lines per section, not 6/12.** *(R)*
- **The bare and `prepareDisplay`-prepared displays differ** (`ExtendedText` uses `ascent_para` 7
  vs `ascent_A` 6; without `pos top` the Modern template's `fub20` at y=14 renders at rows −9..13,
  **clipped off the top**). *(R)*
- **`kFontHeightFub20 = 23` / `kFontHeightProfont17 = 15` are hand-measured literals for the drawn
  glyph subset**, not font metrics (`max_char_h` is 36 and 17). Port the literals; do not recompute. *(D)*

---

## 8. Process and verification traps

### 8.1 The traps that let defects through (●●)

- **A missing test command is a shipped bug.** 68 `#[test]`s in `cc-hal-esp32` were type-checked by
  `just lint-esp32` and **executed by nothing**, because `cargo test` cannot build the crate. Two
  real device bugs shipped through that gap; running the tests found three more, including an MQTT
  path that **panicked on first publish and would have reset the chip**. The fix is
  `just test-audit`, which fails on an unregistered test, a stale entry, a duplicate, **or any
  `#[ignore]`**. *(R)*
- **A test that asserts the wrong direction hides a bug.** `space2`'s interlock test originally
  asserted *equality* between the states that command the valve and the states allowed to hold it
  — precisely the assertion that conceals a missing state. The fix asserts only the safety
  direction. *This is the single most transferable lesson in the corpus.* *(S)*
- **An exhaustive table can silently collapse to one case.** The 4 140-pair `state × event` table
  reduced to 3 outcomes and only ever exercised *emergency*, because guard 1 always wins when every
  predicate is true at once. The test now asserts it **hasn't** collapsed. *(R)*
- **A timing instrument that has never disagreed with a result is not known to be working.** The
  first tick-cost probe took its timestamp *after* the delay and reported 431 ms for a 400 ms
  period. It would have hidden a real overrun — and a criterion passed *because* of it. *(R)*
- **A missing test command means tests that were never run.** Superseded by the `test-audit` guard
  above; see `implementation-evidence.md` §2.4. *(R)*
- **The repo's own `config.json` is a misleading fixture** — it lacks `format_version` and the
  `safety.*` keys, so it passes an importer that rejects every real export. *(S)*
- **An empty baseline must be an error, not a skip.** A directory of Rust-produced observations
  labelled `cpp/` "would be a lie that the parity runner then confirms on every future run." *(R)*

### 8.2 Claims that must not be made (●●)

- **"A capability that compiles is not a capability that works."** Only `device-verified` counts.
  All three adopt the `repo-verified` / `build-verified` / `device-verified` / `needs confirmation`
  ladder, with the rule that raising a level requires the named evidence **in the same commit as
  the claim**. *(D, S, R)*
- **A synthesised waveform proves the arithmetic and nothing about a real sensor.** *(R)*
- **Multi-drop 1-Wire enumeration is unverified** — and the honest framing is that a real machine
  has one sensor, so it is a gap in the tests rather than a path the firmware takes. *(S)*
- **`space2` reports its own board crates have never seen a compiler** and calls that "the fact
  most likely to be forgotten by the next reader". Its stated next action is therefore not a task
  but a toolchain. *(S)*
- **Design-phase estimates of U8g2 font metrics cannot be read from the repository**, so a display
  port built on new font tables verifies its layouts against *its own* metrics. Physical
  appearance stays unverified until a human looks at the panel. *(S)*

### 8.3 Safety rules that should be non-negotiable (●●)

- **Never run brew, backflush or manual flush during a spike.** One firmware on the chip at a time;
  log C++ for 10 minutes, log Rust for the same 10 minutes, diff offline. *(D)*
- **Every task that energises anything carries a written procedure**: element physically isolated,
  observed at the relay input pin, duty swept 0→100→500→1000→0, power-cut means. *(D)*
- **Make the harness structurally safe, not conventionally safe.** `rewrite`'s `cc-parity` crate
  has no GPIO and no HAL crate in its tree, so 12 actuator-energising scenarios run on the host
  with nothing connected — and every scenario closes with an `actuator_safe` assertion (duty 0,
  pump off, both valves closed, latch not set). *(R)*
- **If parity requires removing a safety check, that is a bug in the plan — escalate.** *(D)*
- **If validation fails three times, or the hardware is unavailable, make no success commit.** *(S)*

---

## 9. The defects in the *plans* themselves

Worth recording because all three branches made the same class of mistake:

- **Stale counts after a late correction.** `architecture.md` says the watchdog has one subscriber
  while `task-list.md` still says two (design). 96 vs 99 vs 100 parameters; sixteen vs seventeen
  crates; nineteen vs eighteen states (space2). "18 states not 19, 96 params not 108, 10 fonts not
  11" (rewrite, found by adversarial review — **all six errors in the same direction**).
- **Superseded decision text left in place under a strike-through** rather than rewritten.
- **A conclusion and its own refutation living in different files with no cross-reference.**
  `rewrite` has **two §17s, two §18s, two §19s and two §20s** in its largest findings document; the
  first versions are factually wrong and carry in-file retractions.
- **A dependency graph that makes its own gate un-passable** — `rewrite` put parity-baseline
  capture in Phase 1 while the runner was scheduled for Phase 4, and `space2` has a task
  (delete the C++ tree) whose stated prerequisite "is a parity gate that cannot pass."
- **A false hardware belief that changes a decision** — "the original ESP32 has no Bluetooth
  radio" is what made "drop the scales" look safe.

**Lesson: append-then-retract does not work in a planning corpus.** Corrections must rewrite the
section in place, and any count asserted in prose needs an owner and a test.
