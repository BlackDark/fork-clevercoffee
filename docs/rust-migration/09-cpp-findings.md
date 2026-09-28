# C++ findings — bugs and ambiguities found while porting

Found 2026-09-28 while implementing R2-04/R2-05/R2-06. **Every one of these was
*preserved* in the Rust port, not silently fixed**, each pinned by a named parity test.
They are recorded here because they are decisions, not accidents.

Each becomes a line in [`intentional-diffs.md`](./intentional-diffs.md) (R1-08) when the
Rust behaviour intentionally diverges.

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

- Rust: preserved. Scenario D of the PID oracle pins the `NaN` behaviour.
- **Decision needed:** clamp or reject at configuration time, or document 1000 ms as a
  hard invariant of the chopper. Recommend the latter plus an assertion.

## 2. The steam valve has no safety whitelist at all

`BrewHandler::valveSafetyShutdownCheck()` (S5) gates the **water** valve on an explicit
state whitelist. `openSteamValve()` (`HardwareManager.cpp:397-400`) checks **only**
`emergencyMode_` — there is no `steamSafetyShutdownCheck` anywhere in the tree.

So the steam valve can be commanded open in any state, while the water valve cannot.

- Rust: preserved, pinned by `s5_the_steam_valve_is_not_whitelist_gated`.
- **This is a real safety gap**, not a port artifact. Recommend adding a steam whitelist
  symmetric to S5 in the Rust port and recording it as an intentional divergence.

## 3. The water valve is not gated on an empty tank

Only `enablePump` and `setPumpPressure` check `waterTankEmpty_`
(`HardwareManager.cpp:325-328, 398-406`). `openWaterValve` does not.

- Rust: preserved, pinned by `s4_empty_tank_does_not_block_the_water_valve`.

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
