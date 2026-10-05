# The pin map

**Which GPIO does what, and the two traps in it.** The firmware's own copy is
`crates/cc-hal-esp32/src/pins.rs`, which validates it at compile time and at
boot. This page is the human-readable copy, kept because a pin map you have to
open Rust to read is a pin map nobody reads.

The rules are numbered in [`AGENTS.md`](../../AGENTS.md); none are restated here.

## Provenance, and what it costs

This table was transcribed from `include/clevercoffee/hardware/pinmapping.h` of
the C++ firmware, which was deleted when the Rust port became the product. It is
a **transcription, not live evidence** — recover the original with
`git show 9fa8c834:include/clevercoffee/hardware/pinmapping.h` if you need to
check a row against its source.

`crates/cc-hal-esp32/src/pins.rs` is the live copy, and where the two ever
disagree, that file is right. Its `assert_valid` is a `const fn` called from a
`const _` item, so a pin this chip does not have, a pin in the SPI-flash bank, an
input driven as an output, or a wire claimed twice is a **build failure**. That
is what the C++'s 21 `static_assert`s gave the oracle.

## The map

| GPIO | role |
| --- | --- |
| 39 | power switch (input-only) |
| 34 | brew switch (input-only) |
| 35 | steam switch (input-only) |
| 36 | hot-water switch (input-only) |
| 4 / 3 / 5 | rotary encoder DT / CLK / SW — declared, unused |
| 16 | TSIC-306 (ZACwire) or DS18B20 (1-Wire) temperature sensor |
| 23 | water tank float switch |
| 32 / 25 | HX711 #1 / #2 data |
| 33 | HX711 shared clock |
| 17 | valve relay — steam **and** water, multiplexed |
| 27 | pump relay |
| 2 | heater relay |
| 26 | status LED |
| 19 | brew LED |
| 1 | steam LED — see the trap below |
| 18 | dimmer zero-crossing — declared, never used |
| 22 / 21 | I²C0 — OLED + ABP2 pressure sensor |

The C++ header had no chip-variant `#ifdef` anywhere, so the map was one table
for one board and not a set of variants.

## The traps

**GPIO1 is UART TX, and the steam LED is on it.** The C++ header's own comment
says the LED was moved off GPIO1 for exactly that reason. The move was written
down and not made, so the C++ drove both and the last attach won — parity with a
bug, not with a working feature. The port leaves the steam LED unwired; see
[`divergences.md` [§30](../history/divergences.md#d30)](../history/divergences.md).

**Moving it is a hardware change, not a firmware change.** GPIO32 is the
HX711 data pin, and that is the very alternative the comment names ("32 works
with logging"). Choosing the new pin is therefore constrained by the scale
wiring, not free.

## Steam and water share one relay

`AG-REPO-22` exists because of this row: an ungated steam valve is an ungated
*water* valve, which is why `cc_safety::steam_flow_allowed` is checked every
tick and not only while brewing. See
[`ADR-0003`](../adr/0003-state-machine-hardware-control-contract.md).

## The heater relay is fail-closed by configuration, not by default

An undriven GPIO at reset energises a `LOW_TRIGGER` heater relay, so a
configuration selecting one is refused outright rather than made safe. The rule
is normative and its derivation is in
[`08-recovered-oracle.md`](../history/recovered-oracle.md).
