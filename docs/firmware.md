# Firmware

Building, flashing, first setup and what runs on the chip.

## Profiles

| Profile | Contents | App size* | Build |
| --- | --- | --- | --- |
| `minimal` | Control, sensors (TSIC/DS18B20), web UI, WiFi, mDNS | 1.15 MB | `just image minimal` |
| `standard` (default) | + MQTT, OLED display | 1.33 MB | `just image` |
| `full` | + HX711 scale, Bluetooth scales, ABP2 pressure sensor | 1.54 MB | `just image full` |
| `bench` | `full` + `POST /api/debug/hang` (watchdog test). Never on a machine. | | `just image bench` |

\* With the web UI embedded; the OTA slot holds 1.875 MB, `just size` fails above 90 %.

Pick `minimal` if you have no display and no MQTT broker, `full` only if a scale or pressure sensor is fitted: a configured Bluetooth scale costs 64 KB of RAM (ADR 0008). The features behind the profiles (`mqtt`, `display`, `hx711-scale`, `ble-scale`, `pressure`) combine freely:

```bash
cd firmware && cargo build --release --no-default-features --features esp32,display
```

Parameters of a part that is not built in stay in the UI but have no effect.

## Flash

First install, or after the C++ firmware (USB, erases settings):

```bash
just ui            # build the web UI that gets embedded
just erase         # only when coming from the C++ firmware
just flash         # build, flash, open the serial monitor (profile: just flash full)
```

`just flash` also erases `otadata`: after an update over the web the bootloader would keep starting the other slot, not the image just flashed.

A release `clevercoffee-<profile>.bin` is a merged image: `espflash write-bin --chip esp32 0x0 <file>`.

Later updates: web UI → System → Firmware Update, upload `target/app-<profile>.bin` (`just size <profile>` writes it) or the release `app-<profile>.bin`. The device writes the inactive slot, switches and restarts; heater, pump and valve stay off meanwhile.

## First setup

Either:

- **Portal**: with no WiFi configured the device opens `CleverCoffee-XXXX` (password `CleverCoffee`). Join it; phones and Macs open the UI, otherwise go to http://192.168.4.1. Enter SSID and password under Configuration → System, save, restart. If the home network is unreachable for 2 minutes, the portal opens again next to the station; the station then retries every 60 s, and not while a client is on the portal (each attempt scans all channels and stalls the portal for seconds). Once the home network is back the device restarts, as soon as no shot, steam or hot water is running, which closes the portal.
- **Provisioning over USB**, before or instead of the portal:

  ```bash
  just provision --ssid Home --password secret system.hostname=silvia mqtt.broker=10.0.0.2
  ```

  Any parameter key works (`/api/config` lists them); the tool rejects unknown keys and bad values. The device applies the record at the next boot, saves it, then erases it.

The machine is then reachable at `http://<hostname>.local`.

## Flash layout (`firmware/partitions.csv`)

| Name | Offset | Size | Use |
| --- | --- | --- | --- |
| nvs, phy_init | 0x9000 | | WiFi driver data |
| otadata | 0xD000 | 8 KB | Active slot |
| ota_0, ota_1 | 0x10000, 0x1F0000 | 1.875 MB each | Firmware (UI included) |
| cc_cfg | 0x3D0000 | 128 KB | Settings: one entry per changed parameter, wear-levelled (ADR 0005) |
| cc_prov | 0x3F0000 | 4 KB | Provisioning record |

## Tasks

| Task | Executor | Period | Does |
| --- | --- | --- | --- |
| control | interrupt, priority 3 | 10 ms | switches, `Machine::tick`, relays, LEDs, watchdog, history |
| tsic / ds18b20 | thread | 20 ms / ~400 ms | temperature |
| hx711, pressure, ble scale | thread | 100 ms / 100 ms / notifications | scale, pressure |
| display | thread | 100 ms | render, flush when changed |
| storage | thread | on change (+2 s) | save settings and shot counter; restarts |
| wifi, net ×2, web ×4 (×2 with a BLE scale, shared by home network and portal), dns, dhcp, mdns, mqtt | thread | | network (ADR 0008) |

GPIO and timing: the TSIC is decoded from edge timestamps taken in the GPIO interrupt; DS18B20 and HX711 bit slots run with interrupts off (≤ 70 µs each).

## Pins (ESP32 DevKitC v4, unchanged from the C++ firmware)

| Function | GPIO | Notes |
| --- | --- | --- |
| Power, brew, steam, hot water switch | 39, 34, 35, 36 | input-only, external pull resistors |
| Temperature (TSIC 306 / DS18B20) | 16 | |
| Water tank switch | 23 | internal pull per mode |
| HX711 data 1, data 2, clock | 32, 25, 33 | |
| Valve, pump, heater relay | 17, 27, 2 | trigger level per parameter |
| Status, brew, steam LED | 26, 19, 1 | GPIO1 is UART TX |
| I²C SDA, SCL (OLED, ABP2) | 21, 22 | 400 kHz |

## RAM

ESP32: ~320 KB data RAM. Heap: 94 KB reclaimed bootloader RAM, plus 80 KB main RAM without the `ble-scale` feature, or the Bluetooth controller's 64 KB when no Bluetooth scale is configured (ADR 0008). Main stack: 41 KB (`minimal`), 33 KB (`standard`), 35 KB (`full`). The device logs each new low of free heap and of never-used stack; `/api/status` reports `freeHeap`, `minFreeHeap` and `minFreeStack`.

Measured on the bench (DevKitC v4, 2026-10-25), with MQTT and Home Assistant discovery on (not in `minimal`), two UI tabs open (Home and Configuration) and `just bench` run 3 times. `minFreeHeap` is the allocator's high-water mark, exact:

| Image | Heap after WiFi start | `minFreeHeap` | `minFreeStack` (of the main stack) |
| --- | --- | --- | --- |
| `minimal` | 140 KB | 76 KB | 25.9 of 41 KB |
| `standard` | 140 KB | 70 KB | 17.4 of 33 KB |
| `full`, no Bluetooth scale | 123 KB | 54 KB | 19.6 of 35 KB |
| `full` with the Bluetooth RAM reserved (as with a scale; 2026-10-23, before ADR 0008, 4 workers) | 44 KB | 7 KB, then a panic | |

Soak (`full`, 2026-10-25): 4 h 20 min without a restart, two UI tabs open, MQTT connected throughout, `just bench` every 3 min for the first 30 min (all passed); free heap steady at 68–69 KB, `minFreeHeap` 53.8 KB. The deepest stack use is at boot (settings, flash, WiFi start), not under load. A firmware upload runs at about 40 KB/s (38–45 s for 1.5 MB). HTTP (ADR 0008): the 21 KB parameter list in 0.25 s, eight connections at once in 0.3–0.6 s with two tabs open.

`tools/bench` fails below 12 KB of heap or 4 KB of stack. A Bluetooth scale has not been measured connected (docs/todo.md).

Control loop (`full`, `system.timing_debug.enabled`): a tick takes ≤ 1 ms; ticks start at most 11 ms apart (10 ms period) under HTTP and MQTT load, 50 ms apart during a firmware upload (flash erases stall the CPU; the relays are off then), about 75 ms once at boot.

## Logs

Serial at 115200 (`just monitor`). Level: parameter `system.log_level`. `system.timing_debug.enabled` logs the slowest control tick and the longest gap between two ticks every 10 s.
