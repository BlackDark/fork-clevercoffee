# C++ findings — bugs and ambiguities found while porting

Found 2026-09-28 while implementing R2-04/R2-05/R2-06. **Every one of these was
*preserved* in the Rust port, not silently fixed**, each pinned by a named parity
test. They are recorded here because they are decisions, not accidents.

Each becomes a line in [`intentional-diffs.md`](./intentional-diffs.md) (R1-08) when the
Rust behaviour intentionally diverges.

> **Updated 2026-09-28 (R1-07 + safety-gap work).** Four findings have since been
> **closed on purpose** — §1, §2, §3 and §11 — and each now has a `div<N>_` test
> instead of an `s<N>_` one. The text below is left as the record of what the C++
> does; the divergence and its reasoning live in
> [`intentional-diffs.md`](./intentional-diffs.md). A `div<N>_` test replaces its
> `s<N>_` counterpart; the two are never both present, because they would disagree.
>
> | finding | closed by | test |
> | --- | --- | --- |
> | §1 integer division by zero | [intentional-diffs #4](./intentional-diffs.md#4-the-pid-derivative-is-taken-over-the-real-elapsed-time-🔴-fixed) | `cc-domain::pid_parity::scenario_d_the_cpp_goes_nan_and_this_port_does_not` |
> | §2 no steam-valve whitelist | [intentional-diffs #2](./intentional-diffs.md#2-the-steam-valve-is-whitelist-gated-🔴-added) | `cc-machine::parity_findings::div2_the_steam_valve_is_whitelist_gated` |
> | §3 water valve not tank-gated | [intentional-diffs #3](./intentional-diffs.md#3-the-water-valve-is-gated-on-the-water-tank-🔴-added) | `cc-machine::parity_findings::div3_the_water_valve_is_tank_gated` |
> | §11 pump timeouts dead | [intentional-diffs #1](./intentional-diffs.md#1-both-pump-safety-timeouts-are-armed-🔴-closed) | `cc_machine::parity_findings::div1_the_pump_timeouts_are_armed` |
>
> §4, §5, §6, §7, §12–§16 remain **preserved** and are listed in the
> "Preserved C++ behaviours" table of
> [`intentional-diffs.md`](./intentional-diffs.md#preserved-cpp-behaviours--do-not-fix-these).

---

## 1. `PID_v1.cpp:85` — integer division by zero when `SampleTime < 1000`

```cpp
dInput = (lastFilteredInput - oldFiltered) / (SampleTime / 1000);
```

`SampleTime` is `unsigned long`, so `SampleTime / 1000` is **integer** division. Any
sample time below 1000 ms yields a divisor of `0`, and the entire P_ON_E output becomes
`NaN` or `±inf`.

The shipped firmware escapes this **only by coincidence**: `SystemInitializer` sets the
sample time to exactly `processWindowSize()` = 1000 ms. **Nothing enforces that.**

**Why this matters now:** R1-07 proposes changing the heater output method. Any change
that moves the PID window off 1000 ms trips this immediately. The Rust port exposes the
trap explicitly as `cc_domain::Controller::derivative_seconds()` so the next person
cannot step on it.

- **Rust: FIXED 2026-09-28** (R1-07). The port divides by the real elapsed time in
  `f64`, so the trap cannot occur at any window. Scenarios A–C of the PID oracle are
  still bit-identical to the C++; scenario D is retained *as the C++'s `NaN`* and the
  divergence is asserted. See
  [`intentional-diffs.md` #4](./intentional-diffs.md#4-the-pid-derivative-is-taken-over-the-real-elapsed-time-🔴-fixed).
- The `Controller::derivative_seconds` / `derivative_seconds_at` pair keeps the trap
  documented.

## 2. The steam valve has no safety whitelist at all

`BrewHandler::valveSafetyShutdownCheck()` (S5) gates the **water** valve on an explicit
state whitelist. `openSteamValve()` (`HardwareManager.cpp:397-400`) checks **only**
`emergencyMode_` — there is no `steamSafetyShutdownCheck` anywhere in the tree.

So the steam valve can be commanded open in any state, while the water valve cannot.

- **Rust: CLOSED 2026-09-28.** The port has `cc_safety::steam_flow_allowed` —
  `STEAM_RUNNING` and nothing else, a `match` with no wildcard arm — plus a
  `steamValveSafetyShutdownCheck` in the reducer's tail. Pinned by
  `div2_the_steam_valve_is_whitelist_gated`, which **replaced**
  `s5_the_steam_valve_is_not_whitelist_gated`. See
  [`intentional-diffs.md` #2](./intentional-diffs.md#2-the-steam-valve-is-whitelist-gated-🔴-added)
  for the derivation from the C++.
- This was a real safety gap, not a port artifact: steam and water share **one relay**
  (`ValveState.h:8-11`), so an ungated steam valve is an ungated water valve.

## 3. The water valve is not gated on an empty tank

Only `enablePump` and `setPumpPressure` check `waterTankEmpty_`
(`HardwareManager.cpp:325-328, 398-406`). `openWaterValve` does not.

- **Rust: CLOSED 2026-09-28.** The verdict's `may_open_water` now requires the tank to
  be full as well as the state to be on the S5 whitelist. Pinned by
  `div1_s4_empty_tank_blocks_the_water_valve_too` and
  `div3_the_water_valve_is_tank_gated`. See
  [`intentional-diffs.md` #3](./intentional-diffs.md#3-the-water-valve-is-gated-on-the-water-tank-🔴-added).

## 4. S1 keeps heating through the debounce window

Emergency stop needs three consecutive readings above the threshold. At the production
400 ms sensor interval the heater stays energised for up to ~800 ms while already above
the emergency temperature.

- Rust: preserved. **Real exposure.** Recommend the Rust port trip immediately on a
  reading above the threshold and use the debounce only for recovery, or reduce the
  interval. Deliberate divergence.

## 5. Two dead "145 °C" constants; the live default is 150 °C

`constants/Temperature.h:6-7` defines `EMERGENCY_THRESHOLD_C = 145.0` and
`EMERGENCY_RESET_THRESHOLD_C = 120.0`. **Neither is read anywhere.** The live value is
`config_.emergencyStopTemp`, default **150.0**, range 120–180 (`Config.h:813-820`).
The test suite's "145" is a fixture that sets it explicitly.

- Rust: uses **150.0**, matching the live behaviour, not the dead constant.

## 6. The anti-windup dead band can freeze the integrator

`PID_v1.cpp:70` accumulates only when the previous output is strictly inside
`(outMin+0.01, outMax-0.01)`. With production limits `(0, 1000)` and a start-up output
of `0.0`, the integrator cannot accumulate until the output leaves the exact boundary.

- Rust: preserved. Consider a follow-up; not a migration concern.

## 7. The shipped PID gains look like bang-bang control

`kd = Tv*Kp = 11.5*62 = 713`, rescaled by `SetSampleTime(1000)` to ~7130 ms⁻¹. A 3.1 °C
change in the EMA-filtered input moves the D term by ~22 000 against a 1000-wide output
window. The parity vector shows the output alternating `1000 → 0 → 0 → …`.

- **Not a migration bug** — this is the real, shipped control law, faithfully reproduced.
  Flagged because the machine may effectively be running on/off control rather than PID,
  which is worth someone's attention independent of the port.

## 8. Config validation is per-parameter only

`Config.h::isValid` is a pure range check, so `steam.setpoint = 140` with
`safety.emergency_temp = 120` is accepted — a machine that cannot be steamed.

- Rust: adds the oracle's **cross-parameter** rule in `cc_safety::validate_config`.
  Defaults are safe by 10 °C of margin; range maxima leave 25 °C.

## 9. Eight string-length constants in `defaults.h:118-125` are never enforced

`Config.h::isValid` returns `true` unconditionally for `String`, so a 4 KB hostname is
accepted and handed to `WiFi.setHostname()`.

- Rust: **not enforced** (enforcing it would reject configs the production firmware
  accepts). `json::MAX_TEXT_LEN = 4096` is a loose storage bound only.

## 10. Duplicate `order` value in the config schema

`Config.h:817` and `Config.h:837` both use order `203` in section 1. Harmless.

---

## Corroboration that the ported schema is right

The Rust default config blob serialises to **2077 bytes**. The recovered firmware logged
`cc_firmware: config: nvs (2071 B stored)` (see [08 — Oracle](./08-recovered-oracle.md) §3)
for a 98-key schema whose source no longer exists. A 6-byte delta against a lost
firmware is strong evidence the parameter shape is correct.

---

## 11. 🔴 Both pump safety timeouts are dead code

`PumpTimer::isExpired()` (`PumpTimer.h:30-33`) returns `false` unless `isRunning_` is set,
and `isRunning_` is only set by `PumpTimer::start()`.

**`start()` is never called anywhere in the tree.** The complete set of references is:

| File | Reference |
| --- | --- |
| `HotWaterHandler.h:23` | member declaration `PumpTimer pumpTimer_` |
| `HotWaterHandler.h:28` | constructor, `pumpTimer_(60000)` |
| `HotWaterHandler.h:115` | `pumpTimer_.isExpired()` **read** |
| `BrewHandler.h:25` | member declaration `PumpTimer pumpTimer_` |
| `BrewHandler.h:32` | constructor, `pumpTimer_(300000)` |
| `BrewHandler.h:255` | `pumpTimer_.isExpired()` **read** |

No call site. So `isRunning_` is permanently `false` and **`isExpired()` is
unconditionally `false`.**

**Consequence: the 5-minute brew pump limit and the 60-second hot-water pump limit can
never fire.** Hold the water switch indefinitely and the pump runs indefinitely. This is
the single most serious finding in this document — it is an unbounded pump run on a
machine with a heated boiler.

- **Rust: CLOSED 2026-09-28.** Both timeouts are now **armed on the activating edge** and
  a trip emits `Effect::PumpTimeoutFired`, whose message is the C++'s own `logError`
  text verbatim — so a field log answers "did this ever trip?" Deliberately
  one-directional: the Rust can trip a watchdog the C++ cannot. Pinned by
  `div1_the_pump_timeouts_are_armed`, which **replaced**
  `s11_the_pump_timeouts_are_never_armed`. See
  [`intentional-diffs.md` #1](./intentional-diffs.md#1-both-pump-safety-timeouts-are-armed-🔴-closed).
- **Not covered, on purpose:** `MANUAL_FLUSH_RUNNING` and the backflush fill/flush phases
  also run the pump and neither C++ timer covers them. Recorded as a follow-up.

## 12. 🔴 `SensorErrorState`'s recovery clock is measured from the wrong instant

`ErrorStates.cpp:47-50` documents the recovery delay as running "from when the error
actually clears". In fact, the sensor-error guard in `BaseState.h:145-148` has **no
exclusion list**, so while the probe is faulted `checkSpecificTransitions()` is never
reached. The delay is therefore measured from **entry into `SENSOR_ERROR`**.

**Consequence:** a fault that persists for an hour recovers immediately on clear.

- Rust: preserved. Pinned by `s12_the_sensor_error_recovery_clock_is_never_reset`.

## 13. Backflush states never re-assert their hardware in `update()`

All four backflush `update()` methods only log. **This is not unique to `Filling`** —
`Filling`, `Flushing`, `Idle` and `Finished` all fail to re-assert.

The severity differs:

- **`BackflushFillingState`** is the worse case. Its `onEntryImpl` calls `enablePump()`
  and `openWaterValve()`, and nothing re-asserts either. ADR-0003 exists precisely because
  of this class of bug — a state that enables hardware without re-asserting it loses the
  hardware to any safety check that intervenes.
- **`BackflushFlushingState`** `onEntryImpl` calls `cleanupPumpAndValve()`, so the
  failure mode is inverted: a safety check that opens the valve mid-flush is never
  re-closed.

- Rust: preserved. Pinned by `s13_*`.

## 14. The water switch cannot wake the machine from standby

`hasUserActivity()` and `shouldExitStandby()` are hard `return false` stubs
(`MachineStateContext.cpp:419-429`). The water switch resets the standby countdown and
does nothing else.

## 15. `powerOff()` shuts down before requesting standby

`PowerHandler.h:167-170` performs the safe shutdown *before* requesting standby, so for
one loop the machine is in `PID_NORMAL` with hardware off — and `PidNormalState::update`
re-enables the pump if the water switch happens to be held.

---

## 16. The C++ state-machine test coverage is much thinner than 340 cases suggests

Worth recording so nobody treats the C++ suite as a complete safety net:

- `test_state_machine` exercises **only gMock plumbing**; its own comment says *"Full
  StateMachine tests require additional setup"*.
- `test_pid_state_transitions` tests **hand-written mock states**, not the real ones.
- `test_steam_water_injection` and `test_pid_mode_water_dispensing` do not include the
  real state source files at all.

The Rust port replaces these with real state coverage and a 4140-pair exhaustive table,
which is why it has 255 tests in `cc-machine` against 150 in the fifteen C++ suites it
replaces.
