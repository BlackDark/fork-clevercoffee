---
title: Flow rate predictive brew-by-weight stop
upstream_pr: 620
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/620
category: maybe
advise: defer
stake: "Naive measured-weight stop already matches upstream-before-PR; one-machine LS trickle coefficients are not a default until scale users report overshoot — then prefer brew.by_weight.stop_offset."
effort: S
risk: medium
priority: 7
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

# Flow rate predictive brew-by-weight stop (PR 620)

## Verdict

**Defer.** Do not port the least-squares predictor or the empirical trickle formula.

A **user-tunable static offset** (`brew.by_weight.stop_offset`) is the better first feature if overshoot is real. Predictive LS is more code, needs a filled sample window during brew, and the published coefficients (`0.83743 * flowRate + 0.36623`, cap 0–3 g) are a fit to **one** machine, valve, pump, and scale. Wrong coefficients undershoot (stop too soon) or still overshoot.

Fork already stops on measured weight with no offset — the same naive rule upstream had before this PR. There is no in-repo evidence that scale users overshoot. Complexity is not justified yet.

If taken later: math in a small **pure function** (native-testable), not in a brew state class. Default offset `0` so behavior is unchanged until someone opts in.

## Upstream

Open PR: [rancilio-pid/clevercoffee#620](https://github.com/rancilio-pid/clevercoffee/pull/620) (`scottcondie24`). ~95 / −13. Not merged.

Intent: from scale samples, estimate flow (g/s) with least squares; stop brew **early** so post-stop trickle lands on target. MQTT `flowRate`. Author later **dropped fullscreen OLED flow** (still MQTT at 5 s idle / 500 ms during brew). Also skips MQTT/website/display for one cycle on `kBrewFinished` and blocks display near the predicted end (`blockDisplayRefresh`).

### Stop condition (`src/brewHandler.h`)

Inside brew-running, with scale enabled:

```
weightOffset        = constrain(0.83743 * flowRate + 0.36623, 0, 3)
predictedBrewWeight = currBrewWeight + flowRate * (millis() - lastWeightTime) / 1000
blockWeight         = flowRate * 0.035   // display-block only, ~35 ms of mass
stop if predictedBrewWeight >= (targetBrewWeight - weightOffset)
```

Two compensations stacked:

1. **Scale lag** — extrapolate last sample forward to “now”.
2. **Post-stop trickle** — subtract a flow-linear offset (~0.37 g at 0 g/s, ~1.6 g at 1.5 g/s, 3 g cap at ~3.15 g/s).

Previous stop was `currBrewWeight > targetBrewWeight` (no offset). PR also deletes unused `scaleDelayValue = 2.5` in `scaleHandler.h`.

### Flow estimator (`src/scaleHandler.h` `updateFlow`)

- Ring of **N = 10** weight/time deltas; min 150 ms between samples (~1.5 s window).
- Reconstruct cumulative time, ordinary least-squares slope, clamp `FLOW_RATE_MIN/MAX` (0–15 g/s).
- EMA smooth: `alpha = 0.3`.
- Called from `getScaleWeight()` on every successful scale update (idle and brew).
- **Ring is not reset on brew start** — leftover idle/previous-shot samples bias the slope until 10 new points.

### Drive-bys (do not port)

| Change | Why skip |
|--------|----------|
| Fullscreen / OLED flow | Author removed it; ModernTemplate already shows weight |
| `blockDisplayRefresh` + skip long tasks on `kBrewFinished` | Cooperative-loop timing hack; fork `LoopManager` is a different scheduler |
| `TARGET_BREW_WEIGHT_MIN` 0 → 10 | Unrelated; fork uses `enabled` + `targetWeight > 0` |
| `POST_BREW_TIMER_DURATION` 3 → 6 s | Unrelated; fork uses `FINISHED_DISPLAY_TIMEOUT_MS` (3000) |
| HA `GenerateNumberDevice("flowRate")` | Writable setpoint for a read-only rate — should be a sensor if added |
| `VERSION.txt` bump | N/A |

## Current fork

Brew is a state machine (`BrewHandler` + brew states), not `brewHandler.h`. Scale is `SensorCoordinator` over `BluetoothScale` / `HX711Scale`. Weight is already on ModernTemplate (and Scale/Upright).

### How we stop on weight today

Exact site: the **weight branch** of `BrewRunningState::checkSpecificTransitions` in `src/state/states/BrewStates.cpp` (same function also handles brew-stop request and brew-by-time; those run first). Preinfusion/pause do not evaluate weight.

Automatic mode only; `brew.by_weight.enabled`; then:

```
currentWeight = context.getCurrentBrewWeight()   // SensorCoordinator brew delta
targetWeight  = brew.by_weight.target_weight     // Config: brewByWeightTargetWeight
stop → BREW_FINISHED if targetWeight > 0 && currentWeight >= targetWeight
```

No prediction, no offset, no flow. Same naive measured-weight rule as upstream **before** 620, except fork already uses `>=` (upstream-before used `>`). A later offset/LS change belongs only in this weight branch — do not invent a second stop site.

Weight path:

- `LoopManager::updateBrewWeight` starts tracking on any brew state except `BREW_FINISHED`, stops otherwise (`src/core/LoopManager.cpp`).
- `SensorCoordinator::updateScale` sets `cachedBrewWeight_ = cachedWeight_ - preBrewWeight_` while tracking.
- `MachineStateContext::getCurrentBrewWeight()` forwards `sensorCoordinator().getBrewWeight()`.
- Scale poll: `SCALE_SENSOR_INTERVAL_MS` = 100 ms (10 Hz). BLE `tryGetValue` is non-blocking; Acaia packets are bursty and can lag hundreds of ms.

Preinfusion/pause can put water on the cup; **weight stop is only evaluated in `BREW_RUNNING`**, not preinfusion (same as upstream `kBrew`).

Config today (`include/clevercoffee/Config.h`, `CONFIG_REFERENCE.md`):

- `brew.by_weight.enabled` (default false)
- `brew.by_weight.target_weight` (default 36, 0–500)
- `brew.by_weight.auto_tare`

MQTT: `currReadingWeight`, `currBrewWeight`, writable `targetBrewWeight`. No `flowRate`.

`brewByWeightFallbackActive` exists on `SensorState` but is unused.

### Hardware on the stop path (do not change)

- `BrewRunningState::onExitImpl` already `disablePump()` + `closeWaterValve()`. Predictive stop only returns `BREW_FINISHED` earlier; same exit.
- `BrewHandler::valveSafetyShutdownCheck` already allows valve open for brew states except `BREW_FINISHED`, plus manual flush and active backflush filling. No new water-flow state.
- `update()` re-asserts pump/valve while running. Never poke relays.
- `BaseState::checkTransitions` PID-disable list needs no change.

## Need it?

**Not by default.** Overshoot after pump/valve off is real physics (relay latency, 3-way dump, group volume, scale lag). Magnitude depends on **this** machine:

- 3-way vs 2-way valve
- vibe vs rotary pump
- HX711 (fast, noisy) vs BLE (slow, jittery)
- puck resistance / flow (ristretto vs turbo)

A linear fit from one author’s shots will not transfer. Typical 1.5 g/s espresso → ~1.6 g early stop; a tight 0.8 g/s shot → ~1.0 g; a gusher → 3 g cap. Easy to **undershoot** if trickle is smaller than the fit, or still overshoot if BLE lag dominates.

LS needs ~1.5 s of in-brew samples and a well-conditioned window. Short shots, preinfusion→running flow steps, and BLE dropouts yield `flowRate ≈ 0` → offset collapses to the intercept (0.37 g) and prediction does nothing useful. Idle ring contamination (upstream bug) makes this worse.

**Simpler fix that covers most reports:** stop when `measured >= target - stop_offset`. User dials 0.5–2 g on their bench. No sample window, no EMA, no machine-specific slope. Works on the first gram of the shot.

Predictive LS is only worth it if **the same machine overshoots a lot at high flow and little at low flow**, and users will not retune a static offset per style. That is a niche we have not heard.

### Trigger to reopen (in order)

1. Scale user reports **systematic** final weight **above** target (not one noisy shot). Capture: scale type (HX711/BLE), target, stopped weight, settled weight, shot time.
2. Ship **`brew.by_weight.stop_offset` only** (default `0`, range 0–5 g). Keep current stop otherwise.
3. Add LS **only if** offset users still miss in a **flow-dependent** way after tuning.

## Plan

This section is the **later** implementation path. Do not start it while status is `planned` / advise is `defer`.

### Path A — static offset (preferred if trigger #1 fires). Effort S

TDD, then wire config into the existing comparison.

1. **Red:** native tests for a pure stop decision. Header-only, same shape as `include/clevercoffee/maintenance/BackflushReminderLogic.h` / `test/test_backflush_mode`.
   - `shouldStopBrewByWeight(measured, target, stopOffset)` → true iff `target > 0` and `measured >= target - stopOffset`.
   - Clamp or reject `stopOffset` so it cannot exceed `target` (`target=1`, `offset=5`, `measured=0` must not stop).
   - Table: offset 0 (today), 1 g, target 0 never stops, measured below threshold stays, exact equality stops.
2. **Green:** implement the function. Weight branch of `BrewRunningState::checkSpecificTransitions` calls it with a new `ParamDef<double> brewByWeightStopOffset{"brew.by_weight.stop_offset", ...}` (default `0`, min 0, max 5) next to `brewByWeightTargetWeight`. Register in `Config.cpp`. Document in `CONFIG_REFERENCE.md` and `docs/example_config.json`.
3. **Do not** add flow, MQTT, display, or `TARGET_BREW_WEIGHT_MIN` changes.
4. **Completion:** `pio run --target format -e esp32_usb -s`; `pio run -e esp32_usb -s`; `pio test -e native_test`. Existing brew hardware tests still expect pump/valve off in `BREW_RUNNING::onExitImpl`.

### Path B — predictive LS (only after Path A is insufficient). Effort M

Keep Path A offset as the **named, configurable** trickle term. Replace the magic line with config (or documented constants `trickle_slope`, `trickle_intercept`, `trickle_cap`) — never ship unnamed 0.83743.

1. **Red:** `test/test_brew_weight_predictor/test_main.cpp` (or extend Path A tests).
   - LS slope on a synthetic constant-flow series (known g/s).
   - EMA / clamp / insufficient samples (`flowRate` stays 0, no stop from prediction).
   - Stop decision table: `(flow, remaining, offset, dt_since_sample)` → stop / continue.
   - Ring **reset** between shots: leftover samples must not produce a false high slope.
   - Jittered BLE-like series: must not stop many grams early.
2. **Green:** pure types in `include/clevercoffee/control/BrewWeightPredictor.h` (name TBD): `pushSample(weight, timeMs)`, `reset()`, `flowRate()`, `predictedWeight(nowMs)`, `shouldStop(...)`. No Arduino, no state class, no hardware.
3. **Feed:** `SensorCoordinator` pushes samples **only while brew-weight tracking is active**; `reset()` in `startBrewWeightTracking()`. Do not estimate during idle.
4. **Stop:** `BrewRunningState` reads predictor + config; still only `return BREW_FINISHED`. No relay calls.
5. **MQTT (optional, last):** read-only `registerSensor("flowRate", ...)` + `generateSensorDevice`, not `generateNumberDevice`. Idle interval is fine; no OLED flow.
6. **Completion:** same format/build/native as Path A, plus predictor tests. Manual: HX711 and BLE, target 36 g, record stopped vs settled weight for slow and fast shots.

### Completion checks (either path)

- [ ] Stop decision covered by native table tests.
- [ ] Default config preserves today’s `measured >= target` behavior (`stop_offset = 0`, or predictor disabled until window is valid).
- [ ] `BrewRunningState::onExitImpl` still disables pump and valve; `valveSafetyShutdownCheck` whitelist unchanged.
- [ ] No OLED flow; ModernTemplate weight footer/bar untouched unless a bug appears.
- [ ] No `blockDisplayRefresh` / loop-skip port.
- [ ] Format, `esp32_usb` build, `native_test` all pass.

## Do not copy

- Magic `0.83743` / `0.36623` / `0.035` / `N=10` / `alpha=0.3` / `denom > 0.1` as unnamed literals in a state class.
- Global `w[N]`, `t[N]` in a scale handler; uncleared ring across brews.
- Fullscreen or footer **flow rate** on 128×64 (author dropped it; we already show weight).
- Display/MQTT blocking around brew end.
- HA writable `flowRate` number.
- `TARGET_BREW_WEIGHT_MIN = 10` and post-brew timer 6 s.

## Tests / verification

**Now (defer):** nothing to run beyond keeping this plan accurate.

**Path A:** `test/test_brew_weight_stop/test_main.cpp` (name TBD) — stop table only. No hardware.

**Path B:** LS + stop table:

| flow (g/s) | remaining to target (g) | offset (g) | dt since sample (s) | expect |
|------------|-------------------------|------------|---------------------|--------|
| 0 | 0.1 | 0 | 0 | continue (below target) |
| 0 | 0 | 0 | 0 | stop (measured == target) |
| 1.5 | 2.0 | 1.6 | 0 | continue (`remaining > offset`, dt 0) |
| 1.5 | 5.0 | 1.6 | 0 | continue |
| 2.0 | 0.2 | 0 | 0.2 | stop if predicted = measured + 0.4 |
| 0 (window empty) | 1.0 | (formula) | 0 | continue — no false early stop |
| 15 clamped | — | cap 3 g | — | offset never > cap |

On device (when implementing): one HX711 shot and one BLE shot at ~36 g; confirm pump/valve off on entry to `BREW_FINISHED`; settled cup vs target. If BLE jitter causes >1 g early stop, ship offset-only.

## Risks

- **Early stop too soon** — overfitted offset or noisy high flowRate. Mitigate: default offset 0; require valid window; cap offset; log measured vs predicted vs target on stop.
- **BLE scale jitter** — 10 Hz poll, packet loss, 1000 ms read timeout. LS on jitter spikes flow → large offset → undershoot. Mitigate: reset ring on brew start; ignore samples when `tryGetValue` fails; do not run estimator idle.
- **Hardware shutdown on brew exit** — stop must only transition to `BREW_FINISHED`. `onExitImpl` already shuts pump/valve. Do not add a second shutdown path or poke relays. Abort during preinfusion still relies on `valveSafetyShutdownCheck` (already documented regression).
- **Preinfusion mass** — tracking includes preinfusion; stop still only in `BREW_RUNNING`. Unchanged; do not “fix” unless a user report says otherwise.
)
