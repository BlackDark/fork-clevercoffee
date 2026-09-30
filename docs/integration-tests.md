# Manual Integration Test Checklist

Pre-release validation. Every item must pass before merging to main or tagging a release.

## Prerequisites

- Device flashed with the build under test (USB or OTA)
- Device connected to WiFi and reachable at its hostname. The Rust firmware
  defaults to **`test-cc-rust`** (`cc_config::schema::DEFAULT_HOSTNAME`); the C++
  firmware defaults to `silvia`. The name distinguishes the two firmwares, which
  share a network during the migration.
- Serial monitor available (USB) **or** telnet client for WiFi logging

## 1. Build & Unit Tests

- [ ] `pio run -e esp32_usb` — firmware compiles without errors
- [ ] `pio test -e native_test` — all native tests pass (280/280 or current count)
- [ ] `pio run --target format -e esp32_usb` — no formatting changes

## 2. OTA Update

All three update paths must be exercised — they use independent code paths and
have each broken separately before.

### 2a. ArduinoOTA / espota (`pio`)

- [ ] `pio run -e esp32_ota -t upload` completes at 100% with "Result: OK"
- [ ] Device reboots and responds to `/api/health` within 15s after OTA
- [ ] Serial log shows **no** `task_wdt: Task watchdog got triggered` during the
      transfer. The transfer blocks `loop()` for ~25s, so the Task Watchdog must
      be suspended for its whole duration (regression: espota died at ~18% with
      `OTA_RECEIVE_ERROR` because the watchdog rebooted the device mid-flash).

### 2b. HTTP firmware upload (`/api/ota/firmware`, used by the web UI)

```sh
curl -w "\n%{http_code}\n" -X POST http://<ip>/api/ota/firmware \
  -F "firmware=@.pio/build/esp32_usb/firmware.bin;filename=firmware.bin"
```

- [ ] Responds **200** with `{"success": true, ...}` (regression: returned 400
      "No firmware file provided" because the request callback overwrote the
      upload callback's response — the request callback runs *after* the body)
- [ ] Device reboots on its own within ~20s (regression: the scheduled restart
      never fired because it was only polled while an OTA was active)
- [ ] Rejections return the correct status and leave `/api/ota/status` at
      `idle`, not `error`:
  - [ ] no file at all → 400 "No firmware file provided"
  - [ ] `filename=bad.txt` → 400 "Invalid firmware file. Expected .bin extension."
  - [ ] `/api/ota/filesystem` with `bad.txt` → 400 "Invalid filesystem file..."

### 2c. URL-based update (`/api/ota/url`)

```sh
(cd .pio/build/esp32_usb && python3 -m http.server 8765 &)
curl -w "\n%{http_code}\n" -X POST http://<ip>/api/ota/url \
  -d "url=http://<host-ip>:8765/firmware.bin&type=firmware"
```

- [ ] Responds **202** immediately, before the download finishes (regression:
      the download ran inside the async web handler, starving AsyncTCP so the
      client got no response at all / broken pipe)
- [ ] `/api/ota/status` reports rising `progress` with `status: "downloading"`
- [ ] Device reboots and comes back; status returns to `idle`

## 3. USB Serial Logging

- [ ] `pio device monitor -e esp32_usb` shows boot log lines (WiFi connect, state transitions)
- [ ] Log lines appear at INFO level during normal operation (e.g. temperature readings, state changes)
- [ ] Log level filtering works (DEBUG messages hidden at INFO level)

## 4. WiFi Telnet Logging

- [ ] `nc <hostname> 23` connects and shows "CleverCoffee log stream connected"
- [ ] Log lines appear when activity occurs (API calls, state changes)
- [ ] Idle machine at INFO level = quiet telnet is expected (not a bug)
- [ ] Telnet disconnect/reconnect works cleanly

## 5. Web API Endpoints

Test each with `curl -s http://<hostname>/<endpoint>` and verify non-empty valid JSON response:

- [ ] `GET /api/health` — HTTP 200
- [ ] `GET /api/status` — JSON with temperature, setpoint, machineState, uptime
- [ ] `GET /api/parameters?filter=all` — full parameter list (~19KB JSON array)
- [ ] `GET /api/config` — current config JSON
- [ ] `GET /api/history` — temperature history with currentTemps/targetTemps/heaterPowers arrays
- [ ] `GET /api/temperatures` — current temperature reading
- [ ] `GET /api/nvs-debug` — NVS metadata and parameters

### 5a. Writing a parameter (`POST /api/parameters`)

The one endpoint that changes the machine. **Both encodings are accepted and are the
same request** — `WebServerManager.cpp:823` iterates `request->params()`, which is the
query string and the body together:

```sh
curl -s -X POST "http://<host>/api/parameters?pid.regular.kp=2.25"
curl -s -X POST http://<host>/api/parameters --data "pid.regular.kp=2.25"
```

All four parameter kinds, one request:

- [ ] `POST /api/parameters` with a **bool**, an **int**, a **float** and a **text**
      parameter → `200 {"success":true,"message":"Parameters updated and saved"}`
- [ ] The serial log carries `config: N parameter(s) written: Applied { updated: N, failed: [] }`
      and `config: the configuration was persisted`
- [ ] **Reboot, then** `GET /api/parameters` — every one of the four has the new value.
      This is the persistence check and it is the only one that counts: the `GET`
      reads the configuration as it was **loaded at boot**, so immediately after a
      `POST` it still reports the old value. That asymmetry is recorded, not a bug
      (see `main.rs`, "the control task holds the authoritative value the HTTP
      server does *not* see").

The four failure modes, which are the C++'s (`:829-878`) and answer with one `400`
body each:

- [ ] An **unknown key** → `400 {"error":"Some parameter updates failed"}`, nothing written
- [ ] A value that **does not parse** (`pid.regular.kp=hello`) → `400`. The C++ writes
      **0** here and answers `200`; see `09-cpp-findings.md` §25
- [ ] A value **out of range** (`standby.time=1234.5`, bounds 1..120) → `400`
- [ ] A request naming **no** parameter, or only valueless fields → `200
      {"success":true,"message":"No parameters updated"}`
- [ ] **One bad key among good ones** → `400`, and the serial log names the offender
      (`rejected pid.regular.kp: value rejected: wrong type`) while the good ones are
      still written. The C++ does not roll back either (`:829-865`)
- [ ] A request with more than 64 pairs → `400 {"error":"too many parameters in one
      request"}`. A deliberate bound the C++ does not have (it will iterate ten
      thousand); a body over 1024 B is truncated at that, and the query string is
      bounded by `CONFIG_HTTPD_MAX_URI_LEN` (512 B)
- [ ] `PUT /api/parameters` → `405`. The status is ESP-IDF's own, not a handler of
      ours, so the body is ESP-IDF's rather than the C++'s `{"error":"Method not
      allowed"}`

### 5b. The command endpoints take a query string

`POST /api/pid?on=0`, `?on=1` — the spelling every script and the React UI use. The
C++'s `POST /api/pid` reads **no** field at all and is a pure toggle
(`WebServerManager.cpp:462-479`); there is no C++ answer for `?on=0` to match, so
both encodings are accepted and the body wins if both are present.

- [ ] `POST /api/pid?on=1` → `202`, and `/api/status` reports `pidEnabled: true`
- [ ] `POST /api/pid?on=0` → `202`, `pidEnabled: false`
- [ ] `POST /api/setpoint?value=95` → `202`; the setpoint changes
- [ ] `POST /api/steam` with **no field at all** → `200`, and it **toggles** (the C++'s
      semantics, `WebServerManager.cpp:444`). Repeat and confirm `steamMode` alternates.
      `/api/status` must follow.
- [ ] `POST /api/pid` with no field at all → `200 {"success":true,"pidEnabled":<flipped>}`.
      ⚠ **Not** `400` — that was the bug: the handler used to demand a `value` field the
      C++ never reads and the UI never sends.
- [ ] `POST /api/backflush` with no field → `200`, toggles backflush mode
- [ ] The explicit forms still work: `POST /api/pid?on=0` → `{"pidEnabled":false}`,
      `?on=1` → `{"pidEnabled":true}`, and body `value=0` likewise
- [ ] `POST /api/parameters` with `pid.enabled=0` → `200`, and `/api/status` `pidEnabled`
      becomes `false` **without a reboot**. A `200` that does not change the running
      machine is the Bug-2 failure; check `/api/status`, not just the status code.
- [ ] `POST /api/parameters` with a boot-only parameter (e.g.
      `hardware.switches.brew.enabled=1`) → `200` **with** `"requiresReboot":true` and the
      offending key named. A plain `{"success":true}` for that key is the old lie.

### 5c. The operator switches

⚠ **`hardware.switches.*.enabled` now defaults to `true` in the Rust firmware and `false`
in the C++** (`Config.h:985,1004,1023,1042`). Changed on request 2026-09-30; the reasoning
and the risk are in `intentional-diffs.md` §13. The short version: the human pressed the
switches and nothing happened, because a disabled switch's edges are read and discarded.

⚠ **GPIO 34/35/36/39 are input-only with no internal pull** (`switches.rs` has the full
argument), so an enabled switch on an unwired pin is a *floating* input the debouncer will
eventually settle as **pressed** — and a settled brew-switch press starts a brew. The pull
is `Pull::Floating` on purpose (`OPERATOR_PULL`); do not "fix" it to `Pull::Down`, which
ESP-IDF accepts on GPIO34 and silently ignores.

- [ ] Boot log lists all four switches as `enabled true` (no "reducer will IGNORE" suffix)
- [ ] **First: confirm each switch's settled resting level in the boot log.** A switch that
      settles **high** with nothing touching it is a missing external pull — fix the
      wiring or set its flag back to `false` before going further. This is the one check
      that can prevent a brew starting on its own.
- [ ] Press the physical brew switch. **This step needs a hand** — no automated check can
      substitute for it. Watch `/api/status` (`brewing`) and the serial log
- [ ] The same for `steam`, `power` and `hot_water`
- [ ] To disable one without reflashing:
      `POST /api/parameters hardware.switches.brew.enabled=0`, then reboot

## 5d. The event stream (`GET /events`)

⚠ **This is the check that catches the two-responses bug**, which is invisible to `curl`
and obvious to a browser. It must be done at the socket level.

- [ ] Exactly **one** `HTTP/1.1 200` on the connection. Two is the bug: an empty
      `Content-Length: 0` response followed by the real chunked one.
- [ ] `Transfer-Encoding: chunked` is present and **`Content-Length` is absent**
- [ ] Bytes keep arriving for **minutes** (a 200 s capture is enough) — the connection must
      not close and must not go quiet
- [ ] The first frame is `event: hello` / `data: {"connected":true}`
      (`WebServerManager.cpp:308-317`)
- [ ] A browser opens the UI and the console shows **no** "cannot connect" / "connection
      lost while the page was loading"
- [ ] With one `/events` client open, `GET /api/parameters?filter=all` still answers in
      well under a second. The httpd is one task; a handler that does not return is a
      server that does not serve.

Raw capture (no extra dependency):

```sh
python3 - <<'PY'
import socket, time, re
s = socket.create_connection(("<host>", 80), timeout=10)
s.sendall(b"GET /events HTTP/1.1\r\nHost: h\r\nAccept: text/event-stream\r\n\r\n")
s.settimeout(30); buf = b""; end = time.time() + 30
while time.time() < end:
    try: c = s.recv(4096)
    except socket.timeout: break
    if not c: print("SERVER CLOSED"); break
    buf += c
print("status lines:", len(re.findall(rb"HTTP/1\.1 \d{3}", buf)))
print("Content-Length present:", b"Content-Length" in buf)
print("chunked present:", b"Transfer-Encoding: chunked" in buf)
print("bytes:", len(buf))
PY
```

## 5e. OTA (deferred — the routes must still answer)

OTA is **not implemented** (R3-15). These check that the UI's OTA tab gets an honest
answer instead of a `404`, which would look like a lost feature.

- [ ] `GET /api/ota/status` → `200`, with `status`, `progress` and `updateInProgress`
      present (`OtaStatusSchema` requires all three), and `message`/`reason` naming R3-15
- [ ] `GET /api/ota/status` carries **no** `error` key — "never built" is not "failed"
- [ ] `POST /api/ota/firmware` → `501` + `{"error":"OTA is not available in this build"}`
- [ ] The same for `/api/ota/filesystem` and `/api/ota/url`
- [ ] The OTA page renders in the browser without a console error

## 6. Web UI

- [ ] `GET /ui/` — serves SPA (HTTP 200, HTML content)
- [ ] `GET /ui/config/behavior` — serves SPA (HTTP 200)
- [ ] UI loads fully in browser without console errors
- [ ] UI displays current temperature and machine state

## 7. Concurrent Load (Telnet + API)

This tests the critical OOM scenario that caused crashes.

- [ ] Connect telnet: `nc <hostname> 23`
- [ ] With telnet open, load UI page in browser — device must not crash
- [ ] With telnet open, hit 5+ API endpoints in rapid succession — all return HTTP 200
- [ ] After load burst, `/api/health` still responds with HTTP 200
- [ ] Repeat after fresh reboot (boot window is the most fragile period)

## 8. Stability

- [ ] Device does not crash/reboot during 5 minutes of idle operation
- [ ] No `abort()`, watchdog reset, or stack overflow in serial output
- [ ] Free heap stays above 50KB during normal operation (`/api/nvs-debug` → `free_heap` field)

## 9. Temperature Sensor Robustness (TSIC)

A single out-of-range TSIC sample must never trip emergency stop or flood the log.

- [ ] During idle/heating, logs do NOT continuously repeat `Temperature not stable`
      (the change-rate stabilisation must latch within the first few readings)
- [ ] A transient bad reading is logged once as `Temperature reading out of range, ignoring: …`
      and does NOT produce an `Emergency: Invalid temperature reading` entry
- [ ] If emergency does trigger (sustained overtemp/fault), recovery returns to
      `PID Normal` (state 20), not `PID disabled` — see ADR 0002 / emergency recovery fix

## 10. Water Tank Sensor, Pump Safety & MQTT/Home Assistant Export

- [ ] With `hardware.sensors.watertank.enabled=true` and the sensor disconnected/dry,
      the machine transitions to `Water Tank Empty` state and the pump refuses to
      start (`Cannot enable pump - water tank is empty` in logs)
- [ ] If the pump was running when the tank goes empty, it stops immediately
      (`Water tank is empty - pump operations disabled` in logs)
- [ ] With `hardware.sensors.watertank.keep_heater_on_empty=false` (default), the
      heater/PID turns off when the tank goes empty
- [ ] With `hardware.sensors.watertank.keep_heater_on_empty=true`, the heater/PID
      continues to run (temperature keeps climbing toward setpoint) while the tank
      is empty; brewing/hot water/backflush remain blocked (pump still can't run)
- [ ] Refilling the tank logs `Water tank refilled - pump operations enabled` and
      the machine returns to its previous PID state
- [ ] With `keep_heater_on_empty=true` and standby enabled, leaving the tank empty
      past the standby timeout moves the machine to `Standby` and turns the heater
      off — the heater must never run unattended indefinitely
- [ ] A standby request (power switch/MQTT) is honored while in `Water Tank Empty`
- [ ] Removing the tank while the machine is in `Standby` does NOT wake it up
      (it stays in standby with the heater off)
- [ ] With a Home Assistant instance subscribed to the MQTT discovery prefix, a
      "Water Tank Full" binary_sensor entity appears (on when full, off when empty;
      not classified as a moisture/leak sensor) after connecting with
      `hardware.sensors.watertank.enabled=true`
- [ ] `mosquitto_sub -t '<prefix>/<hostname>/waterTankFull'` reports `ON` when full
      and `OFF` when empty; the message is retained, so a fresh subscriber (or a
      restarted Home Assistant) receives the current state immediately

## 11. Frontend (when `ui/` files changed)

Run from `ui/packages/frontend`:

- [ ] `pnpm test:run` — all frontend tests pass
- [ ] `pnpm tsc` — no TypeScript errors

Run from `ui/`:

- [ ] `pnpm lint` — Biome lint and format check pass

---

## Quick Smoke Test (minimum for non-release changes)

If the full checklist is too heavy for a minor change, at least verify:

1. Build passes
2. Native tests pass
3. `/api/health` responds 200
4. `/api/parameters?filter=all` returns full JSON
5. No crash when loading UI with telnet connected
