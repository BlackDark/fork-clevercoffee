# CleverCoffee — Feature Inventory (pre-migration baseline)

**Status:** Frozen baseline for the Rust migration. Read-only reference.
**Captured:** 2026-09-28, against `main` @ `2006b71`.
**Purpose:** Every feature the current C++ firmware provides, mapped to source files and
hardware. Any Rust implementation that omits a row here is a behavioural regression.

Related documents:
- [02 — Research & compatibility matrix](./02-research-compatibility-matrix.md)
- [03 — Decision record (ADR-0004)](./03-decision-record.md)
- [04 — Target architecture](./04-target-architecture.md)
- [05 — Tooling & developer workflows](./05-tooling-and-workflows.md)
- [06 — Migration task list](./06-migration-task-list.md)
- Existing context: [`../state-machine-architecture.md`](../state-machine-architecture.md),
  [`../display-architecture.md`](../display-architecture.md),
  [`../adr/0003-state-machine-hardware-control-contract.md`](../adr/0003-state-machine-hardware-control-contract.md)

---

## 1. Target hardware — what "ESP32 v4" actually means

**`v4` is a board PCB revision, not a chip variant.** There is exactly one chip in this
project: the **original Espressif ESP32** (Xtensa LX6, dual core, 4 MB flash, no PSRAM).
Nothing in the repository — code, CI, docs, or the full 1875-commit history — references
ESP32-S3, C3, C6, H2, or S2.

| Property | Value | Evidence |
| --- | --- | --- |
| Chip | ESP32 (original) | `platformio.ini:9` `platform = espressif32 @^7.0.1`; `README.md:5` `esptool.py --chip esp32` |
| Board | `az-delivery-devkit-v4` (AZ-Delivery ESP32-DevKitC-V4) | `platformio.ini:12`; `REPOSITORY_SUMMARY.md:43` |
| Board history | `esp32dev` → `nodemcuv2` → `az-delivery-devkit-v4` (commit `0dafb58`) | `git log -p platformio.ini` |
| Framework | Arduino (ESP-IDF 4.4 under Arduino core 2.0.x) | `platformio.ini:36` `framework = arduino` |
| C++ standard | `-std=gnu++2a` | `platformio.ini:28` |
| Filesystem | LittleFS | `platformio.ini:13` |
| Flash | 4 MB, DIO, 40 MHz | `README.md:5`; `.github/workflows/release.yml:125,132` |
| USB | **None.** USB-to-UART bridge (CP210x class) | no `HWCDC`/`TinyUSB`/`usb_serial_jtag` anywhere in `src/`+`include/`; `DEBUG_GUIDE.md:15` uses `/dev/ttyUSB0`; `pinmapping.h:45` moves `PIN_STEAMLED` off GPIO 1 because "UART TX" |
| `MAX_GPIO_PINS` | 40 (compile-time constant) | `pinmapping.h:58` |

> **The `esp32_usb` PlatformIO environment name is a misnomer.** It means "upload over the
> USB-to-UART cable", not "the chip has native USB". The original ESP32 has no USB
> peripheral at all, so **USB device mode (CDC) is impossible on the current hardware** —
> this is decisive for §8 (Wi-Fi provisioning).

### Build environments

| Env | Chip | Transport | Notes |
| --- | --- | --- | --- |
| `esp32_usb` | ESP32 | USB-to-UART (esptool) | primary dev env; `debug_tool = esp-prog` |
| `esp32_ota` | ESP32 | `espota` to `silvia.local` | identical firmware, OTA upload path |
| `native_test` | host | none | GoogleTest, `test_build_src` disabled |

CI (`.github/workflows/main.yml`) builds exactly one firmware target through two envs,
plus `native_test`. `format.yml` runs `pio run --target check-format`. `release.yml` produces
a `merged-flash.bin` for manual flashing at 0x1000/0x8000/0x10000/0x350000 (`README.md:5`, `release.yml:116-119`).

### Partition table — `partitions_4M.csv`

| Name | Type | Offset | Size |
| --- | --- | --- | --- |
| nvs | data/nvs | 0x9000 | 20 KB |
| otadata | data/ota | 0xE000 | 8 KB |
| app0 | app/ota_0 | 0x10000 | 1664 KB |
| app1 | app/ota_1 | 0x1B0000 | 1664 KB |
| spiffs (LittleFS) | data/spiffs | 0x350000 | 640 KB |
| coredump | data/coredump | 0x3F0000 | 64 KB |

Constraints: the Rust app must fit in 1664 KB. Web UI + `/config.json` seed live in the
640 KB `spiffs` partition. A `coredump` partition exists, so TWDT panics are captured —
`esp_task_wdt_init(5, /*panic=*/true)`.

---

## 2. Pin map — `include/clevercoffee/hardware/pinmapping.h`

Unconditional; **no chip-variant `#ifdef` anywhere in the file.** 21 `static_assert`s
validate the map at compile time.

### Inputs

| GPIO | Macro | Role | Line |
| --- | --- | --- | --- |
| 39 | `PIN_POWERSWITCH` | power switch (input-only) | `:17` |
| 34 | `PIN_BREWSWITCH` | brew switch (input-only) | `:18` |
| 35 | `PIN_STEAMSWITCH` | steam switch (input-only) | `:19` |
| 36 | `PIN_WATERSWITCH` | hot-water switch (input-only) | `:20` |
| 4 / 3 / 5 | `PIN_ROTARY_DT` / `_CLK` / `_SW` | rotary encoder — **declared, never used** | `:22-24` |
| 16 | `PIN_TEMPSENSOR` | TSIC-306 (ZACwire) **or** DS18B20 (1-Wire) | `:27` |
| 23 | `PIN_WATERTANKSENSOR` | water tank float switch | `:28` |
| 32 / 25 | `PIN_HXDAT` / `PIN_HXDAT2` | HX711 #1 / #2 data | `:29-30` |
| 33 | `PIN_HXCLK` | HX711 shared clock | `:31` |

### Outputs

| GPIO | Macro | Role | Line |
| --- | --- | --- | --- |
| 17 | `PIN_VALVE` | valve relay (steam **and** water, multiplexed) | `:38` |
| 27 | `PIN_PUMP` | pump relay | `:39` |
| 2 | `PIN_HEATER` | heater relay | `:40` |
| 26 | `PIN_STATUSLED` | status LED | `:43` |
| 19 | `PIN_BREWLED` | brew LED | `:44` |
| 1 | `PIN_STEAMLED` | steam LED (moved off UART0 TX) | `:45` |
| 18 | `PIN_ZC` | dimmer zero-crossing — **declared, never used** | `:48` |

### Bidirectional

| GPIO | Macro | Role | Line |
| --- | --- | --- | --- |
| 22 / 21 | `PIN_I2CSCL` / `PIN_I2CSDA` | I2C0 — OLED + ABP2 pressure sensor | `:53-54` |

GPIOs 6-11 and 16-17 are also flash/PSRAM-strapping on the original ESP32 (per `esp-idf-hal`
docs); this project uses 16 and 17 anyway, which works but is worth knowing for a future
hardware revision.

---

## 3. Feature → source → hardware matrix

| # | Feature | Primary source | Hardware | Migration risk |
| --- | --- | --- | --- | --- |
| F1 | PID temperature control (brew + steam) | `src/control/ProcessController.cpp`, `lib/Arduino-PID-Library` | — | Low (port ~200 lines) |
| F2 | State machine, 18 states | `src/state/StateMachine.cpp`, `src/state/states/*.cpp` | — | Medium (large, must be behaviour-identical) |
| F3 | Emergency stop (overtemp / invalid reading) | `src/control/EmergencyStopManager.cpp` | temp sensor | **High (safety)** |
| F4 | HardwareManager: relays, LEDs, switches, interlocks | `src/hardware/HardwareManager.cpp` | relays, LED, switches | **High (safety)** |
| F5 | Heater 10 ms ISR PWM | `include/clevercoffee/isr.h`, `src/isr.cpp` | GPIO 2 relay | **High (safety)** |
| F6 | Task watchdog (5 s, panic) | `include/clevercoffee/utils/Resilience.h` | — | **High (safety)** |
| F7 | `valveSafetyShutdownCheck` (every loop) | `include/clevercoffee/handlers/BrewHandler.h:105-122` | valve relay | **High (safety)** |
| F8 | Water-tank interlock | `src/hardware/HardwareManager.cpp:546-561` | GPIO 23 | **High (safety)** |
| F9 | Temperature sensor — TSIC-306 / ZACwire | `src/hardware/tempsensors/TempSensorTSIC.cpp` | GPIO 16 | **High (no Rust crate)** |
| F10 | Temperature sensor — DS18B20 / 1-Wire | `src/hardware/tempsensors/TempSensorDallas.cpp` | GPIO 16 | Medium (no mature crate) |
| F11 | Rate-of-change filter, blinking phase | `include/clevercoffee/hardware/tempsensors/TempSensor.h` | — | Low |
| F12 | Pressure sensor — Honeywell ABP2 I2C | `include/clevercoffee/hardware/pressureSensor.h` (header-only) | I2C 0x28 | Medium (10 ms blocking read) |
| F13 | Scale — HX711 ×1 or ×2 | `src/hardware/scales/HX711Scale.cpp` | GPIO 32/25/33 | **Dead code** |
| F14 | Scale — Acaia BLE | `src/hardware/scales/BluetoothScale.cpp` | BLE — the ESP32 **does** have a BT+BLE radio | **Dead code** (never constructed). Technically possible; see below |
| F15 | OLED 128×64 SSD1306/SH1106 over I2C | `src/display/DisplayManager.cpp`, `src/ui/OledDriver.cpp` | I2C 0x3C/0x3D | **High (U8g2 fonts have no Rust equivalent)** |
| F16 | 6 display templates, **10 bitmap fonts** (6 `profont`, 4 `fub`) | `include/clevercoffee/display/templates/*.h`, `DisplayLayoutUtils.h` | OLED | **High** |
| F17 | Display localization | `include/clevercoffee/display/languages.h` | OLED | Low |
| F18 | 4 debounced switches + long-press | `src/hardware/IOSwitch.cpp` | GPIO 39/34/35/36 | Low |
| F19 | 3 LEDs | `src/hardware/StandardLED.cpp` | GPIO 26/19/1 | Low |
| F20 | Wi-Fi STA + hostname | `src/network/CleverCoffeeWiFiManager.cpp` | radio | Medium |
| F21 | Wi-Fi captive portal (tzapu WiFiManager) | same file `:112-127` | radio | Medium |
| F22 | MQTT v3.1.1 + HA discovery | `src/network/MQTTManager.cpp` (932 lines) | radio | Medium |
| F23 | Web server, 24 route registrations (20 `/api/*` + redirect + `/ui`), 6 `serveStatic` mounts, 1 SSE stream | `src/network/WebServerManager.cpp` (1208 lines) | radio | Medium |
| F24 | SSE `/events` | `WebServerManager.cpp` + `LoopManager.cpp:523-557` | radio | **Medium-High (SSE on esp-idf is awkward)** |
| F25 | React SPA served from LittleFS | `ui/` → `build_frontend.py` → `spiffs` | radio | Low (re-embed) |
| F26 | OTA — espota + HTTP upload + URL | `src/ota.cpp` (868 lines) | flash | Medium |
| F27 | NVS config, **96 registered params** (133 declared, 37 unregistered) | `src/Config.cpp`, `src/ConfigJson.cpp` | NVS | Medium |
| F28 | First-boot `/config.json` seed | `src/Config.cpp:273-323` | LittleFS | Low |
| F29 | Telnet log server + ring buffer | `src/Logger.cpp` | radio, 16×304 B static | Low |
| F30 | Retry policy / circuit breaker | `include/clevercoffee/utils/Resilience.h` | — | Low |
| F31 | Standby / auto-sleep | `src/coordinators/StandbyCoordinator.cpp` | — | Low |
| F32 | Backflush cycles + maintenance reminder | `src/state/states/BackflushStates.cpp`, `src/coordinators/MaintenanceCoordinator.cpp` | NVS | Low |
| F33 | Sensor coordinator with async start/try-get | `src/coordinators/SensorCoordinator.cpp` | — | Low |
| F34 | Wokwi simulation | `diagram.json`, `tools/platformio_wokwi.py` | — | Optional |

### F13/F14 are dead code — do not migrate

Neither `HX711Scale` nor `BluetoothScale` is ever constructed.
`HardwareContext::setScale()` and `SensorCoordinator::setScaleSensor()` have zero call
sites in `src/`; `HardwareManager::getScale()` returns `nullptr` with a TODO
(`src/hardware/HardwareManager.cpp:641-651`). `src/main.cpp:145-150` only logs. On top of
that, this is the one technically *unproven* claim worth checking: the original ESP32 **does**
have a BR/EDR + BLE radio (ESP32 datasheet: *"2.4 GHz Wi-Fi + Bluetooth + Bluetooth LE
SoC"*, *"Bluetooth 4.2 BR/EDR and Bluetooth LE dual mode controller"*), so F14 is not
physically impossible — it is simply dead code. The `recommend: drop` stands, but the
*reason* is dead code, not missing silicon.
**Recommendation: drop brew-by-weight and the scale code entirely.** See task R2-07.

---

## 4. Execution model today

Single-threaded cooperative loop. **Zero application-level FreeRTOS primitives** — no
`xTaskCreate`, `xQueue*`, `SemaphoreHandle_t`, `EventGroup`, `portENTER_CRITICAL`, or
`esp_timer_*` callbacks in `src/` or `include/`.

### Startup — `src/main.cpp:108-202`

1. `g_watchdog.begin()` → `esp_task_wdt_init(5, true)` + `enableLoopWDT()` (`:111`)
2. `SystemInitializer::initialize()` (`:121`) — see `src/core/SystemInitializer.cpp:94-244`:
   `SystemContext` → logger/`Serial.begin(115200)` → LittleFS + `Config` + PID object →
   `Wire.begin()` → display → hardware → handlers → networking (**blocking up to 60 s**) →
   MQTT → PID tunings → sensors → ISR context → `initTimer1()` → `enableTimer1()` →
   `markISRReady()`
3. `StateMachine` → `finalizeMachineState()` (`PID_NORMAL`/`PID_DISABLED` from the power
   switch) → `ProcessController` → `LoopManager`
4. `markReady()`

### Main loop — `src/main.cpp:205-241`

`g_watchdog.feed()` **first**, then `loopManager->update()`.

### Loop body — `src/core/LoopManager.cpp:90-253`

1. `Logger::update()` — flush ring buffer to telnet
2. `OTA::pollPendingRestart()` / `pollPendingUrlUpdate()`
3. **If OTA active → `runMainLoopTick()` and `return`** (`:128-132`). The state machine,
   `valveSafetyShutdownCheck`, and PID all stop; heater is force-disabled by
   `otaPrepareHardware()` instead.
4. `SensorCoordinator::update()` → `setWaterTankEmpty(...)` (`:138-139`)
5. switches + standby (`:149`)
6. `StateMachine::update()` + `brewHandler.process()` + `hotWaterHandler.process()` +
   **`valveSafetyShutdownCheck()`** (`:617-619`)
7. `ProcessController::updateProcessControl()` — emergency check, then PID compute
8. LEDs, network, SSE
9. display render (100 ms timer) + buffer flush

### Timers

| Timer | Interval | Constant |
| --- | --- | --- |
| Display refresh | 100 ms | `Timing::DISPLAY_REFRESH_INTERVAL_MS` |
| Temperature | 400 ms | `Timing::TEMPERATURE_SENSOR_INTERVAL_MS` |
| Pressure | 50 ms | `Timing::PRESSURE_SENSOR_INTERVAL_MS` |
| Scale | 100 ms | `Timing::SCALE_SENSOR_INTERVAL_MS` |
| HASSIO discovery | 300 s | `Timing::HASSIO_DISCOVERY_INTERVAL_MS` |
| **ISR PWM tick** | **10 ms** | `Timing::ISR_TIMER_INTERVAL_US` |

### Blocking calls in the loop

| Where | Duration | Risk |
| --- | --- | --- |
| `pressureSensor.h:35` `delay(ABP2_READ_DELAY_MS)` | 10 ms every 50 ms | 20 % of wall clock |
| `PowerHandler.h:183,190` `triggerSystemReboot` | 2 × 1 s | reboot path only |
| `HX711Scale.cpp:44,51` spin loops | unbounded | dead code (F13) |
| `CleverCoffeeWiFiManager.cpp:97-99` | up to 10 s | boot only, WDT disarmed |
| `CleverCoffeeWiFiManager.cpp:121-122` | up to 60 s | boot only, WDT disarmed |

### ISRs

Exactly one: `CleverCoffee::ISR::onTimer()` (`include/clevercoffee/isr.h:63-119`).
`timerBegin(0, 80, true)` → 1 MHz, `timerAlarmWrite(10 000 µs, true)`. It compares
`processPidOutput()` against a free-running counter that wraps at `processWindowSize()`,
then calls `heaterRelay->on()/off()` **directly** — bypassing `HardwareManager`'s
`heaterEnabled_` bookkeeping (documented, `isr.h:47-62`). No FreeRTOS API is called from
the ISR; shared state is `std::atomic`.

### The AsyncTCP priority inversion

`platformio.ini:19-22` sets `CONFIG_ASYNC_TCP_PRIORITY=10` (line 19) while the Arduino `loopTask`
runs at priority 1. The network task can preempt the control loop. ADR-0002 documents the
consequences (OOM aborts under 6-10 parallel API requests) and the 30 KB heap shed.

---

## 5. State machine — 18 states

IDs from `include/clevercoffee/state/MachineStateIds.h:11-39`; classes in
`src/state/StateFactory.cpp`.

| ID | State | ID | State |
| --- | --- | --- | --- |
| 0 | `INIT` | 51 | `STEAM_RUNNING` |
| 20 | `PID_NORMAL` | 60 | `BACKFLUSH_IDLE` |
| 31 | `BREW_PREINFUSION` | 61 | `BACKFLUSH_FILLING` |
| 32 | `BREW_PREINFUSION_PAUSE` | 62 | `BACKFLUSH_FLUSHING` |
| 33 | `BREW_RUNNING` | 63 | `BACKFLUSH_FINISHED` |
| 34 | `BREW_FINISHED` | 70 | `WATER_TANK_EMPTY` |
| 36 | `MANUAL_FLUSH_RUNNING` | 80 | `EMERGENCY_STOP` |
| 90 | `PID_DISABLED` | 95 | `STANDBY` |
| 100 | `SENSOR_ERROR` | 110 | `EEPROM_ERROR` |

`StateMachine::update()` (`src/state/StateMachine.cpp:71-99`) performs **at most one
transition per loop**. `executeTransition()` (`:106-149`) runs `onExit(old)` → swap →
`onEntry(new)`. An unknown ID triggers `ESP.restart()` (`StateFactory.cpp:65-69`).

### Transitions

Global guards first (`include/clevercoffee/state/BaseState.h:137-175`), in order:
emergency stop → sensor error → water tank empty (excluded: `WATER_TANK_EMPTY`,
`STANDBY`) → PID runtime disabled (excluded: `PID_DISABLED`, `PID_NORMAL`, `STANDBY`,
`INIT`, `EMERGENCY_STOP`, `SENSOR_ERROR`, `WATER_TANK_EMPTY`, `EEPROM_ERROR`). Then
`checkSpecificTransitions()`.

Behavioural rules that must be preserved (ADR-0003):

- `onEntryImpl` enables hardware, `update()` re-asserts it, `onExitImpl` disables it.
- Every state that energises pump or valve must disable both in `onExitImpl`.
- States that cannot act on action requests must drain stale request flags.
- `EMERGENCY_STOP` re-runs `performEmergencyShutdown()` **every** loop.

### Valve safety whitelist

`BrewHandler.h:105-122` closes the valve every loop unless the current state is one of:
`BREW_PREINFUSION`, `BREW_PREINFUSION_PAUSE`, `BREW_RUNNING`, `MANUAL_FLUSH_RUNNING`,
`BACKFLUSH_FILLING`, `BACKFLUSH_FLUSHING`. **Any new water-flow state in the Rust port
must be added here.** Close through `HardwareManager` so `valveState_` stays consistent —
poking the relay directly leaves state stuck and the next open becomes a no-op.

---

## 6. Safety-critical control paths

These are the paths where unexpected heating, pumping, or actuation must be prevented.
Each is a hard requirement, verified by the existing native tests in `test/`.

| ID | Path | Mechanism | Test suite |
| --- | --- | --- | --- |
| S1 | Overtemp | `EmergencyStopManager::checkEmergencyConditions` — 3 consecutive readings above `safety.emergency_temp`; **out-of-range readings trip immediately, no debounce** (`EmergencyStopManager.cpp:25-33`) | `test_emergency_stop_manager` |
| S2 | Emergency latch | `emergencyMode_` blocks `enableHeater`, `enablePump`, `openWaterValve`, `openSteamValve`, `setHeaterPower`, `setPumpPressure` (`HardwareManager.cpp:278,306,321,353,398,443`) | `test_emergency_stop_manager` |
| S3 | Emergency recovery | Requires reading valid **and** `< EMERGENCY_SAFE_TEMP_C` (100 °C) (`EmergencyStopManager.cpp:83-87`) | `test_emergency_stop_manager` |
| S4 | Water tank empty | `setWaterTankEmpty(true)` kills a running pump **immediately**; `enablePump` refuses (`HardwareManager.cpp:546-561, 325-328`) | `test_hardware_water_tank`, `test_sensor_coordinator_water_tank`, `test_water_tank_empty_state` |
| S5 | Valve fail-safe | `valveSafetyShutdownCheck()` every loop with an explicit whitelist (§5) | `test_brew_handler` |
| S6 | Heater PWM bounding | Counter wraps at `processWindowSize`; `pidOutput <= counter` → relay off (`isr.h:96-118`) | `test_isr_initialization` |
| S7 | Watchdog | 5 s TWDT, panic on trigger; fed as the **first** statement of `loop()` (`main.cpp:111, 208`) | `test_support.h` stub |
| S8 | OTA session | `otaPrepareHardware()` = `disableTimer1()` + `disableHeater()`; watchdog suspended (`SystemInitializer.cpp:54-59`, `ota.cpp:103-110`) | — |
| S9 | Wi-Fi blocking | WDT suspended around the 10 s connect and the 60 s portal (`SystemInitializer.cpp:816-830`) | `test_wifi_sta_hostname` |
| S10 | Partial-init unwind | `cleanupPartialInit()` tears down in reverse order, relays last, each explicitly off (`HardwareManager.cpp:563-639`) | — |
| S11 | Stale action flags | `PID_DISABLED`, `STANDBY`, and error states drain requests they cannot act on | `test_pid_state_transitions` |

### Known gap to fix, not replicate

While `OTA::isActive()`, `LoopManager::update()` returns early
(`LoopManager.cpp:128-132`): the state machine never runs, so the pump and valve are not
driven during a flash. `beginSession()` only disables the heater. The design assumes the
previous state's `onExit` already cleaned up. **The Rust port must call
`safe_hardware_shutdown()` (pump + valve + heater) rather than only `disable_heater()`.**
Tracked as task R4-09.

### Hardware-invariants cheat sheet for the Rust port

- Never drive a relay except through the actuator facade that also updates its own
  bookkeeping. Exception: the heater PWM path, which must be a single owner (see
  [04 — Target architecture](./04-target-architecture.md) §5).
- `ActuatorState` must be a single value that cannot represent "pump believed off while
  relay is on". Model it as an explicit state machine, not three booleans.
- Every energising state must have a matching `on_exit` that returns the machine to a
  known-safe state, and an `update` that re-asserts the desired state.

---

## 7. Third-party C++ dependencies and their Rust fate

| Library | Version | API surface used | Rust equivalent | Verdict |
| --- | --- | --- | --- | --- |
| `lebuni/ZACwire` | 2.0.0 | `ZACwire(pin, 306)`, `begin()`, `getTemp(maxChangeRate)` | **none — hand-write** from the IST app note | **High risk** |
| `milesburton/DallasTemperature` | 4.0.6 | `getAddress`, `setResolution`, `setWaitForConversion`, `requestTemperaturesByAddress`, `getTempC`, fault sentinels | port the C directly, or `onecable` 0.1.x | Medium |
| `paulstoffregen/OneWire` | 2.3.8 | bit-bang primitives | as above | Medium |
| `olkal/HX711_ADC` | 1.2.12 | `begin`, `startMultiple`, `getData`, `tare`, `setCalFactor`, timeout flags | `hx711` 0.7.0, or port | Drop (dead code) |
| `olikraus/U8g2` | 2.36.18 | 21 methods, 10 `profont`/`fub` bitmap fonts, ~18 bitmaps | `ssd1306` 0.10.0 + `embedded-graphics` 0.8, fonts ported to `ImageRaw` | **High risk** |
| `knolleary/PubSubClient` | 2.8.0 | connect/subscribe/publish/chunked publish, 1024 B buffer | `esp_idf_svc::mqtt::EspMqttClient` | Low |
| `bblanchon/ArduinoJson` | 7.4.3 | v7 `JsonDocument`, `measureJsonPretty`, nested path helpers | `serde` + `serde_json` 1.0.151 (`alloc`) | Low |
| `ESP32Async/AsyncTCP` | 3.5.0 | (transitive) | replaced by lwIP/esp-netif | n/a |
| `ESP32Async/ESPAsyncWebServer` | 3.12.1 | 20+ APIs incl. `AsyncJsonResponse`, `AsyncEventSource`, CORS/auth middleware, `serveStatic` | `esp_idf_svc::http::server::EspHttpServer` | Medium |
| `tzapu/WiFiManager` | 2.0.17 | captive portal, `WiFiManagerParameter`, `setConfigPortalTimeout(60)` | `esp-wifi-provisioning` 0.1, or hand-built softAP | Medium |
| `h2zero/NimBLE-Arduino` | 2.5.1 | (transitive, scale only) | `esp_idf_svc::ble` (NimBLE) 0.53 | Drop |
| `AcaiaArduinoBLE` | v4.0.1 | proprietary BLE scale | none | Drop |
| `Arduino-PID-Library` (vendored `lib/`) | — | `PID_v1` compute, AUTOMATIC/DIRECT | port ~150 lines, no `setpoint` pointer aliasing | Low |
| `esp_wifi` (Arduino) | — | STA, hostname | `esp_idf_svc::wifi::Wifi::new_async` | Low |
| `Preferences` (Arduino) | — | NVS read/write/clear | `esp_idf_svc::nvs::EspDefaultNvsPartition` | Low |
| `LittleFS` (Arduino) | — | mount, read `/config.json`, serve files | `esp_idf_svc::fs::littlefs` + `joltwallet/littlefs` component | Low |
| `Update` / `ArduinoOTA` (Arduino) | — | `Update.begin/write/end`, `ArduinoOTA.handle()` | `esp_idf_svc::ota` | Medium |

---

## 8. Wi-Fi provisioning — the current mechanism and its limit

### Current behaviour (`src/network/CleverCoffeeWiFiManager.cpp`)

- STA only; the AP exists only inside `startConfigPortal()`. No `WiFi.softAP()` call.
- Hostname is set **before** `WiFi.begin()` so the DHCP client-ID matches (`:13-30`).
- If `system.wifi.ssid` is non-empty → explicit STA connect, 10 s poll. On failure:
  **offline mode, no portal** (`:106-109`).
- If empty → `autoConnect` with the portal disabled (`:112-114`); on failure, show
  "Starting Portal AP" on the OLED, `setConfigPortalTimeout(60)`, then
  `startConfigPortal(hostname, password)` (`:116-127`).
- On success: write the hostname to NVS, `delay(1000)`, `ESP.restart()` (`:154-158`).
- Resilience: `RetryPolicy(10 s, 5 min, ×2, 5 attempts)` and
  `CircuitBreaker(5 failures, 60 s open, 30 s half-open)` (`:38-48`).
- `resetSettings()` → `wifiManager_->resetSettings(); delay(500); ESP.restart()` (`:185-189`).
  Exposed as `POST /api/wifi-reset`.

### Credential storage today

`Preferences` in the `config` NVS namespace, under **FNV-1a-hashed keys**
(`Config.h:318-332, 487-501`). `system.wifi.ssid`, `system.wifi.password`,
`mqtt.username`, `mqtt.password`, `system.auth.password`, and `system.ota_password` are
stored **in plaintext**. There is no NVS encryption partition and no `nvs_encryption` key.
tzapu WiFiManager keeps its own copy in its own NVS namespace.

### Why USB provisioning is not an option on this hardware

The original ESP32 has **no USB peripheral**. The USB cable is a CP210x-class bridge wired
to UART0. There is no CDC, no TinyUSB, no USB-JTAG, and no way to enumerate the device as
a USB peripheral. Therefore:

- **Espressif's `wifi_provisioning` transport "USB Serial" is unavailable on ESP32.** That
  transport requires a native USB-Serial-JTAG peripheral, which exists on ESP32-S2/S3,
  C3, C6 and H2 — not on the original ESP32.
- The only serial channel available is **UART0, which is already the log stream** and
  whose pins cannot be repurposed (GPIO 1 is `PIN_STEAMLED`, GPIO 3 is an unused rotary
  pin). A UART provisioning protocol would have to share UART0 with logging.

### Options carried into the decision

1. **Keep the captive portal** (behaviour parity, zero new risk). **Baseline.**
2. **UART0 line protocol** — a simple framed command channel on the same 115200 baud
   stream, e.g. `WIFI SET <ssid> <pass>` with a marker prefix so it can be demultiplexed
   from log output. Feasible but noisy and needs a safe-erase contract.
3. **ESP32-S3 (or C6) hardware with native USB** — enables `wifi_provisioning` over
   `usb_serial_jtag`, which is what "provision over a local USB connection" normally
   means. This is a **hardware change**, not a firmware change, and it is a separate
   decision.

See [03 — Decision record](./03-decision-record.md) §5 and
[05 — Tooling](./05-tooling-and-workflows.md) §5.

---

## 9. Build, test, and release workflows

| Activity | Command | Source |
| --- | --- | --- |
| Build firmware | `~/.platformio/penv/bin/pio run -e esp32_usb -s` | `REPOSITORY_SUMMARY.md:49` |
| Format | `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s` | `CLAUDE.md` |
| Format check (CI) | `pio run --target check-format -e esp32_usb -s` | `.github/workflows/format.yml:43` |
| Native tests | `~/.platformio/penv/bin/pio test -e native_test` | `CLAUDE.md` |
| OTA deploy | `pio run -e esp32_ota -t upload` → `silvia.local` | `platformio.ini:66-71` |
| Merge binary | `esptool.py --chip esp32 merge_bin --flash_mode dio --flash_size 4MB 0x1000 …0x350000` | `README.md:5` + `release.yml:129` |
| Serial monitor | `screen /dev/ttyUSB0 115200` | `DEBUG_GUIDE.md:15` |
| Telnet logs | `telnet esp32.local 23` | `DEBUG_GUIDE.md:60-62` |
| Frontend build | `scripts/build_frontend.py` (pre-build hook) | `platformio.ini:57` |
| Wokwi | `tools/platformio_wokwi.py` (post-build) | `platformio.ini:58` |

Native tests use `test_build_src = false` and `#include` the `.cpp` files directly against
hand-written stubs in `test/` (`test/Arduino.h`, `test/Wire.h`, `test/Preferences.h`,
`test/ZACwire.h`, `test/U8g2lib.h`, `test/OneWire.h`, `test/DallasTemperature.h`,
`test/WiFi*.h`, `test/PubSubClient.h`, `test/esp_task_wdt.h`, `test/esp_system.h`,
`test/esp_heap_caps.h`). **Verified 2026-09-28: 340 test cases, 340 pass** (`pio test -e native_test`, 55 s). Note `docs/plan/task-list.md` still says 234 — it is stale.

---

## 10. Local environment state (2026-09-28) — what is and is not verified

**Verified on this machine**

- `~/.platformio/penv/bin/pio` → PlatformIO Core 6.2.0.
- `.mise.toml` present and now trusted; declares node 24, pnpm, python 3.14.7,
  clang-format 23.1.1 — all currently **missing** (not installed).
- `git` clean at `2006b71`; no `rustup`, no `cargo`, no `just`, no `espflash`.
- `~/.platformio/platforms` does not exist — the `espressif32` platform has never been
  installed here, so the C++ firmware has **not** been built in this environment.

**NOT verified — no hardware**

- `ioreg -p IOUSB` and `system_profiler SPUSBDataType` show **no Espressif device**.
  `/dev/cu.*` contains only Bluetooth devices and a debug console.
- `DEBUG_GUIDE.md` expects `/dev/ttyUSB0`; it does not exist.
- **Conclusion: no ESP32 test board is currently attached to this machine.** No flashing,
  monitor, or hardware validation is possible until one is connected.

**NOT verified — documentation vs. code discrepancies found during the audit**

- `safety.emergency_temp` and `safety.emergency_hysteresis` are defined
  (`Config.h:813, 822`) and read (`EmergencyStopManager.cpp:18-19`) but are **missing from
  `getAllConfigParams()`** (`src/Config.cpp:438-563`), so they are never loaded, saved, or
  exported. They silently reset to compiled defaults on reboot. **This is a live
  over-temperature-settings bug in S1.** Do not port it; record it in the parity notes.
- `CONFIG_REFERENCE.md:114` documents `display.blescale_brew_timer` and `:165-172`
  documents `display.blinking.mode` — neither exists in the code.

**Still to confirm on physical hardware**

| Question | Why it matters |
| --- | --- |
| Exact module on the board: ESP32-WROOM-32E vs WROVER-32E | flash size, PSRAM presence |
| Chip revision (v0/v1/v2/v3 silicon) | errata affecting RF and USB-serial behaviour |
| Does GPIO 16 (temp sensor) and GPIO 17 (valve) really work alongside the module strapping pins | they are documented as not-recommended on ESP32 |
| Relay board active-high vs active-low in the field | `trigger_type` defaults to `HIGH_TRIGGER` |
| Which temperature sensor variant is actually installed (TSIC-306 default) | different wire protocol |
| Free flash after `pio run -t buildfs` | 640 KB `spiffs` budget |

---

## 11. Parity checklist for the Rust port

A feature is ported when all of these are true:

1. The corresponding row in §3 is implemented and reviewed.
2. Host-runnable unit tests exist in the Rust workspace for the logic (no hardware).
3. The C++ native test suites listed in §6 have Rust equivalents with the same assertions.
4. The observable behaviour (display text, MQTT topics, REST payloads, state transitions)
   matches — a side-by-side diff of `/api/status`, `/api/parameters?filter=all`, and the
   state-transition log against the C++ build.
5. Safety paths S1-S11 are covered by Rust tests, with the S1 bug **fixed** rather than
   replicated.
