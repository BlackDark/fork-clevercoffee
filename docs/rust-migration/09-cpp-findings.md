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

> **🔴 THE TITLE OF THIS SECTION IS WRONG, AND WAS WRONG WHEN IT WAS WRITTEN
> (2026-09-28, R1-03/R3-07).** The title claims the C++ accepts -251 °C and -250 °C.
> It does not: `rawToCelsius` folds them to -127 and `TempSensorDallas`'s first `if`
> rejects them, exactly as it rejects the other four. The body text below gets the
> *mechanism* right — all six raws are at or below `DEVICE_DISCONNECTED_RAW` — but then
> draws the wrong conclusion from it. Read
> [the correction in §18](#18-🔴-corrected-2026-09-28-the-ds18b20s-power-sentinels-are-rejected)
> instead; it is short, it is right, and it has a test that stops it being re-inverted
> (`cc_domain::sensor::onewire::div7_every_ds18b20_fault_is_rejected_by_the_cpp`).
>
> **What survives from the original finding**, and is the part worth keeping: the C++
> rejects all six faults but reports all six as *"Temperature sensor not connected"*,
> so a power-on reset on the probe sends an operator to look at the wiring. And
> `rawToCelsius` cannot represent -55 °C, the bottom of the DS18B20's range.

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

## 18. 🔴 CORRECTED 2026-09-28: the DS18B20's power sentinels **are** rejected

The first version of this section claimed that `TempSensorDallas` "accepts −251 and
−250" and that the three wiring checks are "dead code". **Both claims were wrong**, and the
reason is one line of the library the C++ wraps
(`.pio/libdeps/esp32_usb/DallasTemperature/DallasTemperature.cpp:406-410`):

```cpp
if (raw <= DEVICE_DISCONNECTED_RAW) return DEVICE_DISCONNECTED_C;
return (float)raw * 0.0078125f;
```

`calculateTemperature` returns the **raw** sentinels (`DallasTemperature.h:33-55`):

| fault | raw | `<= -7040`? | what `rawToCelsius` returns | what `TempSensorDallas` sees |
| --- | --- | --- | --- | --- |
| `Disconnected` | -7040 | yes | -127 | `== DEVICE_DISCONNECTED_C` → **rejected** |
| `Open` (MAX31850) | -32512 | yes | -127 | rejected |
| `ShortGnd` (MAX31850) | -32384 | yes | -127 | rejected |
| `ShortVdd` (MAX31850) | -32256 | yes | -127 | rejected |
| `PowerOnReset` (DS18B20) | -32128 | yes | -127 | rejected |
| `InsufficientPower` (DS18B20) | -32000 | yes | -127 | rejected |

**All six are rejected.** The second `if` block (`TempSensorDallas.cpp:33-35`), which
compares against -254/-253/-252, *is* dead — but not because those faults are
unreachable, because `rawToCelsius` can never produce those values.

What the C++ actually gets wrong is the **message**: a DS18B20 reporting a power-on
reset is logged as *"Temperature sensor not connected"* (`:30`), which sends an operator
to look at the wiring when the fault is on the probe.

**The real, still-true findings from this section:**

* `rawToCelsius` cannot read **-55 °C**. `DEVICE_DISCONNECTED_RAW` is -7040, which is
  exactly -55 °C in 1/128 units, so the bottom of the DS18B20's range is reported as
  disconnected. A genuine off-by-one in the library; preserved and pinned
  (`cc_domain::sensor::onewire::s6_minus_55_c_is_reported_as_disconnected`).
* The three MAX31850 wiring faults are unreachable on a DS18B20, so on *this* machine the
  second `if` block is dead — but it is kept in the port, so a swapped probe fails closed.

Pinned by `cc_domain::sensor::onewire::div7_every_ds18b20_fault_is_rejected_by_the_cpp`,
whose test name exists specifically so this correction cannot be re-inverted.

See also §17 above, which is the long-form version of the same finding and carries the
same correction.

## 19. The moving-average filter divides 0/0 on its first sample

`TempSensor::update_moving_average` initialises its sum to zero and divides by the sample
count before the first sample is committed, producing `NaN` on the first reading.

## 20. The temperature path has no range check

`isValidTemperature` in `TempSensor.h` is **dead code** — the Dallas path never calls it,
so there is no `-50..150 °C` validation on the DS18B20 reading. A 165 °C reading is
therefore cached, averaged and handed to the PID.

**🔴 CLOSED 2026-09-28 (R1-03/R3-07).** The check is now applied, in
`cc_domain::sensor::ds18b20::Driver::poll`, as a rejected read rather than a temperature.
The reachable cost is small and was measured rather than assumed: the DS18B20's own
range is **-55..+125 °C**, so the only real readings the new check refuses are the
**-55..-50 °C** band, and the upper bound is inert because the sensor cannot report above
150 at all. What *does* change is the diagnostic for a genuine over-temperature in
150..200 °C: it now arrives as a *sensor read failure*, so the machine reaches
`SENSOR_ERROR` rather than `EMERGENCY_STOP`. The heater is off either way.

Tests: `cc_domain::sensor::ds18b20::div8_the_dallas_path_applies_the_range_check_it_never_applied`,
`::div8_the_two_ranges_overlap_only_between_zero_and_a_hundred_and_fifty`,
`::div8_only_the_cold_end_of_the_ds18b20s_range_is_now_refused`,
`::div8_a_reading_outside_the_range_is_a_read_failure_not_a_hot_temperature`.
See `intentional-diffs.md` #8.

The original "combined with §18, a sentinel value can reach PID" claim is **withdrawn**:
§18 is corrected above and no sentinel reaches the PID.

---

## 21. New C++ findings from the sensor port (2026-09-28)

- **§17 correction:** `rawToCelsius` (`DallasTemperature.cpp:406-410`) folds every raw at or
  below −7040 to −127, and the two power-on sentinels are −32128 / −32000, so **all six
  DS18B20 faults are in fact rejected** by the C++. The three MAX31850-only checks are
  still dead here, but for a different reason: `rawToCelsius` cannot produce
  −254/−253/−252. What the C++ gets wrong is the **message** — a power-on reset is logged
  as "not connected". Pinned by `div7_every_ds18b20_fault_is_rejected_by_the_cpp`.
- **`TempSensorTSIC::validTemps` is a function-local `static` inside a `const` member
  function** — process-global, never reset, shared between instances. Per-instance state
  in the port.
- **The C++'s TSIC no-signal timeout is 100 ms against a 10 Hz sensor**, i.e. one
  transmission period with zero margin. The port uses 250 ms.
- **`temp >= 180` and `temp <= 0.0` are both effectively dead bounds on a TSIC-306**:
  the 11-bit span ends at 150 °C, and 0.00 °C is off the grid. First victim is DS 511.
- **The TSIC change-rate constant is used in two different units in two adjacent C++
  files**: `ZACwire.cpp:58` compares it against a gradient in raw counts, while
  `TempSensorTSIC.cpp:39` compares it against degrees. Unresolved; the port applies
  degrees and exposes `COUNT_SCALE`.

## 22. 🔴 Not a C++ finding: an FPU instruction in a level-1 ISR panics the original ESP32

Found 2026-09-28 while diagnosing the R1-07 heater panic. **It is a hardware
constraint of the original ESP32, not a bug, and it constrains every ISR this
firmware will ever write** — not just the heater's.

### What it is

The original ESP32's FPU is coprocessor 0 (`XCHAL_CP_MASK 0x01`,
`core-isa.h:122` `XCHAL_HAVE_FP 1`; `SOC_CPU_HAS_FPU 1`). Xtensa does not
save coprocessor state in an interrupt: `xtensa_vectors.S:1005-1046`
(`_xt_coproc_exc`) exists to *move* the FP save area between **threads** on a
coprocessor exception, and an ISR has no thread save area to hand. It calls
`XT_RTOS_CP_STATE`, and on a null save area jumps to `.L_xt_coproc_invalid`
(`xtensa_vectors.S:1198-1201`), which writes `PANIC_RSN_COPROCEXCEPTION` — **4** —
into `EXCCAUSE` and panics.

So the boot log's `Coprocessor exception` is **not** a mislabel, even though
ESP-IDF's `panic_arch.c:230` `reason[]` table claims `exccause 4` is
`"Level1Interrupt"`. `4` is reached twice by two different mechanisms and the
panic handler prints the wrong one; the vector table writes it deliberately.

`CONFIG_FREERTOS_FPU_IN_ISR` (`freertos/Kconfig:462`) relaxes this — **default
`n`**, and it is off in this build's `sdkconfig.h`. It is the sanctioned
workaround and is *not* used: it costs an FP save/restore per ISR entry and
buys nothing, since the correct answer is not to emit FP at all.

### Why the C++ never hit it

`isr.h:96` compares `const double currentPidOutput <= unsigned int
currentCounter` — floating point in an ISR, exactly the hazard. It is safe
there only because **GCC lowers it to soft-float** (`__ledf2`-style library
calls), not to an FPU instruction: `xtensa-esp32-elf-gcc` targets the ESP32
without hardware FP enabled. **LLVM does not.** The `esp` toolchain's
`xtensa-esp32-espidf` target advertises `target_feature="fp"`, so the same
expression becomes real `ufloat.s` / `ult.s` instructions instead of library
calls — five of them inside the heater ISR, the first of which is the faulting
PC in the boot log.

### The rule

**No floating point in an ISR, on this chip.** The trap is that it is invisible
in review: the Rust source reads as integer arithmetic, and only the emitted
instruction is a coprocessor op. `AtomicChopper::tick` did
`self.duty_ms() as f32` then `chopper_tick_level`, and the panic PC landed
exactly on the `ufloat.s`.

Both operands were already `u32` and at most `WINDOW_MS` (1000), so the `f32`
round trip was lossless and provably redundant. Fixed by
`heater::chopper_tick_level_ms`, an integer compare, proved equivalent to the
`f32` reference across the **whole** input space (1001 duties × 103 counters) by
`an_integer_level_matches_the_f32_reference_for_every_duty_and_counter`.

**How to check a new ISR** — disassemble it and grep. There is **no
compile-time warning** for this, and the naive `.s`-suffix grep is not good
enough: it also matches `divn.s`, `un.s` and `moveqz.s`, which are *integer*
instructions that merely end in `.s`. This whitelist is the FP set (any hit
inside an ISR body is a bug):

```sh
xtensa-esp32-elf-objdump -d firmware.elf \
  | grep -E '\t(ufloat|lfloat|abs|add|sub|mul|div|neg|sqrt|madd|msub|float|movf|round|trunc|utrunc|movt|ceil|floor|quos|ueq|une|ult|ule|ugt|uge|oeq|one|olt|ole|ogt|oge)\.[sd]\b'
```

The remaining hits are all in task-context code (PID, the f64 `on_fraction` log
line, formatting) and are fine. The number to watch is the ISR one, and it is
**zero**: `grep`ping the heater's `AlarmEventData` callback body finds no FP
instruction at all. The pre-fix callback had five, the first of which — the
`ufloat.s` at the entry — was the faulting PC in the boot log.

---

## 23. 🔴 The entire scale stack is unreachable — it is never constructed, and its weight is never read

**Found 2026-09-29**, while re-scoping what had been planned as "drop the dead scale
code". It is not merely unused: the feature is *structurally* absent while the
surrounding code pretends it exists.

**The scale is never created.** `HardwareContext::setScale`
(`include/clevercoffee/context/HardwareContext.h:143`) is the only way a `Scale` gets
into the context, and **nothing in `src/` or `include/` ever calls it.** Consequently
`scale_` (`HardwareContext.h:340`) is always `nullptr`, so:

- `HardwareManager::getScale()` (`:641`, `:647`) always returns `nullptr`.
- `MachineStateContext::getScale()` (`src/state/MachineStateContext.cpp:100`) always
  returns `nullptr`.
- `DisplayWidgets.h:324` does `if (systemContext.hardwareContext().scalePtr())` and
  `:325` dereferences it. **The null check is always false**, so the guarded block
  never runs — the display can never show a scale's connection state.

**The weight is never produced or consumed.** There is no `setScaleWeight` /
`setWeight` anywhere in the tree, and no caller of `getScale()` outside the
`HardwareManager`/`MachineStateContext` accessors themselves. So no weight is ever
placed into `SystemContext`, never reaches the display, and never reaches MQTT.

**The API surface exists and silently does nothing.** MQTT and the web server both
expose scale commands against the `SensorCoordinator` —
`MQTTManager.cpp:309` (`setScaleTareMode`), `:317` (`setScaleCalibrationMode`),
`WebServerManager.cpp:540`, `:563`, and `SystemContext.cpp:159`, `:163`. Those set
**flags on a coordinator that has no scale registered.** The commands are accepted,
return success, and have no effect on any hardware. A user following the web UI's
calibration flow gets no error and no measurement.

**Why it is dead, in the source's own words.** `src/main.cpp:145-150`:

```cpp
if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
    CleverCoffee::SensorCoordinator* sensorCoord = &...sensorCoordinator();
    // Scale initialization will be handled via SensorCoordinator when Scale implements ISensor
    logMemoryBasic("Scale sensor support via SensorCoordinator");
}
```

The guard exists and reads the config, but the body is a comment describing future work.
`scale` does not implement `ISensor`. The variable `sensorCoord` is bound and never
used. **The log line claims scale support is present.** The correct port is R3-17/R3-18,
which implement the drivers properly rather than replicating this.

**Severity note.** This is recorded as a C++ finding rather than a parity gap precisely
because the human who owns the hardware has confirmed the deadness is **their** bug, not
a decision to drop the feature ([06 open decisions](./06-migration-task-list.md)). Rust is
expected to make the scale work; there is no C++ behaviour to match, so R3-17 and R3-18
have no parity baseline and are new functionality.

### R3-17 outcome (HX711), measured 2026-09-29

Implemented. The driver is constructed at boot, sampled on a dedicated priority-6
FreeRTOS task, and the weight reaches `/api/status`, the SSE stream and the MQTT
registry. `hardware.sensors.scale.*` now selects cells, averaging, the rate and the
calibration target instead of describing a feature that does not exist.

**No scale is fitted.** GPIO32/25/33 are unconnected, so no weight can be measured and
**none is claimed**. What is proven on hardware:

| | result |
| --- | --- |
| driver initialises | boot log: `pins configured — data 32=high, data 25=high, clock 33 low=true; rate Gain128 = gain 128, 10 SPS, 25 clocks per read; 2 cell(s)` |
| idle bus levels | DOUT high on both lines (the internal pull-up), SCK low — the datasheet's power-up state |
| **timeout/fault path** | `E (991) scale: DOUT has been high for more than 100 ms — the cell is not answering` — **120 ms after the driver started at 871 ms**, i.e. one `SIGNAL_TIMEOUT` |
| no hang, no crash, no reset | the machine ran the full capture; the control task kept beating and the watchdog stayed fed |
| actuators | `pin readback OK: heater=GPIO2 … valve=GPIO17 pump=GPIO27 all inactive`, unchanged, and the scale pins are two inputs and one clock |

**The first build did not fault, and that is the interesting part.** An earlier revision
armed `SignalWatchdog` on the *first conversion* rather than at driver start, reasoning
that "a cell that has never spoken has not yet been late". On hardware that is visibly
wrong: with no scale, `note_ready` is never called, `is_faulted` is permanently `false`,
and the machine reports a healthy scale forever while measuring nothing — §23's exact
defect, reproduced in new code. The C++ avoids it only by accident (`HX711_ADC.cpp:129`
sets `lastDoutLowTime = millis()` before its first `update`). Rust now arms from driver
start and `a_cell_that_never_converts_is_faulted_from_the_moment_the_driver_starts` is
the host test that says so.

**Not proven:** the tick-timing comparison. The in-firmware instrument was measuring
across the tick's 400 ms sleep rather than across its work, so it reported ~431 ms for
every tick. Fixed in the tree; **not re-flashed**, because the flash budget was two and
both were spent. See the closing procedure in the R3-17 report.

**The spin loops are a second, independent defect** in the same files:
`HX711Scale.cpp:44` and `:51` spin unbounded (01 §5 lists them as `unbounded` latency).
R3-17 gives them real timeouts rather than copying them.

**And the timeout they *do* have is itself broken.** `HX711_ADC.cpp:135` reads

```cpp
static unsigned long timeout = millis() + tareTimeOut;
```

A `static` local, initialised on the first call and never reset. The deadline therefore
belongs to *when the function was first reached*, not to the call — and
`HX711Scale.cpp:53` and `:57` call `startMultiple` a **second** time for the dual-cell
case, inheriting a deadline that may already be in the past. A two-cell scale could fail
its start-up on its first iteration for a reason that has nothing to do with the scale.

---

## 24. 🟡 Found in Rust, not C++: the control tick overruns its own budget

**Found 2026-09-29** while establishing R3-17's "the control tick is measurably
unaffected" acceptance criterion. It is not a C++ finding, and it is recorded here because
this file is where the findings that shape the remaining work live.

The tick is 400 ms of period with a **10 ms work budget** (`TICK_BUDGET_MS`). Measured
over 138 ticks on hardware, with the tick's own cost timed *before* the sleep:

```
control tick: worst 32 ms of the last 138 (baseline 32 ms over the first 25,
              budget 10 ms, 86 over budget)
```

**86 of 138 ticks — 62 % — exceed the budget, and the worst is 3.2x over.**

**It is not the scale.** A control build with the sampling task disabled:

```
control tick: worst 32 ms of the last 136 (baseline 32 ms over the first 25,
              budget 10 ms, 111 over budget)
```

Identical worst, and the overrun is *more* frequent without the sampler. So R3-17's
acceptance criterion **passes** — the scale costs the tick nothing measurable — and the
overrun is pre-existing in the tick's own work.

**Why it was invisible until now.** The first version of the instrument took its timestamp
*after* `delay_ms(CONTROL_TICK_MS)` and so reported ~431 ms for every tick. That is
obviously wrong (the period is 400 ms), but the fix is the point worth recording: **a
timing instrument that has never disagreed with a result is not known to be working.**
The instrument was wrong in the direction that would have hidden a real overrun only if
someone read past the 431.

**Consequence for R4-01b.** That task's acceptance is "worst-case tick ≤ 5 ms, mean ≤ 2 ms,
zero ticks > 10 ms, compared against the C++ histogram recorded at R0-04". The
**zero-ticks-over-10-ms** half is currently failed by the Rust tick on its own. R4-01b
must either find and fix the cost, or record against the criterion that the C++ baseline
also overruns — which is checkable, because R0-04 recorded the C++ per-iteration histogram.
Do not "fix" this by relaxing `TICK_BUDGET_MS`.

## 25. 🟡 `POST /api/parameters` cannot fail on a scalar, so a mistyped value is saved as zero

**Found 2026-09-30** while porting R3-14's parameter writer. Recorded because the
response *shape* is reproduced exactly and the *arithmetic* deliberately is not, and a
later reader who has not read this will "fix" the difference back.

`ParamDef<T>::fromString` (`Config.h:242-262`) converts and returns; it cannot report a
conversion failure, because the conversions it uses cannot fail:

```cpp
newValue = value.equalsIgnoreCase("true") || value == "1";  // bool:  EVERYTHING else is false
newValue = value.toInt();                                    // int:   "12abc" -> 12, "abc" -> 0
newValue = value.toDouble();                                 // double: likewise, 0.0
```

The only validation is `isValid` (`:190-200`), which is a range check on the numeric
kinds and unconditionally `true` for `bool` and `String`. So:

| request | C++ result |
| --- | --- |
| `?pid.regular.kp=hello` | writes **0**, answers `200 {"success":true}` if 0 is in range |
| `?pid.enabled=yes` | writes **false**, answers `200` |
| `?brew.setpoint=95x` | writes 95 (`toInt`/`toDouble` stop at the junk) |
| `?standby.time=9999` | `400` — the range check caught it |
| `?no.such.parameter=1` | `400` — `findConfigParameter` returned `nullptr` |

Three of those five are a **silent wrong write reported as success**, on a parameter
that steers a PID integrator. `Arduino::String::toInt` has no failure channel, so the
C++ cannot do better without changing its own signature.

**What this port does.** `cc_config::assign::parse` requires the whole field to be the
number and rejects `NaN`/`inf` explicitly (`f64::parse` accepts them and every
comparison against `NaN` is false, so a range check waves `NaN` through), and answers
`400` — the same status the C++ gives for the two cases it *can* detect. The response
bodies and status codes are the C++'s, verbatim, for all five rows above except that
rows 1–3 are `400` here.

**Also not reproduced:** `EnumParamDef::fromString` (`:437-455`) falls back to matching
the option's **label** (`?brew.mode=Automatic`). `cc_config::ParamSpec` carries no label
table, so a label is `WrongType` and the integer discriminant is the write. Every
enumeration is a `u8` discriminant on the wire (`Config.h:212`), which is also what
`/api/parameters` reports as the current `value`, so a client that reads before it
writes never needs the label.

## 26. 🟡 A parameter write is one NVS key per parameter, so a power cut leaves a half-changed machine

**Found 2026-09-30**, same task. Not a defect — a design consequence with a safety
shape worth naming.

`ParamDef<T>::set` (`Config.h:156-180`) saves **inside the setter**, one
`Preferences::putX` per parameter, and a `POST /api/parameters` with six parameters is
six transactions (`WebServerManager.cpp:841-844` calls `fromString` per pair). A power
cut between the second and the third leaves a configuration where two values are new
and ninety-six are old. For a machine that heats to 150 °C, "some new" is a
configuration nobody ever chose.

This port's store holds **one blob** (`cc_config::store`), so a request is one write:
either all of it is durable or none of it is. That is the reason the C++'s
per-parameter persistence has no counterpart here rather than a thing that was
simplified away — and it is why `POST /api/parameters` here answers `200` *before* the
write rather than after it, with a store failure reported in the log
(`config: the parameters were applied but NOT persisted`) instead of being folded into
the HTTP status the way `set`'s `false` return is in the C++ (`:171-172`).

## 27. 🟡 `POST /api/pid` and `/api/steam` are toggles that read no parameter at all

**Found 2026-09-30**, while checking whether the Rust firmware's `?on=0` 400 was a
parity gap. It is a gap in the opposite direction, and the C++'s own documentation
disagrees with the C++.

`WebServerManager.cpp:462-479` (`/api/pid`) and `:437-459` (`/api/steam`) take **no**
request field. Both compute `!current` and set it:

```cpp
const bool newPidState = !Config::getInstance().pidEnabled.get();
```

So `POST /api/pid?on=0` in the C++ **toggles** and ignores `on=0`; there is no C++
behaviour for a query string to match. `docs/api/openapi.yaml:55-67` documents a JSON
body with `enabled: boolean`, which the handler also never reads — so the spec, the
handler and every client that has ever used it disagree three ways.

**What this port does.** `/api/pid` and `/api/steam` keep the Rust firmware's
*explicit* value (`?on=0` / `?on=1` / a form body, body first) and now accept the query
string as well as the body, because `?on=0` is what the integration checklist and the
UI's own button send. A request with **no** value is still a `400` rather than a
toggle: a toggle on a retried POST is not idempotent, and `202 Accepted` for a command
that may be applied twice is a claim the transport cannot make. Both are recorded here
because the deviation is deliberate and the C++'s own answer is "it depends which
document you read".

---

## 25. 🔴🔴 A request to sleep is silently dropped whenever the PID is off

**Found 2026-09-30**, by the human driving the web UI. `POST /api/sleep` returned
`202 {"accepted":true}`, the log showed `control: command Sleep` — so the request
reached the machine — and the state never left `PID_DISABLED`. Ever.

**Two C++ defects compose into one unreachable feature:**

**1. `PidDisabledState::update` destroys the request before it can be read.**
`PidStates.cpp:122-133` runs *before* `checkTransitions` (`StateMachine.cpp:84`)
and calls `clearAllActionRequests()`, which clears `requestStandby_` along with the
ten action flags (`MachineStateContext.h:615-627`, line 626).

**2. `PidDisabledState::checkSpecificTransitions` never looks at it anyway.**
`PidStates.cpp:135-148` checks `isPidRuntimeEnabled()` and the standby **timer**,
and nothing else. Across all of `src/`, only two states consult
`isStandbyRequested()`: `PidNormalState` (`PidStates.cpp:85`) and
`EepromErrorState` (`ErrorStates.cpp:78`).

Either defect alone would be survivable — fix the drain, or add the check. Together
they mean `PID_DISABLED` can only be left on a **timeout**.

**Why it is a defect and not a design.** `pid.enabled` defaults to `false`, so this
is the out-of-the-box configuration and the machine's own web interface cannot put
it to sleep. `PidNormalState` — the sibling state, one line away — does honour the
request, which is the strongest available evidence that the omission is an oversight.
And `requestStandby_` is not an *action* request: it asks the machine to go
somewhere rather than start doing something, so it is outside the purpose of the
drain, whose whole point is that a stale action must not fire the moment the PID is
re-enabled (S11).

**Closed 2026-09-30** at the human's decision. Two changes, both in `cc-machine`:
`Requests::clear_all` spares `standby`, and `PidDisabled`'s transition check
honours it. See `intentional-diffs.md` §13 and the `div13_*` pins in
`crates/cc-machine/tests/parity_findings.rs`.

**Verified on hardware**, with the PID off throughout: `POST /api/sleep` →
`isStandby=true`, state 95; `POST /api/wake` → `isStandby=false`, state 20
(`PidNormal`). The panel blanks as it should — `display: blanked=true frames=7`,
seven frames drawn and then the 100 ms gate correctly stops writing to a blanked
panel.

## 28. 🔴🔴 A second task may not own the DS18B20: `interrupt::free` is a global cross-core critical section

**Found** 2026-10-01, on hardware, while porting the control loop to the 100 Hz
cadence 04 §2 specifies.

**The claim.** The 1-Wire bit-bang must stay on the same task that owns the rest
of the control loop. Moving it to a task of its own — which is what a
cadence-decoupled sensor task is — asserts inside the FreeRTOS kernel on every
boot.

**The evidence.** One variable at a time, flashed and read off the serial log:

| sensor task | display task | crashes per boot |
| --- | --- | --- |
| not started | not started | **0** |
| started, DS18B20 poll disabled | not started | **0** |
| started, DS18B20 poll enabled (100 Hz, 50 Hz, 400 Hz) | not started | 3–4 |
| not started | started | **0** |
| started | started | 3–4 |

Every crash is one of two, both from the first sensor publish:

```
assert failed: xTaskRemoveFromEventList tasks.c:3894 (pxUnblockedTCB)
Guru Meditation Error: Core  1 panic'ed (LoadProhibited). Exception was unhandled.
```

The second one lands in `sys_arch_mbox_fetch` inside the **lwIP tcpip thread** —
a task with nothing to do with this firmware, corrupted from outside. The
frequency does not matter, which rules out a timing or CPU-budget explanation and
leaves the *presence of a second task touching the probe* as the variable.

**The mechanism, as far as the evidence goes.** The DS18B20 is the only thing in
the firmware that calls `esp_idf_hal::interrupt::free`, and on the original ESP32
that is `vPortEnterCritical` on a **process-global** `IsrCriticalSection`
(`esp-idf-hal-0.47.0/src/interrupt.rs`: `pub(crate) static CS`). esp-idf-hal's own
comment on it says what happens when a second task reaches it from the other core:

> the second core will then spinlock (busy-wait) in `IsrCriticalSection::enter`,
> until the first CPU releases the critical section

1-Wire enters and leaves that critical section 80-odd times per scratchpad read,
for 3–65 µs each (`cc_domain::onewire::timing`, and the C++'s own numbers). On
one task that is unremarkable — it is precisely what the C++ does with
`noInterrupts()`. On a second task it is a spinlock the `FreeRTOS` port also
expects to be able to reschedule through, and this build asserts.

**What was done about it.** The sensor task was removed and the probe left on the
control task, where it has run without incident. The **display** task was in the
same bisect and was clean in every combination, so it stayed: that is the half of
the split that fixes the reported latency. A queue-based wake, a `std::sync::Mutex`
hand-off and an ESP-IDF task notification were each tried as the way for a
producer to shorten the control loop's sleep, and **each one asserts the same
way**; the loop therefore runs on its 10 ms deadline and consumes every event at
the top of the next period. The evidence is kept in
`crates/cc-firmware/src/sensor_task.rs`, which is now a note about why the sensor
task does not exist.

**Open.** Whether this is a defect in `esp-idf-hal`, in ESP-IDF v5.5.5's
non-SMP `FreeRTOS` port, or in the way the two interact is **not established**
here, and the firmware should not be changed on a guess. The C++ firmware is
unaffected: Arduino-ESP32 runs one loop task and never enters that critical
section from a second one.
