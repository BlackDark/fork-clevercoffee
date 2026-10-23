# 0006: Safety model

Status: accepted, 2026-10

## Decision

- **One owner**: only the control task drives heater, pump and valve, from `Machine::outputs()` each 10 ms tick. Interlocks are computed in `cc-core` and tested on the host: tank empty → pump off; emergency (over `safety.emergency_temp` three readings in a row, or outside 0–200 °C) → everything off until below 100 °C and below the hysteresis; sensor timeout 2 s → SensorError, all off; firmware write → all off.
- **Hard caps** independent of the user: brew 5 min, hot water 60 s.
- **Priority**: the control loop runs on an interrupt executor (priority 3) above network tasks. Shared state is a critical section held for microseconds; rendering and JSON happen on copies.
- **Watchdog**: TIMG1 MWDT, 2 s, fed by the control tick only. A hung control loop resets the chip. The `bench` build proves it.
- **Panic**: `custom_pre_backtrace` drives heater, pump and valve to their off level before the backtrace, then the chip resets.
- **Boot**: relays are configured at their off level before any task starts.
- `safety.emergency_temp` and `…_hysteresis` are exposed parameters (the C++ firmware had them but did not expose them).

## Consequences

- Relay polarity is a parameter (`hardware.relays.*.trigger_type`), read at boot; a wrong value inverts the relay. The bench cannot detect this; the machine's first boot with an empty tank can.
