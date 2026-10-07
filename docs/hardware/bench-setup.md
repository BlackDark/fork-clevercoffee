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
| Brew button — **momentary** | 34 | Returns to rest when released. |
| Steam button — **toggle** | 35 | Retains its position. |
| Hot-water button — **toggle** | 36 | Also the water-injection switch while steaming. |
| *(no power switch)* | 39 | **Nothing is fitted here**, so `hardware.switches.power.enabled` is `false` in the profile — see below. |
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
## The bench configuration profile

Five parameters differ from their compiled-in defaults, and every one is
deliberate. Nothing else needs changing: the four operator switches default to
`TOGGLE`, which is also what the deleted C++ defaulted them to
(`9fa8c834:include/clevercoffee/Config.h:988-1046`), so a bench that "fixes"
them to `MOMENTARY` has quietly diverged from the machine it stands in for.

| Parameter | Bench value | Why |
| --- | --- | --- |
| `pid.enabled` | `true` | The default is `false`; a bench wants the PID driving the heater LED. |
| `hardware.switches.brew.type` | **`0`** (Momentary) | The brew button returns to rest when released. All four operator switches default to `TOGGLE` — which is also the deleted C++'s default (`9fa8c834:include/clevercoffee/Config.h:988-1046`) — so this is the one place the bench deliberately differs, because **the bench's hardware differs**. |
| `hardware.switches.power.enabled` | **`false`** | **No power switch is fitted on this bench.** Leaving it `true` with GPIO39 unwired leaves a floating input, and because the power switch is a `TOGGLE` that reads *off* at boot, the machine starts in `PID_DISABLED` whatever `pid.enabled` says. |
| `hardware.switches.steam.type` | default `TOGGLE` (`1`) | **Not changed**: those two buttons really do latch, and the deleted C++ defaulted them the same way. |
| `hardware.switches.hot_water.type` | default `TOGGLE` (`1`) | As above. |
| `hardware.sensors.temperature.type` | **`1`** (Dallas DS18B20) | The default is `0` = TSIC-306, and **a DS18B20 is what is on GPIO16 here.** With the default the boot log says `driver = Tsic306 … but the probe measured on this board is DallasDs18b20`, and the machine sits in `SENSOR_ERROR` with `currentTemp: NaN`. |
| `hardware.sensors.watertank.enabled` | `true` | The default is `false`, which makes an absent float report the tank **full** so S4 cannot block the pump forever. Needed for runbook §13.3. |
| `hardware.sensors.watertank.mode` | **`0`** — bench only | Matches a breadboard tank switch wired pin → 3V3 with the pin idling low: low = empty, high = full. **The machine uses `1`**, which configures an internal pull-*up* so a cut wire reads empty and blocks the pump. `0` is the unsafe direction for a real float and must not be copied to a machine. |
| `system.wifi.ssid` / `password` | your network | Provisioned with `just wifi-provision`; deliberately preserved by the validator even when the rest of the configuration is discarded. |

Two things that look like misconfiguration and are not:

- **`state 20` vs `state 90` on `/api/status`.** `20` is `PID_NORMAL` and `90` is
  `PID_DISABLED`; the numbers are the enumerator ids, not a count. With the power
  switch disabled a healthy bench sits in `20` with `pidEnabled: true`.
- **`hardware.sensors.scale.enabled` and `hardware.sensors.pressure.enabled` are
  `false`** because nothing is fitted. Leave them; a floating input is worse
  than an absent one.

**Apply and check**, with the tank switch in whichever position you are testing:

```sh
curl -X POST 'http://<host>/api/parameters?pid.enabled=1'
curl -X POST 'http://<host>/api/parameters?hardware.switches.brew.type=0'
curl -X POST 'http://<host>/api/parameters?hardware.switches.power.enabled=false'
curl -X POST 'http://<host>/api/parameters?hardware.sensors.temperature.type=1'
curl -X POST 'http://<host>/api/parameters?hardware.sensors.watertank.enabled=1'
curl -X POST 'http://<host>/api/parameters?hardware.sensors.watertank.mode=0'
curl -X POST 'http://<host>/api/restart'          # both sensor keys are read at boot
curl 'http://<host>/api/parameters?filter=all' | jq '.[]|select(.value != .default)|{name,value}'
```

Then read `the boot decision was `Stored`` in the boot log — **`DiscardedUnsafe`
means the validator threw the whole configuration away** and you are back on
defaults, which is how this bench lost its Dallas probe once already
([`../history/outstanding-findings.md` #12](../history/outstanding-findings.md)).
