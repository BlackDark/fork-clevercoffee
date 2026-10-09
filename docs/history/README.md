# How the firmware became what it is

**The story of a C++ firmware being replaced by a Rust one, and what that cost,
what it found, and what it did not finish.**

This is the spine of `docs/history/`. Every other document in the folder is the
detail behind a claim made here — **with one exception**:
[`recovered-oracle.md`](recovered-oracle.md) is normative, because it is the sole
derivation of the fail-closed `LOW_TRIGGER` heater rule. The folder says where
knowledge came from, not whether it is still load-bearing. If you want the shape of the firmware rather
than its history, read [`../architecture.md`](../architecture.md) instead; if you
want to know what works right now, read [`../status.md`](../status.md), which is
the only page allowed to claim that.

---

## Why the port happened

The machine runs an ESP32 with about 320 KB of RAM. The firmware that ran on it
was C++ on Arduino, and it worked, but it was the product of a decade of
incremental edits by several hands. A bug fixed in the display layer could not be
shown to be fixed in the heating logic, because the two were the same object and
the tests could not run without the chip.

Decided 2026-09-28. Not re-litigated.

The target was deliberately *not* bare metal. `esp-hal` plus `esp-radio` plus
embassy has no HTTP server, no OTA, no NVS and no filesystem, and a verified TCP/IP
stack for `esp-radio` did not exist. Building those would have been a multi-month
project that dwarfed the firmware it served. The `esp-idf-*` crates are
community-maintained and were the only option covering the feature set.

Three decisions were locked on day one and have held:

- **Updates are a forced full flash over USB.** The device has no native USB; the
  Micro-USB port is a CP2102N UART bridge.
- **No NVS backward compatibility.** The Rust firmware owns its own key namespace
  and does not read the old one, except to say so once at boot.
- **The React UI in `ui/` stays.** It was not rewritten to make the port easier.

## The shape of the work

Four phases, planned as R0 through R4 and executed in that order.

| Phase | What it was | Outcome |
| --- | --- | --- |
| **R0** | Confirm the physical board exists and is what we think | Blocked for weeks: no device attached. Resolved late, in person. |
| **R1** | Feasibility spikes on the three things that could kill the port | All three survived: flash size, the TSIC-306 protocol, the toolchain. |
| **R2** | The portable domain: pure crates, no hardware | `cc-domain`, `cc-protocol`, `cc-safety`, `cc-machine`, `cc-display` |
| **R3** | The hardware abstraction | `cc-hal-esp32`, and the pin map that finally stopped lying about itself |
| **R4** | Integration, and getting it onto the board | R4-01, 2026-09-30: the reducer running at 100 Hz with the PID driving the heater |

The single most consequential decision was structural, and it is the reason most of
this firmware is testable: **the control logic was written as a pure reducer with
no access to hardware.** `reduce(machine, context, event) -> (machine', effects)`.
It asks for `EnablePump`; it does not set a pin. That is not a style preference.
It is why the state machine is testable with a plain `cargo test` at all, why the sensor protocols are
state machines over bytes, and why a bug in the heating logic can be demonstrated
fixed without a coffee machine in the room.

## What the port found

The C++ was the specification, so reading it was the job. It is also where the
interesting material is: [`cpp-findings.md`](cpp-findings.md) is the
per-feature catalogue of every bug and ambiguity found on the way through, each one
pinned by a named test. Its duplicate section numbers are evidence a finding was
corrected — they were left in place rather than renumbered.

Four findings worth knowing without reading the catalogue:

**The steam valve had no safety gate at all.** The C++ had a whitelist check for
the water valve and no equivalent for steam, and nothing called the function that
would have been one. This port adds `cc_safety::steam_flow_allowed` and closes the
valve in every state but `STEAM_RUNNING`. That is not tidying: steam and water
share **one relay** — which is true of the software model and, since 2026-10-07,
corrected in `pins.md`: the relay is the *water* valve, and steam is released by a
hand-operated wand valve, so no build has ever asked for that position. The
whitelist is a guard against a future change, not a repair of a live hazard.

**The steam LED is wired to GPIO 1, which is UART TX.** The C++ header's own
comment records the decision to move it and the move was never made, so both
drivers ran and the last one won. The port implements the rule and leaves the pin
unwired, because moving it is a *hardware* change: GPIO 32, the alternative the
comment names, is the scale's data line. See [`../hardware/pins.md`](../hardware/pins.md).

**A tank-empty machine could run the pump.** The C++ checked the float switch for
display purposes but not before pumping. The port gates the pump.

**The PID's derivative term was computed from the wrong clock.** It is now taken
from real elapsed time.

All four are in [`divergences.md`](divergences.md) with the reasoning, and all four
are in [`../differences.md`](../differences.md) in the short form an operator would
meet them.

## The two things the harness taught us

Both were learned the hard way and both are now structural.

**A LOST is not a pass.** The device-test audit checked that tests were
*registered*. Nothing checked that they *ran*. When the display tests' 2 KB
recorder overflowed the main task's 3584-byte stack, the device reset, and the
runner scored seven cases LOST while reporting "125 passed, 0 failed". A green run
that had quietly stopped testing anything. Each case now runs on its own 8 KB task,
and the audit exists because of it.

**A test that has never run is not a test.** Those seven had been LOST since they
were written. When they actually executed, two failed on assertions that had been
wrong the whole time — one helper scanned backwards for a control byte and so found
one *inside the rendered pixels*, because the test ramp contains `0x40`. It now
records the offset.

## What the measurements said

**Flash was never the binding constraint; RAM is.** The image fits with 133 KB to
spare in the `app0` slot. Static RAM is 133,168 B — 42 % of the ESP32's 320 KB —
and 94 KB of that is IRAM belonging to the prebuilt Wi-Fi MAC, which is untouchable
without dropping Wi-Fi. This is why the web UI is served from flash: a 199 KB gzipped
bundle costs 0 B of RAM because `build.rs` embeds it with `include_bytes!`.

**The 10 ms period is met.** Remeasured 2026-10-09: mean work 1 ms, achieved
period 10 ms. About 5% of ticks still exceed 10 ms. The September 62% figure
was the display frame inside the tick. Do not relax `TICK_BUDGET_MS`.

**The loop was running at 2.5 Hz.** `CONTROL_TICK_MS` was 400 ms, and the display
frame was written inside that tick, which is why a switch press took half a second
to appear. The control task now runs at 100 Hz and the panel has its own task. A
sensor task was tried and removed: the DS18B20's bit-bang asserts inside the
FreeRTOS kernel when it runs on a second task. Kept. Recheck 2027-01.

## What it did not finish {#where-it-is-now}

Stated here because a reader will otherwise assume otherwise. The full list, with
what backs each line, is [`../status.md`](../status.md).

- **The parity baseline was never captured.** The harness works; 13 scenarios
  report `BASELINE-MISSING` and exit 2. Capturing one means flashing the C++ onto a
  powered, wired machine, which runs its own control loop. The owner declined,
  and a baseline never measured beats one fabricated. **Parity has therefore never
  been demonstrated on this machine.** What the harness proves today is that the
  scenarios run and the ledger is readable.
- **The C++ is deleted.** As of 2026-10-06 the tree, its PlatformIO build and its
  Wokwi simulator are gone. There is no rollback image and no way to rebuild one
  short of checking out `9fa8c834` and reconstructing the tooling.
- **Switch presses have still never been tested by hand.** Debounce and long-press
  are pinned by 17 host tests against a synthetic clock. The press itself is still
  a human's to make.
- **A machine that boots to `PID_DISABLED` is the power switch.** It defaults
  to enabled, type `Toggle`, so a toggle reading off starts disabled. Set
  `hardware.switches.power.enabled` false and `pid.enabled` true to boot in
  `PID_NORMAL`.
- The rotary encoder and the zero-crossing dimmer are not ported. OTA has never
  been exercised on hardware. The HX711 is implemented and no scale is fitted.

## Where the detail is

| Document | What it holds |
| --- | --- |
| [`feature-inventory.md`](feature-inventory.md) | What the C++ did, feature by feature. The scope the port had to cover. |
| [`cpp-findings.md`](cpp-findings.md) | Every bug and ambiguity found in the C++, each pinned by a named test. |
| [`divergences.md`](divergences.md) | Every place this firmware deliberately differs, with reasoning. Read by `cc-parity`. |
| [`target-architecture.md`](target-architecture.md) | The intended crate boundary. Partly superseded; check before trusting it. |
| [`recovered-oracle.md`](recovered-oracle.md) | **Normative despite living here.** A Rust firmware recovered from a flash dump whose source is gone, and the only surviving derivation of the fail-closed `LOW_TRIGGER` heater rule. |
| [`dependency-evaluation.md`](dependency-evaluation.md) | Per-crate evidence: licence, MSRV, what was verified and what was not. |
| [`scenario-format.md`](scenario-format.md) | The scenario file format `cc-parity` parses. |
| [`review-2026-10-03.md`](review-2026-10-03.md) | An independent review's findings, one per row, with status. |
| [`scenarios/`](scenarios) · [`baseline/`](baseline) | The scenario set, and the deliberately empty baseline directory. |

For the decisions that outlived the port, see [`../adr/`](../adr). For the
migration's own planning records, see [`../archive/migration/`](../archive/migration).
