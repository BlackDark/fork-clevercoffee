# Implementation Plan: CleverCoffee Architecture Improvement

**Project**: CleverCoffee — ESP32, C++20/23, PlatformIO, Google Test
**Date**: 2026-03-05
**Phases**: 8
**Estimated Total**: 16–28 hours

---

## Phase 1: Safety-Critical Fixes

**Objective**: Eliminate undefined behavior and memory safety issues in production code paths.

**Files to Modify**:
- `src/network/MQTTManager.cpp` — VLA removal, sscanf field-width limits
- `include/clevercoffee/context/SystemContext.h` — Make ISR counter `std::atomic`
- `src/context/SystemContext.cpp` — Update ISR counter accessors for atomic usage

**Changes**:

1. **Fix VLA in MQTT callback** (MQTTManager.cpp:236):
   Replace `char data_str[length + 1]` with a fixed-size buffer (e.g., `char data_str[512]`) plus a bounds check that rejects messages exceeding the buffer. VLAs on the stack are non-standard C++ and can overflow the stack on malicious input.

2. **Add `sscanf` field-width limits** (MQTTManager.cpp:247):
   Change the `snprintf` format pattern to produce `%119[^/]` and `%63[^/]` (matching `configVar[120]` and `cmd[64]` buffer sizes minus 1). Currently unbounded `%[^/]` can overflow both `configVar` and `cmd`.

3. **Make ISR counter atomic** (SystemContext.h:1403):
   Change `unsigned int timing_isrCounter_` → `std::atomic<unsigned int> timing_isrCounter_{0}`. The ISR increments this from interrupt context while the main loop reads it — this is a textbook data race. Update `getIsrCounter()`, `setIsrCounter()`, `incrementIsrCounter()` to use appropriate memory ordering (`memory_order_relaxed` is sufficient for a counter).

**Tests**:
- Unit test for MQTT `messageCallback` with oversized payload (verify truncation/rejection)
- Unit test for MQTT `messageCallback` with oversized topic segments (verify no overflow)
- Verify existing test suite still passes

**Acceptance Criteria**:
- No VLA in codebase (`rg 'char.*\[.*\+' src/` returns no dynamic-length stack arrays)
- `sscanf` calls have field-width specifiers matching their destination buffers
- `timing_isrCounter_` is `std::atomic<unsigned int>`
- `pio run -e esp32_usb -s` compiles clean
- `pio test -e native_test` passes

**Dependencies**: None

**Estimated Time**: 1–2 hours

---

## Phase 2: Handler Ownership & `noexcept` Safety

**Objective**: Replace raw `new` with `std::make_unique` for handlers and fix `noexcept` correctness on accessor methods that dereference raw pointers.

**Files to Modify**:
- `src/context/SystemContext.cpp` — Replace raw `new` with `std::make_unique` for handler creation
- `include/clevercoffee/context/SystemContext.h` — Change handler storage from raw pointers to `std::unique_ptr`; remove `noexcept` from accessors that dereference nullable pointers (or add assertions)
- `src/main.cpp` — Adapt any handler ownership transfer if needed

**Changes**:

1. **Replace raw `new` with `std::make_unique`** in `initializeHandlers()` (SystemContext.cpp:596-599):
   Convert the file-static raw pointers `brewHandler`, `hotWaterHandler`, `powerHandler`, `steamHandler` into `std::unique_ptr<T>` and create via `std::make_unique<T>(systemContext)`. Pass raw pointers to `setXxxHandler()` via `.get()`.

2. **Fix `noexcept` on nullable accessors** (SystemContext.h):
   The `brewHandler()`, `hotWaterHandler()`, `powerHandler()`, `steamHandler()` accessors are marked `noexcept` but dereference raw pointers that could be null before `initializeHandlers()` is called. Two options:
   - **Option A** (preferred): Keep `noexcept`, add `assert(ptr != nullptr)` with a comment documenting the precondition. These pointers are always set during init before any access.
   - **Option B**: Return `T*` instead of `T&` and let callers null-check.

   Apply the same reasoning to `processController()` and `machineStateContext()` which return raw pointers and are already `noexcept` — these are fine as-is since returning null is valid.

**Tests**:
- Verify handler construction and destruction with ASAN (address sanitizer) if available
- Existing tests should pass unchanged

**Acceptance Criteria**:
- Zero raw `new` for handler allocation in SystemContext.cpp
- Handler lifetime managed by `std::unique_ptr`
- `pio run -e esp32_usb -s` compiles clean
- `pio test -e native_test` passes

**Dependencies**: None (independent of Phase 1, can be done in parallel)

**Estimated Time**: 1–2 hours

---

## Phase 3: Test Infrastructure — IConfig Interface & Simplified Mocks

**Objective**: Create testability infrastructure that unblocks all subsequent test-enablement phases.

**Files to Modify**:
- `test/mocks/MockConfig.h` — Extend or rewrite to implement a clean `IConfig` interface
- `test/mocks/MockRelay.h` — Simplify to remove GPIOPin dependency (pure mock)
- `test/test_support.h` — Add config cleanup helpers for test fixtures

**Files to Create**:
- `include/clevercoffee/IConfig.h` — Abstract interface extracted from `Config` for the subset of methods used by handlers, coordinators, and states
- `test/mocks/MockProcessController.h` — Mock for ProcessController
- `test/mocks/MockStateMachine.h` — Mock for StateMachine
- `test/mocks/MockSystemContext.h` — Extend existing mock or create comprehensive version

**Changes**:

1. **Extract `IConfig` interface**: Identify the Config methods actually used by handlers and coordinators (primarily getters via `ParamDef::get()`). Create a pure virtual interface `IConfig` with those methods. Have the existing `Config` class inherit from `IConfig`. This does **not** require changing any production code that uses `Config::getInstance()` yet — that comes in Phase 5.

2. **Simplify `MockRelay`**: The current `MockRelay` in `test/mocks/MockRelay.h` doesn't inherit from `Relay` (it's standalone). Verify it has the needed interface (`turnOn()`, `turnOff()`, `isOn()`) without requiring GPIO initialization. If it already works standalone, confirm and document.

3. **Create missing mocks**: `MockProcessController`, `MockStateMachine`, `MockSystemContext` — these are needed by handler and state tests. Use Google Mock (GMock) `MOCK_METHOD` macros or simple fakes.

4. **Config cleanup in test fixtures**: Ensure `Config::getInstance()` can be reset between test cases (call `Config::getInstance().loadDefaults()` or equivalent in `SetUp()`/`TearDown()`).

**Tests**:
- Compile-test all new mocks
- Write a simple test using `MockProcessController` and `MockStateMachine` to verify they work

**Acceptance Criteria**:
- `IConfig` interface exists and `Config` inherits from it
- `MockProcessController`, `MockStateMachine` exist and compile
- `MockRelay` works without GPIO dependency in native tests
- `pio test -e native_test` passes

**Dependencies**: None

**Estimated Time**: 2–4 hours

---

## Phase 4: Enable Handler Tests (P0 Coverage)

**Objective**: Enable the disabled handler tests (BrewHandler, SteamHandler, HotWaterHandler, PowerHandler) — these are safety-critical components with 0% coverage.

**Files to Modify**:
- `test/test_brew_handler/test_main.cpp` — Remove `DISABLED_` prefix, fix compilation
- `test/test_steam_handler/test_main.cpp` — Remove `DISABLED_` prefix, fix compilation
- `test/test_hot_water_handler/test_main.cpp` — Remove `DISABLED_` prefix, fix compilation
- `test/test_power_handler/test_main.cpp` — Remove `DISABLED_` prefix, fix compilation (if it exists; otherwise create)

**Changes**:

1. For each handler test file:
   - Remove the `DISABLED_` prefix from test names
   - Update test fixtures to use the mocks from Phase 3 (`MockSwitch`, `MockRelay`, `MockSystemContext`)
   - Ensure handlers can be constructed with mock dependencies
   - Fix any compilation issues (missing includes, API changes)

2. **Add new tests** for critical handler behavior:
   - `BrewHandler`: Switch press detection, brew start/stop, valve control
   - `SteamHandler`: Switch detection, steam mode activation
   - `HotWaterHandler`: Hot water activation, null switch safety
   - `PowerHandler`: Power switch handling

**Tests**: All 4 handler test suites should be fully enabled and passing.

**Acceptance Criteria**:
- Zero `DISABLED_` tests in handler test files
- All handler tests pass: `pio test -e native_test -f test_brew_handler -f test_steam_handler -f test_hot_water_handler -f test_power_handler`
- Each handler has at least 3 meaningful test cases

**Dependencies**: Phase 3 (needs mocks)

**Estimated Time**: 2–4 hours

---

## Phase 5: Inject Config by Reference into Handlers & Coordinators

**Objective**: Begin eliminating `Config::getInstance()` calls from handlers and coordinators by injecting `IConfig&` via constructor.

**Files to Modify**:
- `include/clevercoffee/handlers/BrewHandler.h` — Add `IConfig&` constructor parameter
- `include/clevercoffee/handlers/SteamHandler.h` — Add `IConfig&` constructor parameter
- `include/clevercoffee/handlers/HotWaterHandler.h` — Add `IConfig&` constructor parameter
- `include/clevercoffee/handlers/PowerHandler.h` — Add `IConfig&` constructor parameter
- `src/handlers/*.cpp` — Replace `Config::getInstance()` with injected reference
- `src/context/SystemContext.cpp` — Pass `Config::getInstance()` to handler constructors
- `include/clevercoffee/coordinators/SensorCoordinator.h` — Add `IConfig&` if it uses Config directly
- `src/coordinators/SensorCoordinator.cpp` — Replace `Config::getInstance()` calls

**Changes**:

1. **Add `IConfig&` to handler constructors**: Each handler already takes a `SystemContext&`. Add `IConfig& config` as a second parameter (or extract from `SystemContext` if SystemContext already holds a config reference). Store as `IConfig& config_` member.

2. **Replace `Config::getInstance()` calls** in handler .cpp files with `config_.methodName()`.

3. **Update `initializeHandlers()`** to pass config reference to each handler constructor.

4. **Update handler tests** to pass `MockConfig` or real `Config` instance.

**Tests**:
- All handler tests from Phase 4 still pass with injected config
- Add tests that verify handlers use the injected config (e.g., mock config returns custom setpoint, verify handler uses it)

**Acceptance Criteria**:
- Zero `Config::getInstance()` calls in handler source files
- Handlers accept `IConfig&` in constructor
- `pio run -e esp32_usb -s` compiles clean
- `pio test -e native_test` passes

**Dependencies**: Phase 3 (IConfig interface), Phase 4 (handler tests)

**Estimated Time**: 2–3 hours

---

## Phase 6: Inject StateMachine into LoopManager & Enable State Tests

**Status**: ✅ COMPLETED (Part A — StateMachine injection). Part B deferred — all state implementation tests are empty stubs with no test logic; enabling them requires writing new test bodies and mocking MachineStateContext + handler dependencies.

**Objective**: Remove the `extern std::unique_ptr<StateMachine>` hack from LoopManager and enable disabled state implementation tests.

**Files to Modify**:
- `src/core/LoopManager.cpp` — Remove extern hack, accept StateMachine via constructor or setter
- `include/clevercoffee/core/LoopManager.h` — Add StateMachine dependency
- `src/main.cpp` — Wire StateMachine into LoopManager
- `test/test_state_implementations/test_brew_states/test_main.cpp` — Enable tests
- `test/test_state_implementations/test_steam_states/test_main.cpp` — Enable tests
- `test/test_state_implementations/test_pid_states/test_main.cpp` — Enable tests
- `test/test_state_implementations/test_system_states/test_main.cpp` — Enable tests
- `test/test_state_implementations/test_error_states/test_main.cpp` — Enable tests
- `test/test_state_implementations/test_backflush_states/test_main.cpp` — Enable tests

**Changes**:

1. **Inject StateMachine into LoopManager**: Add `StateMachine&` (or `StateMachine*`) as a constructor parameter to `LoopManager`. Remove `extern std::unique_ptr<StateMachine> stateMachine;` from LoopManager.cpp:583. Update `main.cpp` to pass the StateMachine when constructing LoopManager.

2. **Enable state implementation tests**: Remove `DISABLED_` prefix from all state tests. Update test fixtures with proper mocks (from Phase 3). States primarily need `IConfigContext` (already exists), `IHardwareContext`, and mock dependencies.

3. **Eliminate dual state tracking**: If `MachineStateContext` tracks state independently from `StateMachine`, refactor so `MachineStateContext` reads state from `StateMachine`. This may be complex — scope this carefully and defer full resolution if it requires too many cross-cutting changes.

**Tests**:
- All state implementation tests enabled and passing
- LoopManager test updated to verify StateMachine injection
- `pio test -e native_test -f test_state_implementations`

**Acceptance Criteria**:
- No `extern` declarations for StateMachine in any .cpp file
- LoopManager receives StateMachine via constructor/setter
- State tests enabled: verify with `rg 'DISABLED_' test/test_state_implementations/` returning zero matches
- `pio run -e esp32_usb -s` compiles clean
- `pio test -e native_test` passes

**Dependencies**: Phase 3 (mocks), Phase 5 recommended but not strictly required

**Estimated Time**: 3–4 hours

---

## Phase 7: Deprecated Method Removal & Security Hardening

**Objective**: Remove deprecated methods from SystemContext, restrict CORS, add security limits.

**Files to Modify**:
- `include/clevercoffee/context/SystemContext.h` — Remove ~15 remaining `[[deprecated]]` methods
- `src/context/SystemContext.cpp` — Remove implementations of deprecated methods
- `src/network/WebServerManager.cpp` — Restrict CORS origin, add config upload size limit, redact passwords from config export
- `include/clevercoffee/network/WebServerManager.h` — Update if needed
- `src/network/MQTTManager.cpp` — Remove `instance_` static (replace with lambda capture or `std::function`)

**Changes**:

1. **Remove deprecated methods from SystemContext** (~15 methods marked `[[deprecated]]`):
   - `scaleCalibrationOn()`, `setScaleCalibrationOn()` → callers use `sensorCoordinator()`
   - `scaleTareOn()`, `setScaleTareOn()` → callers use `sensorCoordinator()`
   - `currBrewWeight()`, `currReadingWeight()` → callers use `sensorCoordinator()`
   - `inputPressure()` → callers use `sensorCoordinator()`
   - `inX()`, `setInX()`, `inY()`, `setInY()`, `inOld()`, `setInOld()`, `inSum()`, `setInSum()` → internal to SensorCoordinator
   - `inputPressureFilter()` → callers use `sensorCoordinator()`
   - First, search codebase for callers. If callers still exist, migrate them to use the coordinator methods. Then remove the deprecated methods.

2. **Restrict CORS** (WebServerManager.cpp:263):
   Replace `corsMiddleware_->setOrigin("*")` with same-origin default. For development, provide a config option or compile flag.

3. **Add config upload size limit** (WebServerManager.cpp):
   Add a request body size check in the config import handler (e.g., reject bodies > 16KB).

4. **Redact passwords from config export** (WebServerManager.cpp:342):
   When exporting config JSON, replace password fields with `"***"` or exclude them.

5. **Replace `MQTTManager::instance_`**: Convert the static `instance_` used for the C-style callback into a lambda capture or `std::function` binding, removing the global mutable state.

**Tests**:
- Verify deprecated methods are gone: `rg 'deprecated' include/clevercoffee/context/SystemContext.h` returns only the removal comments
- Test CORS header in WebServer response (if testable in native)
- Test config export doesn't contain password values

**Acceptance Criteria**:
- Zero `[[deprecated]]` methods in SystemContext.h
- CORS origin is not `"*"` in production builds
- Config export redacts passwords
- Config import rejects oversized payloads
- `pio run -e esp32_usb -s` compiles clean
- `pio test -e native_test` passes

**Dependencies**: Phase 1–6 should be complete (deprecated methods may be called by code changed in earlier phases)

**Estimated Time**: 2–4 hours

---

## Phase 8: Enable Remaining Test Suites & Code Quality

**Objective**: Enable remaining disabled tests, add `[[nodiscard]]`, clean up empty test directories.

**Files to Modify**:
- `test/test_sensor_coordinator/test_main.cpp` — Enable disabled tests
- `test/test_hardware_manager/test_main.cpp` — Enable disabled tests
- `test/test_loop_manager/test_main.cpp` — Enable disabled tests
- `test/test_config/test_main.cpp` — Enable disabled tests (at least `DISABLED_RealConfigWithNVS`)
- `test/test_mqtt_manager/test_main.cpp` — Enable disabled tests
- Various header files — Add `[[nodiscard]]` to error-returning functions

**Files to Remove/Clean**:
- Empty test directories (if any exist with no test files)

**Changes**:

1. **Enable SensorCoordinator tests** (7 disabled):
   Fix mocks for temperature sensor, scale, water tank. Remove `DISABLED_` prefix. These tests verify async read patterns, caching, null-sensor handling.

2. **Enable HardwareManager tests** (2 disabled):
   Fix construction test and relay access test. May need simplified mock hardware context.

3. **Enable LoopManager test** (1 disabled):
   Now that StateMachine is injected (Phase 6), the initialization test should be fixable.

4. **Enable Config NVS tests** (2 disabled):
   These may need a NVS mock/stub for native testing. If infeasible natively, tag them as integration-only.

5. **Enable MQTT tests** (1 disabled):
   Test MQTT connection handling with mocked PubSubClient.

6. **Add `[[nodiscard]]`** to error-returning functions across the codebase:
   - Config methods that can fail (e.g., `importFromJson()`)
   - Hardware initialization functions
   - Any function returning a success/failure bool or error code

7. **Clean up empty test directories**: Remove or populate any test directories that contain no meaningful tests.

**Tests**:
- Target: reduce disabled tests from 43 to < 10 (keeping only those that need real hardware)
- All newly enabled tests pass

**Acceptance Criteria**:
- `rg 'DISABLED_' test/ --count` shows < 10 remaining disabled tests
- Remaining disabled tests are documented (reason: requires hardware/NVS/WiFi)
- `[[nodiscard]]` on all bool-returning config/init functions
- `pio run -e esp32_usb -s` compiles clean
- `pio test -e native_test` passes

**Dependencies**: Phases 3–7

**Estimated Time**: 3–4 hours

---

## Dependency Graph

```
Phase 1 (Safety Fixes)           ──┐
Phase 2 (Handler Ownership)       ──┼── Independent, can be done in parallel
Phase 3 (Test Infrastructure)     ──┘
                                    │
                                    ├── Phase 4 (Enable Handler Tests)   ← needs Phase 3
                                    │       │
                                    │       ├── Phase 5 (Inject Config)  ← needs Phase 3 + 4
                                    │       │
                                    │       └── Phase 6 (StateMachine + State Tests) ← needs Phase 3
                                    │
                                    └── Phase 7 (Deprecated Removal + Security) ← needs Phase 1-6
                                            │
                                            └── Phase 8 (Remaining Tests + Quality) ← needs Phase 3-7
```

## Items Explicitly Deferred

These items from the analyses are deferred beyond Phase 8 for reasons of scope, risk, or ROI:

| Item                                                                              | Reason for Deferral                                                                        |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Batch NVS writes for MQTT bulk updates                                            | Performance optimization — profile first to confirm it's a bottleneck                      |
| FreeRTOS task for network                                                         | Architectural change requiring careful concurrency design; should be prototyped separately |
| Remove empty timer callbacks                                                      | Low impact, can be done as drive-by cleanup                                                |
| Evaluate TLS for MQTT                                                             | Requires ESP32 memory analysis and certificate management design                           |
| Split `displayCommon.h` into compilation units                                    | Code organization — low urgency, can be done incrementally                                 |
| Continue decomposing SystemContext (PID management, handler registry, ISR bridge) | Large refactor — should follow after test coverage is solid                                |
| Time abstraction for deterministic timer tests                                    | Nice-to-have for test infrastructure, not blocking                                         |

## Notes

- **Always verify build before and after each phase**: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
- **Always run tests after each phase**: `~/.platformio/penv/bin/pio test -e native_test`
- **Format code after each phase**: `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`
- Each phase should be a single, well-scoped commit (or a small series of atomic commits)
- If a phase is taking longer than estimated, split in-progress work into a working subset and defer the rest
