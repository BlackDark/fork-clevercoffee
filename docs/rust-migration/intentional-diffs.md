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

## 5. The heater is driven by LEDC, not a 10 ms ISR 🔴 changed

| | |
| --- | --- |
| **Task** | R1-07; [04 §5](./04-target-architecture.md) |
| **Test** | `cc-domain/src/heater.rs` — 16 tests, all host. `cc-hal-esp32/src/heater.rs` — device, not host-testable |
| **Hardware** | **NOT VERIFIED.** See "Open" below. |

### What the C++ does

A hardware timer ISR every 10 ms drives the heater relay directly, bypassing
`HardwareManager` entirely:

```cpp
if (currentPidOutput <= currentCounter) { relay->off(); } else { relay->on(); }
unsigned int newCounter = currentCounter + ISR_COUNTER_INCREMENT; // 10
if (newCounter >= ctx->processWindowSize()) newCounter = 0;        // 1000
```

(`include/clevercoffee/isr.h:85-118`, `windowSize_ = 1000` at
`include/clevercoffee/context/ProcessState.h:183`,
`ISR_COUNTER_INCREMENT = 10` at `include/clevercoffee/constants/Timing.h:17`.)

This is the **only hard-real-time requirement in the firmware**: 100 interrupts
per second at the highest priority the chip offers, each doing a GPIO write and a
counter add, and the C++ deliberately bypasses the hardware manager's
`heaterEnabled_` bookkeeping so the ISR cannot race it — which is a design smell
the migration exists to remove.

The recovered previous Rust firmware did the same, and said so in its boot log:
`"heater interrupt running on GPIO2 (active high), output held off until the
supervisor beats"` ([08 §3](./08-recovered-oracle.md)).

### ⚠ The ISR rate is not the chopper rate — this is the whole entry

**An earlier revision of this file, of `04 §5` and of the `cc-hal-esp32` and
`cc-domain` module docs, justified a 100 Hz LEDC carrier as "reproducing the
existing 10 ms-step / 1 Hz chopping exactly". That was wrong, and the error was
in reading the C++'s ISR rate as its switching rate.** It is recorded here
because the correction is the substantive content of this diff, not the
mechanism.

The ISR *fires* 100 times a second. The relay *level* changes about **twice** a
second. The predicate `pidOutput > counter` is **monotone in the counter**, so
for a constant PID output the energised run is one contiguous block starting at
counter 0; the other 99 ISR entries per second re-assert the level they already
set. Re-asserting a level is not a transition, and a contactor does not care
about it:

| `pid_output` | ISR entries/s | on-ticks | on-time | relay level over the window | **level changes/s** |
| --- | --- | --- | --- | --- | --- |
| 0 | 100 | 0 | 0.0 % | OFF throughout | **0** |
| 50 | 100 | 5 | 5 % | ON 50 ms, then OFF 950 ms | **2** |
| 500 | 100 | 50 | 50 % | ON 500 ms, then OFF 500 ms | **2** |
| 950 | 100 | 95 | 95 % | ON 950 ms, then OFF 50 ms | **2** |
| 1000 | 100 | 100 | 100 % | ON throughout | **0** |

Two edges per second, never more: one falling edge inside the window, one rising
edge at the wrap. `cc_domain::heater::cpp_transitions_per_second` computes that
from the transcribed predicate rather than asserting it.

So the C++ is a **1 Hz chopper with a 10 ms duty quantum**, and a 100 Hz carrier
would switch the contactor **200 times a second — a hundred times its mechanical
duty** — while matching the delivered power almost exactly. On a 2 kW boiler
contactor, whose life is counted in operations, that is a defect dressed up as
fidelity. **The carrier frequency must be low.**

### What the Rust does

**The window is unchanged — 1 Hz, 100 steps of 10 ms — so the PID's control law
and every gain in `include/clevercoffee/defaults.h` are untouched.** Only the
delivery mechanism changes: a **1 Hz LEDC carrier**, one period per control
window, whose duty *is* the chopper's on-time fraction.

| | C++ ISR chopper | Rust LEDC |
| --- | --- | --- |
| on-time resolution | 10 ms (1 % of the window) | 1/131 072 of the carrier period (7.6 µs) |
| delivered power | `on_ticks / 100` | identical, by construction |
| **contactor level changes/s** | **0 or 2** | **0 or 2** — required to be equal, and tested |
| CPU per second | 100 interrupt entries | **0** |
| worst-case latency | one interrupt-priority preemption | n/a — the register *is* the output |
| 10 ms hard real-time requirement | yes | **no** |

The delivered power is identical because the quantisation is the same function:
`cc_domain::heater::chopper_on_ticks` is a transcription of the ISR's
`pidOutput > counter` predicate, counted, and `on_fraction` is
`on_ticks / 100`. `the_tick_table_is_the_cpp_isr` walks all 100 counters at duty
0 and at full duty and checks both edges; `the_ten_millisecond_quantisation` pins
the two off-by-ones a division gets wrong (a duty below one step still gets a
whole step, and a duty of exactly `n * 10` is `n` steps, not `n + 1`).

The transition rate is a separate, equal property, not a consequence of the
power match: `the_carrier_does_not_switch_the_contactor_more_than_the_cpp_does`
computes both sides and requires them to be **equal** at every half-millisecond of
the PID range, and requires the carrier's answer never to exceed 2/s.

### The frequency and resolution: what is actually achievable

**1 Hz.** An `f` Hz square wave makes `2f` level changes per second, and the C++
makes at most 2, so `f ≤ 1 Hz`. Independently, one carrier period *is* one
control window, so at 1 Hz the duty count can mean the same thing the C++'s
millisecond duty meant.

That constrains the resolution in the opposite direction from the usual one: a
low carrier means a *coarser* duty step, so the 1 % (10 ms) fidelity has to be
bought with bits rather than with frequency. Reproducing the C++'s 10 ms quantum
at a 1 s period needs `max_duty ≥ 100`, i.e. ≥ 7 bits. The binding constraint is
the peripheral's, not that one.

`ledc_calculate_divisor` (`esp_driver_ledc/src/ledc.c:459-477`) computes

```text
div_param = ((src_clk << 8) + freq_hz * precision / 2) / (freq_hz * precision)
```

with `precision = 1 << duty_resolution` (`ledc.c:600`), and
`LEDC_IS_DIV_INVALID` rejects `div_param <= LEDC_LL_FRACTIONAL_MAX` (255) or
`> LEDC_TIMER_DIV_NUM_MAX` (`0x3FFFF` = 262 143) (`ledc.c:115,111`). The
original ESP32's LEDC source is APB at 80 MHz (`LEDC_LL_GLOBAL_CLOCKS` lists
`LEDC_SLOW_CLK_APB` first; `esp-idf-hal` passes `LEDC_AUTO_CLK`). The timer
period is `(div_param >> 8) * 2^bits` source clocks:

| bits | `div_param` at 1 Hz | valid? | `max_duty` | duty step | realised carrier |
| --- | --- | --- | --- | --- | --- |
| 7 | 160 000 000 | ✗ above `0x3FFFF` | 128 | 7.81 ms | — |
| 16 | 312 500 | ✗ above `0x3FFFF` | 65 536 | 15.3 µs | — |
| **17 (chosen)** | **156 250** | **✓** | **131 072** | **7.63 µs** | **exactly 1.000 000 Hz** |
| 18 | 78 125 | ✓ | 262 144 | 3.81 µs | exactly 1.000 000 Hz |
| 19 | 39 063 | ✓ | 524 288 | 1.91 µs | 0.999 987 Hz |
| 20 | 19 531 | ✓ | 1 048 575 | 0.95 µs | 1.000 013 Hz |

**At 1 Hz the reachable resolutions on the original ESP32 are 17, 18, 19 and 20
and nothing coarser** — 16 bits already overflows the maximum divider, and every
bit below that overflows it further. `Bits17` is the **coarsest that works**,
which is the right way round: most margin against the divider arithmetic being
wrong, and 19 and 20 are the two that lose exact frequency to the rounding term.
Concretely `div_param = 156 250 = 610 × 256 + 50`, so the period is
`610.3515625 × 131 072` = **80 000 000 APB clocks = 1.000 000 s exactly**. If
`LEDC_AUTO_CLK` were to fall through to `RC_FAST` (≈8 MHz) instead, `div_param`
would be 15 625 and the period 8 000 000 `RC_FAST` clocks — also exactly 1 s.
The frequency does not depend on which clock the driver picks.

**For the record, the earlier revision's 100 Hz table was also wrong**, in a way
that happened not to change its conclusion: at 100 Hz the valid resolutions are
**10–19 bits**, not "8, 9, 10" — `Bits8` asks for `div_param` 800 000 and
`Bits9` for 400 000, both above `0x3FFFF`. `div_param` at 100 Hz / `Bits10` is
**200 000**, not the 50 000 the old text quoted. 100 Hz / `Bits10` is a *valid*
pair, so the old build would have configured fine; it is the *frequency* that was
the defect.

### The duty ends, and why 17 rather than 20

`esp-idf-hal`'s `Resolution::max_duty` is `2^N`, **except** at 20 bits where it is
`2^20 - 1`, and `ledc_channel_config` says why in its own comment (`ledc.c:873-880`):

> On ESP32 … due to a hardware bug, 100 % duty cycle (i.e. `2**duty_res`) is not
> reachable when the binded timer selects the maximum duty resolution.

The maximum low-speed resolution on the ESP32 is 20 bits, so at `Bits20` "100 %
duty" would be `1 048 575 / 1 048 576` — a permanent 0.95 ms notch once a second.
At 17 bits `max_duty` is a plain `131 072`, so:

* **duty 0** is a *steady low level* — the idle level `ledc.c` configures — and
  is what a closed gate produces, so "disabled" and "off" are the same value;
* **duty `max_duty`** is a *steady high level*, a different value from 0, so full
  power is never confused with disabled;
* **any interior duty** is one high pulse per second at `hpoint = 0` — i.e.
  starting at the beginning of the period, which is where the C++ puts it too
  (energised at counter 0, de-energised as the counter passes `pidOutput`).

`full_duty_is_distinguishable_from_disabled` pins all three, and
`RESOLUTION`/`CHOSEN_MAX_DUTY` are tied together by a **`const` assert** in
`cc-hal-esp32::heater` so that reaching `Bits20` is a compile error rather than a
review note.

`duty_counts` still clamps to `0 ..= max_duty`, and
`the_count_never_exceeds_max_duty` still pins it, because
`LedcDriver::set_duty` clamps *silently* and a silently-wrong duty on a heater is
worse than an error.

### The reproduction error, measured

R1-07's acceptance bound is "duty matching the PID output within 1 %". The chosen
pair is three orders of magnitude inside it:

| | C++ chopper | Rust LEDC at 1 Hz / `Bits17` |
| --- | --- | --- |
| duty quantum | 10 ms (1 % of the window) | 7.63 µs (1/131 072) |
| worst-case reproduction error | — | **3.815 µs** = half a count = 0.00038 % of the window |
| stated tolerance (`REPRODUCTION_TOLERANCE_MS`) | — | **10 µs**, 2.6× the rounding floor |

`the_chosen_carrier_reproduces_the_cpp_on_time` walks the whole PID range at
half-millisecond granularity and requires `|delivered_on_time_ms − C++ on-time| ≤
10 µs`. `the_chosen_carrier_reproduces_the_cpp_on_time_at_the_analytic_bound`
separately pins the 3.815 µs floor, which is arithmetic and cannot drift.
**Neither tolerance was loosened to make a test pass**, and both fail loudly if
`CARRIER_HZ` or `CHOSEN_RESOLUTION_BITS` is changed.

The narrowest pulse the hardware can be asked for is therefore still the C++'s own
10 ms step, because the duty is `on_fraction` and `on_fraction` is quantised to
100 steps — 131 072 counts of headroom that nothing ever uses.
`the_minimum_pulse_is_the_cpp_ten_millisecond_step` walks the range to prove it.

### High-speed mode is not used, and does not need to be

The original ESP32 is the only chip with LEDC high-speed mode
(`esp-idf-hal-0.47.0/src/ledc.rs`, `#[cfg(esp32)] pub struct HighSpeed`).
**Decision: low speed mode.** High speed exists for the multi-MHz carriers low
speed cannot produce; at 1 Hz, low speed is six orders of magnitude inside its
range. Taking high speed would buy nothing and would consume one of the four
high-speed timers, which is the scarce resource on this part. If the carrier ever
has to go above ~1 MHz, `LedcPwm::new`'s timer type parameter is the line to
change.

### The gate, preserved from the oracle

The recovered firmware's two safety properties are implemented, not invented:

* **The output is held at duty 0 until the supervisor's first heartbeat** —
  `"output held off until the supervisor beats"` ([08 §3](./08-recovered-oracle.md)).
  `HeaterGate::new()` is the only constructor and there is no public field, so
  "the gate starts open" is not expressible. `a_fresh_gate_refuses_every_duty`
  pins it.
* **A supervisor that stops beating drops the heater** — the deadman
  ([08 §4](./08-recovered-oracle.md)). `DEADMAN_TIMEOUT_MS` is **1000 ms**, chosen
  rather than recovered: the value is not in the recovered binary's strings, only
  the phrase `deadman=armed`. It must be shorter than the 5 s task watchdog or the
  deadman buys nothing over the reset, and longer than one interlock period
  (500 ms, also from the boot log) or a single long preemption would drop the
  heater of a healthy machine. Two interlock periods satisfies both and bounds
  de-energisation at 1.5 s rather than 5 s. It is a named constant in one place
  for exactly that reason.

Gating happens **last**, immediately before the register write, so the value in
the register is at most one interlock period stale and the worst case is
stale-**low**, never stale-high. The millisecond rollover is covered
(`the_deadman_survives_the_millis_rollover`): the heater runs across the 49.7-day
wrap for the whole of it.

### ⚠ Open — the contactor is unknown, and R1-07 stays open

**Matching the C++'s transition rate is necessary and not sufficient. This
section is the reason R1-07 is not finished, and it needs a person with the
machine in front of them, a scope, a dummy load and the boiler disconnected.**

What has been *removed* by this correction is the specific worry that motivated
the 100 Hz carrier: the contactor is no longer being asked for a hundred times
its mechanical duty. What has **not** been established, and cannot be
established from this repository, is any of the following. Each is stated as a
measurement to be made, not as an assumption to be made:

1. **The contactor's minimum on-time and minimum off-time.** The software
   guarantees it never requests a pulse narrower than 10 ms, because that is the
   C++'s own quantisation. Whether 10 ms is *itself* within the contactor's
   ratings is a datasheet or measurement question. **Unknown.**
2. **Whether a hardware-PWM output is acceptable to the coil at all.** LEDC drives
   a square wave into the same relay pin the C++ drove with a GPIO write. 1 Hz is
   a frequency the C++ never *produced* (its average was 1 Hz, its waveform was
   10 ms steps), so the coil's behaviour at a true 1 Hz square wave is untested
   even though the heating is identical. **Unknown.**
3. **The realised frequency and duty on the pin.** Everything above is
   arithmetic. `ledc_timer_config`'s divider selection, the duty register, and the
   idle level have not been read back from hardware. **Not measured.**
4. **Whether 1 Hz is the *best* choice even if it is acceptable.** The trade is
   real and it has not been settled: a lower carrier reduces switching further but
   coarsens the duty step and eventually drops below the C++'s 10 ms quantum; a
   higher carrier is finer but is more mechanical wear. 1 Hz was chosen because it
   is the frequency that makes the two match exactly. Someone with the machine
   may reasonably prefer a different point on that curve, and that is a decision
   to record, not to guess.

R1-07 steps 1 and 2 (drive a dummy load, measure with a scope) are **not run**.
There is no scope and no dummy load attached, the board's boiler-disconnection
state is unconfirmed, and skill §2 rule 4 forbids an energising test without a
reviewed procedure. **The hardware acceptance criterion is therefore unverified
and is left that way rather than claimed.**

---

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
