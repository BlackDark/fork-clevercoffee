# Test fixtures

## `config-export-cpp.json`

A real `GET /api/config/download` export from the **C++ firmware**, supplied by the
project owner on 2026-09-28.

This is the golden fixture for **ORACLE-4** and **DOMAIN-8**. Since
[ADR 0005](../../docs/adr/0005-no-backward-compatibility-usb-flash-migration.md)
dropped backward compatibility, this file format is the **only** compatibility
surface between the C++ and Rust firmware: users export it from the old web UI and
import it after flashing the new one. So the Rust importer is tested against this,
not against what the C++ source suggests the exporter emits.

Shape, as verified:

- nested JSON keyed by dotted path, 10 top-level sections
  (`pid`, `brew`, `steam`, `display`, `hardware`, `backflush`, `maintenance`,
  `standby`, `mqtt`, `system`)
- **96 leaf parameters**, which matches the registered-parameter count derived from
  `Config::getAllConfigParams()` in the inventory
- values are bare scalars, not `{value: …}` wrappers — note the C++ importer accepts
  both, so a reader must tolerate either
- **no `emergency*` key appears anywhere**, which independently confirms inventory
  finding §7.1 item 10 from real device data: `emergencyStopTemp` and
  `emergencyStopHysteresis` are declared in `Config.h` but absent from
  `getAllConfigParams()`, so they never persist and never export. 98 declared minus
  those 2 is exactly the 96 seen here. DOMAIN-4 registers them.

### Credential fields are scrubbed

Eight fields that can carry a real secret were replaced with explicit placeholders
before committing:

| Path | Placeholder |
|---|---|
| `system.wifi.ssid` | `EXAMPLE-SSID` |
| `system.wifi.password` | `EXAMPLE-WIFI-PASSWORD` |
| `system.ota_password` | `EXAMPLE-OTA-PASSWORD` |
| `system.auth.username` / `.password` | `example-user` / `EXAMPLE-AUTH-PASSWORD` |
| `mqtt.broker` | `mqtt.example.invalid` |
| `mqtt.username` / `.password` | `example-user` / `EXAMPLE-MQTT-PASSWORD` |

Nothing else was altered: every key, every structure and all 88 non-credential
values are exactly as exported. The scrub does not reduce the fixture's usefulness —
it tests shape, coverage and round-tripping, none of which depend on those values —
and it keeps a committed file from resembling a credential dump.

**If you regenerate this fixture, scrub it again.** A raw export from a real machine
contains that machine's Wi-Fi password in plaintext.

### The untracked `config.json` at the repo root

That is the unscrubbed original and it is **gitignored**. It is also the path
`docs/wokwi.md` uses as a device seed, so treat a root `config.json` as
potentially-secret working data, never as something to commit.
