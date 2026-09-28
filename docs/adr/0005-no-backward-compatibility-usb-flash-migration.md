# ADR 0005: No backward compatibility — USB flash and config.json migration

## Status

Accepted (2026-09-28)

**Amends:** [ADR 0004](0004-rust-migration-platform-selection.md)
**Related:** [inventory.md](../rust-migration/inventory.md) · [compatibility-matrix.md](../rust-migration/compatibility-matrix.md) · [architecture.md](../rust-migration/architecture.md) · [tooling.md](../rust-migration/tooling.md) · [task-list.md](../rust-migration/task-list.md)

## Context

[ADR 0004](0004-rust-migration-platform-selection.md) was written on the assumption
that the Rust firmware had to be data-compatible with the C++ firmware: read the
same NVS layout, keep the same partition table, and be reachable by OTA from a
deployed device. That assumption drove a large part of its reasoning and several
frozen constraints in [architecture.md](../rust-migration/architecture.md).

**The project owner has removed that requirement.** The stated position is:

- NVS may be restructured.
- The partition layout may change.
- OTA from the old firmware to the new one must **not** be possible.
- Migration is manual and user-driven: flash over USB, then import a `config.json`
  that the user exported from the old web UI.

This removes the single hardest technical constraint in the migration and replaces
it with a much easier one — but it also introduces a safety obligation that did not
exist before, because "not possible" has to be *enforced* rather than assumed.

### Why "OTA is not possible" needs enforcing

ESP-IDF application images are relocatable across OTA slots: the same image boots
from `ota_0` or `ota_1`. So simply changing offsets does not stop the old firmware
from pushing a new image and booting it. The old firmware's web OTA and ArduinoOTA
paths both accept any `.bin` and neither validates what is inside it.

Left unguarded, the failure mode is a device running the Rust firmware against the
C++ partition table: no `ccfs` partition, so no web UI and no asset storage, and a
configuration store that was never initialised. That device still has a heater, a
pump and a valve wired to it. This is precisely the half-migrated state that must
not be allowed to run.

## Decision

**Break compatibility deliberately and explicitly, and enforce the break at boot.**

### 1. New partition table

`partitions_rust_4m.csv` replaces `partitions_4M.csv` for the Rust firmware. The
C++ table stays in the tree unchanged, because the C++ firmware remains the parity
oracle and must keep building.

| Name | Type | SubType | Offset | Size |
|---|---|---|---|---|
| `nvs` | data | nvs | 0x9000 | 20 KB |
| `otadata` | data | ota | 0xE000 | 8 KB |
| `app0` | app | ota_0 | 0x10000 | 1664 KB |
| `app1` | app | ota_1 | 0x1B0000 | 1664 KB |
| **`ccfs`** | data | **littlefs** | 0x350000 | 640 KB |
| `coredump` | data | coredump | 0x3F0000 | 64 KB |

App slot geometry is **unchanged on purpose**, so SPIKE-1's measured size budget
(58.5 % of 1664 KB) still applies without re-verification. The only substantive
change is the filesystem partition: `spiffs` → `ccfs`, and the subtype corrected
from `spiffs` to `littlefs`, which is also simply more honest — the C++ table
declared subtype `spiffs` while formatting LittleFS.

Dual app slots are kept. The prohibition is on **old → new** OTA; **new → new** OTA
remains a wanted feature.

### 2. Boot-time layout guard

On startup, after the actuators are driven safe and before anything else, the
firmware requires a partition labelled **`ccfs`** with subtype `littlefs`. If it is
absent:

1. all actuators are already safe (this check runs after step 1 of the startup
   sequence, never before);
2. the firmware logs a single explicit message naming the cause and the remedy;
3. it **halts** — it does not start the control task, does not arm the heater ISR,
   and does not proceed to Wi-Fi.

The C++ firmware cannot produce a `ccfs` partition, and its filesystem-OTA path
targets the literal label `spiffs`, so it cannot write this partition either. An
app-only OTA from the old firmware therefore lands on a device that refuses to run
rather than one that runs incorrectly.

This is a safety mechanism, not a licence check. It is not meant to be
unbypassable by someone with a USB cable — it is meant to make the half-migrated
state inert.

### 3. Restructured configuration storage

The C++ scheme is abandoned rather than reproduced. For reference, what it did:
NVS namespace `config`, one key per parameter named `"p" + fnv1a32(dotted.path)` in
hex, written through to flash on **every individual `set()`**, with no schema
version, no migration hook and no collision detection on the 32-bit hash.

The Rust scheme:

| Namespace | Key | Type | Contents |
|---|---|---|---|
| `wifi` | `ssid` | str | Wi-Fi SSID |
| `wifi` | `pass` | str | Wi-Fi password |
| `cfg` | `ver` | u16 | schema version |
| `cfg` | `blob` | blob | the whole configuration, `postcard`-encoded |

Why this shape:

- **One versioned blob instead of ~97 keys.** The 15-character NVS key limit is
  what forced the C++ to hash its keys in the first place; a single blob removes
  the problem rather than working around it. Saving becomes one atomic write
  instead of 97 open/write/close cycles, which also removes the flash-wear and
  latency problem the C++ has on every bulk parameter POST.
- **`ver` is separate and plain**, so a future migration can read the version
  without being able to decode the blob.
- **Wi-Fi credentials stay as two plain string keys**, deliberately outside the
  blob. They must be writable by a host tool over USB before any firmware has run,
  and keeping them independent means provisioning does not need to understand the
  configuration schema at all. It also means a configuration factory-reset does not
  drop the network, and re-provisioning does not reset configuration — two things
  that are tangled in the C++ (which keeps Wi-Fi in WiFiManager's own private NVS
  area, separate again from `config`).

### 4. `config.json` is the migration interface

The old web UI's `GET /api/config/download` produces nested JSON keyed by dotted
path. **That file format is the compatibility surface** — the only one. The Rust
firmware must import it, and its own export must stay in the same shape so users
can move between versions.

This is a much better place for the compatibility boundary than the NVS binary
layout: it is human-readable, diffable, versionable, testable on the host, and it
already exists and is already documented (`docs/example_config.json`).

## Consequences

### Removed from the plan

- **SPIKE-3 (NVS continuity against a real device)** — deleted. It was the most
  important unverified claim in the plan and it no longer needs to hold.
- **ORACLE-3 (capture NVS key vectors from the C++)** — deleted, replaced by
  ORACLE-4, which captures a real `config.json` export as a golden fixture.
- The `"p" + FNV-1a` key derivation, the Arduino `Preferences` float/double-as-blob
  rule, and the frozen partition table all stop being constraints. Architecture
  constraints **C6** and **C7** are struck out.
- Prerequisite **P0** collapses from three blocking questions to one courtesy
  question, because the partition table is now ours to choose and no device with
  preserved configuration is needed.

### Added to the plan

- **BOOT-1** — the layout guard above, with a host-testable decision function.
- **DOMAIN-8** — `config.json` import and export, including tolerance for the old
  format's quirks (it accepts either a bare value or a `{value: …}` wrapper, and
  explicitly rejects legacy flat dotted-key documents).
- **ORACLE-4** — capture a real `config.json` from the C++ firmware as a fixture.
- A documented **user-facing migration procedure**: export, flash, provision,
  import.

### Effect on ADR 0004

The platform decision **does not change**, but its basis does, and that is worth
being honest about.

ADR 0004's argument 1 ("config continuity is provable on A and speculative on B")
is now **moot** — it was the strongest argument for ESP-IDF/std and it has
evaporated. Argument 2 (serving the deployed LittleFS web UI) is **weaker**: with
the layout free, a bare-metal build could define its own asset format on a data
partition and stream it from flash, so the gap shrinks from "no path exists" to
"needs a prototype".

The arguments that still stand, and that carry the decision on their own:

- `esp-radio` is pre-1.0 with appliance-hostile open issues —
  [#5889](https://github.com/esp-rs/esp-hal/issues/5889) (reconnect stays broken
  after an authentication failure) and
  [#1600](https://github.com/esp-rs/esp-hal/issues/1600) (no WPA3 station on
  ESP32). A coffee machine that quietly stops rejoining the network is the worst
  failure mode for support.
- Measured bare-metal RAM: ~320 KiB of DRAM essentially fully allocated by the
  official Wi-Fi examples before any application code, with stack collapsing to
  54 KiB once BLE coexistence is on.
- The threads-and-tasks model maps onto the existing single-loop architecture;
  bare metal would mean an async-first rewrite stacked on top of an already large
  behavioural port.
- Everything needed already has a wrapper: Wi-Fi, HTTP with streaming bodies, MQTT
  with LWT and retain, OTA with rollback, NVS, LittleFS, raw partitions, mDNS.
- **SPIKE-1 measured** the std path fitting at 58.5 % of the app partition.
  Switching now would trade a verified result for an unverified one.

So: **the margin narrowed and the decision held.** ADR 0004's revisit conditions
are updated accordingly — the two gaps that decided against bare metal are no
longer both blocking, so a future reopening needs a lower bar. If the Wi-Fi
maturity and RAM questions are ever answered favourably, bare metal becomes a
genuine candidate rather than a fallback.

### Risks this introduces

1. **Users who do not export their configuration before flashing lose it.** There
   is no recovery path — the old NVS is not read. This must be the first line of
   the migration documentation, not a footnote.
2. **`config.json` export must be verified to round-trip before anyone relies on
   it.** ORACLE-4 exists for that. Note the old import path returns success if
   **≥ 1** parameter updated, so a mostly-garbage file reports success today; the
   Rust import must be stricter and report exactly what it accepted and rejected.
3. **The guard could strand a user** who flashes the app but not the partition
   table. Mitigation: the halt message must name the cause and the fix, and
   `just flash` writes the partition table as part of the same operation.
4. Two partition tables now coexist in the tree. Mitigation: the C++ build keeps
   `partitions_4M.csv`, the Rust build uses `partitions_rust_4m.csv`, and neither
   references the other.
