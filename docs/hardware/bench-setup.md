# The test bench

**What to attach to a spare board so the firmware can be exercised without a coffee
machine.** The machine's own pin map is [`pins.md`](pins.md); this page is the
subset of it that a bench can carry, and — more usefully — what a bench proves
and what it cannot.

Decided 2026-10-06 by Eduard Marbach, together with the deletion of R4-01's
bring-up inhibit (`docs/status.md`). A bench is the first place anything is run
now that the water path is live.

## The board

An **original ESP32, rev 3.0** — not an S3, not a C6 (`AG-REPO-10`). The
firmware's pin table is validated at compile time and at boot
(`crates/cc-hal-esp32/src/pins.rs`), so a board with a different GPIO count is a
build failure rather than a surprise at 2 a.m.

## What to attach

| Part | GPIO | Notes |
| --- | --- | --- |
| DS18B20 | 16 | One temperature sensor is enough. The TSIC-306 arm shares the pin and is not exercised. |
| SSD1306 (128×64, I²C) | 21 = SDA, 22 = SCL | Shares the bus with the ABP2 pressure sensor; with no sensor fitted the display still works. |
| Four momentary buttons | 34, 35, 36, 39 | Brew, steam, hot water, power. **Each needs a pull resistor** — see the warning below. |
| LED + 330 Ω | 2 | Heater. |
| LED + 330 Ω | 27 | Pump. |
| LED + 330 Ω | 17 | Valve — steam and water, multiplexed, so one LED shows both. |

Wi-Fi is not optional: the web UI, the API, MQTT and the OTA exercise all need
the board associated. Provision it with `just wifi-provision <port>`, or over the
UART console with `wifi set <ssid>`.

### The buttons need pull resistors

`docs/operations/runbook.md` records the failure this prevents: **all four
switches reading `true` at boot means the inputs are floating**, and the first
brew switch press is then your own boot. Wire each button with a 10 kΩ pull to
3V3 (buttons to GND), or to GND with a pull-up, and confirm the boot log line
`switch resting levels after settling:` reports `false` for all four before
anything else runs.

## What an LED proves, and what it does not

An LED on a GPIO proves **the pin**. It does not prove:

- **a relay coil or its contacts.** The bench never switches current, so a coil
  that is open, a contact that is welded, or a wiring error between the pin and
  the relay board is invisible here. `AG-REPO-26` is about who *reaches* the
  pin, not about what the pin is wired to.
- **relay polarity against this machine's wiring.** Polarity is read from
  `hardware.relays.{pump,valve,heater}.trigger_type` at boot
  (`crates/cc-firmware/src/main.rs`), defaulting to `HIGH_TRIGGER`, which matches
  the recovered oracle's boot log. A bench cannot tell you what *your* board's
  relays are wired for — the value is a config fact you read, not measure.
- **the `LOW_TRIGGER` heater fail-closed case.** A configuration selecting
  `LOW_TRIGGER` for the heater is refused outright by `validate_config` rather
  than made safe (`pins.md`, and the normative derivation in
  `docs/history/recovered-oracle.md`). It is a compile- and boot-time refusal, so
  there is no runtime behaviour to watch on a bench.
- **water.** Nothing here moves water. Brew, backflush, hot water and steam all
  transition with LEDs standing in for the pump and the valve: the firmware side
  is proven, the hydraulic side is not.

## What a bench can prove outright

The R4-04 safety cases, written up in
[`../operations/runbook.md`](../operations/runbook.md) §R4-04, split along this
line. On a bench, observable as pin transitions or log lines: overtemp trip,
emergency latch and recovery, tank-empty **pump inhibit**, watchdog reboot, and
actuator-off during OTA. Machine-only, because they are about liquid that must
not move: tank-empty pump **kill** and valve fail-safe.

## First water-enabled run on the machine

The bench proves the firmware. The machine proves the plumbing. In this order:

1. Bench, LEDs only: run the five bench-exercisable R4-04 cases.
2. Machine, **reservoir empty**: boot and confirm `switch resting levels after
   settling:` reports `false`, so the tank-empty pump inhibit proves itself
   before any water can move.
3. Machine, filled: the first real brew, and the two machine-only R4-04 cases
   with it.

Reversal, if the polarity or the float switch turns out wrong on the machine:
`actuators.set_inhibit` in `crates/cc-firmware/src/main.rs` holds the pump and
valve off again, and `cc_hal_esp32::Inhibit` exists for exactly that.