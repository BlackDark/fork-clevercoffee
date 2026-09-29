# Known-defects register

Every bug, race, unsafe pattern and improper implementation found in the C++ firmware. The Rust
port implements the corrected behaviour; each row states the fix. Rows are also cross-referenced
from [task-list.md](task-list.md) and [architecture.md](architecture.md).

Severity: **critical** (can heat, pump or flood unattended), **high** (wrong behaviour, crash,
or a security hole), **medium** (a defect with a workaround), **low** (hygiene).

Cross-links: [inventory.md](inventory.md), [architecture.md](architecture.md),
[api-contract.md](api-contract.md), [config-export-schema.md](config-export-schema.md),
[board-pinouts.md](board-pinouts.md).

---

## Critical

### D01 — An active OTA leaves the pump and valve energized

| | |
| --- | --- |
| Location | `src/core/LoopManager.cpp:128-132`, `src/core/SystemInitializer.cpp:54-59`, `src/ota.cpp:103-110` |
| Impact | `LoopManager::update()` returns early while an OTA is active, before the sensor read, the state machine, `valveSafetyShutdownCheck()` and the PID. `otaPrepareHardware()` disables the heater timer and calls `disableHeater()`, which is a no-op because of D02. An OTA started mid-brew therefore leaves the pump running and the water valve open for the whole flash, which can be minutes. The web upload path additionally calls `beginSession()` from the network task, not the loop task. |
| Severity | critical |
| Fix | OTA is refused unless the machine is idle. `Actuators::force_off()` is called on the OTA path, in the safety task, and in the panic handler. See [architecture.md](architecture.md#15-fail-safe-actuator-states). |

### D02 — `heaterEnabled_` is never set, so every heater shutdown path is inert

| | |
| --- | --- |
| Location | `src/hardware/HardwareManager.cpp:293-303`, `:210-234`, `:530-544`, `src/control/ProcessController.cpp:160,421` |
| Impact | `enableHeater()` has zero call sites, so `heaterEnabled_` is always false. `disableHeater()`, `safeShutdown()`, `disableAllHardware()` and the partial-init cleanup all early-return on `if (!heaterEnabled_)`. The heater relay is controlled only by the 10 ms ISR reading `processPidOutput`, so the entire `HardwareManager` heater contract does nothing. |
| Severity | critical |
| Fix | `Actuators` holds no shadow boolean. `heater_off()` always writes the pin; there is no path where the bookkeeping can disagree with the hardware. |

### D03 — A disconnected temperature sensor is never detected

| | |
| --- | --- |
| Location | `include/clevercoffee/hardware/tempsensors/TempSensor.h:110-146`, `src/coordinators/SensorCoordinator.cpp:41-71`, `include/clevercoffee/constants/Temperature.h:14` |
| Impact | `tryGetValue()` returns `SENSOR_NOT_READY` for every failure mode, and the coordinator treats that as "still in progress". The 1000 ms timeout branch can never fire because `startRead()` re-arms every 400 ms. `tempSensorError_` therefore stays false, `hasSensorError()` is permanently false, and `SENSOR_ERROR` is unreachable for temperature. `cachedTemperature_` stays at its initial `0.0`, which is inside the valid range, so the emergency-stop manager sees no fault either. The PID then computes an error of 95 - 0 and saturates its output at 1000, running the heater flat out with no firmware protection. `TempSensor::updateTemperature()`, which holds the `bad_readings_` counter, has no call sites. |
| Severity | critical |
| Fix | A reading that fails its CRC or falls outside the valid range sets a `SensorFault` latch. The heater is inhibited until a valid reading arrives, and the latch is a state the safety task reads, not a log line. See [architecture.md](architecture.md#1-concurrency). |

## High

### D04 — Unchecked display dereference when the OLED is disabled

| | |
| --- | --- |
| Location | `include/clevercoffee/handlers/PowerHandler.h:158,179`, `src/core/SystemInitializer.cpp:356-358` |
| Impact | `hardwareContext().display()` returns `nullptr` when `hardware.oled.enabled` is false, because `setDisplay()` is never called in that branch. Both call sites dereference it unchecked, so powering on or long-pressing power to reboot crashes the device. |
| Severity | high |
| Fix | No raw pointers. The display is a trait object owned by one task; a disabled display is a `None` variant handled at construction. |

### D05 — The 10 ms heater ISR calls flash-resident code

| | |
| --- | --- |
| Location | `src/isr.cpp`, `include/clevercoffee/isr.h:96-108`, `src/hardware/Relay.cpp:14-20`, `src/hardware/GPIOPin.cpp:13-17` |
| Impact | `onTimer` is `IRAM_ATTR`, but `Relay::on/off()` and `GPIOPin::write()` are ordinary functions in `.cpp` files with no `IRAM_ATTR`; `Relay.h:17-25` explicitly claims they are ISR-safe. During a flash erase or write the instruction cache can be bypassed, producing an illegal-instruction panic. The ISR also calls `timerAlarmWrite()` on every tick to reprogram an already auto-reloading alarm. |
| Severity | high |
| Fix | The interrupt handler reads an `AtomicU32` and writes one register, both inlined. It calls no driver, allocates nothing, and never touches flash. |

### D06 — OTA state is raced between the network task and the main loop

| | |
| --- | --- |
| Location | `src/ota.cpp:215-242`, `:494-553`, `include/clevercoffee/ota.h:42-148` |
| Impact | `OTAStateManager` members and the module globals, including heap-allocating `String`s, are mutated from the AsyncTCP task and read from loopTask with no synchronisation. A torn `String` is a garbage-pointer dereference; two lost updates on the busy flag allow two concurrent flashes. |
| Severity | high |
| Fix | One executor. The OTA state is owned by the `net` task and mutated only through messages. |

### D07 — OTA progress draws into the shared OLED framebuffer from the network task

| | |
| --- | --- |
| Location | `src/ota.cpp:121-130`, `:297-309`, `src/display/DisplayOtaScreen.cpp:43-58`, `src/core/LoopManager.cpp:197-199` |
| Impact | `reportProgress()` draws and flushes the shared U8G2 buffer from the network task while the main loop's display task draws into the same buffer. Both tasks run on core 1, so they interleave by preemption. |
| Severity | high |
| Fix | Only the `display` task touches the framebuffer; other tasks send it a message. |

### D08 — A 10 ms blocking delay inside the 50 ms pressure read

| | |
| --- | --- |
| Location | `include/clevercoffee/hardware/pressureSensor.h:13,35`, `src/coordinators/SensorCoordinator.cpp:107-124` |
| Impact | `measurePressure()` contains a hard `delay(10)` and runs every 50 ms from the main loop, so 20 percent of loop wall-clock is spent sleeping whenever pressure sensing is enabled. The class comment and ADR 0002 both describe the path as non-blocking. |
| Severity | high |
| Fix | The pressure driver is a future that starts a conversion and returns; the read happens on a later tick. |

### D09 — The pump run-time limits are never armed

| | |
| --- | --- |
| Location | `include/clevercoffee/handlers/PumpTimer.h:30-33`, `include/clevercoffee/handlers/BrewHandler.h:254-262`, `include/clevercoffee/handlers/HotWaterHandler.h:114-122` |
| Impact | `PumpTimer::isExpired()` returns false whenever `isRunning_` is false, and neither `start()` nor `stop()` is ever called for either handler, so both `checkPumpTimeout()` bodies are unreachable. The advertised 5-minute brew and 60-second hot-water limits do not exist and a stuck switch runs the pump indefinitely. |
| Severity | high |
| Fix | Every pump command carries a deadline; the safety task enforces it independently of the handler that issued it. |

### D10 — The authentication middleware never enables an auth type

| | |
| --- | --- |
| Location | `src/network/WebServerManager.cpp:282-296` |
| Impact | `setUsername`, `setPassword` and `setRealm` are called but `setAuthType()` never is, so the middleware's auth method stays `AUTH_NONE` and `allowed()` returns true for every request. Every route, including `/api/factory-reset`, `/api/restart`, `/api/wifi-reset` and all of `/api/ota/*`, is unauthenticated even with `system.auth.enabled` set. |
| Severity | high |
| Fix | Authentication is a check in the router itself. A mutating route without a valid credential returns 401. |

### D11 — NVS load assigns the stored value without the range check

| | |
| --- | --- |
| Location | `include/clevercoffee/Config.h:284`, `:464`, against `:156-160` |
| Impact | `set()` rejects out-of-range values through `isValid()`, but `loadFromNvs()` assigns `currentValue_ = value` with no check. A corrupted region, a hand-edited blob, or a blob written by a firmware with wider bounds loads straight into live control state, including setpoints and PID gains. |
| Severity | high |
| Fix | The config region is CRC-checked and every field is range-checked on decode, before any value is used. |

### D12 — The emergency-stop parameters are consumed but never registered

| | |
| --- | --- |
| Location | `include/clevercoffee/Config.h:813-829`, `src/Config.cpp:450-453`, `src/control/EmergencyStopManager.cpp:18-19` |
| Impact | `safety.emergency_temp` and `safety.emergency_hysteresis` are read by the emergency-stop manager but are absent from `getAllConfigParams()`. They are therefore never persisted, never exported, never imported, never returned by `/api/parameters`, and always the compiled default of 150.0 and 5.0 at runtime. A user who lowers the emergency threshold in the UI is silently ignored. |
| Severity | high |
| Fix | The schema table is the single registry; an unregistered field cannot exist. |

## Medium

### D13 — Config import is non-atomic and reports false success

`src/Config.cpp:325-345`, `src/network/WebServerManager.cpp:747-760`. Each parameter is applied and committed to NVS as it is parsed, and the function returns `updatedCount > 0`. A file with 5 valid and 91 invalid values persists the 5, logs 91 warnings, and the route answers HTTP 200 "Configuration validated and applied successfully." **Fix:** transactional import with a structured report, per [architecture.md](architecture.md#52-field-mapping-and-validation).

### D14 — Secrets are exported in plaintext from four endpoints

`src/Config.cpp:510,517,522,526`, `src/network/WebServerManager.cpp:380,658,710`. `system.auth.password`, `system.ota_password`, `mqtt.password` and `system.wifi.password` are returned verbatim by `GET /api/config`, `GET /api/config/download`, `GET /api/parameters` and `GET /api/nvs-debug`, with CORS set to reflect any origin and credentials allowed. **Fix:** the schema marks secret fields; export and every status response redact them.

### D15 — `/api/temperatures` returns error bodies with HTTP 200

`src/network/WebServerManager.cpp:620-628` combined with `:1170,1179,1184`. When the system context is missing or serialization fails, the error object is sent as the body of a 200. **Fix:** error bodies are only ever sent with a non-2xx status.

### D16 — Firmware-from-URL has no validation and is an SSRF vector

`src/ota.cpp:380-433` and `:626-677`. `POST /api/ota/url` fetches an arbitrary URL with no scheme or host allow-list, and the firmware variant applies no extension check at all, unlike the filesystem variant. The device will stream any reachable content into the app partition. **Fix:** the path is kept (the user confirmed OTA stays) but requires an `http` or `https` scheme, a host on a small allow-list, and the same extension check the filesystem variant already had.

### D17 — OTA is unauthenticated

`src/ota.cpp:847-866`. `system.ota_password` is only used for espota; the HTTP OTA endpoints never check it. **Fix:** every OTA path, HTTP and espota and USB, requires the configured password, and refuses to start unless the machine is idle, which is also the D01 fix.

### D18 — The 10 ms ISR reads a non-atomic `double` written by the PID

`include/clevercoffee/isr.h:91`, `src/control/ProcessController.cpp:88,116`, `include/clevercoffee/context/ProcessState.h:176`. The PID library writes through a raw pointer; the ISR reads a plain `double`. Formally a data race. Xtensa aligned 8-byte access is a single instruction so tearing is unlikely, but the compiler is free to tear it. **Fix:** the duty cycle crosses the boundary as an `AtomicU32` in permille.

### D19 — `BackflushFillingState::update()` does not re-assert its hardware state

`src/state/states/BackflushStates.cpp:71-76`. Every other water-flow state re-asserts `enablePump()` and `openWaterValve()` in `update()`; this one only logs, which violates ADR 0003. If the valve safety check or the water-tank latch turns the pump off mid-fill, it does not come back until the fill times out. **Fix:** `Actuators` is the only writer and the control task re-asserts the current command every tick.

### D20 — `cleanupPumpAndValve()` never closes the steam valve

`include/clevercoffee/state/BaseState.h:129-132`, `src/hardware/HardwareManager.cpp:467-485`. `closeWaterValve()` early-returns when the valve state is `STEAM_OPEN`, so the shared relay would stay energised. Latent only because nothing calls `openSteamValve()`. **Fix:** `Actuators::valve_close()` closes every valve it owns.

### D21 — The documented "safe mode" performs no hardware action

`src/state/states/ErrorStates.cpp:13-22`, `src/state/MachineStateContext.cpp:383-402`. `enterSafeMode`, `exitSafeMode`, `disableWaterOperations`, `enableWaterOperations` and `setManualFlushState` are log-only stubs, and `SensorErrorState::onEntryImpl` calls `enterSafeMode()` believing it disables hardware. **Fix:** a fault state is one enum variant of the state machine with one `force_off` in its entry, and the compiler makes an unhandled variant an error.

### D22 — `HardwareManager::hasTemperatureError()` is a working fail-safe that nothing calls

`src/hardware/HardwareManager.cpp:245-250`, `src/state/MachineStateContext.cpp:110-112`. It returns true when no sensor exists, which is exactly the guard D03 needs, but the context delegates to `SensorCoordinator::hasTemperatureSensorError()` instead, which is always false. **Fix:** one sensor-fault state, one owner, no second path.

### D23 — String parameters have no length limit and the constants are dead

`include/clevercoffee/Config.h:187`, `include/clevercoffee/defaults.h:118-125`. `isValid` returns true for every string, and the eight `*_MAX_LENGTH` constants are referenced nowhere outside `defaults.h`. `CONFIG_REFERENCE.md` documents lengths that disagree with the constants. **Fix:** every string field in the schema has an enforced maximum, and the documented value is the enforced value.

### D24 — The setpoint endpoint applies to live control before validating

`src/network/WebServerManager.cpp:391-408`. The handler validates against its own ad-hoc range of 0 to 150, applies it to the running PID, and only then calls `Config::brewSetpoint.set()` whose real bound is 20 to 110; the return value is discarded with `(void)`. A setpoint of 150 is applied and then silently rejected. **Fix:** one schema entry, validated before anything is applied, with the applied value in the response.

### D25 — `POST /api/parameters` silently ignores empty values

`src/network/WebServerManager.cpp:830`. The guard `p->value().length() > 0` means a String parameter cannot be cleared through the API. **Fix:** an explicit empty value clears the field to its default, and the response reports it.

### D26 — The parameter filter does not do what it claims

`src/Config.cpp:380-406`. `filter == "hardware"` maps to section 4, but hardware parameters are declared in sections 11, 12, 13 and 15; `filter == "other"` maps to section 5, which is the display. Unrecognised filters silently fall back to sections 0 and 1. **Fix:** filter by an explicit tag on each schema entry, not by a section number.

### D27 — Importing a double loses precision

`src/Config.cpp:222`. `String(importValue.as<double>())` uses Arduino's two-decimal default, so `0.005` imports as `0.01` and `11.456` as `11.46`. **Fix:** the import path is typed, not stringly typed.

### D28 — `/api/pid` returns 200 with a value it did not apply

`src/network/WebServerManager.cpp:462-480`. When the machine state context is null, the handler computes the new value from config, skips the apply, and still returns 200 with that value. **Fix:** the response reports the value that is actually in effect, read back after the apply.

### D29 — `HX711Scale::init()` can hang forever

`src/hardware/scales/HX711Scale.cpp:31-77`. `while (!loadCell1->startMultiple(5000, true)) {}` with no yield and no timeout, then the same shape for the second cell. Not currently reachable because the scale is never constructed, but it is the documented init path. **Fix:** every driver init has a bounded retry count.

### D30 — The scale is configured but never constructed

`src/main.cpp:145-150`, `src/coordinators/SensorCoordinator.cpp:73-75`, `src/hardware/HardwareManager.cpp:641-651`. `setScaleSensor()` has no call sites and `getScale()` returns null, so a configured scale silently reports 0.0 g and brew-by-weight can never terminate. `hasScaleSensorError()` returns false rather than an error. **Fix:** a configured sensor that is absent is a configuration error surfaced at boot, not a silent zero.

### D31 — The I2C bus runs at 100 kHz, so each display flush blocks ~90 ms

`src/display/DisplayManager.cpp:40-41`, `src/core/SystemInitializer.cpp:121`. `Wire.setClock()` is never called, and the full-page buffer mode pushes 1024 bytes per frame at 10 Hz, which exceeds the project's own 100 ms slow-loop threshold. **Fix:** run the bus at 400 kHz, and flush only the pages that changed.

### D32 — Four handlers block the AsyncTCP task

`src/network/WebServerManager.cpp:680-703,766-782,785-805` and `include/clevercoffee/Config.h:164-177`. `/api/wifi-reset` sleeps 1 second then erases settings and restarts, `/api/restart` sleeps then restarts, `/api/factory-reset` clears the config region then restarts, and `/api/parameters` opens, writes and closes NVS once per parameter. All of it runs on the network task while the control loop keeps heating. **Fix:** handlers are pure functions; every state change is a message; nothing in a handler blocks.

### D33 — Three conflicting emergency-stop thresholds coexist

`include/clevercoffee/types/GlobalTypes.h:126`, `include/clevercoffee/constants/Temperature.h:6-7,14-15`, `include/clevercoffee/Config.h:813-820`. A `GlobalTypes` constant of 145, a `Temperature.h` pair of 145 and 120, and the live config defaults of 150 with a 100 clear. `testEmergencyStop()` is dead code. **Fix:** one threshold and one clear threshold, both from the schema.

### D34 — The spec and the code disagree in 26 places

`docs/api/openapi.yaml` against the handlers. Full list in [api-contract.md](api-contract.md#spec-drift). Includes a documented `POST /api/config` that does not exist, a documented `/api/scale/calibrate` that never existed, wrong field names on `/api/status`, `/api/steam`, `/api/backflush`, `/api/temperatures`, `/api/history` and `/api/parameters`, a wrong request shape on `/api/setpoint` and `POST /api/parameters`, a wrong response shape on `/api/parameter-help`, a wrong status code on `/api/ota/url`, a wrong claim that config upload restarts the device, and no `securitySchemes` at all. **Fix:** the Rust contract is [api-contract.md](api-contract.md), derived from the code, and `openapi.yaml` is regenerated from it.

### D35 — `docs/`, `README` and `examples/` are stale

`REPOSITORY_SUMMARY.md` describes a `frontend/` directory that does not exist, an ArduinoJson version that is wrong, a test file that moved, and a `develop` branch that does not exist. `DEBUG_GUIDE.md` cites line numbers that are all wrong and a state machine enum that no longer exists. All six files in `examples/` include headers that do not exist. `CONFIG_REFERENCE.md` documents two parameters that do not exist, omits two that do, and gives ranges that disagree with the code. **Fix:** the final phase rewrites the docs against the Rust firmware and deletes `examples/`.

## Low

### D36 — The heater relay sits on a boot-mode strapping pin and is driven late

`include/clevercoffee/hardware/pinmapping.h:40` puts the heater relay on GPIO2, and
`src/hardware/HardwareManager.cpp:70-93` creates the relays, and drives them off, only after the
logger, LittleFS, NVS, I2C and the display are up. ESP32 Datasheet v5.3, table 3-1, lists GPIO2
as a boot-mode strapping pin sampled at reset. The steam LED is on GPIO1, which is UART0 TX, and
the C++ pin map already notes the conflict. **Fix:** the new board map moves the heater to GPIO4,
and the boot order drives every actuator to its inactive state before anything else is
configured, so the strapping sample is correct. See
[board-pinouts.md](board-pinouts.md#53-proposed-pin-maps).

### D37 — The switch `isPressed()` is not idempotent

`src/hardware/IOSwitch.cpp:19-52`. It mutates debounce and long-press state, yet it is called several times per loop from the sensor coordinator, the power handler and two states, so the first caller's timestamp wins and later callers see stale readings for the same loop pass. **Fix:** a switch is sampled once per tick into an immutable snapshot that everything downstream reads.

### D38 — Locks protect state whose readers take none

`include/clevercoffee/utils/SystemUtils.h:21-53`. `setRuntimePidState`, `setUserPidEnabled` and `setSteamMode` lock a function-local static mutex, but the protected fields are plain bools read without a lock, and everything runs on one task. False assurance plus three wasted mutex cycles per state entry. **Fix:** no locks in the control path.

### D39 — Logger nesting can overflow the 8 KB loop task stack

`src/Logger.cpp:212,245,281`, `include/clevercoffee/Logger.h:159-161`. A nested `LOGF` chain uses about 576 bytes of stack plus `vsnprintf`'s frame, and states log from entry, exit, update and transition checks, so two or three frames can nest. The task stack is the framework default and is not raised in `platformio.ini`. **Fix:** logging writes into a fixed-size stack buffer and formats in place, with no nested frames.

### D40 — The hot-path logging races on the level

`include/clevercoffee/Logger.h:150`, `src/Logger.cpp:235,272`. `level_` is a plain enum written by `setLevel` and read by every log statement, from two tasks. **Fix:** an atomic, or better, compile-time filtering with a runtime override in a cell.

### D41 — ADR 0002's concurrency rationale is wrong

`docs/adr/0002-*.md:31` against `platformio.ini:21`. The ADR justifies the lock-free ring by asserting the main loop is on core 1 and AsyncTCP on core 0; the build forces `CONFIG_ASYNC_TCP_RUNNING_CORE=1`, so both are on core 1 and there is only preemption. The CAS is still correct; the reasoning is not. **Fix:** rewritten in the Rust architecture, where there is only one executor.

### D42 — Duplicate ordering values make parameter order non-deterministic

`include/clevercoffee/Config.h:1344,1350,817,835`. `system.offline_mode` and `system.log_level` both use order 1103; `safety.emergency_temp` and `steam.setpoint` both use 203. **Fix:** the schema's order is derived from declaration index, not a hand-written number.

### D43 — `ParamType` has unused variants and a broken float case

`include/clevercoffee/Config.h:29-37,210-211,256-257,271-300`. `UINT8`, `FLOAT` and a second `DOUBLE` are declared; `getParamType()` never returns them, and `float` falls through to `INT`. A `ParamDef<float>` would pass the `static_assert` but fail to load and fail to save. **Fix:** the schema has exactly the types the wire format supports, and the compiler rejects anything else.

### D45 — The PID gains configured for the brew phase are always overwritten by the brew-detection gains

`src/core/SystemInitializer.cpp:649-670`. `calculateDerivedValues()` computes `aggKi` and `aggKd` from the regular gains, stores them, and then immediately recomputes both from the brew-detection gains and stores those over the top, with the comment at `:662` "Note: aggbKi and aggbKd are mapped to aggKi/aggKd for now". `initializePID()` at `:541-544` then reads `processPidAggKi()` and `processPidAggKd()`, so the initial tuning is always the BD set, whatever `pid.regular.*` says. In the shipped `config.json` that is `pid.bd` kp 50 / tn 0 / tv 20 against `pid.regular` kp 50 / tn 200 / tv 20: the observable difference is the integral action, which the BD set has none of because its `tn` is zero. **Fix:** the Rust port takes the gain set from the state machine, so a brew uses the regular gains and only the brew-detection window uses the BD ones. `Pid` holds one `Gains` and the selection between regular, steam and BD belongs to the control task, which is where the state is known.

### D46 — The initial PID integrator limit is hardcoded and contradicts the configuration

`src/core/SystemInitializer.cpp:553` sets the integrator limits to `(0, 55.0)` with the comment "AGGIMAX constant", before any state transition has run. The configured `pid.regular.i_max` is not applied there; `ProcessController::calculatePIDParameters()` reads it into `aggIMax_` at `src/control/ProcessController.cpp:47` and passes it to `setPidIntegratorLimits` at `:211`, which runs on the first state change and thereafter on every PID re-tune. So the effective limit is the configured value, but only after the first transition, and the two disagree until then: with the shipped `config.json` (`i_max: 75`) the boot-time limit is 55 and the running limit is 75. **Fix:** the Rust port takes the limit as a constructor argument with no hardcoded default, so there is only ever one value.

### D47 — The Arduino PID's input filter starts at zero, so the first heater compute is suppressed

`lib/Arduino-PID-Library/PID_v1.cpp`. `lastFilteredInput` is initialised to 0, so the first `Compute()` sees `dInput = ((1-alpha)*input - 0) / dt`, which with this firmware's `kd` of roughly 713 is about -14 000 counts. The output is then clamped to 0, so the heater does not start for one full sample period and the machine appears to ignore the first temperature reading after boot. **Fix:** the port seeds the filter with the first real reading, so the first derivative term is 0. A test pins this, because the symptom is a machine that seems not to heat at all and is easy to misread as a sensor fault.

### D49 — The backflush flush phase closes the water valve, so the group cannot drain

`src/state/states/BackflushStates.cpp:97-98`. `BackflushFlushingState::onEntryImpl` calls `cleanupPumpAndValve`, which is `disablePump()` plus `closeWaterValve()` (`include/clevercoffee/state/BaseState.h:129-132`), and `update()` at `:106-111` re-asserts neither. The state therefore sits for the whole flush period with the water valve shut, which is what isolates the group. The same file logs "flushing into drip tray" at `:99`, and `BrewHandler::valveSafetyShutdownCheck` explicitly lists `BACKFLUSH_FLUSHING` among the states that may hold the valve open (`include/clevercoffee/handlers/BrewHandler.h:114`), so the C++ contradicts itself about what the phase is for. With the valve shut the group holds the water it was just filled with. **Fix:** the Rust port opens the valve and stops the pump for the flush phase, matching the interlock whitelist and the log message, and the heater stays off as in every other backflush phase.

### D50 — The steam and manual-flush start flags are edge requests the C++ consumed but the pure function cannot

`src/state/states/SteamStates.cpp:52-54` and `src/state/states/SystemStates.cpp:84-92` read and clear a request flag, so a stale start flag cannot terminate a running phase. A pure function has no such consumption. **Fix:** the control task raises the request for exactly one tick, which makes the edge explicit and removes the possibility of a stale flag.

### D51 — The manual-flush edge out of `PID_NORMAL` is dead in the C++

`src/state/states/PidStates.cpp:75-79` transitions to `MANUAL_FLUSH_RUNNING` when `requestManualFlushStart_` is set, but nothing ever sets that flag: it is only read there and cleared at `MachineStateContext.h:620`. The reachable edge is `BACKFLUSH_IDLE` to `MANUAL_FLUSH_RUNNING` (`BackflushStates.cpp:47-51`). **Fix:** the Rust table has both edges, so the dead one is a working path rather than a silent gap, and the difference is documented here.

### D52 — The scale calibration range permits a zero divisor

`include/clevercoffee/Config.h:1151-1160`. The calibration is a **divisor** applied to the raw
load-cell counts, and the accepted range is -999999 to 999999, which includes zero. A zero
calibration produces a division by zero, and the negative half of the range is legitimate: the
shipped `config.json` has -1750.05 and -1685.21, because an inverted load cell really does read
negative. So the range cannot be tightened to exclude zero without also excluding real devices.
**Fix:** the schema keeps the signed range and adds a separate `forbid_zero` flag, so the rule is
enforced without narrowing the range.

### D53 — Dead and stale code

`examples/` (6 files, all including headers that do not exist), `scripts/auto_compression.py` (disabled, references a pre-Vue asset list), `test/TESTING_GUIDE.md` (references two deleted test directories and a stale test count), `PlatformIO::check_tool = clangtidy` with no `.clang-tidy` file and no CI job, `HX711Scale.cpp` and `BluetoothScale.cpp` with no construction site, `PIN_ZC` and `PIN_ROTARY_*` with no code reference, `HardwareManager::setHeaterPower` and `setPumpPressure` as TODO stubs, `openSolenoid` as a TODO stub, `getAllStateParams` as a no-op, `Valve::openSteamValve` with no caller. **Fix:** the final phase deletes the C++ tree outright rather than porting dead code.

### D54 — The ABP2 pressure reading is seven bytes and takes its temperature from a status byte

`include/clevercoffee/hardware/pressureSensor.h`. `ABP2_data[7]` is seven bytes and the loop reads
seven, then builds the temperature count from `ABP2_data[6] + ABP2_data[5]*256 + ABP2_data[4]*65536`
and converts it with `* 270.0 / 16777215.0 - 40.0`. An ABP2 returns two six-byte words, three
status and three data bytes each: twelve bytes. The seventh byte the C++ read is the *second word's
status*, not temperature data, and the first word's three data bytes end at index 3, so the C++
temperature count mixes one status byte with two data bytes. **Fix:** the port reads the full
twelve-byte frame, checks both status bytes for a diagnostic fault before converting anything, and
takes each word's three data bytes from its own position. It also rejects a count outside the
configured span rather than converting it into a negative pressure that the over-pressure logic
would treat as valid.

### D55 — The pressure read blocks the main loop for ten milliseconds

`include/clevercoffee/hardware/pressureSensor.h`, `measurePressure()`. After the convert command
it calls `delay(ABP2_READ_DELAY_MS)`, ten milliseconds with the whole main loop stopped. At the
100 ms debug interval the effect is small, but at any faster sampling rate it is a tenth of the time
the control loop is not running. **Fix:** `Abp2::start_conversion` returns immediately and
`Abp2::read` is called later; `Abp2::settle_ms` states the requirement and the caller spends it.
No driver in this port sleeps. This is the concrete form of defect D08.

### D56 — The valve interlock closed the hot-water valve, so hot water pumped with the valve shut

`include/clevercoffee/handlers/BrewHandler.h:105-122` and `include/clevercoffee/state/*.h`. The
valve safety check closes the water valve in any state that is not a brew, a manual flush or a
backflush fill or flush. The hot-water dispense, however, runs *inside* `PID_NORMAL` and
`STEAM_RUNNING` with no state of its own (`src/../PidStates.cpp:33-43`,
`SteamStates.cpp:36-46`), so the check closed the valve on every tick of a hot-water dispense while
the pump kept running. A machine in that state pumps with the three-way valve shut: no water
reaches the group, the pump runs against a closed path, and the UI shows a dispense that is
happening. The two lists disagreed for the same reason the C++ lists always disagreed: one place
decided which states move water, another decided which states may hold the valve, and the hot-water
path existed in only one of them.

**Impact:** the hot-water button is broken on the C++ firmware for any configuration that reaches
`PID_NORMAL`, which is the default. Whether a user has ever noticed depends on whether their
machine's three-way valve plumbing happens to pass water with the solenoid de-energised, which is a
property of the plumbing rather than of the firmware.

**Severity:** high for the feature, medium for safety: the pump runs longer than it should with no
flow, and the heater is on throughout.

**Fix:** the interlock's question is "may water flow here", and the answer is the list of states in
which water may flow, which includes the two states the hot-water dispense runs inside. The port's
list is `State::may_hold_water_valve_open()` in `crates/domain/src/state.rs`, and it now names
`PidNormal` and `SteamRunning` explicitly with a comment saying why. The domain's own test asserts
the *one* direction that is a safety property — a state that commands the valve must be allowed to
hold it — rather than the equality the previous version asserted, which is exactly the assertion
that would have hidden this.

### D57 — The config exporter wrote keys the importer rejects, so an export could not be re-imported

`crates/config/src/import.rs`, `export()`. The exporter emitted each parameter's **full dotted
key as a leaf name inside its group object**: `"pid": { "pid.regular.kp": 50.0 }`. The import
format, which is the same document the C++ firmware wrote and the same one
`config-export-schema.md` documents, nests: `"pid": { "regular": { "kp": 50.0 } }`. Sixteen keys
in a real export therefore named parameters the schema does not have, and because this port
rejects unknown fields rather than ignoring them — a deliberate fix, D13's family — **every
export this port produced was refused by its own importer**. The C++ did not notice, because its
importer ignored unknown fields, which is the same defect as D13 seen from the other end.

**Impact:** `/api/config/download`, the USB config export and any "save my settings" round trip
produced a file the machine itself would then reject. A user restoring their own backup would be
told the backup was corrupt.

**Severity:** high for the migration path, which is the one thing this port exists to provide.

**Fix:** the exporter emits the key relative to its group and nests the remaining segments, with
a two-level open stack so siblings share one intermediate object. Two tests hold it: the export is
parsed and validated by the same importer with zero unknown keys and zero rejections, and the
repository's own `config.json` still imports clean. A comma-state bug in the first attempt (an
intermediate object emitted `{,`) was caught by the same round-trip test, which is the argument
for having it.

### D58 — `format_version` was written by the exporter and rejected by the importer

`crates/config/src/json.rs` and `import.rs`. `config-export-schema.md` line 237 specifies
`format_version` as "top level, in the export and accepted in an import", and the exporter wrote
it. The importer had no such parameter, so the key was resolved to the dotted path
`format_version`, matched against the schema, missed, and **rejected as an unknown field**.

**Impact:** every real C++ export — which is the file a user migrating from the C++ firmware
actually has, and the only migration path this port offers — carries `format_version: 1` and was
refused outright. The repository's own `config.json` happens not to carry it, which is why the
T-06 fixture tests passed and this survived two review passes.

**Severity:** critical for the migration, and invisible to the repository's own fixtures.

**Fix:** `format_version` is a property of the document, not a setting, so it is recorded on
`ResolvedDoc` rather than inserted as a parameter. A version this firmware does not read is
refused **by name and in full**, before any field is looked at, because a document from a newer
firmware may mean something different by the same key and half-applying it is how a downgrade
destroys a configuration. A test feeds a `format_version: 2` document and asserts that nothing is
applied.


### D59 — `setHeaterDuty` was a no-op, so every heater duty was 100 percent

`include/clevercoffee/relay.h` and the heater ISR in `src/isr.cpp`. The PID computed a duty from
0 to 1000 and the firmware's duty path wrote it to a variable that the ISR read — but the relay
that switched the heater on was commanded by the state machine's actuator command, which set the
pin high regardless of the duty. The result is that a machine at temperature with the PID asking
for 30 percent had a heater running at 100 percent, and the only symptom was a boiler that takes
longer than the PID's model predicts to reach setpoint and overshoots on a cold start.

**Impact:** the PID does not control the heater's power. The temperature loop still converges,
because the integral term winds down, so this is invisible in normal operation and obvious on a
bench with a thermocouple. It is also a safety-adjacent defect: a machine whose heater cannot be
turned down is a machine whose overshoot is limited only by the loop's stability.

**Severity:** high, and of the family D05 belongs to: the heater's power stage was not actually
under the control loop that was supposed to be regulating it.

**Fix:** the port drives the heater from the MCPWM peripheral's hardware PWM at the C++ firmware's
10 ms window and 0-to-1000 duty, so the machine has **no interrupt at all** for the heater — the
D05 shape removed rather than documented. The policy is in `clevercoffee_app::heater` and is pure:
a duty becomes a timestamp, a duty above the window is clamped rather than wrapped, and the
smallest non-zero duty is one tick rather than nothing.

The fail-safe matters more than the mechanism. If the PWM peripheral cannot be brought up, the
stage is [`Stage::HeldOff`](../../crates/app/src/heater.rs) and the heater is **held off**, not
driven from the relay command. A heater with no working power control that still heats is a
machine that ignores its PID, which is the defect above; holding it off is a machine that does not
heat, which is a machine the user notices.
