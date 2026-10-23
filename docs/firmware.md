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

Pick `minimal` if you have no display and no MQTT broker, `full` only if a scale or pressure sensor is fitted: Bluetooth costs 64 KB of RAM (ADR 0001). The features behind the profiles (`mqtt`, `display`, `hx711-scale`, `ble-scale`, `pressure`) combine freely:

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

A release `clevercoffee-<profile>.bin` is a merged image: `espflash write-bin --chip esp32 0x0 <file>`.

Later updates: web UI → System → Firmware Update, upload `target/app-<profile>.bin` (`just size <profile>` writes it) or the release `app-<profile>.bin`. The device writes the inactive slot, switches and restarts; heater, pump and valve stay off meanwhile.

## First setup

Either:

- **Portal**: with no WiFi configured the device opens `CleverCoffee-XXXX` (password `CleverCoffee`). Join it; phones open the UI, otherwise go to http://192.168.4.1. Enter SSID and password under System, restart. If the home network is unreachable for 2 minutes, the portal opens again next to the station.
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
| wifi, net ×2, web ×3, dns, dhcp, mdns, mqtt | thread | | network |

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

ESP32: ~320 KB data RAM. Heap: 94 KB reclaimed bootloader RAM, plus 48 KB main RAM without Bluetooth. Main stack: ~57 KB (`standard`), ~27 KB (`full`). The device logs the lowest free heap; `/api/status` reports `freeHeap` and `minFreeHeap`.

## Logs

Serial at 115200 (`just monitor`). Level: parameter `system.log_level`. `system.timing_debug.enabled` logs the slowest control tick every 10 s.
