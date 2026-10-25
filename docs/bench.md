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

`just bench-provision` without arguments takes the WiFi from `.env` (git-ignored):

```bash
WIFI_SSID=MyNetwork
WIFI_PASS=secret
```

For MQTT, a throwaway broker on the PC (`mqtt.enabled=1 mqtt.broker=<PC address>`):

```bash
docker run --rm -p 1883:1883 eclipse-mosquitto:2 sh -c 'printf "listener 1883\nallow_anonymous true\n" > /m.conf && mosquitto -c /m.conf'
```

`bench-provision` sets the WiFi, hostname `cc-bench`, and the profile below. Every value differs from the default on purpose:

| Parameter | Value | Why |
| --- | --- | --- |
| `pid.enabled` | 1 | PID drives the heater LED |
| `hardware.switches.{brew,steam,hot_water}.enabled` | 1 | the buttons are fitted; switches default to off |
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

`tools/bench` checks: status and the heap low-water mark (≥ 12 KB), the largest response (all parameters) alone and three in parallel, a parameter write and restore, history, config download, JSON 404s, the gzipped UI, server-sent events, eight connections at once (more than there are HTTP workers).

Open UI tabs hold the event-stream slots (two on the home network): the server-sent events check then fails with "slots busy". Close the tabs for a clean run.

On macOS a rebuilt `bench` binary needs the Local Network permission again (System Settings → Privacy & Security → Local Network); until it is granted, `connect` hangs without an error.

## By hand

| Case | Do | Expect |
| --- | --- | --- |
| Heating | boot cold | heater LED blinks at the PID duty, state 20 |
| Brew | tap brew (momentary) | pump + valve LEDs on, heater off for `brew.pid_delay` (10 s); the next tap stops; the target time applies only with `brew.by_time.enabled` |
| Tank interlock | open the tank switch during a brew | pump LED off at once, state 70 |
| Steam | steam button on | setpoint switches to steam, state 51 |
| Sensor loss | unplug the DS18B20 | state 100 after 2 s, all LEDs off |
| OTA interlock | upload firmware during a brew | relays off while writing |
| Panic | `--hang` | device resets within 2 s, LEDs off during the reset |
| Portal | provision a wrong SSID, reset | after 2 min `CleverCoffee-XXXX` opens; joining it opens the UI (macOS: the portal window) within seconds |
| MQTT | broker running, machine cold for 5 min | stays connected (no "exceeded timeout" in the broker log) |
| Floating inputs | remove a pull resistor | the switch reads pressed; why the resistors matter |

Not provable here: relay coils and contacts, relay polarity against your machine's wiring, anything with water.

## Found on the bench

What the first run on a chip (2026-10-23) found; each has a fix and, where the logic is in a crate, a test.

| Symptom | Cause |
| --- | --- |
| RTC watchdog reset loop, `main` never reached | `xtensa-lx-rt`'s `Reset` split over several `global_asm!` blocks that fat LTO reordered (vendored fix) |
| Panic "station interface already taken" | the WiFi station interface taken twice (MAC read) |
| "write to the stack guard" on the first big request | the router's stack frame was 22 KB of the 27 KB main stack |
| Panic "memory allocation failed" under parallel downloads | WiFi driver buffers and per-request copies on a 94 KB heap |
| `ERR_CONNECTION_REFUSED` for the logo or `/events`; the portal loads forever | no listen backlog; one worker on the access point; station scans every 5 s while the portal is open |
| Every request to `cc-bench.local` takes 5 s on macOS | no AAAA answer from the mDNS responder (now an NSEC record says there is none; confirm on the bench) |
| `/api/health` takes 1 s | no `Content-Length`; the client waited for the connection to close |
| Firmware upload fails after 3 s | picoserve's 3 s limit for a whole request body |
| Now and then a download ends with "connection reset" under parallel load | picoserve's 1 s write timeout aborts with a RST; one lost packet takes about that long. 5 s now |
| `--hang` gets no answer, then a read timeout | the hung loop starved the network stack before it sent the reply and the FIN: the route waits 200 ms, `bench` reads by `Content-Length` |
| Flashing over USB has no effect | after an OTA update `otadata` points to the other slot; `just flash` erases it now |
| MQTT reconnects every minute on a cold machine | a publish round without changes was counted as traffic; no ping was sent |
| State passes WATER_TANK_EMPTY at every boot | the tank switch read empty until its first debounce period was over |
| `ds18b20: Crc` now and then during a firmware upload | flash writes stall the 1-Wire bit timing; a reading is dropped, nothing else |
| Panic in the first flash call, a pointer reading 0xA5A5A5A5 | the stack monitor painted below a local's address, but inlined into `main` the local sat kilobytes above the stack pointer: it overwrote `main`'s variables. `paint` is not inlined now |
| `just provision --ssid "My Net"` fails with "not KEY=VALUE" | the recipe split arguments at spaces |
| With two UI tabs open, connections stalled 3–4 s at a time | picoserve kept an idle keep-alive connection for 1 s, then waited up to 3 s for the browser's FIN. `Connection: close` now (ADR 0008) |
| (listener, first version) connections hung until the client timed out | an accepted socket took another client's SYN; requests of clients that had given up held workers. Covered by `test_accepted_socket_takes_only_its_own_client` and the listener tests in the vendored smoltcp |
| In a 4 h soak the idle machine left standby four times | DS18B20 read errors (about 2 % of readings) raised a sensor error, and its recovery went to PID, not back to standby |
| Opening the serial port resets the board | DTR/RTS auto-reset: release RTS before DTR, or use `just monitor` |
