# Parity report

The checklist T-20 asks for: every behaviour the C++ firmware has, what the Rust port does with
it, and whether the difference is a defect fix, a documented deviation, or a gap.

**How to read a row.**

- **pass** — the behaviour is reproduced and the evidence is a test in this repository.
- **fixed** — the C++ behaviour was a defect; the port does something different, and the defect
  register says which and why.
- **deviation** — the port does something different and it is *not* a defect fix. Each of these
  needs a reason, and each is a decision someone made rather than something the port discovered.
- **gap** — not implemented. The machine does not do this yet.
- **unverified** — implemented and tested on the host, never run on hardware. **Every row that
  touches a physical machine is in this state**, because no board is connected.

Cross-links: [task-list.md](task-list.md), [defects-register.md](defects-register.md),
[compatibility-matrix.md](compatibility-matrix.md), [api-contract.md](api-contract.md).

**The standing caveat.** No hardware was connected for this run, and the Xtensa toolchain could
not be installed on this host, so the ESP32 and S3 images have never been compiled. The C6
compiles and links. A "pass" in the control column means a host test asserts the behaviour against
a recording fake, not that a machine did it.

---

## 1. Functional parity

| Behaviour | C++ | Rust | Row | Evidence |
| --- | --- | --- | --- | --- |
| Machine powers on into temperature control | `INIT` → `PID_NORMAL` | same | pass | `boot::a_runtime_with_the_compiled_defaults_settles_and_heats` |
| Heater follows the PID at a 1 s sample | `PID_v1`, 0..1000 | same library, same window | pass | `domain/src/pid.rs` |
| Heater PWM at a 10 ms window | ISR, `Timing.h:16` | interval kept; ISR not written | gap | — |
| Brew by time, total including pre-infusion | `BrewStates.cpp:279` | same comparison | pass | `scenarios::a_normal_brew_...` |
| Brew by weight | `BrewStates.cpp` | same | pass | `domain/src/transition.rs` |
| Manual brew skips pre-infusion | `brew.mode` 0 | same | pass | `config_rt` + `transition` |
| Pre-infusion, then the pause, then the shot | `BrewStates.cpp:155` | same | pass | `scenarios::a_normal_brew_...` |
| Steam mode at its own setpoint | steam state handler | one PID, target switched in `run_pid` | deviation | the C++ switched the setpoint in the state handler, where a state that moved out of steam would leave the PID aimed at 135 C; here it is derived from the state |
| Manual flush, pump and valve | `MANUAL_FLUSH_RUNNING` | same | pass | `scenarios::a_manual_flush_...` |
| Backflush: N cycles of fill and flush | `BackflushStates.cpp` | same, cycle arithmetic in the domain | pass | `scenarios::a_backflush_...` |
| Backflush never heats | `shouldPIDBeEnabled` | enforced in the domain and in the interlock list | pass | `scenarios` + `board-profiles` |
| Hot water from a panel switch | `PidStates.cpp:33` | same state, and the valve interlock fixed | fixed | **D56** |
| Hot water has its own pump deadline | constant, never armed | armed and enforced | fixed | **D09**, `scenarios::a_stuck_switch_...` |
| Water tank switch stops everything | `WATER_TANK_EMPTY` | same | pass | `scenarios::a_water_tank_emptying_...` |
| Empty tank may keep heating if configured | `keep_heater_on_empty` | same | pass | `heater_allowed` |
| Emergency stop above a threshold | three disagreeing constants | one, from the configuration | fixed | **D33**, `config_rt::the_emergency_threshold_...` |
| Emergency stop is latched | cleared on a falling edge | latched until a power cycle | fixed | **D33**, `scenarios::the_emergency_stop_is_latched_...` |
| Sensor fault stops the heater and recovers | no fault detection at all | fault latches, recovery after the delay | fixed | **D03**, `scenarios::a_sensor_fault_...` |
| A failed read yields no value | a cached 0 C | `None`, always | fixed | **D03** |
| Standby after an idle timeout | `StandbyCoordinator` | a request from the coordinator, feature-gated | pass | `scenarios::standby_is_entered_...` |
| Power switch, boot guard and long-press reboot | `PowerHandler` | same guard, reboot as a flag | pass | `scenarios::a_power_...` |
| Post-brew screen duration | `display.post_brew_timer_duration` | schema key, listed as inert | gap | `config_rt::NOT_READ_BY_THE_MACHINE` |
| Display: six templates, three languages | six templates | six templates, one font | deviation | see §5 |
| Display: refresh only what changed | whole buffer every render | page diff against the last flush | fixed | **D31** |

## 2. Control and safety parity

| Rule | C++ | Rust | Row | Evidence |
| --- | --- | --- | --- | --- |
| Every state that energises an actuator de-energises on exit | per-state, and two of them forgot | one function, tested per state | pass | `scenarios::every_state_that_starts_water_stops_it_on_exit` |
| One valve allow-list | two copies that drifted | one list, one direction asserted | fixed | **D49**, **D56** |
| A pump command carries a deadline | constants, never armed | armed per pump, enforced by the safety task | fixed | **D09** |
| A switch press is an edge, not a level | flags read by reference | a request consumed by one tick | fixed | **D22**, `scenarios::a_stale_request_...` |
| Watchdog fed from a task that can still run | fed once at the top of one loop | fed by the safety task | pass | `tasks::the_safety_task_feeds_the_watchdog` |
| Interlocks cannot be starved by a control-path await | one loop, so no such thing | a separate safety step after every control step | pass | architecture §1.2 |
| A fault outranks a routine transition | four checks in priority order | the same order, one function | pass | `domain/src/transition.rs` |
| Relays are never poked directly | a rule in `CLAUDE.md` | the type system: one owner, no other handle | pass | `hal-traits` |

## 3. API parity

All thirty routes exist with the contract's status codes. `api_routes.rs` covers each in its
success case and in each documented error case; the notable ones:

| Route | C++ | Rust | Row | Evidence |
| --- | --- | --- | --- | --- |
| `GET /api/status` | 15 fields, states by id | identical, scale fields only with a scale | pass | `api_routes::status_returns_...` |
| `POST /api/setpoint` | form or query, `400` | form, query or JSON; `400`; `NaN` refused | pass | `api_routes::the_setpoint_route_...` |
| `POST /api/parameters` | empty values silently skipped | an empty value resets to default and is reported | fixed | **D25** |
| `GET /api/parameters?filter=` | ignored | honoured, unknown filter is `422` | fixed | **D26** |
| `GET /api/config`, `/download` | plaintext secrets | redacted | fixed | **D14** |
| `POST /api/config/upload` | `200` with 90 of 96 rejected | transactional, counts, `400` | fixed | **D13** |
| `GET /api/temperatures` | `200` with an error body | `500` | fixed | **D15** |
| `GET /api/nvs-debug` | plaintext secrets | redacted | fixed | **D14** |
| Four OTA routes | present, unauthenticated, allowed while brewing | present, password, idle-only, URL allow-list, extension check | fixed | **D01**, **D16**, **D17** |
| `POST /api/ota/url` | `202` | `202` | pass | the frontend checks for it |
| Auth | middleware that never authenticated | a real check; `401` on protected routes | fixed | **D10** |
| `GET /` → `/ui/` | `302` | `302` | pass | `api_routes::the_root_redirects_...` |
| The socket that serves them | `AsyncTCP` | a bridge, a router and a connection task, tested end to end | pass | `net::tests::a_get_reaches_the_handler_...` |
| A stalled client | blocked the loop (D39) | two bounded rings; a client that stops reading is dropped | fixed | **D39**, `net::tests::a_stalled_client_is_detected...` |

## 4. Configuration parity

| Behaviour | Row | Evidence |
| --- | --- | --- |
| The repository's `config.json` imports with nothing rejected | pass | `config::the_repository_config_export_imports_cleanly` |
| An export is importable by this firmware's own importer | fixed | **D57**, `an_export_is_importable_by_its_own_importer` |
| `format_version` is accepted, and a newer one is refused in full | fixed | **D58** |
| A rejected document applies nothing | pass | **D13** |
| Every parameter is either read by the machine or listed as inert | pass | `config_rt::every_schema_key_is_either_read_...` |
| The emergency threshold reaches the machine | fixed | **D12**, `config_rt::the_emergency_threshold_...` |
| A corrupt region boots on defaults rather than refusing to start | pass | `store::an_erased_region_falls_back_...` |

## 5. Deviations, with reasons

There are four. Three are the port doing the same thing somewhere more defensible; one is a real
loss of fidelity.

1. **The font is not the C++ font.** The U8G2 bitmaps live in a library PlatformIO fetched, so
   their metrics cannot be read from this repository. The port ships one 5x7 table at two integer
   scales. It is legible and measurable and almost certainly not what the panel looked like
   before. Consequence: every layout is verified against *its own* metrics, and the physical
   appearance is unverified until T-21 runs on a panel. This is the one deviation a user would
   notice.
2. **The post-brew screen duration is inert.** The C++ kept the brew timer on screen for a
   configurable time after the shot; the port shows the finished state for the compiled constant.
   The schema key is registered and listed as inert rather than quietly ignored, so a user who
   sets it is not misled.
3. **The steam setpoint is applied where the state is, not in the state handler.** Same behaviour,
   one fewer place for a transition to get wrong.
4. **The maintenance backflush reminder is not ported.** The C++ counted shots since a backflush
   and nagged. The port publishes the count over MQTT and does not nag. Listed as inert.

## 6. Gaps, in the order they should be closed

1. **The Wi-Fi association and the listener.** The bridge, the router, the connection task and
   all thirty handlers are written and tested end to end; what is missing is `esp-radio` bringing
   up an interface and `embassy-net` accepting a socket. That is a `.await` away, and it cannot be
   exercised in this checkout.
2. **The heater PWM ISR.** The interval and the window are kept, and the duty is a value on the
   actuator trait, but no interrupt drives a pin. The heater therefore has a command and no power
   stage control.
3. **The sensor drivers in the firmware.** The DS18B20, ABP2 and HX711 drivers are written and
   host-tested. The aggregator reads them. Nothing constructs them: the 1-Wire bus is bit-banged
   and cannot be done from an async task without blocking the executor, which is the shape of D05.
4. **The display bus.** The framebuffer, the templates and the page diff are written and tested.
   No SSD1306 driver moves the pages to the panel.
5. **MQTT's socket and the line server's listener.** Both halves exist; the listeners do not.
6. **The relay polarity setting.** `hardware.relays.*.trigger_type` is registered and listed as
   inert: this port wires the relays active-low, which is what the machines in the field have, and
   a machine wired the other way would need a firmware change rather than a setting.

## 7. The rows that cannot be closed without hardware

Everything in §1 that involves a relay, a heater, a panel switch, a tank switch or a sensor, plus
the whole of the display's physical rendering, the provisioning channel on real hardware, and the
heater's timing. `compatibility-matrix.md` tracks each one; the short version is that **nothing in
this port is device-verified**, and a row that reads "pass" above means a host test asserts it
against a fake, not that a machine has done it.

The one thing that would move the most rows fastest is a bench machine flashed with the
`mock-actuators` image, which exercises the entire control path, the API and the config import
with no load energised.
