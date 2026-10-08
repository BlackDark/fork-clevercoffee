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

## 1. Both pump safety timeouts are armed 🔴 closed {#d01}

| | |
| --- | --- |
| **Finding** | [09 §11](./cpp-findings.md#cf11) — "Both pump safety timeouts are dead code" |
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
  firmware, [08 §4.2](./recovered-oracle.md)). It is emitted **before** the
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

```ledger
{"id":"div1","heading":"## 1. Both pump safety timeouts are armed","scenarios":["brew_by_time","brew_aborted_mid_flow","backflush_full_cycle","steam_on_off","water_tank_empty_mid_brew"],
 "matchers":["/effect rust:.*PumpTimeoutFired/","/log .*Pump timeout - stopping for safety/","/log .*Hot water pump timeout - stopping for safety/"]}
```

---

## 2. The steam valve is whitelist-gated 🔴 added {#d02}

| | |
| --- | --- |
| **Finding** | [09 §2](./cpp-findings.md) — "The steam valve has no safety whitelist at all" |
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

```ledger
{"id":"div2","heading":"## 2. The steam valve is whitelist-gated","scenarios":[],
 "matchers":["/effect rust:CloseSteamValve/","/effect cpp:.*CloseSteamValve/","/actuator.steam_valve/"]}
```

---

## 3. The water valve is gated on the water tank 🔴 added {#d03}

| | |
| --- | --- |
| **Finding** | [09 §3](./cpp-findings.md) — "The water valve is not gated on an empty tank" |
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

```ledger
{"id":"div3","heading":"## 3. The water valve is gated on the water tank","scenarios":["water_tank_empty_mid_brew","water_tank_refill","ota_start_from_idle","ota_start_during_brew"],
 "matchers":["/effect rust:.*OpenWaterValve/","/effect cpp:.*OpenWaterValve/","/actuator.water_valve/"]}
```

---

## 4. The PID derivative is taken over the real elapsed time 🔴 fixed {#d04}

| | |
| --- | --- |
| **Finding** | [09 §1](./cpp-findings.md) — `PID_v1.cpp:85`, integer division by zero |
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

```ledger
{"id":"div4","heading":"## 4. The PID derivative is taken over the real elapsed time","scenarios":[],
 "matchers":["/effect rust:SetHeaterDuty/","/effect cpp:SetHeaterDuty/"]}
```

---

## 5. ~~The heater is driven by LEDC, not a 10 ms ISR~~ — superseded by #9 🔴 reversed {#d05}

| | |
| --- | --- |
| **Superseded** | 2026-09-28, by [#9](#9-the-heater-is-chopped-by-a-10-ms-gptimer-isr-not-by-ledc-🔴-changed) |
| **Why** | R1-07's 1 Hz LEDC carrier **panics the original ESP32 at boot** — `ledc_ll_set_duty_start` spins inside `portENTER_CRITICAL` for up to one carrier period with interrupts masked, against a 300 ms interrupt watchdog. The spin is unique to this chip: every other `ledc_ll.h` in the tree has it removed. |
| **What survives** | The **argument** for a low carrier, and the divider arithmetic (`esp_driver_ledc`'s `div_param` formula, the 17/18/19/20-bit table). Both are kept in `cc_hal_esp32::heater`'s module docs and both are what a different target would use. The `LedcPwm` transport they belonged to has since been **deleted** — see #9's "What is kept and what is dead". |
| **Hardware** | The 1 Hz carrier was measured panicking on 2026-09-28. The 10 ms ISR replacement has **not** yet been confirmed running — see #9's "Not yet verified". |

The text below is left as the record of what R1-07 decided and why, because the
*reason* is still correct and the next person to look at the heater needs it.

## Preserved C++ behaviours — do not "fix" these {#preserved}

**Deliberately unnumbered, because these are not divergences.** The numbered
sections below are places this firmware does something *different*; this one is
a list of places it does the same thing on purpose. They are in this file so
nobody "fixes" them.

These are C++ bugs the port **reproduces on purpose**. A diff at any of these is
*not* a regression; the `s<N>_`-prefixed tests in
`cc-machine/tests/parity_findings.rs` pin each one, and the `s<N>` numbering is
[09](./cpp-findings.md)'s.

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

## 6. `cc-display` does not implement `embedded-graphics::DrawTarget` 🟡 open {#d06}

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
  [`docs/display/parity.md`](../../docs/display/parity.md)) and U8g2's
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

**Deliberately unnumbered**, like the section above it: this is a collection, not
one divergence. It is read by `cc-parity` all the same, and its ledger entry
(`ota_gap`) names this heading verbatim, which is why the number is absent rather
than merely omitted from the index.

Recorded here so the file is complete; each was decided in its own task.

| What | Why | Pinned by |
| --- | --- | --- |
| `safety.emergency_temp` and `safety.emergency_hysteresis` are registered and persisted | The C++ defines and reads them but omits them from `getAllConfigParams()` (`src/Config.cpp:438-563`), so they silently reset to the compiled default on every reboot — a live bug on safety path S1 | `cc-config/tests/config_schema.rs` |
| Cross-parameter config validation, and refusing to store or run an unsafe one | Recovered from the previous Rust firmware ([08 §4.1](./recovered-oracle.md)). The C++ has neither | `cc-safety::tests::safety_paths.rs::config_*`, `::store_refuses_an_unsafe_config` |
| A `LOW_TRIGGER` heater relay is refused outright | An undriven GPIO during reset would energise a 2 kW heater. No firmware can prevent it; it is a wiring property | `cc-safety::tests/safety_paths.rs` |
| The `SetHeaterDuty` effect is gated on `may_heat` at emission, where the C++ emits the value and zeroes it microseconds later | Fail-safe rather than fail-fast. The machine state is identical; the port never asks the heater to be on when it must not be | `cc-machine/tests/*` |
| `Duty` is bounded at `0..=1000`, i.e. the PID output is a **millisecond** duty, not `setHeaterPower`'s `uint8_t` percent | The C++ heater path is a PWM window compared against the PID output, and `HardwareManager::setHeaterPower` (`HardwareManager.cpp:305-318`) is a TODO stub | `cc-domain/src/units.rs::tests::duty_bound_is_the_chopper_window` |
| Blocked steam mode being requested from the C++ test suite (`test_steam_handler`, `test_steam_water_injection`) | 27 `#[ignore]`d records of C++ mock cases with no Rust equivalent, each with a comment saying why | the `#[ignore]` attributes themselves |
| SSE over WebSocket for the UI's live channel | R1-05; `EspHttpConnection::write` (chunked) and `raw_connection().write_all` both ship in `esp-idf-svc` 0.53 | R1-05 |

```ledger
{"id":"ota_gap","heading":"## Also intentional, from before this file existed","scenarios":["ota_start_from_idle","ota_start_during_brew"],
 "matchers":["/effect rust:SafeHardwareShutdown/","/effect cpp:.*SafeHardwareShutdown/","/effect rust:EnablePump/","/effect cpp:EnablePump/"]}
```

---

## 7. R1-03 / R3-07: both temperature sensors are implemented 🔴 changed {#d07}

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
[01 §"The temperature sensor fitted to this machine is a DS18B20"](./feature-inventory.md)
— so the C++ default constructs a TSIC-306 driver pointed at a 1-Wire bus it
does not own, and the recovered firmware logged the substitution rather than
refusing it
([08 §4.1](./recovered-oracle.md)).

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
  [`09 §17`](./cpp-findings.md#cf17) — corrected in this pass), and the C++
  reports all six as *"Temperature sensor not connected"*. The **decision** is
  identical; the diagnostic is not. Tests:
  `onewire::div7_every_ds18b20_fault_is_rejected_by_the_cpp`,
  `ds18b20::div6_every_ds18b20_fault_is_rejected_and_named`.
* **The dead range check is now applied** — see below.

---

## 8. The Dallas temperature path applies `isValidTemperature`'s range 🔴 added {#d08}

| | |
| --- | --- |
| **Finding** | [09 §18](./cpp-findings.md#cf18) — `TempSensor::isValidTemperature` is dead and the DS18B20 path has no range check |
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

## 9. The heater is chopped by a 10 ms `GPTimer` ISR, not by LEDC 🔴 changed {#d09}

| | |
| --- | --- |
| **Finding** | [09 §17](./cpp-findings.md#cf17) — the original ESP32 cannot use LEDC at a low carrier |
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
([08 §3](./recovered-oracle.md): *"heater interrupt running on GPIO2 (active
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
**including the duty-0 write an `LEDC` transport's constructor makes**, so the
firmware panicked on every boot before the control task ran.

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

**Dead: the `LedcPwm` transport, and the `HeaterDuty` seam it justified.** The
transport was ~190 lines with **zero construction sites**, self-documented as
*"not brought up, and not bringable on this chip"*. It existed so that
`HeaterOutput<HeaterDuty>` would have two impls and the word "swappable" would
be true. Both are deleted. `HeaterOutput` is now a concrete struct holding a
`TimerIsrPwm`, and its `set_duty` no longer returns a `Result` — the only
fallible call in the chain was `LedcPwm::apply`'s `ledc_set_duty_and_update`.

**Dead: `telnet::pump` and `telnet::LineBuffer`.** ~100 lines with no caller in
`cc-firmware`, whose doc called them *"staged for the R3-16 transport"*. The
staging claim did not survive the oracle: **the C++'s `Logger` never reads a
line from a telnet client.** `Logger::update` (`src/Logger.cpp:123-156`) accepts
a client, writes the banner, flushes the ring and pumps the heartbeat, and
`rg 'client_\.'` over the file finds only `write`, `flush`, `stop` and
`connected`. So this was not staged parity work but a *read* path with no C++ to
match, and `LineBuffer` existed only to feed it.

**Measured, and it contradicts the obvious reasoning: the deletion cost 0 B.**
The argument for keeping unreachable `pub` code is that LTO cannot drop it,
because an rlib exports its public symbols. That is true of an incremental build
and false of this one: `lto = "fat"` with `codegen-units = 1` sees the whole
program, so an uncalled `pub fn` in an rlib **is** removed. The release image is
**byte-identical** with and without the two functions (1,697,488 B either way) —
which is why the honest reason to delete is maintenance, not flash: 110 device
tests that only a chip can run, a `[u8; 256]` stack buffer nothing shipped
reaches, and a doc claiming a C++ behaviour that does not exist. The `embedded-io`
dependency went with it; its only user was the fake reader behind `pump`'s test.

**Kept: everything that is a fact about the hardware or about the C++.** The 1 Hz
argument, the `div_param` arithmetic and the 17/18/19/20-bit table are in
`cc_hal_esp32::heater`'s module docs; `CARRIER_HZ` and `RESOLUTION` are still
there and are still `const`-asserted against `cc_domain::heater`'s
host-tested `CARRIER_HZ` / `CHOSEN_RESOLUTION_BITS` / `CHOSEN_MAX_DUTY`, so the
numbers cannot drift away from the tests. A target whose chip has no spin re-adds
a transport from those, and the three things that cost the deleted one — own the
timer driver, duty 0 is the safe state, clamp before the register — are listed in
that module's docs rather than left to be rediscovered. The same applies to the
telnet pair: the 256 B line bound (ADR-0002 decision 1) and the whole shed policy
stay, `LINE_BUFFER_BYTES` is still `web.rs`'s `drain_body_bounded` limit, and a
future client-read feature re-derives its splitter from the C++ — which has none.

`cc-firmware` has **no LEDC construction site at all** and there is no LEDC
transport type to construct, so no future edit can reach a duty write by
accident.

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

## 10. R3-05: the ABP2 read no longer blocks, and checks what the C++ ignores 🔴 changed {#d10}

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
[`cpp-findings.md` §17](./cpp-findings.md#cf17)). A plausible pressure built
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

### Implemented at R3-17 (HX711)

The driver is built and started. The differences that are choices rather than
transcriptions:

* **The spin loops are gone entirely.** `HX711Scale.cpp:44` and `:51` are
  `while (!startMultiple(...))` — unbounded, and with a `static` deadline
  (`HX711_ADC.cpp:135`) that a second call inherits already-expired. There is no
  timeout loop here at all: a read is a query (`cc_domain::sensor::hx711::
  read_raw`) and presence is `SignalWatchdog`, a type that owns its deadline.
* **Presence is armed at driver start, not at the first conversion.** The C++
  arms by accident (`lastDoutLowTime = millis()` before the first `update()`);
  a port that armed on the first *reading* would never fault an absent scale,
  which is the C++'s defect reproduced in new code. Caught on hardware — see
  the test `a_cell_that_never_converts_is_faulted_from_the_moment_the_driver_
  starts`.
* **A fault advances the cell alternation.** The shared SCK has already clocked
  whichever cell was selected whatever DOUT said, so leaving the alternation
  where it was would make a dual cell read on alternate clock pulses against a
  half-rate pipeline. The C++ has the same alternation and does not have this
  problem only because it never runs.
* **`isConnected` is a query, not a flag only a successful read can clear.** In
  the C++ the flag is set inside `update()` and read by `HX711Scale::isConnected`
  (`HX711Scale.cpp:196-210`), so a scale that is never read is never declared
  faulty.
* **`dataOutOfRange` is read.** The C++ sets it (`HX711_ADC.cpp:371-374`) and
  never looks at it again, anywhere.
* **The tare survives a reboot.** The C++ holds it in a `long` member
  (`HX711_ADC.h:66`) and loses it on every reset. It is persisted under the NVS
  key `cc.scale.tare` — a separate key, not a configuration field, because a
  tare is not a parameter and has no range, default or UI.
* **The calibration factor is persisted into the configuration blob**, so
  `hardware.sensors.scale.calibration` / `calibration2` mean what they say
  across a reboot rather than being re-typed.
* **`hardware.sensors.scale.type` selects one or two cells.** The C++ honours
  the same enum in `HX711Scale`'s two constructors and never constructs either;
  a `Bluetooth` type is refused by name, because reporting 0 g for a scale that
  is not an HX711 is the "accepted and silently does nothing" shape again.
* **The MQTT weight topics are the ones `cc_config::discovery` advertises.**
  `MQTTManager.cpp:903-904` registers `currReadingWeight` and `currBrewWeight`;
  the registry used to publish a topic called `weight`, which matches neither,
  so the Home Assistant weight entity was advertised and never updated. The two
  sides live in different crates and nothing made them agree until
  `the_weight_topics_are_exactly_the_ones_discovery_advertises` did.
* **The MQTT registry is `SystemInitializer.cpp:687-800`.** See section 18 —
  three parameters and three "sensors" became 32 parameters and 13 sensors, and
  two invented topics went the other way.

### Still open: the Acaia BLE scale (R3-18)

---

## 11. MQTT actually runs 🔴 changed {#d11}

| | |
| --- | --- |
| **C++** | `src/network/MQTTManager.cpp`, driven from `LoopManager::updateNetwork` (`LoopManager.cpp:485-508`) |
| **Rust** | `cc_hal_esp32::mqtt::{Client, Feed}`, built and driven by the **control task** (`cc-firmware/src/mqtt_link.rs`) |
| **Pinned by** | `mqtt::the_registry_is_the_csqs_registration`, `mqtt::an_inbound_reading_resolves_only_if_it_is_registered`, `mqtt::the_interval_follows_the_machine_state`, `mqtt::an_inbound_topic_is_parsed_exactly_as_the_csqs_matcher_does` |

### What the C++ does

`setup` (`:54-92`) builds the topics and refuses to continue when
`mqtt.broker` is empty. `checkConnection` (`:96-193`) and `loop` (`:194-196`)
are called from the main loop, and `writeSysParamsToMQTT` (`:366-570`) runs a
three-phase pass — retained parameters, non-retained sensors, retained binary
sensors — under a **10 ms budget** (`MQTTManager.h:259`), resuming from
`mqttVarsIt_` / `publishPhase_` on the next call, and skipping any topic whose
value has not changed (`mqttLastSent_`, `:512`). The interval is 500 ms while a
brew state other than `BREW_FINISHED` is active, 10 s in `STANDBY`, else 5 s
(`:384-387`). `sendHASSIODiscoveryMsg` (`:828-926`) republishes the whole Home
Assistant entity set every 300 s. `messageCallback` / `assignParameter`
(`:237-336`) act on `<base><param>/set`, with four special targets
(`STEAM_MODE`, `BACKFLUSH_ON`, `TARE_ON`, `CALIBRATION_ON`) that are machine
state rather than configuration.

### What the Rust does — and why

**The client lives in the control task.** It used to be constructed in
`bring_up` and read two lines later, which meant `EspMqttClient`'s `Drop` —
`esp_mqtt_client_destroy` (`esp-idf-svc` `src/mqtt/client.rs:742-747`) — ran at
the end of the `match` arm, microseconds after `esp_mqtt_client_start`. Nothing
anywhere called `publish`, `publish_online`, `discovery_due`,
`due_for_reconnect` or `subscribe`: a fully implemented client that never did
anything, and `/api/status`'s `mqttConnected` structurally always `false`. The
control task is the main loop — it owns the clock, the configuration, the
store, the machine and the only watchdog subscription — which is exactly where
`LoopManager::updateNetwork` puts the C++'s.

**The pass is the C++'s phase machine**, cursor and budget included
(`Feed::service`). The registry is `SystemInitializer.cpp:687-800` in full: 32
parameters and 13 sensors where three parameters and three "sensors" were
registered before, plus the value-change dedupe and the three intervals, none of
which existed. `discovery_due()` publishes `cc_config::discovery::all` and
`mark_discovery_sent()` re-arms the timer, so the 17–29 entities in that
module are no longer inert.

**The events are handled where they are delivered.** `EspMqttClient::new_cb`
installs the callback, so `Connected` / `Disconnected` / `Received` are noted on
the `esp-mqtt` task and what crosses back is two atomics and a bounded queue.
`EspMqttClient::new` was the alternative and its `EspMqttConnection::next()`
**blocks** on a condvar until the producer has something
(`src/private/zerocopy.rs:29-40`), with no non-blocking poll and no
`is_connected` accessor — a rendezvous on a 10 ms control tick, on the same
signal as the heater deadman. There is no synchronous accessor to use instead.

**Inbound commands work, and so does everything outbound.** `mqtt set <param>
<value>` resolves the topic through the registry's topic-to-key map and goes
through `cc_config::assign::apply`, exactly as `POST /api/parameters` does, then
pushes into the running machine through the shared `push_into_machine`. The four
specials are handled before the configuration lookup, as `assignParameter`
does. The web commands were *not* in the "subscribed and ignored" state before
this change, and neither was the outbound path — **this file previously said
only the inbound path was absent, which was wrong in the direction that
mattered**: the client was being destroyed before its first publish.

### The divergences inside it

* **The availability publish is retained; the C++'s is not.**
  `MQTTManager.cpp:400` calls `publish("status", "online")` against a
  `bool retain = false` default (`MQTTManager.h:291`) and survives only because
  it repeats every five seconds. A retained `online` costs one write per pass,
  is what the retained `offline` last will on the same topic already implies,
  and removes the window in which a broker restart leaves every entity
  unavailable.
* **The Home Assistant discovery set is published under the same 10 ms budget**,
  a few documents per control tick, where the C++'s 300 s timer callback builds
  and publishes all of them in one go. That callback runs from the main loop, so
  it is a stall measured in hundreds of milliseconds in the task that also runs
  the heater deadman.
* **`TARE_ON` and `CALIBRATION_ON` are latches that clear.** The C++'s
  `scaleTareMode_` / `scaleCalibrationMode_` (`SensorCoordinator.h:203-233`) are
  written by `setScaleTareMode` and read by `isScaleTareMode`; `git grep
  scaleTareMode_ main -- src` finds **no clear anywhere**, so a `TARE_ON` command
  latches to `1` for the life of the boot and the Home Assistant switch can
  never be turned off. Here the latch is cleared when the sampler answers
  (`SamplerEvent::Tared` / `Calibrated` / `Refused`).
* **An inbound parameter write is persisted.** `assignParameter`'s tail
  (`MQTTManager.cpp:325-336`) calls `fromString`, which writes the configuration
  singleton's field and nothing else, so a setpoint set from Home Assistant is
  gone after the next reboot. The store is this task's, so the write goes
  through it.
* **`currBrewWeight` publishes `0`.** The C++'s is
  `cachedWeight_ - preBrewWeight_` while `brewWeightTrackingActive_`
  (`SensorCoordinator.cpp:85-92`); this firmware has no brew-weight tracker, so
  there is no `preBrewWeight_` to subtract and the topic carries a constant.
  The topic is still registered, because the alternative — not registering it —
  is the "advertised and never updates" shape again, on a different axis.
  **This is now only a reporting divergence.** The same field,
  `Sensors::brew_weight`, is the state machine's own by-weight **stop
  condition** (`BrewStates.cpp:294-299`) and the weight arm of
  `recordBrewIfQualified` (`BrewStates.cpp:311`), and it used to be written as
  the literal `0.0` at `cc-firmware/src/main.rs` — which made both of those
  unreachable, not merely unreported. That was a P1 defect and it is fixed;
  see §24. The residual divergence is the *delta*: the C++ subtracts the
  pre-brew weight and this firmware does not, so the number a Home Assistant
  card shows is the weight on the tray rather than the weight in the cup.
* **`usePonM` is still the C++'s inconsistency.** `SystemInitializer.cpp:695`
  registers `pidUsePonM` while `MQTTManager.cpp:869` advertises the entity as
  `usePonM`, so the Home Assistant switch moves a topic the registry does not
  know and it lands in the "not found in mapping" arm. Reproduced; the fix
  belongs where the advertisement is built.
* **`steamON = 0` does not leave steam mode**, because
  `setSteamFirstActivated(false)` plus `setNormalOperationRequested(true)` does
  not leave it in the C++ either (`MQTTManager.cpp:296-302`). Reproduced rather
  than tidied into a real stop.
* **The "MQTT is off while brewing" guard is gone.** It was a `BREWING` flag
  that made every publish a no-op during a brew, justified as
  "`MQTTManager.cpp:113-115` is a hard stop on *every* MQTT call". Line 113 is
  inside `checkConnection` and is the *reconnect* guard;
  `writeSysParamsToMQTT` has no brew guard at all and selects a **500 ms**
  interval instead (`:384-387`). The flag was never set by any caller, so the
  behaviour was already parity and only the comment was wrong. `interval_for`
  now reproduces the three intervals.
* **Two invented topics are gone.** The registry published `brewing` and
  `tankEmpty`, which are neither in the C++ nor advertised by
  `cc_config::discovery` — no Home Assistant entity ever subscribed to them.

### Not covered, and said so

The zero-allocation claim is a property of `Feed`'s signature (reused buffers,
a map of fixed-size values keyed by the leaked registry, and a value producer
that writes into a `&mut Payload`), and it is **argued, not measured**:
`crates/cc-machine/tests/tick_allocations.rs` measures the reducer, not this
path. The firmware's own tick report (`control tick: worst … budget 10 ms`) is
the instrument that would show a regression. **A broker session has never been
exercised**: no broker, no hardware and no flashing were available, so every
claim about what the broker sees is derived from the C++ and from
`esp-idf-svc`'s source rather than measured.

---

## 12. R3-17 / R3-18: scale support exists at all 🔴 new {#d12}

`config` already carries `hardware.sensors.scale.*` (enabled, type, calibration,
calibration2, samples, known_weight) and `cc-display` already has the scale templates, so
**the Rust config and UI surface looked like parity already.** It is not: the C++ can never
read a weight, so the equivalent of every one of those settings is inert there.

**The C++ defect, in one line:** nothing calls `HardwareContext::setScale()`, so
`scale_` is always `nullptr`, and `src/main.cpp:145-150` only logs
`"Scale sensor support via SensorCoordinator"` under a comment saying the work is
pending. Full analysis, including the MQTT and web tare/calibration commands that accept
input and silently do nothing, is in [09 §23](./cpp-findings.md).

**What this means for the parity harness.** A scenario that exercises the scale has **no
C++ baseline to diff against** — the C++ produces no weight in any state. R3-17 and R3-18
are therefore *new functionality*, not divergence, and the diff classifier must not treat a
Rust weight against a C++ empty as a regression. That is a case the classifier has to
know about explicitly; a generic "C++ returned nothing" rule would silently swallow real
regressions elsewhere.

**Decided 2026-09-29 by the human who owns the hardware**, explicitly against the
recommendation this file's sibling sections previously carried: the deadness is a bug on
their side, not a decision to drop the feature. Both scales are ported and made to work.

**The C++ spin loops are not copied.** `HX711Scale.cpp:44` and `:51` spin unbounded
(01 §5). Rust gives them real timeouts; that is a deliberate difference, not an
accident.

---

## 13. The device's default hostname is `test-cc-rust`, not `silvia` 🔴 changed {#d13}

**Decided 2026-09-29 by the human**, replacing the C++ default in
`include/clevercoffee/defaults.h:14` (`#define HOSTNAME "silvia"`).

The reason is operational, not cosmetic: **during the migration the C++ and the Rust
firmware are on the same network.** Both answering to `silvia.local` makes it ambiguous
which firmware answered a request, and the two are *not* interchangeable — the Rust port
diverges deliberately (pump timeouts armed, steam valve whitelist closed, PID divide
fixed). A hostname that is unambiguous about which firmware is talking is worth more
than one that is brand-neutral.

`test-cc-rust` says both halves: this is a test device, and it is the Rust port.

**One definition, not several.** `cc_config::schema::DEFAULT_HOSTNAME` is the single
source. `Config::default()` and the schema's `system.hostname` both reference it, and
`config_schema.rs` asserts against the constant rather than a literal — so a rename
cannot half-apply. `docs/example_config.json` is kept in step by an existing import test
that parses that exact file, which is what makes the pairing a guard rather than a
convention.

**The C++ firmware is unchanged and still answers to `silvia`.** That is the point: the
two are distinguishable. Nothing in the port depends on the value — the name reaches the
netif only as `DHCPClientSettings::hostname`, so an operator can set `system.hostname` to
anything, including `silvia`, from the web UI.

`mqtt.password`'s default is also `"silvia"` (`defaults.h:54`). That is a **credential**,
not a name, and it is deliberately **not** renamed — it is a placeholder in both
firmwares and changing it in one would break a config the other reads.

---

## 14. The four operator switches default to `enabled: true`, not `false` 🔴 changed {#d14}

**Decided 2026-09-30 by the human**, who owns the machine, after pressing the switches
and finding that nothing happened.

The C++ defaults all four to `false` (`Config.h:985,1004,1023,1042`), and this port was
faithful to that. Faithful is why the buttons did nothing: `SwitchBank::poll` emits an
edge for every switch it reads, and `cc_machine` drops `ButtonPressed` for a switch whose
`hardware.switches.<name>.enabled` is `false` — so the human's presses were read off the
pins, debounced, and discarded. The boot log said so explicitly
(`switches.rs:214`: "the reducer will IGNORE this switch"), which is the log line that
identified it.

Four fields changed, in `cc-config` only:

| Parameter | C++ default | This firmware |
| --- | --- | --- |
| `hardware.switches.brew.enabled` | `false` | `true` |
| `hardware.switches.steam.enabled` | `false` | `true` |
| `hardware.switches.power.enabled` | `false` | `true` |
| `hardware.switches.hot_water.enabled` | `false` | `true` |

Two places had to move together, because a schema default that disagreed with the
struct default would render "default: false, value: true" in the config editor — a lie of
exactly the kind this file exists to prevent: `Config::default()` in
`crates/cc-config/src/config.rs`, and `ParamValue::Bool(..)` in
`crates/cc-config/src/schema.rs`.

### The risk, stated rather than assumed

**GPIO 34, 35, 36 and 39 are input-only and have no internal pull-up or pull-down.** The
pad is a bare input. The C++ asks for `GPIOPin::IN_HARDWARE`
(`src/hardware/HardwareManager.cpp:140,151,162,173`), which is `pinMode(pin, INPUT)`
(`GPIOPin.cpp:47-51`) — floating, with the board's external pull doing the work. This
firmware keeps that: `switches::OPERATOR_PULL` is `Pull::Floating`, **not**
`GpioIn::pull_for`'s `Pull::Down`, because ESP-IDF accepts a `Pull::Down` on GPIO34
without complaint while doing nothing at all.

So the default is now `true` on pins whose resting level depends on wiring this repository
cannot see, and that is a real hazard:

* A **floating** input wanders. `poll` is edge-detecting over the debounced level, and the
  debouncer settles after 20 ms, so a pin with no external pull will eventually present a
  settled *apparent* edge. On the brew switch that edge is `ButtonPressed`, and a
  `ButtonPressed` with the machine in `PidNormal` **starts a brew**.
* Whether that can happen depends entirely on whether the board has external pull-ups on
  those four pins. **That was not determinable from this repository** — there is no
  schematic in the tree, and the machine was not available for measurement during this
  change. The C++ does not have the problem because it leaves the switches disabled, so
  it never looks at the level.

The mitigation in place is the one the C++ also relies on: an edge must survive the 20 ms
debounce (`cc_domain::switch`), so a transient is not enough — a genuinely floating input
does settle, though. `Poll` reports each switch's settled level after the first settling
interval, so the resting state is visible in the boot log; **`brew` settling high in that
log is the signature of a missing pull and is the thing to look at first.**

**If a switch reads as permanently pressed, set its `enabled` back to `false` from the web
UI** — that is the same escape hatch the C++ has, and it does not need a reflash.

The change is made **because the human asked and they own the hardware**, not because the
floating-input risk was resolved. It is not resolved.

---

## 15. `/api/ota/status` sends `status` as a string, not an integer 🔴 changed {#d15}

OTA itself is deferred to R3-15 and nothing here implements it. What is registered is the
**route**, because the UI has an OTA tab and a `404` is indistinguishable from a firmware
that lost the feature.

The C++ serialises `doc["status"] = state.getUpdateStatus()` (`ota.cpp:738`), and
`Status` is an unscoped `enum`, so ArduinoJSON writes it as an **integer**.

The UI parses it as a **string**:

```ts
// ui/packages/frontend/src/lib/schemas.ts:59-72
export const OtaStatusSchema = z.object({
  status: z.enum(["idle", "downloading", "uploading", "processing", "complete", "error"]),
  …
});
```

`z.enum` rejects a number, so the C++'s integer would fail this firmware's validation and
`pollOtaStatus` would return `null` — the OTA page could not render at all
(`OTAUpdateSection.tsx:56-62`). This firmware therefore sends `"idle"` as a string.

The full C++ key set is present at idle values (`updating`, `updateInProgress`, `type`,
`uploadedSize`, `totalSize`, `filesystemPartition`, `ota.cpp:735-742`) so a client written
against the C++'s shape gets zeros rather than `undefined`.

**`error` is deliberately absent.** The C++ emits it only when an update has actually
failed (`ota.cpp:744-751`); "OTA was never built" is not a failed update, and reporting it
as one would make the UI raise a failure toast on a machine that simply has no OTA. The
absence is carried by `message` and `reason: "R3-15"` instead.

The three mutating routes (`/api/ota/firmware`, `/api/ota/filesystem`, `/api/ota/url`)
answer `501` with `unavailable_json("OTA", "R3-15")`. **501, not 404 and not 200**: the
route exists and this build declines to implement it, which is what 501 means.

---

## 16. A request to sleep is honoured with the PID disabled 🔴 changed {#d16}

**Closed 2026-09-30** by the human's decision, after the human found it by using
the UI: `POST /api/sleep` answered `202 {"accepted":true}`, the command reached the
machine (`control: command Sleep`), and the state never left `PID_DISABLED`.

The C++ cannot do this. Two defects compose — see
[09 §25](./cpp-findings.md#cf25):

* `PidDisabledState::update` clears `requestStandby_` via `clearAllActionRequests()`
  (`MachineStateContext.h:626`) **before** `checkTransitions` runs;
* `PidDisabledState::checkSpecificTransitions` (`PidStates.cpp:135-148`) never
  consults `isStandbyRequested()` at all — only `PidNormalState` and
  `EepromErrorState` do.

`pid.enabled` defaults to `false`, so this is the out-of-the-box path: the machine's
own web interface could not put it to sleep.

### What changed

1. **`Requests::clear_all` spares `standby`.** The drain exists so a stale *action*
   request cannot fire the moment the PID is re-enabled (S11). `requestStandby_` is
   not an action — it asks the machine to go somewhere rather than start doing
   something — and `STANDBY` re-arms nothing on entry, so a surviving flag costs
   nothing. The other ten flags are cleared exactly as the C++ clears them.
2. **`PidDisabled` honours the request**, which is what `PidNormalState` one line
   away already does.

### Verified on hardware, with the PID off throughout

```
POST /api/sleep -> 202,  isStandby=true,  state 95 (STANDBY)
POST /api/wake  -> 202,  isStandby=false, state 20 (PID_NORMAL)
display: present=true blanked=true frames=7 failed=0
```

The panel blanks as it should — seven frames drawn, then the 100 ms gate correctly
stops writing to a blanked panel.

Three pins in `parity_findings.rs` (`div13_*`) hold both halves: the request is
honoured, it is **consumed** on the transition that acted on it, and waking still
works — so closing this does not make standby a one-way door.

## 17. The status bar's uptime and `°C` column are laid out from the frame edge, not from a fixed x 🔴 changed {#d17}

**What changed.** Two numbers on the Standard (and Minimal, and Scale) template.

| | C++ | Rust | why |
| --- | --- | --- | --- |
| uptime | `drawStr(84, 0, "%02luh %02lum")` | drawn right-aligned to `128 − 1` | the format's width is a *minimum* |
| `°C` unit | `setCursor(currentValueX + 31, …)` | `currentValueX + 30` | `84 + 31 + 12` = 127, the last column |

**Why.** Both were reported by the human as "the old screen does not fit text":
the header time lost its trailing `m`, and the degree `C` was not on the panel.
Both are real and both are measurable:

* `"%02luh %02lum"` is a *minimum* width, not a fixed one. Past 100 hours the
  hours field grows a digit, and this machine is routinely up for weeks: at
  `x = 84` a 377-hour uptime is 47 px wide and ends at **131**, so the `m` is cut
  in half by the frame. Measured on the rendered framebuffer, not inferred.
* The `°C` unit is 12 px at `profont11` from `84 + 31 = 115`, so it ends at 127 —
  the last column. One pixel of glyph-width drift and it is off the panel.

**Cost.** Two pixel columns of difference from the C++ on the two busiest rows of
the default template. Pixel parity is explicitly not a requirement (07 §14), and
AGENTS.md's OLED rules — everything fits fully within 128×64, numeric fields in a
reserved fixed width — win over matching a layout that clips.

**What pins it.** `widgets::tests::a_long_uptime_is_right_aligned_rather_than_clipped`
(six uptimes from 0 h to 9,999 h) and
`widgets::tests::the_unit_column_ends_inside_the_frame_for_a_three_digit_value`
(the widest reading a boiler can report). The regenerated goldens
(`minimal.ppm`, `standard*.ppm`, `scale*.ppm`, `screen_heating.ppm`, the Modern
set) differ **only** by these two columns; every other pixel is where it was.


## 18. The inverted value field closes on the frame, and the `°C` unit is placed by its ink 🔴 changed {#d18}

**What changed.** Two numbers on the Standard and Minimal templates.

| | C++ | Rust | why |
| --- | --- | --- | --- |
| the brew/weight inverted box | `drawBox(x + 50, y+1, 78, 10)` | `drawBox(x + 50, y+1, 128 - (x + 50), 10)` | 78 is exact for **Scale** (row origin 0 → 50..127) and 33 px too wide for **Standard/Minimal** (origin 34 → 84..**161**) |
| the `°C` unit | `setCursor(valueX + 31, y)` | `valueX + 31` where it fits, otherwise right-aligned to end at column 126 | `"°C"` **advances 12 px and inks 17** |

**The report was "during brew the values under temp/set/brew do not fit properly".** Two distinct defects, both measured rather than eyeballed:

1. **The inverted field ran off the panel.** One constant cannot serve two row
   origins: `kValueColumnWidth = 78` (`DisplayWidgets.h:171`) was sized for the
   Scale template, and on Standard and Minimal the box starts at 84 and ran to
   161. Its right border was never drawn, so the field looked like it fell off the
   edge of the screen — with a brew or a weight in it, which is exactly when the
   field is there. The width is now what is left to the frame, which is 78 on
   Scale (byte for byte the C++) and 44 on Standard and Minimal.
2. **The `°C` was three columns off the panel.** It advances 12 px but inks 17 —
   the advance excludes the trailing side bearing — so a layout computed from
   `str_width` puts the last three columns of the `C` outside a 128 px frame.
   **This is the second time this number was computed from `str_width` here**,
   and both times it was wrong: the first fix reduced the offset from 31 to 30,
   moved it two pixels and left it clipped. `Font::ink_box` is the quantity that
   matters, and it is now what both the code and the test use.

The value is **right-aligned** to two pixels before the unit, which is what
AGENTS.md asks for (a fixed-width field, digits that do not shift): `"92.5"` is
23 px of ink and `"100.0"` is 29, so a fixed x would push a three-digit reading
into the unit on a boiler that is genuinely at 100 °C.

**Cost.** The Standard and Minimal value columns shift left by up to four pixels
and the unit moves from 115 to 110; the Scale template moves one pixel. Twelve
goldens changed, all of them confined to the two value rows — verified by the
diff bounds, which are exactly the value columns.

**What pins it.** `tests/languages.rs`:
`the_unit_is_inside_the_frame_and_clear_of_the_widest_value` (asserts the unit's
ink ends inside the frame and the widest reading does not touch it, per template)
and `the_inverted_field_ends_on_the_last_column` (asserts the field closes on
column 127, and that Scale is still exactly the C++'s 78 px).

**Also fixed, and it was a real port defect:** the portrait sensor-error screen
drew the *landscape* strings. The C++ carries `langstring_error_tsensor_ur[5]`
for the portrait screen (`languages.h:35,69-73`) and the port had dropped it, so
a **64 logical pixel**-wide panel received 111 px of ink and 91 px of it was
dropped. Restored for all three languages.


## 19. S1's over-temperature debounce counts probe *samples*, not control ticks 🔴 changed {#d19}

**What changed.** `cc_safety::Telemetry` gained a `sample_seq`, and `reduce`
advances the debounce **only when it changes**. `Sensors` carries the same
counter, and the firmware fills it from the DS18B20 driver's own conversion count.

**Why.** The C++ increments `emergencyTempReadingCount_` once per
`updateTemperature()` (`EmergencyStopManager.cpp:41-49`), which the coordinator
calls on a **400 ms** sensor cadence (`Timing.h:42`), so `DEBOUNCE_COUNT = 3` is
about **1.2 s** of sustained overheat — and `cc-safety`'s own doc comment says so
("roughly 1.2 s", `lib.rs:71-72`).

This port calls `reduce` **once per 10 ms control tick** with whatever the last
conversion produced, so without the counter the *same* reading was counted about
forty times and the debounce tripped in **30 ms**. A probe spike — a 1-Wire CRC
retry, a flash write, someone touching the probe on a boiler at 155 °C — would
latch an emergency stop in the middle of a brew that the C++ rides out. That is
the most user-visible divergence in the safety path, and it was invisible to every
test: each test called `reduce` once per reading, which is the *correct* calling
convention and therefore hid it.

The parity runner already modelled the 400 ms cadence (`sampled`), so it needed
no new modelling — only the same sequence threaded through, which is why its three
`overtemp_trip` scenarios still trip on the third reading and not the second.

**Cost.** One `u32` in `Telemetry` and in `Sensors`, one comparison in `reduce`, and
the test suite's calling convention updated to model successive readings.

**What pins it.** Two new cases in `cc-safety`'s suite: one sample delivered forty
times stays at one count and does not latch, and three *distinct* samples do; plus
one for the trap in the other direction — after `clear()`, a sample that is still
over threshold must count again, so a recovery is not swallowed.

## 20. The reboot request now shuts the hardware down before the 500 ms pause 🔴 changed {#d20}

`POST /api/restart` did `delay_ms(500)` **inside the control task** and then
reset. For those 500 ms the loop was not running: no heartbeat, no watchdog feed,
no interlock — while the heater ISR kept chopping at the last commanded duty. The
power-switch reboot branch, a few lines below, already did the right thing and
said why; this one did not. It now applies `Effect::SafeHardwareShutdown` through
`apply_one` before the pause, exactly as its sibling does.


## 21. The Scale template's five rows are re-pitched; the setpoint no longer disappears during a brew 🔴 changed {#d21}

**What changed.** The Scale template's content rows move from the C++'s
`16 / 26 / 26 / 36 / 46` to **`13 / 22 / 31 / 40 / 49`**.

**Why.** The C++ puts the **setpoint row and the brew row both at `y = 26`**
(`ScaleTemplate.h:24` and `:59-61`), and the brew row's inverted field is
`78 x 10` at `(x + 50, y + 1)` (`DisplayWidgets.h:170-171`). The brew field
therefore erases the setpoint's **label**, its **value** and its **`°C`** — the
whole row. Measured on the rendered framebuffer before the change: of the
setpoint's pixels, two fragments survived.

This is **not** hidden behind the fullscreen timer. `display.fullscreen_brew_timer`
defaults to **false** in both firmwares, so the Normal layout is what a real
Scale-template machine shows during a brew, and the collision is what an operator
sees. It is inherited verbatim from the C++, which has the same coordinates, the
same box and the same draw order.

**Why nine pixels and why from 13.** A ten-pixel pitch cannot hold five rows above
the progress bar at `y = 60`: the last row's ink ends at 58–61 and touches it. A
**nine**-pixel pitch fits, and starting at 13 puts the last row's ink at 50–58,
two clear of the bar. The adjacent inverted fields overlap by exactly one row —
the lower border of the upper box — and the later one is drawn after, top to
bottom, which is what the C++ already relies on.

**Verified both ways**, by rendering the same input before and after:

| | rows | the setpoint during a brew |
| --- | --- | --- |
| before | 17–24 Temp, 25–26 fragments | **gone** |
| after | 15–21 Temp, **24–30 Set**, 33–39 Brew, 42–48 Weight | **present, with its value and `°C`** |

**Cost.** The C++'s row positions move, so `scale.ppm`, `scale_fault.ppm` and
`scale_offline.ppm` change and the screen looks different from the baseline
firmware. What does not change is that nothing is clipped and nothing is erased.
Pixel parity on this template is deliberately given up; read parity is kept.

The rejected alternatives, for the record: **dropping the inverted field on Scale**
does not work — the brew *label* at `x 0..36` still overwrites `Set:` at
`x 0..24`; and **shrinking the field** is geometrically impossible, because both
rows use the same value column at `x = 50`, so any box wide enough for the brew
value covers the setpoint's too.

---

## 22. `/api/config/upload` exists, and takes `application/json` 🔴 new {#d22}

**What the C++ does.** `WebServerManager.cpp:725-762` registers
`AsyncURIMatcher::exact("/api/config/upload")` for `HTTP_POST` with an
`AsyncCallbackJsonWebHandler` and `setMaxContentLength(MAX_CONFIG_UPLOAD_SIZE)`,
`MAX_CONFIG_UPLOAD_SIZE = 16384` (`:48`). The line above its own registration
says what the body is: *"Config upload: application/json body
(AsyncCallbackJsonWebHandler buffers full body before parse)"*.

**What the Rust does.** The same route, the same body encoding, the same 16 KB
transport cap, and the C++'s response document verbatim —
`{"success":…,"message":…,"restart":…}` with `Connection: close`
(`sendConfigUploadResponse`, `:50-62`).

**Why this entry exists at all**, given it is parity: the route was *absent*, and
`ui/packages/frontend/src/pages/SystemPage.tsx:182` has a live "Upload
configuration" button that POSTs to it. An operator who clicked it got a 404.

**A correction to the task brief, recorded because it is load-bearing.** The
brief specifies "multipart/form-data body with a JSON part". The oracle does not
use multipart, and neither does the button: `SystemPage.tsx:178-183` reads the
selected file with `selectedFile.text()` and posts it with
`Content-Type: application/json` and no boundary. A multipart reader here would
have answered the live button with `400` — fixing the 404 by replacing it with a
different failure. The oracle and the UI agree, so the oracle won.

**Three deliberate differences.**

1. **The document is read into pairs, not deserialised into a `Config`.**
   `Config::importFromJsonObject` (`Config.cpp:323-345`) walks the parameters and
   applies the ones the document mentions; the C++'s `importFromJson` fills the
   gaps with defaults, which is right for seeding a fresh store from
   `/config.json` and wrong for an upload, where a document that mentions twelve
   keys must leave the other eighty-six alone. So `cc_config::json::document_pairs`
   returns the `(key, value)` pairs and the control task writes them with
   `cc_config::assign::apply` — **the same writer `POST /api/parameters` uses**.
   A second applier would be a second set of type rules, and the two would drift.
2. **An over-long body is refused, not truncated.** `drain_body_bounded` caps at
   the limit and stops, which is right for a form body and wrong here: a document
   cut short is a document whose keys are individually valid and collectively a
   different machine. Truncating would apply *half a configuration* — a new PID
   gain with the old emergency cut-off — to a machine that may be mid-shot. So
   `drain_body_checked` returns `None` the moment the body exceeds the cap, the
   route answers `413`, and nothing is parsed. The C++'s `Connection: close` on
   every response of this route (`:60`) is **not** reproduced: ESP-IDF already
   purges a body a handler left unread (`httpd_req_delete`,
   `httpd_parse.c:841-855`), so the tail cannot be parsed as the next request on
   a kept-alive socket.
3. **An invalid value rejects the whole document.** Already this crate's
   documented position for `json_import` (`cc-config/src/lib.rs`, difference 3),
   and it is *more* important here: the C++ logs a warning per bad parameter and
   reports success if one imported, so an operator uploading a configuration with
   an out-of-range `safety.emergency_temp` gets `200` and runs on the old value.
   The C++'s own error message (`:750`) already claims to reject "invalid values".

**What pins it.** `cc-config/tests/config_schema.rs`, section *POST
/api/config/upload* — fourteen cases including
`an_upload_carries_only_the_keys_it_mentions` (the difference that matters),
`an_upload_with_one_bad_value_returns_nothing_at_all` (nothing to half-apply),
`an_every_pair_from_an_upload_is_accepted_by_the_one_writer` (the two validators
cannot disagree) and `an_oversized_document_is_refused_rather_than_truncated`.
On the wire: `cc-hal-esp32::web::tests::the_config_upload_route_is_registered`,
`::the_upload_response_is_the_cpp_shape`,
`::the_upload_body_is_bounded_and_the_cap_is_the_cpps`.

## 23. HTTP Basic authentication is implemented, and is boot-time 🔴 new {#d23}

**What the C++ does.** `WebServerManager::setupMiddleware`
(`WebServerManager.cpp:272-296`) installs `AsyncCorsMiddleware` and, when
`Config::systemAuthEnabled` is set, an `AsyncAuthenticationMiddleware` with realm
`"CleverCoffee"` (`:286`).

**What the Rust does.** The same control, on every route, plus `OPTIONS`.

**Why this entry exists at all.** `system.auth.enabled/username/password` were
registered in `cc-config`'s schema, writable through `POST /api/parameters`,
readable through `GET /api/parameters` — **and did nothing**. A key an operator
can set that silently does nothing is worse than an absent key: it looks like a
security control. A 2026-10 review (finding H-6, fixed by `71fcf364`) calls
this the repo's own named anti-pattern.

**The alternative, and why it was rejected.** Deleting the three keys was the
brief's other option and is defensible. It was not taken because it is a *worse*
security outcome for the operator this firmware replaces: an operator who had
`system.auth.enabled` set on the C++ machine, flashed this firmware, and deleted
the keys would have gone from "protected" to "wide open" with nothing in the
migration telling them. Implementing restores the control the C++ has.

**Four properties of the C++ that are reproduced rather than improved**, because
each is the C++'s behaviour and each is a decision an operator needs to know
about:

1. **Boot-time.** The C++'s middleware is installed once, from
   `WebServerManager::initialize`, so enabling `system.auth.enabled` protects
   nothing until the next reboot. Same here — and unlike the C++, this port
   *says so*: `needs_reboot` lists `system.auth.*`, so a write answers with
   `requiresRebootKeys: ["system.auth.enabled"]`. An operator who sets it, sees
   `200`, and is still serving an open API is the exact lie this repository calls
   out, and this is the answer to it.
2. **Empty credentials mean no authentication at all.** `:290-294` logs
   *"Web authentication enabled but credentials not set"* and serves the API
   open. Reproduced, with the same warning. Failing closed instead would mean
   that enabling auth and then not finishing locks an operator out of a machine
   whose only other console is a UART.
3. **`/events` is protected.** `EventSource` cannot *set* an `Authorization`
   header, but HTTP Basic credentials are cached per origin and realm once a
   browser answers a challenge, and it replays them on subsequent same-origin
   requests including this one — so the stream works after the operator has
   logged in through the UI. The C++'s middleware covers `/events` too, so this is
   the same posture, not a new one.
4. **The static UI is protected**, so the login prompt appears on a top-level
   navigation where a browser can actually show it. No UI change is needed: once
   the browser has the credential cached it sends it on every same-origin
   `fetch`.

**Two things this is not.** It is **not TLS**: Basic auth base64-encodes and does
not encrypt, the C++ has no HTTPS listener either, and a credential crosses this
LAN in the clear. And it has **no retry limiting and no lockout**, which the C++
also lacks — an attacker on the LAN gets unlimited guesses.

**What pins it.** `cc-domain/src/http_auth.rs` — 23 host tests over the
credential check, including
`a_username_that_is_a_prefix_of_the_real_one_is_refused`,
`a_password_containing_a_colon_survives` (RFC 7617 §2 splits at the *first*
colon), `malformed_base64_is_refused` and
`base64_agrees_with_a_full_alphabet_round_trip`. That module is in `cc-domain`
and not `cc-hal-esp32` **because `cc-hal-esp32` cannot be tested without a
device**: a hand-rolled base64 decoder and credential compare is the one piece
of this work where "untested" means "ships a machine that opens or locks itself",
and it is `no_std`, allocation-free and covered by `just test` on the host. The
wiring is pinned by `cc-hal-esp32::web::tests::auth_*` (device-only).

## 24. `/api/status` reports `steamMode`, and keeps `brewing` as an addition 🔴 changed {#d24}

**What the C++ does.** `doc["steamMode"] = systemContext_->steamMode()`
(`WebServerManager.cpp:359`).

**What that actually is.** `SystemContext::steamMode()` (`SystemContext.cpp:395`)
delegates to `MachineStateContext::isSteamModeActive()`
(`MachineStateContext.h:395`), which returns `steamON_` (`:785`). That flag is
**latched**: set `true` in `SteamRunningState::onEntryImpl`
(`SteamStates.cpp:16`), cleared in `SteamRunningState::onExitImpl` (`:21`), and
cleared again in `StandbyState::onEntryImpl` (`SystemStates.cpp:17`). It means
"steam mode is engaged", and it is not derived from the current state at read
time.

**What the Rust does.** `cc-machine`'s `Machine::steam_mode` is set at exactly
those three points (`states.rs:218`, `:263`, `:365`), so `Telemetry::steam_mode`
**is** the C++'s `steamMode()`. `/api/status` now emits it under the C++'s name.

**What was wrong.** The route emitted `"brewing": t.brewing`, where `brewing` is
`state.is_brew_state() && state != BrewFinished` (`main.rs:2983`) — a derived
brew-state flag with no C++ counterpart, published **under the C++'s steam-mode
name**. A client asking "is steam mode on?" got an answer about brewing. That was
an *undeclared* divergence and a wrong value, not merely a renamed field.

**Why both keys, rather than a rename.** `brewing` is real information the C++
never had, nothing consumed it (`rg` finds no reader of `/api/status` in the UI —
the UI reads `steamMode` from the `/api/steam` **toggle response**,
`machine-toggle-result.ts:9`, which is untouched), and dropping a published field
buys nothing. So `steamMode` carries the C++'s value and `brewing` stays as an
**addition**. The UI's toggle and a status poll now agree, which is the property
that matters: a switch that sets steam mode and a poll that then contradicts it
is the same class of bug this file already records once, for a toggle reading its
value before the command was applied.

**What pins it.**
`cc-hal-esp32::web::tests::steam_mode_is_the_latched_steam_flag_and_not_the_brew_state`
and `::the_status_steam_mode_agrees_with_the_steam_toggle_response` (device-only).

## 25. CORS preflight is answered; the C++'s per-response `Access-Control-Allow-Origin: *` is not 🔴 changed {#d25}

**What the C++ does.** `AsyncCorsMiddleware` with `setOrigin("*")`,
`setMethods("GET,POST,PUT,DELETE,OPTIONS")` and
`setHeaders("Content-Type,Authorization,X-Requested-With")`
(`WebServerManager.cpp:272-277`), applied to **every** response by middleware.

**What the Rust does.** `OPTIONS /api*` answers `204` with those three headers.
The per-response header is **not** ported.

**Why.** Two reasons, and the second is the one that matters.

1. The advertisement was a phantom: `routes()` listed
   `("/api/status", Method::Options)` and **no `fn_handler` anywhere** registered
   it, so a preflight 404'd while the boot log claimed the route existed. That is
   now a real handler on a real wildcard. ESP-IDF matches URI and method
   independently (`httpd_uri.c:97-122` — a URI match with the wrong method sets 405
   and the search *continues*), so one `/api*` entry answers every API preflight
   without touching any `GET`/`POST` routing.
2. A wildcard origin on an endpoint that can reboot a boiler, change its
   emergency cut-off or erase its configuration is a widening with **no
   consumer**: the SPA is served same-origin from `/ui` and needs nothing. Adding
   `Access-Control-Allow-Origin: *` to authenticated responses would also be
   incoherent next to `WWW-Authenticate` — a browser will not attach a cached
   Basic credential to a wildcard-origin response — so the honest CORS surface
   here is "this server answers preflight and does not opt into cross-origin
   reads".

`OPTIONS` is registered **outside** the authentication gate, deliberately and
visibly: a preflight is by definition the one request a browser sends without
credentials, so challenging one makes every cross-origin request fail
permanently and confusingly.

**What pins it.** `cc-hal-esp32::web::tests::the_advertised_options_handler_is_a_wildcard_over_the_api`
(device-only), which fails if the advertised `Options` entry is anything other
than the one real wildcard.

## 26. `POST /api/setpoint` takes the schema's bound, and the brew pair is cross-checked 🔴 changed {#d26}

**What the C++ does.** `WebServerManager.cpp:391-402` filters the field to
`0.0..=150.0`, applies `setProcessSetpoint` to the **running** machine, and then
calls `Config::brewSetpoint.set(newSetpoint)`, which range-checks
`20..=110` (`Config.h:795-802`) and returns `false`. So the C++ rejects the
persisted value, answers `400`, and — because `setProcessSetpoint` already ran —
leaves the rejected value as the live process setpoint for that boot.

**What the Rust does.** Two changes.

1. The handler parses the field through `cc_config::assign::parse("brew.setpoint", …)`
   (`cc-hal-esp32::web::parse_setpoint`), so the accepted range is the schema's
   `20.0..=110.0` and cannot drift from `ParamSpec` the way a repeated literal does.
   Out-of-range values are refused with `400`, matching what the C++ already
   answers for a value it will not take and what `/api/parameters` answers for the
   same key.
2. `cc_safety::validate_config` now also requires `safety.emergency_temp` to exceed
   `brew.setpoint + brew.temp_offset + safety.emergency_hysteresis`, not just the
   steam pair it already checked. This is the *effective* setpoint the PID is
   actually told to hold, which is why the offset is carried in `SafetyConfig`.

**Why.** The port had no second range-check on this route. `persist_setpoint`
wrote whatever it was handed to the one store slot the machine reads at every
boot, so `?value=150` was accepted, persisted, and reloaded — and 150 °C defeats
the interlock, because `safety.emergency_temp` defaults to 150 and S1's
over-temperature test is **strictly greater**. The machine was driven *to* the
emergency threshold and held there with a debounce that could never count a
breach. That is the same shape as the steam rule already in this ledger (09 §8),
extended to the setpoint that is actually used.

The rule lives in `validate_config` rather than at the write path so that **every**
writer is covered — HTTP, MQTT and `/api/parameters` — rather than only the one
where the bug was found. It is the fail-closed rule of
[08 §4.1](./recovered-oracle.md): a configuration that cannot run safely is
discarded at load, and `SafetyConfig` reaches the validator through the same
`safety_view` the boot path already uses.

**What it costs, deliberately.** `?value=5` is a `202` on the C++ and a `400`
here, and `?value=150` is a `200` on the C++ that leaves a 150 °C process setpoint
in RAM. Narrowing the accepted range is a divergence. A silent clamp was rejected
instead: the caller would be told `accepted: true` for a request it did not make,
so a UI slider stuck at 110 looks like a stuck UI rather than a refused write. A
fractional setpoint still truncates rather than being refused, because
`setProcessSetpoint` takes a `double` in the C++ and truncation is not a safety
question.

**What pins it.** `cc-safety::tests::safety_paths.rs::config_*` (nine host tests,
covering the boundary, the ordering against the steam rule, and the load-time
discard), plus
`cc-hal-esp32::web::tests::the_setpoint_route_{takes_what_the_schema_will_store,refuses_a_value_that_would_defeat_the_interlock}`
(device-only, registered in `CASES`).

## 27. `brew.by_weight` stops the shot, and is refused where nothing can 🔴 changed {#d27}

**What the C++ does.** `BrewRunningState::checkSpecificTransitions`
(`BrewStates.cpp:266-303`) ends an automatic shot on `brew.by_time` (whose
target comes from `initTotalTargetBrewTime`, `:53-62`, and is **zero** unless
`brew.by_time.enabled`) or on `brew.by_weight`, which compares
`getCurrentBrewWeight()` against `brew.by_weight.target_weight`. With `by_time`
off there is one stop condition and it needs a scale — and the C++ has no scale:
`HardwareContext::setScale` has no caller anywhere in `src/` or `include/`, so
`getBrewWeight()` returns 0 for the life of the process (09 §23). The by-weight
arm is therefore dead in the C++ **unconditionally**, and an operator who turned
it on got a shot that ran until the brew switch or the 300-second pump watchdog.

**What the Rust does.** Two changes, because there were two defects.

1. **The weight reaches the field that acts on it.** `cc-hal-esp32::Sampler`
   measures a real weight and `weight_g` was already published to `/api/status`
   and MQTT from it, but `Sensors::brew_weight` — the same number, read a few
   lines earlier in the same tick — was written as the literal `0.0`. The
   reading was published and the state machine was not given it, so the machine
   measured a shot correctly and refused to stop on it. The by-weight stop
   condition and the weight arm of `recordBrewIfQualified` (`BrewStates.cpp:311`)
   were both dead for the same reason, and `has_scale_error` could not cover it
   either: a machine with no scale has no scale error.
2. **`cc_safety::validate_config` refuses the combination that has no stop
   condition at all** — `ConfigViolation::BrewByWeightWithNoScale`: automatic
   mode, `brew.by_weight.enabled`, `brew.by_time` disabled, no scale. Without
   this, a scale-equipped machine whose scale later stops answering is back to a
   300-second shot, and no range check anywhere can catch it: all four
   parameters are individually legal (`brew.mode` is a two-valued enum, the two
   `enabled` flags are independent booleans both defaulting to `false`,
   `target_weight` is bounded `0 ..= 500`), so the rule is a conjunction read
   across four of them.

**Why the second change is in `validate_config` and not at the write path.** The
same argument as §23: it has to cover HTTP, MQTT and `/api/parameters` alike,
and the fail-closed rule of [08 §4.1](./recovered-oracle.md) is what makes a
configuration written by an older firmware — or by a machine that *had* a scale
— safe on the next boot. `hardware.sensors.scale.enabled` is carried in
`SafetyConfig::scale_fitted` because it is **the C++'s own definition of a
fitted scale** (the guard on every scale command, `WebServerManager.cpp:540,563`,
and the third argument of `recordBrewIfQualified`, `BrewStates.cpp:311`), and
because a pure validator cannot ask a driver whether it has answered yet.

**Why the weight is read where the temperature is, not where the events are
drained.** `drain_scale` returned it, and `drain_scale` runs at step 7b —
*after* `control.tick`. That is the right place for the NVS commit inside it (an
erase-and-write in milliseconds, which must not sit between the reducer's
decision and the pins) and the wrong place for a number the reducer is about to
decide on. A 10 Hz sample is a snapshot, not a message — losing one loses
nothing — so the weight is now `scale_weight`, read at step 4 beside the
temperature and the pressure, and only the *events* (a completed tare, a new
calibration factor, where dropping one loses an operator's action) stay on the
queue.

**What it costs, deliberately.** A machine with no scale cannot be configured to
brew by weight alone, and the write is refused with the violation logged rather
than accepted and then silently ignored — the same choice §23 makes for a
setpoint that would defeat the interlock. `MANUAL_BREW` is untouched: the
operator ends the shot with the brew switch, which is a stop condition no
configuration removes. And an automatic brew with `by_time` on is untouched,
because that is a stop condition that needs no scale at all.

**What pins it.** `cc-safety::tests::safety_paths.rs` — nine host tests, named
`config_an_automatic_brew_that_can_only_stop_on_a_weight_needs_a_scale` and
following, covering the refusal, the same configuration with a scale, each of the
three ways out (`by_time`, `MANUAL_BREW`, `by_weight` off), the ordering against
the relay rules, `check_storable`, and the load-time discard.
`cc-parity::run::tests::the_safety_view_mapping_is_the_one_the_driver_uses`
carries the four new fields so a rename in `cc-config` cannot silently drop them.

## 28. `pid.regular.i_max = 0` disables integral action 🔴 changed {#d28}

**What the C++ does.** `ProcessController::setPIDTunings`
(`ProcessController.cpp:203-218`) computes `Ki = Kp / Tn` — with no reference to
`aggIMax_` — and then calls `setPidIntegratorLimits(0, aggIMax_)` (`:211`), which
forwards to `PID_v1::SetIntegratorLimits(0, iMax)`. That method **refuses** a
window whose `min >= max` and returns without touching anything
(`PID_v1.cpp:220-231`). So `pid.regular.i_max = 0` — a value both firmwares
accept, `PID_I_MAX_REGULAR_MIN` is `0.0` (`defaults.h:71`) — left the controller
on `PID_v1`'s own `-100 ..= +100` limits (`PID_v1.cpp:35`), while the operator
had asked for no integral action. The C++ discards the rejection silently.

**What the Rust does.** `Config::pid_tunings` reads an `i_max` of 0 as
`Ki = 0`, which is this codebase's existing expression of "no integral action":
it is what a `Tn` of 0 already produced, and `Controller::set_tunings` pins the
accumulator to zero for it (`PID_v1.cpp:167-169`). Both firmware call sites skip
the now-meaningless `SetIntegratorLimits` rather than make the call and discard
its `bool`.

**Why not raise the schema's lower bound instead.** It cannot be raised. Home
Assistant's `aggIMax` number entity publishes this bound — the C++ at
`MQTTManager.cpp:869` (`PID_I_MAX_REGULAR_MIN`), the port through
`cc_config::discovery::bounds` — so a floor above zero would diverge from the C++
*and* from this firmware's own verified MQTT discovery surface. A silent `Ki` of
1.19 with a configured ceiling of zero is the worse answer in a machine whose
boiler is the thing being regulated.

**What pins it.**
`cc-config/tests/config_schema.rs::an_integrator_ceiling_of_zero_is_no_integral_action_not_the_library_default`,
next to `a_zero_tn_gives_a_zero_ki_rather_than_a_division_by_zero`, which is the
rule this one joins.

## 29. The water valve's interlock consults S5's whitelist too 🔴 added {#d29}

**What the C++ does.** `HardwareManager::openWaterValve`
(`HardwareManager.cpp:404-419`) checks only `emergencyMode_`, exactly as
`openSteamValve` does — and S5's whitelist lives somewhere else entirely, in
`BrewHandler::valveSafetyShutdownCheck` (`BrewHandler.h:105-122`), which runs
every loop and closes the valve unless the state is on the list. The C++ is
therefore correct **only** because that function runs after the state machine
and the closing effect lands after the opening one.

**What the Rust does.** `cc_hal_esp32::Interlock::may_open_water` now consults
`cc_safety::water_flow_allowed`, so the actuator facade refuses an
`OpenWaterValve` in a non-whitelisted state exactly as
`may_open_steam` already refused an `OpenSteamValve` in a non-steam state
(§2). The tick's trailing `CloseWaterValve` is unchanged, so the observable
behaviour is identical; what changes is that the second line of defence exists
rather than being an argument about effect ordering.

**Why it matters here specifically.** Steam and water share one relay
(`ValveState.h:8-11`, GPIO17), and the MQTT inbound-command path applies its own
effects *after* the tick's whitelist tail. That path is safe today only because
no `Command` emits `OpenWaterValve` — a fact about one function
(`handlers::apply_command`) that nothing enforces and that a future command
would break silently.

**And the one thing this exposed.** The facade caches the machine state for the
interlocks, and the shell told it *before* the tick — so the cache held the
state the tick was leaving, not the one it entered. A state-gated
`may_open_water` would therefore have refused the `OpenWaterValve` that
`BrewPreinfusionState::onEntryImpl` emits on the very tick the machine enters
`BREW_PREINFUSION`, opening the valve one tick late and logging a refusal at
every brew start. `Actuators::set_state` now runs after `Control::tick` and
before the `apply` it belongs to, which is what the MQTT path at
`main.rs` already had to do for the same reason.

**What pins it.**
`cc-hal-esp32::actuators::tests::the_water_valve_is_whitelist_gated_to_the_water_flow_states`
(device-only, registered in `CASES`), which asserts
`may_open_water() == cc_safety::water_flow_allowed(state)` for every state and
mirrors the existing steam test. `a_healthy_interlock_permits_the_pump_the_valves_and_the_heater`
was corrected with it: `PID_NORMAL` is not a water state either.

---

## 30. The steam LED is not driven: GPIO1 is the provisioning console's 🔴 known deviation {#d30}

| | |
| --- | --- |
| **Finding** | [32-findings §3.1](./review-2026-10-03.md) — "3 status LEDs absent" |
| **Severity in the C++** | Cosmetic, and the C++ is the broken one. See below. |
| **Test** | `cc-display/src/leds.rs` — all 11 tests, host. The steam LED's *rule* is tested even though its pin is not. |

### What the C++ does

`LoopManager::updateLEDs` (`src/core/LoopManager.cpp:255-286`) drives three
LEDs, and `HardwareManager::initializeLEDs` (`:95-125`) constructs them on three
pins from `include/clevercoffee/hardware/pinmapping.h:43-45`:

```c
#define PIN_STATUSLED 26 // 25 works with logging // Moved from pin 26 (pin 26 had hardware issues)
#define PIN_BREWLED   19 // Working correctly
#define PIN_STEAMLED  1  // 32 works with logging // Moved from pin 1 (UART TX - conflicts with serial logging)
```

**The third line contradicts itself, and that is the most important thing in this
entry.** Its comment says the steam LED was *moved off* GPIO1 because GPIO1 is
UART TX and conflicts with serial logging. The `#define` on that same line is
still **1**. The migration was written down and never made.

So the C++ constructs a `StandardLED` on `GPIOPin(1, OUT)`
(`HardwareManager.cpp:118`) while `Serial.begin()` has UART0 routed to GPIO1.
Two owners of one wire; whichever attaches last wins, and the C++ has no way to
express the conflict — `StandardLED` writes the pin as a GPIO and the UART driver
writes it as a peripheral function.

### What the Rust does

Two of the three LEDs, on the C++'s pins, with the C++'s rules:

- `PIN_STATUSLED` (GPIO26) and `PIN_BREWLED` (GPIO19) are wired, driven, and
  logged at boot.
- `PIN_STEAMLED` is **not**. There is deliberately **no `STEAM_LED` constant**
  in `cc_hal_esp32::pins` — adding one would be claiming a pin this firmware
  gives to something else.

`LedOutput.steam` is still computed, and still tested, by
`cc_display::leds::LedOutput::from_state`: the *rule* is implemented, only the
pin is absent. One `warn!` at boot in `cc-firmware/src/main.rs` fires if an
operator has `hardware.leds.steam.enabled` set, because a setting this firmware
cannot honour is worth a line in the log rather than silence.

### Why this and not literal parity

Parity here would mean parity with a C++ bug, bought with the machine's
recovery path. The facts, all of them checkable:

1. **A pin has exactly one owner.** `Peripherals::take()` hands out each pin
   field once. The only consumer of `peripherals.pins.gpio1` is the UART
   provisioning console (`cc-firmware/src/main.rs:1027`). A second `PinDriver`
   needs `AnyIOPin::steal`, which is `unsafe`, and `unsafe_code` is `deny`
   workspace-wide (`Cargo.toml:132`).
2. **That console is the documented recovery path.** `wifi set` + `wifi apply`
   run on it, and it is the only way to point a machine joined to a nonexistent
   network at a real one. `cc-firmware/src/main.rs:1000-1024` records that
   finding from the bench: a machine configured for a network that did not exist
   could not be fixed any other way.
3. **The C++'s own comment says to move it.** The author identified the
   conflict, named the fix, and did not apply it.

Trading a recovery path for an LED is the wrong end of the trade, and it is the
opposite of what `pinmapping.h:45` intended. The C++ did not have a working
steam LED to be parity with; it had a race between two peripherals on one pin.

### What a move to GPIO 32 would actually cost

`pinmapping.h:45` names GPIO 32 as the alternative ("32 works with logging"), so
it is the obvious candidate. **GPIO 32 is `SCALE_DATA_1`** — the HX711 scale's
first data line, `PIN_HXDAT` at `pinmapping.h:29`. Every reference:

- `crates/cc-hal-esp32/src/pins.rs` — the `SCALE_DATA_1` constant, and the
  uniqueness entry in `ALL` that would reject a second claim on 32 at compile
  time.
- `crates/cc-hal-esp32/src/pins.rs::assert_wiring` — `("SCALE_DATA_1", …,
  Pin::pin(&pins.gpio32))`, which checks the map against the real wiring at boot.
- `crates/cc-hal-esp32/src/scale.rs:74` — `use crate::pins::{SCALE_CLOCK,
  SCALE_DATA_1, SCALE_DATA_2}`, and the `GpioHx711(data = SCALE_DATA_1, …)`
  driver built at `:173`.
- `crates/cc-firmware/src/main.rs` — wired at `data_1: peripherals.pins.gpio32`
  in the scale bring-up, and named in the `pins:` boot-log line.
- `crates/cc-device-tests/src/main.rs:208` — `lend_test_pins(peripherals.pins
  .gpio32, …)`.
- `crates/cc-hal-esp32/src/scale.rs` — a test asserting `SCALE_DATA_1 == 32`, so
  the number is pinned by a test as well as by the map.
- `crates/cc-safety/src/lib.rs` and `cc-firmware/src/main.rs` — GPIO 32 is named
  in the reasoning about whether a scale is fitted at all, alongside 25 and 33.

**So this is a hardware change, not a firmware change.** With the machine in
hand, a person would have to open the case, trace the HX711 module's first data
wire off the ESP32's GPIO32 header pin, and re-terminate it on another pin.

**Is there a free pin to move it to? Yes — two, and this is the part worth being
precise about.** Of the 28 GPIOs this chip exposes (the set `is_gpio` in
`cc_hal_esp32::pins` admits), 18 are claimed by `ALL`. The ten unclaimed are
**0, 4, 5, 12, 13, 14, 15, 18, 37, 38**, and they divide:

- **0, 5, 12, 15** — strapping pins (ESP32 Series Datasheet v5.3, Table 3-1;
  `feature-inventory.md:120-122`). Their reset state is a boot decision, not
  the firmware's, so they cannot carry a clocked data line.
- **37, 38** — input-only, no output driver. `pins::is_input_only` exists for
  exactly this bank (34-39). Unusable for the HX711.
- **4, 5, 18** — claimed by the **C++**: `PIN_ROTARY_DT`, `PIN_ROTARY_SW` and
  `PIN_ZC` (`pinmapping.h:22,24,48`). Unwired in the Rust port — the rotary
  encoder and the zero-crossing dimmer were never ported — but the hardware
  exists on the board, so taking one forecloses a peripheral the C++ has.
- **13, 14** — **genuinely free**: absent from `pinmapping.h`, absent from the
  Rust tree, neither strapping nor input-only.

**So GPIO 32 was never actually available either, but not for the reason one
might assume.** The blocker was never that the chip ran out of pins. It was that
the one line `pinmapping.h:45` names was already doing something else, and the
alternative — relocating the scale — is a solder joint rather than a
configuration change. A person with the machine in hand **can** do this, and
13/14 is where the wire goes.

**Recommendation: do not.** The cost is a case opened and a scale's data wire
re-terminated, against an LED whose information the panel already shows: in
`STEAM_RUNNING`, `LedOutput` lights `status` *and* `steam` together, so the
status LED's widened 5 °C tolerance is what an operator actually reads while
steaming. Nothing is lost that the machine cannot already say out loud. If the
steam LED is ever judged worth a solder joint, this note is the recipe.

### What pins it

`cc_display::leds::LedOutput::from_state`, all host tests:
`only_the_steam_state_lights_the_steam_led` iterates **all eighteen**
`MachineState` variants, so the steam rule cannot silently widen; and
`the_three_leds_are_independent_of_each_others_conditions` proves each LED's
gate is its own. The absence of the pin is checked the only way it can be —
`cc_hal_esp32::pins` has no `STEAM_LED` to be wrong about, and `assert_valid` is
a `const fn`, so a future edit that adds one back **fails the build** with the
duplicate-pin assertion naming it.

## 31 — OTA is implemented, and is stricter than the C++ in four ways {#d31}

R3-15, finding 3.3 of [`review-2026-10-03.md`](./review-2026-10-03.md).
Requirement **S8** of
[`feature-inventory.md`](./feature-inventory.md#6-safety-critical-control-paths).

`/api/ota/firmware` and `/api/ota/filesystem` write to flash. `/api/ota/url`
answers `501` and says why. `/api/ota/status` reports a real session.

### The three differences, and why each is safer

**1. An OTA is refused while water or steam is flowing.** `otaPrepareHardware`
(`src/core/SystemInitializer.cpp:57-63`) is `disableTimer1()` plus
`disableHeater()` — the pump and the 3-way valve are **not** touched, and no
state is ever refused. `cc_machine::ota::admit` refuses the six water-flowing
states and `SteamRunning`, so a machine flashed mid-shot is not a machine this
firmware will flash.

It is deliberately **not** `cc_safety::water_flow_allowed`. That answers *"may
this state open the valve?"* and returns **false** for `SteamRunning`, which is
the one state where a valve on this machine is open. Asking the whitelist would
have answered the wrong question; `steam_is_refused_even_though_the_water_
whitelist_allows_it` asserts both answers so the confusion cannot return.

**2. The full safe hardware shutdown, not a heater disable.**
`cc_machine::ota::begin_session` emits `Effect::SafeHardwareShutdown`, which
reaches `Actuators::safe_hardware_shutdown` (`actuators.rs:775`): pump off,
valve closed, heater duty zero. This is the effect finding 3.3 records as
unused by OTA, and it is what 04 §4's shutdown table names for "OTA start".

It is **not** `Effect::EmergencyShutdown`. That latches, and every later
`enable_*` would be refused until something cleared it — so a machine whose OTA
failed halfway would come back permanently dead with no way out over the network.
A failed update must leave a machine that still runs;
`a_session_is_not_latched` is the test of that decision.

**3. The watchdog stays armed.** The C++ suspends it for the whole OTA
(`ota.cpp:99-110` → `g_watchdog->suspend()`), because its flash write runs on
the same loop that feeds the watchdog. Here the write is on the **httpd** task
and the watchdog is subscribed to the **control** task (04 §2), so it keeps
feeding while the flash erases and a genuinely wedged flash still resets the
chip. Removing the fail-safe for the exact window one most wants one is a
worse trade than the C++'s.

**4. The probe is not polled, and the shutdown is re-applied, while the session is busy.** The C++ erases flash on the loop that reads the DS18B20, so the probe does not run. Here the write is on the httpd task and the control task keeps ticking. That is what put the machine in `SENSOR_ERROR` (runbook §13.4). From admission until the session ends, the probe stays on its last reading and `SafeHardwareShutdown` is applied again each tick, including after an MQTT effect. A failed upload clears the hold. Measured 2026-10-08: [runbook §13.4](../operations/runbook.md).

### Where S8 is enforced

`ota_upload_route` in `cc-hal-esp32/src/web.rs` runs eight checks in a fixed
order, and the order is the argument: admission, claim, `Command::OtaBegin`,
boundary, open the slot, stream, validate, finalise. Step 3 is the safety hook —
it goes through the command queue, so the **control task** applies the shutdown
through the real applier on its next tick, before step 5 erases anything.

The window between the request and that tick is up to one 10 ms control period
in which the machine still runs normally. It is not a hole: `admit` has already
established that nothing is flowing, and the control task re-applies the shutdown
each tick while the session is busy, which is what keeps the heater off.
Waiting for an ack would buy nothing and would
put a 10 ms stall on the httpd task for every upload.

### The memory strategy, and why it cannot OOM

A firmware image is 1,675,952 B. The heap is ~320 KB. The pipeline is

```text
socket -> [ 4 KiB stack buffer ] -> cc_web::ota::PartReader -> esp_ota_write
```

and the **only** per-upload buffer is a `[u8; 4096]` on the task stack.
Nothing in the loop allocates; `PartReader`'s hold-back is `boundary.len() + 4`
bytes in a `Vec` whose capacity is reached on the first push. So the heap cost
of a 1.6 MB upload equals that of a 4 KB one. This is the **inbound** half of
[ADR-0002](../adr/0002-wifi-logging-ota-memory-architecture.md) decision 2,
which fixed the outbound half; without it, one 1.6 MB `String` would undo that
work.

Three parser bugs the host tests found, all of which would have shipped:

- the closing delimiter is `\r\n--BOUNDARY`, not `--BOUNDARY`. RFC 2046 puts a
  CRLF before every delimiter but the first, and that CRLF belongs to the
  envelope — searching for the bare form splices two bytes of MIME framing into
  the middle of an image.
- the Headers arm cleared its carry, so a `\r\n\r\n` split across two reads was
  never found and an ordinary upload died of `HeadersTooLong`. **At small chunk
  sizes only**, which is the worst shape of bug to reach hardware.
- `const MIN_ACCEPTED_BYTES: usize = match self` does not compile as written;
  the shape that does is one value for both variants, which silently applied the
  512 KiB firmware floor to a 384 KiB filesystem partition.

### The power-cut story, and how far it was verified

**Verified by reading ESP-IDF v5.5.5, and corrected on hardware 2026-10-07.**

- **This section originally said** that `esp_ota_end` validates the image and
  *only then* calls `esp_ota_set_boot_partition`. That was wrong, and the
  measurement is what disproved it: `esp_ota_end` is `ota_verify_partition` and
  cleanup (`esp_ota_ops.c:477-524`) and **never** touches `otadata`. The slot is
  selected by a separate call — which this firmware did not make until
  [`cc-hal-esp32/src/ota.rs`](../../crates/cc-hal-esp32/src/ota.rs) started
  calling `esp_ota_set_boot_partition` after a successful `esp_ota_end`.
- Therefore a power cut **before** that call leaves `otadata` pointing at the slot
  the machine booted from, and it boots that slot again. Measured 2026-10-07: an
  upload answered `200`, the device rebooted, and the bootloader logged
  `Loaded app from partition at offset 0x10000` — the slot it came from.
- A power cut **during** the `otadata` write leaves a CRC-invalid sector. **The
  bootloader's fallback to the factory app was NOT verified** — it depends on the
  bootloader binary flashed alongside this firmware, which was not inspected.
- A power cut **after `esp_ota_set_boot_partition`** means the new image is
  selected and is a complete, validated image. A cut in the window between
  `esp_ota_end` and that call does **not** select it: `otadata` is untouched and
  the machine boots what it booted from, which is the safe direction.

**There is no rollback.** `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` is absent from
this build's `sdkconfig` (checked: no `BOOTLOADER_APP_ROLLBACK` line at all), so
`esp_ota_mark_app_valid_cancel_rollback` is a no-op and a new image that boots
and then misbehaves **stays selected**. This is a stated limitation, and the C++
behaves identically — it never enables rollback either. Enabling it is a
bootloader rebuild and is not in scope for R3-15.

### What was not built

`/api/ota/url` answers `501`. The C++ implements it (`ota.cpp:704-724`) by
queueing the download for its main loop, with the reason stated in a comment:
*"Running it here would stall the AsyncTCP task and the response would never
reach the client"*. Doing it here needs an HTTP client, a second long-lived
task, and a restart-on-failure path — for a feature every part of which a browser
upload already reaches. The route is registered and says so, rather than 404ing,
because the UI has a live tab that calls it.

Also not built, and not in the C++ either: resume, delta updates, rollback to a
previous version, a progress websocket, and scheduling.

### What pins it

- `cc_machine::ota` — 10 host tests: every state classified, the six
  water-flowing states refused by name, steam refused *despite* the water
  whitelist allowing it, `BrewFinished` admits while `BrewRunning` does not, the
  session emits exactly `SafeHardwareShutdown` and not the latching variant.
- `cc_web::ota` — 28 host tests: byte-exact recovery at **eleven** chunk sizes
  from 1 B to 4 KiB, near-miss delimiters, truncation refused rather than
  finalised, oversized headers a hard error, every status message non-empty, and
  every status document validated against the UI's `OtaStatusSchema` enum. Plus
  `progress_is_scaled_against_each_kinds_own_floor`, which pins `ota.cpp:234`:
  the same 256 KiB reads 45 % as firmware and 90 % as a filesystem image.
- `cc_hal_esp32::ota` — 6 device tests: one claim at a time, exactly one restart
  request, a failure asks for no restart, and the claimed `Kind` reaching the
  progress bar (the half only this crate can see — the arithmetic itself is
  host-tested in `cc_web::ota`).

### The progress floor is per **kind**, and it is now in the portable half

The first cut of this port scaled *both* kinds against the firmware floor,
because the floor was chosen by switching on the update's `Phase` rather than on
its `Kind`. The C++ chooses it from `isFilesystem` (`ota.cpp:234`):

```cpp
const size_t  minSize  = isFilesystem ? (256 * 1024) : (512 * 1024);
```

A filesystem image therefore stalled at ~50 % on a successful upload. Two changes
close it, and they are one change because the second is what the first needed:
`Session::kind` — written by `claim`, read by nothing in the workspace — is the
session's answer to `isFilesystem`, so `note_progress` now reads it, and the
arithmetic itself moved to `cc_web::ota::progress_percent(kind, uploaded)` where
`just test` reaches it. It was in `cc-hal-esp32` before, which meant a piece of
pure arithmetic could only ever be run on a chip.

### A premise worth correcting: what actually protects the OTA routes

It is tempting to read `system.ota_password` (`Config.h:1336`) as "the OTA routes
are password-protected, so Basic auth on them is parity". It is not. Its **only**
reader in the C++ is

```cpp
OTA::initializeArduinoOta(Config::getInstance().systemHostname.get().c_str(),
                          Config::getInstance().systemOtaPassword.get().c_str());
```

(`src/core/SystemInitializer.cpp:471-474`), which is `ArduinoOTA.setPassword` —
the **espota** protocol on port 3232, not HTTP. The three HTTP endpoints are
protected by `system.auth_enabled` / `system.auth_username` /
`system.auth_password` through `AsyncAuthenticationMiddleware`
(`WebServerManager.cpp:280-294`), which covers every route including
`/events` and the not-found handler.

So this port's situation is: the HTTP OTA routes are behind `Auth` because
**every** route here is (`cc_hal_esp32::web::register`), which is the parity
behaviour and needs no OTA-specific decision; and `system.ota_password` remains a
schema key with no implementation, precisely because espota is not built. When
espota is added, that parameter becomes live and this paragraph stops being
true.

---

## 32 — A first boot on a C++-flashed machine says so, once 🔴 added {#d32}

Finding 3.6 of [`review-2026-10-03.md`](./review-2026-10-03.md).

### What the C++ does

Nothing, because it cannot. The C++ reads the namespace it writes — `config`,
`include/clevercoffee/defaults.h:13` — so "the settings I saved are the settings
I am running" has never been a thing that could go wrong on a C++ machine. There
is no predecessor firmware in its history.

### What the Rust does

The two firmwares use **different NVS namespaces**, so a machine that ran the C++
has not lost its configuration — the Rust firmware has never opened the
namespace it is in:

| | namespace | key | written by |
| --- | --- | --- | --- |
| C++ | `config` (`defaults.h:13`) | `p` + 8 FNV-1a hex digits | `Config.cpp:154-171` |
| Rust | `cc` (`blob_store::NAMESPACE`) | `cc.config`, one JSON blob | `BlobConfigStore::save` |

One key per parameter in the C++, one blob here: the key-shape difference is
`Config.h:318-332` and is what a migration would have to reproduce.

So on a machine flashed from the C++, `store.load()` returns `Ok(None)`, the
firmware writes the compiled-in defaults, and those carry no SSID. The operator
gets a machine that is on a network it is no longer configured for, and — until
now — only a boot log saying "nothing stored" to explain it.

At boot, **when the Rust store is empty**, the firmware now opens `config`
**read-only**, takes the first key name, and drops the handle. If there was one,
it prints this and nothing else:

```text
config: the previous firmware's settings are in NVS namespace "config" and this
firmware uses "cc": not read, not deleted, still on the chip. Re-enter the Wi-Fi
SSID and password. Expected on a first flash.
```

(One physical line; wrapped here to fit this page.)

It also says the two firmwares use different storage namespaces and that this is
expected on a first flash — because an operator reading "your settings are not
read" without that is entitled to suspect a bug. The re-provisioning path is
named rather than described: `wifi set <ssid>` on the UART provisioning console,
which is the same path §27 keeps GPIO1 wired for.

### Why this and not a migration

`cc_config::blob_store`'s module documentation carries the full reasoning, decided
2026-09-28. The short form: a migration means shipping the C++'s FNV-1a key hash,
its 98 key names and its per-parameter type table, and writing the result into
the **one** configuration slot a machine has, with no rollback. On a device that
heats a boiler, that is a bad trade for values that are almost all within a
whisker of their defaults — and the only parameters whose loss is actually felt
are the Wi-Fi credentials, re-entered in seconds over a UART the firmware already
has. **Detection plus one sentence is the whole of this.**

### What was deliberately not built

- **No migration.** Above. Nothing reads a C++ key or writes a C++ namespace.
- **No prompt, no second boot, no modal.** One `warn!` line at boot, once. The
  same information is on the console, in the telnet ring and in the UART log,
  which is where an operator with a machine on the wrong network will look.
- **No key counting.** One key is enough to answer "was anything ever written
  here", and reading the names would be the first half of a migration.
- **No `/api/nvs-debug` field.** The endpoint reports on *this* firmware's
  namespace; a boot log is where a machine that cannot associate has to be
  explained, and adding an endpoint field would be a second spelling of the same
  sentence.
- **No change when the Rust store is populated.** A machine that already has this
  firmware's settings is told nothing, and the namespace is not walked at all —
  `PredecessorProbe::Skipped`.

### What pins it

- `cc_config::predecessor` — 4 host tests: the two namespaces are asserted to be
  different key spaces; the four `PredecessorProbe` cases map to the four
  outcomes, with the two quiet ones silent; the found message names both
  namespaces, says "not deleted" and names the action; the unreadable message
  does not also claim a finding.
- `cc_web::telnet::tests::the_predecessor_boot_lines_are_not_truncated_on_the_wire`
  — the sentence, the namespace and the **real** formatter meet, on the widest
  `u32` uptime and this module's own log target. A hand-copied byte budget
  inside `cc-config` would have been invalidated silently by a change to
  `ENTRY_BYTES` or a rename of the target; this cannot.
- `cc_hal_esp32::nvs::probe_predecessor` — device-only, and cannot fail: a
  missing namespace is `Absent` (`ESP_ERR_NVS_NOT_FOUND` from a read-only open)
  and every other error is `Unreadable`. It reads one key name and writes
  nothing.

---

## 33 — Three defects the C++ does not have, found on a bench ESP32 🔴 changed {#d33}

All three were found by running
[`integration-checklist.md`](../operations/runbook.md) against a
bench board on 2026-10-05, and all three are places where this firmware was
**less faithful** than the oracle rather than more. Each was verified against
the C++ source before being changed, so none of them is a design decision.

### 30a. A fractional setpoint was truncated to an integer

The C++ hands the request's `double` straight to `setProcessSetpoint` and to
`Config::brewSetpoint` (`WebServerManager.cpp:391-404`), and `brew.setpoint` is
a **float** parameter. `cc_web::request::parse_setpoint` cast it to `i32`, so
`?value=93.5` was accepted with `202 {"accepted":true}` and arrived as 93.
Measured: `93.5`, `80.5` and `91.2` were all accepted and all landed on the
truncated integer. `Command::SetSetpoint` now carries an `f64`, which takes the
enum from 8 to 16 bytes; `set_tunings` writes gains without touching the
integrator, so the size costs nothing at runtime.

### 30b. Backflush mode could not be turned off

`POST /api/backflush` toggled by feeding `Command::BackflushEnter` /
`Command::BackflushStop`. `BackflushStop` stops a running cycle and leaves
`backflush.on` set, so **the mode had no off switch**: four presses, including
the explicit `?on=0`, all answered `{"backflushOn":true}` and the machine stayed
in `BACKFLUSH_IDLE`. The C++ has one call for both directions —
`setBackflushMode(!backflushMode())` (`WebServerManager.cpp:489-491`) — and
`Command::SetBackflushMode(bool)` is that call.

### 30c. An unknown `/api/` path answered ESP-IDF's `405`, not the C++'s `404`

The C++'s CORS is `AsyncCorsMiddleware`, which is not a URI handler and shadows
nothing, so a mistyped URL reaches `handleNotFound` and gets a JSON `404`
(`WebServerManager.cpp:1006-1027`). This firmware answers preflight with a URI
wildcard `/api*` (§22), and to ESP-IDF a wildcard **is** a handler: the URI
matches, the method does not, and ESP-IDF answers its own `405 text/html`
before the registered `404` handler can run. The same handler is now registered
for `405` as well, and `cc_web::help::unmatched` decides between the C++'s
`404` (nothing is registered for that path) and an honest `405` (a real route,
wrong method).

### What pins them

- `cc_web::request::tests::a_fractional_setpoint_survives_to_the_command` —
  `93.5`, `80.5`, `91.2` reach the command intact.
- `cc_machine::handlers::tests::the_backflush_mode_toggle_turns_the_mode_off_again`
  — on, off, on again, through the command the web layer now sends.
- `cc_web::help::tests` — the `404`/`405`/`plain` decision, and that the query
  string does not change it. Plus
  `cc_hal_esp32::web::tests::every_registered_route_is_in_the_raw_handlers_own_table`,
  a device case, because `ROUTE_PATHS` is a compile-time list that has to agree
  with the runtime `routes()` or a real route answers a `404`.

The retune behaviour is **not** in this section: leaving it is §0 behaviour, and
changing it is recorded at §31.

---

## 34 — A PID gain written at runtime takes effect on the next tick 🔴 changed {#d34}

**The C++ deliberately does not do this**, and this is the one item here that is
a decision rather than a defect.

`ProcessController::updatePIDState` chooses the gains inside
`if (lastMachineStatePid_ != machineState)` (`ProcessController.cpp:170`), and
`lastMachineStatePid_` is only a state. So a `pid.regular.kp` written over HTTP
is range-checked, applied to the `Config`, written to NVS, reported back by
`GET /api/parameters` and survives a reboot — while the **running** PID keeps
the old gains until the machine happens to change state. Measured on a bench
ESP32 on 2026-10-05: `POST /api/parameters pid.regular.kp=62` after a test had
left it at 2.5, and `heaterPower` stayed at 21.8 % (the old tuning's answer at
a 65 K error) for as long as the machine sat in `PID_NORMAL`. Cycling the PID
with `POST /api/pid?on=0` then `?on=1` made it jump to 100 % at the same error,
which is what the status page documents for this machine.

`Control::retune_now` clears `tuned_for`, so the next tick re-chooses the gains
for the state the machine is in. Called from `config_io::push_into_machine`
when any written key starts with `pid.` — a prefix rather than a list of keys,
because the gains are derived (`ki` comes from `tn` and `i_max`, brew detection
has its own subtree) and a hand-kept list would rot. `set_tunings` writes gains
without touching the integrator, so being broad costs one idempotent write.

**What did not change.** The state rule is intact: the same gains are still
chosen per state, and the brew-detection gains still cannot leak into
`PID_NORMAL` by a stale assignment, because that guard is *why* `tuned_for`
exists. Only the trigger moves — from "the state changed" to "the state changed,
or a gain was written".

Approved on request, 2026-10-05.

### What pins it

- `config_io::touches_pid_gains` — the predicate, by prefix.
- Bench only, and honestly labelled as such: this path is in `cc-firmware`,
  which `cc-hal-esp32` cannot depend on, so the device-test registry cannot
  reach it. The verification is the measurement above, repeated after the
  change: write `kp`, read `heaterPower` within one control period, no state
  change in between.

---

## 35 — The port is closed; five decisions, and what each costs 🔴 changed {#d35}

Decided 2026-10-06 by Eduard Marbach, at the close of the migration. **None of
this changes runtime behaviour**, so unlike the sections above there are no
`ledger` blocks: `cc-parity` classifies differences the firmware *exhibits*, and
there are none here. What there is instead is the record of five things that were
open, what was decided, and what would reverse each decision.

### 35.1 The C++ parity baseline is never captured

`docs/history/baseline/cpp/` holds only `.gitkeep`, and `just parity` reports
`BASELINE-MISSING` for all 17 scenarios. Capturing one means flashing the
deleted C++ onto a powered, wired machine — which runs its own control loop.
The tree is recoverable (`git show 9fa8c834:...`, `AG-REPO-27`) but its
PlatformIO build is not, so the cost is a reconstruction plus a machine.

**Decided: abandoned, permanently.** Not deferred — the owner declined the
capture when it was first offered, and the cost has since gone up rather than
down. **What "intentional difference" means in `divergences.md` from now on is
reviewed and reasoned, never measured.** **Reverses if** the C++ is rebuilt from
`9fa8c834` and a powered machine is available to run it against.

### 35.2 The water path is live, and the bring-up inhibit is gone

R4-01 added a `test_only` inhibit holding the pump and the valve off while the
heater stayed live, because its acceptance criterion was the PID. Nothing since
has revisited it, and a machine flashed with that image heats, displays and
serves an API **and cannot move water**. Steam was dead with it: steam shares
the valve relay.

**Decided: the inhibit is deleted.** It was not parity — the recovered oracle's
boot log records "pump on GPIO27, valve on GPIO17, both asserted off" as a
*boot* state (`recovered-oracle.md:92`) and its debug surface includes
`/debug/brew/start` and `/debug/hotwater/on` (`:209`); the C++ moved water.
`cc_hal_esp32::Inhibit` stays, with its device test, for a bring-up build that
wants it. **Verified on a bench ESP32**: the boot log reads `no inhibit, the
water path follows the reducer`, and the control line reports
`refused pump=0 water=0 steam=0 heater=0`. **Reverses with** one
`actuators.set_inhibit` call.

### 35.3 The configuration moves by download and re-upload, not by migration

The two firmwares use different NVS namespaces (`config` against `cc`), and
§32's reasoning for refusing a migration was accepted then. What is new is the
operator path being pinned rather than assumed: an operator downloads
`config.json` from the C++ UI and uploads it here, which works because the key
names are the C++'s own dotted names.

**Decided: the hand-off is the procedure, and it is pinned by a test.**
`every_cxx_config_key_is_still_a_key_the_schema_knows` holds the C++'s own 96
keys — recovered from `getAllConfigParams()` at `9fa8c834:src/Config.cpp:438` —
and fails on a rename. Runbook §12 is the operator's steps, and says plainly
that the downloaded file contains Wi-Fi and MQTT passwords in cleartext.

### 35.4 Four of the six R4-04 safety cases are runnable on a bench; all four have now been run

Runbook §13 has procedures for overtemp trip, the emergency latch and its
recovery, the tank-empty pump inhibit and OTA actuator-off — all observable on a
bench with LEDs on GPIO2/27/17. **All four have since been run on a bench
ESP32 (2026-10-07):** 13.4 and 13.1 passed, 13.2 half-passed (its refusal half
confirmed, and its recovery half is what found finding #15), 13.3 part-passed with
its tick timing unmeasured. Runbook §13 carries the results.

**Not runnable on a bench, each with what it needs:** the watchdog reboot needs a
debug route this port does not have (the oracle's `/debug/hang-supervisor` was
never ported); tank-empty pump *kill* and valve fail-safe need the machine,
because they are about a real float switch, a real pump and a real valve
de-energised. Owner: Eduard Marbach.

### 35.5 The flash path, and the two defects that surfaced while exercising it

Running §13 and §2b on a bench ESP32 on 2026-10-06 found two things that the
green gate could not see, both now in [`../status.md`](../status.md):

- **`just flash` never wrote the partition table.** The comment claiming the ELF
  carried it was wrong in both halves; esp-idf-sys 0.38.1's README says the build
  does not consume a custom CSV and that flashing must pass
  `--partition-table`. Fixed, and the device now boots `app0`/`app1`/`littlefs`/
  `coredump` from `rust/partitions_4M.csv`.
- **An OTA does not take effect.** `esp_ota_end` validates the image and does not
  select the slot; `esp_ota_set_boot_partition` is never called, and rollback is
  not compiled in. The upload answers `200` and the device reboots into the slot
  it came from. **Fixed 2026-10-07 (`83b3c419`)** after the owner took the
  decision: `Writer::end` now calls `esp_ota_set_boot_partition` after a
  successful `esp_ota_end`. The cost stands and is stated in the code: with no
  rollback, a bad image in the selected slot is unbootable without USB.

---

## 36 — The emergency latch drains action requests, and the C++ does not 🔴 changed {#d36}

Finding #15 of [`outstanding-findings.md`](./outstanding-findings.md). Approved
by Eduard Marbach on 2026-10-07, on the measurement below.

### What the C++ does

Nothing. `EmergencyStopState::performEmergencyShutdown`
(`9fa8c834:src/state/states/EmergencyStopState.cpp:49-52`) is
`context.emergencyShutdown()` plus `context.setPidRuntimeState(false)`, and no
handler clears `brewStartRequested_` while the latch is up. A brew pressed during
an over-temperature survives the whole latch.

### What the Rust does

`EMERGENCY_STOP` drains the action requests on entry **and on every tick while
the latch persists** (`cc-machine/src/states.rs`, both arms), emitting
`Effect::ClearActionRequests` so the flag is cleared in the appliance and in the
reducer's next view of the machine. The guard on the tick drain is
`machine.is_emergency_stop()` — the same condition as this state's exit
transition, which is what ADR-0003 rule 3 asks for.

### The measurement that decided it

Bench ESP32 rev 3.0, `just bench-flash`, emergency threshold 30 °C, probe warmed
by hand, 2026-10-07:

1. The machine tripped into `EMERGENCY_STOP`; the PID stopped and the heater LED
   went dark.
2. A brew press during the latch was refused — `enablePump REFUSED — latched 1`,
   pump LED dark. Correct, and the reason it looked harmless.
3. The probe cooled below the threshold, the latch cleared, **and the machine
   started that same brew with nobody pressing anything.**

On a machine the water is at brewing temperature and the reservoir is not empty,
and the operator who pressed brew has usually walked away.

### Why this is compliance, not invention

`AG-REPO-24` — "States that cannot act on action requests must drain incoming
flags to prevent unexpected transitions on recovery" — and ADR-0003 rules 2 and 3
both ask for exactly this. The C++ never did it; `AG-REPO-24` was written *about*
this class of defect (ADR-0003's own background is `PidDisabledState` not
draining, which cost a brew that started itself).

**The "never drain wake-up signals" rule is not touched.** That rule is about
`STANDBY` preserving `brewStartRequested` so that *waking* runs it. An emergency
stop is not a state anyone intends to wake from, and draining it is the whole
point: recovery must require a fresh press.

### What pins it

`cc-machine/tests/emergency_latch_drains.rs`, three tests, all of which fail
against the pre-fix code and pass against it:

- `a_brew_pressed_during_the_latch_is_refused_and_then_dropped` — nothing is
  energised while latched, `ClearActionRequests` is emitted **every tick** (a
  request arrives after entry has run, so an entry-only drain is not enough),
  and `requests.brew_start` is false afterwards.
- `the_recovery_tick_starts_no_brew` — after five latched ticks and a cleared
  latch, the machine leaves `EMERGENCY_STOP` and **no brew begins**.
- `an_empty_tank_drains_the_same_press_and_the_latch_now_does_too` — the
  contrast with R4-04 case 13.3, where `WATER_TANK_EMPTY` already drained the
  same press and the operator watched it happen at the LEDs.

```ledger
{"id":"div36","heading":"## 36 — The emergency latch drains action requests","scenarios":["overtemp_trip","overtemp_recovery"],
 "matchers":["/effect rust:ClearActionRequests/","/state EMERGENCY_STOP/"]}
```

---

## 37 — The error states drain action requests, and the C++ does not 🔴 changed {#d37}

Found by review on 2026-10-07, and the same class as [§36](#d36): a state that
cannot act on an action request must not leave one behind for the state it
recovers into (`AG-REPO-24`).

### What the C++ does

Nothing. `ErrorStates.cpp` never clears `brewStartRequested_`.

### What the Rust does

`SensorError` and `EepromError` drain on entry, alongside the `EMERGENCY_STOP`
drain from §36.

### Why it matters now and did not before

`SENSOR_ERROR` recovers on `error_duration > ERROR_RECOVERY_DELAY_MS`
(`states.rs`) and `PID_NORMAL` acts on whatever `brew_start` it finds. Before the
inhibit's deletion a brew request that survived an error state ended at
`enablePump REFUSED`; now it starts a real brew, at whatever temperature the
boiler happens to be. The trigger is ordinary rather than exotic — **runbook
§13.4 records the machine falling into `SENSOR_ERROR` during an ordinary OTA
write**, because the DS18B20 stalls while flash is erased.

### The test that was wrong, and why

`ported_pid_state_transitions.rs` asserted the opposite — *"the error states drain
nothing, and that is deliberate … the drain happens on entry to the recovery state
instead"*. Two things were false about that:

1. `PidNormal`'s `on_entry` is empty. **There is no drain on the recovery
   state.**
2. The comment it cited (`ErrorStates.cpp:52-54`) is about the machine *staying*
   in `SENSOR_ERROR` so a persistent fault stays visible. It says nothing about
   requests.

The test had carried a plausible-sounding mechanism that does not exist, for as
long as it existed. It now asserts the corrected behaviour, and the reasoning is
recorded here rather than deleted.

### What pins it

- `ported_pid_state_transitions.rs::the_error_states_drain_requests` — both
  error states, entry drain and the request's disappearance.
- `cc-machine/tests/emergency_latch_drains.rs::a_brew_pressed_during_a_sensor_error_is_not_acted_on_after_it_recovers`
  — the end-to-end version: press brew, fault the probe, let it recover, and the
  brew has not started. Verified to fail against the pre-fix code.

### Related, deliberately NOT changed

`BackflushFilling`'s update still does not re-assert its pump or valve. That is a
**recorded decision** (`cpp-findings.md` §13: "Rust: preserved. Pinned by
`s13_*`"), and the `s13_backflush_filling_never_re_asserts_its_hardware` test
still enforces it, so changing it here would have overturned a decision this
branch was not asked to revisit. What the inhibit's removal changes is the cost:
a single tank-interlock refusal on entry now leaves the fill stalled and silent
for the whole cycle, where before it was indistinguishable from correct
behaviour. Recorded as `outstanding-findings.md` #18 for the owner.

---

## 38 — An unsafe configuration is refused at the write, and repaired at boot 🔴 changed {#d38}

Finding #12. 2026-10-08, bench ESP32: previous image stored `safety.emergency_temp=120`; this image's first boot repaired that key only; repeat `POST /api/parameters` answered `400`. Record: [`outstanding-findings.md` #12](outstanding-findings.md).

### What the C++ does

No cross-parameter check. A key in range (`Config.h:isValid`) is stored, including a pair that cannot run: `safety.emergency_temp` below `steam.setpoint + hysteresis`, a `LOW_TRIGGER` relay, brew-by-weight with no scale.

### What the Rust does

`validate_config` failure stores nothing:

- `POST /api/parameters` and `POST /api/config/upload` wait, then HTTP 400 naming `ConfigViolation::implicated_keys`.
- MQTT `apply_parameter` logs, does not save, does not `push_into_machine`.
- `Command::SetSetpoint` validates first. `/api/setpoint` already answered `202 {"accepted":true}` (`register_command`). No second ack. Control task must not apply an unsafe setpoint.

Boot repair keeps reverting the keys `validate` returns. `REPAIR_ESCALATION_KEYS` run one key at a time, and only when those keys make no progress. Still unsafe: full defaults plus the Wi-Fi credential. Boot line: `stored but unsafe`, not `DISCARDED`.

### What pins it

- `cc-safety/tests/safety_paths.rs::div38_every_violation_names_the_keys_it_implicates`
- `cc-config/tests/config_repair.rs`
