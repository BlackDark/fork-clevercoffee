# CleverCoffee — Architecture Overview

Status: snapshot of repository at /Users/marbaced/projects/forks/fork-clevercoffee (2026-02-08)

This document gives a high-level architecture overview, module responsibilities, interfaces, build/runtime notes for ESP32/PlatformIO, testing strategy, safety and performance guidance, and example interaction sequences.

## High-level project summary and goals

CleverCoffee is an ESP32-based controller for a coffee/espresso machine. Goals:
- Reliable, safety-first control of heater, pump, valves and sensors.
- Testable, modern C++ code using RAII, dependency injection (SystemContext), and modular coordinators.
- Deterministic PID control for heater output using a hardware timer ISR.
- Configurable behavior and persistent settings using a type-safe Config system (NVS).
- Network integration (WiFi, MQTT), local Web UI, display support and handlers for machine functions (brew, hot water, steam, power).

Primary runtime constraints:
- Runs on ESP32 (Arduino framework + PlatformIO). Real-time parts (heater PWM) are implemented in IRAM via hw-timer ISR.
- Memory and real-time constraints require careful allocation and hardware-aware design.

## Directory layout and purpose

Top-level important folders:
- `src/` — application source code and module implementations (C++ files).
  - `src/main.cpp` — program entry: constructs `SystemInitializer` and top-level managers, creates `StateMachine`, `ProcessController`, `LoopManager`.
  - `src/isr.cpp` — ISR accessor and helper functions for timer initialization. Contains ISR code controlling heater relay.
  - `src/Config.cpp`, `src/Logger.cpp`, `src/ota.cpp`, etc.
  - subfolders: `context/`, `coordinators/`, `core/`, `hardware/`, `handlers/`, `state/`, `sensors/`, `network/`, `display/`, `ui/`, `control/`.
- `include/clevercoffee/` — public headers describing APIs and interfaces, e.g.:
  - `core/` — `SystemInitializer.h`, `LoopManager.h`
  - `context/` — `SystemContext.h` (central), `HardwareContext.h`, `ProcessState.h`, `SensorState.h`, `TimingState.h`
  - `coordinators/` — `SensorCoordinator.h`, `NetworkCoordinator.h`, `UICoordinator.h`, `StandbyCoordinator.h`
  - `hardware/` — `HardwareManager.h`, `Relay.h`, `TempSensor*.h`, `GPIOPin.h`, `Switch.h`, `pinmapping.h`
  - `state/` — state machine abstractions and IDs
  - `types/` — shared type definitions and enums
  - `utils/` — helpers, timers, memory utils
- `test/` and `unit/` — test harness, Arduino mocks, unit tests and integration tests.
- `docs/` — documentation (to be added/extended).
- `platformio.ini` — build configuration for ESP32.

## Key modules and responsibilities

Below is per-module description with key public interfaces and interactions.

### SystemInitializer (include/clevercoffee/core/SystemInitializer.h; implemented in `src/`)

Responsibility:
- RAII wrapper that performs full system initialization: logger, config, display, hardware, networking, MQTT, PID, sensors.
- Owns many managers (DisplayManager, HardwareManager, WiFiManager, MQTTManager, SystemContext).
- Exposes getters for managers (getDisplayManager(), getHardwareManager(), getSystemContext(), getWiFiManager(), getMQTTManager()).

Important functions:
- `SystemInitializer()` constructor
- `bool initialize()` — performs init steps (detailed in header)
- `bool finalizeMachineState()` — finalize machine state after StateMachine constructed

Interactions:
- Called from `main::setup()`. After initialization, `main.cpp` constructs `StateMachine`, `ProcessController`, and `LoopManager` using references from `SystemInitializer`.

Notes:
- Accessors frequently log `LOG(FATAL)` if a required component is missing; callers must check `isInitialized()` before usage.

### SystemContext (include/clevercoffee/context/SystemContext.h; central)

Responsibility:
- Central shared state container and service locator.
- Contains coordinators (SensorCoordinator, NetworkCoordinator, UICoordinator, StandbyCoordinator).
- Holds non-owning pointers to handlers, controllers, network managers, and unique_ptr timers required at runtime.
- Provides process-level getters/setters: temperature, setpoint, PID output, brew timers.
- Contains ISR-related members: `timing_isrCounter_`, `timing_isrReady_`, and `machineTimer()` => used directly by ISR.

Key public API (examples):
- Coordinator access:
  - `SensorCoordinator& sensorCoordinator()`
  - `NetworkCoordinator& networkCoordinator()`
  - `UICoordinator& uiCoordinator()`
- Handler registration:
  - `void setBrewHandler(BrewHandler*)`, `BrewHandler& brewHandler()`
- Process and PID accessors:
  - `double processTemperature()`, `void setProcessTemperature(double)`, `void computePid()`, `void setPidTunings(...)`, `void setPidMode(int)`
- ISR and timer:
  - `hw_timer_t* machineTimer()`, `void setMachineTimer(hw_timer_t*)`, `void markISRReady()`, `bool isISRReady()`, `unsigned int isrCounter()`, `void setIsrCounter(unsigned int)`

Interactions:
- Distributed to nearly every major subsystem (HardwareManager, ProcessController, LoopManager, state machine).
- ISR fetches `SystemContext` via `CleverCoffee::ISR::getSystemContext()` (safe static pointer set during initialization).

Design notes:
- Provides explicit dependency injection across modules while keeping quick global access. This improves testability but centralizes many concerns in a single type — watch for high coupling.

### HardwareManager (include/clevercoffee/hardware/HardwareManager.h)

Responsibility:
- RAII manager for hardware: relays, LEDs, switches, temp sensors, scale.
- Implements `IHardwareContext` for `StateMachine` and handlers to operate hardware without owning internals.

Key API:
- `Relay* getHeaterRelay()`, `Relay* getPumpRelay()`, `TempSensor* getTempSensor()`, `Scale* getScale()`
- Methods for safe shutdown, `void safeShutdown()`, `void updateLEDs(MachineStateId, double, double)`.
- Heater state tracking: atomic `heaterEnabled_` to coordinate with ISR-led direct relay toggling.

Interactions:
- Created by `SystemInitializer`, used by `StateMachine`, `ProcessController`, `LoopManager`, and handlers.
- Heater control is partially done by ISR (PID PWM) directly on `Relay` objects; `HardwareManager` provides high-level control usable outside ISR (pump/valve etc).

Safety-critical notes:
- `cleanupPartialInit()` ensures relays are turned off if initialization fails.
- `heaterEnabled_` is atomic but is only approximate because ISR modifies the physical relay directly.

### SensorCoordinator (include/clevercoffee/coordinators/SensorCoordinator.h)

Responsibility:
- Polls sensors (temperature, scale, pressure, water-tank) on configurable intervals, caches values for fast read, performs timeouts and error tracking, supports tare and calibration modes.
- Non-blocking `update()` called from loop manager.

Key API:
- `void update() noexcept`
- Temperature: `double getTemperature()`
- Scale: `double getWeight()`, `void startBrewWeightTracking()`
- Pressure: `float getFilteredPressure()`

Interactions:
- Owned by `SystemContext`. `LoopManager` calls `sensorCoordinator().update()` regularly; `ProcessController` and `StateMachine` read cached values from it.

### ProcessController (src/control/ProcessController.* and include)

Responsibility:
- Implements PID control logic (aggressive/regular modes), manages brew timers and process transitions required for brewing and hot-water/steam operations.
- Uses `SystemContext` and `HardwareManager` for sensor inputs and actuator outputs.

Interactions:
- Instantiated in `main.cpp` and registered into `SystemContext` (non-owning pointer).
- Works with `LoopManager` and `SensorCoordinator` for periodic computation.
- PID output is stored in `SystemContext` and consumed by ISR for heater PWM.

### StateMachine and Handlers

Responsibility:
- `StateMachine` manages high-level machine state transitions (Idle, Heating, Brewing, Steam, Standby, Emergency) and uses handlers (BrewHandler, PowerHandler, HotWaterHandler, SteamHandler) to perform actions per state.

Key files:
- `src/state/StateMachine.*`, `include/clevercoffee/state/` (interfaces and IDs)
- `src/handlers/*` — per-feature handler implementations

Interaction:
- `StateMachine` depends on `SystemContext`, `HardwareManager`, `DisplayManager`, and network managers (WiFi/MQTT) to decide transitions and to issue commands.

### LoopManager (include/clevercoffee/core/LoopManager.h; src implementation)

Responsibility:
- Single place to coordinate periodic tasks: sensor updates, process controller updates, UI polling, network tasks, display refreshes.
- Designed to centralize the main loop rather than rely on ad-hoc `loop()` code.

API:
- `bool initialize()`, `void update()`, `void configureSensorTimers(...)` (sample rate tuning)

### Logger (include/clevercoffee/Logger.h, src/Logger.cpp)

Responsibility:
- Central logging with levels, multiple sinks (Serial, optional WiFi server/client).
- `Logger::logf()` and `LOGF()` macros are pervasive.

Important notes:
- Logger can open a WiFi server when WiFi is connected — this couples logging with network stack; may be undesirable for safety-critical logs.

### Config (include/clevercoffee/Config.h, src/Config.cpp)

Responsibility:
- Type-safe runtime configuration API with templated `ParamDef<T>` and `EnumParamDef<E>`, automatic NVS persistence (`Preferences`), JSON import/export via ArduinoJson.
- Central access via `Config::getInstance()` (singleton-like).

Notes:
- Uses `String` heavily; consider memory and fragmentation effects in long-running embedded apps.

### ISR and Real-time control (include/clevercoffee/isr.h, src/isr.cpp)

Responsibility:
- Timer ISR runs every 10ms to implement PWM-based heater control. It:
  - Safely obtains `SystemContext` via `CleverCoffee::ISR::getSystemContext()`.
  - Reads `processPidOutput()` and `isrCounter()` from `SystemContext`.
  - Directly toggles heater `Relay::on()`/`off()` (hardware-level).
  - Uses IRAM_ATTR for ISR placement and `timerAlarmWrite`/`timerAlarmEnable`.

Safety points:
- ISR performs pointer checks and `isISRReady()` guards.
- `heaterEnabled_` in `HardwareManager` is atomic but may be approximate because ISR toggles hardware directly.

## Build and runtime instructions (ESP32 / PlatformIO)

PlatformIO is used as build system. Typical commands (run locally in terminal):

- Build:
```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
```
- Build verbose:
```bash
~/.platformio/penv/bin/pio run -e esp32_usb
```
- Format code:
```bash
~/.platformio/penv/bin/pio run --target format -e esp32_usb -s
```

Important compile-time flags and considerations:
- Ensure proper C++ standard in `platformio.ini` (project seems modern C++-oriented — use at least `-std=gnu++17` or later).
- For ISR-critical code, functions expected to be in IRAM must be annotated with `IRAM_ATTR`. Confirm `timerBegin`, `timerAttachInterrupt`, and `timerAlarmWrite` calls match ESP32 API expectations.
- Configure optimization and inline settings for ISRs; avoid heavy code or function calls that are not IRAM safe in ISR.
- If using `std::vector` and `std::string` features note their heap use; prefer fixed-size data structures in ISR-context.

Runtime considerations:
- Memory: minimize dynamic allocations, especially in frequently-called code. Monitor heap fragmentation (String usage + ArduinoJson).
- ISR constraints:
  - Keep ISR code small and in IRAM.
  - Do not call blocking APIs from ISR (no Serial, no network, no malloc).
  - Only perform atomic or lock-free updates to shared variables or use volatile/atomic types.
- Concurrency:
  - `SystemContext` uses atomics for flags accessed by ISR (e.g., `timing_isrReady_`, `timing_isrCounter_`).
  - Coordinators and LoopManager run in main loop; avoid long-blocking operations.
- ISR initialization:
  - The static pointer in `clevercoffee::ISR` is set once (must be set before ISR activation). `SystemInitializer` must set `CleverCoffee::ISR::setSystemContext(&systemContext)` before enabling timer.

## Testing strategy

Existing tests are under `test/` and `unit/` with Arduino mocks.

Recommended test tiers:

1. Unit tests (host and cross-compiled)
   - Focus: business logic (Config parsing/validation), PID math, data transformations, state machine transitions.
   - Use: GoogleTest or Catch2 with PlatformIO test runner. Many modules can be tested by linking with test harnesses/mocks found in `test/`.
   - Use dependency injection to provide fake `HardwareManager`/`SensorCoordinator`/`SystemContext` for testing state transitions.
   - Place tests in `unit/` and platformio test environments referencing `test/mocks`.

2. Integration tests (native emulator + hardware-in-the-loop)
   - Use PlatformIO `pio test -e esp32_usb` for integration test execution on device.
   - Tests verifying hardware I/O should run on real ESP32 hardware (CI can run on a hardware runner or developer machines).

3. Hardware-In-The-Loop (HIL)
   - Use a bench ESP32 attached to CI runner or local dev machine.
   - Use mocked sensors (or breakout boards) to feed repeatable inputs (pressure, temperature).
   - Validate safety paths (emergency stop, sensor failure) and power-down behavior.

4. Regression tests
   - Snapshot display rendering JSON, important NVS config read/write, and PID regression.

Recommended tooling and CI:
- GitHub Actions pipeline matrix:
  - Host checks: `clang-format` check, `clang-tidy` (C++ static analysis) on sample translation units, `cppcheck`.
  - Build checks: PlatformIO build for target board(s) (esp32dev / esp32_usb) using `pio run`.
  - Unit tests: `pio test -e native` or `pio test` for host tests; `pio test -e esp32*` on hardware runner.
  - Optionally run `pio check` with static analysis options, and run `python -m unittest` if Python test scripts exist.

Notes on sanitizers:
- AddressSanitizer/ThreadSanitizer are typically unavailable for bare-metal ESP32 builds. Use host-based tests (native builds) with ASan as much as possible.

## Safety considerations and failure modes

Critical safety surfaces:
- Heater control (ISR toggles relay)
- Emergency stop and over-temperature detection
- Water tank empty detection
- Hardware initialization failures and safe shutdown

Potential failure modes & mitigation strategies:

1. ISR runs before SystemContext ready (danger):
   - Guard: `SystemContext::isISRReady()` and `CleverCoffee::ISR::getSystemContext()` checks in ISR (`isr.h`/`isr.cpp`). Ensure `markISRReady()` is called only after `SystemContext` and hardware are fully initialized.
   - Mitigation: keep `timing_isrReady_` atomic and default false.

2. Heater relay stuck on / hardware failure:
   - Mitigation: ensure `HardwareManager::cleanupPartialInit()` forces relays off in destructor and on critical path; design emergency watchdog (external or software) that toggles hardware to off if heartbeat missing.
   - Add hardware-level fuse/thermal cutout independent of MCU.

3. NVS or config corruption:
   - Mitigation: `Config::loadAll()` logs failures and falls back to defaults. Provide `resetAllToDefaults()` and fail-safe behaviors if critical parameters missing (e.g., disable PID, safe setpoints).

4. Network or logging interference with timing:
   - Mitigation: logging over WiFi should be optional and not performed from ISRs. Keep logging non-blocking; use circular buffers for logs if necessary.

5. Memory fragmentation (String, ArduinoJson):
   - Mitigation: prefer `StaticJsonDocument` with bounded sizes, pre-allocate long-lived buffers, avoid repeated `String` concatenations. Consider migrating heavy JSON handling to host or less frequent operations.

6. Blocking I/O or long-running tasks in main loop:
   - Mitigation: `LoopManager` should schedule tasks and avoid blocking calls. Use small non-blocking tasks and time slices.

7. Unexpected exceptions:
   - Mitigation: use RAII and ensure destructors call safeShutdown. Avoid use of exceptions in code compiled for Arduino/ESP32 (exceptions cost and may be disabled). If exceptions are used, ensure try/catch boundaries in non-ISR code.

8. ISR not IRAM-resident:
   - Mitigation: mark ISR functions with `IRAM_ATTR` and keep code small; avoid calls to non-IRAM functions or dynamic memory from ISR.

## Performance considerations and tips for ESP32

- Minimize dynamic memory allocation (heap) and String usage:
  - Replace heavy `String` usage with `std::string` or char buffers where possible, or use `F()` / PROGMEM for static strings.
  - Configure ArduinoJson `StaticJsonDocument` with a safe, bounded size for repeated operations.

- ISR and IRAM:
  - Keep ISR short; only read atomic/volatile state and toggle pinned hardware.
  - Place ISR and any functions it calls in IRAM (`IRAM_ATTR`) to avoid flash cache misses.

- Heap caps and memory allocation:
  - Use ESP32 heap caps (e.g., MALLOC_CAP_8BIT, MALLOC_CAP_32BIT) for allocation-sensitive operations.
  - Profile heap usage at runtime; add debug endpoints for heap stats (Heap caps logging functions referenced in `utils/memoryUtils.h`).

- Avoid syscalls or blocking OS primitives in ISR or timing-sensitive code.
- Use atomic operations for small shared flags (already used for some fields in SystemContext and HardwareManager).

## Interaction/sequence examples (ASCII diagrams)

Startup sequence (simplified):
```
main.setup()
  -> SystemInitializer ctor
     -> initializeLogger()
     -> initializeConfiguration() (Config::begin())
     -> initializeHardware() (HardwareManager constructed)
     -> create SystemContext (holds coordinators)
     -> set ISR context (CleverCoffee::ISR::setSystemContext(&systemContext))
     -> initializeSensors()
     -> mark systemInitialized_ = true
  -> main: create StateMachine(systemContext, hardwareManager, displayManager, wifiManager, mqttManager)
  -> main: stateMachine->initialize()
  -> systemContext.setMachineStateContext(&stateMachine->getContext())
  -> systemInitializer->finalizeMachineState()
  -> create ProcessController(Config, systemContext, hardwareManager, displayManager, mqttManager)
  -> systemContext.setProcessController(processController.get())
  -> create LoopManager(systemContext, hardwareManager, *processController, sensorCoordinator, uiManager)
  -> loop() runs: loopManager->update()
```

Heating control flow (simplified):
```
Loop:
  LoopManager.update()
    -> SensorCoordinator.update()
       -> updateTemperature() -> cachedTemperature_
    -> ProcessController.update()
       -> compute desired PID output -> systemContext.setProcessPidOutput()
ISR (10ms):
  onTimer()
    ctx = CleverCoffee::ISR::getSystemContext()
    if !ctx->isISRReady() => return
    // PWM logic
    read pidOutput = ctx->processPidOutput()
    read counter = ctx->isrCounter()
    if pidOutput <= counter: relay->off() else relay->on()
    increment counter and ctx->setIsrCounter(new)
```

## Where to look in code (pointers)

- Entry point & orchestration: `src/main.cpp`
- Initialization lifecycle: `include/clevercoffee/core/SystemInitializer.h`, `src/*` implementation
- System state & coordinators: `include/clevercoffee/context/SystemContext.h`, `include/clevercoffee/coordinators/*`
- Hardware abstraction: `include/clevercoffee/hardware/HardwareManager.h`
- ISR: `include/clevercoffee/isr.h`, `src/isr.cpp`
- Configuration: `include/clevercoffee/Config.h`, `src/Config.cpp`
- Logger: `include/clevercoffee/Logger.h`, `src/Logger.cpp`
- Handlers: `src/handlers/*`
- Tests and mocks: `test/`, `unit/`

## Assumptions

- PlatformIO configuration supports required C++ standard (>= C++17).
- External hardware safety protections (fuses, thermostats) exist in the physical machine; code is an additional mitigation layer.
- The code uses Arduino/ESP32 APIs; certain modern C++ features may not be available in the ESP32 toolchain (per `CLAUDE.md`).
- Some implementation details (function internals) were inferred from headers and selected source files (`main.cpp`, `Config.cpp`, `isr.cpp`, `Logger.cpp`), not exhaustively from every file.

## Recommendations summary (see improvement document for prioritized action list)
- Keep ISR minimal, statically check IRAM placement, ensure systemContext-safe initialization ordering.
- Add more unit tests around `ProcessController`, `StateMachine`, and `Config` persistence logic.
- Reduce usage of `String` and heap-heavy dynamic allocations; use `StaticJsonDocument` or pre-allocated buffers.
- Isolate hardware interfaces behind testable abstractions to allow host unit tests.
```
