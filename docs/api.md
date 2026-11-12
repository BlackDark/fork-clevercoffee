# HTTP API and MQTT

Paths and payloads follow the C++ firmware v1.3.4 so the web UI and existing integrations keep working. Differences are marked. Implementation and tests: `crates/web`.

## HTTP

With `system.auth.enabled`, every route needs HTTP basic auth.

| Method | Path | |
| --- | --- | --- |
| GET | `/api/status` | machine state, temperatures, standby, shot counter, WiFi, MQTT, heap |
| GET | `/api/health` | liveness |
| GET | `/api/temperatures` | `currentTemp`, `targetTemp`, `heaterPower` |
| GET | `/api/history` | last 30 min of temperature, setpoint and heater power |
| GET | `/api/metrics` | controller health in one small reply, counts since boot: firmware version, reset reason (esp-hal code), uptime, heap (free, lowest), lowest free stack, requests refused by the low-heap gate (503), control loop (longest tick gap and slowest tick in µs, tick gaps longer than two periods), TSIC (good and bad frames, ms since boot of the last good frame (low 32 bits, 0 before the first)), DS18B20 read errors, settings saves and failures and the last result (0 none, 1 saved, 2 failed), log-ring bytes (wraps after 4 GiB), WiFi state, RSSI and reconnects, MQTT state |
| GET | `/api/logs` | the newest log lines as text *(new)*; `?since=<X-Log-Next of the last reply>` returns only what is new, a smaller `X-Log-Next` than asked for means the device restarted (a restart that logged more than `since` bytes before the next poll goes unnoticed); about 6 KB, a few minutes of a quiet machine |
| GET | `/api/parameters` | all parameters: name, type, value, default, limits or options; `requiresReboot` appears only when `true` (`?filter=all`; secrets are blanked); labels and help texts are the UI's; option `label` is the wire name MQTT and provisioning accept |
| POST | `/api/parameters` | `key=value&…` form or query; all-or-nothing validation. Enum values: send the number (canonical, stable); the option label is accepted too (`Automatic`, case-insensitive) |
| GET | `/api/config`, `/api/config/download` | nested config file |
| POST | `/api/config/upload` | import a config file (multipart or JSON, ≤ 16 KB, all-or-nothing) |
| POST | `/api/setpoint`, `/api/wake`, `/api/sleep`, `/api/steam`, `/api/pid`, `/api/backflush` | commands |
| POST | `/api/maintenance/reset-backflush-counter`, `/api/scale/tare`, `/api/scale/calibration` | commands |
| POST | `/api/ota/firmware` | multipart field `firmware`: streams into the inactive slot, restarts |
| GET | `/api/ota/status` | `status` (`idle`, `uploading`, `complete`, `error`), `progress`, `updateInProgress` |
| POST | `/api/restart`, `/api/factory-reset`, `/api/wifi-reset` | answered, then executed |
| GET | `/events` | server-sent events, 1 s: `new_temps`, `weight` *(new)*; `?metrics=1` sends `hello`, then only `metrics` *(new)*, the `/api/metrics` body each second; at most two streams (one with a Bluetooth scale), then `503` *(new)* |
| GET | `/ui/…` | the embedded web UI; `/` redirects there |
| POST | `/api/debug/hang` | `bench` builds only *(new)* |

Changed: `/api/ota/filesystem` and `/api/ota/url` answer `410 Gone` (the UI is in the firmware; no download from URLs). Unknown `GET` paths redirect to `/ui/` so phones detect the captive portal. Any request but `/api/health` answers `503` with `Retry-After: 1` while less than 8 KB of heap is free, and `/api/history` and `/api/config/upload` when the heap cannot spare their buffer at that moment; retry. A firmware upload may take 3 minutes. Every response closes its connection (`Connection: close`, no keep-alive). A connection that finds every HTTP worker busy waits in a backlog instead of being refused (ADR 0008).

## MQTT

Topics: `<mqtt.topic><hostname>/<name>` (state), `…/<name>/set` (command), `…/status` (`online`, retained; last will `offline`). Values publish only when they change, every 5 s (500 ms during a shot, 10 s in standby); with nothing to publish the client pings within half the 30 s keep-alive. Home Assistant discovery under `<mqtt.hassio.prefix>/<component>/clevercoffee-<hostname>/<name>/config`, re-sent every 5 min. Entity list: `cc_mqtt::ENTITIES`. Diagnostic sensors (`freeHeap`, `minFreeHeap` in KiB, `uptimeMinutes`, `wifiRssi`, `tsicBadFrames`, `ds18b20ReadErrors`) are in the diagnostic category and publish once a minute, only when the shown value changed; `wifiRssi` and the two error counters are disabled by default in Home Assistant.

MQTT is v5 only (`rust-mqtt`); brokers from the last decade all speak it.
