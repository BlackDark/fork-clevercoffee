# Web API contract

Derived from the C++ handlers, not from `docs/api/openapi.yaml`, which disagrees with the code
in 26 places (see [Spec drift](#spec-drift) and defect D34). The frontend in `ui/` is reused
unchanged, so this document is a compatibility requirement for the Rust backend.

The C++ device serves on port 80. The Rust device will do the same.

Cross-links: [inventory.md](inventory.md), [architecture.md](architecture.md),
[defects-register.md](defects-register.md).

---

## Middleware

Applied to every route, as in the C++ firmware.

| Concern | C++ behaviour | Rust behaviour | Note |
| --- | --- | --- | --- |
| CORS | `origin="*"`, credentials allowed, origin reflected | `origin="*"`, no credentials | D34, security |
| Preflight | `OPTIONS` answered by middleware with 200 | same | |
| Auth | middleware that never authenticates (D10) | a real check in the router; mutating routes require credentials when auth is enabled | D10 |
| OTA password | unused by the HTTP OTA routes (D17) | OTA is USB-only | D17 |

## Routes

`Auth` is the requirement: `none`, or `basic` when `system.auth.enabled` is true. Every route
is `none` in the C++ firmware regardless of configuration, because of D10.

| # | Method | Path | Auth | C++ status codes | Rust status codes |
| --- | --- | --- | --- | --- | --- |
| 1 | GET | `/api/status` | none | 200, 500 | 200, 500 |
| 2 | GET | `/api/config` | none | 200, 500 | 200, 500 |
| 3 | POST | `/api/setpoint` | none | 200, 400 | 200, 400, 422 |
| 4 | GET | `/api/health` | none | 200 | 200 |
| 5 | POST | `/api/wake` | none | 200, 500 | 200, 500 |
| 6 | POST | `/api/sleep` | none | 200, 500 | 200, 500 |
| 7 | POST | `/api/steam` | none | 200, 503, 500 | same |
| 8 | POST | `/api/pid` | none | 200, 500 | same, plus 503 when refused |
| 9 | POST | `/api/backflush` | none | 200, 400, 503, 500 | same |
| 10 | POST | `/api/maintenance/reset-backflush-counter` | none | 200, 500 | same |
| 11 | POST | `/api/scale/tare` | none | 200, 500, or 404 when the scale is off | same |
| 12 | POST | `/api/scale/calibration` | none | 200, 500, or 404 | same |
| 13 | GET | `/api/parameter-help` | none | 200, 404, 422, 500 | same |
| 14 | GET | `/api/temperatures` | none | 200, 500 (and error bodies with 200, D15) | 200, 500 only |
| 15 | GET | `/api/history` | none | 200, 500 | same |
| 16 | GET | `/api/nvs-debug` | none | 200, 500 | 200, 500; secrets redacted (D14) |
| 17 | POST | `/api/wifi-reset` | none | 200, 500 | 200, 500 |
| 18 | GET | `/api/config/download` | none | 200, 500 | same; secrets redacted |
| 19 | POST | `/api/config/upload` | none | 200, 400, 413 | 200, 400, 413; atomic (D13) |
| 20 | POST | `/api/restart` | none | 200, 500 | same |
| 21 | POST | `/api/factory-reset` | none | 200, 500 | same |
| 22 | ANY | `/api/parameters` | none | 200, 400, 405, 500 | same, plus 401 |
| 23 | POST | `/api/ota/firmware` | none (D17) | 200, 400, 409, 500 | same, plus 401 when auth is on, plus 409 when busy |
| 24 | POST | `/api/ota/filesystem` | none (D17) | 200, 400, 409, 500 | same |
| 25 | POST | `/api/ota/url` | none (D17) | 202, 400, 409 | same, plus 401, plus 400 for a rejected URL |
| 26 | GET | `/api/ota/status` | none | 200 | same |
| 27 | GET | `/events` | none | 200 SSE | same |
| 28 | GET | `/` | none | 302 to `/ui/` | same |
| 29 | GET | `/ui/**` | none | 200, 404 | same |
| 30 | * | anything else | none | 404 JSON for `/api/`, 404 text otherwise | same |

**OTA is kept**, in all four paths, because the user confirmed it stays. The plan had proposed
deleting these four routes; they are instead corrected:

- every OTA path requires the configured password when `system.auth.enabled` is set, and refuses
  to start unless the machine is idle (D01, D17);
- `/api/ota/url` requires an `http` or `https` scheme and a host on a small allow-list, and the
  firmware variant gains the extension check the filesystem variant already had (D16);
- a USB OTA path is added through the provisioning channel.

The frontend's OTA section (`ui/packages/frontend/src/components/OTAUpdateSection.tsx`) is
therefore **unchanged**: same routes, same payloads, same status codes. The one deviation is the
extra 401 when auth is enabled, which a correctly configured frontend already handles.

`POST /api/config` is documented in `openapi.yaml` but does not exist in the code. It stays
absent.

## Payloads

### GET `/api/status`

```json
{
  "temperature": 92.4,
  "setpoint": 92.0,
  "heaterPower": 37.5,
  "machineState": 20,
  "isStandby": false,
  "standbyTime": 0,
  "pidEnabled": true,
  "steamMode": false,
  "uptime": 123456,
  "shotsSinceBackflush": 12,
  "backflushReminderThreshold": 50,
  "backflushReminderDue": false,
  "weight": 0.0,
  "brewWeight": 0.0
}
```

`weight` and `brewWeight` are present only when the scale is enabled. `machineState` is the
integer state id, unchanged from the C++ enum, because the frontend renders states by id
(`ui/packages/frontend/src/lib/`).

### GET `/api/parameters`

A JSON **array**. Each element:

```json
{ "name": "brew.setpoint", "label": "...", "section": 1, "order": 104,
  "helpText": "...", "type": 3, "value": 92.0, "default": 95.0, "min": 20.0, "max": 110.0 }
```

`type` is an integer: 0 bool, 1 int, 2 double, 3 enum, 4 string. `min` and `max` are absent for
bool and string. Secret parameters are present with `"value": ""` and `"redacted": true`.

The C++ `filter` query parameter is accepted and ignored by the C++ code. The Rust server honours
it, with the values `all`, `hardware`, `behavior`, `other`, and it tags each entry so the
documented filters mean what they say (D26).

### POST `/api/parameters`

Content type is `application/x-www-form-urlencoded` or `multipart/form-data`. One entry per
parameter, keyed by its dotted name. The C++ handler silently skips empty values; the Rust
handler treats an explicitly empty value as "reset to default" and reports it (D25).

### GET `/api/config` and `/api/config/download`

A nested object, same shape as the import format, described in
[config-export-schema.md](config-export-schema.md). `/api/config/download` adds
`Content-Disposition: attachment; filename="config.json"` and pretty-prints.

**Difference from C++:** secret fields are redacted. The C++ firmware returns the real values
from both endpoints and from `/api/parameters` and `/api/nvs-debug` (D14). The frontend does not
read secret values back, so this is not a user-visible break.

### POST `/api/config/upload`

Content type must be `application/json`. Body is the nested config object. Max 16 KB.

C++ response:

```json
{ "success": true, "message": "Configuration validated and applied successfully.", "restart": true }
```

C++ returns 200 even when 90 of 96 parameters failed to import (D13). The Rust response is:

```json
{
  "success": false,
  "message": "configuration rejected",
  "restart": false,
  "accepted": 0, "rejected": 6, "clamped": 0, "unknown": 1, "missing": 89,
  "details": [ { "path": "brew.setpoint", "reason": "out_of_range", "value": 150, "min": 20, "max": 110 } ]
}
```

with status 400 when anything is rejected or unknown, unless the request carries
`?mode=partial`, in which case valid fields apply and the status is 200. The `restart` field
keeps its C++ meaning in both: a hint the client acts on by calling `/api/restart`. Neither
version restarts itself (D34 item 19).

### GET `/events`

Server-sent events, `text/event-stream`.

| Event | Data |
| --- | --- |
| `hello` | `"Connected to CleverCoffee"` on connect |
| `ping` | empty, keeps the connection alive |
| `new_temps` | `{"currentTemp": 92.4, "targetTemp": 92.0, "heaterPower": 37.5}` at 1 Hz |
| `weight` | `{"weight": 0.0, "brewWeight": 0.0}` at 1 Hz when the scale is enabled |

The `id` field is milliseconds since boot, matching the C++ firmware.

### OTA and static assets

The four OTA routes are kept with the corrections above. `GET /ui/**` serves from the `assets` flash region. The C++ server
sets `Content-Encoding: gzip` whenever a `.gz` sibling exists, with no `Vary` and no negotiation;
the Rust server negotiates on `Accept-Encoding` and sets `Vary`. `index.html` is served with
`no-cache`, everything else with `max-age=604800`, matching C++.

## Spec drift

`docs/api/openapi.yaml` against the C++ code. Each row is a place where the shipped spec is
wrong about the shipped firmware. The Rust implementation follows the code, except where the
code is itself a defect, in which case it follows the corrected behaviour and says so.

| # | Spec says | Code does | Rust follows |
| --- | --- | --- | --- |
| 1 | `POST /api/config` updates config | no such route; 404 | absent |
| 2 | `/api/steam` returns `steamEnabled` | returns `steamMode` | `steamMode` |
| 3 | `/api/pid` accepts `{enabled}` | body ignored, pure toggle | toggle |
| 4 | `/api/backflush` returns only `success` | also `backflushOn` | both |
| 5 | `/api/setpoint` takes JSON `{value}` | reads a form or query param | both, JSON preferred |
| 6 | `/api/status` has `brewSwitch`, `steamSwitch`, `pumpSwitch`, `currentTemp` | none of those; has `temperature`, `machineState`, `isStandby`, `standbyTime`, `steamMode`, `uptime`, `setpoint`, `heaterPower`, `pidEnabled`, `shotsSinceBackflush`, `backflushReminderThreshold`, `backflushReminderDue`, optional `weight`, `brewWeight` | the code |
| 7 | `/api/parameters?filter=` takes `visible`, `editable` | ignored; hardcoded `all`; the code knows `hardware`, `behavior`, `other`, `all` | the code, honoured |
| 8 | parameter object has `id`, `type` as a string, `defaultValue`, `visible` | has `name`, `label`, `section`, `order`, `helpText`, `type` as an int, `value`, `default`, `min`, `max` | the code |
| 9 | `POST /api/parameters` takes JSON `{id, value}` | takes form pairs keyed by name; no `id` exists | the code |
| 10 | `/api/parameter-help` returns an array, no parameters | requires `?param=`, returns `{name, helpText}`, 422/404 | the code |
| 11 | `/api/temperatures` returns `{heaterPowers[], heaterTemps[], ambientTemp}` | returns `{currentTemp, targetTemp, heaterPower}` | the code, errors only with 500 (D15) |
| 12 | `/api/history` returns `{data: []}` | returns `{currentTemps[], targetTemps[], heaterPowers[]}` | the code |
| 13 | `/api/ota/status` has `message`, enum lacks `queued` | no `message`; has `updating`, `type`, `uploadedSize`, `totalSize`, `filesystemPartition`; `queued` exists | the code, and `queued` is documented |
| 14 | `/api/ota/url` returns 200 | returns 202 | the code |
| 15 | OTA upload documents only 200 | also 400, 409, 500 | the code, plus 401 |
| 16 | `/api/config/upload` documents 200/400 | also 413 and library-generated 400s | 200/400/413 |
| 17 | config upload "persists to NVS, and restarts" | does not restart; `restart: true` is a hint | same meaning, documented |
| 18 | `/api/scale/*` always exist | only when the scale is enabled at boot, else 404 | same |
| 19 | `/api/scale/calibrate` is deprecated | never existed | absent |
| 20 | no `securitySchemes` at all | Basic auth config exists but is inert | real `securitySchemes` |
| 21 | `POST /api/wake` and `/api/sleep` undocumented | both exist | both, documented |
| 22 | `/events`, `/`, `/ui/**` undocumented | all exist | all, documented |
| 23 | `hardware.oled.address` "default 60, range 0-255" | an enum index, `0` or `1` | the code, and the config schema is regenerated |
| 24 | `system.log_level` valid 0-5, `5` = CRITICAL | 6 values, `5` = FATAL, `6` = SILENT | the code |
| 25 | `display.template` valid 0-4 | six templates, `5` = MODERN | the code |
| 26 | `display.blescale_brew_timer` documented | does not exist | rejected as an unknown field on import |

Rows 23 to 26 are config, not API, but they are the same class of drift and are recorded here
because `openapi.yaml` carries the `ConfigFile` schema. See
[config-export-schema.md](config-export-schema.md).
