# ADR-0003: State Machine Hardware Control Contract

## Status

Accepted

## Context

During the refactoring from the original flat state machine (`handleMachineState()` in main.cpp) to a class-based state pattern, several hardware control bugs were introduced:

1. **ManualFlushRunningState** set a flag but never activated pump/valve hardware.
2. **valveSafetyShutdownCheck()** only excluded brew states, causing it to force-close the valve during manual flush and backflush filling.
3. **PidDisabledState** did not drain stale action request flags, causing unexpected brew starts on PID re-enable.
4. **No state enforced PID disable during active operations** (brew, steam, backflush), unlike the original which checked `!pidON` in every operational state.
5. **ManualFlush had no stop mechanism** — the switch release was not handled.

The root cause: the original project controlled hardware directly in procedural functions (`brew()`, `manualFlush()`, `hotWaterHandler()`) called every loop. The refactored state pattern moved responsibility to `onEntry`/`onExit`/`update` methods, but some states were implemented as "flag-only" without the corresponding hardware control.

## Decision

### Hardware control ownership

Each state that requires active hardware (pump, valve) **must**:
- Enable hardware in `onEntryImpl()`
- Reinforce hardware state in `update()` (defense against safety checks toggling it off)
- Disable hardware in `onExitImpl()`

### Safety layers

1. **`valveSafetyShutdownCheck()`** — runs every loop, closes valve unless in an explicit whitelist of water-flow states (brew active, manual flush, backflush filling/flushing).
2. **`BaseState::checkTransitions()`** — uses `constexpr if` to enforce PID-disable transitions for all operational states. Excluded: PID_NORMAL (handles it explicitly), PID_DISABLED, STANDBY, INIT, and error states.
3. **`HardwareManager`** — force-disables pump on water tank empty detection, refuses `enablePump()` when tank is empty or emergency mode is active.

### Flag lifecycle

- **On state entry**: drain all action flags that cannot be acted upon in that state.
- **During update**: drain incoming flags only when the condition preventing action persists (guarded by the same check used for exit transitions).
- **Never drain wake-up signals**: STANDBY preserves `brewStartRequested` and `steamStartRequested` as intentional wake triggers.

## Consequences

- Adding a new water-flow state requires updating `valveSafetyShutdownCheck()` whitelist AND the `BaseState` constexpr exclusion list.
- Every state's hardware contract is explicit in its entry/exit/update methods — no implicit "the loop handles it" assumptions.
- Defense-in-depth: multiple layers ensure hardware safety even if one layer has a bug.
