# Manual Integration Test Checklist

Pre-release validation. Every item must pass before merging to main or tagging a release.

## Prerequisites

- Device flashed with the build under test (USB or OTA)
- Device connected to WiFi and reachable at its hostname. The firmware
  defaults to **`test-cc-rust`** (`cc_config::schema::DEFAULT_HOSTNAME`). A
  machine still running the deleted C++ answers to `silvia` — that difference is
  how you tell which firmware is on the board before you start.
- Serial monitor available (USB) **or** telnet client for WiFi logging

## 1. Build & Unit Tests

- [ ] `just check` — fmt, clippy (pedantic, `-D warnings`), rustdoc, the host test
      suite, the parity harness, the device-test audit, and the markdown links
- [ ] `just gate` — the above plus device clippy, the Xtensa release build and
      the image-size budget
- [ ] `just size-check` — the image still fits `app0` and inside the growth limit

## 2. OTA Update

All three update paths must be exercised — they use independent code paths and
have each broken separately before.

### 2a. ~~ArduinoOTA / espota~~ — removed

There is no espota path. The C++ firmware offered one through PlatformIO's
`esp32_ota` environment; the Rust firmware uses ESP-IDF's own OTA, and its three
update routes are 2b (multipart upload), 2c (download from a URL) and 2d
(`/api/ota/status`). Do not expect an `espota`-shaped flow to work, and do not
add one back: a serial OTA that blocks the control task is a regression waiting
for a release.

### 2b. HTTP firmware upload (`/api/ota/firmware`, used by the web UI)

```sh
curl -w "\n%{http_code}\n" -X POST http://<ip>/api/ota/firmware \
  -F "firmware=@firmware.bin;filename=firmware.bin"
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
(mkdir -p /tmp/cc-ota && cp firmware.bin /tmp/cc-ota/ && cd /tmp/cc-ota && python3 -m http.server 8765 &)
curl -w "\n%{http_code}\n" -X POST http://<ip>/api/ota/url \
  -d "url=http://<host-ip>:8765/firmware.bin&type=firmware"
```

- [ ] Responds **202** immediately, before the download finishes (regression:
      the download ran inside the async web handler, starving AsyncTCP so the
      client got no response at all / broken pipe)
- [ ] `/api/ota/status` reports rising `progress` with `status: "downloading"`
- [ ] Device reboots and comes back; status returns to `idle`

## 3. USB Serial Logging

- [ ] The serial monitor shows boot log lines (WiFi connect, state transitions).
      Open it with `just mon <port>`; `pio device monitor` no longer exists.
- [ ] Log lines appear at INFO level during normal operation (e.g. temperature readings, state changes)
- [ ] Log level filtering works (DEBUG messages hidden at INFO level)
- [ ] **A boot whose NVS was written by the C++ firmware prints the predecessor line**
  (finding 3.6). Flash the Rust firmware over a machine that has run the C++, or
  erase the NVS partition and plant a key in the `config` namespace. The boot
  log must contain, verbatim and untruncated:

  ```text
  config: the previous firmware's settings are in NVS namespace "config" and this firmware uses "cc": not read, not deleted, still on the chip. Re-enter the Wi-Fi SSID and password. Expected on a first flash.
  ```

  Then: the machine does **not** associate (it has no SSID, which is the
  condition this line exists to explain), and re-provisioning with
  `wifi set <ssid>` + `wifi apply` over the serial console (§4 / `just
  wifi-provision <port>`) restores it. **No line** should appear on a boot
  where the `cc` namespace already holds a configuration. **Not yet run on
  hardware** — see [`intentional-diffs.md` §29](../rust-migration/intentional-diffs.md).

The serial node is `/dev/cu.usbserial-*` on macOS and `/dev/ttyUSB*` on Linux. The device's
bridge is a **WCH CH340** (`iProduct` = `"USB Serial"`, VID `0x1A86` / PID `0x7523`), not a
CP210x — `just wifi-provision <port>` and `tools/serial_log.py` take that node verbatim.

### 3a. What a healthy boot log looks like — **the C++ firmware only**

**Historical. Nothing in this section is runnable any more.** It is preserved
from the deleted root `DEBUG_GUIDE.md`, which was the only document that
described a healthy C++ boot, because it is the reference for *what the machine
used to print* and a surprising line in a Rust boot log is best judged against
it. The C++ source it quotes is recoverable with
`git show 9fa8c834:src/...`.

The items above it in this section, and `docs/status.md`, are the runnable
record for the Rust firmware.

- [ ] *(historical, C++ only — recorded, not re-run)* Capture the first ~30 s of
      serial output with `python tools/serial_log.py 30 --reset`.

- [ ] The tail of Phase 5 in `SystemInitializer.cpp` appears, in this order. This is the
      whole point of the phase: the ISR context pointer is set **before** the timer is armed,
      because the ISR dereferences it on its first tick.

      ```text
      [DEBUG] Setting ISR SystemContext at 0x...
      [DEBUG] Global SystemContext set: ptr=0x..., valid=1
      [DEBUG] Calling setupTiming()
      [DEBUG] Calling initTimer1() - create timer after ISR context is available
      [DEBUG] Calling enableTimer1() - ISR will now fire
      [INFO]  ISR marked as ready - timer ISR can now safely execute
      [DEBUG] Timer enabled - ISR should be firing every 10ms
      [INFO]  System initialization completed successfully
      ```

      `SystemInitializer.cpp:206` (`Global SystemContext set`, with the `valid=%d` flag),
      `:210-223` (the ordering above). **`valid=0` is a failure**, not a warning: the ISR
      returns early and the heater is never driven.
- [ ] `LoopManager initialized successfully` (`src/core/LoopManager.cpp:86`) and
      `ProcessController initialized successfully` (`src/control/ProcessController.cpp:68`)
      both appear. `Handlers initialized` is `SystemInitializer.cpp:424` — its absence means
      a handler constructor threw and the boot is degraded.
- [ ] At **DEBUG** level, within ~10 s, `src/main.cpp:225` prints a status line every 5 s:

      ```text
      [DEBUG] LOOP STATUS: loops=N, ISR enabled=1, ISR calls=N, relay_on=N, relay_off=N,
              temp=23.5°C, setpoint=90.0°C, pidOutput=500.0
      ```

      Read it against the counters in `include/clevercoffee/isr.h:24-27` (`isr_enabled`,
      `isr_call_count`, `isr_relay_on_count`, `isr_relay_off_count` — `std::atomic`, declared
      extern, defined in `isr.cpp`):

      - `ISR enabled=1`, and `ISR calls` **rising between successive lines** — the only proof
        the 10 ms timer ISR is running. A frozen count with `enabled=1` is a stopped timer.
      - `relay_on` **and** `relay_off` both rising while `pidOutput` is between 0 and 1000 —
        the heater PWM is cycling. One frozen at 0 means the ISR is not reaching the relay
        (`isr.h` null-checks `hardwareContext().heaterRelay()` and returns early if absent).
      - `temp` changing over minutes, never pinned to its last value. A frozen `temp` is a
        sensor problem, not a loop problem — see the "Temperature reading never changes"
        section of this checklist's history, and `docs/rust-migration/09-cpp-findings.md` for
        the probe-selection trap (a TSIC306 driver driving a fitted DS18B20 logs an error
        and reports nothing).
      - `setpoint` > 0. A `0` is a configuration read failure, not a setting.
- [ ] When the state machine is in `PID_MODE` (state 4), the PID logs `updateProcessControl:`
      with a **changing** `pidOutput`. A `pidOutput` pinned at 0 in `PID_MODE` means the
      process controller is not being ticked.

To keep the log for later, capture rather than scroll:

```sh
/tmp/venv/bin/python tools/serial_log.py 60 --reset > boot.log
grep -E 'ERROR|FATAL' boot.log          # anything here is a real clue
grep -E 'LOOP STATUS|State transition' boot.log
```

### 3b. ~~DEBUG level on the C++ firmware~~

The C++ takes its log level over **telnet** (port 23 — §4), not over USB: USB is the
transcript you are reading, so it cannot carry the instruction that changes its own verbosity.
Connect a telnet client, raise the level to `DEBUG`, then read the transcript over USB.
`docs/rust-migration/01-feature-inventory.md` §9 records this as the telnet story's origin.

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
- [ ] Connect a second terminal while one is open — the first is replaced, and
      the machine stays up (`MAX_CLIENTS` is 1, `Logger.h:154`)
- [ ] Stop reading on the connected terminal and drive the heap below 30 KB: the
      telnet stream goes quiet and the connection stays open; the serial stream
      keeps running (ADR-0002 decision 5)
- [ ] Let the heap recover: one `heap recovered` line, then the stream resumes.
      More than one means the shed is oscillating

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


## Added 2026-10-01 — from the nine defects fixed on the board

Each of these is a check that would have caught one of the reported defects, and
each is a check that can be run without a hand on the machine. Run them in this
order after any change to the control loop, the display task or the HTTP layer.

### Control loop

1. **The loop is at 100 Hz, not slower.** Read the periodic tick line:

   ```
   control tick: worst N ms of the last M (baseline B ms ..., budget 10 ms, K over budget)
   ```

   `K` must be 0 once the first second has passed, and `N` must be under 10 ms.
   A `N` in the tens of milliseconds means something slow is back inside the tick
   — the display frame is the usual culprit, and it belongs in the display task.

2. **A 400 ms period is itself a failure.** If a change reintroduces a period
   longer than 10 ms, the machine is back to reacting in half a second, which is
   what the human reported.

3. **The loop does not sleep longer than it is told.** A regression here is
   invisible in the log and catastrophic in the field: the machine stops
   answering HTTP while the panel keeps working. `curl -m 5 .../api/status` twice,
   ten seconds apart, and compare `uptime` — it must advance.

### Display

4. **The startup screen appears, twice.** In the serial log, within the first
   second: `display: boot screen — <version>` and then, once the radio has an
   address, `display: wifi screen — <ip>`. An address of `0.0.0.0` is a *failed*
   check, not a pass: it means the screen was drawn before DHCP finished.

5. **The standby screen survives.** `POST /api/sleep`, then read the panel's
   periodic report:

   ```
   display: present=true blanked=false frames=N failed=0
   ```

   `blanked=true` within ten minutes of entering standby is a **failure** — the
   display-off countdown is ten minutes (`StandbyCoordinator.h:14`) and the panel
   must still be drawing the standby screen until then. `frames` must keep
   advancing.

6. **The right edge is inside the frame.** The header's uptime and the `°C` unit
   are the two fields that used to clip. The goldens
   (`just snapshot-display`, then *read the diff*) pin them, and
   `widgets::tests::a_long_uptime_is_right_aligned_rather_than_clipped` is the
   host-side check for the case the goldens do not cover: an uptime past 100
   hours, which is what this machine reaches in four days.

### HTTP

7. **`GET /api/history` answers 200 with three arrays of equal length**, oldest
   point first, and one point per three seconds. An empty ring answers three
   empty arrays; a 501 or a timeout is a failure. A **timeout or a device reset**
   here means a large value is on a task stack — see finding 8.

8. **No response-sized value lives on a task stack.** The ring is 7.2 KB, the
   framebuffer is 1 KB, and the httpd task has 8 KB. Three separate device resets
   came from this during 2026-10-01 (a 7.2 KB ring inside a by-value `Shared`, a
   7.2 KB return value from `history_json`, and a 1 KB `Display` on the display
   task). If a new field is a fixed-size array, it is a `Box`.

9. **`POST /api/parameters` reads back what it wrote.** Save a value and read it
   back *in the same breath*, with no sleep:

   ```
   curl -X POST '.../api/parameters?brew.setpoint=91.0'
   curl '.../api/parameters' | jq '.[]|select(.name=="brew.setpoint")'
   ```

   The second command must show 91. The old failure was a 1-second window in
   which the API served the pre-write values, and the UI put them back into the
   form. A `503` from the POST means the control task did not apply the request
   within the ack timeout — that is the handler being honest, and the UI should
   keep the typed value.

10. **`/api/status` reports a real backflush threshold.**
    `backflushReminderThreshold` must be the configured value (50 by default), not
    `0`. A `0` means the value is not being published, which is a bug, not a
    setting.

12. **`POST /api/config/upload` round-trips a downloaded configuration.** The UI
    button at `SystemPage.tsx:182` is the operator's path; before this route
    existed it was a live `404`.

    ```
    curl -s '.../api/config/download' -o /tmp/config.json
    curl -s -X POST '.../api/config/upload' -H 'Content-Type: application/json' \
      --data-binary @/tmp/config.json | jq
    curl '.../api/parameters?filter=all' | jq '.[]|select(.name=="brew.setpoint")'
    ```

    Expect `{"success":true,"message":"Configuration validated and applied
    successfully.","restart":true}` and the setpoint unchanged. **The body is
    `application/json`, not multipart** — sending it as `multipart/form-data`
    must give `400 "JSON body must be a top-level object"`, and that is correct
    behaviour, not a bug.

13. **The upload is refused, not truncated, when it is too large.**

    ```
    python3 -c "print('{"system":{"hostname":"' + 'x'*20000 + '"}}}')" \
      > /tmp/big.json
    curl -s -o /dev/null -w '%{http_code}\n' -X POST '.../api/config/upload' \
      -H 'Content-Type: application/json' --data-binary @/tmp/big.json
    ```

    Must be **413**, and the configuration must be untouched. The failure this
    guards is a body read to exactly the cap and then applied: half a
    configuration — a new PID gain with the old emergency cut-off — on a machine
    that may be brewing.

14. **An upload is all-or-nothing.** One out-of-range value anywhere rejects the
    whole document and changes nothing:

    ```
    curl -s -X POST '.../api/config/upload' -H 'Content-Type: application/json' \
      -d '{"brew":{"setpoint":95.0},"safety":{"emergency_temp":900.0}}' | jq
    curl '.../api/parameters?filter=all' | jq '.[]|select(.name=="brew.setpoint")'
    ```

    `400`, and `brew.setpoint` must still be what it was. A `200` here means a
    partial apply.

15. **`system.auth.*` is not inert — and it fails OPEN on empty credentials.**
    With `system.auth.enabled` unset the API is open, which is the default. Set
    it and reboot:

    ```
    curl -X POST '.../api/parameters?system.auth.enabled=1' | jq .requiresRebootKeys
    curl -i '.../api/status' | head -1                       # 401
    curl -i -u admin:admin '.../api/status' | head -1         # 200
    curl -i -u admin:wrong '.../api/status' | head -1         # 401
    ```

    Three things to check, all of which were wrong before this work: the POST
    must report `requiresRebootKeys: ["system.auth.enabled"]`, because the
    credential check is decided at boot exactly as the C++'s `setupMiddleware`
    decides it; the `401` must carry `WWW-Authenticate: Basic realm="CleverCoffee"`;
    and **no** `Authorization` header may appear in the telnet log. Now clear the
    username or the password and reboot again: the API is **open**, with a boot
    warning. That is the C++'s behaviour (`WebServerManager.cpp:290-294`) and it
    is deliberate — see `intentional-diffs.md` #20.

16. **The UI logs in without a code change.** With auth on, open `http://<device>/ui/`
    in a browser: it must raise the native credential prompt, and after that the
    live `/events` stream and every `fetch` must work **without** a reload. If
    the temperature display freezes while the rest of the page works, the browser
    is not replaying the cached credential on the `EventSource` — capture the
    `GET /events` response code in devtools to tell a `401` from a stream that
    simply stopped.

17. **`/api/status` reports `steamMode`, and it is the steam flag.** While the
    steam state is entered and left, `steamMode` must follow it and `brewing`
    must stay `false`:

    ```
    curl -s '.../api/status' | jq '{steamMode, brewing}'
    ```

    Before this work the route emitted a *brew-state* value under the name
    `steamMode`; the two are different facts (`intentional-diffs.md` #21).

18. **A CORS preflight answers.** `OPTIONS` on any `/api` route must be `204`
    with `Access-Control-Allow-Origin` — and must **not** be challenged, because a
    browser sends a preflight with no credentials:

    ```
    curl -i -X OPTIONS '.../api/status' | head -8
    ```

### The kernel defect, so nobody re-derives it

11. **Do not move the DS18B20 onto another task.** If a sensor task is ever
    reintroduced, flash it and read the log: `assert failed:
    xTaskRemoveFromEventList` or a `LoadProhibited` in the lwIP `tcpip_thread`
    means the bit-bang is on a second task and the kernel is being corrupted. The
    bisect is in 09 §28 and in `crates/cc-firmware/src/sensor_task.rs`. The same
    applies to every cross-task *blocking* hand-off: a `Queue` with a blocking
    receive, a `std::sync::Mutex`, and an ESP-IDF task notification were each
    measured to assert. Non-blocking `CommandQueue::try_send` does not.


### The host-side screen verifier (added 2026-10-01)

Before flashing anything, two checks that need no hardware:

13. **`just test` runs `crates/cc-display/tests/screen_matrix.rs`**, which renders
    every template against ~40 extreme inputs and asserts: nothing inks outside
    128x64; every system screen is reachable from some input; and **every
    `DisplayInput` field changes what is drawn** — a caller cannot leave a field
    at its default forever without the picture proving it. That last one is what
    catches "the firmware never sets this field", which is how the missing brew
    timer, the missing `brew_active` and the missing `now_ms` all survived.
14. **`just screens`** writes `/tmp/cc-screens.png`: every template against every
    case, labelled, in one sheet. **Read it.** It is the only check that catches
    a screen which renders but is *wrong* — the clipped `°C`, the cut-off uptime
    `m` and the missing brew timer all passed every assertion and were found by
    looking.

Known-and-accepted, from reading the sheet (2026-10-01, inherited from the C++):

* The sensor-error and EEPROM-error message screens use a 10 px line pitch with
  `profont11`, so adjacent lines overlap by one pixel. `displayMessage` has six
  lines at a ten-pixel pitch, and six lines at eleven pixels is 66 — taller than
  the panel — so the pitch cannot simply grow. Fixing it means dropping those
  screens to `profont10`, which changes the typography of six screens.
* The OTA error message and the offline splash can run past the right edge
  depending on the string. Worth a decision before anyone relies on them.


### Measured layout overruns (2026-10-01, third pass — all C++ behaviour unless noted)

Every number is `Font::str_width` in the font the screen uses, and every one is
pinned by `crates/cc-display/tests/languages.rs` so a *new* overrun fails the
build.

| where | line | width | panel | note |
| --- | --- | --- | --- | --- |
| EepromError, all languages | `EEPROM Error, please set Values` | 185 | 128 | 57 px cut |
| SensorError, German only | `Temp.-Sensor ueberpruefen!` | 153 | 128 | 25 px cut; EN/ES are 111 and fit |
| SensorError, **portrait** | `ueberpruefen!` | 75 | **64** | the baseline's German translation |
| OTA title | `Update failed` in `fub17` | 150 | 128 | centred at `x = -11`, so **both** edges clip at once |
| Scale, English | `Pressure: ` | 60 | 50 | runs into the value column |
| Scale, Spanish | `Pressure: ` | 54 | 50 | |
| Scale, German | `Weight: `, `Flush: ` | 54 | 50 | |

The Scale value column is 50 px (`kValueColumnOffset`, `DisplayWidgets.h:170`),
which is also where the **brew row and the setpoint row collide** — see §"the
Scale row map" below. Both are the same layout defect wearing two hats, and
neither is visible on the Standard template this machine is configured for.

**The general check.** The firmware has `Font::ink_box` (the real inked extent,
not the advance) and `Display::clip_window()` (rotation-aware, 64x128 in
portrait) and neither was used for fitting anywhere. The test now uses
`str_width` against the rotation-aware width, which is the same class of check
one layer up, and it deliberately does **not** add a runtime "refuse to draw"
primitive: that would change four screens that currently match the C++, in
exchange for hiding a defect the baseline also has.


## Added 2026-10-05 — from a bench ESP32 (original, rev 3.0), Rust firmware

Found while running the sections above against a bench with a DS18B20, an
SSD1306 and four switches, and nothing on the heater pin. Four new checks and
two open defects. The sections above are unchanged; these are additive.

### New checks

- [ ] `GET /api/status` on an **associated** machine reports `wifiAssociated:
      true`, a non-zero `wifiSignal` and the DHCP address in `ip`. It read
      `false` / `0` / `null` continuously on an associated machine, because the
      control task's publish runs every 10 ms and `Shared::publish` replaces the
      whole slot — so the radio's values, written once a second, were gone again
      within one tick. Fixed by carrying the four fields forward.
- [ ] `GET /api/parameter-help` takes **`param=`**, not `parameter=`. A wrong key
      answers `422 {"error":"parameter is missing"}`, which reads like "this
      endpoint is broken" rather than "you spelled it wrong".
- [ ] `POST /api/parameters` → `GET /api/parameters` **after a reboot** is the
      only persistence check that counts. Verified on the bench for a bool, an
      int, a float and a text parameter, and for a boot-only key (which answers
      `200` with `requiresReboot: true` and names the key).
- [ ] Boot log line `switch resting levels after settling: …`. With the bench's
      buttons wired, all four reported `false`. **All four `true` means a
      floating input on GPIO34/35/36/39 and a brew that starts on its own** —
      fix the pull or set the flag back to false before anything else.

### Open defects — FOUND AND FIXED 2026-10-05

Both were recorded here first as open and are now closed. The text is left as
written, with the closure underneath, because a checklist that quietly drops a
finding loses the only record of what the bench is for.

- [x] **An unknown `/api/...` path answered `405 text/html`, not the JSON
      `404`.** `GET /api/nope` → `405`, `Specified method is invalid for this
      resource`, `content-type: text/html`. The C++ answers
      `{"error": "API endpoint not found"}` (`handleNotFound`,
      `WebServerManager.cpp:1006-1027`). Cause: the `/api*` `Options`
      preflight wildcard matches the URI, so ESP-IDF reports *method mismatch*
      rather than *no match*, and the registered `404` handler never ran.
      **Fixed** by registering the same handler for `405` and deciding the
      status from the route table. After: `GET /api/nope` → `404`
      `application/json`, `POST /api/status` → `405` `application/json`,
      `GET /nope` → ESP-IDF's `text/html` `404`. Verified on the bench.
      [`intentional-diffs.md` §30c](../rust-migration/intentional-diffs.md).

- [x] **A PID tuning written over HTTP did not reach the running PID.** With
      `pid.regular.kp` at `62` restored by `POST`, `/api/parameters` reported
      the new value and the boot log persisted it, but `heaterPower` stayed at
      the old tuning's `21.8 %` for 15 s, and only jumped to `100 %` at a 65 K
      error after the PID was cycled. **This was parity, not a defect** —
      `ProcessController.cpp:170` gates the C++'s retune on a state change too.
      It is now a divergence **on request**: a gain write re-chooses the gains
      on the next tick. After: `kp=10` → duty 43.79 %, `kp=62` → 100 %, with
      no state change in between. Verified on the bench.
      [`intentional-diffs.md` §31](../rust-migration/intentional-diffs.md).

### Two more found in the same pass — also fixed

- [x] **A fractional setpoint was truncated.** `POST /api/setpoint?value=93.5`
      answered `202 {"accepted":true}` and set 93. Same for `80.5` and `91.2`.
      The C++ passes the `double` straight to `setProcessSetpoint`
      (`WebServerManager.cpp:394-396`); this port cast to `i32`. Now `88.5`,
      `91.2` and `60.75` all land exactly. This is almost certainly what "the
      setpoint control does nothing" looked like from the UI.
      [`intentional-diffs.md` §30a](../rust-migration/intentional-diffs.md).

- [x] **Backflush mode could not be turned off.** Four presses in a row —
      including the explicit `?on=0` — all answered `{"backflushOn":true}` and
      the machine stayed in `BACKFLUSH_IDLE`, because the toggle fed
      `BackflushStop`, which stops a cycle without clearing the mode flag. Now
      on → off → on, verified on the bench.
      [`intentional-diffs.md` §30b](../rust-migration/intentional-diffs.md).

### Still open

- [ ] **`standby.enabled` and `standby.time` changed to `true` / `2` with no
      recorded write.** Both were at their defaults, then read back as
      `standby.enabled = true, standby.time = 2` (defaults `false` / `35`)
      during the 2026-10-05 session. Nothing in the firmware assigns either
      outside the parameter-write path, no MQTT session was configured, and no
      browser was opened. **Not diagnosed.** Reproduce by watching
      `GET /api/parameters` across a reboot and an idle hour.

## Outstanding — recorded 2026-10-01, none of it fixed

Each item is a decision or a measurement, not a defect with an obvious fix.

| # | What | Why it is not fixed | What is needed |
| --- | --- | --- | --- |
| 1 | **The Scale template's brew row erases the setpoint row.** Both are at `y = 26`; the brew row's inverted field is `78 x 10` at `(x + 50, y + 1)` and it erases the setpoint's label, value and `°C`. | The C++ has the identical collision, and both firmwares default `fullscreen_brew_timer` to false, so this is **live on a Scale-template machine during a brew**, not hidden. Every fix is a visible layout change: re-pitch the rows to `13 / 22 / 31 / 40 / 49`, or drop the row, or shrink the field — and the field cannot shrink, because both rows use the same value column. | A layout decision. The row map and the arithmetic are in `docs/operations/integration-checklist.md` and in the review that produced it. |
| 2 | **The Scale value column is 50 px and four labels are wider**, including English `Pressure: ` at 60 px. | Same family as #1, same "fixing it moves a screen somebody looks at". | The same decision. Measured widths are pinned by `tests/languages.rs`. |
| 3 | **The EEPROM error line is 185 px into a 128 px panel** (57 px cut), in all three languages. | C++ parity: the same string, the same `displayMessage`, the same per-glyph clipping. | Shorter text, or wrapping into the five slots the call already leaves empty. Both are deliberate divergences. |
| 4 | **The German sensor-error line is 153 px into 128** (25 px cut); English and Spanish are 111 px and fit. | C++ parity. | A shorter German string. |
| 5 | **The OTA error title is 150 px in `fub17`, centred at `x = -11`** — clipped at *both* edges at once. | C++ parity, and no bounds test of any kind can detect it. | A smaller font for that screen, or a shorter title. |
| 6 | **The sensor-error and EEPROM message screens overlap by a pixel** (a 10 px pitch with an 11 px font). | The pitch cannot grow: `displayMessage` is six lines and six at 11 px is 66 px into a 64 px panel. | `profont10` for those two screens, which changes six screens' typography. |
| 7 | **The control loop's 10 ms period is not met**: mean tick work ~16 ms, achieved period ~13–22 ms. The work is **not localised** — a bisect says the temperature poll is about half of it, which does not square with a scratchpad read at 2.4 Hz. | I have not localised it and will not guess in the source. | A profile that is not more `now_ms()` calls: a GPIO toggle captured by the idle task's accounting, or an external logic analyser. |
| 8 | **`CONFIG_FREERTOS_HZ`** — measured at 1000 and **reverted**: identical work and period, so the tick rate was never the limiter. | Reverted on evidence. | Revisit only if #7 lands under a millisecond. Note it is 100 because **ESP-IDF 5.5.5's own default** is 100 (`components/freertos/Kconfig:37`), not the widely-quoted 1000. |
| 9 | **A sensor task cannot own the DS18B20**: the bit-bang is the only user of `esp_idf_hal::interrupt::free`, a process-global cross-core critical section, and running it from a second task asserts inside the kernel. | The mechanism is in the `FreeRTOS`/hal layer, not in a design choice of ours. | An upstream fix, or a driver that does not need a global critical section. The display half of the split works and is in the firmware. |
| 10 | **The cross-flash deadman assert**: a `Queue` with a blocking receive, a `std::sync::Mutex` hand-off and an ESP-IDF task notification each assert on this build. | Same. The frame hand-off is lock-free instead. | As #9. |
| 11 | **A reboot into `PID_DISABLED` is the C++'s behaviour**, not a bug: the power switch is a `Toggle` and a toggle reading off at boot starts disabled. `pid.enabled` wins only when no power switch is configured. | Correct parity. | If the config should win, that is a *config default* change, and it is a product decision. |

## The screen contact sheet — how to make one

```bash
just screens        # writes /tmp/cc-screens.png
```

`just screens` runs
`cargo run -p cc-display --features scenarios --example screens`, which renders
**every template against every case into one labelled PNG** — 172 tiles, written
by a hand-rolled dependency-free PNG encoder inside the example. It needs no
hardware and no browser.

**Read it.** It is the only check that catches a screen which renders but is
*wrong*: every display defect found on 2026-10-01 (the clipped `°C`, the cut-off
uptime `m`, the missing brew timer, the field running off the panel, the swapped
numbers) passed every assertion and was found by looking at a panel or a sheet.

The automated half is `just test`, which runs
`crates/cc-display/tests/{screen_matrix,languages,text_safety}.rs`:
* `screen_matrix.rs` — every template × ~40 extreme inputs × on/off configs:
  nothing leaves the frame, every system screen is reachable, every rendering
  flag changes a frame, two cases in different stages never render the same
  pixels, and every stage a policy permits is reached;
* `languages.rs` — every template × every state × all three languages: fit,
  label columns, message lines (landscape **and** the 64 px portrait panel), the
  `°C` arithmetic, and a glyph for every translated character;
* `text_safety.rs` — a hostile string corpus (emoji, CJK, NUL, 400 characters)
  across twelve fonts: no panic, and a missing glyph measures zero and draws
  zero.


### Wi-Fi: four checks, in the order to run them (2026-10-01)

The machine went unreachable and took three faults to bring back. These four
checks exist so that the next one costs ten minutes instead of an afternoon.

1. **What does it think it is connecting to?**

   ```
   /tmp/venv/bin/python tools/serial_log.py 25 --reset | grep 'stored credential'
   firmware: wifi: stored credential — ssid "Cappuxinno" (10 bytes), password 14 bytes
   ```

   Compare against `.env` **byte for byte**, not by eye. A 7-byte SSID next to a
   10-byte one is a machine looking for a network that is not there — and it
   produces *no* `wifi:state:` transitions at all, which reads exactly like a
   dead radio.

2. **Is it the encryption?**

   ```
   /tmp/venv/bin/python tools/serial_log.py 30 --reset | grep authmode
   wifi:authmode threshold failure, ignore!, (recvd, thresh) : (3, 7)
   ```

   ESP-IDF's `wifi_auth_mode_t` is a **sequence, not a bitmask**, and the check
   is equality. `recvd` is the AP, `thresh` is what we asked for. Naming a mode
   the AP does not use **prevents** association — `WPA2WPA3Personal` will not join
   a WPA2-only AP. For this machine: `WPA2Personal`, and PMF
   `Capable { required: false }`.

3. **Can the network be changed over USB at all?**

   ```
   just wifi-provision /dev/cu.usbserial-XXXX
   ```

   It must not refuse with "a credential is already stored". If it does, the
   recovery path is closed and a wrong network is unfixable without HTTP — which is
   the state this firmware was in until 2026-10-01.

4. **After any of the above, prove the machine is actually back:**

   ```
   ping -c 2 -W 3000 test-cc-rust.lan
   curl -s -m 8 http://test-cc-rust.lan/api/temperatures
   ```

   Associating is not recovering: the stored configuration can still hold a probe
   type this board does not have, and the temperature will be `null` with the
   machine otherwise perfectly healthy. That is a *separate* fault and the log
   names it (`driver = Tsic306 ... but the probe measured on this board is
   DallasDs18b20`).


### The control tick is not at 100 Hz, and we now know where the time is

```
control tick: worst N ms of the last M (… budget 10 ms, K over budget) —
  mean work 15 ms, achieved period 15 ms of a 10 ms target
```

The loop runs at **~65 Hz**, not 100 Hz, and the time is in the **applier span**
— `cc_machine::apply`, the scale drain and the reboot checks — at ~12 ms per tick.
Not the sensors (0 ms), not the reducer (0 ms), not the display (0 ms). Full
measurement and the two traps that produced wrong numbers on the way are in
`09-cpp-findings.md` §31.

To narrow it further, split the applier span into `apply` / `drain_scale` / the
reboot checks and read the same line. Do **not** attribute it without a
measurement: "the applier is slow" is not a finding, "the applier is 12 ms" is.
