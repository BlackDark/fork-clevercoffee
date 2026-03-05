# CleverCoffee — Improvements, Refactoring, Testing & CI Recommendations

This document lists concrete refactor suggestions, repository restructuring ideas, prioritized actionable changes, tests to add, CI/tooling suggestions, and guidance for power/efficiency and real-time behavior on ESP32.

## Goals for improvements

- Increase safety and determinism (critical for heater control)
- Reduce runtime failures via defensive programming and tests
- Improve testability (reduce coupling, enable host unit tests)
- Improve maintainability and code clarity (clear module boundaries)
- Optimize for ESP32 resource constraints (memory and real-time)

## Concrete refactor suggestions (small → large)

Quick wins (low risk, small changes)
1. Centralize string-use policy:
   - Replace repeated use of `String` in hot paths with fixed-size char buffers or `std::string` on host-tests paths. Use `F()` macro and `const char*` for static strings.
   - Rationale: reduce heap fragmentation and runtime OOM risks.
   - Files: `src/Logger.cpp`, `src/Config.cpp`, many `toJson`/`log` call sites.

2. Lock down ISR-ready order:
   - Add one `initializeCritical` step in `SystemInitializer` that sets `CleverCoffee::ISR::setSystemContext(&systemContext);` only after `SystemContext` and `HardwareManager` are valid.
   - Ensure `markISRReady()` is called as the last step.
   - Files: `include/clevercoffee/isr.h`, `src/isr.cpp`, `include/clevercoffee/core/SystemInitializer.h`.

3. Prevent logging from blocking:
   - Make logger non-blocking: buffer logs in an MPSC ring buffer and flush in `Logger::update()`; avoid socket writes in hot paths.
   - Files: `include/clevercoffee/Logger.h`, `src/Logger.cpp`.

4. Add runtime self-check on startup:
   - `SystemInitializer` should perform critical hardware self-tests (relay off test, sensor presence checks) and fail to safe state if any critical test fails.
   - Files: `include/clevercoffee/core/SystemInitializer.h` and implementation.

5. Harden NVS interactions:
   - Add checks and versioning for config schema and failover to defaults if deserialization fails.
   - Provide atomic update (write new key, then flip version flag) to prevent partially written configs from corrupting state.

Medium changes (refactor, test improvements)
6. Extract hardware interface traits for mocking:
   - Define pure-virtual interfaces for Relay, TempSensor, Scale, Switch and use concrete classes in `hardware/`.
   - Add mock implementations under `test/mocks` for host unit tests.
   - Files: new `include/clevercoffee/hardware/I*` interfaces; implement `HardwareManager` to hold `std::unique_ptr<IRelay>` etc.

7. Reduce `SystemContext` coupling:
   - Split `SystemContext` into smaller contexts: `ProcessContext`, `SensorContext`, `NetworkContext` and a `GlobalContext` only for critical items (ISR timer).
   - This makes dependencies smaller for unit testing.

8. Decouple logger from network and core services:
   - Provide optional log sink registration; network sink should be disabled unless explicitly configured.

9. Migrate heavy JSON operations to bounded buffers:
   - Replace dynamic `DynamicJsonDocument` uses with `StaticJsonDocument` sized conservatively.
   - Files: `src/Config.cpp`, web server handlers.

Large changes (architectural)
10. Move PID compute to a deterministic, small RTOS task or keep minimal in ISR pipeline:
    - Option A: keep ISR PWM only and compute PID on a high-priority RTOS task with fixed tick rate.
    - Option B: compute PID in ISR if compute is small and IRAM-safe (not recommended).
    - Rationale: correctness and deterministic timing; avoid computation in main loop causing jitter.

11. Introduce a hardware watchdog and heartbeat system:
    - Monitor main-loop health and ISR toggles; if checks fail, perform safe shutdown (turn relays off) and optionally trigger external hardware reset.

12. Introduce a small HAL layer and move board-specific pin mapping into per-board files:
    - Simplify supporting multiple board configs and make unit tests board-agnostic.

## Proposed repository restructuring (if desired)

- `src/` current layout is largely fine. Suggested changes:
  - Move header-only APIs and interfaces to `include/clevercoffee/public/` vs internal headers to `include/clevercoffee/internal/`.
  - Add `hal/` or `board/` directory for pin mapping and board-specific definitions.
  - Create `tests/host/` for host-only unit tests that don't need Arduino mocks and `tests/device/` for PlatformIO device tests.

Rationale: clearer separation between public APIs, internal implementation, and board-specific code.

## Prioritized actionable list (ordered by impact & cost)

1. (Quick win) Replace dynamic Arduino `String` in logging and frequent JSON outputs with fixed buffers and `StaticJsonDocument` (High value / Low work)
2. (Quick win) Enforce ISR initialization ordering — ensure `ISR::setSystemContext` and `markISRReady` run only after hardware is safe (Low risk)
3. (Medium) Add ring-buffered non-blocking Logger sink and disable WiFi logging by default (Medium effort, prevents runtime stalls)
4. (Medium) Add unit tests for:
   - `Config::loadAll()`, `saveAll()`, `importFromJson()` edge cases
   - `SensorCoordinator` update/timing behavior with mocks
   - `ProcessController` PID behavior via simulated sensor inputs
5. (Medium) Introduce interfaces for hardware components to enable host-unit testing with mocks (Medium effort)
6. (Large) Split `SystemContext` to smaller contexts and reduce coupling (High effort; large payoff for testability)
7. (Large) Introduce CI with PlatformIO builds, static analysis, and host unit tests (High effort, necessary for long-term reliability)

## Suggested tests to add and placement

- Unit tests (host):
  - `unit/test_config.cpp` — test ParamDef min/max, `fromString()` and NVS mock interactions (use in-memory or file-based stub).
  - `unit/test_pid.cpp` — unit test of PID math (simulate temperature updates).
  - `unit/test_state_machine.cpp` — assert transitions between states with mocked `HardwareManager`.
  - Location: `unit/` or `test/host/` with PlatformIO native environment.

- Integration tests (device):
  - `test/integration/temperature_loop` — run on ESP32, simulate sensor change and assert heater relay response via pin reads.
  - `test/integration/handlers` — exercise BrewHandler and HotWaterHandler on device.

- Safety tests:
  - `test/safety/emergency_stop` — simulate over-temp/overpressure and assert immediate heater off and emergency flag set.

- CI-local tests:
  - Use `pio test -e native` (host) and `pio run -e esp32_usb` (compile-only) in GitHub Actions. For hardware tests, run on a self-hosted runner with an attached ESP32.

## CI and tooling suggestions

Suggested GitHub Actions workflow (stages):
1. Static checks (on every PR):
   - `clang-format` check (enforce style)
   - `cppcheck` and basic `clang-tidy` runs (on selected translation units)
   - `platformio ci` style "pio check" / build for `esp32dev` and `esp32_usb`.
2. Build matrix:
   - Build for configured board(s) using `pio run -e <env>`.
   - Run `pio test -e native` (unit tests on host).
3. Optional Hardware tests (on push to main or release):
   - Self-hosted runner with physically connected test harness runs `pio test -e esp32_usb --target test --environ <board>` or `pio test -e <board>`.
4. Notifications: comment PR with build/test results.

Tooling:
- Static analysis: `clang-tidy`, `cppcheck`.
- Format: `clang-format` enforced by CI.
- Lint: `cpplint` for stylistic checks (optional, adapt to project style).
- Memory/heap monitoring: Add runtime logging endpoints to dump heap stats (`ESP.getFreeHeap()` and heap caps).
- Bill of materials: generate a `README` entry for hardware pins & electrical safety notes.

## Power/efficiency and real-time guidance

- Prefer deep sleep and wake cycles for power saving only if machine design allows (some machines need persistent monitoring—carefully evaluate).
- Avoid high-frequency timers that compete for CPU on ESP32 if not required.
- For energy-critical builds:
  - Shutdown WiFi when idle for extended time; re-enable only on user interaction.
  - Reduce display brightness and disable unnecessary animations.
  - Use efficient timers (hardware timers) and minimize software RTOS tasks.

Real-time:
- ISR must be IRAM-resident; avoid complex function calls inside ISR.
- Keep ISR to toggle hardware and adjust counters; heavy computation should be in a high-priority RTOS task or the main loop with bounded latency.
- Use tens-of-ms resolution for control unless control loop demands faster update rates.

## Example migration & testability steps (concrete plan)

1. Add non-blocking logger ring buffer and disable WiFi sink by default.
2. Add unit-testable hardware interface classes and update `HardwareManager` to use them.
3. Create host-level mocks under `test/mocks` and add `native` tests for `ProcessController`, `SensorCoordinator`, and `StateMachine`.
4. Add `clang-format` and `clang-tidy` configs and enforce them in CI.
5. Add runtime assertions and a health-check endpoint (web or serial) for watchdog checks.

## File-by-file short notes (where to change / focus)

- `src/main.cpp` — ensure safe ordering: SystemContext created -> set ISR pointer -> system mark ready -> enable timers.
- `include/clevercoffee/isr.h` & `src/isr.cpp` — verify IRAM placement and keep functions minimal.
- `include/clevercoffee/Config.h` and `src/Config.cpp` — add schema versioning and safe import fallback.
- `include/clevercoffee/Logger.h` and `src/Logger.cpp` — implement ring-buffered sink and non-blocking update.
- `include/clevercoffee/hardware/HardwareManager.h` — create interface abstractions for hardware pieces to ease mocking.
- `include/clevercoffee/context/SystemContext.h` — consider splitting or narrowing public surface for lower coupling.

## Final notes & assumptions

- I assumed the ESP32 toolchain supports C++17 or later; if not, adjust accordingly (reduce use of some language features).
- Where runtime behavior is involved (timers/ISR), the code was read from headers and core source files provided; verify any platform-specific API usage (`timerBegin`, `timerAttachInterrupt`) with the target SDK version.
- This list balances safety and practicality: quick changes first, deeper refactors later.
