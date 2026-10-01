# Migration map

Read time: 4 minutes. Purpose: what maps to what, in what order, and what is left.

**TL;DR**

1. The control logic, sensors, display and config are ported and verified on hardware.
2. Network, storage and display are ported and running. OTA and BLE are not.
3. Migrate nothing new until the heater relay polarity is metered.

Status uses the same grades as [FINDINGS.md](FINDINGS.md).

## Module map

| C++ | Rust | Status | Order |
|---|---|---|---|
| `PID_v1.cpp`, `lib/Arduino-PID-Library` | `cc-domain/src/pid.rs` | ✅ bit-exact parity, 900 host tests | 1 |
| `state/StateMachine.cpp`, `states/*` | `cc-machine` (reducer, guards, applier) | ✅ 4140-pair table | 1 |
| `control/EmergencyStopManager.cpp` | `cc-safety`, S1–S11 | ✅ 5 C++ defects fixed, 11 tests | 1 |
| `handlers/BrewHandler`, `HotWater`, `Steam`, `Power` | `cc-machine/src/handlers.rs` | ✅ | 1 |
| `hardware/tempsensors/TempSensorDallas.cpp` | `cc-domain/src/sensor/{ds18b20,onewire}.rs` | ✅ on board | 2 |
| `hardware/tempsensors/TempSensorTSIC.cpp` | `cc-domain/src/sensor/tsic306/` | ❓ simulation only | 7 |
| `hardware/scales/HX711Scale.cpp` | `cc-domain/src/sensor/hx711.rs` + `cc-hal-esp32/src/scale.rs` | ✅ on board | 2 |
| `hardware/scales/BluetoothScale.cpp` | — | ❌ not built, `2013bda9` | 8 |
| `hardware/pressureSensor.cpp` | `cc-domain/src/abp2.rs` + HAL SPI | ⚠️ ported; C++ read defect fixed | 2 |
| `hardware/IOSwitch.cpp`, `hardware/LED.cpp` | `cc-domain/src/switch.rs` + `cc-hal-esp32/src/switches.rs` | ✅ | 2 |
| `hardware/HardwareManager.cpp`, `Relay.cpp`, `isr.cpp` | `cc-hal-esp32/src/{actuators,heater}.rs` | ✅ 10 ms ISR | 2 |
| `ui/OledDriver.cpp`, `display/*` | `cc-display` + `cc-hal-esp32/src/display.rs` | ✅ pixel parity | 3 |
| `Config.cpp`, `ConfigJson.cpp`, `defaults.h` | `cc-config` (schema, blob store, JSON) | ✅ 98 parameters | 3 |
| `network/WebServerManager.cpp` | `cc-hal-esp32/src/web.rs`, `web_async.rs` | ✅ 23 routes, SSE | 4 |
| `network/MQTTManager.cpp` | `cc-hal-esp32/src/mqtt.rs` | ⚠️ built, publish not re-verified | 4 |
| `network/CleverCoffeeWiFiManager.cpp`, `WiFiStaConnect.cpp` | `cc-hal-esp32/src/wifi.rs` | ✅ | 4 |
| `Logger.cpp` (telnet) | `cc-hal-esp32/src/telnet.rs` | ✅ | 4 |
| `ota.cpp` | routes only, answer `501` | ❌ not implemented | 6 |
| `coordinators/SensorCoordinator.cpp` | `cc-firmware/src/sensor_task.rs` | ✅ | 4 |
| `core/LoopManager.cpp`, `main.cpp` | `cc-firmware/src/{main,control}.rs` | ⚠️ runs; 12 ms applier span unexplained | 4 |
| `coordinators/{Network,UICoordinator}.cpp` | `cc-firmware/src/{network,display_task}.rs` | ✅ | 4 |
| `coordinators/{Standby,Maintenance}Coordinator.cpp` | `cc-machine/src/maintenance.rs` | ⚠️ ported, not exercised on board | 5 |
| `maintenance/BackflushReminderLogic.cpp`, `backflush/*` | `cc-machine/src/backflush.rs` | ✅ parity scenarios | 5 |
| `utils/*` (`ApiResponses`, `Resilience`, `helperUtils`, `memoryUtils`, `ModernTimer`) | `cc-domain/src/{error,resilience,units}.rs` | ✅ as needed | 5 |
| `types/GlobalTypes.h`, `constants/*` | `cc-domain/src/units.rs`, `units` | ✅ | 5 |

## Next steps, by risk

1. **Meter the heater relay polarity.** Boiler disconnected, meter on the coil. An undriven GPIO during reset energises 2 kW. Nothing downstream is parallelisable past this. Open — see [ARCHITECTURE.md](ARCHITECTURE.md).
2. **Explain the 12 ms applier span.** Split `apply` / `drain_scale` / the reboot checks and read the same line. Do not relax the budget to make the criterion pass.
3. **Make a gate fail on a LOST device test.** One is pre-existing and nobody has looked at it.
4. **Capture a C++ parity baseline.** 13 scenarios report `BASELINE-MISSING` and exit 2. Nothing was fabricated, which is correct; the capture method is unsolved.
5. **Decide on OTA.** It is the first item in the pre-agreed drop order, and `espota` over USB is the primary update path.
6. **Decide on BLE.** +205 KB flash and +40 KB static RAM for a scale with no device paired to it.
7. **Bring up the ZACwire path or drop it.** It is written and unreachable on the fitted DS18B20 wiring.
8. **Add a gate for text overflow and the 1 px message overlap.** Needs a product decision on typography; `profont10` for the message screens is the cheapest fix and touches six screens.
9. **Import a real C++ export fixture.** The repository's own `config.json` hides the `format_version` and `safety.*` bugs. Take the fixture from the old firmware.
