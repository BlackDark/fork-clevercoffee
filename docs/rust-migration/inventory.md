# C++ Firmware Inventory

Scope: everything the current Arduino/ESP-IDF firmware does, traced from `setup()` to the
steady-state main loop, plus the API contract, the config export schema and the known-defects
register.

Evidence tags used throughout:

| Tag | Meaning |
| --- | --- |
| `repo-verified` | Read directly from the source in this repository. |
| `device-verified` | Confirmed on a physical board. |
| `needs confirmation` | Inferred or assumed, not confirmed by any source. |

**No device was available for this inventory.** Every hardware claim is `repo-verified`
(from code, pin maps, datasheet-class reasoning) or `needs confirmation`. Nothing in this
document is `device-verified`. See [verification-levels.md](verification-levels.md).

Cross-links: [compatibility-matrix.md](compatibility-matrix.md),
[architecture.md](architecture.md), [decision-record.md](decision-record.md),
[task-list.md](task-list.md), [defects-register.md](defects-register.md),
[api-contract.md](api-contract.md), [config-export-schema.md](config-export-schema.md).

---

## 1. Target hardware

| Property | Value | Source | Tag |
| --- | --- | --- | --- |
| Board | `az-delivery-devkit-v4` (PlatformIO board id) | `platformio.ini:12` | repo-verified |
| Chip | ESP32, Xtensa LX6 dual-core, revision-dependent | `platformio.ini:9` (platform `espressif32`) | repo-verified |
| Flash | 4 MB (partition table is `partitions_4M.csv`, offsets reach `0x400000`) | `partitions_4M.csv` | repo-verified |
| PSRAM | none | `diagram.json:11` (Wokwi: "0 PSAR") | repo-verified |
| Framework | Arduino (`framework = arduino`), PlatformIO `espressif32 ^7.0.1` | `platformio.ini:9,13` | repo-verified |
| C++ standard | `gnu++2a` (C++23) | `platformio.ini:28` | repo-verified |
| Display | 128x64 mono OLED, SSD1306 or SH1106, hardware I2C | `include/clevercoffee/hardware/pressureSensor.h:18` for I2C addr of the pressure sensor; `src/display/DisplayManager.cpp:32-47` | repo-verified |
| USB | No native USB on ESP32. The `esp32_usb` env name refers to flashing/serial over the USB-serial bridge | `platformio.ini:60-64` | needs confirmation |

The C++ build only targets the original ESP32. There is no S3 or C6 build environment in
`platformio.ini`. The three targets for the Rust port are therefore: original ESP32 (Xtensa,
bench target, currently unavailable), ESP32-S3, ESP32-C6.

### 1.1 Pin map (`include/clevercoffee/hardware/pinmapping.h`)

| Signal | GPIO | Direction | Notes |
| --- | --- | --- | --- |
| Power switch | 39 | in | input-only pin on ESP32 |
| Brew switch | 34 | in | input-only |
| Steam switch | 35 | in | input-only |
| Hot water switch | 36 | in | input-only, no internal pull available |
| Water tank switch | 23 | in | internal pull selectable |
| Temp sensor (1-Wire) | 16 | bidirectional | DS18B20 or TSIC |
| HX711 data 1 / 2 / clock | 32 / 25 / 33 | in/out | scale, never constructed |
| Valve relay | 17 | out | active-high in `config.json` |
| Pump relay | 27 | out | active-high |
| Heater relay | 2 | out | active-high |
| Status / brew / steam LED | 26 / 19 / 1 | out | pin 1 is UART0 TX, conflicts with serial logging |
| Zero-cross dimmer | 18 | out | declared, never used in code |
| I2C SDA / SCL | 21 / 22 | bidir | OLED and pressure sensor |

The file carries 20 `static_assert`s validating that every pin is below 40
(`include/clevercoffee/hardware/pinmapping.h:80-100`).

---

## 2. Peripherals and integrations

| Peripheral | Part / interface | Bus | Period | Source | Tag |
| --- | --- | --- | --- | --- | --- |
| Brew temperature | DS18B20 (Dallas) or TSIC 306 (ZACwire), selectable | 1-Wire bit-banged on GPIO16, or single-wire pulse protocol | 400 ms read | `src/hardware/tempsensors/TempSensorDallas.cpp`, `src/hardware/tempsensors/TempSensorTSIC.cpp` | repo-verified |
| Pressure | Honeywell ABP2, I2C `0x28` | I2C | 50 ms, 10 ms blocking conversion delay | `include/clevercoffee/hardware/pressureSensor.h:18,35` | repo-verified |
| Display | SSD1306 / SH1106, 128x64 | I2C, SDA 21 / SCL 22 at default 100 kHz | 100 ms render, full-buffer flush | `src/display/DisplayManager.cpp:32-47` | repo-verified |
| Scale | HX711 (1 or 2 load cells) or Acaia Bluetooth LE | bit-banged GPIO / BLE | 100 ms | `src/hardware/scales/HX711Scale.cpp`, `src/hardware/scales/BluetoothScale.cpp` | repo-verified |
| Relays | pump, valve, heater, one GPIO each | GPIO | event driven | `src/hardware/Relay.cpp` | repo-verified |
| LEDs | status, brew, steam | GPIO | event driven | `src/hardware/StandardLED.cpp` | repo-verified |
| Network | Wi-Fi station, Wi-FiManager captive portal, Telnet log server (port 23) | radio | event driven | `src/network/CleverCoffeeWiFiManager.cpp`, `src/Logger.cpp` | repo-verified |
| MQTT | MQTT broker client with Home Assistant discovery | TCP | 3 publish intervals | `src/network/MQTTManager.cpp` | repo-verified |
| Storage | NVS (`Preferences`), LittleFS filesystem partition | flash | event driven | `src/Config.cpp`, `partitions_4M.csv` | repo-verified |
| OTA | espota (ArduinoOTA), HTTP upload, HTTP URL download | Wi-Fi | event driven | `src/ota.cpp` | repo-verified |
| Web UI | React 19 + Vite bundle served from LittleFS at `/ui/` | HTTP | on request | `scripts/build_frontend.py`, `src/network/WebServerManager.cpp` | repo-verified |

### 2.1 Sensor detail

**DS18B20.** Created with `setResolution(11)` and `setWaitForConversion(false)`, so the
firmware issues a conversion and reads the scratchpad on a later loop pass
(`src/hardware/tempsensors/TempSensorDallas.cpp:9-19`). 11-bit resolution is 375 ms of
conversion time. ROM address is captured in the constructor. Error sentinels are
`DEVICE_DISCONNECTED_C` and `DEVICE_FAULT_*_C`.

**TSIC 306.** Uses the `ZACwire` library. `getTemp()` blocks for the pulse train
(`src/hardware/tempsensors/TempSensorTSIC.cpp:14-19`). Readings `<= 0` or `>= 180` degrees C
are rejected as glitches.

**Shared sensor path.** `TempSensor::tryGetValue()` runs the read synchronously and applies a
15-sample moving average (`include/clevercoffee/hardware/tempsensors/TempSensor.h:226`). A
`max_bad_readings_` counter of 10 exists but `updateTemperature()` has no call sites.

**Pressure.** Global mutable state in a header (`include/clevercoffee/hardware/pressureSensor.h:18-29`),
24-bit counts, span 0-10 bar, 10 ms conversion delay inside the main loop. Disabled by
default in `config.json`.

**Scale.** Never instantiated. `SensorCoordinator::setScaleSensor()` has no call sites and
`HardwareManager::getScale()` returns `nullptr`, so the scale feature is dead code in the
current firmware even when enabled in config.

### 2.2 Display

128x64 only, full-page buffer mode (`_F_` in the U8G2 type name) so every flush pushes 1024
bytes over I2C. `Wire.setClock()` is never called, so the bus runs at the Arduino default
100 kHz, which puts a single flush at roughly 90 ms of blocked main loop. Six templates
(standard, minimal, temperature-only, scale, upright, modern) and three languages
(English, Spanish, German as the fallback branch for any unrecognised enum value,
`include/clevercoffee/display/languages.h:131-132`).

### 2.3 Switches

Four panel switches plus the water tank sensor. Panel switches use plain `INPUT` with no
internal pull (`src/hardware/GPIOPin.cpp:47-50`), so the board must supply external
pull-ups or pull-downs. Normally-open or normally-closed is configurable per switch. Debounce
is 20 ms, long press 500 ms (`include/clevercoffee/hardware/IOSwitch.h:63-64`).
`isPressed()` mutates the debounce and long-press state, so it is not idempotent.

### 2.4 Relays and the heater

Each relay has a configurable trigger level. `config.json` sets all three to high-trigger.
The heater relay is not driven by the state machine at all: it is driven directly by a
10 ms hardware timer ISR that software-PWMs the pin from the PID output
(`include/clevercoffee/isr.h:63-119`). This is documented in `CLAUDE.md` as the one
legitimate exception to the "never poke relays directly" rule.

---

## 3. Startup and main loop

### 3.1 Startup order (`src/main.cpp`, `src/core/SystemInitializer.cpp`)

1. Global `Watchdog` object, 5 s timeout (`src/main.cpp:105`).
2. `setup()` feeds the watchdog start, then `SystemInitializer::initialize()`.
3. Serial at 115200, `Logger` init (100 ms blocking delay, `src/Logger.cpp:112`).
4. `LittleFS.begin()`, then config load and LittleFS seeding (`src/core/SystemInitializer.cpp:262-276`).
5. Maintenance coordinator reads its NVS namespace.
6. PID gains derived, PID controller constructed.
7. `Wire.begin()` on the default SDA 21 / SCL 22.
8. Display created and probed over I2C, then a full logo flush.
9. `HardwareManager` constructed: relays, LEDs, switches, temperature sensor.
10. Handlers constructed.
11. Networking: Wi-Fi connect with up to a 10 s blocking wait, or a 60 s captive portal
    (`src/network/CleverCoffeeWiFiManager.cpp:97-122`). The watchdog is suspended across this.
12. Web server on port 80, OTA, MQTT.
13. PID configured: sample time 1000 ms, output 0-1000, integrator limits hardcoded 0-55.
14. Hardware timer 0 armed at a 10 ms auto-reload period (`include/clevercoffee/isr.h:124-136`).
15. ISR released once the system context is ready.
16. `StateMachine`, `ProcessController`, `LoopManager` constructed.
17. `loop()` starts: feed watchdog, `LoopManager::update()`.

If initialization fails, `setup()` calls `exit(0)`
(`src/main.cpp:129-133`), which on ESP32 restarts the chip.

### 3.2 Main loop (`src/core/LoopManager.cpp:90-253`)

Single-threaded, in this order:

1. `Logger::update()` (telnet accept/flush, blocking TCP writes).
2. OTA pending restart and pending URL download polling.
3. **Early return when an OTA is active**: sensors, state machine, PID, LEDs, network, web
   and display are all skipped.
4. Sensor coordinator: temperature, pressure, water tank, scale.
5. Water tank empty latch update.
6. Centred sensor timers.
7. Switch handlers and standby coordinator.
8. State machine update, then the valve safety shutdown check.
9. Process control (PID).
10. LEDs.
11. Network: Wi-Fi maintenance, MQTT, ArduinoOTA.
12. Website: SSE temperature and weight events (1 Hz).
13. Display render (10 Hz) and deferred flush.

The watchdog is fed once, at the top of `loop()`.

---

## 4. State machine

19 states (`include/clevercoffee/state/MachineStateIds.h:11-39`). Transitions are evaluated
in `BaseState::checkTransitions` in priority order: emergency stop, sensor error, water tank
empty, PID disabled, then per-state rules. Details in
[the existing state machine doc](../state-machine-architecture.md).

States that energize the pump: `BREW_PREINFUSION`, `BREW_RUNNING`, `MANUAL_FLUSH_RUNNING`,
`BACKFLUSH_FILLING`, plus `PID_NORMAL` and `STEAM_RUNNING` while the hot-water switch is
held. States that open the water valve: the same set plus `BREW_PREINFUSION_PAUSE`.

`BrewHandler::valveSafetyShutdownCheck()` runs once per loop
(`include/clevercoffee/handlers/BrewHandler.h:105-122`) and closes the valve unless the
current state is one of brew, preinfusion, preinfusion pause, manual flush, backflush filling
or backflush flushing.

The steam valve (`openSteamValve`) and the solenoid (`openSolenoid`) exist but are never
called. Hot water is not a state: it is pump control inside `PID_NORMAL` and
`STEAM_RUNNING` (`src/state/states/PidStates.cpp:33-43`).

---

## 5. Timing and blocking

| Interval | Value | Source |
| --- | --- | --- |
| Heater ISR period | 10 ms | `include/clevercoffee/constants/Timing.h:16` |
| Temperature read | 400 ms | `include/clevercoffee/constants/Timing.h:42` |
| Pressure read | 50 ms, plus 10 ms blocking | `include/clevercoffee/constants/Timing.h:43` |
| Scale read | 100 ms | `include/clevercoffee/constants/Timing.h:44` |
| Water tank poll | 200 ms | `include/clevercoffee/coordinators/SensorCoordinator.h:272` |
| Display render | 100 ms | `include/clevercoffee/constants/Timing.h:38` |
| Switch debounce / long press | 20 ms / 500 ms | `include/clevercoffee/hardware/IOSwitch.h:63-64` |
| Watchdog | 5000 ms, panic on trip | `src/main.cpp:105` |
| Brew pump timeout | 300 s, never armed | `include/clevercoffee/handlers/BrewHandler.h:32` |
| Hot water pump timeout | 60 s, never armed | `include/clevercoffee/handlers/HotWaterHandler.h:28` |
| Emergency stop | trip above 150 C after 3 consecutive readings, hysteresis 5 C, clear below 100 C | `src/control/EmergencyStopManager.h:108` |

Blocking calls in the loop: Telnet log writes, the pressure sensor's 10 ms delay, the DS18B20
or TSIC read, the I2C display flush (~90 ms at 100 kHz), and the OTA URL download.

---

## 6. Libraries

| Library | Version | Rust equivalent | Risk |
| --- | --- | --- | --- |
| Arduino-PID (vendored, `lib/Arduino-PID-Library`) | vendored | own implementation, ~150 lines, host-testable | low |
| U8g2 | 2.36.18 | `ssd1306` / `embedded-graphics` (unverified per chip) | medium |
| DallasTemperature + OneWire | 4.0.6 / 2.3.8 | no adequate crate, see decision record | high |
| ZACwire (TSIC) | 2.0.0 | own crate | medium |
| HX711_ADC | 1.2.12 | own crate, ~150 lines | medium |
| AcaiaArduinoBLE | 4.0.1 (git) | TrouBLE + own Acaia client, or drop | high |
| PubSubClient | 2.8.0 | `rumqttc` (no_std capable) or own client | medium |
| ArduinoJson | 7.4.3 | `serde_json` | low |
| ESPAsyncWebServer + AsyncTCP | 3.12.1 / 3.5.0 | no embedded Rust equivalent; see decision record | high |
| NimBLE-Arduino | 2.5.1 | `TrouBLE` | high |
| WiFiManager | 2.0.17 (git) | not needed, see architecture (USB provisioning replaces it) | low |
| Preferences (NVS) | framework | `esp-storage` + own encoder, or own partition | medium |
| LittleFS | framework | not needed if assets go in a dedicated flash region | low |

The two high-risk items are the web server and the sensors. Both are addressed in
[decision-record.md](decision-record.md).

---

## 7. Build, test and flash workflow

| Workflow | Command | Source |
| --- | --- | --- |
| Format | `pio run --target format -e esp32_usb` | `scripts/run_clangformat.py` |
| Build | `pio run -e esp32_usb` | `platformio.ini:60` |
| Native tests | `pio test -e native_test` | `platformio.ini:73-93` |
| Flash (USB) | `pio run -e esp32_usb -t upload` with `esp-prog` | `platformio.ini:62` |
| Flash (OTA) | `pio run -e esp32_ota -t upload` with espota and `--auth=otapass` | `platformio.ini:66-71` |
| Frontend build | `pnpm install && pnpm prepare-esp` in `ui/` | `scripts/build_frontend.py:20` |
| Filesystem image | `pio run --target buildfs` | `platformio.ini:13` |
| CI | 4 workflows: build, format, frontend, release | `.github/workflows/` |

Native tests use GoogleTest with `test_build_src` disabled; test files `#include` production
`.cpp` files directly and a stub `test/Arduino.h` is force-included
(`platformio.ini:73-93`). 37 test directories, 302 test cases. Roughly half of `src/*.cpp` is
never compiled into any test binary: the web server, OTA, MQTT, LoopManager, SystemInitializer,
both scales, the Dallas sensor and every display template are untested.

All third-party GitHub Actions are pinned by commit SHA and every workflow uses
`permissions: contents: read`, except the release workflow which uses `contents: write`.

---

## 8. Web API contract

The full contract, route by route, with request and response payloads and status codes, is in
[api-contract.md](api-contract.md). Summary: 30 routes on port 80, including 26 under
`/api/`, an SSE stream at `/events`, static UI serving at `/ui/`, and a JSON catch-all 404.
The `docs/api/openapi.yaml` file that ships with the repo disagrees with the code in 26
places; every disagreement is listed in [api-contract.md](api-contract.md#spec-drift).

Frontend assets are built by Vite into `data/ui`, packed into a LittleFS image at partition
offset `0x350000`, and served from flash. The Vite postbuild step deletes every `html`, `js`
and `css` file from `dist/` and relies on the `.gz` siblings produced by `rollup-plugin-gzip`;
if the gzip plugin ever skips a file, that file is lost.

---

## 9. Config export schema

The exact schema, every field, type, unit, range, default and its reconciliation against
`config.json` and `docs/example_config.json`, is in
[config-export-schema.md](config-export-schema.md). Headline findings:

- 96 registered parameters in 10 top-level groups.
- **No schema version marker anywhere.** The only marker is a `_seeded` boolean in NVS.
- NVS keys are 9-character FNV-1a hashes of the dotted parameter name, not readable names.
- Range limits are enforced on the `set()` path but **not** on the NVS load path.
- `safety.emergency_temp` and `safety.emergency_hysteresis` are defined and consumed by the
  emergency-stop manager but are never registered, never persisted and never exported, so
  they are always the compiled default.
- The two shipped JSON files disagree with each other on the temperature sensor type, the
  scale type and the scale calibration, and `config.json` is not the file that seeds LittleFS.

---

## 10. Problem features

Each entry: why it is a problem, a size estimate, and the proposed handling. Sizes are
engineering estimates in lines of new Rust plus test harness.

| # | Feature | Problem | Size | Handling |
| --- | --- | --- | --- | --- |
| P1 | Web server with 30 routes and SSE | No embedded Rust HTTP server with ESP32 track record; the largest single subsystem in the C++ code and entirely untested | 1500-2500 | Split into its own task; build a minimal no_std HTTP/1.1 server crate, test handler logic on host |
| P2 | DS18B20 driver | No maintained Rust crate; `ds18b20` 0.1.1 is 6 years stale and pins `embedded-hal` 0.2.3 while `esp-hal` 1.x exposes 1.0 | 300-500 | Write our own `one-wire` + `ds18b20` crate, host-testable against a bit-level model |
| P3 | Acaia Bluetooth scale | BLE stack swap plus a vendor protocol; scale is dead code today so there is no parity pressure | 600-1000 | Defer to a later phase, or drop with approval; the HX711 path is enough for brew-by-weight |
| P4 | 6 display templates with pixel-accurate layout | 3146 lines in C++, zero tests, and the C++ test stub cannot measure glyph bounding boxes | 1200-1800 | Port the layout engine with a font-metrics table and a host framebuffer; treat as its own phase |
| P5 | MQTT + Home Assistant discovery | 1388 lines in C++, zero tests, needs a broker to verify | 700-1000 | Split; host-test the discovery-document generator against golden JSON, defer broker verification to a device phase |
| P6 | Telnet logging | RFC 2217-ish log stream on port 23, low value for the migration | 200-300 | Keep as a simple line server, or defer. Proposed: defer to a post-migration task |
| P7 | Wi-Fi captive portal | Replaced by USB provisioning in the new design | 0 | Drop. `needs confirmation` from the user that the portal is not required |
| P8 | Wokwi simulation | Requires PlatformIO; the Rust firmware cannot run in Wokwi | 0 | Drop `diagram.json`, `wokwi.toml` and `tools/platformio_wokwi.py` in the final phase |

---

## 11. Known defects register

The full register with locations, impact, severity and the planned fix is in
[defects-register.md](defects-register.md). The critical and high entries, all verified
against the code, are:

| ID | Location | Defect | Severity |
| --- | --- | --- | --- |
| D01 | `src/core/LoopManager.cpp:128-132`, `src/core/SystemInitializer.cpp:54-59` | An active OTA returns from the main loop before the state machine, the valve safety check and the PID, and the OTA prepare hook disables only the heater timer, never the pump or valve | critical |
| D02 | `src/hardware/HardwareManager.cpp:293-303` | `heaterEnabled_` is never set to true anywhere, so every `heaterEnabled_`-guarded shutdown path is inert and `disableHeater()` is a no-op | critical |
| D03 | `include/clevercoffee/hardware/tempsensors/TempSensor.h:110-146` | A disconnected temperature sensor is never detected: every failure returns the same "not ready" code the coordinator treats as "in progress", so `SENSOR_ERROR` is unreachable and PID output saturates at 100 percent with the cached 0.0 reading | critical |
| D04 | `include/clevercoffee/handlers/PowerHandler.h:158,179` | Unchecked `display()` dereference when the OLED is disabled, which the config permits | high |
| D05 | `include/clevercoffee/isr.h:91`, `src/isr.cpp` | The 10 ms heater ISR calls flash-resident C++ functions that are not marked `IRAM_ATTR`, while the ISR itself is | high |
| D06 | `src/ota.cpp:215-242` | OTA state and the pending-URL `String`s are mutated from the network task and read from the main loop with no synchronisation | high |
| D07 | `src/ota.cpp:297-309`, `src/display/DisplayOtaScreen.cpp:43-58` | The OTA progress callback draws into the shared OLED framebuffer from the network task while the main loop draws into the same buffer | high |
| D08 | `include/clevercoffee/hardware/pressureSensor.h:13,35` | A 10 ms `delay()` inside the 50 ms pressure read, so 20 percent of loop wall-clock is spent sleeping whenever pressure sensing is on | high |
| D09 | `include/clevercoffee/handlers/PumpTimer.h:30-33` | The 5 minute brew and 60 second hot-water pump run-time limits are never armed, so a stuck switch runs the pump indefinitely | high |
| D10 | `src/network/WebServerManager.cpp:287-291` | The authentication middleware never enables an auth type, so every route, including factory reset and OTA, is unauthenticated even with auth configured | high |
| D11 | `include/clevercoffee/Config.h:284,464` | NVS load assigns the stored value without the range check that the setter performs | high |
| D12 | `include/clevercoffee/Config.h:813-829`, `src/Config.cpp:450-453` | The emergency-stop temperature and hysteresis are consumed by the emergency manager but never registered, so user configuration of them is silently ignored | high |
