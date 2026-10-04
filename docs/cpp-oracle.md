# The C++ oracle

**Where the C++ firmware lives, why it is frozen, what may still be built from it,
and what must never be done to it.** Nothing in the repository said so before this
page, and guessing wrong here means flashing a control loop onto a powered, wired
machine.

The rules themselves are numbered in [`AGENTS.md`](../AGENTS.md) — `AG-ORACLE-*` —
and are not restated here.

## Where it is

`src/`, `include/`, `lib/` (the firmware), `test/` (the native tests),
`platformio.ini` (the PlatformIO build), and `partitions_4M.csv` at the
**repository root**.

**There are two partition tables and they are not interchangeable.** The Rust one
is `rust/partitions_4M.csv`, and its own header says: *"DO NOT confuse this with
the repository-root `partitions_4M.csv`: that one is C++-owned."*

## What it is, and why it is frozen

The C++ firmware is **both the parity oracle and the definition of feature scope**
for the Rust port. Every deliberate divergence is recorded in
[`intentional-diffs.md`](./rust-migration/intentional-diffs.md); the one-page index
of the ones a reader can actually meet is
[`34-known-differences.md`](./rust-migration/34-known-differences.md).

It is frozen because it **runs its own control loop** on a powered, wired machine:
pump, valve, heater, and a real boiler behind them. Flashing it is not a build
step — it is putting the real machine's control loop back on the board. Capturing
a parity baseline from it would mean the same thing, which is why
`rust-migration/baseline/cpp/` is deliberately empty and `just parity` reports
`BASELINE-MISSING` rather than pretending otherwise.

## What may still be built from it

- **As a readable reference for behaviour.** It is the authority on what the
  machine used to do. Read it; do not edit it.
- **As a build.** `pio run -e esp32_usb` and `pio test -e native_test` still work,
  and task R4-10 keeps them working for one release cycle as a rollback path. The
  `merge_bin` line in [`README.md`](../README.md) still produces a flashable image
  — for someone who has decided to flash it.

## What must NEVER be done

1. **Modify it.** Not its behaviour, not its formatting, never as a side effect of
   a Rust change (`AG-ORACLE-3`: `just fmt-cpp` only when a C++ change is the
   deliberate subject of the commit).
2. **Reformat it** as a drive-by — it is the parity baseline, and a whitespace
   diff across it buries the next real change.
3. **Flash it.** Never, under any circumstances, from this migration.
4. **Deprecate or remove the PlatformIO build.** R4-10 deprecates it *without*
   removing it; that is a separate, later decision.

## The two firmwares are not interchangeable

That is **why** the Rust default hostname is `test-cc-rust` and the C++'s is still
`silvia`. The port diverges on pump timeouts, the steam-valve whitelist and the PID
divide, so answering to a hostname that names the firmware is worth more than a
brand-neutral one. One definition for the Rust name:
`cc_config::schema::DEFAULT_HOSTNAME`, with `docs/example_config.json` kept in
step. Full reasoning:
[`intentional-diffs.md` §12](./rust-migration/intentional-diffs.md).

## The pin table

From `include/clevercoffee/hardware/pinmapping.h`. Its 21 `static_assert`s
validate the map at compile time and there is no chip-variant `#ifdef` anywhere in
the file. This is the copy you actually need mid-change; the port's own validated
map is `crates/cc-hal-esp32/src/pins.rs`, whose module doc explains every
divergence from the table below.

| GPIO | macro | role | line |
| --- | --- | --- | --- |
| 39 | `PIN_POWERSWITCH` | power switch (input-only) | `:17` |
| 34 | `PIN_BREAWSWITCH` | brew switch (input-only) | `:18` |
| 35 | `PIN_STEAMSWITCH` | steam switch (input-only) | `:19` |
| 36 | `PIN_WATERSWITCH` | hot-water switch (input-only) | `:20` |
| 4 / 3 / 5 | `PIN_ROTARY_DT` / `_CLK` / `_SW` | rotary encoder — declared, unused | `:22-24` |
| 16 | `PIN_TEMPSENSOR` | TSIC-306 (ZACwire) or DS18B20 (1-Wire) | `:27` |
| 23 | `PIN_WATERTANKSENSOR` | water tank float switch | `:28` |
| 32 / 25 | `PIN_HXDAT` / `PIN_HXDAT2` | HX711 #1 / #2 data | `:29-30` |
| 33 | `PIN_HXCLK` | HX711 shared clock | `:31` |
| 17 | `PIN_VALVE` | valve relay (steam **and** water, multiplexed) | `:38` |
| 27 | `PIN_PUMP` | pump relay | `:39` |
| 2 | `PIN_HEATER` | heater relay | `:40` |
| 26 | `PIN_STATUSLED` | status LED | `:43` |
| 19 | `PIN_BREWLED` | brew LED | `:44` |
| 1 | `PIN_STEAMLED` | steam LED — see the trap below | `:45` |
| 18 | `PIN_ZC` | dimmer zero-crossing — declared, never used | `:48` |
| 22 / 21 | `PIN_I2CSCL` / `PIN_I2CSDA` | I2C0 — OLED + ABP2 pressure sensor | `:53-54` |

Two traps in that table, both of which have already cost time:

- **`PIN_STEAMLED` is 1**, and its own comment says the LED was moved off GPIO1
  because GPIO1 is UART TX. The move was written down and not made, so the C++
  drives both and the last attach wins. That is parity with a C++ bug, not with a
  working feature. The port leaves the steam LED unwired — see
  [`intentional-diffs.md` §27](./rust-migration/intentional-diffs.md).
- **GPIO32 is `PIN_HXDAT`**, the very alternative that comment names ("32 works
  with logging"). Moving the steam LED is therefore a *hardware* change.

## Per-feature behaviour, and the record that cannot be lost

- [`09-cpp-findings.md`](./rust-migration/09-cpp-findings.md) — the per-feature
  catalogue: every bug and ambiguity found in the C++ while porting it, each
  pinned by a named parity test, and which have since been closed on purpose. Its
  **duplicate section numbers are evidence that a finding was corrected** — do not
  renumber it.
- [`08-recovered-oracle.md`](./rust-migration/08-recovered-oracle.md) — the only
  surviving record of a Rust firmware that ran on this board, recovered from a
  flash dump whose source is gone. It is **normative** for the fail-closed
  `LOW_TRIGGER` heater-relay rule: an undriven GPIO at reset energises a
  `LOW_TRIGGER` heater relay, so a configuration selecting one is refused outright
  rather than made safe.
