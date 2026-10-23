# Glossary

Terms as the code and docs use them.

| Term | Meaning |
| --- | --- |
| **Machine** | `cc_core::Machine`: the state machine that owns all control decisions. Fed `Inputs` every tick, produces `Outputs` and `Events`. |
| **Tick** | One 10 ms control step: read switches, take sensor values, `Machine::tick`, drive relays, feed the watchdog. |
| **State** | `cc_core::State`, numbered as in the C++ firmware (`machineState` in the API): Init 0, PidNormal 20, Brew* 31–34, ManualFlush 36, Steam 51, Backflush* 60–63, WaterTankEmpty 70, EmergencyStop 80, PidDisabled 90, Standby 95, SensorError 100. |
| **Request** | A wish from a switch, the web UI or MQTT (start brew, steam on, …). Valid for one tick; the machine acts on it or drops it. Standby is the exception: it latches. |
| **Interlock** | A rule that overrides outputs regardless of state: tank empty → pump off, emergency → all off, firmware update → all off. |
| **Outputs** | Logical heater/pump/valve/LED states. The firmware maps them to pin levels with the relay trigger type and LED inversion. |
| **Heater window** | 1 s period in which the PID output is a duty cycle (heater on for `power %` of the window). |
| **Brew detection (BD) PID** | Second tuning set used during and right after a shot. |
| **PonM** | Proportional on measurement: a PID mode with less overshoot. |
| **Shot counter** | Shots since the last backflush; drives the backflush reminder. |
| **Parameter** | One entry of `config.rs`'s `define_params!` table, addressed by its key (`pid.regular.kp`). The same key in the API, config files, MQTT and storage. |
| **Reboot parameter** | Read only at boot (pins, sensor types, WiFi). The UI marks it; changing it needs a restart. |
| **Profile** | Firmware image flavour: `minimal`, `standard`, `full`, `bench` (Cargo feature sets, ADR 0003). |
| **Provisioning** | Settings written over USB into the `cc_prov` partition (`just provision`), applied once at the next boot, then erased. |
| **Portal** | The access point `CleverCoffee-XXXX` with captive DNS and DHCP, opened when no WiFi is configured or it fails for 2 minutes. |
| **OTA slot** | One of the two app partitions; an update writes the inactive one and switches on success. |
| **Template** | One of the six display layouts (standard, minimal, temp only, scale, upright, modern). |
| **Bench** | An ESP32 with LEDs and buttons instead of relays and a machine (`docs/bench.md`). |
| **Oracle** | The C++ firmware v1.3.4: its tests and behaviour were the specification for the port (ADR 0004 lists the deliberate differences). |
