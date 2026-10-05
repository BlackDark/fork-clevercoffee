# 10 — The parity scenario format

**Owner:** R1-08.

This is the answer to the hole in 06 §R1-08 step 1: *"the scenario format is currently
undefined, which is the real hole."* Without it, "parity" has no operational meaning,
because "the same scripted input" was never written down.

Everything here is a **contract between the C++ firmware and the Rust port**. A
scenario is the same script run against both, and the two are compared. If a scenario
is not expressible in this format, it is not something the harness can prove parity
for, and that is a gap to record rather than paper over.

The format is deliberately small. It has exactly seven stimulus kinds, and each one
corresponds to a door into the machine that both firmwares have.

---

## 1. The two halves of a scenario

```text
scenario.yaml
├── identity      name, title, why           — what and why
├── mode          dry_run | hardware         — where it runs
├── covers        [S1, S4, …]               — the safety paths it exercises
├── config        dotted config overrides   — the machine's configuration
├── stimuli       timed inputs              — the script
├── capture       what to observe           — the observations
└── assert        expected outcomes         — the pass/fail criteria
```

`stimuli` and `capture` are the *inputs* and the *observations*. `assert` is what makes
the scenario a test rather than a recording.

## 2. `mode` — the safety decision, stated per scenario

| `mode` | What runs | Actuators | Used by |
| --- | --- | --- | --- |
| `dry_run` | the `cc-machine` reducer and `cc-safety`, **in process, on the host** | **never energised** — the effect stream is asserted, never applied | every scenario that would brew, flush, or steam |
| `hardware` | the flashed firmware on the real device | only what the scenario's own `assert` block permits | cold boot, sensor read, overtemp with the heater gate closed |

`dry_run` is not "simulated hardware". It is the real reducer, the real safety
monitor, and the real effect stream, with a recording `Actuators` implementation in
place of the GPIO. A `brew_by_time` dry-run **does** produce `Effect::EnablePump` and
`Effect::OpenWaterValve`; the test asserts those effects were emitted and asserts the
safety ordering around them. Nothing is ever wired to a pin, because the harness is a
host binary that has no pin.

This is the property that makes the harness usable at all: **twelve of the thirteen
scenarios run with no device attached and no actuator capable of being energised.**

A scenario that *must* be `hardware` is one whose subject is the device, not the
logic: the boot sequence, a real sensor read, and an over-temperature trip observed
through a real probe. Those are the only three, and none of them energises the pump,
the valve, or the heater above zero.

## 3. `stimuli` — the timed script

Every entry is `{ at_ms, kind, … }`. `at_ms` is milliseconds since the scenario's
`at_ms: 0`, which is the first control-loop iteration after `boot`. Entries are
applied in ascending `at_ms`; equal timestamps are applied in file order, and the
runner rejects an unsorted file rather than guessing.

Between stimuli the runner advances the control loop at `tick_ms` (default 10 ms, the
C++'s `Timing::MAIN_LOOP_INTERVAL_MS` — see `constants/Timing.h`) and feeds
`Event::Tick`. That is what makes a `wait` of 5000 ms mean five hundred iterations
and not one.

### The seven kinds

| `kind` | Fields | What it does |
| --- | --- | --- |
| `rest` | — | no stimulus; the loop keeps ticking. Used as a deliberate pause. |
| `wait` | `duration_ms` (default 1000) | tick for a fixed interval |
| `button` | `switch` (`brew`/`steam`/`power`/`hot_water`), `action` (`press`/`release`), `long_press` (default false) | a switch edge → `Event::ButtonPressed`/`ButtonReleased` |
| `sensor` | `temperature_c`, `water_tank_full`, `has_temperature_error`, `has_scale_error`, `brew_weight` | a new sample → `Event::SensorUpdated`, and a `cc_safety::reduce` |
| `config` | `set: { dotted.key: value }` | mutate `cc_config::Config` mid-run. The C++ equivalent is a `/api/parameters` write. |
| `mqtt` | `command` — a `cc_machine::Command` name | a request from outside the switch layer → `Event::Command` |
| `ota` | `action` (`begin`/`end`), `path` (for `begin`) | an OTA session. `begin` asserts the actuator gap is closed (see §7). |

`mqtt` is named for the C++'s transport but the *stimulus* is the request flag it
sets, which is what both firmwares consume (`MQTTManager` and `WebServerManager` both
call `context->setBrewStartRequested(true)`). A scenario that says what it means, not
which socket carried it, is a scenario both firmwares can be driven with.

### A worked example

```yaml
name: brew_aborted_mid_flow
title: Aborting a brew during the flow phase leaves every actuator off
mode: dry_run
covers: [S5, S11]

why: >
  The C++'s `checkBrewStopRequest` (`BaseState.h:103-111`) runs before the
  specific rules, so the stop request is honoured from `BREW_RUNNING`
  whatever the pump watchdog is doing. The valve fail-safe
  (`BrewHandler.h:105-122`) then closes the valve on the following loop, one
  loop *after* the state change — a window worth pinning.

config:
  set:
    pid.enabled: true
    brew.mode: 1                 # Automatic
    brew.pre_infusion.enabled: true
    brew.pre_infusion.time: 3.0
    brew.pre_infusion.pause: 2.0
    brew.by_time.enabled: false  # a manual brew, so the switch ends it
    hardware.switches.brew.enabled: true

stimuli:
  - { at_ms: 0,    kind: button, switch: brew, action: press }
  - { at_ms: 100,  kind: sensor, temperature_c: 95.0, water_tank_full: true }
  - { at_ms: 6000, kind: button, switch: brew, action: release }   # mid BREW_RUNNING
  - { at_ms: 6200, kind: wait, duration_ms: 200 }

capture:
  state_transitions: true
  effects: true
  endpoints: []

assert:
  - { kind: visited_states, value: [BREW_PREINFUSION, BREW_PREINFUSION_PAUSE, BREW_RUNNING] }
  - { kind: final_state, value: PID_NORMAL }
  - { kind: never, effect: EnablePump, after_ms: 6100 }
  - { kind: actuator_safe }
```

## 4. `capture` — what is observed

| Field | Default | Meaning |
| --- | --- | --- |
| `state_transitions` | `true` | the ordered list of `MachineState` names entered, from `Effect::EnterState`. The C++'s equivalent is the `Entering state N (NAME)` log line (`MachineStateContext.cpp:317`). |
| `effects` | `true` | the ordered `Vec<Effect>` stream, by `Effect::name()`. **`dry_run` only** — the C++ has no effect stream, it has GPIO writes, so this is a Rust-side observation the scenario can assert on but the C++ cannot produce. |
| `endpoints` | `[]` | HTTP paths to snapshot (`/api/status`, …). **`hardware` only.** |
| `log_patterns` | `[]` | log lines to collect, as plain substrings. Matched against the UART/telnet stream. |

The observation is **canonicalised before diffing**: state names are the C++
`SCREAMING_SNAKE` enumerators (`MachineState::name()`), effect names are the stable
`Effect::name()` strings, and timestamps are dropped. Two runs of the same firmware
must produce byte-identical observations, or the diff is noise.

`effects` is the reason `dry_run` scenarios cannot be compared against a C++
baseline and why they do not need to be: a `dry_run` scenario's assertion is against
the **effect stream and the visited states**, both of which are properties of the
decision, and the C++'s own equivalent (the state-transition log, the pump/valve GPIO
states) is what `hardware` captures.

## 5. `assert` — the expected observable outcomes

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `visited_states` | `value: [NAME, …]` | these states were entered, in this relative order |
| `not_visited` | `value: [NAME, …]` | none of these was entered |
| `final_state` | `value: NAME` | the machine ends here |
| `always` | `effect: NAME` | the effect was emitted at least once |
| `never` | `effect: NAME`, `after_ms` (optional) | the effect was never emitted (optionally only after `at_ms`) |
| `count` | `effect: NAME`, `min`, `max` (optional) | how many times it was emitted |
| `ordering` | `before: NAME`, `after: NAME` | the first effect occurred before the second |
| `actuator_safe` | — | at the end of the run, heater duty 0, pump off, both valves closed, emergency latch **not** set. 06 §Definitions' *known-safe state*. |
| `http` | `path`, `expect: { key: value }` | a captured endpoint's JSON contains these keys with these values |

`actuator_safe` is the assertion that makes a `dry_run` scenario honest. A scenario
that ends with the pump running is a scenario that would have left a pump running, and
the harness says so even though nothing was ever connected.

## 6. `config.set` — the machine under test

`config.set` is a map of the **C++ dotted keys** (`brew.pre_infusion.time`,
`hardware.switches.brew.enabled`) to values, applied on top of
`cc_config::Config::default()`. The keys are the C++'s own names, not Rust field
names, because the parity question is "did the C++ firmware with this configuration
behave like the Rust one" and the configuration is shared vocabulary.

Every key is validated against `cc_config::schema::SCHEMA` and rejected if unknown, if
out of range, or if the wrong type. A scenario that names a parameter which does not
exist is a broken scenario, not a configuration the firmware ignores.

## 7. `ota` — the actuator gap, and why the assertion is a negation

06 §6 records the known C++ gap: while `OTA::isActive()`, `LoopManager::update()`
returns early (`LoopManager.cpp:128-132`), so the state machine does not run and the
pump and valve are not driven. `OTA::beginSession()` only disables the heater
(`SystemInitializer.cpp:54-59`).

So the C++ behaviour during an OTA started mid-brew is **"the pump stays on"**, and
the Rust port must be **"everything is off"**. This is the one scenario where the C++
baseline is *expected* to be unsafe, and the `assert` for `ota: begin` is therefore a
negation:

```yaml
- { kind: never, effect: EnablePump, after_ms: 0 }
- { kind: actuator_safe }
```

`divergences.md` §1's sibling entry — the OTA gap — is the ledger entry that
declares this diff expected. A diff here is the fix working.

## 8. Where the files live

```text
docs/history/
├── scenarios/*.yaml          the scripts
├── baseline/
│   ├── cpp/<name>.json       the C++ reference observation
│   └── rust/<name>.json      the Rust observation
└── divergences.md      the divergence ledger the runner consults
```

A baseline is a canonicalised observation (§4) as JSON. The runner diffs
`rust/<name>.json` against `cpp/<name>.json`.

**A scenario with no `baseline/cpp/` file has no C++ reference, and the runner says
so rather than reporting a pass.** That is the `baseline missing` state, and it is an
error, not a skip: the whole point of R1-08 is that `just parity` has something to
compare against.
