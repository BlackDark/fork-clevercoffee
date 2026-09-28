# Intentional divergences from the C++ firmware

**Owner:** R1-08 creates this file; R4-09 keeps it current.
**Seeded:** 2026-09-28, by the R1-07 / safety-gap work.

This file is the answer to one question: **when the Rust firmware's behaviour
differs from the C++ firmware it replaces, is that a bug or a decision?**

Every entry is a place where a parity harness (`scripts/parity/run.sh`, R4-03)
may legitimately produce a diff. Anything *not* listed here that shows up in a
diff is a regression, and `run.sh` must exit non-zero on it.

Three things every entry carries:

* **What the C++ does** — with the file and line, so it can be checked.
* **What the Rust does** — and why that is the right answer rather than merely
  a different one.
* **The test that pins it** — a named test, so the behaviour cannot drift
  silently after this file is written.

The second half of the file lists the C++ behaviours the port **preserved on
purpose**. Those are the ones a reviewer is most likely to mistake for a Rust
bug, so they are recorded with equal care.

---

## 1. Both pump safety timeouts are armed 🔴 closed

| | |
| --- | --- |
| **Finding** | [09 §11](./09-cpp-findings.md#11-🔴-both-pump-safety-timeouts-are-dead-code) — "Both pump safety timeouts are dead code" |
| **Severity in the C++** | The most serious finding in 09. An unbounded pump run on a machine with a 2 kW boiler. |
| **Test** | `cc-machine/tests/parity_findings.rs::div1_the_pump_timeouts_are_armed`, `::div1_the_watchdogs_re_arm_after_a_release` |

### What the C++ does

`PumpTimer::isExpired()` returns `false` unless `isRunning_` is set, and
`isRunning_` is only set by `PumpTimer::start()`:

```cpp
bool isExpired() const {
    if (!isRunning_ || startTime_ == 0) return false;
    return (millis() - startTime_) > maxRunTime_;
}
```

(`include/clevercoffee/handlers/PumpTimer.h:23-26`, `isRunning_` initialised
`false` at `:14`.)

**`start()` is never called anywhere in the tree.** The complete set of
references to either `pumpTimer_` is two member declarations, two constructor
arguments, and the two `isExpired()` reads:

| File | Reference |
| --- | --- |
| `BrewHandler.h:25` | member declaration |
| `BrewHandler.h:32` | constructor, `pumpTimer_(300000)` |
| `BrewHandler.h:255` | `pumpTimer_.isExpired()` **read** |
| `HotWaterHandler.h:23` | member declaration |
| `HotWaterHandler.h:28` | constructor, `pumpTimer_(60000)` |
| `HotWaterHandler.h:115` | `pumpTimer_.isExpired()` **read** |

So the 5-minute brew limit and the 60-second hot-water limit **can never fire**.
Hold the water switch and the pump runs for as long as the switch is held. The
`logError` lines that would announce a trip are unreachable too.

### What the Rust does

Both watchdogs are **armed on the pump-on edge** and a trip is **announced before
it acts**.

* Armed while the pump is actually being commanded on, not while the state merely
  permits it: `BREW_PREINFUSION` / `BREW_RUNNING` for the brew timer,
  `PID_NORMAL` with the water switch held for the hot-water timer
  (`cc_machine::handlers::arm_pump_watchdogs`).
  `BREW_PREINFUSION_PAUSE` is deliberately excluded: it is a state the C++'s own
  `isBrewActive()` accepts, but its `update()` pushes `DisablePump`
  (`BrewStates.cpp:172`), so a pause is not pump run time.
* A trip emits `Effect::PumpTimeoutFired { watchdog }`, whose message is the C++'s
  own `logError` text verbatim — `"Pump timeout - stopping for safety"`
  (`BrewHandler.h:256`) and `"Hot water pump timeout - stopping for safety"`
  (`HotWaterHandler.h:117`, also recovered verbatim from the previous Rust
  firmware, [08 §4.2](./08-recovered-oracle.md)). It is emitted **before** the
  action, so a field log answers "did this ever trip?" rather than leaving it to
  be inferred from a missing cup of coffee.
* The action is unchanged: the brew timer *requests* a stop
  (`BrewHandler.h:257-260` sets a flag consumed by the next `checkTransitions`),
  the hot-water timer calls `disablePump()`.

### Why

A protection the firmware plainly intended to have, written and named and given a
five-minute deadline, that can never execute because one call site is missing, is
a defect — not a design choice. The alternative "faithful" port would have
deleted a check whose comment says `// 5 minute max brew time safety`, and that is
precisely the kind of silent decision this file exists to prevent.

The direction of the divergence matters: **strictly one-directional.** The Rust
can trip a watchdog the C++ cannot. Nothing the C++ could do becomes impossible.

### Not covered, on purpose

`MANUAL_FLUSH_RUNNING`, `BACKFLUSH_FILLING` and `BACKFLUSH_FLUSHING` also run the
pump, and **neither** C++ timer covers them. Extending a watchdog's scope is a
specification change, not a parity fix, so it is recorded here as an open
follow-up rather than smuggled in.

---

## 2. The steam valve is whitelist-gated 🔴 added

| | |
| --- | --- |
| **Finding** | [09 §2](./09-cpp-findings.md) — "The steam valve has no safety whitelist at all" |
| **Severity in the C++** | A real gap, not a port artefact. Closed only by the accident that nothing calls `openSteamValve()`. |
| **Test** | `cc-safety/tests/safety_paths.rs::div2_the_steam_valve_is_whitelist_gated`, `::div2_steam_running_is_the_only_state_that_may_flow_steam`, `::div2_the_two_whitelists_never_agree_on_a_state`; `cc-machine/tests/parity_findings.rs::div2_the_steam_valve_is_whitelist_gated` |

### What the C++ does

`HardwareManager::openSteamValve()` checks **only** the emergency latch:

```cpp
void HardwareManager::openSteamValve() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot open steam valve - emergency mode active");
        return;
    }
    ...
```

(`src/hardware/HardwareManager.cpp:397-400`; the same three lines with no
whitelist at `src/state/MachineStateContext.cpp:556-558`.)

There is no `steamSafetyShutdownCheck` anywhere in the tree.
`BrewHandler::valveSafetyShutdownCheck` (`BrewHandler.h:105-122`) is the **water**
valve's, and it names only the water relay.

**The steam valve and the water valve are the same physical relay:**

> "Steam and water valves share the same physical relay. This enum tracks which
> valve(s) should be open, ensuring correct relay control."

— `include/clevercoffee/hardware/ValveState.h:8-11`

The pin map agrees: one valve relay, GPIO17
(`include/clevercoffee/hardware/pinmapping.h:39`).

So an ungated `openSteamValve()` does not open some other solenoid — **it
energises the very relay that S5 spends its whole existence keeping closed.** The
C++ is protected only by `rg -n openSteamValve src/ include/` finding the
definition, the pass-through, the interface declaration and nothing else.

### What the Rust does

`cc_safety::steam_flow_allowed(state)`, a `match` with **no wildcard arm** — so a
19th `MachineState` is a compile error until somebody classifies it, exactly as
for `water_flow_allowed`. The whitelist is **`STEAM_RUNNING` and nothing else.**

The reducer's tail closes the valve in every other state, mirroring S5
(`steamValveSafetyShutdownCheck`, which the C++ does not have), and the verdict's
`may_open_steam` refuses the actuator call.

### Why — the derivation

The whitelist is *the set of states in which steam is drawn*, derived from the
C++:

1. `SteamRunningState::onEntryImpl` is the **only** place in the tree that turns
   steam mode on: `context.setSteamMode(true)`
   (`src/state/states/SteamStates.cpp:16`). Every other state either never
   touches it or clears it — `SteamRunningState::onExitImpl` (`:21`),
   `src/state/states/SystemStates.cpp:17`.
2. Steam mode is what the process controller keys off to select the steam
   setpoint, and nothing else: `updateSetpoint(isSteamModeActive())`
   (`src/control/ProcessController.cpp:119-120`, `:235-244`). So the only state in
   which the machine holds the boiler at `steam.setpoint` is `STEAM_RUNNING`.
3. Water injection during steam — the second place the C++ moves water while
   steaming — happens **inside** `STEAM_RUNNING`, not in a state of its own:
   `SteamRunningState::update` drives the pump from the water switch
   (`SteamStates.cpp:36-46`). `MachineStateIds.h` has exactly one steam state
   (`STEAM_RUNNING = 51`).
4. `WebServerManager.cpp:444-445` can flip `isSteamModeActive()` directly over
   HTTP without changing the state. That is a debug surface, and it is the case
   that makes a *mode-based* gate wrong: the steam mode would be on in
   `PID_NORMAL`, where the setpoint has not changed and no steam can be drawn. A
   state-based whitelist ignores it, correctly.

A single-state whitelist is not a stub. A wider one would be **actively wrong**:
because the relay is shared, listing a water-flow state as a steam state would
make `may_open_steam` agree with `may_open_water` and quietly re-open S5's hole
from the other side. Hence the third test above: **the two whitelists must be
disjoint**, and a test enforces it.

---

## 3. The water valve is gated on the water tank 🔴 added

| | |
| --- | --- |
| **Finding** | [09 §3](./09-cpp-findings.md) — "The water valve is not gated on an empty tank" |
| **Test** | `cc-safety/tests/safety_paths.rs::div1_s4_empty_tank_blocks_the_water_valve_too`; `cc-machine/tests/parity_findings.rs::div3_the_water_valve_is_tank_gated` |

### What the C++ does

`waterTankEmpty_` is checked in `enablePump` and `setPumpPressure`
(`src/hardware/HardwareManager.cpp:325-328,398-406`) and **not** in
`openWaterValve`, which checks only `emergencyMode_`. An empty tank in a
water-flow state therefore still permits the valve.

### What the Rust does

`Verdict::may_open_water` requires the tank to be full **as well as** the state to
be on the S5 whitelist. The two are independent: a refilled tank in a
non-water-flow state is still refused, and a full tank in a water-flow state
permits it.

### Why

The boiler is fed from the reservoir, so an empty tank means the pump is running
dry. Leaving the valve open is not what makes that safe — it is just as much of a
mistake, and the C++'s asymmetry (`enablePump` blocked, `openWaterValve` not) is
almost certainly an oversight rather than a decision. The gate costs nothing: the
S5 whitelist is consulted in the same breath.

**The heater is deliberately not gated on the tank.** The boiler is a separate
vessel and `hardware.sensors.watertank.keep_heater_on_empty` is a real
configuration the machine must honour.

---

## 4. The PID derivative is taken over the real elapsed time 🔴 fixed

| | |
| --- | --- |
| **Finding** | [09 §1](./09-cpp-findings.md) — `PID_v1.cpp:85`, integer division by zero |
| **Test** | `cc-domain/src/pid.rs::derivative_divisor_is_never_zero_at_any_window`, `::a_sub_second_window_now_yields_a_finite_derivative`, `::a_late_step_uses_the_real_interval`; `cc-domain/src/pid_parity.rs::scenario_d_the_cpp_goes_nan_and_this_port_does_not`, `::scenario_e_a_late_step_uses_the_real_interval` |

### What the C++ does

```cpp
dInput = (lastFilteredInput - oldFiltered) / (SampleTime / 1000);
```

(`lib/Arduino-PID-Library/PID_v1.cpp:85`.) `SampleTime` is an `unsigned long`, so
`/ 1000` is **integer** division. Any window below 1000 ms makes the divisor zero,
the derivative `±inf`, and the output `NaN` — and the C++'s own output clamp
cannot catch it, because `if (x > max) … else if (x < min)` lets `NaN` through.

The shipped firmware escapes this **only by coincidence**:
`SystemInitializer.cpp:551` sets the sample time to exactly `processWindowSize()`
= 1000 ms, so `1000 / 1000 == 1`. Nothing enforces it, and R1-07 — changing the
heater output method — is exactly the kind of change that moves a window.

### What the Rust does

`Controller::compute` divides by the **actual elapsed time** in `f64`
(`Controller::derivative_seconds_at`). The divisor is strictly positive for every
window, so no configuration can manufacture a `NaN`. The nominal term is still
exposed as `Controller::derivative_seconds`, and both are documented, so the
trap stays visible rather than being rediscovered from a `NaN` on the bench.

### Parity, measured

The oracle (`crates/cc-domain/tools/pid_oracle/run.sh`) compiles the **real,
unmodified** `PID_v1.cpp` and replays the identical sequence.

| scenario | window | before the fix | after the fix |
| --- | --- | --- | --- |
| A production `P_ON_E` (22 steps) | 1000 ms | exact, `to_bits()` identical | **exact, `to_bits()` identical** |
| B brew detection `P_ON_M` (11 steps) | 1000 ms | exact | **exact** |
| C manual/automatic, limits (9 steps) | 1000 ms | exact | **exact** |
| **maximum \|delta\| over A–C** | | **0.0** | **0.0** |
| D sub-second window (5 steps) | 500 ms | C++ `NaN`, port `NaN` | C++ `NaN`, port finite — **deliberate** |
| E ragged steps (4 computing steps) | 1000 ms | n/a (new) | **max \|delta\| = 1.0** on a ±1000 output |

Scenario D, measured, C++ vs port:

| t (ms) | C++ | Rust |
| --- | --- | --- |
| 500 | `NaN` | 500 (saturated at `outMax`) |
| 1000 | 0 | 500 |
| 1500 | 0 | 500 |
| 2000 | 8.5 | 8.5 |
| 2500 | 0 | 0 |

The port saturating at 500 is not a bug: over half the window the same heating
authority *is* twice the derivative, so the output legitimately asks for more. The
C++ cannot express that — it is `NaN`.

Scenario E exists solely to quantify the residual difference. It drives the
**shipped 1000 ms window** with deliberately ragged timestamps (intervals of
1100, 1500, 1000 and 2000 ms) and an input that steps by exactly 2.0, with
`kd = 1.0`:

| t (ms) | elapsed (ms) | C++ | Rust | delta |
| --- | --- | --- | --- | --- |
| 0 | — | 0 (did not compute) | 0 (did not compute) | 0 |
| 1000 | 1100 | 31 | 31.18182 | 0.18182 |
| 2500 | 1500 | 29 | 29.66667 | 0.66667 |
| 3500 | 1000 | 27 | **27** | **0 — bit-identical** |
| 5500 | 2000 | 25 | 26 | 1.0 |

### Why

The C++'s *intent* is plainly `dInput/dt` with `dt` the time between samples. A
controller whose `dt` is a configuration constant is one whose derivative gain is
wrong by exactly the ratio of the real interval to the nominal one. At a 100 Hz
control loop with a 1000 ms window that error is under 1 %, which is why nobody
has noticed — and why moving the window below 1000 ms does not merely make it
wrong, it divides by zero.

**The residual difference is bounded by the scheduling jitter**, and it is
*stale-low* in effect: the port's derivative is the smaller-magnitude one for a
late step, so the port is marginally more conservative than the C++ there. A step
that lands exactly on the window is bit-identical, which the test asserts.

### The oracle is not to be "fixed"

`scenario_d` in `crates/cc-domain/tools/pid_oracle/pid_oracle.cpp` still produces
`NaN`, on purpose. It is the only evidence for this entry. **Do not change the
oracle to agree with the port.**

---

## 5. ~~The heater is driven by LEDC, not a 10 ms ISR~~ — superseded by #9 🔴 reversed

| | |
| --- | --- |
| **Superseded** | 2026-09-28, by [#9](#9-the-heater-is-chopped-by-a-10-ms-gptimer-isr-not-by-ledc-🔴-changed) |
| **Why** | R1-07's 1 Hz LEDC carrier **panics the original ESP32 at boot** — `ledc_ll_set_duty_start` spins inside `portENTER_CRITICAL` for up to one carrier period with interrupts masked, against a 300 ms interrupt watchdog. The spin is unique to this chip: every other `ledc_ll.h` in the tree has it removed. |
| **What survives** | The **argument** for a low carrier, and the divider arithmetic (`esp_driver_ledc`'s `div_param` formula, the 17/18/19/20-bit table). Both are kept in `cc_hal_esp32::heater`'s module docs and both are what a different target would use. `LedcPwm` is retained, unbrought-up, behind the same `HeaterDuty` seam. |
| **Hardware** | The 1 Hz carrier was measured panicking on 2026-09-28. The 10 ms ISR replacement has **not** yet been confirmed running — see #9's "Not yet verified". |

The text below is left as the record of what R1-07 decided and why, because the
*reason* is still correct and the next person to look at the heater needs it.

## Preserved C++ behaviours — do not "fix" these

These are C++ bugs the port **reproduces on purpose**. A diff at any of these is
*not* a regression; the `s<N>_`-prefixed tests in
`cc-machine/tests/parity_findings.rs` pin each one, and the `s<N>` numbering is
[09](./09-cpp-findings.md)'s.

| # | C++ behaviour | Pinned by | Note |
| --- | --- | --- | --- |
| 09 §4 | S1's three-reading debounce keeps the heater energised for up to ~800 ms above the emergency temperature | `s4_the_emergency_debounce_keeps_the_heater_on` | **Real exposure.** Tripping immediately on the first reading above the threshold is recommended and is *not* implemented. Needs a human decision. |
| 09 §5 | The live emergency threshold is `config_.emergencyStopTemp` (default 150 °C); the `145 °C` and `120 °C` constants in `constants/Temperature.h:6-7` are dead | `s5_the_emergency_threshold_constant_is_dead` | The port uses 150, matching the running firmware. |
| 09 §6 | The anti-windup gate is a strict interior test with a 0.01 dead band, so an output sitting exactly on `outMin` cannot accumulate | `pid::tests::an_output_exactly_on_a_limit_does_not_accumulate` | Shipped behaviour. |
| 09 §7 | The shipped gains (`kd = Tv·Kp = 713`, ~7130 ms⁻¹ after rescaling) make the controller behave like on/off control | `pid_parity::scenario_a_production_pon_e_matches_the_cpp_library` | **Not a migration bug** — this is the real, shipped control law. Worth someone's attention independent of the port. |
| 09 §8 | The C++ validates each config parameter in isolation, so `steam.setpoint = 140` with `safety.emergency_temp = 120` is accepted | `cc-safety::tests::safety_paths.rs::config_*` | The port *adds* the cross-parameter rule, so this is listed for completeness: the diff here is intentional but is a rule the C++ lacks, not a rule it had. |
| 09 §9 | Eight string-length constants in `defaults.h:118-125` are never enforced; a 4 KB hostname is accepted | — | Not enforced in the port either; enforcing it would reject configs the C++ accepts. |
| 09 §10 | Two config parameters share `order = 203` | — | Cosmetic; the port's schema has one of them. |
| 09 §12 | `SensorErrorState`'s recovery clock is measured from **entry**, not from the moment the error clears, because the sensor-error guard in `BaseState.h:145-148` has no exclusion list | `s12_the_sensor_error_recovery_clock_is_never_reset` | The `errorStartTime_ = millis()` line at `ErrorStates.cpp:49` is unreachable. |
| 09 §13 | `BackflushFillingState::update` (`BackflushStates.cpp:71-76`) only logs, so it never re-asserts its pump or valve — violating the contract ADR-0003 exists to state | `s13_backflush_filling_never_re_asserts_its_hardware` | All four backflush states fail to re-assert; `Filling` is the worse case because it is on the S5 whitelist. |
| 09 §14 | `hasUserActivity()` is a hard `return false` (`MachineStateContext.cpp:419-423`), so the water switch cannot wake the machine from standby | `s14_the_water_switch_does_not_wake_the_machine_from_standby` | |
| 09 §15 | `powerOff()` performs the safe shutdown *before* requesting standby (`PowerHandler.h:163-175`), so for one loop the machine is in `PID_NORMAL` with the hardware off and `PidNormalState::update` re-enables the pump | `s15_the_power_off_happens_before_the_standby_request` | One loop. Narrow and real. |
| 09 §16 | The C++ state-machine test coverage is far thinner than 340 cases suggests: `test_state_machine` exercises gMock plumbing only, `test_pid_state_transitions` uses hand-written mock states, and `test_steam_water_injection` / `test_pid_mode_water_dispensing` never include the real state sources | — | The port replaces them with 4140 real state × event pairs. |

---

## 6. `cc-display` does not implement `embedded-graphics::DrawTarget` 🟡 open

| | |
| --- | --- |
| **Task** | R2-10, whose stated architecture (R1-04 step 1) calls for an `embedded-graphics` `DrawTarget` |
| **Status** | **Not done, and not decided.** Recorded here because it is a deviation from a written plan, not a silent omission. |
| **Test** | — there is nothing to pin; the claim is about an API that does not exist |

### What the plan says

`cc-display` should render through `embedded-graphics::DrawTarget`, so that the
display implements a standard trait and could be driven by `embedded-graphics`
fonts and shapes.

### What the crate does

It implements its own framebuffer and its own U8g2-compatible draw calls,
against no third-party crates. The reasons given in `crates/cc-display/Cargo.toml`:

* `cc-display` is `no_std` **with no `alloc`**, and the display is a fixed
  `[u8; 1024]` page buffer. The device has ~320 KB of RAM; a display layer that
  can allocate is a display layer that can fail to allocate.
* The crate's whole job is bit-exact U8g2 parity, and U8g2 has behaviour that
  does not survive being expressed as `embedded-graphics` primitives — notably
  the 16-bit coordinate wrap in `u8g2_is_intersection_decision_tree` (see
  [`docs/display-parity.md`](../../docs/display-parity.md)) and U8g2's
  last-glyph and balanced-width quirks in `getStrWidth`. Going through a
  `DrawTarget` would mean re-deriving those on the far side.
* The ten embedded fonts are raw U8g2 RLE (42,722 bytes) rather than
  `ImageRaw` (177,723 bytes), a measured 135,001-byte saving on a
  size-constrained target.

### Why this needs a decision rather than a footnote

The saving and the `no_std` argument are real, and the parity argument is
decisive *for the current requirement*. But R1-04 step 1 was chosen for a reason
this file does not record, and "we did not need it" is not the same as "we
decided not to". If the intent was ever to make the display drivable by
`embedded-graphics` — for a simulator, a test harness, or a future non-OLED
panel — then this deviation forecloses it and the decision should be revisited
**before** the templates are finished, not after.

Recommended resolution: either amend R1-04 to record that `cc-display` is
U8g2-specific by design, or add a thin `DrawTarget` adapter over
`cc_display::display::Display` that satisfies the trait without putting it on
the drawing path. The adapter is a few dozen lines and costs the firmware
nothing; the design change is not cheap.

---

## Also intentional, from before this file existed

Recorded here so the file is complete; each was decided in its own task.

| What | Why | Pinned by |
| --- | --- | --- |
| `safety.emergency_temp` and `safety.emergency_hysteresis` are registered and persisted | The C++ defines and reads them but omits them from `getAllConfigParams()` (`src/Config.cpp:438-563`), so they silently reset to the compiled default on every reboot — a live bug on safety path S1 | `cc-config/tests/config_schema.rs` |
| Cross-parameter config validation, and refusing to store or run an unsafe one | Recovered from the previous Rust firmware ([08 §4.1](./08-recovered-oracle.md)). The C++ has neither | `cc-safety::tests::safety_paths.rs::config_*`, `::store_refuses_an_unsafe_config` |
| A `LOW_TRIGGER` heater relay is refused outright | An undriven GPIO during reset would energise a 2 kW heater. No firmware can prevent it; it is a wiring property | `cc-safety::tests/safety_paths.rs` |
| The `SetHeaterDuty` effect is gated on `may_heat` at emission, where the C++ emits the value and zeroes it microseconds later | Fail-safe rather than fail-fast. The machine state is identical; the port never asks the heater to be on when it must not be | `cc-machine/tests/*` |
| `Duty` is bounded at `0..=1000`, i.e. the PID output is a **millisecond** duty, not `setHeaterPower`'s `uint8_t` percent | The C++ heater path is a PWM window compared against the PID output, and `HardwareManager::setHeaterPower` (`HardwareManager.cpp:305-318`) is a TODO stub | `cc-domain/src/units.rs::tests::duty_bound_is_the_chopper_window` |
| Blocked steam mode being requested from the C++ test suite (`test_steam_handler`, `test_steam_water_injection`) | 27 `#[ignore]`d records of C++ mock cases with no Rust equivalent, each with a comment saying why | the `#[ignore]` attributes themselves |
| SSE over WebSocket for the UI's live channel | R1-05; `EspHttpConnection::write` (chunked) and `raw_connection().write_all` both ship in `esp-idf-svc` 0.53 | R1-05 |

---

## 7. R1-03 / R3-07: both temperature sensors are implemented 🔴 changed

| | |
| --- | --- |
| **Task** | R1-03, R3-06, R3-07 |
| **Decision** | 2026-09-28. A previous revision of this entry recorded the opposite — `TSIC_306` was **refused** and the compiled-in default was moved to `DALLAS_DS18B20`. **That is reversed.** The TSIC-306 / ZACwire driver now exists, so there is nothing left to refuse. |
| **Tests** | `cc_domain::sensor::ds18b20::*`, `cc_domain::sensor::tsic306::*`, `cc_domain::sensor::onewire::div7_*`; `cc-safety::tests::safety_paths.rs::both_temperature_sensor_types_are_accepted`, `::the_compiled_in_default_is_the_cpps_tsic_306`, `::a_stored_tsic_306_config_is_loaded_not_discarded`; `cc-config::tests::config_schema.rs::the_default_temperature_sensor_is_the_cpps_tsic_306` |

### What the C++ does

`HardwareManager::initializeTemperatureSensor` (`HardwareManager.cpp:180-198`)
builds whichever driver the config names, and the config default is `TSIC_306`
(`Config.h:1085-1092`). On this machine the probe is a **DS18B20** — family
`0x28`, ROM `0x28 69 37 aa cd 78 af 41`, measured,
[01 §"The temperature sensor fitted to this machine is a DS18B20"](./01-feature-inventory.md)
— so the C++ default constructs a TSIC-306 driver pointed at a 1-Wire bus it
does not own, and the recovered firmware logged the substitution rather than
refusing it
([08 §4.1](./08-recovered-oracle.md)).

### What the Rust does

1. **Both drivers are implemented.** `cc_domain::sensor::ds18b20` over
   `cc_domain::sensor::onewire` for the DS18B20, and
   `cc_domain::sensor::tsic306` (protocol, edge ring, frame decoder, driver) for
   the TSIC-306, with `cc_hal_esp32::onewire` and `cc_hal_esp32::zacwire` as the
   only device-side code. `cc_domain::sensor::probe::TemperatureProbe` is the one
   interface, so the state machine is not generic over both.
2. **The compiled-in default is `TSIC_306`**, the C++'s value, in both
   `cc-config` (`HardwareSensorsTemperature::default`) and `cc-safety`
   (`SafetyConfig::default`).
3. **`ConfigViolation::UnsupportedTemperatureSensor` is gone.** There is no value
   of `hardware.sensors.temperature.type` the firmware cannot honour, so there
   is nothing for the validator to refuse. The **remaining** rules are unchanged
   and still tested: the cross-parameter emergency-temperature check, the
   `LOW_TRIGGER` heater refusal, and discarding an unsafe stored configuration.
4. **The driver is selected from the board, not from the configuration.**
   `cc-firmware`'s `PROBE` constant names the fitted probe
   (`DallasDs18b20`, measured) and the configured value is logged next to it.

### Why

The original reasoning for the refusal was sound *given a missing driver*: a user
who configures `TSIC_306` and is handed a DS18B20's reading is misinformed about
**which sensor is feeding the over-temperature interlock**, and cannot tell by
looking at the machine. That is a safety defect and silence is what makes it one.

The driver now exists, so the defect is prevented by construction instead: a
machine with the wrong probe fitted gets a boot log that names the sensor the
configuration asked for **and** the one that answered, and the two drivers fail
differently and visibly (`ProbeFault::NotConnected` for a 1-Wire bus with no
presence pulse; `ProbeFault::ReadFailed` for a ZACwire line that moves and does
not decode). Refusing the configuration is no longer the only way to prevent the
silence, and it was always a blunt instrument — it made a correctly-configured
TSIC-306 machine unrunnable.

### 🔴 The TSIC-306 is unverified on hardware, and that is not a formality

**No TSIC-306 is fitted to the machine this was written on, and none has ever
been.** There are two separate gaps and both are open:

* **The pure logic is host-tested against a synthesised waveform.**
  `cc_domain::sensor::tsic306::simulator` generates a GPIO level function from the
  IST AG app note's own duty cycles and timings, and `edges()` *scans* it at 1 µs
  to discover the transitions, so the decoder never sees the encoder's intent.
  That proves the arithmetic, the bit ordering, the parity, the rejection of
  damaged frames and the DS → °C conversion — and it proves **nothing** about a
  real sensor. A real TSIC-306's clock tolerance, its 31.25 µs pulses through a
  pull-up and a cable, its behaviour when brownout, and the EMI the app note's
  parity bit exists for, are all outside what a synthesiser can produce. See
  `cc_domain::sensor::tsic306`'s module docs, which say the same thing.
* **The device side has never executed.**
  `cc_hal_esp32::zacwire` is implemented and is **not brought up**, because the
  pin it would capture (GPIO16, `pinmapping.h:27`) is carrying 1-Wire traffic
  from the DS18B20 that is actually fitted. The TSIC branch of
  `cc-firmware::main` is compiled and type-checked on every build and
  dead-code-eliminated when `PROBE` is the DS18B20, so its image cost is also
  unmeasured.

**Anyone fitting a TSIC-306 must treat the first reading as unverified**, and
R3-07's acceptance criterion (a C++-versus-Rust reference log) is still
unachievable without the hardware.

### Two smaller divergences inside the TSIC driver, both deliberate

| | C++ | Rust | Test |
| --- | --- | --- | --- |
| **The no-signal timeout** | 100 ms (`ZACwire.h:29`) against a **10 Hz** sensor, i.e. one transmission period with zero margin | 250 ms (2.5 periods) | `protocol::tests::the_no_signal_timeout_is_longer_than_the_cpps_and_says_why` |
| **The rate-limit latch** | `static bool validTemps` in a `const` member function — process-global, never reset, shared between instances | per-instance state, so a reconnect starts permissive | `tsic306::tests::the_latch_is_per_instance_not_process_global` |

The 100 ms timeout is a false negative on a safety input: a single missed or
jittered frame reports a probe that is present and working as disconnected. The
`static` is a genuine C++ bug — it is what a `const` member function with mutable
process state looks like — and the permissive direction is the safe one, since
that is what a first-ever boot gets.

### One unresolved ambiguity, flagged rather than decided

`ZACwire::getTemp(maxChangeRate)` compares the limit against
`int16_t grad = (temp - prevTemp) / (heartbeat|1)` (`ZACwire.cpp:58`) where
`temp` is the **raw 11-bit count**, not degrees — while its own comment says
`//grad is [°C/s]`, and `TempSensorTSIC`'s latch condition compares the same
`RUNTIME_CHANGERATE` constant against two **degrees**
(`TempSensorTSIC.cpp:39`). One constant, two units, two adjacent files.

This port applies both limits in **degrees** (200 °C/sample → 5 °C/sample), which
is what 02 §6 calls them and what the C++'s latch condition unambiguously is.
Under the count reading the C++'s effective limits are ≈ 19.5 °C/sample and
≈ 0.49 °C/sample. `tsic306::COUNT_SCALE` makes the other reading one
multiplication away, and `tsic306::tests::the_count_based_reading_of_the_rate_is_one_multiplication_away`
quantifies the difference. **This is the single most likely thing to be wrong in
the module** and it is unresolved for want of hardware.

### Also inside the DS18B20 driver

* **The fault is named, not folded.** All six `DallasTemperature` faults arrive at
  `TempSensorDallas` as `-127` (see
  [`09 §17`](./09-cpp-findings.md#17-) — corrected in this pass), and the C++
  reports all six as *"Temperature sensor not connected"*. The **decision** is
  identical; the diagnostic is not. Tests:
  `onewire::div7_every_ds18b20_fault_is_rejected_by_the_cpp`,
  `ds18b20::div6_every_ds18b20_fault_is_rejected_and_named`.
* **The dead range check is now applied** — see below.

---

## 8. The Dallas temperature path applies `isValidTemperature`'s range 🔴 added

| | |
| --- | --- |
| **Finding** | [09 §18](./09-cpp-findings.md#18-) — `TempSensor::isValidTemperature` is dead and the DS18B20 path has no range check |
| **Test** | `cc_domain::sensor::ds18b20::div8_the_dallas_path_applies_the_range_check_it_never_applied`, `::div8_the_two_ranges_overlap_only_between_zero_and_a_hundred_and_fifty`, `::div8_only_the_cold_end_of_the_ds18b20s_range_is_now_refused`, `::div8_a_reading_outside_the_range_is_a_read_failure_not_a_hot_temperature` |

### What the C++ does

`TempSensor::isValidTemperature` (`TempSensor.h:91-93`) is a `static constexpr`
predicate for **-50..150 °C** and is **never called** — not from
`updateTemperature` (`:31-54`), not from `tryGetValue` (`:110-146`), not from
anywhere in the tree. So on the Dallas path a 165 °C reading is accepted, cached,
folded into the 15-sample moving average and handed to the PID. The only range
that acts is S1's, in `EmergencyStopManager::checkEmergencyConditions`
(`EmergencyStopManager.cpp:25-30`): 0.0..200.0, outside which emergency stop
latches immediately with no debounce.

The TSIC driver, by contrast, has its own reject at `temp <= 0.0 || temp >= 180.0`
(`TempSensorTSIC.cpp:59-62`).

### What the Rust does

`cc_domain::sensor::ds18b20::Driver::poll` applies `isValidTemperature`'s own
range, and a reading outside it is a **rejected read**
(`Ds18b20Fault::OutOfRange`) rather than a temperature.

### Why, and what it costs

The two families now cannot disagree about what a plausible reading is, and the
check the C++ wrote is the check it runs. The cost is real and is stated rather
than argued away:

* The **reachable** part of the change is small. The DS18B20's own range is
  **-55..+125 °C** (AT24+DS18B20), so the only real readings the new check
  refuses are the **-55..-50 °C** band. The upper bound is inert: the sensor
  cannot report above 125, so nothing above `PLAUSIBLE_RANGE.1` = 150 is
  reachable at all. `div8_only_the_cold_end_of_the_ds18b20s_range_is_now_refused`
  measures this rather than asserting it.
* The **diagnostic** does change for the 150..200 band S1 used to see: a genuine
  over-temperature now arrives as a *sensor read failure*, so after ten of them
  the machine reaches `SENSOR_ERROR` rather than `EMERGENCY_STOP`. The heater is
  off in both cases, but an operator is told "sensor error" instead of "too
  hot". Failing the other way — accepting 165 °C into the PID — is what the C++
  does today, and the DS18B20's own TH/TL alarm registers are the right place to
  make that distinction precisely.
* The alternative considered and rejected: keep the check a **query**
  (`is_plausible`) and let S1 act. That was the previous revision's position, on
  the grounds that filtering here turns an emergency stop into a silently-held
  last-good value. It is a defensible position, and it is not this one, because
  the user asked for the check the C++ wrote to be applied and because a driver
  that returns 165 °C as a temperature is not reporting what it measured.

### 🔴 The TSIC driver's own range check is *not* symmetric, and half of it is dead

`temp <= 0.0 || temp >= 180.0` is **preserved verbatim**, inclusive at both ends.
Two consequences, both pinned:

* `temp >= 180.0` **cannot fire on a TSIC-306**: the sensor's span ends at
  150 °C. `tsic306::tests::the_cold_extreme_is_rejected_and_the_hot_extreme_is_not`
  is the test that says so, and it is why a 150.00 °C boiler reading is accepted
  by both the C++ and this port.
* `temp <= 0.0` cannot fire at exactly 0.00 °C either, because 0.00 is not on the
  11-bit grid: the nearest codes are -0.024 °C (DS 511) and +0.024 °C (DS 512).
  The bound's first victim is DS 511
  (`tsic306::tests::the_lower_bound_fires_from_raw_511_downwards`).

The constant is shared with the TSIC-506 (`ZACwire.cpp:62-63` switches formula on
`_sensor < 400`), which is why it is left alone.

---

## 9. The heater is chopped by a 10 ms `GPTimer` ISR, not by LEDC 🔴 changed

| | |
| --- | --- |
| **Finding** | [09 §17](./09-cpp-findings.md#17-) — the original ESP32 cannot use LEDC at a low carrier |
| **Test** | `cc_domain::heater::isr_tests::*` (13 tests), `cc_domain::heater::atomic_chopper_tests::*` (7), `cc_domain::heater::transport_tests::*` (5) |

### What the C++ does

The C++ chops the heater relay in a 10 ms hardware-timer ISR
(`include/clevercoffee/isr.h:85-118`) at the highest interrupt priority the chip
offers:

```cpp
if (currentPidOutput <= currentCounter) relay->off(); else relay->on();
unsigned int newCounter = currentCounter + ISR_COUNTER_INCREMENT;   // 10
if (newCounter >= ctx->processWindowSize()) newCounter = 0;         // 1000
```

with `Timing::ISR_TIMER_INTERVAL_US = 10000` and
`Timing::ISR_COUNTER_INCREMENT = 10` (`constants/Timing.h:15,17`). The lost
firmware did the same
([08 §3](./08-recovered-oracle.md): *"heater interrupt running on GPIO2 (active
high), 1000 ms window"*).

### What the Rust did, and what it does now

R1-07 replaced the ISR with a **1 Hz LEDC hardware carrier**, on an argument
that is correct and is the reason LEDC would be preferred: the pin is a
contactor, an `f` Hz square wave makes `2f` contactor operations per second, and
two per second is the right budget for a 2 kW boiler contactor.

**That was reversed, because the LEDC driver cannot run on this chip.**
`components/hal/esp32/include/hal/ledc_ll.h:485-489`, on the original ESP32 only:

```c
// wait until the last duty change took effect (duty_start bit will be
// self-cleared when duty update or fade is done)
// this is necessary on ESP32 only, otherwise, internal logic might mess up
while (hw->channel_group[speed_mode].channel[channel_num].conf1.duty_start);
```

`duty_start` is cleared by the hardware at the next **timer period** and the
spin is inside `portENTER_CRITICAL(&ledc_spinlock)`
(`components/esp_driver_ledc/src/ledc.c:1603-1606`), with interrupts masked. At
1 Hz that is up to **one second**; the original ESP32's interrupt watchdog is
**300 ms** (`components/esp_system/int_wdt.c`). Every duty write trips it —
**including the duty-0 write in `LedcPwm::new`**, so the firmware panicked on
every boot before the control task ran.

Every other `ledc_ll.h` in this tree (`esp32c2`, `esp32c3`, `esp32c5`, and the
s3/h2/p4 equivalents) has the loop removed, so this is a property of *this* chip
and the requirement is in direct conflict with the contactor-friendly carrier:
any carrier slow enough to be mechanically kind is slow enough to trip the
watchdog through that spin.

**The 10 ms `GPTimer` ISR is the default again.** The decisions — the window, the
10 ms quantisation, the counter's own state, the gate, the armed check — are all
in `cc_domain::heater` (`IsrChopper` for the reference state machine and
`AtomicChopper` for the ISR-shaped one, with
`the_atomic_chopper_and_the_reference_agree_for_a_whole_window` proving they are
the same function), and the whole tick-by-tick pattern is walked on the host at
every counter value for every duty the C++ can express. The device crate's
callback is that call, a branch on its result, three diagnostic counters and one
GPIO write — which is 04 §3.1's "nothing beyond one GPIO write" plus the
arithmetic the C++ also does in its ISR.

### The cost, stated rather than hidden

The ISR changes the relay level **100 times a second** where LEDC would have
changed it twice. That is what the C++ has always done and what the contactor has
always survived, so the trade is **contactor wear, not watchdog panics**. 100
interrupts a second on a 240 MHz Xtensa is 0.04 % of one core, against a 300 ms
watchdog.

### What is kept and what is dead

`LedcPwm` stays in `cc-hal-esp32` **unbrought-up**, behind the same `HeaterDuty`
seam, with the 1 Hz argument and the divider table intact — for a target whose
chip has no spin. `cc-firmware` has **no LEDC construction site at all** and a
`const _: () = assert!(!BRING_UP_HEATER_LEDC, ...)`, so no future edit can reach
a duty write by accident.

### ⚠ Not yet verified

`just flash` with the ISR build **panicked on its first bring-up** on a
configuration error in the `GPTimer` alarm setup (`reload_count` must differ from
`alarm_count` when auto-reload is on — see
`components/esp_driver_gptimer/src/gptimer.c`, `gptimer_set_alarm_action`).
That is fixed in source; **it is not yet confirmed on hardware**, because the
two-flash budget for this task was spent. The thing this entry was written to
verify — that the device **boots without an INT WDT panic** — *is* confirmed: both
boots reached the control loop's setup with no watchdog panic, and with no LEDC
construction site the panic is structurally impossible. R1-07's scope-and-duty
measurement against a dummy load is still **not** done and **R1-07 stays open**.

---

## 10. R3-05: the ABP2 read no longer blocks, and checks what the C++ ignores 🔴 changed

| | |
| --- | --- |
| **Task** | R3-05 |
| **Tests** | `cc_domain::abp2::tests::div1_a_short_read_is_an_error_not_a_stale_sample`, `::div2_a_nack_on_the_command_is_an_error`, `::the_first_poll_writes_the_command_and_returns_immediately`, `::the_cpp_would_have_slept_twenty_percent_of_the_loop` |

### What the C++ does

`measurePressure()` (`pressureSensor.h:35-40`), called from
`SensorCoordinator::updatePressure` on **every loop iteration**, with
`PRESSURE_UPDATE_INTERVAL_MS = 50` (`SensorCoordinator.h:271`):

```cpp
int stat  = Wire.write(ABP2_cmd, 3);
stat     |= Wire.endTransmission();
delay(ABP2_READ_DELAY_MS);        // 10 ms, unconditionally
Wire.requestFrom(ABP2_id, static_cast<uint8_t>(7));
```

Three things, all of them wrong in the same direction:

* `delay(10)` out of every 50 ms is **20 % of the control loop asleep**,
  permanently, whether or not anything is brewing.
* `stat` is computed and **never read**, so a NAK on the address is invisible.
* `requestFrom`'s return value is discarded, so a short read leaves stale bytes
  in the `ABP2_data` globals and the firmware converts **the previous sample**
  as if it were fresh.

### What the Rust does

* The 10 ms is a **deadline**, not a sleep. `Driver::poll` writes the command on
  one tick and reads on a later one; the loop's own sleep covers the wait. The
  driver never asks the caller to block.
* `endTransmission`'s result and the byte count are both checked
  (`div1`, `div2`). A short read is an error and the previous value is kept.
* The update rate is **unchanged**: the next command is anchored on the previous
  *command*, not on the read, so the period is still 50 ms with the 10 ms inside
  it rather than 60 ms.

### Why

This is one of the only two real performance wins in the migration (the other is
R1-07's LEDC carrier, which is currently blocked — see
[`09-cpp-findings.md` §17](./09-cpp-findings.md#17-)). A plausible pressure built
from bytes the sensor never sent would flow into the brew pressure control.

### Known, and deliberately not changed

* `counts_to_percentage` divides by the **full scale** while `counts_to_bar`
  divides by the output span, so 10 bar reads as 90 % rather than 100 %
  (`pressureSensor.h:50` vs `:53-54`). The C++ does this and it is preserved and
  pinned, because the percentage is a log field and the bar figure is the one
  the control loop uses.
* A pressure count below the part's offset yields a **negative** pressure and
  the C++ carries it on. Preserved; `is_below_range` exposes the condition so the
  decision can be made rather than smuggled in. The DS18B20 range decision
  ([#7](#7-r1-03--r3-07-both-temperature-sensors-are-implemented-🔴-changed)) is
  the same kind of call and went the other way, because there the C++ was
  *claiming* to read a sensor it was not.

### Also changed, and smaller

The I²C bus runs at **400 kHz**, where Arduino's `Wire.begin()` defaults to
100 kHz. 400 kHz is the ABP2's maximum and comfortable for the SSD1306 on the
same bus, so it is strictly less bus time for a shared peripheral.
