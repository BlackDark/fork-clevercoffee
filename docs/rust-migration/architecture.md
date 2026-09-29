# Architecture of the Rust firmware

Target: `esp-hal` 1.2.2 bare metal with `embassy`, on the original ESP32, ESP32-S3 and
ESP32-C6. The decision and its alternatives are in
[decision-record.md](decision-record.md); what builds today is in
[compatibility-matrix.md](compatibility-matrix.md).

Cross-links: [inventory.md](inventory.md), [defects-register.md](defects-register.md),
[task-list.md](task-list.md), [config-export-schema.md](config-export-schema.md),
[api-contract.md](api-contract.md), [board-pinouts.md](board-pinouts.md).

---

## 1. Execution model

The C++ firmware runs one unsynchronised `loop()` with a 10 ms hardware-timer ISR for heater
PWM, plus a network task from AsyncTCP. That model has three concrete problems the defects
register documents: the network task and the main loop race on the OTA state and on the OLED
framebuffer (D06, D07), the ISR calls flash-resident code (D05), and an active OTA skips the
state machine and the valve safety check entirely (D01).

The Rust design has **one executor, no second task, and one interrupt handler that only does
arithmetic and a single register write**.

| Activity | Runs on | Why it is concurrent |
| --- | --- | --- |
| Heater PWM | a hardware timer interrupt | Must keep time while the rest of the system is blocked on I2C, Wi-Fi or a flash write. This is the only interrupt-driven activity. |
| Everything else | the single embassy executor | One place to reason about ordering. |

There is no network task. `embassy-net` runs on the executor, so a slow HTTP client cannot
preempt the control loop; the control loop is a task that yields at a bounded period and is
therefore always schedulable. The trade is that any `.await` that blocks for a long time must
be given a timeout. The rule is: **no await in the control path without a timeout**, enforced by
review and by a clippy lint where one fits.

### 1.1 Tasks

| Task | Priority | Period | Owns |
| --- | --- | --- | --- |
| `control` | high | 1 ms tick, 10 ms PWM window | state machine, PID, all actuator commands |
| `sensors` | medium | 400 ms temperature, 50 ms pressure, 200 ms water tank, 100 ms scale | sensor drivers, the moving-average filter |
| `safety` | high | every `control` tick | valve and heater interlocks, emergency stop, watchdog |
| `display` | low | 100 ms render, flush on change | framebuffer, OLED |
| `net` | low | event driven | Wi-Fi, HTTP server, MQTT |
| `provision` | low | event driven | the serial provisioning channel |
| `logger` | low | 30 s heartbeat, flush on demand | ring buffer, USB and UART output |

`control` and `safety` are separate tasks on purpose: the interlocks must not be starved by a
control-path await, and separating them makes the fail-safe property checkable by reading two
short functions instead of auditing a long loop body.

### 1.2 Avoiding races, deadlocks and priority inversion

- **Shared mutable state is owned, not shared.** Each domain object is owned by exactly one
  task. Cross-task communication is a message over an `embassy_sync::channel`, which is
  lock-free and cannot deadlock. No `Arc<Mutex<...>>` crosses a task boundary for control state.
- **Actuator state lives in one place.** A single `Actuators` struct owns the pump, valve,
  heater and solenoid output pins. It exposes only `command(Command)` and `force_off()`. No
  other code holds a pin handle. This is the Rust form of the C++ rule in `CLAUDE.md` that
  relays must never be poked directly, and it is enforced by the type system because the pins
  are moved into the struct at startup.
- **Priority inversion** is avoided by never holding a lock across an `.await`. There are no
  locks in the control path at all.
- **The ISR touches nothing shared.** It reads an `AtomicU32` duty cycle and writes the heater
  pin. It does not call into a driver, does not allocate, and does not touch flash. The C++
  defect D05 is therefore structurally impossible here.

### 1.3 Timing, cancellation, backpressure and the watchdog

- The PWM window is the hardware timer period: 10 ms, matching the C++ firmware, so heater
  behaviour is unchanged.
- The PID sample time stays 1000 ms, matching the C++ firmware.
- Every long operation is a message with a reply channel and a `with_timeout`, so a cancelled
  operation cannot leave a half-applied command.
- Backpressure: the sensor tasks publish into bounded `heapless` channels and drop the oldest
  sample on overflow, which is correct for a periodic reading. The HTTP server applies back
  pressure by refusing new connections when its accept queue is full rather than by buffering
  unbounded request bodies.
- Watchdog: the `safety` task feeds the task watchdog every 50 ms. Because the watchdog feed is
  in its own task, a hang in the control loop, the display or the network still trips it. The
  C++ firmware fed it once at the top of a single loop, so any long block was a false trip and
  a hung loop was a real trip with no distinction. On trip the device resets, and the GPIO
  reset state de-energises the relays; the new design additionally latches actuators off in an
  `on_reset` path where the HAL allows it.

### 1.4 Safe startup and shutdown

Startup is ordered and each step either succeeds or aborts the boot into a safe state:

1. Clocks, then actuators driven to their inactive level **before** anything else is
   configured. The C++ firmware created relays and drove them off late in startup
   (`src/hardware/HardwareManager.cpp:70-93`), after Wi-Fi could already block for 10 seconds.
2. Config region read and CRC-checked. A bad CRC falls back to compiled defaults and logs, it
   does not refuse to boot.
3. Sensors probed. A missing temperature sensor is a **fatal** condition for the heater: the
   heater stays off until a valid reading arrives. This is the direct fix for defect D03, where
   a disconnected sensor was never detected and the PID saturated at 100 percent.
4. Network started last, so a Wi-Fi failure cannot delay or block safety.
5. Provisioning channel opened regardless of network state, so a device with no credentials can
   always be fixed over USB.

Shutdown paths: every state that energizes an actuator de-energizes it in its exit handler, and
`Actuators::force_off()` is callable from the safety task, the provisioning channel and the
panic handler. It is idempotent and safe to call from any state.

### 1.5 Fail-safe actuator states

| Fault | Pump | Valve | Heater |
| --- | --- | --- | --- |
| Temperature sensor missing or CRC-failing | off | closed | off |
| Temperature above the emergency threshold | off | closed | off |
| Water tank empty | off | closed | off unless `keep_heater_on_empty` |
| Watchdog trip | off | closed | off |
| Firmware panic | off | closed | off |
| OTA in progress | off | closed | off, and OTA is refused while brewing |
| Provisioning in progress | off | closed | off |

The OTA row is the direct fix for D01: in the C++ firmware an OTA left the pump and valve
energized because `otaPrepareHardware` only disabled the heater timer.

### 1.6 Testing without hardware

Every crate below the BSP is `no_std` and hardware-free, so it compiles and tests on the host
with `cargo test`. Concretely:

- The state machine, PID, backflush logic, sensor fusion, config schema, config import, HTTP
  handlers and the provisioning protocol parser are all host-tested.
- Actuators and sensors are traits. The host tests use recording fakes that assert the exact
  sequence of actuator commands, which is a stronger assertion than the C++ tests make.
- The display is tested against a host framebuffer with a font-metrics table, so layout
  regressions are caught by asserting pixel content, not by a human reading a screen.
- An on-device **mock actuator mode** is a build feature: `mock-actuators` replaces the real
  `Actuators` with a logger that records every command and never drives a pin. A device flashed
  with that feature can exercise the entire control path, the web API and the config import
  with zero risk of energizing anything. This is the mode any bench test that would otherwise
  drive a load must use.

### 1.7 Board variation

| Layer | Varies by | Mechanism |
| --- | --- | --- |
| Domain logic | nothing | one crate, no chip cfgs |
| HAL traits | nothing | one crate, trait definitions only |
| Drivers | pin numbers and peripheral instances | a `Board` trait, one impl per board |
| BSP | pin map, peripheral selection, clock config | one module per board behind a cargo feature |
| Provisioning transport | UART0 vs USB Serial/JTAG, and on S3/C6 both are present | one `ProvisioningTransport` trait, one impl per transport |
| App | nothing | wires the selected board |

Target selection is a cargo feature: exactly one of `board-esp32`, `board-esp32s3`,
`board-esp32c6`, and exactly one of `prov-uart`, `prov-usb-cdc`. This is a compile error rather
than a runtime branch when a combination is wrong. The pin map for each board is in
[board-pinouts.md](board-pinouts.md), and the C6 map does not currently fit: the project needs 17
pins and the ESP32-C6-DevKitC-1 exposes 16.

---

## 2. Crate layout

Dependency direction is strictly downward. No crate depends on a crate to its right.

```
clevercoffee-domain      pure logic, no I/O, no_std
  ^        ^         ^
  |        |         |
hal-traits        drivers      (hal-traits defines the traits drivers implement)
  ^                 ^
  |                 |
bsp-esp32  bsp-esp32s3  bsp-esp32c6
  ^                 ^
  |                 |
app  (state machine, PID, scheduling, config import, HTTP, provisioning)
  ^
  |
fw (one binary per board, #[main], panic handler, image metadata)
```

| Crate | Contents | Host-testable |
| --- | --- | --- |
| `domain` | `State`, `Transition`, `Pid`, `SensorReading`, `BrewPlan`, `BackflushPlan`, `StandbyPlan`, all the timing constants | yes |
| `hal-traits` | `Actuators`, `TemperatureSensor`, `Display`, `Switch`, `Scale`, `Storage`, `ProvisioningTransport`, `Clock` | yes (compile only) |
| `onewire` | bit-bang transport, timing model, ROM search, CRC-8 | yes, against a simulated bus |
| `ds18b20` | command layer over `onewire` | yes |
| `ds18b20` selection | both sensors are kept; `hardware.sensors.temperature.type` chooses at boot | yes |
| `drivers-hx711` | HX711 driver | yes, against a scripted waveform |
| `drivers-tsic` | TSIC driver | yes |
| `http` | HTTP/1.1 server, routing, SSE, static assets from flash | yes, over an in-memory socket |
| `storage` | versioned config region, CRC, encode and decode | yes |
| `config` | the parameter schema, ranges, defaults, the old-format import mapping | yes |
| `bsp-<board>` | pin maps, peripheral selection, `Actuators` impl, display init, provisioning transport | compile only |
| `app` | tasks, state machine wiring, HTTP routes, provisioning protocol, MQTT | mostly; HAL behind traits |
| `fw` | binaries, panic handler, image metadata | no |

Dependency rules, enforced by review and by a dependency-direction CI check:

- `domain`, `config`, `storage`, `http`, `onewire`, `ds18b20` and the driver crates depend on
  **no** other workspace crate except `hal-traits`.
- `bsp-*` may depend on `hal-traits` and the drivers, never on `app`.
- `app` may depend on everything except `fw`.
- No workspace crate other than `fw` enables a chip feature of `esp-hal`.

The reason `config` is separate from `domain`: the config schema has 96 parameters and the
safety limits on them, and it is the crate that the import validation and the API both depend
on. Keeping it out of `domain` stops the pure logic crate from growing a dependency on the
storage format.

---

## 3. Storage

Designed for the new system alone. Nothing about the C++ NVS layout is carried over.

### 3.1 Partition table

4 MB flash, the same size as the C++ target.

| Name | Type | Subtype | Offset | Size | Purpose |
| --- | --- | --- | --- | --- | --- |
| `nvs` | data | nvs | 0x9000 | 0x5000 | not used by the firmware; kept because the ROM bootloader expects it |
| `otadata` | data | ota | 0xe000 | 0x2000 | two copies of the active OTA slot |
| `ota_0` | app | ota_0 | 0x10000 | 0x180000 | application slot A, 1.5 MB |
| `ota_1` | app | ota_1 | 0x190000 | 0x180000 | application slot B, 1.5 MB |
| `config` | data | 0x40 | 0x310000 | 0x10000 | 64 KB, versioned config region |
| `assets` | data | 0x40 | 0x320000 | 0xD0000 | 832 KB, gzipped web UI |
| `coredump` | data | coredump | 0x3F0000 | 0x10000 | panic dumps |

Generated with `espflash partition-table --to-binary`. The two app slots are 1.5 MB rather than
the C++ firmware's 1.625 MB because the config and asset regions are explicit rather than
sharing one `spiffs` partition, and 1.5 MB is comfortably more than the firmware needs: the
spike binary is a 99 KB app image.

The ESP32 bootloader lives at 0x1000 and the partition table at 0x8000, as with the C++ build.

### 3.2 Config region format

The `config` partition holds two 32 KB slots, A and B, each written whole and alternately.

```
offset  size  field
0       4     magic "CCFG"
4       2     format_version (u16 LE)      = 1
6       2     slot_generation (u16 LE)     monotonically increasing
8       4     payload_length (u32 LE)
12      4     payload_crc32 (u32 LE)       CRC-32 over the payload
16      16    payload_sha256_prefix        first 16 bytes of SHA-256 over the payload
32      N     payload                      postcard-encoded parameter block
32+N    ...   padding to 32768
```

Read algorithm at boot: read both slots, discard any with a bad magic, bad CRC or a payload
length that does not fit, then take the slot with the higher `slot_generation` (with wraparound
comparison). If neither is valid, use compiled defaults and write slot A.

Write algorithm: write the payload to the slot with the lower generation, then read it back and
verify the CRC, then mark it as the newer slot. A power cut mid-write leaves the other slot
intact, so a torn write can never produce a half-applied configuration. This is the direct fix
for the C++ behaviour where an import applies parameters one at a time and reports success if
any one of them matched.

The payload is a versioned, self-describing parameter block: each parameter carries its key, its
value and its schema version, so a future firmware can migrate a region written by an older one
without a migration table keyed on firmware version.

### 3.3 OTA

Two app slots, selected through `otadata`. The C++ firmware's three update paths are all kept,
because the user is keeping OTA; what changes is that each path is **correct** rather than
removed.

| Path | C++ | Rust |
| --- | --- | --- |
| HTTP file upload, `/api/ota/firmware` and `/api/ota/filesystem` | unauthenticated (D17), firmware variant skips the extension check | kept, authenticated when `system.auth.enabled` is set, extension check on both variants, requires the machine to be idle (D01) |
| URL download, `/api/ota/url` | no scheme or host allow-list, so the device fetches any URL reachable from the ESP32 (D16) | kept, but the URL must be `https` or `http` with a host on a small allow-list, and the firmware variant gains the extension check it was missing |
| espota, ArduinoOTA | password-protected | kept, password from `system.ota_password` |
| USB, through the provisioning channel | n/a | new, and the preferred path |

Every update path runs the same sequence:

1. Refuse unless the machine is idle, then `Actuators::force_off()`. This is the fix for D01: the
   C++ firmware accepted an OTA while brewing and left the pump and valve energized.
2. Stream the image into the inactive slot, verifying the image header and a trailing SHA-256.
3. Read the slot back and compare the digest.
4. Switch the boot partition in `otadata`.
5. Reboot.

Two app slots at 1.5 MB is enough for all four paths. The spike binary is a 99 KB app image, and
the largest thing the firmware will hold is the web asset blob, which lives in its own region, so
the app slot is not a constraint. If a future feature pushes the image past the slot, the fix is
to shrink `assets` or to grow both slots on the 8 MB S3 and C6 boards, not to drop OTA.

---

## 4. Web backend

`clevercoffee-http` is a minimal HTTP/1.1 server over `embassy-net` TCP. It supports what the
existing frontend actually uses: GET, POST, PUT, DELETE, OPTIONS, chunked request bodies,
multipart uploads, chunked responses and server-sent events. No TLS; the device is on a local
network and the C++ firmware had SSL disabled
(`platformio.ini:26`).

Routes, request and response payloads and status codes are fixed by
[api-contract.md](api-contract.md), which is derived from the C++ handlers. Where the C++
behaviour is a defect, the Rust implementation returns the corrected status code and the
deviation is listed in [defects-register.md](defects-register.md). The frontend is reused
unchanged, so the wire contract is a compatibility requirement, not a preference.

Static assets are served from the `assets` flash region. The Vite build already emits a `.gz`
sibling for every file, and the C++ server picks the gzip variant unconditionally; the Rust
server negotiates on `Accept-Encoding` and falls back to the plain file, which is a deviation
listed as a defect fix.

Handlers are plain functions from a parsed request to a response, with no access to the network
or the flash. That is what makes the whole API host-testable, which the C++ code never was.

---

## 5. Config import

The import path is the only migration mechanism. Users export JSON from the old UI, flash the
new firmware over USB, and import the JSON over USB.

### 5.1 Accepted inputs

1. The old C++ export: a nested object with the 10 groups `pid`, `brew`, `steam`, `display`,
   `backflush`, `maintenance`, `standby`, `mqtt`, `hardware`, `system`. There is no version
   marker in that format, so the importer identifies it structurally.
2. The same shape with the new system's `safety` group added.
3. The new system's own export, which carries `"format_version": 1` and is self-describing.

### 5.2 Field mapping and validation

For every field the importer resolves the dotted path in the incoming document and looks it up in
the schema table. The table is the single source of truth for the key, the type, the unit, the
default, the minimum, the maximum and whether the field is a secret. See
[config-export-schema.md](config-export-schema.md) for the full list.

Validation happens **before anything is persisted**, and the rules are:

- **Unknown field**: rejected and reported by dotted path. Never silently ignored. The C++
  importer silently ignored unknown fields.
- **Missing field**: the parameter keeps its current value. A missing field is not an error,
  because a partial import is a legitimate operation.
- **Wrong type**: rejected and reported, except for the documented widening cases (an integer
  where a double is expected, a numeric string where a number is expected).
- **Out of range**: the value is **rejected**, and the report says so. The C++ importer rejected
  it too, but continued applying the other parameters and returned HTTP 200. The new importer
  reports every rejection and applies nothing if any field is rejected, unless the caller asks
  for a partial apply.
- **Clamping**: available but **not the default**, and always reported. A silently clamped
  setpoint is a safety problem, so the report must name every clamped field.
- **Field-level overrides of the range rules**, which exist because the C++ ranges are wrong in
  three places and the new firmware follows the config's intent, not the C++ constant:
  `safety.emergency_temp` is accepted and is honoured, which fixes D12;
  `system.log_level` uses the new enum;
  `display.template` accepts the six templates.
- **String length limits** are enforced. The C++ `isValid` returns true for every string and the
  eight length constants in `defaults.h` are referenced nowhere (defect D11 family).
- **Secret fields** (`system.wifi.password`, `mqtt.password`, `system.auth.password`,
  `system.ota_password`) are accepted on import, written to the config region, and **never
  echoed** in any report, log, status response or export with a value. The C++ firmware exported
  all four in plaintext from four different endpoints.

### 5.3 Report

The importer returns a structured report, not a boolean:

```json
{
  "format_version": 1,
  "accepted": 88,
  "rejected": [
    { "path": "brew.setpoint", "reason": "out_of_range", "value": 150, "min": 20, "max": 110 }
  ],
  "clamped": [],
  "unknown": [ { "path": "display.blescale_brew_timer" } ],
  "missing": 8,
  "applied": false
}
```

The C++ importer returned `true` if at least one of 96 parameters matched, and the HTTP layer
turned that into `200 "Configuration validated and applied successfully."` (defect D14).

---

## 6. USB provisioning

The goal: credentials and configuration reach a device with no network, no web UI and no
access-point step, and the device confirms the result without echoing a secret.

### 6.1 Transport

| Board | Transport | Verified |
| --- | --- | --- |
| ESP32 | UART0 through the USB-UART bridge, 115200 8N1. The board has no native USB. | builds (`spikes/stack-smoke`) |
| ESP32-S3 | USB Serial/JTAG CDC, or UART0: the board has both a native port and a bridge | builds (`spikes/usb-smoke`, `spikes/stack-smoke`) |
| ESP32-C6 | USB Serial/JTAG CDC, or UART0: the board has both a native port and a bridge | builds (`spikes/usb-smoke`, `spikes/stack-smoke`) |

Both S3 and C6 expose a UART0 bridge as well as a native USB port, so the firmware supports both
transports on those boards and the user picks by plugging into the right port. See
[board-pinouts.md](board-pinouts.md#6-usb-and-the-provisioning-transport-per-board).

`just wifi <port>` and `just config-import <port> <file>` open the port, speak the line protocol,
read the result line, print a status code, and exit non-zero on failure. They work on a freshly
flashed device and on a running one, because the provisioning task runs from boot and is not
gated on machine state. Provisioning always forces actuators off for the duration, so a
provision attempt can never leave a heater on.

### 6.2 Protocol

Line-oriented, human-readable, no framing, no binary. Every line is `KEY value`, a response is
`OK <code> <token>` or `ERR <code> <reason>`.

```
-> PING
<- OK PONG
-> WIFI SSID <ssid>
<- OK WIFI_ACCEPTED
-> WIFI PASS <password>
<- OK WIFI_SET
-> CONFIG BEGIN <byte-length> <crc32>
<- OK CONFIG_READY
-> CONFIG <chunk of base64>            (repeated)
<- OK CONFIG_CHUNK
-> CONFIG END
<- OK CONFIG_VALIDATED applied=88 rejected=0 clamped=0 unknown=1 missing=8
-> STATUS
<- OK STATUS idle state=PID_NORMAL actuators=pump:off,valve:closed,heater:off
```

Design rules that fall out of this:

- **No secret is ever echoed.** `WIFI PASS` replies `WIFI_SET` with no value. A malformed line
  replies with the reason, never with the payload.
- **Length-prefixed transfers with a CRC**, so a truncated paste is detected rather than
  applied.
- **Validation is on-device**, against the same schema table the API uses, so a config that
  imports over USB is exactly a config that imports over HTTP.
- **The host never logs the secret.** The host script reads `WIFI_PASS` from `.env` inside the
  recipe, writes it to the port, and the process output is a status code only. Nothing
  intermediate is written to disk.

### 6.3 Secret handling

| Property | Decision |
| --- | --- |
| Storage at rest | plaintext in the config flash region; the original ESP32 has no secure storage and the S3 and C6 eFuse would be a separate project |
| NVS encryption | not used; the clean break means no legacy keys to decrypt |
| Recovery | `just factory-reset <port>` clears the config region and reboots |
| Reprovisioning | `just wifi <port>` overwrites the credentials on a running device |
| Host hygiene | `.env` is gitignored; the recipe never echoes a value; no intermediate files |
| Log hygiene | the firmware's log sink redacts any field marked secret in the schema, by construction rather than by remembering |

The lack of encryption at rest is a deliberate, documented limitation, not an oversight. It is
recorded as a problem feature in [task-list.md](task-list.md) with a note that adding NVS
encryption later is possible because the config region is already a single self-describing
blob.

---

## 7. How the design fixes each defect

| Defect | Fix |
| --- | --- |
| D01 OTA leaves actuators energized | OTA is refused unless the machine is idle; `Actuators::force_off()` is called on the OTA path and in the safety task |
| D02 `heaterEnabled_` never set, shutdown paths inert | `Actuators` has no shadow boolean; `heater_off()` always writes the pin |
| D03 disconnected temperature sensor never detected | a reading that fails CRC or is out of range latches `SensorFault`; the heater is inhibited until a valid reading arrives; the fault is a state, not a log line |
| D04 unchecked display pointer | no raw pointers; the display is a trait object owned by one task and absent displays are a `None` variant handled at construction |
| D05 ISR calls flash-resident code | the ISR reads an atomic and writes one pin, both inlined in the interrupt handler |
| D06 OTA state raced between tasks | one executor; the OTA state is owned by the `net` task and is only mutated through messages |
| D07 OTA drew into the shared framebuffer from another task | only the `display` task touches the framebuffer |
| D08 10 ms blocking delay in the pressure read | the pressure driver is a future that starts a conversion and returns; the read happens on the next tick |
| D09 pump run-time limits never armed | the pump command is issued with a deadline that the safety task enforces; a stuck switch cannot extend it |
| D10 auth middleware never enabled | authentication is a first-class check in the router; a mutating route without a valid credential is 401 |
| D11 NVS load skips range validation | the config region is CRC-checked and every field is range-checked on decode, before use |
| D12 emergency parameters never registered | the schema table is the single registry; an unregistered field cannot exist |
| D13 import reports success on partial failure | the importer is transactional and returns a structured report |
| D14 secret fields exported in plaintext | the schema marks secrets; export and every status response redact them |
| D15 `/api/temperatures` returns errors with HTTP 200 | error bodies are only ever sent with a non-2xx status |
| D16 firmware-from-URL has no validation | the URL path is kept but requires an `http`/`https` scheme, a host on an allow-list, and the same extension check the filesystem variant already had |
| D17 OTA is unauthenticated | every OTA path requires the configured password, and refuses to start unless the machine is idle |

| D36 heater relay on a boot-mode strapping pin, driven late | the board map moves it to GPIO4, and actuators are driven inactive before anything else is configured, so the strapping sample is correct |

The remaining entries in [defects-register.md](defects-register.md) are C++-only and are fixed
by construction, by not carrying the pattern over, or are documented as accepted differences.
