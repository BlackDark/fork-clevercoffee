# Config export schema

The exact shape of the JSON that the old firmware exports and the new firmware imports, with
every field, type, unit, range, default, and how it reconciles with the two JSON files in the
repository. Derived from `include/clevercoffee/Config.h`, `src/Config.cpp`,
`src/ConfigJson.cpp` and `include/clevercoffee/defaults.h`.

Cross-links: [inventory.md](inventory.md), [architecture.md](architecture.md#5-config-import),
[api-contract.md](api-contract.md), [defects-register.md](defects-register.md).

Evidence tag: `repo-verified`, read from the source in this repository. No device was
available, so nothing is `device-verified`.

---

## Top-level shape

The export is a **nested** object with exactly ten top-level groups. There is no schema version
marker anywhere in the C++ firmware; the only marker is a `_seeded` boolean in NVS that records
that seeding happened once and says nothing about the shape.

```
{
  "pid": {...}, "brew": {...}, "steam": {...}, "display": {...}, "backflush": {...},
  "maintenance": {...}, "standby": {...}, "mqtt": {...}, "hardware": {...}, "system": {...}
}
```

`safety` is **absent** from the export even though the C++ code defines and consumes
`safety.emergency_temp` and `safety.emergency_hysteresis`; those two are never registered
(defect D12). The Rust export adds a `safety` group and a `format_version` integer.

Flat dotted keys are rejected by the C++ importer at the top level only
(`src/ConfigJson.cpp:45-49`), so `{"pid": {"regular.kp": 5}}` passes the flat-key check and is
then silently ignored. The Rust importer rejects dotted keys at any depth.

## Field table

`Enf.` is whether the C++ range is actually enforced. It is enforced on the `set()`, `fromString`
and import path, and **not** enforced on the NVS load path for any numeric field
(defect D11).

### `pid`

| Path | Type | Unit | Default | Min | Max | Enf. |
| --- | --- | --- | --- | --- | --- | --- |
| `pid.enabled` | bool | | false | | | n/a |
| `pid.use_ponm` | bool | | false | | | n/a |
| `pid.ema_factor` | double | 0-1 | 0.6 | 0.0 | 1.0 | no |
| `pid.regular.kp` | double | | 62.0 | 0.0 | 200.0 | no |
| `pid.regular.tn` | double | s | 52.0 | 0.0 | 200.0 | no |
| `pid.regular.tv` | double | s | 11.5 | 0.0 | 200.0 | no |
| `pid.regular.i_max` | double | % | 55.0 | 0.0 | 100.0 | no |
| `pid.steam.kp` | double | | 150.0 | 0.0 | 500.0 | no |
| `pid.bd.enabled` | bool | | false | | | n/a |
| `pid.bd.kp` | double | | 50.0 | 0.0 | 200.0 | no |
| `pid.bd.tn` | double | s | 0.0 | 0.0 | 200.0 | no |
| `pid.bd.tv` | double | s | 20.0 | 0.0 | 200.0 | no |

`pid_delay` lives under `brew` in C++. `ProcessController` also hardcodes the integrator limit
to 0-55 at initialisation (`src/core/SystemInitializer.cpp:553`) while `pid.regular.i_max`
defaults to 55 and `config.json` sets 75, so the configured value is ignored at startup. The
Rust firmware derives the limit from the schema.

### `brew`

| Path | Type | Unit | Default | Min | Max | Enf. |
| --- | --- | --- | --- | --- | --- | --- |
| `brew.setpoint` | double | degC | 95.0 | 20.0 | 110.0 | no |
| `brew.temp_offset` | double | degC | 0.0 | 0.0 | 20.0 | no |
| `brew.pid_delay` | double | s | 10.0 | 0.0 | 60.0 | no |
| `brew.mode` | enum | | 0 manual | 0,1 | 0,1 | no |
| `brew.by_time.enabled` | bool | | false | | | n/a |
| `brew.by_time.target_time` | double | s | 25.0 | 1.0 | 120.0 | no |
| `brew.by_weight.enabled` | bool | | false | | | n/a |
| `brew.by_weight.target_weight` | double | g | 36.0 | 0.0 | 500.0 | no |
| `brew.by_weight.auto_tare` | bool | | false | | | n/a |
| `brew.pre_infusion.enabled` | bool | | false | | | n/a |
| `brew.pre_infusion.time` | double | s | 2.0 | 0.0 | 60.0 | no |
| `brew.pre_infusion.pause` | double | s | 5.0 | 0.0 | 60.0 | no |

### `steam`, `display`, `backflush`, `maintenance`, `standby`

| Path | Type | Unit | Default | Min | Max | Enf. |
| --- | --- | --- | --- | --- | --- | --- |
| `steam.setpoint` | double | degC | 120.0 | 100.0 | 140.0 | no |
| `display.template` | enum | | 0 standard | 0-5 | 0-5 | no |
| `display.inverted` | bool | | false | | | n/a |
| `display.language` | enum | | 0 english | 0-2 | 0-2 | no |
| `display.fullscreen_brew_timer` | bool | | false | | | n/a |
| `display.fullscreen_manual_flush_timer` | bool | | false | | | n/a |
| `display.fullscreen_hot_water_timer` | bool | | false | | | n/a |
| `display.post_brew_timer_duration` | double | s | 3.0 | 0.0 | 60.0 | no |
| `display.heating_logo` | bool | | true | | | n/a |
| `display.pid_off_logo` | bool | | true | | | n/a |
| `display.blinking.delta` | double | degC | 0.3 | 0.2 | 10.0 | no |
| `backflush.cycles` | int | | 5 | 2 | 20 | no |
| `backflush.fill_time` | double | s | 5.0 | 3.0 | 10.0 | no |
| `backflush.flush_time` | double | s | 10.0 | 5.0 | 20.0 | no |
| `maintenance.backflush_reminder.enabled` | bool | | true | | | n/a |
| `maintenance.backflush_reminder.threshold` | int | shots | 50 | 1 | 500 | no |
| `standby.enabled` | bool | | false | | | n/a |
| `standby.time` | double | min | 35.0 | 1.0 | 120.0 | no |

`display.language` value 1 renders German in the C++ code: the language table is a chain of
`if english / else if spanish / else german`, so any unrecognised value silently renders German
(`include/clevercoffee/display/languages.h:131-132`). The Rust schema is an enum, so an
unrecognised value is a rejected import.

### `hardware`

| Path | Type | Default | Min | Max | Enf. |
| --- | --- | --- | --- | --- | --- |
| `hardware.oled.enabled` | bool | true | | | n/a |
| `hardware.oled.type` | enum | 0 ssd1306 | 0,1 | 0,1 | no |
| `hardware.oled.address` | enum | 0 (0x3C) | 0,1 | 0,1 | no |
| `hardware.relays.heater.trigger_type` | enum | 1 high | 0,1 | 0,1 | no |
| `hardware.relays.valve.trigger_type` | enum | 1 high | 0,1 | 0,1 | no |
| `hardware.relays.pump.trigger_type` | enum | 1 high | 0,1 | 0,1 | no |
| `hardware.switches.brew.enabled` | bool | false | | | n/a |
| `hardware.switches.brew.type` | enum | 1 toggle | 0,1 | 0,1 | no |
| `hardware.switches.brew.mode` | enum | 0 n.o. | 0,1 | 0,1 | no |
| `hardware.switches.steam.*` | bool/enum/enum | false/1/0 | | | no |
| `hardware.switches.power.*` | bool/enum/enum | false/1/0 | | | no |
| `hardware.switches.hot_water.*` | bool/enum/enum | false/1/0 | | | no |
| `hardware.leds.status.enabled` / `.inverted` | bool | false | | | n/a |
| `hardware.leds.brew.enabled` / `.inverted` | bool | false | | | n/a |
| `hardware.leds.steam.enabled` / `.inverted` | bool | false | | | n/a |
| `hardware.sensors.temperature.type` | enum | 0 tsic306 | 0,1 | 0,1 | no |
| `hardware.sensors.pressure.enabled` | bool | false | | | n/a |
| `hardware.sensors.watertank.enabled` | bool | false | | | n/a |
| `hardware.sensors.watertank.mode` | enum | 1 n.c. | 0,1 | 0,1 | no |
| `hardware.sensors.watertank.keep_heater_on_empty` | bool | false | | | n/a |
| `hardware.sensors.scale.enabled` | bool | false | | | n/a |
| `hardware.sensors.scale.samples` | int | 2 | 1 | 20 | no |
| `hardware.sensors.scale.type` | enum | 0 hx711_dual | 0-2 | 0-2 | no |
| `hardware.sensors.scale.calibration` | double | 1.00 | -999999 | 999999 | no |
| `hardware.sensors.scale.calibration2` | double | 1.00 | -999999 | 999999 | no |
| `hardware.sensors.scale.known_weight` | double | 267.00 | 1.0 | 2000.0 | no |

`calibration` is a **divisor** (`Config.h:1151`), so a negative value inverts the sign of every
reading. The range permits it. The Rust schema rejects a negative calibration outright and says
why.

`hardware.sensors.scale.enabled` set to true does nothing in the C++ firmware, because the scale
is never constructed (D30).

### `mqtt`

| Path | Type | Default | Max length | Enforced |
| --- | --- | --- | --- | --- |
| `mqtt.enabled` | bool | false | | n/a |
| `mqtt.broker` | string | "" | 64 | no |
| `mqtt.port` | int | 1883 | 65535 | no |
| `mqtt.username` | string | "rancilio" | 32 | no |
| `mqtt.password` | string **secret** | "silvia" | 64 | no |
| `mqtt.topic` | string | "custom/kitchen/" | 48 | no |
| `mqtt.hassio.enabled` | bool | false | | n/a |
| `mqtt.hassio.prefix` | string | "homeassistant" | 24 | no |

No length limit is enforced anywhere in C++ (D23). `mqtt.password` defaults to `silvia`, a
published default in the repository, which is a credential in shipped firmware.

### `system`

| Path | Type | Default | Max length | Enforced |
| --- | --- | --- | --- | --- |
| `system.hostname` | string | "silvia" | 64 | no |
| `system.ota_password` | string **secret** | "otapass" | | no |
| `system.offline_mode` | bool | false | | n/a |
| `system.log_level` | enum | 2 info | 0-6 | no |
| `system.auth.enabled` | bool | false | | n/a |
| `system.auth.username` | string | "admin" | 32 | no |
| `system.auth.password` | string **secret** | "admin" | 64 | no |
| `system.wifi.ssid` | string | "" | 32 | no |
| `system.wifi.password` | string **secret** | "" | 64 | no |
| `system.timing_debug.enabled` | bool | false | | n/a |
| `system.showdisplay.enabled` | bool | true | | n/a |

### `safety` (C++ defines it, never exports it)

| Path | Type | Unit | Default | Min | Max |
| --- | --- | --- | --- | --- | --- |
| `safety.emergency_temp` | double | degC | 150.0 | 120.0 | 180.0 |
| `safety.emergency_hysteresis` | double | degC | 5.0 | 1.0 | 15.0 |

Consumed by `src/control/EmergencyStopManager.cpp:18-19`, absent from `getAllConfigParams()`
(`src/Config.cpp:450-453`), therefore never persisted, exported, imported or returned by
`/api/parameters`, and always the compiled default at runtime (D12).

## Import semantics in the C++ firmware, and what changes

| Case | C++ behaviour | Rust behaviour |
| --- | --- | --- |
| Unknown field | silently ignored | rejected and reported by dotted path |
| Missing field | skipped, previous value kept | previous value kept, and counted as missing in the report |
| Explicit `null` | same as missing | same as missing |
| Out of range | rejected, previous value kept, other fields still applied, HTTP 200 | rejected; nothing applies unless `?mode=partial` |
| Wrong type, number for bool | coerced through `fromString` | rejected, except documented widenings |
| Wrong type, number for double | routed through Arduino `String(double)`, which truncates to 2 decimals (D27) | parsed as a typed number |
| Wrong type, string for int | `toInt()` returns 0, then range-checked | rejected |
| Flat dotted keys | rejected at the top level only | rejected at any depth |
| `{"value": x}` wrapper | accepted | accepted |
| Partial success | `updatedCount > 0` means success | explicit mode, and the report always says what happened |

## Reconciliation with the repository's JSON files

`config.json` at the repository root and `docs/example_config.json` disagree with each other and
with the code. All of these are import-time facts: the Rust importer rejects a field it does not
recognise, so a file containing `display.blescale_brew_timer` will be **rejected**, where the C++
firmware accepted it and ignored it.

| Finding | Where | Detail |
| --- | --- | --- |
| `display.blescale_brew_timer` does not exist | `docs/example_config.json` | documented in `CONFIG_REFERENCE.md` too. Rejected on import. |
| `display.blinking.mode` does not exist | `CONFIG_REFERENCE.md` | only `display.blinking.delta` exists |
| `maintenance.backflush_reminder.*` missing from `CONFIG_REFERENCE.md` | | exists in code and is registered |
| `safety.*` missing from `CONFIG_REFERENCE.md` | | exists in code, unregistered |
| The two files select **different temperature sensors** | `config.json` says `type: 0` (TSIC 306); `docs/example_config.json` says `type: 1` (DS18B20) | a real behavioural difference |
| The two files select **different scale types** | `config.json` says `0` (dual); `docs/example_config.json` says `1` (single) | |
| The two files give **different scale calibrations** | `config.json` `-1750.05` / `-1685.21`; `docs/example_config.json` `1000.0` / `1000.0` | the example's 1000.0 is 1000x the default of 1.0 |
| The two files give **different known weights** | `config.json` 456; `docs/example_config.json` 250; code default 267.0 | |
| PID ranges in `CONFIG_REFERENCE.md` say 0-999 | code says 200 for kp/tn/tv, 100 for `i_max`, 500 for `pid.steam.kp` | the code is authoritative |
| `system.log_level` documented 0-5 with 5 = CRITICAL | code has 6 values with 5 = FATAL, 6 = SILENT | |
| `display.template` documented 0-4 | code has six templates | |
| `hardware.oled.address` documented as "default 60, range 0-255" | an enum index, 0 or 1, labels `0x3C` and `0x3D` | |
| String length limits in `CONFIG_REFERENCE.md` disagree with `defaults.h` | 253 vs 64 for `mqtt.broker`, 64 vs 32 for `mqtt.username`, 180 vs 48 for `mqtt.topic`, 64 vs 24 for the HA prefix, 32 vs 64 for the hostname | neither set is enforced |
| `mqtt` block in `docs/example_config.json` has only `enabled` | the other seven keys are absent and silently skipped, leaving whatever is in NVS | reported as missing |
| The root `config.json` is not the LittleFS seed file | `seedFromLittleFS` reads `/config.json` from LittleFS (`src/Config.cpp:283`), and LittleFS images are built from `data/`, which is gitignored and absent | the root file is a sample only |
| Two parameters share `order` 1103 and two share 203 | `Config.h:1344,1350,817,835` | ordering is non-deterministic for those four (D41) |
| `showCondition` is defined and never consulted | `Config.h:1292` | dead |

## Schema field set the Rust firmware adds

| Path | Type | Default | Notes |
| --- | --- | --- | --- |
| `format_version` | int | 1 | top level, in the export and accepted in an import |
| `safety.emergency_temp` | double | 150.0 | now registered, now enforced (D12) |
| `safety.emergency_hysteresis` | double | 5.0 | now registered, now enforced |
| `hardware.board` | enum | `esp32` | selects the board profile; the three targets have different pin maps |
| `hardware.sensor.nominal_resistance` | double | 0.0 | only if TSIC is kept; not ported otherwise |

Everything else is dropped: `system.ota_password` is retained because it still gates the OTA
password on the USB console, but the HTTP OTA routes are gone, so it no longer protects a network
surface.
