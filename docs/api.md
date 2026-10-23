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
| GET | `/api/parameters` | all parameters with metadata (`?filter=all`; secrets are blanked) |
| POST | `/api/parameters` | `key=value&…` form or query; all-or-nothing validation |
| GET | `/api/parameter-help?param=<key>` | help text |
| GET | `/api/config`, `/api/config/download` | nested config file |
| POST | `/api/config/upload` | import a config file (multipart or JSON, ≤ 16 KB, all-or-nothing) |
| POST | `/api/setpoint`, `/api/wake`, `/api/sleep`, `/api/steam`, `/api/pid`, `/api/backflush` | commands |
| POST | `/api/maintenance/reset-backflush-counter`, `/api/scale/tare`, `/api/scale/calibration` | commands |
| POST | `/api/ota/firmware` | multipart field `firmware`: streams into the inactive slot, restarts |
| GET | `/api/ota/status` | `status` (`idle`, `uploading`, `complete`, `error`), `progress`, `updateInProgress` |
| POST | `/api/restart`, `/api/factory-reset`, `/api/wifi-reset` | answered, then executed |
| GET | `/events` | server-sent events, 1 s: `new_temps`, `weight` *(new)*; at most two streams (one on the portal), then `503` *(new)* |
| GET | `/ui/…` | the embedded web UI; `/` redirects there |
| POST | `/api/debug/hang` | `bench` builds only *(new)* |

Changed: `/api/ota/filesystem` and `/api/ota/url` answer `410 Gone` (the UI is in the firmware; no download from URLs). Unknown `GET` paths redirect to `/ui/` so phones detect the captive portal. `/api/history` and `/api/config/upload` answer `503` when the heap cannot spare the buffer at that moment; retry. A firmware upload may take 3 minutes. A connection that finds every HTTP worker busy waits for its SYN retry instead of being refused (ADR 0008).

## MQTT

Topics: `<mqtt.topic><hostname>/<name>` (state), `…/<name>/set` (command), `…/status` (`online`, retained; last will `offline`). Values publish only when they change, every 5 s (500 ms during a shot, 10 s in standby); with nothing to publish the client pings within half the 30 s keep-alive. Home Assistant discovery under `<mqtt.hassio.prefix>/<component>/clevercoffee-<hostname>/<name>/config`, re-sent every 5 min. Entity list: `cc_mqtt::ENTITIES`.

MQTT is v5 only (`rust-mqtt`); brokers from the last decade all speak it.
