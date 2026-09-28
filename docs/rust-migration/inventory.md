# CleverCoffee firmware inventory (C++ baseline)

**Status:** Complete (A1)
**Last updated:** 2026-09-28
**Related:** [compatibility-matrix.md](compatibility-matrix.md) · [architecture.md](architecture.md) · [tooling.md](tooling.md) · [task-list.md](task-list.md) · [ADR 0004](../adr/0004-rust-migration-platform-selection.md) · [execution skill](../../.agents/skills/esp32-rust-migration/SKILL.md)

This is the reference description of what the firmware does today, written so a
Rust rewrite can be planned against facts rather than assumptions. Every claim
carries a tag:

- **repo-verified** — read from source in this checkout, with a file reference.
- **device-verified** — observed from the attached hardware or from a build run here.
- **needs confirmation** — inferred, contradictory, or dependent on something not
  checked. Treat as an open question, not as a fact.

---

## 1. Hardware

### 1.1 What "old ESP32 v4" means here

The board string in `platformio.ini:12` is `az-delivery-devkit-v4` — the *board
revision* is v4 of the AZ-Delivery DevKit, which carries an ESP32-WROOM-32
module. It is **not** an ESP32-S3 or -C6, and "v4" is not a chip revision.

`espflash board-info` on the attached device (**device-verified**, output saved to
`research/device-board-info.txt`):

| Property | Value |
|---|---|
| Chip type | `esp32`, revision **v3.0** |
| Architecture | Xtensa LX6, dual core, 240 MHz |
| Crystal | 40 MHz |
| Flash | 4 MB |
| Features | Wi-Fi, BT, VRef calibration in efuse, coding scheme none |
| MAC | `ec:62:60:76:b5:3c` |
| Security features | None (no flash encryption, no secure boot) |

Consequences that shape every later decision:

- Xtensa, not RISC-V. Rust support comes from the Espressif fork via `espup`, not
  from upstream stable Rust (**device-verified**, §1.4).
- No native USB peripheral. Serial, flashing and monitoring all run over the
  board's external USB-UART bridge (**repo-verified**: `monitor_speed = 115200`,
  `platformio.ini:37`; no USB-CDC code anywhere).
- Revision v3.0 is exactly the floor `esp-hal` requires, and it qualifies for the
  `CONFIG_ESP32_REV_MIN_3` size optimisation.

### 1.2 Hardware variants

**There are none.** `include/clevercoffee/hardware/pinmapping.h` is a single flat
set of `#define`s with no `#if defined(BOARD_*)`, no variant header and no
alternate pin map anywhere in the tree (**repo-verified**). A Rust port can treat
the pin map as one compile-time constant table, and the board axis exists only as
future-proofing, not as a present requirement.

`PinValidation` in the same header is cosmetic: `isValidPin` only checks
`0 <= pin < 40` and every `static_assert` passes trivially. It does not encode any
ESP32 reality (**repo-verified**).

### 1.3 Pin map

All **repo-verified** from `pinmapping.h`, `HardwareManager.cpp`,
`SystemInitializer.cpp`, `HX711Scale.cpp`.

| GPIO | Name | Dir | Pull | Peripheral / notes |
|---|---|---|---|---|
| 39 | `PIN_POWERSWITCH` | in | external | input-only pin |
| 34 | `PIN_BREWSWITCH` | in | external | input-only pin |
| 35 | `PIN_STEAMSWITCH` | in | external | input-only pin |
| 36 | `PIN_WATERSWITCH` (hot water) | in | external | input-only pin |
| 16 | `PIN_TEMPSENSOR` | bidir | library-owned | TSIC via ZACwire **or** DS18B20 via 1-Wire |
| 23 | `PIN_WATERTANKSENSOR` | in | `PULLDOWN` if NO, `PULLUP` if NC | polled `IOSwitch`, forced TOGGLE |
| 2 | `PIN_HEATER` | out | — | heater relay, driven **only** from the timer ISR |
| 27 | `PIN_PUMP` | out | — | pump relay |
| 17 | `PIN_VALVE` | out | — | valve relay (steam + water share it) |
| 26 | `PIN_STATUSLED` | out | — | plain GPIO, no PWM |
| 19 | `PIN_BREWLED` | out | — | plain GPIO |
| 1 | `PIN_STEAMLED` | out | — | **GPIO1 is UART0 TX** — see §7.1 |
| 21 / 22 | `PIN_I2CSDA` / `PIN_I2CSCL` | bidir | — | one I²C bus: OLED + pressure sensor |
| 32 / 25 / 33 | `PIN_HXDAT` / `PIN_HXDAT2` / `PIN_HXCLK` | — | `PULLUP` on data | HX711 scale — **dead code**, §4.4 |
| 18 | `PIN_ZC` | — | — | dimmer zero-cross; **referenced nowhere** |
| 4 / 3 / 5 | `PIN_ROTARY_DT` / `_CLK` / `_SW` | — | — | rotary encoder; **referenced nowhere** |

### 1.4 Toolchain feasibility on this host

**device-verified** on this machine:

- `espup` 0.17.1 installed the Espressif Rust toolchain; `rustc +esp` reports
  `1.97.0-nightly (8ea53bcd7 2026-07-08) (1.97.0.0)`.
- Both Xtensa ESP32 targets are present: `xtensa-esp32-espidf` (std) and
  `xtensa-esp32-none-elf` (bare metal).
- The C++ baseline builds here: `pio run -e esp32_usb` → SUCCESS in 65 s.

---

## 2. Build, test and release baseline

### 2.1 The Arduino-core question, resolved

The inventory pass flagged a contradiction worth recording because it was the
single biggest technical red flag: `isr.h` uses the Arduino-ESP32 **2.x** timer
API (`timerBegin(0, 80, true)`, `timerAlarmWrite/Enable/Disable`), which was
removed in Arduino-ESP32 3.x, while `platformio.ini:9` declares
`platform = espressif32 @^7.0.1`.

**Resolved, device-verified.** The build succeeds, and the resolved packages are:

| Component | Version |
|---|---|
| PlatformIO platform `espressif32` | **7.1.3** |
| `framework-arduinoespressif32` | **4.20017** = Arduino core **2.0.17** |
| ESP-IDF underneath | **4.4** |
| Xtensa GCC | 8.4.0+2021r2-patch5 |

So the legacy timer API is genuine and the code is consistent with the platform it
actually resolves. Note the consequence for the migration: the baseline sits on
**ESP-IDF 4.4**, while `esp-idf-hal`/`esp-idf-svc` require **ESP-IDF ≥ 5.3**. The
port is therefore also an IDF major-version jump.

### 2.2 Size and memory baseline

**device-verified**, from the build run here:

```
RAM:   [=         ]  14.1% (used 75240 bytes from 532480 bytes)
Flash: [========= ]  90.4% (used 1539657 bytes from 1703936 bytes)
```

**Flash is the binding constraint on the C++ side: 90.4 % of the 1.625 MB app
partition, 160 KiB of headroom.** This number is the reason binary size was
treated as the gating risk for the migration (see [ADR 0004](../adr/0004-rust-migration-platform-selection.md)).

### 2.3 Partition table

`partitions_4M.csv` (**repo-verified**):

| Name | Type | Subtype | Offset | Size |
|---|---|---|---|---|
| `nvs` | data | nvs | 0x9000 | 20 KB |
| `otadata` | data | ota | 0xE000 | 8 KB |
| `app0` | app | ota_0 | 0x10000 | 1.625 MB |
| `app1` | app | ota_1 | 0x1B0000 | 1.625 MB |
| `spiffs` | data | spiffs | 0x350000 | 640 KB |
| `coredump` | data | coredump | 0x3F0000 | 64 KB |

The `spiffs`-labelled partition is formatted **LittleFS**, not SPIFFS. That is the
normal Arduino-ESP32 convention, and the filesystem-OTA path keys off the literal
label `spiffs` (**repo-verified**, `ota.cpp` `FILESYSTEM_PARTITION_LABEL`).

**The Rust firmware deliberately does not preserve this table.** Per
[ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md) it uses
`partitions_rust_4m.csv`, which keeps the app-slot geometry but renames the
filesystem partition to `ccfs` with the honest `littlefs` subtype. Because the C++
firmware can only write a partition labelled `spiffs`, that rename is part of what
makes an old-to-new OTA fail rather than half-succeed.

### 2.4 Environments, scripts, workflows

Three PlatformIO environments (**repo-verified**): `esp32_usb` (canonical),
`esp32_ota` (espota to the hardcoded `silvia.local` with the hardcoded password
`otapass`), `native_test` (host GoogleTest).

`extra_configs = platformio_extra.ini` (`platformio.ini:4`) **points at a file
that does not exist** and is gitignored. Four CI cache keys hash it, which is a
silent no-op. CI being green shows PlatformIO 6.2.0 tolerates it
(**needs confirmation** as to whether that is intended).

Build scripts: `run_clangformat.py` (registers `check-format` / `format` targets,
prefers Docker `xianpengshen/clang-tools:22`, falls back to mise),
`build_frontend.py` (gated on the `buildfs` target, runs `pnpm prepare-esp`),
`tools/platformio_wokwi.py` (registers the `wokwi` target).
`auto_compression.py` is disabled and operates on a `frontend/` directory that no
longer exists — dead.

**Three different clang-format versions are in play**: Docker clang-tools 22,
mise `clang-format = "23.1.2"`, pre-commit `mirrors-clang-format v17.0.6`.
Formatting drift between local hooks and CI is likely (**repo-verified**).

Workflows: `format.yml` (clang-format gate), `main.yml` (version job, firmware
matrix over both ESP32 envs, `pio test -e native_test`), `frontend.yml` (biome,
tsc, vitest, vite build under `ui/`), `release.yml` (tests, firmware, `buildfs`,
publishes `firmware.bin` + `littlefs.bin` + `bootloader.bin` + `partitions.bin`).
`check_tool = clangtidy` is configured but **no workflow runs `pio check`**.

### 2.5 Host tests

`pio test -e native_test`. 41 suite directories, **303 `TEST`/`TEST_F` cases**.
Arduino is stubbed by `test/Arduino.h` (force-included ahead of everything, with a
controllable `g_test_millis` clock) plus 15 further header shims and 16 gmock
mocks (**repo-verified**).

`test_build_src` is deliberately off, so 21 test files `#include` `src/*.cpp`
directly as a workaround (**repo-verified**, reason documented at
`platformio.ini:90-93`).

Already host-testable: the state machine and its safety gates, all handlers,
`ProcessController`, `EmergencyStopManager`, the coordinators, config schema and
JSON round-trip, display maths, and the timer/retry/circuit-breaker utilities.
**Not** host-tested: `WebServerManager` (~1100 lines of routes), `ota.cpp`,
`MQTTManager`, `LoopManager`, `SystemInitializer`, rendering, and every real driver.

Three problems to carry forward:

- `test/.pioignore` is headed "Ignore empty test directories" but lists
  `test_process_controller/` (370 lines) and `test_state_machine/` (91 lines),
  which are not empty. **~460 lines of tests never run.**
- Test counts disagree three ways: `docs/integration-tests.md` says 280,
  `docs/plan/task-list.md` says 234, the actual count is 303.
- `test/TESTING_GUIDE.md` documents `pio test -f test_system_context`, a suite
  that is in `.pioignore` — the documented example cannot work.

---

## 3. Startup and the main loop

### 3.1 Startup order

`setup()` (`src/main.cpp`) then `SystemInitializer::initialize()`
(`src/core/SystemInitializer.cpp`) — **repo-verified**, 19 steps:

1. `logMemory("Setup Start")` — **before `Serial.begin()`**, so it goes nowhere.
2. `g_watchdog.begin()` → `esp_task_wdt_init(5, panic=true)` + `enableLoopWDT()`.
   Armed across the whole of `initialize()`, which **never feeds it**; survival
   depends on scattered `yield()` calls and on Wi-Fi/OTA calling `suspend()`.
3. `initializeLogger()` — `Serial.begin(115200)`. Cannot fail.
4. `initializeConfiguration()` — LittleFS mount (failure = warning only),
   `Config::begin()` (**the only fatal failure here**), NVS seeding from
   `/config.json`, `calculateDerivedValues()`, and construction of the `PID` object.
5. `Wire.begin()` — return value ignored, no `setClock`, so the bus runs at the
   100 kHz default.
6. `initializeDisplay()` — fatal on exception. A **missing OLED is non-fatal**
   (warning, `setDisplay(nullptr)`).
7. `displayLogo(version)`.
8. `initializeHardware()` — constructs relays, LEDs, switches, temperature sensor.
9. `initializeHandlers()` — brew, hot water, power, steam.
10. `initializeNetworking()` — Wi-Fi (blocking, up to 60 s portal), web server
    (failure non-fatal), ArduinoOTA. Fatal **only if not in offline mode**.
11. `initializeMQTT()` — fatal only if MQTT enabled and online. `setup()`
    returning false still yields success ("can be retried later").
12. `initializePID()` — tunings, sample time 1000 ms, output limits 0–1000,
    integrator limits 0–55 (hardcoded "AGGIMAX").
13. `initializeSensors()` — **non-fatal**. A machine with **no temperature sensor
    boots normally.**
14. `ISR::setSystemContext()` — one-shot static pointer, not atomic.
15. `setupTiming()`.
16. `initTimer1()` — timer group 0, prescaler 80 → 1 µs tick, alarm 10 000 µs,
    autoreload.
17. `enableTimer1()` — **the ISR starts firing here.**
18. `markISRReady()`.
19. `systemInitialized_ = true`.

Back in `main.cpp`, only *after* all of that: `StateMachine`, `ProcessController`
and `LoopManager` are created.

**Ordering hazard (repo-verified):** the heater ISR is enabled at steps 16–18,
before `StateMachine`/`ProcessController`/`LoopManager` exist. It is protected
only by the `isISRReady()` flag and by `processPidOutput()` happening to be 0 at
that moment — a flag, not a structural invariant.

### 3.2 Failure paths

- `initialize()` returning false → `Serial.flush(); exit(0);` in `main.cpp`.
  There is no `atexit`/`_exit` override in the repo. On Arduino-ESP32 this ends in
  newlib `exit` → `_exit` → `abort()` → panic → reset, i.e. a reboot loop rather
  than a clean halt (**needs confirmation** — inferred from ESP-IDF behaviour, not
  proven by code here). Nothing on this path explicitly de-energises relays.
- `isInitialized()` false → three `FATAL` logs and then **falls through anyway**
  into `markReady()` and `loop()` with `loopManager == nullptr`, logging
  "CRITICAL: LoopManager is nullptr!" forever with all actuators unmanaged
  (**repo-verified**). The comment in the source even says "the main loop will crash".

### 3.3 Main loop

`loop()` feeds the watchdog (**the only feed site in normal operation**), logs
status every 5000 ms, and calls `LoopManager::update()`. No `delay()`; it free-runs.

`LoopManager::update()` order (**repo-verified**):

1. `Logger::update()` — the actual drain to serial and TCP.
2. `OTA::pollPendingRestart()` + `pollPendingUrlUpdate()`.
3. **`if (OTA::isActive())` → tick OTA, flush log, `return`.** A hard early-return
   that skips sensors, switches, the state machine, process control, LEDs,
   network, website and display for the whole OTA session. Safe because
   `otaPrepareHardware()` disables the timer and the heater first.
4. Sensors: `sensorCoordinator_.update()`, water-tank push.
5. Switches and standby: steam, power, standby coordinator.
6. State machine: `update()`, mirror state id, then hot-water, brew,
   `valveSafetyShutdownCheck()`.
7. Process control and LEDs.
8. Network: Wi-Fi maintenance, MQTT connection + loop, conditional MQTT publish,
   `ArduinoOTA.handle()`.
9. Website: throttled SSE temperature/weight events.
10. Display: `printScreen` every 100 ms.

Intervals, from `include/clevercoffee/constants/Timing.h`: ISR 10 ms; display
100 ms; temperature 400 ms; pressure 50 ms; scale 100 ms; HASSIO discovery 5 min;
debug throttle 5 s; error recovery 5 s; EEPROM recovery 300 s.

**The sensor intervals are vestigial (repo-verified).**
`updateTemperatureSensor()` and `updatePressureSensor()` only increment counters —
the actual sensor I/O happens unthrottled inside `sensorCoordinator_.update()`
every iteration. The 400 ms / 50 ms / 100 ms numbers describe nothing real, while
the timer-performance report still prints "actual Hz" for these no-ops. The
intervals are also **defined twice** (`Timing.h` and `SensorCoordinator.h`) with
matching values today — a drift hazard.

Blocking or unbounded calls in the loop: `ArduinoOTA.handle()`, MQTT
`loop()`/`checkConnection()`, Wi-Fi reconnect, the OTA flash-write path, the I²C
display flush, and the 10 ms blocking pressure read. The slow-loop detector is
compiled out in release (`#ifdef DEBUG`), leaving only two ad-hoc >50 ms / >100 ms
warnings.

### 3.4 Timing assumptions

Loop-rate dependent, with no timer at all: all sensor I/O, switch debouncing and
edge detection, state-machine updates (**at most one transition per iteration**),
valve safety check, and emergency detection — whose **debounce counts loop
iterations, not milliseconds**, so its effective time constant varies with loop
speed (**repo-verified**).

`millis()` rollover is handled correctly everywhere: every comparison uses the
unsigned-difference form `now - last >= interval`. But `StateMachine` uses
`std::chrono::steady_clock` while `MachineStateContext::getStateElapsedTimeMs()`
uses `millis()` — **two clocks in one subsystem** (**repo-verified**).

No `delay()` in the loop path. `yield()` is used during hardware init to avoid
watchdog trips. Long blocking work (Wi-Fi portal, OTA flash) is handled by
`Watchdog::suspend()`/`resume()` rather than by chunking.

### 3.5 Concurrency model

Exactly two contexts the firmware creates itself: the Arduino `loopTask` and the
10 ms heater ISR. Plus whatever AsyncTCP spawns internally (pinned to core 1,
priority 10, 4 KB stack, task-WDT disabled, via the `CONFIG_ASYNC_TCP_*` flags).

**No `xTaskCreate`, no queues, no semaphores, no `portMUX`, no critical
sections, no `esp_timer`** anywhere in `src/` or `include/` (**repo-verified**).
ISR↔loop sharing is via a mix of `std::atomic` debug counters, a non-atomic
`static SystemContext*`, and plain fields reached through `SystemContext`
accessors. The `double pidOutput` the ISR reads is a **non-atomic 8-byte read
racing with the loop's write** — a torn-read hazard on the heater duty cycle.

This is a tractable model to port, and the single most important thing to
redesign.

---

## 4. Peripherals

### 4.1 Temperature

Runtime-selected by config, both on GPIO 16 (**repo-verified**):

- **TSIC 306** via `ZACwire` 2.0.0 (MIT, vendored copy present). Interrupt-driven
  decode inside the library; reads are effectively non-blocking. Filtering is a
  two-stage change-rate gate (200 initially, then 5), sentinels 222 = read failed
  and 221 = not connected, plus a hard reject outside 0–180 °C so a glitch cannot
  trip emergency stop.
- **DS18B20** via OneWire 2.3.8 + DallasTemperature 4.0.6, 11-bit resolution,
  `setWaitForConversion(false)` → non-blocking, conversion kicked off in the
  constructor.

Shared base `TempSensor`: 10 consecutive failures before `error_` latches; an
async `startRead()`/`tryGetValue()` path with a 1000 ms timeout that **seeds with
the last good reading** because TSIC stabilisation depends on the previous sample.

Two bugs not to replicate (**repo-verified**): the DS18B20 comment reasons about
10-bit/188 ms while the code sets 11-bit/~380 ms against a 400 ms cadence; and
`update_moving_average()` seeds `std::accumulate` with an `int`, truncating a
double sum, and divides by time deltas with no zero guard.

### 4.2 Pressure

**Not an ADC.** It is a Honeywell **ABP2LANT010BG2A3XX** on I²C at `0x28`: write
`{0xAA,0x00,0x00}`, **`delay(10)` — a blocking 10 ms per read** — then read
7 bytes; 24-bit pressure in bytes 1–3, 24-bit temperature in bytes 4–6
(**repo-verified**). State is a pile of file-scope `inline` globals, so only one
instance is possible. The status byte is computed and **never used**, and
`Wire.read()` failures silently yield 0/−1 — a NAK produces 0xFF-filled counts
≈ 10 bar. Sampled at 50 ms, then low-pass filtered `y = 0.3x + 0.7y⁻¹`.

The pressure transfer function is correct per the datasheet. **The temperature
transfer function is wrong**: the code uses `counts * 270 / 16777215 − 40` where
the datasheet gives `counts * 200 / 16777215 − 50`. Impact is contained — that
value only appears in a TRACE log line — but do not carry the constants across.

### 4.3 Display

One 128×64 mono OLED on the shared I²C bus. Device chosen at runtime from config:
`U8G2_SH1106_128X64_NONAME_F_HW_I2C` or `U8G2_SSD1306_128X64_NONAME_F_HW_I2C`,
`U8G2_R0`, address `0x3C` or `0x3D` (**repo-verified**). Full-framebuffer mode →
1 KB RAM buffer. Refresh 10 Hz.

Almost all display code in this repo is **rendering logic**, not driver: one
manager for U8g2 RAII plus a CRTP template pipeline with 6 templates, widgets,
layout helpers, bitmaps and EN/DE/ES strings. `docs/display-modern-layout.md`
hardcodes U8g2 font box heights (`u8g2_font_fub20_tf` = 23 px,
`u8g2_font_profont17_tf` = 15 px) and relies on `setFontPosTop()` **combined with**
`setFontRefHeightExtendedText()`. That combination is the font-parity spec, and it
is where the port's pixel risk lives (see the compatibility matrix).

Display dimensions are duplicated in `defaults.h` and `Timing.h`; the source
comment admits the duplication was to dodge a macro collision.

### 4.4 Scale — dead code

**Both scale implementations exist and neither is ever instantiated.**
`HX711Scale` and `BluetoothScale` are constructed nowhere in the tree;
`HardwareManager::getScale()` returns `nullptr` with a TODO, `getWeight()` returns
`0.0`, `tareScale()` is empty (**repo-verified**). `main.cpp` claims scale setup
happens there and does nothing; `initializeSensors()` claims it happens in
`main.cpp`. So **brew-by-weight cannot work as shipped**, even though the config
parameters, the MQTT topics and the `/api/scale/*` routes all exist.

This is the largest single surprise in the inventory and it substantially reduces
the porting surface: HX711 and the Acaia BLE protocol are not on the critical path.

### 4.5 Actuators

`GPIOPin` is the **single** HAL seam — every `digitalWrite`/`digitalRead`/
`analogRead`/`pinMode` in the firmware goes through `src/hardware/GPIOPin.cpp`
(**repo-verified**). Excellent news for the port. Its `write()` silently no-ops
unless the pin type is `OUT`, which is a silent-failure footgun.

Three relays: heater (GPIO 2), pump (GPIO 27), valve (GPIO 17). Active level per
relay from config (`LOW_TRIGGER` / `HIGH_TRIGGER`). All three are `off()` at
construction. `Relay::on()/off()` is one `digitalWrite` and is documented ISR-safe.

Steam and water valves **share one physical relay**, arbitrated by a
`ValveState { CLOSED, STEAM_OPEN, WATER_OPEN, BOTH_OPEN }` enum.

Stubs with TODOs, i.e. no real modulation exists: `setHeaterPower(percentage)`
degenerates to on/off, `setPumpPressure(bar)` likewise, and
`openSolenoid()`/`closeSolenoid()` only flip a bool with **no GPIO at all**.

No LEDC, no `analogWrite`, no RMT, no DAC, and no pin configured as analog
anywhere (**repo-verified**).

### 4.6 Inputs

All polled and software-debounced. **There is no `attachInterrupt` anywhere in the
repo** — the 10 ms heater timer is the only interrupt (**repo-verified**).

`IOSwitch` debounce is 20 ms, long-press 500 ms, `MOMENTARY` or `TOGGLE`,
`NORMALLY_OPEN` or `NORMALLY_CLOSED` with the NC inversion applied after the
debounce window. Long-press release-clearing depends on an exact
`lastStateChangeTime == currentTime` time-point equality, which is fragile.

The power-switch type decides the boot state: `MOMENTARY` boots into
`PID_NORMAL`; `TOGGLE` reads the switch and boots `PID_NORMAL` or `PID_DISABLED`.

The water-tank sensor's `initialState` polarity is **inverted relative to the
other four switches** (**needs confirmation** — one of the two is probably wrong).

---

## 5. Control: state machine, PID, safety

### 5.1 States

18 states with explicit **non-contiguous** numeric values, and the gaps are
load-bearing because the category predicates are range checks:
`isBrewState` = 31..34, `isBackflushState` = 60..63, and `updateLEDs()` uses
`state <= BACKFLUSH_FINISHED` (≤ 63) as "status-LED-eligible" (**repo-verified**).
A port must preserve either the numbers or replace the ranges with explicit sets.

`INIT=0`, `PID_NORMAL=20`, `BREW_PREINFUSION=31`, `BREW_PREINFUSION_PAUSE=32`,
`BREW_RUNNING=33`, `BREW_FINISHED=34`, `MANUAL_FLUSH_RUNNING=36`,
`STEAM_RUNNING=51`, `BACKFLUSH_IDLE=60`, `BACKFLUSH_FILLING=61`,
`BACKFLUSH_FLUSHING=62`, `BACKFLUSH_FINISHED=63`, `WATER_TANK_EMPTY=70`,
`EMERGENCY_STOP=80`, `PID_DISABLED=90`, `STANDBY=95`, `SENSOR_ERROR=100`,
`EEPROM_ERROR=110`.

### 5.2 Engine

One `unique_ptr<MachineState>`; **every transition heap-allocates a new state
object** and destroys the old one, so state-local data does not survive a
transition. `update()` runs the current state then checks transitions, and
performs **at most one transition per loop iteration** — multi-hop chains take one
iteration per hop. If creating the new state fails, it logs an error and stays put
without calling `onExit`. Initialisation failure falls back to `INIT`, and if that
also fails, `ESP.restart()` (**repo-verified**).

### 5.3 Pre-emptive safety transitions

`BaseState::checkTransitions` evaluates these **before** any state-specific logic,
in this order (**repo-verified**):

1. `isEmergencyStop()` → `EMERGENCY_STOP`, from every state, no exclusions.
2. `hasSensorError()` → `SENSOR_ERROR`, from every state.
3. `!isWaterTankFull()` → `WATER_TANK_EMPTY`, excluding `WATER_TANK_EMPTY` and `STANDBY`.
4. `!isPidRuntimeEnabled()` → `PID_DISABLED`, with a longer exclusion list.
5. Delegate to the state's own `checkSpecificTransitions`.

This chain is the contract that makes every state fail safe, and it is the single
most important thing to reproduce exactly.

### 5.4 Which states drive which actuator

- **Heater** — driven *exclusively* by the timer ISR from `processPidOutput`. The
  state machine influences it only indirectly, via
  `ProcessController::shouldPIDBeEnabled()`.
- **Pump** — `BREW_PREINFUSION`, `BREW_RUNNING`, `MANUAL_FLUSH_RUNNING`,
  `BACKFLUSH_FILLING`; plus **inline in `PID_NORMAL`** and in `STEAM_RUNNING`
  while the hot-water switch is held. There is no separate hot-water state.
- **Valve** — `BREW_PREINFUSION`, `BREW_PREINFUSION_PAUSE` (deliberately held open
  to keep puck pressure), `BREW_RUNNING`, `MANUAL_FLUSH_RUNNING`,
  `BACKFLUSH_FILLING`. Closed in `BACKFLUSH_FLUSHING` so back-pressure flushes to
  the drip tray.
- **LEDs** — not state-machine-driven; set in `LoopManager::updateLEDs()`.

### 5.5 Heater PWM — how the heater is really driven

Software slow-PWM from a hardware timer ISR. Not the loop, not LEDC
(**repo-verified**, `include/clevercoffee/isr.h`):

- Timer group 0, prescaler 80 on the 80 MHz APB clock → 1 µs tick; alarm every
  **10 000 µs**, autoreload (and the ISR redundantly re-arms it every tick).
- Per tick: bail if the context pointer is null, bail if `!isISRReady()`, bail if
  the timer pointer is null or `< 0x1000` (a hand-rolled bogus-pointer guard),
  then `if (pidOutput <= isrCounter) heater->off(); else heater->on();`, then
  advance the counter by 10 and wrap at `processWindowSize()`.
- Window = 1000 ms → **1 Hz PWM with 10 ms resolution = 100 duty steps**.
  `pidOutput` is in milliseconds-on per 1000 ms window, which is why the web layer
  divides by 10 to get a percentage.

`onTimer()` is `static inline IRAM_ATTR` **defined in the header**, so every
translation unit that includes `isr.h` gets its own copy and only the one used by
`initTimer1()` is attached.

**The ISR bypasses `HardwareManager` entirely.** `heaterEnabled_` is therefore only
an approximation while the ISR runs, and the source says so in two places.

### 5.6 PID

Vendored `lib/Arduino-PID-Library` — Brett Beauregard `PID_v1` 1.2.1, **with no
license declaration in either `library.json` or `library.properties`**
(**needs confirmation**; upstream is MIT but that is not verifiable from this
checkout). It is **not stock**: it adds `SetIntegratorLimits`, `SetSmoothingFactor`,
conditional integration when saturated, an EWMA input filter used for the
derivative in `P_ON_E` mode only, and double anti-windup.

Constructed once, bound by pointer to `processTemperature`/`processPidOutput`/
`processSetpoint`, `DIRECT`, `SetSampleTime(1000)` — so it recomputes **at most
once per second** even though `computePID()` is called every iteration.

Tuning sets are selected **only when the machine state changes**:
`PID_NORMAL` → regular gains (optionally `P_ON_M`); `STEAM_RUNNING` → **pure P**
(`steamKp`, 0, 0) with the previous state's integrator limit left in place; brew
states → brew-detection gains if enabled, else regular; everything else → regular.

The temperature fed to the PID is the sensor value **minus `brewTempOffset`**
unless steam mode is active — and the same offset-corrected value is what the
emergency check sees.

### 5.7 Heater interlocks, and the gap in them

`shouldPIDBeEnabled()` returns false — and the controller then forces
`pidOutput = 0` and calls `disableHeater()` — for `PID_DISABLED`, `SENSOR_ERROR`,
`EMERGENCY_STOP`, `EEPROM_ERROR`, `STANDBY`, any backflush state (60..63,
**including `BACKFLUSH_IDLE`**), `isProcessBrewPidDisabled()`, and
`WATER_TANK_EMPTY` unless `keepHeaterOnEmpty` is set. `HardwareManager` refuses
`enableHeater()` while in emergency mode, and `enablePump()` while in emergency
mode or with an empty tank. Output is clamped to 0–1000 and the integrator to
0–`aggIMax`.

**The gap (repo-verified).** The ISR consults *only* `processPidOutput`. It does
not look at `emergencyMode_`, `heaterEnabled_`, tank state or machine state. So
every heater interlock in the system works by forcing `pidOutput` to zero — and in
`updateProcessControl()` the order is `computePID()` (which writes a fresh,
possibly large output) **then** `updatePIDState()` (which zeroes it). That leaves a
window of up to a few milliseconds in which the ISR can drive the heater from a
live duty cycle while the machine is in `EMERGENCY_STOP`, `SENSOR_ERROR` or
`STANDBY`. Combined with the non-atomic 8-byte `pidOutput` read (§3.5), this is
the top safety item for the rewrite.

### 5.8 Emergency stop

`EmergencyStopManager`, evaluated once per loop iteration:

- **Undebounced:** temperature `< 0.0` or `> 200.0` triggers immediately.
  Comparisons are strict, so exactly `0.0` — a plausible "sensor not yet read"
  value — counts as **valid**.
- **Debounced:** temperature above `config.emergencyStopTemp` increments a counter;
  the counter resets only below `emergencyStopTemp - emergencyStopHysteresis`.
  The debounce counts **loop iterations, not time**.
- Clearing requires a valid reading **and** `temp <= 100.0 °C`.

`Temperature::EMERGENCY_THRESHOLD_C (145)`, `EMERGENCY_RESET_THRESHOLD_C (120)`
and `HYSTERESIS_C (10)` are **not referenced** by the manager, which uses config
values instead — dead constants, and the 120 °C reset threshold is misleading
against the real 100 °C clear.

**`clearEmergencyMode()` asymmetry (repo-verified):** `EmergencyStopState::onExit`
does **not** call it; only `ProcessController::testEmergencyConditions()` does.
Depending on which path wins, `HardwareManager::emergencyMode_` can stay latched
after the state machine has left `EMERGENCY_STOP`, silently turning every
subsequent `enableHeater`/`enablePump`/`openWaterValve` into a no-op.

### 5.9 Watchdog

ESP32 Task Watchdog, 5000 ms, `panic_on_trigger = true`. `begin()` uses
`enableLoopWDT()` rather than `esp_task_wdt_add()` — the source explains that
adding the task directly desynchronises the core's `loopTaskWDTEnabled` flag and
leaves `suspend()` unable to disarm the WDT, "blocking OTA → panic".
`suspend()`/`resume()` bracket the Wi-Fi portal and OTA. The timeout is converted
with integer seconds, so any non-multiple of 1000 would be silently truncated.
**The only feed site is `loop()`**; `setup()` runs armed and unfed.

### 5.10 Fault and shutdown behaviour

`emergencyShutdown()` latches `emergencyMode_` and calls `disableAllHardware()`.
`safeHardwareShutdown()` does the same **without** latching. `safeShutdown()` is a
third near-duplicate that also turns the LEDs off.

**`disableAllHardware()` only pokes a relay whose tracked flag says it is on** —
and the heater's flag is documented as unreliable while the ISR runs. A defensive
shutdown would write `off()` unconditionally. Emergency protection therefore rests
on `pidOutput = 0`, not on this call (**repo-verified**).

Power-on hazards for the rewrite (**repo-verified**):

1. `pinMode(OUTPUT)` leaves an ESP32 pin driving **LOW**, and for a `LOW_TRIGGER`
   relay `off()` writes **HIGH** — so between `pinMode()` and `off()` a low-trigger
   heater, pump or valve relay is **momentarily energised**. The gap spans three
   GPIO constructions plus a log call.
2. `HardwareManager` is created late (init step 8), so until then the relay pins
   sit in their post-reset state and the relay board decides. **There is no early
   "safe the outputs first" step.**
3. Nothing runs a shutdown handler on reset. Both `ESP.restart()` and `exit(0)`
   leave relay GPIOs to the SoC's reset behaviour.

### 5.11 Missing timeouts

- Brew: automatic mode stops on time or weight. **Manual mode has no maximum brew
  duration** — pump and valve stay energised until the switch is released or a
  pre-emptive transition fires.
- **Steam has no timeout at all**, and no steam over-temperature protection beyond
  the global emergency threshold.
- `MANUAL_FLUSH_RUNNING` has no timeout.
- `Timing.h` defines `STEAM_STOPPED_DISPLAY_TIMEOUT_MS` and
  `HOT_WATER_STOPPED_DISPLAY_TIMEOUT_MS`, but no such states exist — dead
  constants from an earlier design.

### 5.12 Sensor-error semantics

`SensorCoordinator::getTemperature()` returns the **last successfully read value,
held indefinitely** on failure. So the 0–200 °C validity check will not fire on a
dead sensor unless the cached value is itself out of range; the `SENSOR_ERROR`
state is the actual protection.

`hasSensorError()` is `temperatureError || scaleError`, so **a scale fault takes
the whole machine into `SENSOR_ERROR` and the heater off** — even though the scale
is never instantiated. Pressure and water-tank errors are *not* part of it
(**repo-verified**).

There is exactly one temperature sanity check (the 0–200 °C window). No
rate-of-change check, no stuck-value detection, no plausibility check against
heater duty.

### 5.13 Error-handling style

Five coexisting styles (**repo-verified**): boolean returns with early abort (the
dominant one, labelled "Traditional boolean error handling" in `main.cpp`);
`try/catch` flattened to a bool at each init phase boundary; log-and-continue;
the state machine as the error channel (the only mechanism with real recovery
semantics); and an `include/clevercoffee/errors/` directory with `ErrorCode`,
`Error` and an `Expected<T>` that **appear in none of the core control-flow
files** — an in-progress migration.

`Resilience.h` provides `RetryPolicy` and `CircuitBreaker`. They are used by the
Wi-Fi and MQTT managers but by nothing in the control path. `CircuitBreaker`'s
`halfOpenAttempts_` is read but never incremented, so its "one probe at a time"
limit does not actually work.

---

## 6. Network, storage, config

### 6.1 Wi-Fi

Two credential sources in strict order (**repo-verified**):

1. `Config.systemWifiSsid` / `systemWifiPassword` from **NVS namespace `config`**.
   If the SSID is non-empty, connect with a 10 s busy-wait and, on failure, go
   offline — **the captive portal is deliberately never started** in this case.
2. Only if the SSID is empty: `WiFiManager::autoConnect`, using **WiFiManager's own
   NVS blob**, not the `config` namespace. On failure, an AP portal with a 60 s
   timeout; on save, the device reboots.

Portal AP SSID = hostname, password `CleverCoffee`. Default hostname `silvia`.
Reconnect uses a `RetryPolicy` (10 s → 300 s, ×2, 5 attempts) plus a
`CircuitBreaker` (5 failures, 60 s open, 30 s half-open), and
`disableLoopWDT()`/`enableLoopWDT()` bracket the blocking connect.

**Known wart (repo-verified):** `checkAndMaintainConnection` checks `WiFi.status()`
immediately after the asynchronous `WiFi.begin()`, which almost always reports
not-connected, so every attempt is recorded as a circuit-breaker failure even when
the connection later succeeds.

**No mDNS code exists** anywhere in `src/`/`include/`. The only responder is the
one `ArduinoOTA.begin()` sets up internally, which is why `silvia.local` works for
espota and telnet while the web UI is not discoverable.

### 6.2 Web server

ESPAsyncWebServer on port 80. **Every handler runs on the AsyncTCP task: core 1,
priority 10, 4 KB stack, task-WDT disabled.**

Middleware: CORS with origin `*`, and HTTP Basic auth **only if** `system.auth.enabled`
and both credentials are non-empty. **Default state: auth disabled**, so by default
every endpoint below — including firmware upload and factory reset — is
unauthenticated on the LAN, behind wildcard CORS.

Routes (**repo-verified**): `GET /events` (SSE), `GET /api/status`,
`GET /api/config`, `POST /api/setpoint`, `GET /api/health`, `POST /api/wake`,
`POST /api/sleep`, `POST /api/steam`, `POST /api/pid`, `POST /api/backflush`,
`POST /api/maintenance/reset-backflush-counter`, `POST /api/scale/tare`,
`POST /api/scale/calibration` (the last two registered only if the scale was
enabled at boot), `GET /api/parameter-help`, `GET /api/temperatures`,
`GET /api/history` (600-point ring), `GET /api/nvs-debug`, `POST /api/wifi-reset`,
`GET /api/config/download`, `POST /api/config/upload` (16 KB cap),
`POST /api/restart`, `POST /api/factory-reset`, `GET|POST /api/parameters`,
`POST /api/ota/firmware`, `POST /api/ota/filesystem`, `POST /api/ota/url`,
`GET /api/ota/status`, `GET /` → `/ui/`, the `/ui` SPA tree, and a JSON/SPA-aware
404.

`/api/steam`, `/api/pid` and `/api/backflush` are **toggles that ignore the request
body** — callers cannot set an absolute state.

Several handlers block inside the async callback: `delay(1000)` in
`/api/wifi-reset`, `delay(100)` in `/api/restart` and `/api/factory-reset`, and a
blocking `prefs.clear()` flash erase in `/api/factory-reset`. `CONFIG_ASYNC_TCP_USE_WDT=0`
hides the symptom. `/api/ota/url` queues its work for the main loop instead —
so the codebase knows the correct pattern and breaks it elsewhere.

OpenAPI mismatches against `docs/api/openapi.yaml` (**repo-verified**):
`POST /api/config` is specified but **not implemented** (404);
`/api/wake`, `/api/sleep` and `/events` are implemented but undocumented;
`servers: http://clevercoffee.local` cannot resolve (no mDNS, default hostname
`silvia`); and the optional Basic auth is undocumented.

### 6.3 Frontend

React 19 + Vite + Tailwind 4 in `ui/packages/frontend`. `build_frontend.py` runs
only on the `buildfs` target and shells out to `pnpm prepare-esp`, which builds and
moves the output to `data/ui` — the LittleFS image root. `rollup-plugin-gzip`
emits `.gz` siblings and a `postbuild` step **deletes the plaintext
`.html`/`.js`/`.css`**, so the UI is served **gzip-only from LittleFS**, and the
server's uncompressed fallback is dead for those types. `data/` is gitignored and
absent from a clean checkout, so a fresh build has no filesystem image until
`pio run -t buildfs`.

A second, dead pipeline still compiles in: `auto_compression.py` plus the whole
`#if FRONTEND_PREPROCESSING` branch with templated HTML and `/html_fragments`.

**640 KB is the hard ceiling for the gzipped bundle.**

### 6.4 MQTT

PubSubClient, **no TLS** (plain `WiFiClient`). Topic scheme
`<prefix><hostname>/<leaf>`, e.g. `custom/kitchen/silvia/temperature`. The default
prefix already ends in `/` and the code does not normalise it, so the scheme
depends on the user keeping that slash.

LWT `<prefix><hostname>/status` = `offline`, retained; `online` published each
cycle. **One wildcard subscription:** `<prefix><hostname>/+/set`.

Published: ~26 parameter mirror topics (conditional on brew-switch and scale
config), 9–13 numeric sensors, and one binary sensor `waterTankFull` (`ON`/`OFF`).
Home Assistant discovery publishes retained configs for sensor, binary_sensor,
switch, button and number components, chunked when over 128 bytes.

**Retain policy is inconsistent**: parameters and binary sensors are retained,
numeric sensors are not.

Inbound: every published leaf is settable via `<leaf>/set`. `messageCallback`
coerces **all** payloads through `sscanf("%lf")`, so string-typed parameters
(broker, hostname, credentials) cannot be set over MQTT, and the `sscanf` return
value is unchecked, leaving the target uninitialised on garbage input.

The publish loop is a 3-phase resumable cursor with a time budget and
change-only publishing. Reconnect uses the same RetryPolicy + CircuitBreaker pair
as Wi-Fi, and is deliberately **suppressed during an active brew**. After 5 failed
attempts it stops trying until the breaker half-opens; there is no periodic full
reset.

`findConfigParameter` is a linear scan over ~97 parameters comparing Arduino
`String`s, called for every parameter on every publish cycle.

### 6.5 OTA

Three mechanisms (**repo-verified**):

1. **ArduinoOTA / espota**, password-authenticated, default `otapass`. Also the
   device's only mDNS name.
2. **Web multipart upload** — `POST /api/ota/firmware` and `/api/ota/filesystem`,
   validating the `.bin` extension, rejecting concurrency with 409, streaming into
   `Update.write()`. The filesystem image is written to the partition **labelled
   `spiffs`**.
3. **Pull-from-URL** — `POST /api/ota/url` validates, *queues*, and returns
   immediately, so the blocking download runs on the main loop rather than the
   AsyncTCP task.

`runMainLoopTick` deliberately does not call `ArduinoOTA.handle()` during an
active session, to stop an espota invitation starting a second `Update`.

**None of the web OTA paths have their own auth** — they inherit the global
middleware, which is off by default.

### 6.6 Config

`include/clevercoffee/Config.h` (1567 lines). Four class families over
`BaseParamDef`: `ParamDef<T>` (T restricted to `bool, int, double, float, String`),
`EnumParamDef`, and the read-only, non-persisted `StateParamDef<T>` and
`ComputedParamDef`.

Counts (**repo-verified**): 78 `ParamDef` + 20 `EnumParamDef` = **98 declared**,
of which `getAllConfigParams()` registers **96 `&`-lines / 97 unique names**. The
gap is `emergencyStopTemp` and `emergencyStopHysteresis`: **declared but absent
from the registry**, so they never load from NVS, never save, never appear in
`/api/config`, `/api/parameters` or MQTT, and `resetAllToDefaults()` skips them.
That is a real bug on a safety-relevant parameter and must be resolved before
porting. Plus 16 state + 3 computed read-only params — and
`getAllStateParams()` is an **empty stub with its real body commented out**, so all
19 are currently unreachable.

Three storage backends, one authoritative:

1. **NVS / Preferences, namespace `config`** — the live store. Keys are **not** the
   dotted paths: `generateNvsKey()` returns `"p" + fnv1a_hash(key)` in hex to stay
   under the NVS 15-character limit. Consequences: NVS is opaque without the
   firmware, renaming a parameter silently orphans its value, and there is **no
   collision detection** on the 32-bit hash. One reserved plain key, `_seeded`.
2. **LittleFS `/config.json`** — first-boot seed only, gated by `_seeded`.
3. Compiled-in defaults in `defaults.h`.

Separate namespace `maintenance` holds `shots_since_bf`. WiFiManager keeps
credentials in its own NVS area.

`ParamDef::set()` **writes through to NVS on every single set** — open, write one
key, close. So one `POST /api/parameters` with N fields is N flash transactions on
the network task.

Validation is weak: arithmetic types are range-checked, **bools and Strings are
always accepted**, there is **no string length enforcement** despite the
`*_MAX_LENGTH` constants existing (they only feed UI metadata), and `fromString`
uses `String::toInt()`/`toDouble()`, which return 0 on garbage — so `"abc"`
silently becomes `0` and then passes any range that includes 0.

**There is no schema version and no migration path.** Combined with hash-derived
keys, this was the single biggest data-compatibility risk.
**[ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md) removed
the risk by removing the requirement:** the Rust firmware does not read this layout
at all. Migration is manual — the user exports `config.json` from the old web UI and
imports it into the new firmware — so the compatibility surface moves from an opaque
binary NVS layout to a human-readable JSON file. The scheme above is now history
rather than a constraint, and worth reading mainly to understand what the new scheme
deliberately avoids.

`importFromJsonObject` returns true if **≥ 1** parameter updated, so a mostly
garbage upload reports success. **That is the format the Rust import must accept,
and it must be stricter about it** — reporting exactly what it took and what it
rejected, rather than declaring success on one recognised field.

### 6.7 Logging

Singleton `Logger`, levels TRACE..FATAL + SILENT, runtime-settable. **Two sinks,
neither MQTT nor web**: serial at 115200, and **raw TCP on port 23** — a single
client, kicked when a new one connects, with a `# heartbeat` every 30 s. That is
the port `monitor_port = socket://silvia.local:23` targets, and it is
**unauthenticated and unencrypted while streaming DEBUG logs that include MQTT
parameter values**.

Buffering is a 16-entry × 304-byte lock-free ring with atomic occupancy flags;
**overflow drops the message** and never blocks the caller. The *drain* can block:
`Logger::update()` runs on the main loop and does the real `Serial.write` and
`client_.flush()`, at most 8 entries per call. Wi-Fi output is skipped entirely
below 30 000 bytes of free heap.

`messagesLogged` is declared but appears **never to be incremented**. Timestamps
come from `localtime_r(time(nullptr))` and **nothing in this repo configures
SNTP**, so they are epoch-relative unless something else sets the clock.

### 6.8 BLE

Much thinner than `lib_deps` suggests. `NimBLE-Arduino` is declared but has
**zero direct references** in `src/`/`include/` — it is present only as
AcaiaArduinoBLE's transitive dependency. The only BLE code is `BluetoothScale`,
which is never instantiated (§4.4).

**Wi-Fi/BLE coexistence is not handled anywhere**: no `esp_coex_*` calls, no
radio arbitration, no explicit controller start/stop. Since the scale is dead
code, nothing exercises it today (**needs confirmation** that simultaneous
BLE + Wi-Fi + MQTT has ever been tested — no evidence either way in the repo).

---

## 7. Findings that need a decision

These are contradictions, latent bugs and security defaults found while taking
the inventory. They are recorded here because a rewrite must either reproduce them
deliberately or fix them deliberately — silently changing behaviour is the failure
mode to avoid.

### 7.1 Hardware and safety

1. **`PIN_STEAMLED = 1` is UART0 TX.** The source comment says it was "moved from
   pin 1 (UART TX — conflicts with serial logging)" but the value is still `1`.
   Serial logging is active at 115200, so the steam LED and the console fight over
   GPIO1. **repo-verified.**
2. **`PIN_HEATER = 2` is an ESP32 boot-strapping pin.** The heater relay sits on a
   strapping pin. **repo-verified.**
3. **Low-trigger relays are briefly energised at boot** between `pinMode(OUTPUT)`
   and `off()`, and the relay pins are undriven until init step 8. **repo-verified.**
4. **The ISR bypasses every heater interlock**, and `computePID()` runs before
   `updatePIDState()` zeroes the output. §5.7. **repo-verified.**
5. **`pidOutput` is a non-atomic 8-byte read in the ISR** racing the loop's write.
   **repo-verified.**
6. **`disableAllHardware()` skips relays whose tracked flag says "off"**, and the
   heater's flag is known-unreliable. **repo-verified.**
7. **`clearEmergencyMode()` asymmetry** can leave emergency mode latched after the
   state machine has left `EMERGENCY_STOP`. §5.8. **repo-verified.**
8. **No manual-brew, steam or manual-flush timeout.** §5.11. **repo-verified.**
9. **A scale fault forces `SENSOR_ERROR`** and kills the heater, via an OR over
   temperature and scale errors. **repo-verified.**
10. **`emergencyStopTemp` / `emergencyStopHysteresis` are not in the config
    registry** — unpersisted, unexported, unresettable. §6.6. **repo-verified.**
11. **Water-tank switch `initialState` polarity is inverted** relative to the four
    config switches. **needs confirmation.**
12. **Exactly `0.0 °C` counts as a valid temperature** in the emergency check.
    **repo-verified.**
13. **The emergency debounce counts loop iterations, not time.** **repo-verified.**

### 7.2 Correctness

14. **`calculateDerivedValues()` overwrites the regular PID gains with the
    brew-detection gains** — it sets `aggKi`/`aggKd` from the regular set and then
    immediately again from the BD set. Masked at runtime because
    `updatePIDState()` reapplies proper tunings on the first state change, but the
    window between ISR enable and that first change uses mixed gains. Looks like a
    bug, not a design. **repo-verified.**
15. **`P_ON_M` / `P_ON_E` are inverted between the firmware's `PID_v1.h`
    (`P_ON_M 0`, `P_ON_E 1`) and the host-test stub `test/PID_v1.h`
    (`P_ON_M 1`, `P_ON_E 0`).** Any existing host test that passes a mode constant
    exercises the opposite branch from the firmware. Must be fixed before those
    tests can serve as the port's reference oracle. **repo-verified.**
16. **PID `Compute()` does integer division `SampleTime / 1000`.** Harmless only
    because `windowSize_` is exactly 1000; any value below 1000 divides by zero.
    **repo-verified.**
17. **The ABP2 temperature transfer function is wrong** (§4.2). **repo-verified.**
18. **`initializePID()` uses `{:.3f}` std::format placeholders in a printf-style
    `LOGF`.** **needs confirmation** — depends on the `LOGF` definition.
19. **`main.cpp` shadows its own globals**: `displayManager` and `hardwareManager`
    are file-scope `unique_ptr`s, re-declared as `auto&` locals in `setup()`, so the
    file-scope ones stay null forever. `Resilience.h` is also included twice.
    **repo-verified.**
20. **`EEPROM_ERROR` has no producer** — nothing transitions into it.
    **repo-verified.**
21. **Two clocks in the state machine** (`steady_clock` vs `millis()`).
    **repo-verified.**
22. **Wi-Fi reconnect over-counts circuit-breaker failures** (§6.1).
    **repo-verified.**
23. **MQTT coerces all inbound payloads to `double`** with an unchecked `sscanf`
    (§6.4). **repo-verified.**
24. **The scale subsystem is complete but never instantiated** (§4.4).
    **repo-verified.**
25. **Sensor timer intervals are vestigial** (§3.3). **repo-verified.**

### 7.3 Security defaults

26. **Web auth is off by default** with **wildcard CORS**, on endpoints that flash
    firmware and factory-reset. **repo-verified.**
27. **Hardcoded credentials committed to the repo**: OTA `otapass` (also in
    `platformio.ini:70` as `--auth=otapass`), web auth `admin`/`admin`, MQTT
    `rancilio`/`silvia`, AP portal password `CleverCoffee`. **repo-verified.**
28. **The port-23 log stream is unauthenticated and unencrypted** and carries
    parameter values. **repo-verified.**
29. **No TLS anywhere** — `ASYNC_TCP_SSL_ENABLED=0`, plain `WiFiClient` for MQTT.
    **repo-verified.**
30. `.env` in this working tree holds a real Wi-Fi SSID and password. Gitignored,
    but it must stay out of logs, output and commits. **repo-verified.**

### 7.4 Build, test and docs

31. **`extra_configs` points at a non-existent file** (§2.4). **repo-verified.**
32. **`.pioignore` disables ~460 lines of real tests** (§2.5). **repo-verified.**
33. **Three clang-format versions** (§2.4). **repo-verified.**
34. **`-std=gnu++2a` is commented "use C++23"** — `gnu++2a` is C++20, matching
    `.clang-format`'s `Standard: c++20`. The comment is wrong, not the flag.
    **repo-verified.**
35. **`build_frontend.py` is registered `pre:` but gates on `buildfs`**, so
    `main.yml`'s firmware job never builds or validates the frontend-to-LittleFS
    path; only `release.yml` and the `wokwi` target do. **repo-verified.**
36. **`README.md`'s merge command omits `littlefs.bin`** while `release.yml`
    includes it — following the README leaves the device with no web UI.
    **repo-verified.**
37. **`CONTRIBUTING.md` says target the `develop` branch**, but every workflow
    triggers only on `main` and pre-commit blocks commits to `main`. No `develop`
    branch is referenced in CI. **repo-verified.**
38. **`DEBUG_GUIDE.md` contradicts itself on ISR rate** — "~50 per second" in one
    place, "~100 per second (10 ms timer)" in another. The latter is right.
    **repo-verified.**
39. **`src/main.cpp` includes `<os.h>`** with no symbol from it used anywhere — a
    stale, ESP8266-era include. **repo-verified.**
40. **`docs/plan/task-list.md` cites four documents that do not exist** in the repo
    and is dated 2026-03-28 with a stale test count. Superseded by
    [task-list.md](task-list.md). **repo-verified.**
41. **Library licences are largely unverifiable from this checkout.** Only ZACwire
    is confirmed (MIT, vendored `LICENSE`); the vendored PID library declares
    **none**. `ESPAsyncWebServer`/`AsyncTCP` are believed **LGPL-3.0**, which is a
    real wrinkle for statically linked firmware and one the rewrite removes.
    **needs confirmation** for the other 11.
42. **Two parallel plan directories** exist, `docs/plan/` and `docs/plans/`, and
    ADR 0003 uses `# ADR-0003:` where 0001 and 0002 use `# ADR NNNN:`.
    **repo-verified.**

---

## 8. Feature-to-source map

| Feature | Source | Hardware dependency |
|---|---|---|
| Startup / bring-up | `src/main.cpp`, `src/core/SystemInitializer.cpp` | all |
| Main loop scheduling | `src/core/LoopManager.cpp` | — |
| State machine | `include/clevercoffee/state/`, `src/state/`, `src/state/states/` | relays |
| PID control | `src/control/ProcessController.cpp`, `lib/Arduino-PID-Library` | heater relay |
| Heater PWM | `include/clevercoffee/isr.h`, `src/isr.cpp` | timer group 0, GPIO 2 |
| Emergency stop | `src/control/EmergencyStopManager.cpp` | temp sensor, all relays |
| Watchdog | `include/clevercoffee/utils/Resilience.h` | `esp_task_wdt` |
| Temperature | `src/hardware/tempsensors/` | GPIO 16 |
| Pressure | `include/clevercoffee/hardware/pressureSensor.h` | I²C 0x28 |
| Water tank | `SystemInitializer::createWaterTankSensor`, `SensorCoordinator` | GPIO 23 |
| Switches | `include/clevercoffee/hardware/{Switch,IOSwitch}.h` | GPIO 34–36, 39 |
| Relays / LEDs | `src/hardware/{Relay,GPIOPin,StandardLED}.cpp`, `HardwareManager.cpp` | GPIO 1, 2, 17, 19, 26, 27 |
| Display | `include/clevercoffee/display/`, `src/display/` | I²C 0x3C/0x3D |
| Scale (**dead**) | `src/hardware/scales/` | GPIO 25, 32, 33 / BLE |
| Wi-Fi | `src/network/CleverCoffeeWiFiManager.cpp` | radio |
| Web server | `src/network/WebServerManager.cpp` | radio, LittleFS |
| Frontend | `ui/`, `scripts/build_frontend.py` | `spiffs` partition |
| MQTT | `src/network/MQTTManager.cpp` | radio |
| OTA | `src/ota.cpp` | `app0`/`app1`, `otadata`, `spiffs` |
| Config | `include/clevercoffee/Config.h`, `src/Config.cpp`, `src/ConfigJson.cpp` | NVS, LittleFS |
| Logging | `include/clevercoffee/Logger.h`, `src/Logger.cpp` | UART0, TCP 23 |
| Maintenance / backflush counter | `src/coordinators/MaintenanceCoordinator.cpp` | NVS `maintenance` |

---

## 9. Documented workflows today

Verbatim, from the repo (**repo-verified**):

```bash
# build / upload / monitor
pio run -e esp32_usb
~/.platformio/penv/bin/pio run -e esp32_usb --target upload
screen /dev/ttyUSB0 115200
telnet esp32.local 23            # the port-23 log stream

# host tests
pio test -e native_test

# formatting
pio run -t check-format          # CI gate
pio run -t format

# simulator
cp docs/example_config.json data/config.json
pio run -e esp32_usb -t wokwi

# flashing a merged image (note: omits littlefs.bin)
esptool.py --chip esp32 merge_bin -o merged-flash.bin --flash_mode dio \
  --flash_size 4MB 0x1000 bootloader.bin 0x8000 partitions.bin 0x10000 firmware.bin
```

Provisioning without the captive portal is already possible: put
`system.wifi.ssid` / `system.wifi.password` into `data/config.json`, which seeds
NVS once on first boot. `data/config.json` is gitignored and may contain
credentials.

---

## 10. Existing architecture docs

| Doc | What it asserts | Disposition |
|---|---|---|
| `docs/adr/0001-display-subsystem-architecture.md` | One CRTP render pipeline, no virtual display interface, centralised threshold maths | Keep; the Rust display design follows it |
| `docs/adr/0002-wifi-logging-ota-memory-architecture.md` | Budgets ~320 KB RAM across AsyncWebServer, the telnet logger, OTA and ArduinoJson | **Keep — this is the memory contract the Rust port must also honour** |
| `docs/adr/0003-state-machine-hardware-control-contract.md` | Four hardware-control bugs from the flat→class refactor, fixed as an explicit contract | **Keep — these are the regression traps for any rewrite** |
| `docs/state-machine-architecture.md` | Full transition table and per-state actuator behaviour | Keep as the parity reference |
| `docs/display-architecture.md` | Component/responsibility split for the display stack | Keep |
| `docs/display-analysis.md` | Display hardware facts | Keep |
| `docs/display-modern-layout.md` | Pixel/row map and U8g2 font box heights | **Keep — this is the font-porting spec** |
| `docs/plans/display-refactor-plan.md` | "v3" KISS refactor plan | Likely superseded by ADR 0001 |
| `docs/plan/task-list.md` | Dated 2026-03-28, cites four missing documents | **Stale; superseded by [task-list.md](task-list.md)** |
| `docs/features/backflush-reminder.md` | Advisory shot counter, never blocks brewing | Keep |
| `docs/integration-tests.md` | Manual pre-release checklist | Keep, update counts |
| `docs/api/openapi.yaml` | The HTTP surface a Rust web layer must reproduce | **Keep as the API contract; fix the mismatches in §6.2** |
