# The scenario format

A scenario is a scripted run of the real `cc-machine` reducer and `cc-safety`
monitor, with assertions over the resulting states and effects. It makes a
behavioural claim testable without a device.

The format is deliberately small: seven stimulus kinds, each one a door into the
machine that the firmware already has. A scenario runs the Rust reducer in
process. Nothing is compared with another firmware.

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

Between stimuli the runner advances the control loop at `tick_ms` (default 10 ms,
the firmware's `Timing::MAIN_LOOP_INTERVAL_MS`) and feeds `Event::Tick`. That is
what makes a `wait` of 5000 ms mean five hundred iterations and not one.

### The seven kinds

| `kind` | Fields | What it does |
| --- | --- | --- |
| `rest` | — | no stimulus; the loop keeps ticking. Used as a deliberate pause. |
| `wait` | `duration_ms` (default 1000) | tick for a fixed interval |
| `button` | `switch` (`brew`/`steam`/`power`/`hot_water`), `action` (`press`/`release`), `long_press` (default false) | a switch edge → `Event::ButtonPressed`/`ButtonReleased` |
| `sensor` | `temperature_c`, `water_tank_full`, `has_temperature_error`, `has_scale_error`, `brew_weight` | a new sample → `Event::SensorUpdated`, and a `cc_safety::reduce` |
| `config` | `set: { dotted.key: value }` | mutate `cc_config::Config`; a parameter write. |
| `mqtt` | `command` — a `cc_machine::Command` name | a request from outside the switch layer → `Event::Command` |
| `ota` | `action` (`begin`/`end`), `path` (for `begin`) | an OTA session. `begin` asserts the actuator gap is closed (see §7). |

`mqtt` is named for the transport, but the *stimulus* is the request flag it sets
(`context->setBrewStartRequested(true)`), which is what the firmware consumes. A
scenario that says what it means, not which socket carried it, stays readable.

### A worked example

```yaml
name: brew_aborted_mid_flow
title: Aborting a brew during the flow phase leaves every actuator off
mode: dry_run
covers: [S5, S11]

why: >
  A stop request is honoured from `BREW_RUNNING` whatever the pump watchdog is
  doing. The valve fail-safe then closes the valve on the following loop, one
  loop after the state change — a window worth pinning.

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
| `state_transitions` | `true` | the entered `MachineState` names, from `Effect::EnterState`. |
| `effects` | `true` | the ordered `Vec<Effect>` stream, by `Effect::name()`. **`dry_run` only** — a hardware run observes endpoint bodies and log lines instead. |
| `endpoints` | `[]` | HTTP paths to snapshot (`/api/status`, …). **`hardware` only.** |
| `log_patterns` | `[]` | log lines to collect, as plain substrings. Matched against the UART/telnet stream. |

The observation is **reduced before the assertions read it**: state names are the
`SCREAMING_SNAKE` enumerators (`MachineState::name()`), effect names are the stable
`Effect::name()` strings, and effect values that no assertion needs are dropped. Two
runs of the same scenario must produce equal observations, and a test asserts that
over the whole set.

`effects` is why a `dry_run` scenario asserts what it does: the effect stream and the
visited states are properties of the *decision*, and they are visible without a device.
A `hardware` scenario asserts on the other half — endpoint bodies and log lines — which
only a device can produce.

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
| `actuator_safe` | — | at the end of the run, heater duty 0, pump off, both valves closed, emergency latch **not** set: the *known-safe state*. |
| `http` | `path`, `expect: { key: value }` | a captured endpoint's JSON contains these keys with these values |

`actuator_safe` is the assertion that makes a `dry_run` scenario honest. A scenario
that ends with the pump running is a scenario that would have left a pump running, and
the harness says so even though nothing was ever connected.

## 6. `config.set` — the machine under test

`config.set` is a map of the **dotted schema keys** (`brew.pre_infusion.time`,
`hardware.switches.brew.enabled`) to values, applied on top of
`cc_config::Config::default()`. The keys are the vocabulary the schema, the API
and the UI share, so a scenario reads like the configuration an operator would
write.

Every key is validated against `cc_config::schema::SCHEMA` and rejected if unknown, if
out of range, or if the wrong type. A scenario that names a parameter which does not
exist is a broken scenario, not a configuration the firmware ignores.

## 7. `ota` — the actuator gap, and why the assertion is a negation

The rule: while an OTA session is active the control loop must not run and the pump
and valve must not be driven, and beginning a session disables the heater. Every
actuator ends up off.

The scenario pins the OTA rule: the pump and valve must be shut down when a
session begins, and this is the assertion that catches a regression. The
`assert` for `ota: begin` is therefore a negation:

```yaml
- { kind: never, effect: EnablePump, after_ms: 0 }
- { kind: actuator_safe }
```

An assertion failure here is the fix regressing, not a difference to explain.

## 8. Where the files live

```text
crates/cc-parity/scenarios/*.yaml    the scripts
```

A scenario is the only committed artefact. Running it produces an observation on
stdout (`cc-parity run`), and the assertions judge that observation in the same run;
nothing is recorded for a later comparison.

Thirteen scenarios are `dry_run`, and `just parity-test` runs every one of them. The
other four are `hardware`: they describe what a device run would check, and the binary
refuses them rather than pretending to run them.
