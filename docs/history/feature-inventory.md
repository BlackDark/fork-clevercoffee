# CleverCoffee — Feature Inventory (pre-migration baseline)

**Status:** Frozen baseline for the Rust migration. Read-only reference.
**See also:** [08 — Recovered oracle](./recovered-oracle.md) — a complete Rust
firmware that previously ran on this board, recovered from flash before the device was
erased. Its source is gone; the binary is the only surviving record.
**Captured:** 2026-09-28, against `main` @ `2006b71`.
**Purpose:** Every feature the current C++ firmware provides, mapped to source files and
hardware. Any Rust implementation that omits a row here is a behavioural regression.

Related documents:
- [02 — Research & compatibility matrix](./dependency-evaluation.md)
- [03 — Decision record (ADR-0004)](../archive/migration/03-decision-record.md)
- [04 — Target architecture](./target-architecture.md)
- [05 — Tooling & developer workflows](../archive/migration/05-tooling-and-workflows.md)
- [06 — Migration task list](../archive/migration/06-migration-task-list.md)
- Existing context: [`../state-machine-architecture.md`](../archive/cpp/state-machine-architecture.md),
  [`../display/rendering.md`](../display/rendering.md),
  [`../adr/0003-state-machine-hardware-control-contract.md`](../adr/0003-state-machine-hardware-control-contract.md)

---

## 1. Target hardware — what "ESP32 v4" actually means

**`v4` is a board PCB revision, not a chip variant.** There is exactly one chip in this
project: the **original Espressif ESP32** (Xtensa LX6, dual core, 4 MB flash, no PSRAM).
Nothing in the repository — code, CI, docs, or the full 1875-commit history — references
ESP32-S3, C3, C6, H2, or S2.

| Property | Value | Evidence |
| --- | --- | --- |
| Chip | ESP32 (original), **silicon revision v3.0** | `platformio.ini:9` `platform = espressif32 @^7.0.1`; `README.md:5` `esptool.py --chip esp32`. Revision **measured on hardware 2026-09-28**: `esptool.py --chip esp32 chip_id` → `Chip is ESP32-D0WD-V3 (revision v3.0)`, and the ESP-IDF boot log prints `efuse_init: Chip rev: v3.0` (`Min chip rev: v0.0`, `Max chip rev: v3.99`) |
| Module | **ESP32-WROOM-32E** (high confidence; see [§10](#10-local-environment-state-2026-09-28--what-is-and-is-not-verified) for the evidence chain and the one open gap) | 4 MB flash, VDD_SDIO = 3.3 V, no PSRAM reported at boot, GPIO16/17 in active use — all measured; part marking not photographed |
| Board | `az-delivery-devkit-v4` (AZ-Delivery ESP32-DevKitC-V4) | `platformio.ini:12`; `docs/archive/cpp/REPOSITORY_SUMMARY.md:43`; board JSON `~/.platformio/platforms/espressif32/boards/az-delivery-devkit-v4.json` |
| Board history | `esp32dev` → `nodemcuv2` → `az-delivery-devkit-v4` (commit `0dafb58`) | `git log -p platformio.ini` |
| Framework | Arduino (ESP-IDF 4.4 under Arduino core 2.0.x) | `platformio.ini:36` `framework = arduino` |
| C++ standard | `-std=gnu++2a` | `platformio.ini:28` |
| Filesystem | LittleFS | `platformio.ini:13` |
| Flash | 4 MB, **DIO**, 40 MHz — **verified on hardware 2026-09-28** | `esptool.py flash_id` → `Detected flash size: 4MB`, JEDEC `0xD8` / `0x4016`; 2nd-stage bootloader prints `boot.esp32: SPI Speed : 40MHz`, `SPI Mode : DIO`, `SPI Flash Size : 4MB`. Both `bootloader.bin` and `firmware.bin` image headers encode `flash_mode = 0x02` (DIO), flash size "keep", 20 MHz. Also `README.md:5`; `.github/workflows/release.yml:125,132` |
| PSRAM | **None.** | No `spiram`/`psram` line anywhere in the ESP-IDF 5.5.5 boot log; `heap_init` lists only DRAM/IRAM regions. See §10 for the caveat that this is conditional on `CONFIG_SPIRAM` in the build that produced that log |
| USB | **None on the chip — the "no native USB" claim HOLDS and is now positively verified.** The USB device on this host is a **WCH CH340** USB-to-UART bridge (VID `0x1A86`, PID `0x7523`), *not* a CP210x/CP2102N. | `ioreg -p IOUSB -l -w 0` on 2026-09-28. The original ESP32 has no USB peripheral (ESP32 Series Datasheet v5.3 peripheral list), so USB CDC / TinyUSB / USB-Serial-JTAG remain impossible — the §8 conclusion is unaffected by the bridge being a CH340 rather than a CP210x. Corroborating: no `HWCDC`/`TinyUSB`/`usb_serial_jtag` anywhere in `src/`+`include/`; the serial node is `/dev/cu.usbserial-*`, not `/dev/ttyUSB0` (`docs/operations/runbook.md` §3); `pinmapping.h:45` moves `PIN_STEAMLED` off GPIO 1 because "UART TX". The CP2102N claim in `.agents/skills/esp32-rust-migration/SKILL.md` §1 is **wrong for this board** |
| Auto-reset | **Present and working.** DTR/RTS auto-reset achieved connection 5/5 times with no manual BOOT+EN | `esptool.py chip_id` / `flash_id` / `read_flash_status` / `read_flash_sfdp` / `get_security_info`, all `--port /dev/cu.usbserial-204140`, no manual intervention. Contradicts the warning in `SKILL.md` §6 |
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

> **Correction (2026-09-28, verified on hardware).** An earlier version of this section said
> "GPIOs 6-11 and 16-17 are also flash/PSRAM-**strapping** on the original ESP32". That is the
> wrong term and it is now settled:
>
> - **Strapping pins** are exactly GPIO0, GPIO2, MTDI (GPIO12), MTDO (GPIO15) and GPIO5
>   (ESP32 Series Datasheet v5.3, Table 3-1 *Default Configuration of Strapping Pins*).
>   **GPIO16 and GPIO17 are not strapping pins.**
> - GPIO6-11 and, on modules with in-package memory, GPIO16/17 are **flash/PSRAM pins** —
>   "not recommended for other uses" per Datasheet Table 2-5. On the ESP32-WROOM-32E they are
>   *not* connected to anything inside the module and are led out to the board, so they are
>   free to use. On an ESP32-WROVER (D0WDR2-V3) the same pins go to the in-package 2 MB PSRAM
>   as `CE#` and `SCLK` (Table 2-5; ESP-WROVER-KIT v2 docs: *"the two GPIOs are not broken out
>   to the board's pin headers in order to ensure reliable performance"*).
> - **Measured:** the firmware currently running on this board drives the valve on GPIO17 and
>   reads a live DS18B20 on the 1-Wire bus (GPIO16 is the strongly-inferred probe pin; the
>   running log does not print it). Both pins work.
>
> Conclusion: **GPIO16 (temp sensor) and GPIO17 (valve) are fine on this board.** The risk was
> real for a WROVER module and is nil for the WROOM-32E that is actually fitted — see §10.

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
| F13 | Scale — HX711 ×1 or ×2 | `src/hardware/scales/HX711Scale.cpp` | GPIO 32/25/33 | **KEPT (R3-17)** — unreachable in C++, see 09 §23 |
| F14 | Scale — Acaia BLE | `src/hardware/scales/BluetoothScale.cpp` | BLE — the ESP32 **does** have a BT+BLE radio | **KEPT (R3-18)** — unreachable in C++, see 09 §23 |
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

### ⚠ The temperature sensor fitted to this machine is a DS18B20, not a TSIC-306

**Measured on hardware 2026-09-28** (boot log of the image currently running on the board):

```
W cc_firmware: config asks for Tsic306 but only the DS18B20 driver exists; reading the 1-Wire bus anyway
I cc_firmware::sensor: sensor: DS18B20 at 0x41af78cdaa376928 (family 0x28), 11-bit resolution, reading every 400 ms
I cc_firmware::supervisor: PidDisabled temp=22.88 setpoint=95.0 ...
```

A 1-Wire device with **family code `0x28` = DS18B20** answered on the probe and produced
live, varying room-temperature readings. A TSIC-306/ZACwire sensor would not respond to
1-Wire at all. So on *this* machine **F9 is not the installed sensor; F10 is.**

- Evidence strength: the log comes from a **Rust** image, not the C++ one, and it does not
  print the probe pin. GPIO16 is a strong inference (it matches `pinmapping.h:27`, and the
  same log names GPIO2/GPIO17/GPIO27 for heater/valve/pump exactly as `pinmapping.h` does).
  **Re-confirm by booting the C++ firmware before R1-03 is scheduled.**
- **Consequence for the plan:** R1-03 (TSIC-306 decoder, flagged *highest risk* and "the one
  spike that can invalidate the whole approach") is aimed at hardware that is not attached
  here. Either the sensor is swapped, or R1-03 is re-scoped to R3-06 (DS18B20). **This needs a
  human decision — see §10.**

### F13/F14 are unreachable in C++ — but they are being ported (R3-17, R3-18)

**Superseded 2026-09-29.** This section previously said "do not migrate", on the grounds
that the code was dead. The human who owns the hardware confirmed the deadness is a bug on
their side, so both scales are ported and made to actually work. The findings below are
still accurate and are the reason the task is sized as it is. Full defect analysis in
[09 §23](./cpp-findings.md).


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
  [04 — Target architecture](./target-architecture.md) §5).
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
| `olkal/HX711_ADC` | 1.2.12 | `begin`, `startMultiple`, `getData`, `tare`, `setCalFactor`, timeout flags | `hx711` 0.7.0, on a dedicated task | **Port (R3-17)** |
| `olikraus/U8g2` | 2.36.18 | 21 methods, 10 `profont`/`fub` bitmap fonts, ~18 bitmaps | `ssd1306` 0.10.0 + `embedded-graphics` 0.8, fonts ported to `ImageRaw` | **High risk** |
| `knolleary/PubSubClient` | 2.8.0 | connect/subscribe/publish/chunked publish, 1024 B buffer | `esp_idf_svc::mqtt::EspMqttClient` | Low |
| `bblanchon/ArduinoJson` | 7.4.3 | v7 `JsonDocument`, `measureJsonPretty`, nested path helpers | `serde` + `serde_json` 1.0.151 (`alloc`) | Low |
| `ESP32Async/AsyncTCP` | 3.5.0 | (transitive) | replaced by lwIP/esp-netif | n/a |
| `ESP32Async/ESPAsyncWebServer` | 3.12.1 | 20+ APIs incl. `AsyncJsonResponse`, `AsyncEventSource`, CORS/auth middleware, `serveStatic` | `esp_idf_svc::http::server::EspHttpServer` | Medium |
| `tzapu/WiFiManager` | 2.0.17 | captive portal, `WiFiManagerParameter`, `setConfigPortalTimeout(60)` | `esp-wifi-provisioning` 0.1, or hand-built softAP | Medium |
| `h2zero/NimBLE-Arduino` | 2.5.1 | (transitive, scale only) | `esp_idf_svc::ble` (NimBLE) 0.53 | Drop |
| `AcaiaArduinoBLE` | v4.0.1 | proprietary BLE scale | none (NimBLE, R3-18) | **Port (R3-18)** |
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

See [03 — Decision record](../archive/migration/03-decision-record.md) §5 and
[05 — Tooling](../archive/migration/05-tooling-and-workflows.md) §5.

---

## 9. Build, test, and release workflows

| Activity | Command | Source |
| --- | --- | --- |
| Build firmware | `~/.platformio/penv/bin/pio run -e esp32_usb -s` | `docs/archive/cpp/REPOSITORY_SUMMARY.md:49` |
| Format | `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s` | `CLAUDE.md` |
| Format check (CI) | `pio run --target check-format -e esp32_usb -s` | `.github/workflows/format.yml:43` |
| Native tests | `~/.platformio/penv/bin/pio test -e native_test` | `CLAUDE.md` |
| OTA deploy | `pio run -e esp32_ota -t upload` → `silvia.local` | `platformio.ini:66-71` |
| Merge binary | `esptool.py --chip esp32 merge_bin --flash_mode dio --flash_size 4MB 0x1000 …0x350000` | `README.md:5` + `release.yml:129` |
| Serial monitor | `screen /dev/cu.usbserial-* 115200` | `docs/operations/runbook.md` §3 |
| Telnet logs | `nc <hostname> 23` | `docs/operations/runbook.md` §4 |
| Frontend build | `scripts/build_frontend.py` (pre-build hook) | `platformio.ini:57` |
| Wokwi | `tools/platformio_wokwi.py` (post-build) | `platformio.ini:58` |

Native tests use `test_build_src = false` and `#include` the `.cpp` files directly against
hand-written stubs in `test/` (`test/Arduino.h`, `test/Wire.h`, `test/Preferences.h`,
`test/ZACwire.h`, `test/U8g2lib.h`, `test/OneWire.h`, `test/DallasTemperature.h`,
`test/WiFi*.h`, `test/PubSubClient.h`, `test/esp_task_wdt.h`, `test/esp_system.h`,
`test/esp_heap_caps.h`). **Verified 2026-09-28: 340 test cases, 340 pass** (`pio test -e native_test`, 55 s).
**Re-verified 2026-09-28 at `34bf308`: 340 test cases, 340 succeeded in 22.382 s** (warm cache; see §10.2.2).

---

## 10. Local environment state (2026-09-28) — what is and is not verified

**Tooling verified on this machine**

- `~/.platformio/penv/bin/pio` → PlatformIO Core 6.2.0.
- `~/.platformio/platforms/` **now exists**: `espressif32` and `native`. The C++ firmware **has
  been built here** (the earlier claim that it never had been is stale).
- `esptool.py` v4.11.0 at `~/.platformio/packages/tool-esptoolpy/esptool.py`
  (PlatformIO-bundled; `espflash` is still not installed — that is R1-01's job).
- `git` clean at **`34bf30898f0aa4f1ec7225c1f01e52de916e6b47`** on branch `rewrite/rust`;
  `git status --short` is empty.
- `node` v26.10.0 and `pnpm` 12.6.0 are on `PATH` (Homebrew, **not** mise). `rustup`, `cargo`
  and `just` are still absent.
- **Network limitation, new:** `registry.npmjs.org` is **unreachable** from this machine
  (`curl` → HTTP `000`, connection failure) while `github.com` returns `200`. Therefore
  `pnpm install` cannot run and **the frontend cannot be built here.** This blocks
  `pio run -t buildfs` and therefore the real LittleFS image size (see R0-02).

**Hardware is now attached** (it was not when this section was first written)

- `/dev/cu.usbserial-204140` — the ESP32 board. `ioreg -p IOUSB` resolves it, so the previous
  "no Espressif device / no board attached" conclusion is stale.
- **The device is running a Rust `cc_firmware` image, not the C++ firmware.** See the loud
  warning below — this is the most consequential finding of this pass.

**NOT verified — documentation vs. code discrepancies found during the audit**

- `safety.emergency_temp` and `safety.emergency_hysteresis` are defined
  (`Config.h:813, 822`) and read (`EmergencyStopManager.cpp:18-19`) but are **missing from
  `getAllConfigParams()`** (`src/Config.cpp:438-563`), so they are never loaded, saved, or
  exported. They silently reset to compiled defaults on reboot. **This is a live
  over-temperature-settings bug in S1.** Do not port it; record it in the parity notes.
- `config/reference.md:114` documents `display.blescale_brew_timer` and `:165-172`
  documents `display.blinking.mode` — neither exists in the code.

---

## 10.1 🔴 Findings that CONTRADICT the migration plan

Read these before scheduling any Phase 1 task. Each one invalidates or reshapes a documented
assumption.

### 🔴 1. The board is running a Rust firmware built by someone else, not the C++ firmware

The boot log captured over UART at 115200 on 2026-09-28:

```
I (29)  boot: ESP-IDF v5.5.5 2nd stage bootloader
I (31)  boot: chip revision: v3.0
I (716) app_init: Project name:     libespidf
I (724) app_init: App version:      v0.0.1-3-gfb0564a-dirty
I (729) app_init: Compile time:     Sep 26 2026 22:39:21
I (811) cc_firmware: config: FreeRTOS tick 1000 Hz, heater window 1000 ms, interlock 500 ms
I (887) cc_firmware: heater interrupt running on GPIO2 (active high), output held off until the supervisor beats
I (888) cc_firmware: pump on GPIO27, valve on GPIO17, both asserted off
I (948) cc_firmware::ota: partitions: app slot 1835008 B, littlefs 393216 B
I (5373) esp_idf_svc::http::server: Started Httpd server with config Configuration { http_port: 80, ... }
I (5373) cc_firmware::web: littlefs: 225280 of 393216 bytes used
```

`fb0564a` is **not an object in this repository** (`git cat-file -t fb0564a` → invalid object
name), and no `Cargo.toml` containing `esp-idf-svc` exists under `~/projects`. The image is
therefore from a different clone, a deleted worktree, or another machine.

**Consequences:**

1. The whole of Phase 1 (R1-01 … R1-08) has, at least partly, already been executed somewhere
   that is not in this repository. The plan's assumption that nothing has been built is false.
   **Locate that tree before starting R1-01**, or a second divergent Rust firmware will be
   created.
2. The running image uses a **different partition table from the root `partitions_4M.csv`**:

   | Partition | root `partitions_4M.csv` (C++) | table live on the device (Rust) |
   | --- | --- | --- |
   | `nvs` | `0x9000` / `0x5000` (20,480 B) | `0x9000` / `0x5000` (20,480 B) |
   | `otadata` | `0xE000` / `0x2000` (8,192 B) | `0xE000` / `0x2000` (8,192 B) |
   | `app0` | `0x10000` / `0x1A0000` (1,703,936 B) | `0x10000` / `0x1C0000` (**1,835,008 B**) |
   | `app1` | `0x1B0000` / `0x1A0000` (1,703,936 B) | `0x1D0000` / `0x1C0000` (**1,835,008 B**) |
   | fs | `0x350000` / `0xA0000` (655,360 B) | `0x390000` / `0x60000` (**393,216 B**) |
   | `coredump` | `0x3F0000` / `0x10000` (65,536 B) | `0x3F0000` / `0x10000` (65,536 B) |

   An R0-02-style rebalance is **already live on the device**, and it differs from the
   arithmetic in 06 R0-02. R0-02 must reconcile the two rather than restart from the root CSV.
3. The device has **Wi-Fi credentials written into its NVS** by that image ("credentials
   written at flash time"). Any R1-08 parity capture that diffs `/api/wifi` or the NVS
   contents must not commit them.

### 🔴 2. The USB-to-UART bridge is a CH340, not a CP2102N

Measured: VID `0x1A86`, PID `0x7523`, `iProduct` = `"USB Serial"`, `iSerialNumber` = absent
(macOS therefore names the node from `locationID` `0x20414000` → `usbserial-204140`).
`0x1A86` is QinHeng/WCH and `0x7523` is the **CH340** family. The bridge being a CH340 rather
than a CP210x changes **nothing** in the plan (it is still a plain UART bridge, so the "no
native USB" conclusion is unaffected), but `SKILL.md` §1 and `README.md` state CP2102N and
that is wrong for this board. The WCH CH34x/CH340 driver note now lives in
`docs/operations/runbook.md` §3 (the old `DEBUG_GUIDE.md` it belonged to has been deleted).

### 🔴 3. Auto-reset works — no manual BOOT+EN needed

`SKILL.md` §6 says *"The original ESP32 may need a manual BOOT+RST. Some DevKitC boards lack
the EN↔GND capacitor."* On this board the DTR/RTS auto-reset circuit is **present and
reliable**: five consecutive `esptool.py` invocations connected with no manual intervention.
`just flash <port>` can be non-interactive. Keep the `espflash hold-in-reset` fallback, but
do not build the workflow around it being needed.

### 🔴 4. A DS18B20 is fitted, not a TSIC-306 — R1-03's premise is wrong for this machine

See the callout in §3. **R1-03 is the single most expensive task in the plan** ("the one spike
that can invalidate the whole approach") and it targets a sensor that is not on this board.
Needs a human decision before R1-03 is scheduled.

### 🔴 5. Silicon is revision v3.0 — the errata worry in the old table was unfounded

`esptool.py chip_id` → `Chip is ESP32-D0WD-V3 (revision v3.0)`; the boot log independently
prints `efuse_init: Min chip rev: v0.0 / Max chip rev: v3.99 / Chip rev: v3.0`. v3.0 is the
newest original-ESP32 silicon, so the "errata affecting RF and USB-serial behaviour" concern
in the old table is largely moot — v3.0 still has no USB peripheral at all, so only RF is
even in question. Record it and move on.

### ⚠ 6. `debug_tool = esp-prog` is probably wrong for this board (not tested)

`platformio.ini:47` sets `debug_tool = esp-prog` for `esp32_usb`, which needs an FT2232H-class
probe. This board exposes only a CH340 UART bridge; there is no JTAG probe on it. `pio run`
(upload) is unaffected — that uses esptool over the UART — but **`pio debug` / gdb is expected
to fail.** Not verified: `pio debug` was not run (it would need a live GDB session and a
booted target). R1-01 should decide how debugging is done, and `SKILL.md` §5's `just lint-esp32`
/ gate flow assumes a working toolchain only, not gdb, so nothing is blocked — but do not
promise gdb.

---

## 10.2 Physical board — verified findings (replaces the old "still to confirm" table)

Every row below was measured on the board at `/dev/cu.usbserial-204140` on **2026-09-28**.
"Measured" = read out of the chip over esptool or out of the device's own boot log.
"Datasheet" = read from an Espressif primary source. "Assumed" = inference, flagged as such.

| # | Question | Verdict | How it was determined | Evidence |
| --- | --- | --- | --- | --- |
| 1 | **Exact module: WROOM-32E vs WROVER?** | **ESP32-WROOM-32E** (high confidence; one gap — see §10.3) | Measured (4 independent signals) | (a) 4 MB flash; (b) esptool reports `Flash voltage set by a strapping pin to 3.3V` — **excludes WROVER-E**, whose VDD_SDIO is 1.8 V (ESP32-WROVER-E datasheet v2.3, §Boot Configurations / block diagram); (c) the ESP-IDF 5.5.5 boot log contains **no** `spiram`/`psiram` line and `heap_init` lists only DRAM/D-IRAM/IRAM regions; (d) GPIO16 and GPIO17 are in active use in the running firmware, and on a WROVER (`D0WDR2-V3`) those pins carry the in-package PSRAM `CE#`/`SCLK` (ESP32 Series Datasheet v5.3, Table 2-5) |
| 2 | **Silicon revision** | **v3.0** | Measured, two independent sources | `esptool.py --chip esp32 --port … chip_id` → `Chip is ESP32-D0WD-V3 (revision v3.0)`, `Features: WiFi, BT, Dual Core, 240MHz, VRef calibration in efuse, Coding Scheme None`, `Crystal is 40MHz`. Boot log: `boot: chip revision: v3.0` and `efuse_init: Chip rev: v3.0`. Note `esp32` has no OTP Chip ID, so esptool reads the MAC instead: `ec:62:60:76:b5:3c` |
| 3 | **Flash size** | **4 MB** (4,194,304 B) | Measured | `esptool.py flash_id` → `Detected flash size: 4MB`; boot log → `boot.esp32: SPI Flash Size : 4MB`; board JSON `upload.flash_size = "4MB"`, `maximum_size = 4194304` |
| 4 | **Flash chip identity** | JEDEC **`0xD8` / `0x4016`**, 4 MB. The *physical part* is **UNVERIFIED** | Measured (JEDEC bytes); the part number is not determinable from software | `esptool.py flash_id` → `Manufacturer: d8`, `Device: 4016`, `Detected flash size: 4MB`, `Flash voltage set by a strapping pin to 3.3V`. ESP-IDF logs `spi_flash: detected chip: generic` — i.e. `0xD8` is **not** in IDF's vendor table, so IDF falls back to the generic NOR driver. Device ID `0x4016` is the density code shared by GD25Q32 and W25Q32 (both 32 Mbit), but the manufacturer byte `0xD8` is not a JEDEC-registered code I could identify from a primary source. **Do not assume GD25Q32.** `read_flash_status` → `Status value: 0x0200`. SFDP could not be read: esptool 4.11.0 aborts with *"Reading more than 32 bits back from a SPI flash operation is unsupported"* |
| 5 | **DIO vs QIO, and clock** | **DIO @ 40 MHz.** QIO is *not* configured | Measured | Image header of **both** `bootloader.bin` and `firmware.bin`: `flash_mode = 0x02` (DIO), flash size nibble `0` ("keep"), frequency nibble `0x2` (20 MHz). The 2nd-stage bootloader re-configures to 40 MHz: `boot.esp32: SPI Speed : 40MHz`, `SPI Mode : DIO`. `boot:0x13 (SPI_FAST_FLASH_BOOT)`. **Whether this `0xD8` part supports QIO at all is UNVERIFIED** — no QIO attempt is logged and the part is unidentified; do not enable QIO |
| 6 | **PSRAM present?** | **No — none detected.** Fully conclusive answer still requires one more check (§10.3) | Measured (boot log, conditional) | Zero occurrences of `psram`, `spiram` or `memspiram` in the full ESP-IDF 5.5.5 boot log. `heap_init: Initializing. RAM available for dynamic allocation:` lists only `3FFAE6E0 len 0x1920 (6 KiB): DRAM`, `3FFBA470 len 0x25B90 (150 KiB): DRAM`, `3FFE0440 len 0x3AE0 (14 KiB): D/IRAM`, `3FFE4350 len 0x1BCB0 (111 KiB): D/IRAM`, `40098008 len 0x7FF8 (31 KiB): IRAM` — **no external-memory heap**. Caveat: ESP-IDF only probes PSRAM when `CONFIG_SPIRAM` is enabled, and the sdkconfig of the image that produced this log is not in this repository |
| 7 | **Auto-reset circuit (EN↔GND cap)?** | **Present and working** | Measured | Five consecutive esptool runs (`chip_id`, `flash_id`, `read_flash_status`, `read_flash_sfdp`, `get_security_info`) all reached *"Connecting......"* on the first attempt at `--baud 115200`, and each ended with `Hard resetting via RTS pin`. No manual BOOT+EN was used at any point. The CH340's DTR/RTS drive the auto-reset transistors. **Non-interactive flashing is viable** — see 🔴 3 |
| 8 | **USB identity** | **WCH CH340**, VID `0x1A86` (6790), PID `0x7523` (29987) | Measured | `ioreg -p IOUSB -l -w 0`, node `USB Serial@20414000`: `idVendor = 6790`, `idProduct = 29987`, `bDeviceClass = 255`, `bcdUSB = 272` (USB 2.0), `USBSpeed = 1` (full-speed 12 Mbit/s), `locationID = 541147136`, `iProduct = 2`, `iSerialNumber = 0` (no serial string → macOS derives `usbserial-204140` from the location). `system_profiler SPUSBDataType` returns nothing on this host. **No Silicon Labs (`0x10C4`) device is present at all** — see 🔴 2 |
| 9 | **Do GPIO16/GPIO17 really work?** | **Yes — both work** | Measured + datasheet | Datasheet: strapping pins are GPIO0, GPIO2, MTDI/GPIO12, MTDO/GPIO15, GPIO5 (ESP32 Series Datasheet v5.3, Table 3-1). GPIO16/17 are flash/PSRAM *pins* (Table 2-5), not strapping pins, and are unused inside a WROOM-32E. Measured: the running firmware logs `pump on GPIO27, valve on GPIO17, both asserted off` and drives the heater on GPIO2, and a live 1-Wire device answers on the temp-sensor bus. §2 has been corrected |
| 10 | **Which temperature sensor is fitted?** | **DS18B20** (1-Wire), not the TSIC-306 default | Measured | Boot log: `cc_firmware::sensor: sensor: DS18B20 at 0x41af78cdaa376928 (family 0x28), 11-bit resolution, reading every 400 ms`, plus `temp=22.88` … `temp=23.25` live values. Caveat: the log is from a **Rust** image, not the C++ one, and it does not print the probe pin (GPIO16 is inferred from `pinmapping.h:27` plus the matching GPIO2/17/27 pins). **Re-confirm on the C++ firmware** — see 🔴 4 |
| 11 | **Relay board active-high or active-low?** | **UNVERIFIED** | Not determinable read-only from a laptop | The only signal available is an *assumption* in the running Rust image: `heater interrupt running on GPIO2 (active high)`. That is a firmware belief, not a measurement. C++ defaults to `HIGH_TRIGGER`. To settle it, put a multimeter or scope on the relay coil and watch the level when the firmware commands the heater off — **needs a person at the machine and a written safe procedure** (see 06 R1-07). Must not be improvised |
| 12 | **Free flash after the current C++ build** | See the size table below | Measured | `pio run -e esp32_usb` + `stat` |
| 13 | **Free flash in LittleFS / `buildfs`** | **UNVERIFIED — `buildfs` cannot run on this machine** | Attempted, blocked by network | `pio run -e esp32_usb -t buildfs` fails in the `pre:` hook `scripts/build_frontend.py` → `pnpm install` → `Error: × resolve pnpm@11.25.0 … Failed to fetch metadata from https://registry.npmjs.org/pnpm`. `curl --max-time 12 https://registry.npmjs.org/pnpm` → HTTP `000`; `https://github.com` → `200`. `ui/node_modules` does not exist. **Not worked around, by instruction.** Also note the `packageManager` field pins `pnpm@11.25.0` while the installed pnpm is 12.6.0, so `corepack` tries to self-download |

### 10.2.1 Measured C++ image sizes (2026-09-28, `34bf308`, `pio run -e esp32_usb`)

Exact byte sizes of the build artifacts:

| Artifact | Bytes | Note |
| --- | --- | --- |
| `.pio/build/esp32_usb/firmware.bin` | **1,546,240** | 5 segments; DROM 0x606F4 (394,996 B), DRAM 0x65F0 (26,096 B), IRAM 0x09304 (37,636 B), IROM 0xFCFA4 (1,035,684 B), IRAM 0xC804 (51,204 B) |
| `.pio/build/esp32_usb/bootloader.bin` | **17,536** | 2nd stage; header DIO / 20 MHz / size "keep" |
| `.pio/build/esp32_usb/partitions.bin` | **3,072** | binary table, magic `0x50AA`; matches the root `partitions_4M.csv` (verified by decoding the entries) |
| `.pio/build/esp32_usb/firmware.elf` | 51,335,052 | — |
| `littlefs.bin` | **absent** | `buildfs` never completed — see row 13 |
| Data source dir | **absent** | `data/` does not exist (gitignored; produced by `pnpm copy:dist`) |

Free-space arithmetic against the **root `partitions_4M.csv`** (this is the C++ baseline; it is
*not* the table live on the device — see 🔴 1):

| Region | Size | Used | Free |
| --- | --- | --- | --- |
| `app0` / `app1` slot | 1,703,936 B (0x1A0000 = 1,664 KiB) | 1,546,240 B (`firmware.bin`) | **157,696 B = 154.0 KiB (9.25 % headroom)** |
| `spiffs` (LittleFS) | 655,360 B (0xA0000 = 640 KiB) | **UNKNOWN** | **UNKNOWN** — `buildfs` blocked |
| `nvs` | 20,480 B | runtime | — |
| `otadata` | 8,192 B | runtime | — |
| `coredump` | 65,536 B | runtime | — |
| Total flash | 4,194,304 B (4 MiB) | table ends exactly at `0x400000` | 0 B unused |

PlatformIO's own accounting for the same build: `RAM: 14.1 % (used 75,240 bytes from
532,480 bytes)`, `Flash: 90.4 % (used 1,539,657 bytes from 1,703,936 bytes)`. (1,539,657 vs
1,546,240: the ELF is 7,583 B smaller than the packaged `.bin` because of the 24-byte image
header, 8-byte-per-segment padding and the trailing SHA-256 digest.)

Reference point from the image **live on the device** (Rust, different table — 🔴 1):
app slot **1,835,008 B**, LittleFS **393,216 B of which 225,280 B used → 167,936 B free**.

### 10.2.2 Re-verified baseline (R0-04 inputs)

| Command | Result |
| --- | --- |
| `git rev-parse HEAD` | `34bf30898f0aa4f1ec7225c1f01e52de916e6b47` |
| `git branch --show-current` | `rewrite/rust` |
| `git status --short` | *(empty — clean tree)* |
| `pio run -e esp32_usb` | **SUCCESS**, 3.86 s (incremental). Tooling note: `tool-mklittlefs @ ~1.203.0` was auto-installed during the failed `buildfs` run |
| `pio test -e native_test` | **340 test cases, 340 succeeded, 00:00:22.382**. All 33 suites + 4 sub-suites PASSED. (The "55 s" quoted in §9 includes compilation; with a warm build cache the run itself is 22.4 s) |
| `pio run -e esp32_usb -t buildfs` | **FAILED** — npm registry unreachable, see row 13 |

### 10.2.3 Re-verified native-test count

**340 is correct**, and 06 §"C++ test-suite coverage map" already says so. The C++ cleanup
tracker that once claimed 234 (`docs/plan/task-list.md`) has been deleted rather than left
standing with a number 106 below the truth.

## 10.3 Still UNVERIFIED — what is missing and exactly how to close it

| Gap | Why it is still open | Cheapest way to close it |
| --- | --- | --- |
| **Module part marking** — is it *literally* stamped WROOM-32E? | Four independent software signals all point to WROOM-32E, but none of them reads the module's silkscreen/label. A WROVER-4MB has 4 MB flash **and** 3.3 V VDD_SDIO, so flash size and rail voltage alone do not exclude it. Only signals (c) and (d) exclude it, and both are conditional on the running image's build config | **Photograph the module** (R0-01 originally asked for this). One close-up of the metal can ends it. Alternatively, the board must be opened |
| **PSRAM, conclusively** | The boot log's silence about PSRAM is only meaningful if the image that produced it was built with `CONFIG_SPIRAM=y`. That sdkconfig is not in this repository | Add `esp_psram_get_size()` and an `assert`/log of the result to the **R1-01** minimal `main` (which already does a pin readback and asserts). One boot answers it with a compile-time-checked value. Until then, "no PSRAM" is a high-confidence inference, not a measurement |
| **Which physical flash part is `0xD8:0x4016`?** | `0xD8` is not in ESP-IDF's vendor table (hence `detected chip: generic`) and is not a JEDEC code I could identify from a primary source. `0x4016` only fixes the density (32 Mbit / 4 MB) | Read the chip's markings off the board, or read the 64-bit flash unique ID (RDUID) — ESP-IDF's `esp_flash_read_unique_chip_id` can do this, or `esptool` with a newer version than 4.11.0 (SFDP is blocked by a 4.11.0 limitation: *"Reading more than 32 bits back from a SPI flash operation is unsupported"*) |
| **QIO capability of that flash part** | QIO needs the part's Quad-Enable bit set correctly; the part is unidentified, so there is no datasheet to check against | Read the part number (above) and check its datasheet. **Until then: stay on DIO** — DIO is proven working |
| **Relay active-high vs active-low** | Only a firmware *belief* is available (`GPIO2 (active high)` from the Rust image) | Multimeter or scope on the relay coil while the firmware commands the heater off. **Needs a written safe test procedure, reviewed, boiler disconnected, person present** (06 R1-07). Not to be improvised |
| **Real LittleFS image size** | `pnpm install` cannot reach `registry.npmjs.org` from this machine | Restore npm-registry access (or pre-populate `ui/node_modules` and `data/ui` from a machine that has it), then `pio run -e esp32_usb -t buildfs`. This is the **R0-02** prerequisite |
| **Whether the C++ firmware reads a DS18B20 too** | The DS18B20 evidence came from a Rust image | Flash the C++ firmware and read the boot log. Do this before R1-03 is scheduled (🔴 4) |
| **`pio debug` / gdb** | `debug_tool = esp-prog` needs an FT2232H probe; this board has only a CH340. Not tested | Decide in R1-01. Until decided, **do not promise a gdb workflow** — `espflash` + `defmt`/log-based debugging is the likely answer |
| **Where the Rust firmware on the device came from** | `fb0564a` is not a git object here and no matching workspace exists under `~/projects` | Ask whoever flashed it, or search the filesystem for another clone. **Do this before R1-01** (🔴 1) |

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
