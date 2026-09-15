---
title: Auto boiler refill after steam (upstream #622)
upstream_pr: 622
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/622
category: maybe
advise: defer
stake: "Silvia/Gaggia boiler water loss after steam is real; auto-refill is worth a dedicated BOILER_REFILL state later, not a fake hot-water press now."
effort: M
risk: high
priority: 8
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

# Auto boiler refill after steam (upstream #622)

## Verdict

**DEFER.** The user need is real (single-boiler machines dump boiler water as steam). The upstream shape is wrong for this fork: it fakes a hot-water switch press, pokes `pumpRelay` from `hotWaterHandler`, and drives the feature from globals in `loopPid()`.

Do **not** port. Do **not** skip. If we take it later, it is a new operational pump-on state (`BOILER_REFILL`) entered from steam exit, stopped on time or pressure, with the full ADR-0003 contract. Manual hot water in `PID_NORMAL` already covers refill until then.

## Upstream

PR [#622](https://github.com/rancilio-pid/clevercoffee/pull/622) (open, author r3vile). +254 / −102. No review comments at plan time.

Behavior:

1. While `kSteam`, track peak/min temperature and whether temp reached `steamSetpoint - 3°C`.
2. On steam-off, if enabled and `peak - min > 5°C`, set `steamAutoRefillActive`.
3. `hotWaterHandler` idle sees that flag and **forges** `kHotWaterSwitchShortPressed`, then `pumpRelay->on()`.
4. `loopPid()` clears the flag on duration (`steam.auto_refill.duration`, default 5 s, max 60 s) or pressure (`steam.auto_refill.pressure`, 0 = off, max 3 bar).
5. Handler then moves to `kHotWaterStopped` and `pumpRelay->off()`.

Config (all new):

| Key | Default | Notes |
|---|---|---|
| `steam.auto_refill.enabled` | `false` | Opt-in |
| `steam.auto_refill.duration` | 5 s (1–60) | Hard stop |
| `steam.auto_refill.pressure` | 0 bar (0–3) | Stop if sensor enabled and target > 0 |

Detection is a heuristic for an **unmonitored** steam wand. It is not a water-level sensor.

## Current fork

Class states. Pump/valve/heater only via `HardwareManager` / `MachineStateContext`. Contract: [ADR-0003](../../adr/0003-state-machine-hardware-control-contract.md), [state-machine-architecture.md](../../state-machine-architecture.md).

Relevant now:

- **Steam** (`SteamRunningState`): `onExitImpl` already `disablePump()`. `update()` enables pump only while the hot-water switch is held (water injection). Steam wand is still unmonitored.
- **Hot water**: no `HOT_WATER` state. `PidNormalState::update()` enables pump while the switch is held; `onExitImpl` disables pump. Valve stays closed (`PidNormalState` never opens it). `HotWaterHandler` logs switch activity, sets `setHotWaterActivity`, and may `disablePump()` on its 60 s `PumpTimer` — it never `enablePump()`.
- **Valve whitelist** (`BrewHandler::valveSafetyShutdownCheck`): brew (except finished), `MANUAL_FLUSH_RUNNING`, backflush filling/flushing. Closes valve otherwise. Steam and PID hot-water are **not** listed — correct, they fill/dispense with the 3-way **closed**.
- **PID-disable exclusion** (`BaseState::checkTransitions`): `INIT`, `PID_NORMAL`, `PID_DISABLED`, `STANDBY`, error/emergency. Steam / brew / flush are **not** excluded: PID off aborts them.
- **Tank empty**: BaseState forces `WATER_TANK_EMPTY`; `HardwareManager::enablePump()` refuses; `setWaterTankEmpty()` force-disables pump. `WaterTankEmptyState::onEntryImpl` does **not** disable pump — previous `onExitImpl` is the guaranteed cleanup.
- **Loop order** (`LoopManager`): sensors → `steamHandler`/`powerHandler` `process()` → `StateMachine::update()` → `hotWaterHandler.process()` → `brewHandler.process()` → `valveSafetyShutdownCheck()`. Steam-stop is flagged before the SM tick that would enter refill; do not then let `HotWaterHandler` start the pump.

There is no refill feature, no `steam.auto_refill.*` params, no `BOILER_REFILL` id.

**`MachineStateId` numbering (do not collide):**

Taken today (`include/clevercoffee/state/MachineStateIds.h`):

| Value | Id | Helper |
|---|---|---|
| 0 | `INIT` | |
| 20 | `PID_NORMAL` | |
| 31–34 | brew family | **range** `isBrewState` |
| 36 | `MANUAL_FLUSH_RUNNING` | equality `isManualFlushState` |
| 51 | `STEAM_RUNNING` | equality `isSteamState` |
| 60–63 | backflush family | **range** `isBackflushState` |
| 70 / 80 / 90 / 95 / 100 / 110 | tank, emergency, PID off, standby, sensor, eeprom | |

Range helpers are the trap. `valveSafetyShutdownCheck` opens the 3-way for `isBrewState` (except finished) and active backflush. Putting refill inside **31–34** or **60–63** would dump the group instead of filling the boiler.

Reserve **`BOILER_REFILL = 52`** (after steam, before backflush). Outside every range helper. `isSteamState` stays equality on `51` — do not widen it to cover 52.

Do **not** use: 31–34, 51, 60–63, or the unused **35** (sits between `BREW_FINISHED` and flush; a later brew-band widen swallows it). `StateFactory` unknown id is `FATAL` + `ESP.restart()` — add create + `getStateName` cases. `/api/status` `machineState` and MQTT sensor `machineState` publish the enum int; 52 is unused.

Manual refill today: hold hot-water switch in `PID_NORMAL` (or injection switch during steam). Same hydraulic path as upstream's auto pump-on.

## Need it?

**Need (product):** yes, eventually, for Silvia/Gaggia-class single boilers. Steam converts boiler water to vapor; the next brew starts from a partial boiler, long recovery, sometimes a dry-ish heat. Users already refill by running hot water / pumping until the OPV sputters.

**Need (now):** no. Not a bug. Not a safety hole. Workaround exists and is the same hardware action (pump on, 3-way closed). Auto-run after steam is convenience plus **unattended pump** — that is the risk.

Auto is only worth the state-machine surface if:

- users actually forget / skip manual refill enough to care, **and**
- we implement a real state with abort + display, **and**
- stop conditions are better than upstream's 5°C heuristic + optional 0–3 bar.

Until then, document: after steam, hold water switch until the boiler recovers (or OPV drips). Do not fake that press in software.

## Plan

### Defer — revisit when

Any of:

1. Users report post-steam recovery as a recurring pain (dry boiler, long heat-up, pump cavitation).
2. Pressure sensor is common enough in this fork that pressure-stop is the primary stop, not a 5–60 s timer dumping to the OPV.
3. ADR-0003 new-state checklist is muscle memory (recent water-flow work landed clean).
4. Upstream #622 merges **and** hardens detection (or drops the 5°C-from-peak heuristic).

Then implement as below. Not a drive-by port.

### If include later — dedicated `BOILER_REFILL`

**Hydraulics (do not get this wrong):**

| Mode | Pump | 3-way water valve | Where water goes |
|---|---|---|---|
| Brew / manual flush / backflush fill | ON | **OPEN** | Grouphead |
| PID hot water / steam injection | ON | **CLOSED** | Into boiler; excess via OPV → drip tray |
| **Boiler refill (this feature)** | ON | **CLOSED** | Same as hot water |

Refill is a **pump-on, valve-closed** operation, like hot water, **not** like flush. Blindly adding it to `valveSafetyShutdownCheck` would open the group and miss the point.

**Shape:**

1. New `MachineStateId::BOILER_REFILL = 52` (reserved above; do not reuse steam/brew ids or sit inside a range helper). `StateFactory` create + `getStateName`. Optional `isBoilerRefillState()`; do **not** fold into `isSteamState()` (display/LED would lie; `isSteamState` is equality on `51` today — keep it that way).
2. `SteamRunningState::checkSpecificTransitions`: on steam-stop, if refill enabled **and** usage predicate true → `BOILER_REFILL`, else `transitionToPidState`. Clear `steamStopRequested`.
3. Steam `onExitImpl` still `setSteamMode(false)` + `disablePump()`. Refill `onEntryImpl` turns the pump back on. Heater returns to **brew** setpoint (steam mode off). Do not keep steam PID gains/setpoint while pumping cold water.

**ADR-0003 checklist (mandatory):**

| Contract | `BOILER_REFILL` |
|---|---|
| `onEntryImpl` | `enablePump()`; `closeWaterValve()`; start elapsed timer; snapshot pressure if used. Drain flags this state cannot honor (see flags). |
| `update()` | Re-`enablePump()` every loop (tank-empty / safety may have cleared it). Keep valve closed. |
| `onExitImpl` | **Always** `disablePump()` + `closeWaterValve()`. Next `onEntry` may not run. |
| `valveSafetyShutdownCheck` | **Do not whitelist.** Valve must stay closed. Safety check closing the valve is correct. Document the exception next to the whitelist so nobody "fixes" it. |
| BaseState PID-disable exclusion | **Do not add.** PID off → `PID_DISABLED` → previous `onExitImpl` kills pump. |
| Tank empty / emergency / sensor error | Rely on BaseState. `onExitImpl` is the pump-off. `HardwareManager` still refuses `enablePump()`. |
| Flag drain | Abort on brew-start, steam-start, hot-water switch, backflush-enter, standby. Consume those flags; do not leave them for `PID_NORMAL` after a surprise refill. `PID_DISABLED` / error states already `clearAllActionRequests()`. |
| Relays | Never. Only `context.enablePump()` / `disablePump()` / `closeWaterValve()`. |
| `HotWaterHandler` | Untouched. No forged switch presses, no `hotWaterPumpIsAutoRefill`. |

**Stop conditions (all abort → PID via `transitionToPidState`):**

1. **Timeout** — hard cap. Default ~5 s. Max **much** tighter than upstream 60 s (vibration pump ~8–10 ml/s → 60 s is a flooded drip tray). Suggest max 15–20 s until measured. Timeout always wins even if pressure never hits.
2. **Pressure** — if sensor enabled and target > 0 **and** a **minimum pump time** has elapsed. After steam, residual boiler pressure can already be 1–1.5 bar; upstream 0–3 bar target with no min-time can stop **immediately**. Min-time (e.g. 1–2 s) or "pressure rose by Δ after pump start" — do not copy `inputPressureFilter >= target` raw.
3. **Tank empty / emergency / sensor error / PID off** — BaseState.
4. **User abort** — brew, steam, water switch, web/MQTT stop if we add one.

Default **off**. Opt-in config only.

**Usage predicate (do not copy 5°C-from-peak):**

Steam PID (`steam.kp` default 150) overshoots. Peak-to-min of 5°C while sitting in steam mode is a plausible **false trigger**. False negative: short wand burst that doesn't drop 5°C.

Prefer one of:

- **A (simpler):** if enabled, refill after **every** steam session. Predictable; wastes a few seconds of pump if the user only preheated. Honest.
- **B:** enabled **and** time-in-steam above a floor **and** temp drop **well above** typical steam-PID noise (measure; not 5°C from peak).

Do not ship A+B+pressure+duration as four interacting knobs. Enabled + duration + optional pressure is enough.

**Config (when implementing):**

- `steam.auto_refill.enabled` (bool, default false)
- `steam.auto_refill.duration` (s, tight max)
- `steam.auto_refill.pressure` (bar, 0 = time-only; visible only if pressure sensor enabled)

Defaults in `defaults.h` as `constexpr`, `ParamDef` in `Config.h` (steam section). No new globals in the loop.

**Display / UX:** unattended pump must be visible. OLED: reuse hot-water fullscreen **or** a dedicated "Refill" string; `shouldDisplayHotWaterTimer` / `displayFullscreenHotWaterTimer` currently only treat `PID_NORMAL` and `STEAM_RUNNING`. 128×64 fit, no clip, fixed-width timer. Steam LED off (not `isSteamState`). MQTT/API state name `Boiler Refill`.

**Heater+pump:** expected (cold tank water into a hot boiler). Keep steam mode **off** so PID is brew setpoint, not 120°C+ steam. Short duration limits dry-fire window; tank-empty still blocks the pump.

## Do not copy

- Do not fake a hot-water switch press (`kHotWaterSwitchShortPressed`, `hotWaterPumpIsAutoRefill`, `steamAutoRefillActive` polled from `loopPid()`).
- Do not call `pumpRelay->on()` / `off()` from a handler or the main loop.
- Do not skip `onExitImpl` pump/valve shutdown ("next state will handle it").
- Do not assign `BOILER_REFILL` to 31–34, 35, 51, or 60–63. Use **52**.
- Do not whitelist `BOILER_REFILL` in `valveSafetyShutdownCheck` (valve stays closed).
- Do not add `BOILER_REFILL` to the BaseState PID-disable exclusion list.
- Do not fold refill into `PidNormalState::update()` as a hidden auto-hold of the water switch.
- Do not copy the 5°C-from-peak detector or a 60 s pump cap.
- Do not keep steam setpoint/gains during refill.

## Tests / verification

Native (pattern: `test/test_state_flow_integration`, `test/test_state_classification`; spy pump/valve like brew abort tests):

1. Steam used + enabled → `STEAM_RUNNING` → `BOILER_REFILL`; `enablePump` on entry; valve not opened.
2. Pressure hit after min-time → PID; `disablePump` on exit.
3. Timeout → PID; pump off. Pressure-never-rises still stops.
4. Residual pressure already ≥ target at entry → must **not** stop at t=0 (min-time / ΔP).
5. Enabled but usage predicate false (if B) → steam stop goes to PID, pump stays off.
6. Disabled → steam stop → PID, no refill.
7. Error / tank empty / emergency / PID off **during refill** → `onExitImpl` disables pump **even if next `onEntry` is skipped** (call `onExitImpl` directly; assert `disablePump` / `closeWaterValve`).
8. `valveSafetyShutdownCheck` during refill closes valve (or leaves it closed); does **not** disable pump.
9. `enablePump` during refill with tank empty is refused; state still exits to `WATER_TANK_EMPTY`.
10. Brew/steam/water-switch during refill aborts; flag consumed.
11. Classification: `isSteamState(BOILER_REFILL) == false`; brew/flush helpers unchanged.
12. Factory: unknown id still fatal; new id constructs.

Firmware build `pio run -e esp32_usb`. Format. `pio test -e native_test`.

Manual (add to `docs/integration-tests.md` if implemented): steam, empty cup under group, drip tray empty, enable refill, exit steam → pump runs, group dry, tray only if OPV; abort with brew switch; tank-empty mid-refill.

## Risks

| Risk | Why it matters | Mitigation if we ever ship |
|---|---|---|
| Unattended pump | New water-flow feature; flood / dry-fire | Opt-in default off; short hard timeout; OLED visible; abort flags |
| ADR-0003 miss | `onExit` skip / relay poke / wrong whitelist | Checklist above; native onExit-without-onEntry test |
| Valve whitelist cargo-cult | Opening 3-way dumps group instead of filling | Explicitly **not** listed; comment at whitelist |
| Flooded drip tray | Time-only stop, 60 s cap, OPV dump | Tight max duration; pressure+min-time when sensor present |
| False trigger | 5°C peak-to-min vs steam PID overshoot | Predicate A or measured noise floor, not upstream 5°C |
| Instant pressure stop | Post-steam boiler already ~1 bar | Min pump time or ΔP |
| Heater + pump | Steam-hot boiler, cold fill, steam PID still on | Steam mode off before pump; brew setpoint |
| Handler/state split | Upstream leftover: handler owns pump, state doesn't | Handler never starts pump |
| Dry heater | Very empty boiler, heater on, slow fill | Short duration; tank sensor; thermal fuse is last resort not a plan |
