---
title: Momentary power switch PID_DISABLED wake
upstream_pr: 640
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/640
category: take
advise: include
stake: "Momentary press from PID_DISABLED (web/MQTT pid off) calls powerOff instead of powerOn, so one extra press is required; toggle and powerOn already treat PID_DISABLED as off."
effort: S
risk: low
priority: 2
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

## Verdict

**Include the wake fix only.** Split the PR.

`handlePowerButtonPress` treating `PID_DISABLED` as "on" is a bug. Toggle already wakes from both off states; `powerOn()` already accepts `STANDBY || PID_DISABLED`. Momentary does not.

The second commit (button-off delays OLED blank by `TIME_TO_DISPLAY_OFF`) is a UX preference. Defer it. The fork does not instant-blank today, and the delay must go through `StandbyCoordinator`, not upstream's `millis() - timeout` wrap.

## Upstream

Two changes in `src/powerHandler.h` MOMENTARY branch (~25 lines):

1. **Wake.** `if (machineState == kStandby)` becomes `kStandby || kPidDisabled`. Web/MQTT `pidON = false` lands in `kPidDisabled` (display on, "Standby"). A press fell through to `performSafeShutdown()` / `kStandby`; second press actually woke.
2. **Display.** On button-off, keep `standbyModeRemainingTimeMillis = 0` but set display-off remaining to `TIME_TO_DISPLAY_OFF_MILLIS` and backdate `standbyModeStartTimeMillis = millis() - getStandbyTimeoutMillis()` so `updateStandbyTimer()` counts exactly 10 min. If standby is disabled, still blank now.

Toggle already checked both states. Author tested on Silvia E (momentary).

## Current fork

`include/clevercoffee/handlers/PowerHandler.h`:

- Momentary: `STANDBY` → `powerOn()`, else → `powerOff()`. **Hypothesis confirmed.**
- `powerOn()` already: `STANDBY || PID_DISABLED` → `setNormalOperationRequested(true)` + `setUserPidEnabled(true)` + `setPowerSave(0)`.
- Toggle HIGH always `powerOn()`; PID_DISABLED wake already works there.
- `powerOff()`: `performSafeShutdown()`, `setStandbyRequested(true)`, `standbyCoordinator().setRemainingTimeMillis(0)`. Does **not** zero display-off remaining. Does **not** poke relays.

`PidDisabledState` never reads `isStandbyRequested()`. `update()` drains all action flags while PID stays off. With standby enabled, remaining=0 still trips `shouldEnterStandby()`. With standby disabled, the press is a no-op: stay in `PID_DISABLED`, cannot wake.

Web `/api/pid` and MQTT `pid.enabled` → `setUserPidEnabled` / runtime PID + `requestNormalOperation` + `standbyCoordinator().reset()`. That is `PID_DISABLED` with display on ("PID is disabled manually"), not `STANDBY`. Web `/api/sleep` requests standby without zeroing timers.

Display blank: `LoopManager::updateDisplay` calls `setPowerSave(1)` only when `shouldTurnOffDisplay()` (both remaining timers 0, start time set). `StandbyState::onEntry` does not blank. Button-off in the fork already leaves the OLED on until the coupled countdown finishes. That countdown is `elapsed since last reset` vs `standbyTimeout + displayOffTimeout`, not a fresh 10 min from the press.

`test/test_power_handler/test_main.cpp` covers momentary wake from `STANDBY` and off from `PID_NORMAL`. No `PID_DISABLED` case.

## Need it?

Wake: yes. Extra press after web/MQTT pid-off is wrong, and worse if standby is disabled (no wake at all). One-line gate change; `powerOn()` already correct.

Display delay: no, not with this PR. Original upstream blanked immediately (both timers 0). Fork does not. Matching HA/web "standby screen for 10 min" is a product choice, and HA/web pid-off is not even the same path (`PID_DISABLED` + full standby timeout, then display-off). If we want a fresh display-off window after button-off, add a coordinator API later. Do not take it as a hitchhiker.

## Plan

TDD, wake only.

1. Failing test in `test/test_power_handler/test_main.cpp`: momentary press from `PID_DISABLED` (same millis/init pattern as `MomentarySwitchPowerOnFromStandby`) → `isNormalOperationRequested()`, not `isStandbyRequested()`. Optional: `pidEnabled` config true after press (`setUserPidEnabled`).
2. Fix: `handlePowerButtonPress` call `powerOn()` for `STANDBY || PID_DISABLED`. No other logic in that function.
3. Re-run existing momentary `STANDBY` / `PID_NORMAL` tests. Toggle tests unchanged.

Hardware: power switch must not poke relays. Keep going through `powerOn()` / `powerOff()` → `MachineStateContext` requests (`setNormalOperationRequested` / `setStandbyRequested`) and existing `performSafeShutdown()`.

**Deferred (not this change):** display-off after button-off. If taken later: `StandbyCoordinator` method that sets remaining standby to 0 and starts `getDisplayOffTimeoutMillis()` from **now** (independent start, or decrement display-off remaining on its own once standby remaining is 0). `powerOff()` calls that instead of only `setRemainingTimeMillis(0)`. Tests on the coordinator, not PowerHandler millis arithmetic. If standby is disabled, keep current "no display-off countdown" behavior (`shouldTurnOffDisplay` already requires start time and both zeros).

## Do not copy

- `machineState = kPidNormal` / `kStandby` assignments. Fork uses request flags + state classes.
- `setRuntimePidState(true)` on wake. Button is user intent: `setUserPidEnabled(true)` (already in `powerOn()`). Runtime-only leaves `pidEnabled` false (heater follows config; standby exit restores runtime from config).
- `standbyModeStartTimeMillis = millis() - getStandbyTimeoutMillis()` wrap. Unsigned wrap is cute; the fork timer is coupled (`standby + displayOff` from one start). Fake elapsed breaks `update()` meaning.
- Zeroing both remaining timers (pre-640 upstream instant blank).
- New globals / `TIME_TO_DISPLAY_OFF_MILLIS` in PowerHandler. Coordinator already has `getDisplayOffTimeoutMillis()`.

## Tests / verification

- `~/.platformio/penv/bin/pio test -e native_test` (at least `test_power_handler`).
- `~/.platformio/penv/bin/pio run -e esp32_usb -s` after the one-line fix.
- Bench: momentary switch, `/api/pid` off (OLED "PID is disabled manually"), one press → heat / `PID_NORMAL`. Second press → standby. Repeat with `standbyEnabled` false.
- Do not claim display-off timing until the deferred coordinator work exists.

## Risks

Low. `powerOn()` from `PID_DISABLED` turns PID config on; that matches a physical power button. `PidDisabledState::update` drains flags only while PID stays off; `setUserPidEnabled(true)` in the same press skips the drain on the next loop.

Do not add `PID_DISABLED` handling of `setStandbyRequested` as part of this fix. After the wake change, a press no longer requests standby from that state.
