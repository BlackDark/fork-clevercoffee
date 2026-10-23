# 0005: Settings storage, provisioning and updates

Status: accepted, 2026-10

## Decision

- **No compatibility** with the C++ firmware's NVS/LittleFS data: users flash over USB once and set up again.
- **Settings** in partition `cc_cfg` (128 KB) with `sequential-storage` (wear-levelled, power-fail safe). One entry per parameter, key = parameter key, value = exact binary form (`cc_core::config::stored`). Only values that differ from the last save are written, 2 s after the last change. Unknown keys are ignored and values that no longer fit their type fall back to the default, so adding or removing parameters needs no migration.
- **Provisioning**: `cc_prov` (4 KB) holds `CCPROV1` + `key=value` lines written over USB (`just provision`), applied once at boot, then erased. Same validation as the API.
- **Web UI in the firmware** (gzipped, ~200 KB, `build.rs`). No filesystem partition, no separate UI update: one image is always consistent.
- **Updates**: two 1.875 MB OTA slots; upload only (`/api/ota/filesystem` and `/api/ota/url` return 410). The image must start with the ESP magic byte; the running image confirms itself at boot. Actuators are off during the write.
- espflash rejects custom data subtypes, so `cc_cfg`/`cc_prov` use `undefined` and are found by label.

## Consequences

- Factory reset erases `cc_cfg`; WiFi reset clears only the credentials.
- Rollback on a crashing image needs a bootloader with rollback enabled (docs/todo.md).
