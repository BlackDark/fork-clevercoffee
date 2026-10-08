# Web, MQTT and the UI

**The device's outward faces: 28 HTTP endpoints, an event stream, an MQTT
registry, and a React app served from flash.**

Everything here is a pure function of a telemetry snapshot. `cc-web` and
`cc-mqtt` do not touch a socket — they take the machine's state and return what
the response should be. That is why the HTTP layer can be unit
tested for an application that runs on one microcontroller, and why the same
handlers are testable against the mock server in `ui/` without hardware.

The contract is [`../api/openapi.yaml`](../api/openapi.yaml): 28 paths, checked
against the routes the firmware actually registers by
`scripts/check-openapi.py` on every `just check`.

---

## Routing

The `ROUTES` table in `crates/cc-hal-esp32/src/web.rs` is the authority. It is a
flat list of `(path, method)` pairs, and the handlers are pure functions behind
it.

Three things about it are deliberate:

**A wildcard preflight, not a per-route one.** `OPTIONS /api*` is registered once.
It used to be advertised as `OPTIONS /api/status` with nothing behind it, so a
preflight 404'd.

**Unknown `/api/` paths get a status that says so.** The C++ decided JSON versus
text on `path.starts_with("/api/")` and used one status for all of them. This
splits it out, so the wildcard's shadowing is visible to a caller instead of
silently turning every unknown API path into a `405`.

**The UI is one wildcard.** `/ui*` serves the shell, the assets and the
client-side routes, with the MIME types checked. A deep link to a route that only
exists in JavaScript — `/ui/config/behavior` — has to boot the configuration page
with every live parameter the schema exposes (98 of them), which only works if the fallback serves the shell
*and* the MIME types let the JS and CSS execute. A `200` on `/ui` is not evidence
of any of that.

## The event stream

`GET /events` is a server-sent event stream, consumed by a browser `EventSource`.
Long-lived, no request body, no response envelope.

It is also the reason the display frames the panel in **8** I²C writes rather than
64. The stream, the panel and the ABP2 pressure sensor share the bus, and a write
per pixel row starves the pressure sensor for the sake of telemetry. That was the
failure that made the panel flicker at 10 Hz.

## Authentication

HTTP Basic, decided **at boot** rather than per request. Every route except
`OPTIONS` is behind it, including `/` and `/ui*`.

The C++ sent a wildcard-origin header on every response and had no
authentication at all. Both are divergences.

Clearing the username or the password and rebooting opens the API with a boot
warning. That is the C++'s behaviour and it is deliberate: a machine locked out
of its own API with no serial console is worse than one that says it is open.

## OTA, and the hole in it

Four routes:

| Route | Behaviour |
| --- | --- |
| `GET /api/ota/status` | Answers a real status document |
| `POST /api/ota/firmware` | **Implemented.** Multipart upload, and stricter than the C++: refused while brewing or steaming |
| `POST /api/ota/filesystem` | **Implemented.** Same refusals |
| `POST /api/ota/url` | **Not implemented.** Answers `501` with a JSON body naming the task that deferred it |

Three of the four exist. The fourth is deliberately a `501` rather than a `404`,
because a `404` is indistinguishable from a lost feature.

Full reasoning and the refusal conditions are
[`../history/divergences.md`](../history/divergences.md#d31).

**The OTA has never been exercised on hardware.** It was verified by reading
ESP-IDF v5.5.5, and the bootloader's fallback-to-factory behaviour on a power cut
during the `otadata` write was not verified at all.

## The 98 parameters

All of them are writable over HTTP and survive a reboot. `cc_config::schema`
holds the schema, `POST /api/parameters` applies them, and
[`../config/reference.md`](../config/reference.md) is every key, its type, range
and default.

Three of them are dormant: registered, persisted, and never exercised. They are
named in [`../history/divergences.md`](../history/divergences.md).

## MQTT

`cc-mqtt` holds the topic layout, the publish registry and the value buffers — 19
assertions that ran only under the on-target test runner before it was extracted
into its own portable crate.

Publishing is real and runs. The transport, the auth and the Home Assistant
discovery surface are a different matter; see
[`../status.md`](../status.md) for what is verified and what is not.

## Memory

The web UI is served from **flash**, and that is a deliberate consequence of the
ESP32's 320 KB. A 195,436 B embedded bundle costs **0 B of RAM** because
`cc-hal-esp32/build.rs` embeds it with `include_bytes!`. Static RAM is identical
before and after the UI is included.

Do not "improve" this by buffering the bundle, and do not build large JSON
responses through an intermediate string. Both were real flash and RAM
regressions.
