# Architecture

**What this firmware is, how it is put together, and why it is shaped this
way.** One page, then links. If you only read one document, read this one and
then [`status.md`](status.md).

For the vocabulary, read [`GLOSSARY.md`](../GLOSSARY.md) first. Several terms
below come from the C++ firmware this one replaces, and some of them are now
misleading because the thing they referred to is gone.

---

## The one-sentence version

An ESP32 runs a pure state machine, a pure PID and a pure sensor stack with no
knowledge of hardware, and a thin HAL is the only thing that touches a pin. That
is the whole design, and it is why most of the firmware is testable on a laptop.

## The shape

```mermaid
TB
    subgraph app["Application — pure, no_std, host-testable"]
        direction LR
        DOM["cc-domain<br/>vocabulary, units, enums"]
        MACH["cc-machine<br/>reducer + effects"]
        SAFE["cc-safety<br/>may this actuator run?"]
        PID["cc-domain<br/>PID"]
        SENSOR["cc-protocol<br/>DS18B20, TSIC-306, HX711"]
        DISP["cc-display<br/>layout, framebuffer"]
        CONF["cc-config<br/>98-parameter schema"]
        WEB["cc-web<br/>HTTP handlers"]
        MQTT["cc-mqtt<br/>topics, publish registry"]
        NET["cc-netpolicy<br/>link policy"]
    end

    subgraph dev["Device — the only place that may name esp_idf"]
        HAL["cc-hal-esp32<br/>actuators, pins, I2C, wifi, NVS"]
        FW["cc-firmware<br/>10 ms control task, boot"]
    end

    subgraph tools["Host tools — never flashed"]
        PARITY["cc-parity<br/>scenario harness"]
        DEVT["cc-device-tests<br/>on-target runner"]
    end

    FW --> MACH
    FW --> DISP
    FW --> SENSOR
    FW --> CONF
    FW --> NET
    MACH --> DOM
    MACH --> PID
    MACH --> SAFE
    HAL -.->|"applies"| MACH
    WEB --> DOM
    MQTT --> DOM
    WEB --> CONF
    PARITY -.->|"in-process"| MACH
```

Three boundaries carry the design, and each one is enforced rather than merely
documented.

**Nothing portable names ESP-IDF.** `cc-domain` through `cc-mqtt` are `#![no_std]`
and may not import `esp_idf_{hal,svc,sys}`. A CI check strips comments first, so
a crate can still explain why it avoids ESP-IDF. The payoff is that the state
machine, the PID and every sensor protocol run in a normal `cargo test`.

**The reducer cannot touch hardware.** `cc-machine` is an Elm-style
`reduce(machine, context, event) -> (machine', effects)`. It returns
`Effect::EnablePump`; it never sets a pin. The effects are applied by
`cc-hal-esp32::actuators` in the same 10 ms tick. This is the rule that keeps a
bug in the state machine from being a bug in the GPIO layer.

**Safety is a backstop, not a participant.** `cc-safety::water_flow_allowed` and
`steam_flow_allowed` run every tick, after the reducer, and can close a valve
the state machine had legitimately opened. The reducer decides what should
happen; safety decides what is allowed to happen. Water and steam share one
relay, which is why the steam whitelist is not optional polish.

## The control loop

One iteration of the 10 ms task, in order:

1. Drain the event inbox (button presses, HTTP requests, MQTT commands) and
   `reduce` each one.
2. `reduce` a final `Tick` event carrying the current clock reading.
3. Apply the returned effects through the actuators.
4. Sample the sensors into the shared snapshot.

Steps 1 and 2 are separate `reduce` calls because the reducer handles exactly one
event. Nothing here allocates: `crates/cc-machine/tests/tick_allocations.rs`
asserts zero heap bytes per tick, because a control loop that allocates is a
control loop that eventually fails to be scheduled.

## Where the memory goes

The ESP32 has about 320 KB of RAM, which is the constraint that shapes more of
this design than anything else.

- **The web UI is served from flash**, 0 B of RAM. `cc-hal-esp32/build.rs`
  embeds the gzipped bundle with `include_bytes!`. Do not "improve" this by
  buffering it.
- **A display frame allocates nothing.** `cc-display` has no `alloc` in the
  device build, so an allocation there is a link error rather than a runtime one.
- **The frame is chunked into 8 I²C writes, not 64.** The SSD1306 and the ABP2
  pressure sensor share one bus; a write per pixel row starves the sensor.
- **The PID's heater PWM is an ISR.** See `cc-hal-esp32`'s heater module; this is
  the one legitimate place to bypass the effect layer, and it is documented at
  its definition.

## The boundaries between crates

| Crate | Owns | Deliberately does not |
| --- | --- | --- |
| `cc-domain` | units, enums, the 18 machine states, the PID | anything behavioural |
| `cc-machine` | the reducer, the per-subsystem handlers, the effects | I/O, clock, hardware |
| `cc-safety` | whether an actuator may be energised | what the machine wants to do |
| `cc-protocol` | DS18B20, TSIC-306, HX711, ABP2 as state machines over bytes | the devices themselves |
| `cc-netpolicy` | what the link should do next | the radio |
| `cc-display` | layout, framebuffer, glyph atlases | the panel |
| `cc-config` | the 98-parameter schema and the store | the flash |
| `cc-web` | HTTP handlers as pure functions of a snapshot | the HTTP server |
| `cc-mqtt` | topic layout, publish registry | the broker |
| `cc-hal-esp32` | pins, actuators, I²C, SPI, wifi, NVS, the web server | any decision |
| `cc-firmware` | boot order and the 10 ms task | logic of its own |

`cc-web` and `cc-mqtt` are pure functions of a telemetry snapshot, which is why
they have hundreds of host tests for an application that runs on one microcontroller.

## Tools that are not firmware

**`cc-parity`** links the real reducer and drives it on your machine with a
recording stand-in for the GPIO. It runs the scenarios in
[`history/scenarios/`](history/scenarios) and compares observations. See
[`history/scenario-format.md`](history/scenario-format.md).

**`cc-device-tests`** runs `cc-hal-esp32`'s unit tests *on the chip*, because
that crate cannot be tested by `cargo test` at all. It is never flashed as the
firmware.

## What this does not yet do

Stated plainly because a reader will otherwise assume otherwise: there is no
MQTT-over-anything-else, no scale fitted to the board, no OTA path exercised on
hardware, and neither the rotary encoder nor the dimmer is ported. The full and
current list, with what backs each line, is [`status.md`](status.md).
