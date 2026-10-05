# Oracle: the previous Rust firmware (recovered from flash)

**Recovered:** 2026-09-28, before the device was erased.
**Source:** full 4 MB `read_flash` dump of the attached ESP32-DevKitC V4.
**Build:** ESP-IDF v5.5.5, built **Sep 26 2026 22:39:26**, version `v0.0.1-3-gfb0564a-dirty`.
**Status of the source code: gone.** `fb0564a` is not a git object in this repository, no
`Cargo.toml` exists anywhere on this machine, and no remote branch carries it. Only the
compiled binary survives.

> **The raw dump contains Wi-Fi credentials in plaintext NVS and is therefore NOT in this
> repository.** It lives only in a scratch directory outside the project. Everything in
> this document is derived, redacted analysis. Do not commit a flash dump.

> **This document is NORMATIVE, despite living in `history/`.** It is the sole surviving
> derivation of the fail-closed `LOW_TRIGGER` heater rule (§4.1): a relay that is
> energised by an undriven pin cannot be made safe in firmware, so a configuration
> selecting one is **refused outright** rather than handled. The rule itself is enforced
> by `cc_safety::validate_config` and pinned by `crates/cc-safety/tests/safety_paths.rs`;
> this page is where that decision came from. Do not prune it as historical — the folder
> it lives in describes where the knowledge *came from*, not whether it is still load-
> bearing.

---

## 1. Why this document exists

The migration plan assumed the migration started from zero. It does not — a complete,
sophisticated Rust firmware had already run on this board. Its source is unrecoverable, so
this document is the only surviving record of the design decisions it made. It is a
**reference and a parity oracle**, not a codebase.

**The plan is not replaced by this.** Nothing here is reusable as source. But several
designs it settled are better than what the plan independently proposed, and the plan is
revised where they are. See [§6](divergences.md#d06).

---

## 2. Partition table it used

Parsed from the dump at `0x8000`:

| Label | Type | Sub | Offset | Size |
| --- | --- | --- | --- | --- |
| `nvs` | data | nvs | `0x9000` | `0x5000` (20 KB) |
| `otadata` | data | ota | `0xE000` | `0x2000` (8 KB) |
| `app0` | app | ota_0 | `0x10000` | **`0x1C0000` (1,835,008 B)** |
| `app1` | app | ota_1 | `0x1D0000` | **`0x1C0000` (1,835,008 B)** |
| `littlefs` | data | littlefs | `0x390000` | **`0x60000` (393,216 B)** |
| `coredump` | data | coredump | `0x3F0000` | `0x10000` (64 KB) |

Compared with the C++ `partitions_4M.csv`:

| | C++ | Oracle Rust | Change |
| --- | --- | --- | --- |
| app0/app1 | `0x1A0000` (1,703,936) | `0x1C0000` (1,835,008) | **+128 KB each** |
| filesystem | `0xA0000` (655,360) | `0x60000` (393,216) | **−256 KB** |
| nvs / otadata / coredump | — | unchanged | moved to accommodate |

**This is exactly the rebalance the plan's 07 §1 formula produces**, and it is now
*measured* rather than hypothetical: the author independently arrived at +128 KB per app
slot by taking 256 KB out of the filesystem.

**Image size: the app0 image occupied the full 1,835,008 B slot** (the last non-`0xFF` byte
is at the slot boundary, and the tail is a string literal). So a Rust esp-idf image with
`std` on this project is **≥ 1.8 MB**. The plan's "1.5–2.5 MB" estimate was right, and
**154 KB of C++ headroom is nowhere near sufficient.** [07 — Image size
budget](../archive/migration/07-image-size-budget.md) is validated by this measurement.

---

## 3. Crate and module structure (from log tags and strings)

| Log tag | Responsibility |
| --- | --- |
| `cc_firmware` | top-level config + hardware bring-up |
| `cc_firmware::supervisor` | **the safety supervisor** — heartbeat, deadman, TWDT |
| `cc_firmware::web` | HTTP diagnostics + static server, port 80 |
| `cc_firmware::wifi` | Wi-Fi |
| `cc_firmware::serial_wifi` | **UART Wi-Fi provisioning** |
| `cc_firmware::ota` | partition/OTA |
| `cc_firmware::switches` | switch input |
| `esp_idf_svc::*` | stock esp-idf-svc |

The names match the plan's proposed crate set (`cc-firmware`, and a HAL/wifi/ota layer).

### Runtime configuration (from the boot log)

```
cc_firmware: config: FreeFreeRTOS tick 1000 Hz, heater window 1000 ms, interlock 500 ms
cc_firmware: config: nvs (2071 B stored)
cc_firmware: heater interrupt running on GPIO2 (active high), output held off until the supervisor beats
cc_firmware: pump on GPIO27, valve on GPIO17, both asserted off
cc_firmware::supervisor: supervisor: esp_task_wdt_add -> 0 (0 = ESP_OK)
cc_firmware::supervisor: state -> Init
```

- **Heater on GPIO2 via an interrupt, 1000 ms window** — i.e. it reproduced the C++
  **1 Hz / 100-step chopper** (`ProcessState.h:183` `windowSize_ = 1000 ms`), and it did
  **not** move to LEDC. Plan 04 §5 proposes LEDC as an improvement; this is the baseline
  it would change.
- **`interlock 500 ms`** — a periodic interlock re-assert interval. The plan has no such
  timer; it re-asserts actuators every control tick. 500 ms is a *weaker* guarantee.
- **"output held off until the supervisor beats"** — the heater output is not energised
  until the supervisor's first heartbeat. A **latching software gate** in front of the
  heater. The plan has no equivalent; it relies on the watchdog and startup ordering.
- **`esp_task_wdt_add` succeeded** — the supervisor subscribes itself to the TWDT, which
  matches the corrected design in plan 04 §3.4 (the driver is *moved* into the task).

---

## 4. The supervisor — a safety design the plan did not propose

This is the most valuable part of the recovery. Periodically logged:

```
cc_firmware::supervisor: PidDisabled temp=22.62 setpoint=95.0 duty=0ms
    last_window_on=0/100 on_fraction=0.000 deadman=armed events=133
```

Reconstructed fields: `state`, `setpoint_c`, `temperature_c`, `commanded_duty_ms`,
`heartbeat`, `ticks`, `windows`, `last_window_on_ticks`, `observed_on_fraction`,
`deadman_armed`, `deadman_tripped`, plus a periodic `sse-broadcast`.

**A deadman is a heartbeat-based latch: if the supervisor stops beating, the heater is
de-energised.** That is a *stronger* guarantee than the plan's, which relies on a 5 s
watchdog reset to drop the relays. With a deadman the heater drops in the interlock period
(500 ms) rather than at the next reset. Diagnostic endpoints exist for it:
`/debug/hang-supervisor`, `/debug/hang-server`, `/debug/panic`, `/debug/fault`.

### 4.1 Configuration-time safety validation

Strings recovered verbatim (truncated at 200 chars by the extractor):

```
safety.emergency_temp must be greater than steam.setpoint plus safety.emergency_hysteresis,
  or the machine would stop itself whil[e …]
```

```
… A LOW_TRIGGER heater relay cannot be made safe in firmware: an undriven GPIO would
    energise the heater on every reset
```

```
(configuration is unsafe to run: …)  →  discarding all of it and running defaults
refusing to store an unsafe configuration: …
unsafe stored configuration discarded: …
```

**Three behaviours the plan does not have:**

1. **Cross-parameter validation at load**: emergency temperature must exceed the steam
   setpoint plus hysteresis. Otherwise the machine trips itself during normal steam use.
   The plan has no cross-parameter checks.
2. **A `LOW_TRIGGER` heater relay is refused outright**, on the grounds that an undriven
   GPIO during reset would energise the heater. This is a genuinely good safety insight:
   the plan's `hardware.relays.heater.trigger_type` is configurable with
   `HIGH_TRIGGER` default and is *honoured* (04 §5), which means the plan would allow a
   configuration that is unsafe on reset.
3. **Fail-closed on unsafe config**: a stored config that fails validation is discarded
   and defaults are used, and an unsafe config is refused on store. The plan has none of
   this — it would happily run a config that trips emergency stop on every steam shot.

### 4.2 Emergency-stop surface

The C++ log messages are present in the Rust binary, which means behavioural parity was
aimed for:

```
EMERGENCY STOP ACTIVATED - System entering safe mode
All hardware disabled - system in emergency mode
Cannot enable heater  - emergency mode active
Cannot enable pump    - emergency mode active
Cannot open water valve  - emergency mode active
Cannot open steam valve  - emergency mode active
Cannot open solenoid      - emergency mode active
Cannot set heater power   - emergency mode active
Cannot set pump pressure  - emergency mode active
Emergency cleanup completed - hardware is in safe state
Entering safe mode due to system error
Exiting safe mode - system error resolved
Hot water pump timeout - stopping for safety
Performing emergency cleanup of partial hardware initialization...
ISR marked as ready - timer ISR can now safely execute
```

An `EmergencyReport` type was serialised with fields `trip_temperature_c`, `threshold_c`,
`active`. Note the C++ `EmergencyStopManager` has no such report type — this is an
addition.

---

## 5. Interfaces it exposed

### 5.1 REST (50 route strings recovered; 40 are real routes)

C++ parity plus additions:

- **Carried over:** `/api/status` `/api/health` `/api/history` `/api/temperatures`
  `/api/setpoint` `/api/pid` `/api/steam` `/api/backflush` `/api/sleep` `/api/wake`
  `/api/parameters` `/api/parameter-help` `/api/config` `/api/config/download`
  `/api/config/upload` `/api/nvs-debug` `/api/nvs-debug` `/api/factory-reset`
  `/api/restart` `/api/wifi-reset` `/api/scale/tare` `/api/scale/calibration`
  `/api/maintenance/reset-backflush-counter` `/api/ota/status` `/api/ota/firmware`
  `/api/ota/filesystem` `/api/ota/url` `/events`
- **Added:** `/api/wifi` (POST), `/api/wifi/clear` (POST), `/download/coredump`
- **Debug surface (18 routes):** `/debug/panic` `/debug/fault` `/debug/hang-supervisor`
  `/debug/hang-server` `/debug/brew/start` `/debug/brew/stop` `/debug/steam/on`
  `/debug/steam/off` `/debug/flush/on` `/debug/flush/off` `/debug/hotwater/on`
  `/debug/hotwater/off` `/debug/backflush/start` `/debug/backflush/stop`
  `/debug/tank/empty` `/debug/tank/full` `/debug/ignore-sensor` `/debug/use-sensor`
- **MQTT Home Assistant discovery prefixes:** `/binary_sensor/` `/button/` `/number/`
  `/sensor/`

The `.../ui/littlefs` route suggests the **web UI is served from a mounted LittleFS at
`/ui/*` with a fallback**, not purely from an embedded blob — no HTML/JS/CSS filenames were
found in the LittleFS partition, so the bundle is likely embedded with LittleFS used for
user-uploaded content.

### 5.2 UART Wi-Fi provisioning — already implemented

```
cc_firmware::serial_wifi: serial: WiFi commands available.
  `wifi set <ssid>` then the password on the next line, `wifi clear`, `wifi status`, `wifi apply`.
```

**This is option P2 from plan 05 §5** — the UART0 line protocol — and it is done, with the
password read on a **separate following line** rather than in argv (avoiding the
shell-history problem the plan worried about).

It also had a **captive portal** on-device: route strings `/wifisave` `/paramsave`
`/param` `/erase` `/update` `/close` `/restart` `/exit` `/info` `/status` `/wifi` and
`/0wifi` are a portal form flow, not just the API.

### 5.3 Config schema — **the C++ key names were kept**

97 keys recovered, matching the C++ dotted namespace rather than a new `cc.` prefix:

```
pid.enabled  pid.use_ponm  pid.ema_factor
pid.regular.{kp,tn,tv,i_max}   pid.steam.kp   pid.bd.{enabled,kp,tn,tv}
brew.setpoint  brew.temp_offset  brew.mode  brew.pid_delay
brew.by_time.{enabled,target_time}   brew.by_weight.{enabled,target_weight,auto_tare}
brew.pre_infusion.{enabled,time,pause}
steam.setpoint
safety.emergency_temp  safety.emergency_hysteresis      <-- the pair C++ never registered
display.template  display.language  display.inverted  display.blinking.delta
display.fullscreen_brew_timer  display.fullscreen_manual_flush_timer
display.fullscreen_hot_water_timer  display.post_brew_timer_duration
display.heating_logo  display.pid_off_logo
hardware.oled.{enabled,type,address}
hardware.relays.{heater,valve,pump}.trigger_type
hardware.leds.{status,brew,steam}.{enabled,inverted}
hardware.switches.{brew,steam,power,hot_water}.{enabled,type,mode}
hardware.sensors.temperature.type
hardware.sensors.pressure.enabled
hardware.sensors.watertank.{enabled,mode,keep_heater_on_empty}
hardware.sensors.scale.{enabled,samples,type,calibration,calibration2,known_weight}
standby.enabled  standby.time
backflush.{cycles,fill_time,flush_time}
maintenance.backflush_reminder.{enabled,threshold}
mqtt.{enabled,broker,port,username,password,topic}  mqtt.hassio.{enabled,prefix}
system.hostname  system.offline_mode  system.log_level  system.showdisplay.enabled
system.ota_password  system.timing_debug.enabled
system.auth.{enabled,username,password}
system.wifi.{ssid,password}
```

Notes:
- `safety.emergency_temp` and `safety.emergency_hysteresis` **are present and
  persisted** — the exact pair the C++ firmware defines but never registers in
  `getAllConfigParams()` (plan 01 §10 finding #1). The previous Rust author independently
  hit and fixed that bug.
- `display.blinking.delta` exists; `config/reference.md` documents a `display.blinking.mode`
  that does not exist in C++ (plan 01 §10 finding: doc drift).
- No `hardware.switches.hot_water` omission here — all four switches are present.
- Storage was **a single nested JSON blob** in one NVS namespace (2,071 B), not 96 hashed
  scalar keys. That is a cleaner design than the C++ FNV-1a hashing and than plan 04 §6
  assumed.

---

## 6. What the plan should change because of this

| Plan item | Change | Why |
| --- | --- | --- |
| **R1-03 TSIC-306 spike** | **Downgrade.** A DS18B20 is physically fitted (family `0x28`, 11-bit). The previous firmware logged *"config asks for Tsic306 but only the DS18B20 driver exists; reading the 1-Wire bus anyway"* — i.e. it shipped **1-Wire only** and treated TSIC as aspirational. The spike becomes a *config-option* question, not the highest-risk item. | Measured on the board; the previous author shipped without it. |
| **04 §5 heater output** | Keep LEDC as a *proposed improvement*, but record the **1 Hz / 100-step chopper as the parity baseline** it must reproduce first. Moving to LEDC changes the duty semantics. | The oracle reproduced the C++ chopper; a straight LEDC swap is a behaviour change, not a refactor. |
| **`hardware.relays.heater.trigger_type`** | **Add the oracle's rule: refuse `LOW_TRIGGER` for the heater.** An undriven GPIO at reset energises it. This is a real safety hole in the current C++ firmware. | Oracle string recovered verbatim. |
| **`cc-config` storage** | Prefer **one serialised JSON blob** in NVS over 96 hashed scalar keys. | The oracle's 2,071 B blob is simpler, atomic, and versionable. Beats plan 04 §6 and the C++ FNV-1a scheme. |
| **Safety** | Adopt the **deadman heartbeat** and **config-time cross-parameter validation** as requirements, not ideas. | Strictly stronger than the plan's watchdog-only approach; a 5 s watchdog reset is a long time to leave a heater energised. |
| **NVS key namespace** | The user's decision stands (**no backward compatibility**), but the oracle shows the *C++ names are a good schema*. Keep the C++ dotted names for familiarity; the incompatibility is only that C++-written values are ignored. | Better ergonomics; costs nothing since compat is dropped anyway. |
| **07 §1 headroom** | Replace the estimate with the **measured ≥ 1.8 MB** figure, and note the oracle's +128 KB/slot rebalance as the known-good target. | Direct measurement. |
| **Provisioner** | P2 (UART line protocol) is the proven design on this hardware. P1 (softAP portal) also worked. | The oracle implemented both. |

---

## 7. What remains unknown

- **The source is gone.** No design intent, no test suite, no history.
- Flash chip identity: JEDEC `0xD8`/`0x4016`; the exact part is unread (SFDP read failed
  with the bundled esptool 4.11.0). QIO support therefore unverified.
- PSRAM: no evidence in the boot log, but not positively excluded.
- The previous firmware's MQTT/HA payloads and display templates are not recoverable from
  strings; they would need a live capture, which the device no longer has.
- Whether its OLED driver used U8g2-equivalent fonts or a different approach.
