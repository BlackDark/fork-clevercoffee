# Test bench

An ESP32 DevKitC v4 with LEDs and buttons in place of relays and a machine. It proves the firmware and the pins, not the plumbing.

## Parts

| Part | GPIO | Notes |
| --- | --- | --- |
| DS18B20 + 4.7 kΩ pull-up | 16 | The TSIC path shares the pin and is not exercised. |
| SSD1306 128×64 I²C | 21 SDA, 22 SCL | |
| Brew button, **momentary** | 34 | 10 kΩ pull-down, button to 3V3 |
| Steam button, **toggle** | 35 | as above |
| Hot-water button, **toggle** | 36 | as above |
| Tank switch | 23 | to 3V3 = full |
| LED + 330 Ω each | 2 heater, 27 pump, 17 valve | |
| *nothing* | 39 | no power switch: disabled in the profile |

GPIO34–39 have no internal pulls. Unwired or pull-less, they float and read as pressed.

## Setup

```bash
just ui
just flash bench                                  # full + the watchdog test route
just bench-provision <ssid> <password>            # then reset the board
```

`bench-provision` sets the WiFi, hostname `cc-bench`, and the profile below. Every value differs from the default on purpose:

| Parameter | Value | Why |
| --- | --- | --- |
| `pid.enabled` | 1 | PID drives the heater LED |
| `hardware.switches.brew.type` | 0 (momentary) | the bench's brew button springs back |
| `hardware.switches.power.enabled` | 0 | nothing on GPIO39 |
| `hardware.sensors.temperature.type` | 1 (DS18B20) | the probe fitted here |
| `hardware.sensors.watertank.enabled` | 1 | to test the tank interlock |
| `hardware.sensors.watertank.mode` | 0 (normally open) | pin pulled down, high = full. **Machines use 1**: a cut wire then reads empty. |

## Run

```bash
just bench cc-bench.local                     # HTTP checks, ~10 s
just bench cc-bench.local --ota target/app-bench.bin   # + firmware upload and reboot
just bench cc-bench.local --hang              # + watchdog reset (bench build only)
```

`tools/bench` checks: status and heap, the largest response (all parameters) alone and three in parallel, a parameter write and restore, history, config download, JSON 404s, the gzipped UI, server-sent events.

## By hand

| Case | Do | Expect |
| --- | --- | --- |
| Heating | boot cold | heater LED blinks at the PID duty, state 20 |
| Brew | press brew | pump + valve LEDs on; stops at the target time or on release |
| Tank interlock | open the tank switch during a brew | pump LED off at once, state 70 |
| Steam | steam button on | setpoint switches to steam, state 51 |
| Sensor loss | unplug the DS18B20 | state 100 after 2 s, all LEDs off |
| OTA interlock | upload firmware during a brew | relays off while writing |
| Panic | `--hang` | device resets within 2 s, LEDs off during the reset |
| Floating inputs | remove a pull resistor | the switch reads pressed; why the resistors matter |

Not provable here: relay coils and contacts, relay polarity against your machine's wiring, anything with water.
