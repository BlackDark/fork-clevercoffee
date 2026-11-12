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
| A sensor error that began in standby recovered into PID: a few bad readings woke the idle machine, which heated until the standby timer ran out again. | Recovery returns to standby unless the machine was woken meanwhile (`fix_sensor_error_in_standby_returns_to_standby`). |
| Once the PID was off, only waking from standby switched it on again: the state reset the runtime flag every tick, so `/api/pid` answered `pidEnabled: true` and the machine stayed in PID_DISABLED. `/api/pid` also flipped the stored setting, not what the UI shows, and `pid.enabled` set by MQTT, the parameters API or a config upload never reached the running machine. | The PID can be switched on from PID_DISABLED; the toggle flips the shown (runtime) state; a changed `pid.enabled` is applied unless the machine is in standby or the emergency stop (`fix_pid_can_be_switched_on_again`, `fix_pid_toggle_flips_what_the_ui_shows`, `fix_pid_enabled_set_outside_the_pid_command_applies_at_runtime`, `fix_pid_setting_changed_in_standby_applies_after_waking`). |
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
| WiFiManager portal. | Own portal with the same UI (`CleverCoffee-XXXX`, password `CleverCoffee`); the full UI is reachable on the access point. When the home network comes back the device restarts once idle, which closes the access point. |
| A TSIC frame with disturbed timing could decode to a wrong value that passed parity (50-99 °C among 56 °C readings in the history). | The temperature is the median of the last three frames (`SpikeFilter`, `fix_tsic_removes_a_single_spike_in_either_direction`): one reading of delay, a single wrong frame never reaches the control loop, and the filter restarts after a sensor fault. Two wrong frames in a row would still pass. |
| TSIC bits were timed in a GPIO interrupt (C++ `ZACwire` library); with WiFi and the control loop running about a quarter of the frames were lost and some decoded to a wrong value that passed parity (50–99 °C among 56 °C readings). | The RMT peripheral measures the pulses in hardware, one reception per frame: on the bench 0 parity and framing errors in 10 min and under HTTP load, bit margin 25 µs (`fix_tsic_decodes_an_rmt_capture`, `rejects_incomplete_captures`). About 1 frame in 120 ends in an RMT `ReceiverError` and is lost (the 10 s log line counts it). A median of three frames (`SpikeFilter`) stays as defence. |
| Unlimited server-sent event streams (AsyncWebServer). | Two (one with a Bluetooth scale); a further `/events` gets 503 and the UI polls (ADR 0008, `server_sent_events`). |
| MQTT 3.1.1. | MQTT 5. |
| Parameter labels and help texts from the firmware (`/api/parameter-help`, `label`/`helpText` in `/api/parameters`). | The UI owns them (Paraglide, ADR 0009); the route is gone. |

## Display

The layout tests found and the port fixed: the modern template's brew row, the upright status word, the update screen title, the German sensor-error text and the Spanish pressure line overflowed the 128×64 panel; the brew-time clear box was too small; the maintenance footer was drawn at y = 62, off-screen (dropped; the status bar shows "CLEAN").
