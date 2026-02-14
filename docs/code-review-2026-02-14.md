# Code Review: 2026-02-14

**Overall Rating: B+ (Good with some important issues)**

## Critical Issues (Must Fix)

| ID | Issue | Location | Risk |
|----|-------|----------|------|
| C1 | Duplicate forward declarations | `MachineStateContext.h:28-39` | Code confusion |
| C2 | Inconsistent constructor documentation | `ProcessController.h:46-53` | Maintenance burden |
| C3 | Logger singleton has two separate static instances | `Logger.cpp:22-30` | Race condition, broken singleton |
| C4 | ISR counters use `volatile` instead of `std::atomic` | `isr.h:18-22` | Data corruption |
| C5 | No watchdog timer integration | Global | System hang leaves machine unsafe |
| C6 | ISR relay access without documented atomic guarantee | `isr.h:93-105` | Race condition on heater control |

## Important Issues (Should Fix)

| ID | Issue | Location | Impact |
|----|-------|----------|--------|
| I1 | TODO in safety-critical heater control | `ProcessController.cpp:159-160` | Incomplete implementation |
| I2 | Handler accessors crash on null (LOG FATAL + nullptr deref) | `SystemContext.h:200-205` | Undefined behavior |
| I3 | Inconsistent naming conventions | `MachineStateContext.h` | API usability |
| I4 | Large SystemContext class (1438 lines) | `SystemContext.h` | Maintainability |
| I5 | Deprecated methods lack `[[deprecated]]` attribute | `SystemContext.h:980-1002` | Technical debt |
| I6 | Inconsistent forward declarations | Multiple files | Compilation time |
| I7 | Test files include `.cpp` directly | `test_main.cpp:19-25` | Build fragility |
| I8 | Global state for test stubbing | `test_main.cpp:38-44` | Prevents parallel tests |
| I9 | Minimal IDisplayManager interface | `IDisplayManager.h` | Abstraction leak |
| I10 | Duplicated temperature offset logic | `ProcessController.cpp:131-141` | Logic bugs |
| I11 | No explicit PID anti-windup | `ProcessController.cpp` | Control stability |
| I12 | Water tank check not atomic with pump enable | `HardwareManager.cpp:380-398` | Race condition |

## What is Done Well

1. **Excellent RAII/smart pointers** - Proper memory safety throughout
2. **Strong emergency stop logic** - Debouncing, hysteresis, sensor validation
3. **Good interface abstractions** - `IHardwareContext`, `IMQTTManager`, `IDisplayManager` enable testing
4. **14 mock files** - Comprehensive test infrastructure
5. **Coordinator pattern** - Clean separation of cross-cutting concerns
6. **ISR safety guards** - Multiple checks before ISR execution

## Recommendations

### Immediate Actions (Critical)

1. Fix the ISR counter types from `volatile` to `std::atomic<uint32_t>`
2. Fix the Logger singleton initialization race condition
3. Remove duplicate forward declarations in `MachineStateContext.h`
4. Verify `Relay::on()/off()` are ISR-safe or document the guarantee

### Short-term Actions (Important)

1. Add `std::terminate()` or proper error handling in null handler accessors
2. Mark deprecated methods with `[[deprecated]]` attribute
3. Add watchdog timer integration
4. Complete the TODO for heater control through HardwareManager
5. Fix inconsistent constructor documentation

### Long-term Improvements

1. Refactor `SystemContext` into smaller, focused contexts
2. Configure build system to properly link source files for tests
3. Expand `IDisplayManager` interface to hide U8G2 implementation details
4. Add explicit anti-windup handling in PID state transitions
5. Document and enforce consistent naming conventions

## Implementation Progress

### Completed
- [x] C1: Remove duplicate forward declarations
- [x] C2: Fix inconsistent constructor documentation
- [x] C3: Fix Logger singleton
- [x] C4: Change ISR counters to std::atomic
- [x] C5: Watchdog timer integration
- [x] C6: Document ISR-safe relay operations
- [x] I1: Complete heater control TODO
- [x] I2: Add std::terminate() in null handler accessors
- [x] I5: Add [[deprecated]] attributes
- [x] I7: Document test infrastructure issue (requires build system changes)
- [x] I8: Fix global test state for parallel testing (thread_local)
- [x] I9: Expand IDisplayManager interface
- [x] I10: Add PID anti-windup handling
- [x] I12: Fix non-atomic water tank check with pump enable

### In Progress
- None

### Pending
- I4: Refactor SystemContext (SKIPPED - large undertaking)
- I7 Full Fix: Configure build system to properly link source files for tests
