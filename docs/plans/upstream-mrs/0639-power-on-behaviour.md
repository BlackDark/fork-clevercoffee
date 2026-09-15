---
title: Configurable power-on behaviour (standby vs heat)
upstream_pr: 639
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/639
category: take
advise: include
stake: "Momentary (and Heat-on-mains) boots always enable PID, so OTA/reboot heats the boiler with nobody home."
effort: M
risk: medium
priority: 3
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

## Verdict

**Include.** Hypothesis confirmed. Not a cherry-pick.

Default **must change**: always-heat → **Standby**. That is the point of the PR and a user-visible break. Smart-plug / timer people opt into **Heat up**.

Keep upstream's 3-value enum (`pid.power_on_behaviour` 0/1/2), not a bool. Restore is **not** the same as today's `pid.enabled` on momentary: boot currently **overwrites** that flag to true. A bool cannot preserve current no-switch Restore while adding a safe default.

Toggle stays physical-switch-owned. Do not apply the enum there.

The live SM starts in `INIT`. `setCurrentStateId` only writes the context mirror; InitState never goes to `STANDBY`. Porting `machineState = kStandby` into `finalizeMachineState()` lands in `PID_DISABLED`. Use `StateMachine::initialize(target)` (real API) plus a context-ID sync — PowerHandler reads the mirror **before** LoopManager copies the live SM.

## Upstream

[PR 639](https://github.com/rancilio-pid/clevercoffee/pull/639) (open, `irrwisch1`). +56/−4. `setup()` end, momentary only:

```
machineState = kPidNormal;
setRuntimePidState(true);
```

No-switch previously followed stored `pidON`. Toggle still follows the physical switch after this block.

New `pid.power_on_behaviour` (int 0–2, **default 0**):

| Value | Label | Effect (non-toggle) |
|------:|-------|---------------------|
| 0 | Standby | `kStandby`, runtime PID off |
| 1 | Heat up | `kPidNormal`, runtime PID on, `resetStandbyTimer` |
| 2 | Restore last state | `pidON` from config → `kPidNormal` or `kStandby` |

Heat/Restore-on call `resetStandbyTimer(kPidNormal)`. Upstream static remaining time is 0 until config load; the SM treats 0 as elapsed, so Heat without reset bounced through `kStandby` and only came back via `if (pidON)`.

Toggle explicitly excluded. Author verified on Silvia E, standby 35 min, all three modes.

## Current fork

`src/core/SystemInitializer.cpp` `finalizeMachineState()`:

| Path | What it does |
|------|----------------|
| Power enabled + **momentary** | `PID_NORMAL` + **`setUserPidEnabled(true)`** (NVS write) |
| Power enabled + **toggle** | Pressed → `PID_NORMAL` + persist on; else `PID_DISABLED` + **runtime-only** off |
| Power disabled (default) | `pid.enabled` → `PID_NORMAL` or `PID_DISABLED` via `setRuntimePidState` |

`hardware.switches.power.enabled` default **false**. `pid.enabled` default **false**. Factory no-switch already stays off. The unattended-heat bug is:

1. **Momentary** — always heat, and poisons `pid.enabled=true`.
2. **No-switch after the user turned PID on** — current Restore; OTA then heats. Upstream wants that to become Standby unless they pick Heat or Restore.

**Boot path is the live SM, not the context ID.** `StateMachine::initialize(MachineStateId = INIT)` is a real API (`StateMachine.h` / `.cpp`): it constructs that state and runs `onEntry`. `setCurrentStateId` only writes `currentStateId_` (default `PID_DISABLED`). `logStateEntry` does not update it. `transitionTo` **logs and does nothing**.

Today: `initialize()` → INIT, then finalize writes the mirror. First loop `InitState` → `PID_NORMAL` if runtime PID else `PID_DISABLED`. **Never `STANDBY`.** Momentary heat "works" only because `setUserPidEnabled(true)` makes InitState choose `PID_NORMAL`.

LoopManager copies live SM → context ID **after** `PowerHandler::process()` (`updateSwitchesAndStandby` then `updateStateMachine`). First-loop momentary press reads the **mirror**. `initialize(STANDBY)` without also `setCurrentStateId(STANDBY)` leaves the mirror at `PID_DISABLED` → first press is `powerOff`.

Standby vs `PID_DISABLED` is not cosmetic:

- `PowerHandler` momentary: `STANDBY` → `powerOn()`, **else → `powerOff()`**. Boot `PID_DISABLED` → first press **shuts down** (sibling [0640](0640-momentary-power-switch.md) fixes wake; this plan must still enter `STANDBY` so it works without 640).
- `StandbyState::onExitImpl` restores runtime PID from **config**. Boot must not persist Heat/Standby over `pid.enabled` or Restore is garbage.

`PowerHandler::powerOff()` requests standby, zeros remaining time, **does not** persist `pid.enabled`. Only `setUserPidEnabled` writes NVS. Restore for momentary is a lie until power-off persists.

Standby 0-ms trap: **already guarded**. `StandbyCoordinator::shouldEnterStandby()` requires `startTime != 0`. `PidNormalState` calls `initializeStandbyTimerIfNeeded()` before the timeout check. Still `resetStandbyTimer` on Heat/Restore-on (`onEntryImpl` does **not** reset).

## Need it?

Yes. OTA / watchdog / flash with momentary (or `pid.enabled` left true) starts the heater unmanned. A bool cannot keep today's no-switch Restore while defaulting momentary to Standby. Enum 0/1/2 is worth it **if** power-off persists; otherwise drop Restore.

## Plan

TDD. Enum, not bool. Default Standby.

### 1. Enum + ParamDef (no `ParameterRegistry`)

`include/clevercoffee/defaults.h` `namespace Process`:

```cpp
enum class PowerOnBehaviour : int {
    STANDBY = 0,
    HEAT    = 1,
    RESTORE = 2
};
```

`include/clevercoffee/Config.h` next to `pidEnabled` **or** Standby section 7 order **800** (upstream Power section, sits above `standby.enabled` 801):

```cpp
EnumParamDef<Process::PowerOnBehaviour> pidPowerOnBehaviour{
    "pid.power_on_behaviour",
    Process::PowerOnBehaviour::RESTORE,  // product default — heat iff pid.enabled (match main no-switch)
    "Power-On Behaviour",
    7, 800,
    "What the machine does when it powers up (ignored when a toggle power switch is fitted). Default restores last pid.enabled: stay in standby, start heating (smart plug / timer), or restore the last PID on/off.",
    {{Process::PowerOnBehaviour::STANDBY, "Standby"},
     {Process::PowerOnBehaviour::HEAT, "Heat up"},
     {Process::PowerOnBehaviour::RESTORE, "Restore last state"}}
};
```

`src/Config.cpp` `getAllConfigParams()`: insert `&pidPowerOnBehaviour`.
`test/ConfigTestHelper.h`: `resetToDefault()`.

Do **not** add to `IConfig` / `MockConfig` unless a test actually injects it. Config singleton is enough.

Optional `showCondition`: hide when `hardwareSwitchesPowerEnabled && type == TOGGLE`. Frontend `requiredParameters` cannot express that OR; help text is enough if showCondition is annoying.

### 2. Pure resolver — this is the test seam

New header, e.g. `include/clevercoffee/core/PowerOnBehaviour.h` (no Arduino, no SM):

Inputs: power enabled, switch type, switch pressed (toggle only), `PowerOnBehaviour`, `pid.enabled`.

Outputs: `{MachineStateId state, bool runtimePid, bool resetStandbyTimer}`.

Rules:

- **Toggle + enabled** → ignore enum. Pressed: `PID_NORMAL` / pid on / reset timer. Released: `PID_DISABLED` / pid off / no reset. Unchanged.
- **Else** (momentary or no switch):
  - `STANDBY` (and default): `STANDBY`, runtime pid **false**, no persist.
  - `HEAT`: `PID_NORMAL`, runtime pid **true**, reset timer. **`setRuntimePidState` only** — do not `setUserPidEnabled`.
  - `RESTORE`: `pid.enabled` → `PID_NORMAL` (reset timer) else `STANDBY`. Runtime only.

Off boot state is **`STANDBY`**, not `PID_DISABLED`, for non-toggle. That matches upstream and the momentary press gate.

### 3. Drive the live SM with `initialize(target)` (real API)

`void StateMachine::initialize(MachineStateId initialStateId = MachineStateId::INIT)` already constructs that state and runs `onEntry`. Primary path — reorder `src/main.cpp` after SM construction (context exists in the ctor; `initialize()` is not a prerequisite for `setMachineStateContext`):

1. `setMachineStateContext(&stateMachine->getContext())`
2. `finalizeMachineState()` applies runtime PID + optional `resetStandbyTimer`, returns target. **No** `setUserPidEnabled` on Heat/Standby boot.
3. `stateMachine->initialize(target)` — `onEntry` on `STANDBY` / `PID_NORMAL` / `PID_DISABLED` (toggle off). Skipping INIT is fine: InitState only logs, then PID on → `PID_NORMAL` else `PID_DISABLED`, never `STANDBY`. `STANDBY` is already excluded from the empty-tank force in `BaseState`.
4. `setCurrentStateId(target)` — **mirror sync**, not the boot mechanism. PowerHandler runs before LoopManager copies the live SM. Without this, boot `STANDBY` still looks like `PID_DISABLED` on the first press.

Fallback only if skipping INIT is rejected: `pendingBootState` on context; `InitState::checkSpecificTransitions` returns it instead of the PID on/off pair. Still sync the context ID in finalize. Do **not** use `setCurrentStateId` as the only boot step.

`finalizeMachineState` toggle branch stays as-is (persist on when switch is ON — user intent).

### 4. Make Restore true — persist off

`PowerHandler::powerOff()`: after the standby request, `setUserPidEnabled(false)` (or persist false then runtime false). Button-off is user intent, same as `powerOn()` already persisting true.

Without this, Restore after a momentary session is Heat. Do not ship Restore until this exists.

Do **not** persist on Standby/Heat **boot**. Those are overlays on the saved flag.

### 5. Frontend

`ui/packages/frontend/src/lib/parameter-metadata.ts` after `pid.enabled` or with standby:

```
name: "pid.power_on_behaviour"
type: ENUM
min: 0, max: 2, defaultValue: 2
options: Standby / Heat up / Restore last state
```

Mock server only if it enumerates PID keys. Labels must match firmware `EnumParamDef` (API `toJson` already emits `options`).

### 6. Release note (required)

Existing flash has no NVS key → default **2 Restore** (heat iff `pid.enabled`, like main no-switch).

- Default Restore: `pid.enabled` true heats, false stays cold.
- Standby: opt out of heat on boot.
- Heat up: always heats (smart plug / timer).
- Toggle: no change.

## Do not copy

- `ParameterRegistry.cpp` / `addEnumConfigParam` / `powerOnBehaviours[]`.
- `machineState = kPidNormal` globals. Live SM via `initialize(target)` (real API) + context-ID sync; InitState `pendingBootState` only as fallback. Request flags for later user actions.
- `enum PowerOnBehaviour` in `main.cpp`.
- `ConfigDef::forInt`.
- `setUserPidEnabled(true)` on Heat boot (poisons Restore).
- `setCurrentStateId` / `transitionTo` as the only boot mechanism (`transitionTo` is a no-op).
- Applying the enum to toggle.
- Cherry-pick of upstream `resetStandbyTimer` as the **whole** fix; the fork trap is different and already guarded.

## Tests / verification

RED first on the resolver (`test/test_power_on_behaviour/` or extend `test_system_initialization` with a header-only seam). Native cannot easily construct a full `SystemInitializer`.

| Case | Expect |
|------|--------|
| Momentary + Standby | `STANDBY`, runtime pid false, `pid.enabled` **unchanged** |
| Momentary + Heat, `pid.enabled` false | `PID_NORMAL`, runtime true, config still false |
| Momentary + Restore, config true / false | `PID_NORMAL` / `STANDBY` |
| No-switch + each of 0/1/2 | same as momentary |
| Toggle ON / OFF | `PID_NORMAL` / `PID_DISABLED` for enum 0, 1, and 2 |
| Invalid/default | Standby |

Init/SM: after finalize+`initialize(target)`+mirror sync, live state **and** `getCurrentStateId()` are `STANDBY` not `PID_DISABLED` when behaviour is Standby.

PowerHandler: existing toggle tests unchanged. New: `powerOff` → `pid.enabled` false. Momentary from `STANDBY` still `powerOn`.

Config: enum round-trip 0–2; default 0.

Frontend: `pnpm test:run` / `tsc` in `ui/packages/frontend`; `pnpm lint` + `pnpm format` in `ui/` if metadata changed.

Firmware: `pio run --target format -e esp32_usb -s` then `pio test -e native_test` then `pio run -e esp32_usb -s`.

Bench:

- Momentary, default, OTA → OLED standby, **heater 0**, one press → heat.
- Heat, PID was off before reboot → heats; `pid.enabled` still false if they never pressed power.
- Restore, off via button → stays off; on via button then reboot → heats; standby timer does not immediately re-enter.
- Toggle ON/OFF with enum=0 → still follows switch.
- No-switch, Restore, `pid.enabled` true → heat (documents old default).

Add to `docs/integration-tests.md`: OTA with momentary + default Standby must not heat; Heat setting must.

## Risks

- **Default Standby (medium, accepted):** existing momentary / smart-plug / `pid.enabled=true` no-switch users go cold after OTA. Document. Do not default Heat to "avoid surprise" — that keeps the unmanned-boiler bug.
- **InitState vs context ID (high if ignored):** finalize-only `setCurrentStateId(STANDBY)` + runtime pid false → InitState → `PID_DISABLED` → first momentary press is `powerOff`. Must `initialize(target)` **and** sync the mirror (PowerHandler runs first).
- **Restore without persist-off (high if ignored):** third enum value is Heat in disguise. Persist in `powerOff` or drop Restore.
- **Heat persist (medium):** keeping today's `setUserPidEnabled(true)` on momentary boot makes Restore always-on after first boot. Runtime-only on boot.
- **Standby timer:** fork already guards 0-ms; still reset on Heat/Restore-on. `StandbyState::onExit` re-reads `pid.enabled` — correct only if boot did not overwrite it.
- **640 interaction:** boot `STANDBY` so wake works even if 640 is not merged. After 640, `PID_DISABLED` would also wake; still prefer `STANDBY` for power-off semantics (display, display-off timer).
- **Heater invariant:** `STANDBY` / `PID_DISABLED` already disable heater in `ProcessController::shouldPIDBeEnabled`. No relay pokes. `onEntry` of those states already `setPidRuntimeState(false)`.
