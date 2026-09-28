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

---

## 17. 🔴🔴 `TempSensorDallas` accepts −251 °C and −250 °C, and cannot reach the faults it checks

**Found 2026-09-28 while porting the DS18B20 (R1-03 / R3-06).**

`TempSensorDallas::sample_temperature` (`TempSensorDallas.cpp:26-43`) is two
`if` blocks:

```cpp
if (temp == DEVICE_DISCONNECTED_C) { ... return false; }          // -127
if (temp == DEVICE_FAULT_OPEN_C || temp == DEVICE_FAULT_SHORTGND_C
                               || temp == DEVICE_FAULT_SHORTVDD_C) { ... return false; }
temperature = temp;                                              // -251 and -250 land here
```

Three things are wrong with that, and none of them is the port's fault.

### 1. The two faults it does *not* check are the only two a DS18B20 produces

`DallasTemperature::calculateTemperature` (`:570-577`) returns
`DEVICE_POWER_ON_RESET_RAW` for a `0x50/0x05/0x0C` scratchpad and
`DEVICE_INSUFFICIENT_POWER_RAW` for `0xFF/0x07` — **and both of those arms are
guarded by `if (deviceAddress[DSROM_FAMILY] == DS18B20MODEL)`**. They are the
DS18B20's faults. Neither sentinel is in the check above, so both are accepted
as valid readings.

They do not reach the control loop as −251 °C, though: `rawToCelsius` (`:406-410`)
folds anything at or below `DEVICE_DISCONNECTED_RAW` (−7040) to −127 first, and
both raw sentinels are below that. So the C++ rejects them **as
`Disconnected`**, by accident, through the first `if`.

The result is a *log message* that is wrong, not a safety hole: a
power-on-reset or a brownout on the probe is reported as "Temperature sensor
not connected".

### 2. The three faults it *does* check are unreachable on a DS18B20

`DEVICE_FAULT_OPEN_RAW` / `_SHORTGND_RAW` / `_SHORTVDD_RAW` are returned only by
the `DS1825MODEL` arm (`:539-552`) — the **MAX31850**, family `0x3B`. A DS18B20
never takes that branch, and `rawToCelsius` could not surface the values anyway
for the reason in (1). So the second `if` block in `TempSensorDallas.cpp:33-35`
is **dead code** on the sensor this machine has.

### 3. The all-`0xFF`/`0x07` short-read case is caught, but the short *bus* case is not

An all-`0xFF` scratchpad is `0xFF/0x07` and is caught. A genuinely short bus
read is not distinguishable from a good one, because nothing checks the count.

### What the port does

All four rejections are **kept** — including the three that a DS18B20 cannot
produce — so a machine whose probe is swapped for a MAX31850 fails closed rather
than reading a fault code as a temperature. All six faults are reported as a
typed [`Ds18b20Fault`] rather than as Celsius sentinels, so the diagnostic is
right and the *decision* is bit-identical to the C++'s.

Pinned by `cc_domain::onewire::tests::s5_the_dallas_fault_sentinels_are_ckd`,
`s7_the_ds18b20_fault_registers_cannot_be_reached` and
`s8_only_four_of_the_six_faults_are_rejected`, and by
`cc_domain::ds18b20::tests::s8_only_four_of_the_six_faults_are_rejected`.

### Also found: `rawToCelsius` cannot read −55 °C

`DEVICE_DISCONNECTED_RAW` is **−7040**, which is exactly −55 °C in 1/128 units —
the bottom of the DS18B20's range. The `raw <= DEVICE_DISCONNECTED_RAW` test in
`rawToCelsius` is a range check, not a sentinel check, so **the lowest
temperature the device can physically report is reported as disconnected.** A
real off-by-one in the C++; harmless on a coffee machine, and preserved in the
port because changing it would be a behaviour change. Pinned by
`s6_minus_55_c_is_reported_as_disconnected`.

---

## 18. 🔴 `TempSensor::isValidTemperature` is dead, and the DS18B20 path has no range check

**Found 2026-09-28 (R1-03).**

`TempSensor::isValidTemperature` (`TempSensor.h:91-93`) is a `static constexpr`
predicate for −50 °C..150 °C, and it is **never called**: not from
`updateTemperature` (`:36-58`), not from `tryGetValue` (`:97-141`), not from
anywhere in the tree. The range that is actually enforced is S1's —
`Temperature::MIN_VALID_TEMP_C` (0.0) / `MAX_VALID_TEMP_C` (200.0) in
`EmergencyStopManager::checkEmergencyConditions`
(`EmergencyStopManager.cpp:25-30`).

The consequence is that the two temperature sensors behave differently on
purpose and by accident:

* `TempSensorTSIC` has its own range reject, `temp <= 0.0 || temp >= 180.0`
  (`TempSensorTSIC.cpp:38-42`), explicitly commented as stopping a −2.9 °C
  glitch from tripping emergency stop.
* `TempSensorDallas` has **no** range check at all. Its only rejections are the
  four sentinels above.

So on the Dallas path a reading of, say, 165 °C is accepted, cached, folded into
the 15-sample moving average, and passed to the PID — and only trips S1's
deferred over-temperature path. The TSIC driver rejects it outright.

**Preserved.** `cc_domain::ds18b20::PLAUSIBLE_RANGE` is a *query*, not a
filter, and the driver returns the reading unfiltered; S1 remains the thing that
acts. Filtering here would convert a latched emergency stop into a silently-held
last-good value, which is strictly worse. Pinned by
`s10_the_dallas_path_has_no_range_check_of_its_own`.

---

## 19. 🔴 `TempSensor::update_moving_average` reads uninitialised timing on its first sample

**Found 2026-09-28 (R1-03), incidental to reading `TempSensor.h`.**

`update_moving_average` (`:170-205`) runs, on the very first successful read:

```cpp
if (valueIndex < 0) {
    for (int index = 0; index < numValues; index++) {
        tempValues[index]      = last_temperature_;
        timeValues[index]      = 0;
        tempChangeRates[index] = 0;
    }
}
```

so `timeValues` is all zeros, `valueIndex` is −1, and the branch below computes
`(tempValues[0] - tempValues[1]) / (timeValues[0] - timeValues[1])` — a **0/0
division**. In `double` that is `NaN`, which then propagates into
`average_temp_rate_` and into every brew-detection threshold that reads it.

It is a one-shot: the next sample takes the other branch with real timestamps.
But the first sample after boot produces a `NaN` rate, and
`getAverageTemperatureRate()` is what the state machine's brew-phase detection
consumes.

**Not ported yet** — F11 / the rate-of-change filter is not in this slice. Recorded
here so it is not lost.

---

## 20. 🔴🔴 Not a C++ finding: R1-07's 1 Hz LEDC carrier cannot run on this chip

**Found 2026-09-28, on the board, while bringing up the DS18B20 (R1-03).**
Recorded here because every other finding in this file is a C++ bug and this one
is not, and because it is the more serious of the two.

### What happens

The firmware panics on every boot, before the control task runs:

```text
Guru Meditation Error: Core  0 panic'ed (Interrupt wdt timeout on CPU0).
Backtrace: …  ledc_set_duty_and_update → _ledc_update_duty → ledc_ll_set_duty_start
```

Decoded against a `debug = 2`, `strip = none` build of this tree, the faulting
`PC` is `ledc_ll_set_duty_start` at
`components/hal/esp32/include/hal/ledc_ll.h:487`.

### Why

```c
// ledc_ll.h:485-489, ESP-IDF v5.5.5
// wait until the last duty change took effect (duty_start bit will be
// self-cleared when duty update or fade is done)
// this is necessary on ESP32 only, otherwise, internal logic might mess up
while (hw->channel_group[speed_mode].channel[channel_num].conf1.duty_start);
```

`duty_start` is cleared by the hardware at the next **timer period**, and the
spin sits inside `portENTER_CRITICAL(&ledc_spinlock)` in
`ledc_set_duty_and_update` (`components/esp_driver_ledc/src/ledc.c:1603-1606`),
i.e. with interrupts masked.

R1-07 chose a **1 Hz** carrier — [`cc_hal_esp32::heater`] argues at length for
exactly that, and the argument is sound: an `f` Hz square wave makes `2f`
contactor operations per second, and two per second is the right budget for a
boiler contactor. The spin is therefore **up to one second** with interrupts
disabled. The original ESP32's interrupt watchdog is **300 ms**
(`components/esp_system/int_wdt.c`). Every duty write trips it.

It is not a high-duty problem. `LedcPwm::new` writes duty **0**, and
`duty_start` self-clears at the next period whatever the duty is, so the very
first write panics.

### What it means for the plan

* The "1 Hz or the contactor wears out" argument in `heater.rs` is **correct and
  incomplete**. It never checked what ESP-IDF's own `ledc_ll_set_duty_start` does
  on this chip. *Any* carrier slow enough to matter mechanically is also slow
  enough to trip a 300 ms interrupt watchdog through that spin.
* `just flash` has been landing an image that panics at boot, and the R1-07 report
  says the firmware "builds and boots with the carrier configured at duty 0". It
  builds. It does not boot. The claim was never checked on hardware, and R1-07's
  own docs already say the hardware test is not done — this is that test, failing
  at step 0.

### The options, none of which is this task's to take

| Option | Cost |
| --- | --- |
| Raise the carrier to ≥ ~3 Hz (spin under 300 ms) | Reintroduces the contactor duty R1-07 exists to avoid, though 6 ops/s is far less than the C++'s 200 |
| Bypass the spin by writing `conf1.duty_start` directly | Needs `unsafe`, which `[workspace.lints.rust] unsafe_code = "deny"` forbids, plus a register the HAL does not expose |
| Configure the duty **once** and then use `ledc_set_duty` (deferred) without an update | Only works if the duty never changes, i.e. never heats |
| Feed the interrupt watchdog from the spin | Not possible: the spin is what has interrupts off |

### What this task did

`BRING_UP_HEATER_LEDC` in `cc-firmware/src/main.rs` is **`false`**, and GPIO2 is
driven as a **plain inactive output** alongside the pump and the valve. That is
strictly safer than a PWM carrier nobody has scoped, and it is what lets the rest
of the bring-up — the temperature probe, in particular — run. `LedcPwm` is
untouched in `cc-hal-esp32`; it is only not brought up.

**This is a stop-the-line finding for R1-07 and needs a human decision before
any heater work continues.**

---

## 17. 🔴 The original ESP32 cannot use LEDC at a 1 Hz carrier — hardware constraint

Discovered 2026-09-28 during R1-03, after the LEDC heater implementation **panicked on
every boot** with `Interrupt wdt timeout on CPU0`. Backtrace:
`ledc_set_duty_and_update` → `ledc_ll_set_duty_start`.

The cause is in the chip's own HAL header
(`components/hal/esp32/include/hal/ledc_ll.h:483-489`):

```c
static inline void ledc_ll_set_duty_start(...) {
    // wait until the last duty change took effect ... this is necessary on
    // ESP32 only, otherwise, internal logic might mess up
    while (hw->channel_group[speed_mode].channel[channel_num].conf1.duty_start);
    hw->...conf1.duty_start = 1;
}
```

**The spin is unique to the original ESP32.** Every other chip's `ledc_ll.h`
(esp32c2, esp32c3, esp32c5, and the s3/h2/p4 equivalents) has the function body reduced
to a single register write with no loop.

That spin runs inside `portENTER_CRITICAL(&ledc_spinlock)` in
`esp_driver_ledc/src/ledc.c` (e.g. `:688`), so a duty update **blocks with interrupts
masked until the hardware clears the bit** — up to one full carrier period. The interrupt
watchdog is 300 ms. At a **1 Hz** carrier that is ~1 s, so it panics. It panics at duty 0
too, because the wait happens before the duty value matters.

**Consequence:** the contactor-friendly 1 Hz carrier (which R1-07 correctly chose to avoid
200 switching operations per second) is **unusable on this chip via LEDC**. The two
requirements are in direct conflict:

| Carrier | Contactor ops/s | LEDC on the original ESP32 |
| --- | --- | --- |
| 1 Hz | ~2 (matches the C++) | **panics — INT WDT** |
| 100 Hz | ~200 (100× the C++) | works |

**Decision: use the 10 ms GPTimer ISR**, which is what the C++ and the lost firmware both
use, and is proven on this hardware. The `HeaterDuty` trait seam is kept so LEDC remains
available for a future chip that supports it. `LedcPwm` is not brought up
(`BRING_UP_HEATER_LEDC = false`); GPIO2 is held as a plain inactive output.

The 100 Hz/ISR CPU cost is ~100 IRQs/s on a 240 MHz Xtensa — negligible. The "LEDC costs
zero CPU" argument in 04 §5 does not survive contact on this chip.

## 18. `TempSensorDallas` cannot reach the faults it checks

`TempSensorDallas.cpp` rejects `DEVICE_FAULT_OPEN_C`, `DEVICE_FAULT_SHORTGND_C` and
`DEVICE_FAULT_SHORTVDD_C`, but those are **MAX31850-only** sentinels. A DS18B20 has no such
fault codes; the two faults it does have arrive as `-127`. So the three checks are dead
code, and the check it omits (`-127`) is the one that matters.

It also **accepts −251 and −250**, which are the DS18B20's power-on sentinel values, and
`rawToCelsius` cannot represent −55 °C (the bottom of the 11-bit grid is −0.125, but the
conversion underflows for the out-of-power range).

## 19. The moving-average filter divides 0/0 on its first sample

`TempSensor::update_moving_average` initialises its sum to zero and divides by the sample
count before the first sample is committed, producing `NaN` on the first reading.

## 20. The temperature path has no range check

`isValidTemperature` in `TempSensor.h` is **dead code** — the Dallas path never calls it,
so there is no `-50..150 °C` validation on the DS18B20 reading. Combined with §18, a
sentinel value can reach PID and emergency-stop logic.
