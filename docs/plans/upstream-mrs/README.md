---
title: Upstream MR port plans (rancilio-pid/clevercoffee)
source: https://github.com/rancilio-pid/clevercoffee/pulls?q=is%3Apr+is%3Aopen
created: 2026-09-11
status: planned
tags:
  - upstream-mr
  - index
---

# Upstream MR port plans

Open PRs on `rancilio-pid/clevercoffee` vs this fork. Cherry-pick is dead — procedural `src/*.h` vs class state machine. Each note is the architecture-matched plan.

`advise`: include | defer | skip. `priority`: 1 = do first. `stake`: why that call.

## Include

| P | PR | Advise | Effort | Risk | Stake |
|---|----|--------|--------|------|-------|
| 1 | [0642](0642-wifi-offline-lockout.md) | include | S | low | Reconnect exhaustion latches offline forever; MQTT dead until reboot |
| 2 | [0640](0640-momentary-power-switch.md) | include | S | low | Momentary press from `PID_DISABLED` powers off instead of on |
| 3 | [0639](0639-power-on-behaviour.md) | include | M | medium | Momentary/OTA boot always heats; default Standby is a behaviour break |
| 4 | [0636](0636-nimble-role-flags.md) | include | S | low | Strip unused NimBLE roles, ~21 kB flash |
| 5 | [0641](0641-mqtt-own-task.md) | include | S→L | low→high | Phase 1: ungate RSSI + timeout + no WiFi PS. Task only if Phase 1 fails on device |
| 6 | [0645](0645-diagnostics-coredump.md) | include | M | medium | Subset: retained `resetReason`/`crashInfo` + auth’d chunked coredump. No metric spam |

## Defer / skip

| P | PR | Advise | Effort | Risk | Stake |
|---|----|--------|--------|------|-------|
| 7 | [0620](0620-flow-rate-predictive-weight.md) | defer | S | medium | Naive weight stop already matches pre-620; if overshoot, `stop_offset` before LS |
| 8 | [0622](0622-auto-boiler-refill.md) | defer | M | high | Real need; dedicated `BOILER_REFILL=52` later, not a fake hot-water press |
| 9 | [0630](0630-fullscreen-brew-pressure.md) | skip | S | low | Modern already shows pressure; PR cursor jumps fail OLED rules |

## Frontmatter

```yaml
advise: include | defer | skip
stake: "<one sentence why>"
effort: S | M | L
risk: low | medium | high
priority: 1-9   # 1 = first
status: planned
```

Do not copy upstream `mqtt.h` / `embeddedWebserver.h` / `powerHandler.h` / `brewHandler.h`. Pump/valve/heater only via `HardwareManager`. OLED: 128×64, fixed-width fields, `setFontPosTop()`.
