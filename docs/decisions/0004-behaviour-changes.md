# 0004: Behaviour changes against the C++ firmware

Status: accepted, 2026-10

## Context

The C++ firmware v1.3.4 and its tests were the specification. Porting them line by line exposed bugs; the decision was to fix them rather than reproduce them. Each fix has a test, most named `fix_*` (`crates/core/tests/machine.rs`, `crates/mqtt`, `crates/display`).

## Machine

| C++ behaviour | Now |
| --- | --- |
| The pump timer was never started: brew and hot water ran without a limit. | Brew stops after 5 min, hot water after 60 s (`fix_brew_is_capped`, `fix_hot_water_is_capped`). |
| `/api/steam` changed only the setpoint, the state stayed PidNormal. | Steam via API enters the Steam state like the switch (`fix_api_steam_toggle_enters_the_steam_state`). |
| A brew press during steam or while the tank was empty was replayed later. | Requests last one tick; nothing is replayed (`fix_brew_press_during_steam_is_not_replayed`, `fix_brew_toggle_flipped_while_tank_empty_does_not_brew_after_refill`). |
| Waking from standby with the brew switch also started a shot. | The press only wakes (`fix_wake_by_brew_press_does_not_start_a_shot`). |
| The "normal operation" request lingered: after any web activity standby did not stick. | Standby latches; activity wakes once (`fix_standby_sticks_after_web_activity`). |
| A momentary long press in backflush idle never started a manual flush. | Short press starts a backflush cycle, long press flushes while held (`fix_momentary_long_press_in_backflush_idle_flushes_while_held`). |
| The hot-water timer was never set; the manual-flush time on the display was stale. | Both shown and reset correctly. |
| Brew weight was zeroed right after the shot. | It stays visible during the post-brew display time. |
| Scale tare and calibration flags were set by the UI but never consumed. | Tare and calibration (tare 3 s, place the known weight 10 s) work. |
| A scale fault forced SENSOR_ERROR and switched the heater off. | Only the temperature sensor causes SensorError; a scale fault is reported as `scaleFault`. |
| EEPROM_ERROR state existed but was never entered. | Removed. |
| `safety.emergency_*` were not exposed. | They are parameters. |

## Interfaces

| C++ behaviour | Now |
| --- | --- |
| HA discovery for "Use PonM" pointed to `usePonM`, the command topic was `pidUsePonM`. | Both use the same topic (`fix_ponm_switch_discovery_matches_its_command_topic`). |
| Filesystem and URL updates. | Firmware upload only; the UI is inside it (ADR 0005). |
| Settings survived updates in NVS. | New storage, no import from C++ (ADR 0005). |
| WiFiManager portal. | Own portal with the same UI (`CleverCoffee-XXXX`, password `CleverCoffee`); the full UI is reachable on the access point. |
| Unlimited server-sent event streams (AsyncWebServer). | Two per network (one on the portal); a further `/events` gets 503 and the UI polls (ADR 0008, `server_sent_events`). |
| MQTT 3.1.1. | MQTT 5. |

## Display

The layout tests found and the port fixed: the modern template's brew row, the upright status word, the update screen title, the German sensor-error text and the Spanish pressure line overflowed the 128×64 panel; the brew-time clear box was too small; the maintenance footer was drawn at y = 62, off-screen (dropped; the status bar shows "CLEAN").
