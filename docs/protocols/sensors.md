# Protocols

**The wire protocols the machine speaks, written as pure state machines over
bytes.**

Every sensor this firmware reads had a C++ driver that ran a bit-bang loop,
blocked on a delay, or trusted a checksum it never verified. Porting them as
straightforward blocking code would have reproduced exactly the things that make
the original hard to test.

Instead, each protocol in `cc-protocol` is a state machine that takes bytes and
returns bytes. It has no timer, no GPIO and no I/O, which means every one of them
is testable by feeding it a byte array and asserting what comes back — including
the malformed cases the hardware produces in the field.

| Protocol | Device | Bus | Notes |
| --- | --- | --- | --- |
| `ds18b20` | DS18B20 temperature | 1-Wire | CRC-verified. The C++'s path applied `isValidTemperature`'s range as a debug print rather than a guard; that is a divergence |
| `tsic306` | TSIC-306 temperature | ZACwire / single-wire | The proprietary one. No Rust crate exists, so it was written from the manufacturer's application note and cross-read against the C++ library |
| `hx711` | Load cell | bit-bang + interrupt | The scale. Implemented; no scale is fitted to the board |
| `abp2` | ABP2 differential pressure | I²C | Read on every tick, so the port made it non-blocking and checks what the C++ ignored |
| `onewire` | Shared 1-Wire primitives | — | The family above the DS18B20 |
| `http_auth` | HTTP Basic | — | Boot-time, not per-request |
| `provisioning` | Wi-Fi provisioning console | UART | GPIO1. The only way to recover a machine joined to a nonexistent network |

## Why the TSIC-306 was the risky one

It is the sensor that could have killed the port. There is no Rust crate, the
protocol is proprietary, the timing is sensitive, and the manufacturer publishes
an application note rather than a specification.

The port transcribed the note, then cross-read it against two independent
implementations: the C++ library, and the worked example in the note that the
C++ did *not* implement. Where the note and the library disagree, the note wins
and the disagreement is recorded.

**The device path has never run.** The pure logic is tested; the hardware path
is written and unproven. The over-temperature debounce counts probe *samples*
rather than clock ticks, which is a divergence from the C++ — the C++'s version
under-counts when a probe read is slow, which is exactly when it must not.

`crates/cc-protocol/src/sensor/tsic306/` says which claim came from where.

## Sensor state, and the water tank

`cc-safety` gates the pump on the tank being non-empty. That is a divergence: the
C++ read the float switch for the display and not for the pump, so an empty tank
did not stop a 2 kW machine from being pumped.

The float switch is also the thing most likely to be a lie. It is a mechanical
float in a tank with moving water, so the port treats it as advisory in one place
and authoritative in another, and the reasoning is in
[`../history/divergences.md`](../history/divergences.md).

## Adding a protocol

1. Make it a state machine over bytes. No timers, no `static mut`, no delay. If
   it needs a clock, take the timestamp as an argument.
2. Test the malformed cases first: a truncated frame, a bad CRC, a stuck bus, a
   device that never answers. The hardware will do all of them.
3. Decide what the sensor means when it is *wrong*, and put that decision in
   `cc-safety` or `cc-machine`, not in the protocol crate. A protocol crate that
   decides policy is a protocol crate that cannot be tested against a byte array.

For which crates are portable and why, see
[`../architecture.md`](../architecture.md). For the dependency evidence behind
the choice of crates, see
[`../history/dependency-evaluation.md`](../history/dependency-evaluation.md).
