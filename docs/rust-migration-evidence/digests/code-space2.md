# `refactor/space2` — knowledge mined from the IMPLEMENTED Rust source

Scope read: `crates/**` (17 crates), `spikes/**` (4), `tools/provision/**` (6 files), `justfile`.
~30 200 lines of `.rs`, 668 `#[test]` functions, 30 201 lines total.

**The single most important thing this digest has to say up front, because it changes how every
other section should be read:** the branch's own code states, repeatedly and unambiguously, that
**nothing in it has run on hardware.** There is no section 2 of hard-won on-the-bench numbers
waiting to be extracted, because there were no benches. The code is emphatic and self-aware about
this, and the correct reading of the corpus is "a very large, very carefully reasoned port whose
only *empirical* evidence is a host test suite plus one C6 compile". I have written section 2
anyway, because the hardware-derived *constraints* the code encodes (strapping pins, input-only
GPIOs, I2C bus width, flash partition geometry) are real knowledge — but every row is marked
**inferred** or **compiled-not-run**, and I have not dressed any of it up as a measurement.

The hard-won material that *does* exist is real and valuable, but it is knowledge about the **C++
codebase and the hardware**, discovered by reading both trees against each other and by
discovering that two of the port's own mechanisms were wrong the first time they were written
(e.g. `machine.rs:748`, where the author says in a comment that expressing service mode as "PID
off" "made the cross-cutting PID check eject the machine from whatever it was doing the moment a
provisioning session or an OTA began, so the machine lost its state and had to be restarted by
the user afterwards"). That is the flavour of discovery that is actually in this tree.

---

## 1. Problems discovered by IMPLEMENTATION (not by reading the C++)

Note: for this branch these are overwhelmingly *"found by reading the C++ and the HAL contract"*,
not *"found by running it"*. The "Pinned by a test?" column is the useful part: **empty means the
finding lives only in a comment and will be re-broken.**

| # | Problem | What actually happens | Where (file:line) | Pinned by a test? |
|---|---|---|---|---|
| 1 | A C++ emergency-stop threshold that no user could ever set. `safety.emergency_temp` was read by the emergency-stop manager but was never in the registry, so it was never stored, exported, imported, or API-settable. | Lowering the threshold did nothing, silently. | `crates/config/src/schema.rs:261` (the two `P_SAFETY_*` rows), `crates/app/src/config_rt.rs:5-6` | **Yes** — `the_emergency_threshold_and_hysteresis_both_reach_the_machine` (`config_rt.rs:554`) and `every_schema_key_is_either_read_or_explicitly_accounted_for` (`config_rt.rs:618`) |
| 2 | The config→machine seam did not exist and had to be built. Schema knows 99 parameters; the machine wants ~30. | Before this file, a parameter could exist in the schema and reach nothing — exactly D12's shape. | `crates/app/src/config_rt.rs:3-6`; `READ_KEYS` list at `config_rt.rs:431-465` (32 keys) | **Yes** — `config_rt.rs:618` and `nothing_is_listed_as_read_that_does_not_exist` (`config_rt.rs:637`) |
| 3 | `standby.time` of 0 is a legal schema value meaning "never idle", but read as a boolean it meant "idle immediately". | The machine would go to standby on its **first tick**. | `crates/app/src/machine.rs:63-65`; guard at `machine.rs:808` | No (comment-only; the guard `standby_timeout_ms == 0` is untested) |
| 4 | Service mode expressed as "PID off" ejects the machine out of whatever state it was in, because the PID-off check is a cross-cutting transition. | A provisioning session or an OTA started mid-brew silently destroyed the machine's state; the user had to restart it by hand afterwards. | `crates/app/src/machine.rs:748-753` | **Yes** — `a_provisioning_session_over_a_running_brew_holds_everything_off` (`boot.rs:137`) and `the_service_mode_holds_everything_off_whatever_the_state_says` (`scenarios.rs`) |
| 5 | The Arduino-PID library seeds its input filter with zero, so the first derivative term is `(0 - input)/dt * kd`, about **−14000** with the shipped gains. | The output clamps to zero and **the heater never starts**. | `crates/domain/src/pid.rs:297-306` | **Yes** — `the_first_compute_is_not_swamped_by_a_derivative_transient` (`pid.rs:299`) asserts `d_term == 0.0` and `output > 0.0` |
| 6 | The hot-water switch is held inside `PID_NORMAL`/`STEAM_RUNNING`, but the C++ valve interlock closed the valve in both of them. | The hot-water dispense ran the pump **with the valve shut**. D56. | `crates/domain/src/state.rs:127-133` | **Yes** — `state.rs:256-273` asserts the allow-list and that exactly **8** states may hold the valve open |
| 7 | The C++ contradicts itself on the backflush flush phase: `onEntryImpl` closes the valve, the same file logs "flushing into drip tray", and the interlock whitelists `BACKFLUSH_FLUSHING` as valve-open. | With the valve shut the group cannot drain — the phase has no purpose. D49. | `crates/domain/src/transition.rs:440-448`; `machine.rs:1107` | Partly — a test asserts the DRAIN command, but the *decision to deviate* is comment-only |
| 8 | A missing or stuck load cell made the C++ firmware hang forever in `while(!startMultiple(...))`. | No display, no web interface, no way to fix it without a programmer. D29. | `crates/drivers-scale/src/lib.rs:6-9`, `:27`, `:318` | **Yes** — `STARTUP_TIMEOUT_MS = 5000`, `InitError::Timeout`; the C++ waited forever |
| 9 | `getWeight()` returned whatever the last `update()` produced, with no freshness signal. | A brew that stops on weight could act on a reading several hundred ms stale. | `crates/drivers-scale/src/lib.rs:12-15` | **Yes** — `weight_g()` returns `None` until a conversion has completed |
| 10 | The ABP2 read **7** bytes; the part returns **12** (two 6-byte words). | The C++ built its temperature count from bytes 4–6, one of which is the second word's *status* byte. **Wrong by construction.** D54. | `crates/drivers-pressure/src/lib.rs:31-35`, `DATA_LEN = 12` at `:37` | No — comment-only, no test simulates the 12-byte frame layout |
| 11 | The ABP2 read was `send convert; delay(10); read`. | 10 ms with the main loop stopped, ten times a second at 10 Hz. D08. | `crates/drivers-pressure/src/lib.rs:6-10`; `SETTLE_MS = 10` at `:27`; split into `start_conversion`/`read` at `app/src/sensors.rs:44` | **Yes** — `the_pressure_driver_is_stepped_on_its_period_and_never_blocks` (`sensors.rs:530`) |
| 12 | A TSIC `static` change-rate flag never reset, so a sensor that came back after a fault stayed on the 5 °C filter and rejected its own first valid reading. | The sensor could never recover without a power cycle. | `crates/drivers-tsic/src/lib.rs:88-92`, `:361-366` | **Yes** — `a_reset_returns_to_the_wide_rate`, `a_failed_read_does_not_latch_the_tighter_rate`, `an_out_of_range_read_does_not_latch_either` |
| 13 | A 12-bit DS18B20 conversion takes 750 ms, but the firmware's temperature tick is 400 ms. | Reading on the tick returns the *previous* conversion — a stale number presented as current. | `crates/ds18b20/src/lib.rs:26-32`, `:503-511`; default resolution 11 bits chosen because `375 ms < 400 ms` | **Yes** — `the_conversion_time_matches_the_resolution` (94/188/375/750), `a_conversion_is_pending_until_it_is_waited_out`, `the_default_resolution_is_eleven_bits_as_the_cpp_firmware_used` |
| 14 | The DS18B20 powers on with **85 °C** in the scratchpad as "no conversion performed yet" (raw 1360). | A machine that boots believing the boiler is at 85 °C then holds a cold group head at 95 °C for a whole brew. | `crates/ds18b20/src/lib.rs:270-275` | **Yes** — `a_scratchpad_with_the_85c_placeholder_is_refused` (`ds18b20.rs:380`) |
| 15 | At 9/10/11-bit resolution the unused low bits of the raw register are **stale bits from the previous conversion**, not a coarse reading. | Reading all 16 bits gives a plausible, wrong, drifting number. | `crates/ds18b20/src/lib.rs:259-266`, `:279-285` | **Yes** — `each_resolution_masks_away_the_bits_it_does_not_fill` fills the dead bits with `0b111` and asserts the decoded value is still exactly 90.0 |
| 16 | A DS1820 on the same bus answers the reset and would be read with DS18B20 commands. | Plausible nonsense presented as a temperature. | `crates/ds18b20/src/lib.rs:121` | **Yes** — `a_non_ds18b20_on_the_bus_is_refused_at_bind_time` |
| 17 | The C++ imported numbers via `String(double)`, truncating to two decimals. | `pid.ema_factor` of 0.005 imported as **0.01**. D27. | `crates/config/src/schema.rs:177-180` | **Yes** — `a_type_mismatch_is_never_accepted`; and `import.rs:298` refuses to narrow a fractional value into an integer parameter at all |
| 18 | The C++ importer applied each parameter as it parsed and returned success if ≥1 of 96 matched. | A file with 5 valid and 91 invalid values persisted the 5, and answered `200 "Configuration validated and applied successfully."` D13. | `crates/config/src/import.rs:5-9`; API side `app/src/api.rs:22-23`, `:949-952` | **Yes** — `a_payload_with_a_rejected_field_produces_no_configuration_at_all`; `an_out_of_range_value_blocks_the_whole_import` |
| 19 | The C++ `set()` range-checked but `loadFromNvs()` did not. | A corrupt or hand-edited blob put an arbitrary setpoint into live control. D11. | `crates/config/src/schema.rs:161-165`; fixed by construction in `storage/src/lib.rs:1-11` | **Yes** — the range is re-checked on decode *and* on write; `a_corrupt_crc_is_rejected`, `a_length_that_does_not_fit_is_rejected_before_any_slice_happens` |
| 20 | `set_heater_duty` was a no-op and the *relay* command drove the heater pin. | A 40 % duty was **100 %**. A machine that ignores its PID. | `crates/app/src/heater.rs:20-22`, `:65-70`; board side `bsp-esp32c6/src/lib.rs:401-406` | **Yes** — `a_stage_with_no_power_control_holds_the_heater_off`, and the fail-safe is `HeldOff`, deliberately **not** "full power" |
| 21 | A duty of 1 permille rounded to 0 % . | A heater asked for 0.1 % got nothing, and the PID could not control the bottom of its range. | `crates/app/src/heater.rs:37-51` (`div_ceil` ceiling) | **Yes** — `the_smallest_non_zero_duty_is_still_one_tick` |
| 22 | The C++ printed `pidOutput / 10` with one decimal. | **100.0 % was displayed at both 999 and 1000** — a machine at 99.9 % claimed full power. | `crates/app/src/heater.rs:53-58`; test at `:196-208` | **Yes** — `the_display_percentage_never_shows_a_hundred_when_the_duty_is_not_one` |
| 23 | The C++ emergency thresholds were **three sets that disagreed**: a `GlobalTypes` constant of 145 °C, a `Temperature.h` pair of 145/120 °C, and the live config of 150 °C clearing at 100 °C. D33. | Reading the wrong one gives the wrong safety bound; any of them is defensible and none of them is right. | `crates/domain/src/emergency.rs:3-9`, defaults at `:33-42` | **Yes** — `an_emergency_stop_from_every_state_leaves_everything_off` walks all 12 live states |
| 24 | A `NaN` passes every comparison in the C++ range check. | The C++ treated NaN as a valid reading and **kept heating**. | `crates/domain/src/emergency.rs:100-105` | Partly — the guard exists; no test in this crate walks NaN through `evaluate` |
| 25 | The C++ created the relays **late** in startup, after Wi-Fi could already have blocked for ten seconds. | The relays floated across a strapping-pin sample; the machine's first state depended on pin levels. | `crates/fw/src/lib.rs:10-12`; `bsp-esp32s3/src/lib.rs:22-24` | **Yes** — `a_new_runtime_forces_the_actuators_off_before_anything_else` (`boot.rs:79`) |
| 26 | `displayBufferReady = false` was the C++'s whole mechanism for skipping a flush; a render with no framebuffer meant "don't send". | Layout regressions shipped because nothing could observe them. | `crates/display/src/framebuffer.rs:37-40`, `:163-170`; `clip_log`/`CLIP_LOG = 16` | **Yes** — template tests assert `fb.clipped() == 0` for every template in every state, and a partly-off-panel primitive is **recorded and not drawn at all** |
| 27 | The C++ sent the **whole** 1 KB framebuffer on every 100 ms render tick. | 128 bytes × 8 pages on a **400 kHz** I2C bus whether or not anything moved. D31. | `crates/display/src/framebuffer.rs:8-21` | **Yes** — `take_dirty()` diffs against the last flushed frame; "a re-render of an unchanged screen produces no dirty page at all" is asserted |
| 28 | A `/api/temperatures` failure returned **200 with an error body**. | A client cannot distinguish the error from data. D15. | `crates/app/src/api.rs:26`, `:835` | **Yes** — `temperatures_answers_500_when_there_is_no_reading` |
| 29 | Three OTA routes had **no authentication call at all**, and the URL route had no scheme/host allow-list, and the firmware route had no extension check. D01/D16. | Anyone on the LAN could start a firmware write; a pump could be left energised behind a progress screen. | `crates/app/src/api.rs:11-13`, `:21`; `http/src/route.rs:7-9` | **Yes** — `an_unauthenticated_mutation_is_401_when_auth_is_enabled`, `an_ota_while_brewing_is_409`, `a_protected_route_without_credentials_is_401`, `the_firmware_route_checks_the_extension_the_filesystem_route_already_did` |
| 30 | Auth as *middleware* rather than a *route property* means a handler can forget it. | D10: every mutating route was open. | `crates/app/src/api.rs:21`; `http/src/route.rs:20-23` | **Yes** — `authentication_is_a_property_of_the_route_not_of_the_handler`, and `an_index_past_the_end_is_treated_as_authenticated` (**fails closed**) |
| 31 | The 404 body echoed the requested path back. | Any endpoint was an XSS reflector: a path containing markup came back inside a JSON string a browser would render. | `crates/http/src/route.rs` (404 test) | **Yes** — `a_404_does_not_reflect_the_path_back` |
| 32 | `ESP8266WebServer`'s multipart upload buffered the whole part in a `String`. | A firmware upload could exhaust the heap. | `crates/http/src/parse.rs:10-13`; `ChunkedRequestUnsupported` at `:47` | **Yes** — chunked *requests* are refused by design; the OTA routes stream |
| 33 | Obsolete HTTP line folding, if *joined*, bypasses a header-count limit. | A hostile client gets more headers than the limit allows. | `crates/http/src/parse.rs:15-16`, `:126-130` | **Yes** — the fold check runs **before** the colon split, so the reason is not lost |
| 34 | A bridge that drops the oldest byte is wrong for a socket. | Refusing the write stalls the pump and eventually kills the connection. | `crates/app/src/net.rs:74-78` | **Yes** — `a_bridge_that_never_drains_does_not_grow_without_limit` (1000 × 512 B into a 4 KB ring) |
| 35 | A backflush `u8` cycle counter wrapping to 0 in a release build looks like "back to the first cycle" and loops forever; in debug it panics. | Infinite backflush. | `crates/domain/src/backflush.rs:46-53` | **Yes** — `resolve_cycle_advance` is a `const fn` with `saturating_add` and has its own tests |
| 36 | The `RING`/`MAX_BODY` bounds are not arbitrary: a 4 KB pipe, a 64 KB body cap and a 32 KB reply cap were each chosen against a specific largest body. | — | `app/src/net.rs:41-46`, `http/src/lib.rs:52-56`, `app/src/api.rs:33-38` | Partly — `a_config_upload_needs_json_and_reports_400_when_anything_is_rejected`, `an_oversized_body_is_refused` |

---

## 2. Problems discovered by DEVICE TESTING

**There are none. This is the honest answer and the code says so itself.** Evidence, all read:

- `crates/app/src/lib.rs:32` — *"Nothing in this crate has run on hardware. Every statement about behaviour is a statement about the host tests."*
- `crates/app/tests/scenarios.rs:8-9` — *"Every test here is `repo-verified` and nothing more. None of it has run on a board, because no board is connected."*
- `crates/fw/src/lib.rs:29`, `crates/bsp-esp32/src/lib.rs:5`, `bsp-esp32s3/src/lib.rs:5`, `bsp-esp32c6/src/lib.rs:5` — *"**Build-unverified in this checkout.**"*
- `docs/rust-migration/compatibility-matrix.md:11` — *"**No hardware was connected for this run.**"*

What follows is the hardware knowledge the code *encodes* without having observed it. Marked
**[inferred]** = derived from the datasheet / pinout, not measured here. **[compiled-not-run]** =
the code exists and links (C6 only) but has never executed. Nothing is marked *measured*, because
nothing was.

| # | Problem | What actually happens | Where (file:line) | Pinned by a test? |
|---|---|---|---|---|
| 1 | **[compiled-not-run]** GPIO2 and GPIO15 are **strapping pins**; their level at reset is a hardware property, sampled before any code runs. | A relay on GPIO2 is a boot-mode pin. The C++ map had the heater relay on GPIO2. | `bsp-esp32s3/src/lib.rs:22-24`, `bsp-esp32/src/lib.rs:22-24`; pin map change at `board-profiles/src/lib.rs:19-22` | **Yes** — `the_esp32_map_moved_off_the_two_pins_the_evidence_forbids` (`board-profiles/src/lib.rs:636`), via `const _: () = assert!(...)` |
| 2 | **[inferred]** GPIO1 is UART0 TX. | The C++ had the steam LED on GPIO1. Moved. | `board-profiles/src/lib.rs:19-22` | **Yes** — same test |
| 3 | **[inferred]** The four panel switches sit on **input-only** pins 34–39 with **no internal pull at all**. | The board must supply external resistors; a machine wired to the C++ map has them. S3/C6 have no input-only pins so they use internal pulls. | `board-profiles/src/lib.rs:44-48` (`needs_external_pull`), `Pin::pulled_input` at `:71-77` | **Yes** — the profile's constraint tests |
| 4 | **[inferred]** The C++ I2C pin map **does not exist on S3 or C6**: S3 has no GPIO22–25, C6 uses 8/9. | SDA 21 / SCL 22 simply does not compile on two of the three targets. | `spikes/stack-smoke/src/main.rs:39-55` (per-feature `cfg` arms) | Comment-only in the spike; the board profiles are tested |
| 5 | **[inferred]** The ESP32-C6-DevKitC-1 exposes 23 GPIOs, **14 usable** after removing the flash bus, USB and the RGB LED. | Seventeen signals do not fit. The 3 LEDs and the **second load cell** were dropped, by user decision of 2026-09-29. | `board-profiles/src/lib.rs:395-401`; the `None` entries at `:400-402` | **Yes** — `every_signal_name_is_unique` asserts `Signal::ALL.len() == 17` |
| 6 | **[inferred]** The ESP32 has **no native USB**; a USB-CDC provisioning build for it would compile and never receive a credential. | Silent provisioning failure. | `crates/fw/src/checks.rs:34-37` (`compile_error!`), `bsp-esp32/src/lib.rs:39-41` | **Yes** — a `compile_error!`; plus a `const { assert!(...) }` block at `board-profiles/src/lib.rs:606-616` |
| 7 | **[inferred]** A peripheral clock of **1 MHz** with no prescaler divides to exactly 100 Hz, which is the 10 ms PWM window the C++ used. | A 10 ms window that is really 9.8 ms is a heater whose PID is slightly wrong — a slow, invisible drift, not a crash. | `bsp-esp32c6/src/lib.rs:427-441` | No — the `Err` fallback (`with_prescaler(0)`) is comment-only and untested |
| 8 | **[inferred]** 1-Wire bit timing is **microsecond-scale** (reset 480 µs, recovery 70 µs, slot 60 µs, reply delay 20 µs — Paul Stoffregen's `OneWire` values, because the C++ used that library and the sensors are on the same wire). | Getting one wrong on hardware **looks like a disconnected sensor, not a timing bug**. | `crates/onewire/src/lib.rs:6-8`, `:95-115` | **Yes** — timings are named consts; a `Timings::instant()` variant lets the same code run on the host |
| 9 | **[inferred]** A 400 kHz I2C bus is the bottleneck for a 1 KB full-frame push at 10 Hz. | Display updates compete with the bus the pressure sensor and panel share. | `display/src/framebuffer.rs:9-13`, `:20-21` | Indirectly, via the dirty-page diff |
| 10 | **[inferred]** Flash **erase takes tens of milliseconds**; a read blocks for milliseconds. | The control tick is 1 ms and the safety tick must not slip, so nothing on the control path may touch flash. | `crates/fw/src/config.rs:18-22` | Structurally — the region is read once before the loop (`main_c6.rs:33-38`) |
| 11 | **[inferred]** A machine that boots on compiled defaults still brews, and **cannot be fixed over the network** if it refuses to boot. | A corrupt config region must not brick the machine. | `crates/fw/src/config.rs:63-67`, `app/src/store.rs:10-12` | **Yes** — `both_slots_corrupt_falls_back_to_defaults_rather_than_failing_the_boot`, `an_erased_region_falls_back_to_defaults_rather_than_refusing_to_boot` |
| 12 | **[inferred]** `just build` produces an image **without a partition table**; `just flash` writes one. | A machine flashed that way has no config region at all. | `crates/fw/src/config.rs:12-17`, `ReadFailure::NoPartitionTable` at `:36` | No — the fw crate is uncompiled; `ReadFailure` has no host test |
| 13 | **[inferred]** The config partition must be **≥ 64 KB** (two 32 KB slots). | A table that says otherwise is one this firmware did not write; reading past its end is a buffer overrun. | `crates/fw/src/config.rs:25-27`, `TooSmall` at `:44-47` | Partly — `a_region_holding_a_real_document_is_read_and_applied` covers the app half only |
| 14 | **[inferred]** 16 KB on the stack of a 320 KB machine is a stack overflow waiting for the deepest call chain. | The config buffer must live in `.bss`, not on a task's stack. | `crates/app/src/prov.rs:198-199`; same reasoning at `http/src/response.rs:126`, `sse.rs:89`, `store.rs:89` | **Yes** — `a_response_is_small_enough_to_live_on_a_stack_frame` (`response.rs:422`, asserts `size_of::<Response>() <= 1024`) |
| 15 | **[inferred]** A device that prints a firmware banner on boot makes the first `read_line` return the banner, and **every command appears to fail**. | A provisioning tool that doesn't drain the port first diagnoses a working device as a broken one. | `tools/provision/src/link.rs:20-23` (`Link::drain`) | **Yes** — `a_closed_port_ends_the_session_rather_than_looping`, and the device calls `transport.drain()` before printing its prompt (`prov.rs:464`) |
| 16 | **[inferred]** A device mid-flash or mid-reboot is **silent for seconds**. | A tight serial timeout produces a confusing failure. | `tools/provision/src/lib.rs:36-39` (`TIMEOUT = 10 s`, chosen for the original ESP32's slow boot to prompt) | No — timing constant, comment-only |

---

## 3. Measured numbers

Every row is traceable. "Host-derived" means a value that came out of a host test's *assertion*;
"inherited" means quoted from the C++ or a datasheet and reproduced deliberately. **There are no
device measurements in this tree** — see section 2.

| Metric | Value | How measured | Where |
|---|---|---|---|
| C++ PID first-sample derivative transient | ≈ **−14000** | Computed by hand from the Arduino library's zero-seeded filter, with the shipped gains | `crates/domain/src/pid.rs:300-303` |
| Compiled-default gains | kp 62.0, tn 52.0, tv 11.5 | C++ `defaults.h`, reproduced | `crates/app/src/machine.rs:141-145` |
| Compiled-default EMA factor / integrator max | 0.6 / 55.0 | C++ hardcoded; the C++ ignored `pid.regular.i_max` entirely | `machine.rs:146-147` |
| Setpoint / steam setpoint (compiled) | 93.0 °C / 135.0 °C | C++ `defaults.h` (comment says 93.0) | `machine.rs:111-112` |
| Schema default brew setpoint | **95.0 °C** | Note this differs from `RuntimeConfig::default`'s 93.0 — the schema is the authority for a document read | `machine.rs:111` vs asserted in `config_rt.rs:505-510` |
| Schema default brew target | 25 000 ms | Test assertion | `config_rt.rs:511-514` |
| Temperature filter window | **15** samples | C++ 15-sample moving average, kept because the lag feeds a 10 ms PWM | `crates/domain/src/sensor.rs:48-52`; `machine.rs:151` |
| Filter lag on a 40 °C over-read | **six seconds** | Derived: 40 °C ÷ 15 samples × 400 ms period. This is why the emergency evaluator is fed the **raw** reading | `crates/app/src/machine.rs:622-626` |
| DS18B20 conversion times | 94 / 188 / 375 / **750** ms at 9/10/11/12 bits | Datasheet, asserted | `crates/ds18b20/src/lib.rs:26-32`, test at `:404-417` |
| Default DS18B20 resolution | **11 bits** (375 ms) | Chosen because 375 < the 400 ms sensor period; the C++ did the same | `ds18b20/src/lib.rs:503-511` |
| DS18B20 power-on placeholder | raw **1360** sixteenths = **85 °C** | Datasheet power-on register value | `ds18b20/src/lib.rs:270-273` |
| Valid DS18B20 range | 0–**125 °C** (C++ accepted 0–200) | Part's own range vs the C++'s | `ds18b20/src/lib.rs:100-105`; domain accepts 0–200 at `domain/src/sensor.rs:42-46` |
| TSIC change rates | wide **200 °C**, runtime **5 °C** | C++ `TempSensorTSIC.cpp:11-12`; 5 °C per 400 ms = 12.5 °C/s, faster than a boiler's group head | `crates/drivers-tsic/src/lib.rs:18-27` |
| TSIC sentinels | 222.0 (read failed), 221.0 (not connected) | C++ `TempSensorTSIC.cpp:46,51` | `drivers-tsic/src/lib.rs:29-34` |
| TSIC plausible range | 0–**180 °C** | C++'s accepted range | `drivers-tsic/src/lib.rs:36-40` |
| ABP2 frame length | **12** bytes (C++ read 7) | Two 6-byte words | `crates/drivers-pressure/src/lib.rs:31-38` |
| ABP2 settle | **10 ms** (was a `delay()` on the main loop) | C++ `ABP2_READ_DELAY_MS` | `drivers-pressure/src/lib.rs:25-28` |
| ABP2 counts span | `OUTPUT_MIN` 1 677 722 → `OUTPUT_MAX` 15 099 494, of 16 777 215 full scale, 0–10 bar | C++ `ABP2_*` | `drivers-pressure/src/lib.rs:44-58` |
| HX711 startup timeout | **5000 ms** (C++ waited forever) | Chosen: wider than the part's settling time | `crates/drivers-scale/src/lib.rs:26-28` |
| HX711 averaging | library default **32**, schema range 1–20, so clamped to **20** | The clamp is deliberate: a 32-sample window is outside the schema's own declared range | `drivers-scale/src/lib.rs:34-42`; test at `:510-519` |
| HX711 implausible weight | −500 … **5000 g** | 24-bit part; more bits than it has is a wiring fault | `drivers-scale/src/lib.rs:353-359` |
| Heater PWM window / frequency / duty range | 10 ms / **100 Hz** / 0–1000 permille | C++ `Timing.h:16` + `ProcessState.h:183` | `crates/domain/src/timing.rs:18-21`; `app/src/heater.rs:25-31` |
| PWM timestamp mapping | duty 1000 → **999**; duty 1 → **1** (not 0) | `(permille × 999 / 1000)` with a ceiling | `app/src/heater.rs:41-51`; tests `:135-158` |
| Emergency thresholds (compiled) | trip **150 °C**, clear **100 °C**, 3 consecutive, hysteresis 5 | The values the C++ actually ran with, because `safety.emergency_temp` was never loaded (D12) | `crates/domain/src/emergency.rs:33-42` |
| Clear point derivation | `trip − hysteresis × 10`, floored at 0 → 140−5×10 = **90 °C** | Deliberately far enough below trip that noise cannot chatter | `app/src/config_rt.rs:243-247`; test at `:554-566` |
| Pump deadlines | brew **300 000 ms**, hot water **60 000 ms** | C++ constants that were never armed (D09) | `domain/src/timing.rs:74-77` |
| Derived brew pump deadline | `max(2 × target, 300 000)` — 20 s target → 40 000; 300 s → **600 000** | A 20 s target with a 5 s deadline would stop every shot at 5 s | `app/src/config_rt.rs:259-262`; test at `:583-596` |
| Sensor periods | temperature **400**, pressure **50**, scale **100**, water tank **200** ms | C++ `Timing.h:42-45`; the water-tank period is a **new decision** (C++ polled it from the temperature tick) | `domain/src/timing.rs:27-32`; `fw/src/lib.rs:46-49` |
| Switch sampling | **every 1 ms control tick**, not on a 20 ms timer | A debounced switch read only every 20 ms can miss a press shorter than 20 ms — "a brew the user asked for and did not get" | `app/src/sensors.rs:92-101` |
| PID sample | **1000 ms** | C++ `SystemInitializer.cpp:551` | `domain/src/timing.rs:24` |
| Watchdogs | `SAFETY_FEED` **50 ms** into `SAFETY_TIMEOUT` **5000 ms** (chip task watchdog also 5000) | Feed×10 < timeout is *asserted* so a slow safety task doesn't trip and a hung one does | `domain/src/timing.rs:79-85`; test at `:113-118` |
| Debounce / long press | **20 ms** / **500 ms**; C++ `IOSwitch.h:63-64` | Bounds asserted: ≥5 ms or contacts bounce visibly; ≤50 ms or it feels laggy; long press must outlast debounce | `domain/src/timing.rs:63-66`; test at `:120-135` |
| Power boot guard / reboot long press | **5000 ms** / **1000 ms** | C++ `PowerHandler.h:118,143,144` | `domain/src/timing.rs:68-72` |
| Display auto-sleep | **2 100 000 ms** (35 min) | C++ `AUTO_SLEEP_MINUTES = 35`. Note the trap recorded: the C++ `StandbyCoordinator` used **600 000 ms** for a *different* timer, and reproducing it would have reproduced the wrong constant | `domain/src/timing.rs:46-50` |
| Panel geometry | 128 × 64, 1 KB frame, 128 B per page, 8 pages | SSD1306 page-addressed controller | `display/src/framebuffer.rs:27-35` |
| Framebuffer RAM cost of the dirty fix | **2 KB** (live + last-flushed) to stop pushing **1 KB** down a 400 kHz bus ten times a second | The stated trade | `display/src/framebuffer.rs:17-21` |
| HTTP bounds | request line **256**, header line **256**, headers **32**, body **64 KB** | A real frontend request is under 64 bytes; a browser sends ~12 headers | `crates/http/src/lib.rs:44-56` |
| Reply / body bounds | `MAX_REPLY` **32 KB** (biggest is `/api/parameters` with 99 tagged entries, ~24 KB), `MAX_REQUEST_BODY` **16 KB** | Sizes chosen against the actual largest bodies | `app/src/api.rs:33-38` |
| Connection bridge | 4 KB per direction; **4** concurrent connections = 32 KB of rings; 512 B socket read; **120 s** idle timeout | "A phone, a laptop and a tablet is three" | `app/src/net.rs:41-46`, `:276-292` |
| SSE queue | **12** events = **1.2 KB** per streaming connection on a C6 | Only moved into the connection stack frame when a client actually opens a stream | `http/src/sse.rs:86-91` |
| Log ring | **4096 B** static, `MAX_LINE` **160**, server chunk **256** | Same 4 KB budget the C++ used, taken statically instead of from a contended heap (D38) | `app/src/log.rs:16-27`; budget test at `:447-452` |
| Config region | two **32 KB** slots, 32 B header, 64 KB partition minimum, magic `"CCFG"`, format v1 | Generation is `u16` and compared **across wrap** | `crates/storage/src/lib.rs:20-35`; wrap test at `:437-445` |
| Provisioning | line **1024 B**, config **16 KB** (4× the ~4 KB C++ export), chunk **512** B payload → ~700-char line, tool timeout **10 s** | Every number is 4× or headroom-over the actual largest thing sent | `app/src/prov.rs:33-43`; `tools/provision/src/chunk.rs:1-13`; `tools/provision/src/lib.rs:36-39` |
| Import findings cap | **48** | "A file with 96 bad fields is a file from a different firmware, and the first 48 named fields are enough to diagnose it" | `crates/config/src/import.rs:20-22` |
| JSON parser caps | depth **8**, scalar **128** B | A config document is at most 3 deep; unbounded recursion on a bare-metal target is a panic with no handler | `crates/config/src/json.rs:19-25` |
| Schema size | **99** parameters, 11 groups, 10 emitted by the C++ | Adds the two unregistered safety parameters + `hardware.board` to the C++'s 96 | `config/src/schema.rs:200-353`, test at `:696-703` |
| CRC-32 | IEEE, **no table** (a table is 1 KB of flash) | Config writes are rare; speed is irrelevant, flash on a C6 is not | `crates/storage/src/crc32.rs:1-5` |
| Spikes' heap | **96 KB** (stack-smoke), **32 KB** (usb-smoke) | `esp_alloc::heap_allocator!` sizes chosen per spike | `spikes/stack-smoke/src/main.rs:32`, `spikes/usb-smoke/src/main.rs:27` |
| The 320 KB part | "on a C6 with 320 KB of RAM" / "on a 320 KB machine" | The budget every buffer bound above is argued against | `http/src/lib.rs:10-11`, `app/src/prov.rs:199` |
| Test count | **668** `#[test]` fns; 31 in `api_routes`, 21 in `scenarios`, 30 in `transition` | `grep -c` | measured over `crates/` + `tools/` |

---

## 4. Decisions reversed or corrected during implementation

These are the things the code does that a first reading of the design documents would not predict.

- **Service mode is not "the PID is off".** The first attempt expressed provisioning/OTA as
  `pid_runtime_enabled = false`; the cross-cutting PID check then ejected the machine from
  whatever state it was in. It is now enforced where the command is issued and in `run_pid`.
  `app/src/machine.rs:748-753`. **This is a reversal recorded in the code itself.**
- **`Control` then `safety`, in that order, and it is load-bearing.** Running safety first would
  check the *previous* tick's command — "a check that passes one tick after the thing it meant to
  catch". `app/src/boot.rs:15-18`.
- **Requests are edges, consumed by one tick, not flags.** A stale flag left set by one path could
  end a phase hours later (D22). `machine.rs:13-16`, `transition.rs:42-44`.
- **The fault-recovery timer is measured from the fault *clearing*, not from entry.** The C++
  reset it while the fault persisted, so a flapping sensor hopped straight back; here the timer is
  forced to 0 while the fault is present. `transition.rs:57-60`, `machine.rs:732-739`.
- **Every request is mapped into one of 9 flag slots via a `heapless::Vec<Request, 4>`** and the
  queue is cleared at the end of every tick. A 5th request **force-offs the machine** rather than
  growing a queue nobody drains. `machine.rs:547-553`, `:595`.
- **A reboot is a request, not an action.** `Machine` sets a flag; the firmware layer that owns the
  reset peripheral is the only thing that acts on it. `machine.rs:236-243`.
- **The emergency evaluator is fed the raw reading, not the filter's output** — the filter would
  take six seconds to move 40 °C. `machine.rs:621-626`.
- **But a sensor fault is NOT routed through the emergency stop** — it is the `SENSOR_ERROR` path
  with a recovery delay, because "routing them through the emergency stop instead would make a
  recoverable fault need a power cycle". `machine.rs:630-636`. Two different answers to two
  different untrustworthy inputs, deliberately.
- **The backflush-mode request doubles as the mode flag for the tick it arrives on.** Otherwise
  entering `BackflushIdle` would need the mode already active and the run could never start.
  `machine.rs:765-768`.
- **`brew.mode` manual is the **zero** enum**, automatic is 1 — reversed from the intuitive
  ordering. `config_rt.rs:133`, test `a_manual_brew_mode_is_the_zero_enum_and_an_automatic_one_is_not`.
- **`hardware.switches.brew.present` is just the `enabled` row.** The C++ had one
  `hardwareSwitchesBrewEnabled` and invented a global one on top of it. `config_rt.rs:226-229`.
- **The pump deadline is derived, not configured**: `max(2 × target, 300 s)`. A 20 s target with a
  5 s deadline would stop every shot at 5 s. `config_rt.rs:259-262`.
- **Hardware PWM replaced the 10 ms software-PWM ISR entirely** — "defect D05's shape *removed*
  rather than documented". The machine now has **no interrupt at all** for the heater.
  `bsp-esp32c6/src/lib.rs:401-404`.
- **The heater pin is never a plain `Output`**, and `command()` explicitly does not set it —
  writing the pin level from the relay path would take it away from the peripheral and put it at
  full power. `bsp-esp32c6/src/lib.rs:186-196`.
- **The response header bound is half the request bound** (128 vs 256) — sizing them equally made
  a `Response` **5.8 KB** and it lives on the stack for a whole exchange. `http/src/response.rs:118-128`.
- **A partly-overlapping primitive is recorded as clipped and NOT drawn at all**, rather than
  clipped: a shape that hangs off the edge is a layout bug even when part of it is visible.
  `display/src/framebuffer.rs:162-170`.
- **The log ring drops the *oldest*; the SSE queue drops the *newest*.** Opposite policies, each
  argued: a log that stops recording the present is worse than one that forgot the past; an old
  event delivered late is worse than none, because the frontend would render the machine going
  backwards. `app/src/log.rs:9-10` vs `http/src/sse.rs:119-124`.
- **A config document with any rejected or unknown field applies NOTHING** — not even the fields
  that passed. `app/src/store.rs:110-116`.
- **`auth_enabled == false` means every route is open**, deliberately, and a machine that never had
  a password set must not be locked out of its own UI. `app/src/api.rs:499-504`, `:545-549`.
- **`/api/ota/url` answers 202, not 200**, because "the frontend checks for 202 and would treat a
  200 as a completed update. A contract is a compatibility requirement, and a 'cleaner' status
  code is a broken frontend." `app/src/api.rs:14-16`, `:912`.
- **An index past the end of the route table is treated as authenticated** — fails closed.
  `http/src/route.rs:282`.
- **The 1-Wire simulated bus is a cargo feature, not `cfg(test)`**, so another crate's test can
  drive the *real* driver against it: "a driver test is only worth having if it exercises the real
  bus code". `crates/onewire/src/lib.rs:32-35`.
- **The `no_handler_awaits_or_blocks` test greps the source text of `api.rs` for `.await`.**
  A meta-test that reads the code rather than a behavioural assertion. `app/tests/api_routes.rs:764-778`.

---

## 5. Open questions and unverified claims left in the code

These are the gold. Each is a place the code itself admits it does not know.

1. **Nothing has run on hardware.** Stated in `app/src/lib.rs:32`, `app/tests/scenarios.rs:8-9`,
   and in the module header of all four chip-facing crates. The host test suite "is the only
   behavioural evidence in this checkout" (`docs/rust-migration/compatibility-matrix.md:25`).
2. **The whole Xtensa branch is build-unverified because the host is aarch64 and `espup` is
   x86-64.** `just check-fw esp32` / `esp32s3` "cannot be run here" and the ESP32/S3 board crates
   are `unverified` rather than `build-verified`. `crates/fw/src/lib.rs:29-31`,
   `bsp-esp32/src/lib.rs:5-8`, `bsp-esp32s3/src/lib.rs:5-8`.
3. **Multi-drop 1-Wire enumeration is not verified.** "doing that in simulation needs every device
   driving the line simultaneously, and the simulation written here does not yet model that
   correctly… it is recorded rather than papered over by a test that asserts something weaker than
   it appears to." `crates/onewire/src/lib.rs:19-25`, and again at `:517-521` ("the simulation is
   **not yet trusted**; what is asserted here is that the loop terminates"). The test
   `a_search_that_never_ends_is_bounded` has a tautological assertion
   (`assert!(found.is_ok() || found.is_err())`) — honest, and easy to mistake for coverage.
4. **U8G2 font metrics cannot be read from the C++ tree** — they live in the PlatformIO-fetched
   library. "the task list records that as **the single largest unknown in the display port**."
   The port substitutes one 5×7 table with Small (1:1) and Large (2:1) scales, which means
   **no proportional font is expressible** and the text is not pixel-identical to the C++.
   `crates/display/src/font.rs:3-15`.
5. **No SSD1306/SH1106 bus driver is written at all.** `compatibility-matrix.md:59` — the
   framebuffer is host-tested, the panel is not driven. SH1106 support and partial updates are
   "unverified".
6. **The MQTT client is not chosen and not written.** "A `no_std` MQTT client is a dependency
   decision the task list records as open." `crates/app/src/mqtt.rs:13-15`. No broker has ever
   seen a document; they are only compared against golden JSON on the host.
7. **The Wi-Fi association and `embassy-net` accept loop are deliberately not written.** "a
   hand-written association nobody has run is worse than a missing one." `crates/fw/src/net.rs:7-13`.
8. **The TSIC pulse-train decoder is not in the repository.** "The C++ firmware vendored `ZACwire`
   and its native tests replaced the whole library with a stub that returned queued floats." Only
   the change-rate latch, the sentinels and the range rejection were ported; the timing is
   delegated to an unimplemented `PulseSource` trait. `crates/drivers-tsic/src/lib.rs:8-12`.
9. **The ABP2's 12-byte frame layout is asserted in a comment, not tested.** The fix for D54 is
   the most consequential driver change in the port and `DATA_LEN = 12` has no test proving which
   bytes are which. `crates/drivers-pressure/src/lib.rs:31-38`.
10. **The MCPWM peripheral-clock fallback is silent.** A clock that cannot be set to 1 MHz falls
    back to `with_prescaler(0)` and the resulting period is *not* verified to be 100 Hz — the
    comment concedes "a 10 ms window that is really 9.8 ms is a heater whose PID is slightly
    wrong". `bsp-esp32c6/src/lib.rs:432-441`.
11. **`pump_permitted`'s doc comment claims a check the code does not perform.** It says the pump
    is refused "without a pressure reading when a pressure sensor is fitted and the reading is
    outside the plausible band the C++ checked" — but the function only checks the sensor fault,
    the emergency latch and the tank. The pressure clause is **documentation of an unimplemented
    interlock**. `app/src/machine.rs:954-965`. This is the one place where the code's prose and the
    code itself disagree, and a reader trusting the comment would be wrong.
12. **`advance_brew_timers` contains a no-op branch.** `machine.rs:1058-1063`: the `if let Some(t)`
    block does `let _ = t;` when the fault is clear and sets `None` otherwise — it never uses the
    value. Dead or unfinished logic sitting in the one place that advances the fault-recovery timer.
13. **The provisioning baud rate is a build-time constant, not negotiated.** "a device that wanted a
    different rate could not be provisioned at all." `bsp-esp32c6/src/lib.rs:39-41`.
14. **`hardware.relays.*.trigger_type` is inert and the port knows it.** The relays are hardwired
    active-low "because that is what the machines in the field have", and the difference is
    recorded rather than pretended live. `app/src/config_rt.rs:73-79`.
15. **~80 of 99 schema parameters are listed as deliberately not read by the machine**, including
    `pid.bd.*` (brew detection), `pid.steam.*`, the maintenance reminder, the calibration wizard
    and the BLE scale. `app/src/config_rt.rs:38-118`. A user who sets them is told they are inert.
16. **The 1-Wire `Timings::instant()` used by host tests is not a valid bus timing** — "a real bus
    with these timings would not communicate with anything". `crates/onewire/src/lib.rs:113-122`.
17. **A copy-paste error in the C6 crate's own status header**: it says `just check-fw esp32s3` in
    the ESP32-C6 file. `bsp-esp32c6/src/lib.rs:6`. Trivial, but it is the kind of drift that says
    the headers are not always read carefully.
18. **A broken doc link** in `config/src/schema.rs:78` — "Every group, including , which the C++…"
    (an empty intra-doc link where `Safety` should be). `cargo doc` would warn.
19. **No defect register in the code for D0, D04, D06-D07, D17, D19-D21, D24, D30, D32, D34-D36,
    D40-D41, D44-D48, D50-D53, D56(partial)** — the `Dxx` ids referenced from the code are a sparse
    subset. Whether the rest are not applicable or simply not yet cited is not answerable from
    this tree.

---

## 6. Load-bearing invariants encoded in code

| Invariant | Enforced by | Where |
|---|---|---|
| The actuators are de-energised **before anything else is configured at boot**. | `Machine::new` calls `force_off()` as its last act; asserted by counting recorder events | `app/src/machine.rs:384-387`; test `app/src/boot.rs:78-96` |
| `control_step` runs **before** `safety_step`, every tick. | `Runtime::tick` ordering; asserted by `watchdog_ms() == 0` after a tick | `app/src/boot.rs:52-58`, test `:98-107` |
| Only 8 states may hold the water valve open. | `const fn` allow-list + a test that counts the states whose *own* actuator table opens the valve and asserts it equals 8 | `domain/src/state.rs:118-135`, test `:250-273` |
| A second copy of the valve-interlock list must not exist. | `flows_water`/`energises_heater` read the domain's own table instead of a second list | `app/src/machine.rs:1096-1107` |
| The heater is *permissioned* and the duty is a separate integer; only `Actuators::command` changes actuator state, and `force_off` is callable from anywhere including the panic path. | `hal-traits::Actuators` doc contract; the *only* `Relays` in each bsp crate holds the pins | `hal-traits/src/lib.rs:5-12`, `:66-86`; `bsp-esp32c6/src/lib.rs:141-147` |
| `set_heater_duty` is clamped, so a bad temperature cannot command full power with `65535`. | `.min(1000)` in the recorder; test | `hal-traits/src/lib.rs:344-349`, test `:396-400` |
| Every state that energises an actuator de-energises it **on exit**. | `Machine::exit_state` + `every_state_that_starts_water_stops_it_on_exit` | `app/src/machine.rs:906-919`; `app/tests/scenarios.rs:590+` |
| The emergency stop leaves everything off **from every state**. | A test walks all 12 live states, drives each, then trips | `app/tests/scenarios.rs:302-373` |
| A trip latches until a power cycle. | `emergency_latched: bool`; test `the_emergency_stop_is_latched_until_a_power_cycle` | `app/src/machine.rs:336`; `scenarios.rs:376-393` |
| A CRC failure is an error, never a value. | `TemperatureError::Corrupt` in the return type; a test flips every one of 8 bytes | `crates/onewire/src/crc.rs:7`; `ds18b20/src/lib.rs:357-378` |
| A value can never outlive the buffer it was decoded from. | `Value<'a>` is lifetime-parameterised throughout import/export/store | `config/src/import.rs:100-110`; `config/src/schema.rs:98-113` |
| A secret never appears in an export, a status response or a log. | `Param::secret` flag + `the_four_secrets_are_marked_and_nothing_else_is` + `a_secret_is_not_written_into_an_export` | `config/src/schema.rs:131-132`; `app/src/store.rs:308-327` |
| A credential cannot reach stdout: no `Status` variant can hold one. | Type design; `no_reply_the_device_can_produce_contains_the_password` | `tools/provision/src/lib.rs:44-48`; `app/tests/provisioning.rs:256-296` |
| Every schema key is either read by the machine or **named** as inert. | `NOT_READ_BY_THE_MACHINE` + `every_schema_key_is_either_read_or_explicitly_accounted_for` | `app/src/config_rt.rs:38-118`, test `:618-634` |
| A pin number outside the chip's range **does not compile**. | A `macro_rules!` over literal GPIO numbers, not a `match` (which would make every arm a move out of the same struct) | `bsp-esp32s3/src/lib.rs:56-115`; rationale at `:48-54` |
| The HAL pin for each signal matches the profile pin. | `const _: () = assert!(...)` — a compile-time check, not a test | `bsp-esp32c6/src/lib.rs:107-140` |
| Only the ESP32 provisions over UART and only it lacks USB. | A `const { assert!(...) }` block — checked at compile time | `board-profiles/src/lib.rs:608-617` |
| Exactly one board feature and one provisioning transport reached the binary. | `compile_error!` on every illegal combination | `crates/fw/src/checks.rs:10-37` |
| The PWM window is a whole number of ISR ticks (else the duty is quantised). | `assert_eq!(window_ms % isr_ms, 0)` | `domain/src/timing.rs:96-110` |
| The PWM window divides into 1000 permille exactly, so the display conversion is a plain divide by ten. | `assert_eq!(HEATER_PWM_WINDOW / 10, 100)` | `domain/src/timing.rs:155-160` |
| A `u8` backflush cycle counter cannot wrap into an infinite loop. | `saturating_add` in a `const fn`, plus `resolve_cycle_advance` tests | `domain/src/backflush.rs:44-53` |
| A generation counter comparison survives its own `u16` wrap. | `generation_comparison_survives_a_wrap` — writes at `0xFFFF` then `0x0000` | `storage/src/lib.rs:437-445` |
| A downgraded firmware never destroys a newer machine's configuration. | `UnsupportedVersion` is reported, not written over; tested both in storage and in the importer | `storage/src/lib.rs:48-50`, `:449-461`; `config/src/import.rs:958-968` |
| A no-await rule for API handlers, enforced against the source text. | `include_str!("../src/api.rs")` + a `.await` scan, skipping comment lines | `app/tests/api_routes.rs:764-778` |
| Every buffer has a compile-time bound. | The `MAX_*` consts in `http`, and the reason given: on a C6 with 320 KB an unbounded allocation driven by a LAN request is a DoS the user can cause | `http/src/lib.rs:8-12`, `:44-56` |
| Nothing outside `app` may depend "sideways"; a cycle fails CI. | `tools/check-deps.py`, wired into `just deps` | `justfile:44-46`, `app/src/lib.rs:26-28` |
| A dropped recorder event cannot make a test pass by accident. | `RecordingActuators::dropped` is public and `sequence()`'s doc says "a test must not use this" | `hal-traits/src/lib.rs:283-288`, `:350-359` |
| A clipped draw is a failing test, not an invisible bug. | `Framebuffer::clipped` counter + a 16-entry clip log; every template/state asserts zero | `display/src/framebuffer.rs:36-40`, `:75-77` |
