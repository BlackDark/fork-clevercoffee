---
title: Strip unused NimBLE peripheral/broadcaster roles
upstream_pr: 636
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/636
category: take
advise: include
stake: "Dual-OTA 4MB app slots (0x1A0000); we are BLE central+observer only; flags are valid in NimBLE 2.5.1 and reclaim ~21 kB flash."
effort: S
risk: low
priority: 4
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

## Verdict

Include. Fork is BLE central/observer only (Acaia scan + GATT client). NimBLE 2.5.1 (`platformio.ini` pin; `NimBLECppVersion.h` PATCH 1) honors `CONFIG_BT_NIMBLE_ROLE_*_DISABLED`. Arduino ESP32 `sdkconfig.h` does not predefine those roles, so the flags take effect. Skip only if a post-change `firmware.bin` is not smaller.

## Upstream

`platformio.ini` `build_flags` only (+2 lines):

- `-D CONFIG_BT_NIMBLE_ROLE_PERIPHERAL_DISABLED` — no GATT server / accept inbound connections
- `-D CONFIG_BT_NIMBLE_ROLE_BROADCASTER_DISABLED` — no advertising

Author tested Bookoo Themis Ultra. No firmware source changes.

## Current fork

- Flags: none. `[esp32_v1_base]` in `platformio.ini` already has the same AsyncTCP/log flags; NimBLE is `h2zero/NimBLE-Arduino @ 2.5.1`.
- BLE: `include/clevercoffee/hardware/scales/BluetoothScale.h`, `src/hardware/scales/BluetoothScale.cpp` wrap `AcaiaArduinoBLE` (`#v4.0.1`).
- Acaia v4.0.1: `NimBLEDevice::init`, `getScan()`, `createClient()`, advertised-device callbacks. No `NimBLEServer`, `createServer`, `getAdvertising`, or `startAdvertising`.
- Dual-OTA: `partitions_4M.csv` `app0`/`app1` = `0x1A0000` (1,703,936 B). Remeasure `firmware.bin` on a current build.

NimBLE 2.5.1 `nimconfig.h` size comments: peripheral ≈16 kB, broadcaster ≈5 kB, **≈21 kB**. Mapping is valid: `_DISABLED` → `CONFIG_BT_NIMBLE_ROLE_*=0` → `MYNEWT_VAL_BLE_ROLE_*=0` in `esp_nimble_cfg.h`. Do **not** set `ROLE_CENTRAL_DISABLED` or `ROLE_OBSERVER_DISABLED` (scan+connect would vanish).

## Need it?

Yes. Dual-OTA 4MB is flash-tight; this is free bytes with no feature loss. RAM is unrelated.

## Plan

1. Record baseline size: `ls -l .pio/build/esp32_usb/firmware.bin` after `~/.platformio/penv/bin/pio run -e esp32_usb -s` (or note missing). → check: number written down.
2. Add both `-D` flags to `[esp32_v1_base] build_flags` in `platformio.ini` (next to the existing `ASYNC_TCP_SSL_ENABLED=0` line). Do not add them to `[env:native_test]`. → check: `rg ROLE_PERIPHERAL_DISABLED platformio.ini` shows them only under `[esp32_v1_base]`.
3. Rebuild: `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s` then `~/.platformio/penv/bin/pio run -e esp32_usb -s`. → check: build OK; `firmware.bin` **smaller by ~15–21 kB**. If delta ≈0, flags did not apply — stop and switch to `-D CONFIG_BT_NIMBLE_ROLE_PERIPHERAL=0` / `BROADCASTER=0` (Arduino comment form) and rebuild once.
4. `~/.platformio/penv/bin/pio test -e native_test`. → check: pass (BLE not compiled here; this is the CI gate only).
5. Hardware: BLE scale type, power scale, confirm scan+connect+weight. → check: log `Bluetooth scale connected` and live grams. Cannot prove this native.

## Do not copy

- Upstream `-std=gnu++17` / other `platformio.ini` lines.
- `ROLE_CENTRAL_DISABLED` / `ROLE_OBSERVER_DISABLED`.
- Extra NimBLE knobs (`MAX_CONNECTIONS`, log levels, mbedtls). This PR is two flags.

## Tests / verification

- Format + `pio run -e esp32_usb -s` + size delta (required).
- `pio test -e native_test` (required; does not exercise NimBLE).
- Device: scan, connect, weight, tare, disconnect/reconnect. Native cannot test BLE.

## Risks

Wrong/extra flags: compile fail (good) or silent connect fail (bad). Mitigate: keep central+observer; require the size drop; smoke-test a real scale before merge.
