# Findings

What is known about the C++ firmware and the original ESP32, tiered by how each fact was earned.
Grades are defined in [README.md](README.md). Numbers backing these claims are in
[measurements.md](measurements.md); sources are in [digests/](digests/).

Read the grade, not the confidence of the prose. A `single-source` fact written assertively is
still a `single-source` fact.

---

## 1. The C++ has blocking safety defects

Porting fixes these; it does not rediscover them. The shape of each is contagious — the port
reproduced one of them in new code before catching it
(`findings.md` §3.4).

**device-verified · triangulated — `PumpTimer::start()` is never called.** `isExpired()` returns
false unless `isRunning_`, which only `start()` sets. The advertised **5-minute brew limit and
60-second hot-water limit can never fire**; hold the water switch and the pump runs indefinitely on a
machine with a 2 kW boiler. Manual flush and the backflush phases are uncovered even if armed. Three
independent traces of the same six references.

**triangulated — `heaterEnabled_` is never set.** `enableHeater()` has zero call sites, so
`disableHeater()`, `safeShutdown()`, `disableAllHardware()` and the partial-init cleanup all
early-return. **Every heater shutdown path is inert**; the heater is driven only by the ISR. This is
also why an OTA leaves the pump and valve energized — the OTA's own `disableHeater()` is a no-op.

**triangulated — the steam valve has no safety whitelist.** `openSteamValve()` checks only
`emergencyMode_`; no `steamSafetyShutdownCheck` exists anywhere. Steam and water **share one
physical relay** (`ValveState.h:8-11`, GPIO17), so an ungated steam open is an ungated water valve.
Closed in the C++ only by the accident that `openSteamValve` has no call site.

**triangulated — `safety.emergency_temp` / `emergency_hysteresis` are consumed but never
registered.** Never persisted, exported, imported, or returned by `/api/parameters`. The S1 threshold
silently reverts to its compiled default on every reboot. `design` confirmed absence against a real
`config.json` export (96 leaves); `rewrite` measured `loadAll` reporting "41/96 loaded" on a healthy
machine for the same reason.

**triangulated — three conflicting emergency thresholds coexist.** A `GlobalTypes` constant of 145, a
`Temperature.h` pair of 145 and 120 (read nowhere), and the live default of 150 with a 100 clear.
`testEmergencyStop()` is dead code. Any port that "corrects" this changes behaviour.

**single-source — the water valve is not gated on an empty tank**; only `enablePump()` is.

**single-source — `disableAllHardware()` skips relays whose tracked flag says "off"**, and the
heater's flag is unreliable while the ISR runs. Write all four safe outputs unconditionally.

**triangulated — backflush states never re-assert hardware in `update()`.** `Filling` opens
pump+valve in `onEntry` and nothing re-asserts; `Flushing` fails inverted. The C++ also contradicts
itself about the phase's purpose: the valve is closed while the log says "flushing into drip tray"
and the interlock whitelist permits it open. This is the ADR-0003 class the project already knows.

**single-source — `enterSafeMode` / `exitSafeMode` / `disableWaterOperations` are log-only stubs**,
and `SensorErrorState::onEntryImpl` calls `enterSafeMode()` believing it disables hardware.

### Actuator bookkeeping to reproduce deliberately

Load-bearing C++ behaviour, each pinned by a test so a later reader does not "fix" it:

- **The self-transition skip in `executeTransition` is load-bearing** — it is what makes the
  `SENSOR_ERROR` clock behave as it does.
- **`SENSOR_ERROR`'s recovery clock runs from *entry*, not from the error clearing.** The guard has
  no exclusion list, so `checkSpecificTransitions()` is never reached. The source comment claims the
  opposite and one inventory initially believed the comment.
- **`hasUserActivity()` and `shouldExitStandby()` are hard `return false` stubs** — the water switch
  cannot wake standby.
- **State-id numeric gaps are load-bearing**: `isBrewState` 31..34, `isBackflushState` 60..63,
  `state <= 63` for LED eligibility.
- **The tank reads empty for the first ~220 ms of a cold boot with a full tank.** `SensorCoordinator`
  seeds "assume full" while `IOSwitch` seeds `LOW`, plus a 200 ms rate limit and the debounce.
  The contradiction is intentional.
- **Exactly `0.0 °C` counts as a valid temperature** in the emergency check (strict comparisons).

---

## 2. The original ESP32 traps

### 2.1 Two that panic the chip

**device-verified · triangulated — a floating-point instruction in a level-1 ISR panics this chip.**
Xtensa never saves coprocessor state across an interrupt; the FPU is coprocessor 0 and the handler
dereferences a null save area, writing `EXCCAUSE 0x4` → `PANIC_RSN_COPROCEXCEPTION`. The escape
hatch is `CONFIG_FREERTOS_FPU_IN_ISR`, which defaults **off**. The C++ is safe only by accident: GCC
soft-floats the same source the `esp` target compiles to hardware FP.

No compile-time warning exists. Check emitted code for FP instructions; a naive `.s` grep
false-positives on `divn.s`, `un.s`, `moveqz.s`, and `objdump -d` loses instruction sync on this image
and reports "zero FP" for functions it never decoded. Keep duty arithmetic as integers and wrap it
in a type that cannot hold a float.

`rewrite` shipped this bug and panicked on the first ISR fire.

**device-verified · triangulated — LEDC cannot produce a contactor-friendly carrier.**
`ledc_ll_set_duty_start` spins inside `portENTER_CRITICAL` waiting for the last duty change —
original ESP32 only. At 1 Hz that masks interrupts for ~1 s against a 300 ms watchdog, and it panics
at **duty 0** because the wait precedes the duty value. The "LEDC costs zero CPU" argument does not
survive contact with this chip. See [open-decisions.md](open-decisions.md) §3.

### 2.2 The heater switches ~2×/s

**device-verified · triangulated.** The 10 ms ISR predicate `pidOutput > counter` is monotone, so
the relay **level** changes twice per 1000 ms window — and not at all at duty 0 or full duty. An
earlier revision of `rewrite`'s own docs specified a 100 Hz carrier "because it reproduces the 10 ms
step / 1 Hz chopping exactly"; that was wrong by 100× and would have made 200 contactor changes/s on
a 2 kW contactor.

Contactor minimum on-off time, and the realised frequency and duty on the pin, remain **unmeasured**.
The code fixes a narrowest pulse of 10 ms, one whole C++ quantisation step, and leaves the rest to a
scope with the boiler disconnected.

### 2.3 Timing and bus behaviour

**triangulated — the pressure read blocks 10 ms of every 50 ms**, ~20 % of the control loop asleep,
and both the class comment and ADR-0002 describe the path as non-blocking. It also checks nothing:
the status byte is computed and never read, and `requestFrom`'s return is discarded, so a short read
converts the **previous** sample as fresh. Percentage and bar divide by different scalars, so full
scale reads exactly 90 %.

**single-source — the ABP2 frame is 12 bytes; the C++ reads 7**, building its temperature count from
bytes 4–6, one of which is the *second word's status byte*. It checks neither status byte, and
converts out-of-span counts into negative pressures the over-pressure logic accepts.

**triangulated — I²C runs at the Arduino default 100 kHz** (`Wire.setClock()` is never called), and
the display is in U8g2 **full-page mode**, so every 10 Hz render pushes 1024 bytes and blocks ~90 ms
— past the project's own 100 ms slow-loop threshold. Share the bus behind a mutex and take it for one
transaction; a panel holding it starves the sensor and vice versa.

**device-verified — `FreeRtos::delay_ms(n)` does not sleep `n` ms.** `CONFIG_FREERTOS_HZ` is 100, so a
delay is `ceil(n*100/1000)` ticks of a 10 ms grid. The first device run of a clock suite measured
**12 ms for a 20 ms request** and failed on it. Size timing assertions against the grid, not against
the request.

**device-verified — the millisecond clock wraps every 49.7 days**, and the heater runs across that
boundary. Elapsed-time subtraction must be wrapping; a borrowing subtraction panics in debug.

### 2.4 Sensors

**device-verified · triangulated — a DS18B20 is fitted, not a TSIC-306.** Family `0x28`, ROM
`2869...af41`, measured on the board. `Config.h:1087` defaults to `TSIC_306` and nothing detects the
mismatch. A spike written to validate ZACwire against real TSIC hardware cannot run as written.

**triangulated — `rawToCelsius` folds every raw ≤ −7040 to −127**, and −7040 is exactly −55 °C in
1/128 units, the bottom of the sensor's range — so a legitimate −55.0 °C reads as disconnected. All
six fault classes are rejected and all six report as "not connected", sending an operator at the
wiring when the fault is on the probe. `isValidTemperature` is dead code, so a 165 °C reading reaches
the PID.

**device-verified — the DS18B20 powers on with 85 °C in the scratchpad** as "no conversion
performed yet". A machine booting on that reading holds a cold group head at 95 °C for a whole brew.
At reduced resolution the unused low bits are **stale bits from the previous conversion**, not a
coarse reading, so reading all 16 bits gives a plausible, wrong, drifting number.

**single-source — the TSIC change-rate time base is a frame counter, not time.** It divides by a
`volatile uint8_t heartbeat` reset only on an accepted read, so every rejected frame makes the next
easier to accept, and the counter wraps at 256. A port dividing by elapsed time is a different
filter. The same constant is also used in two units in two adjacent files — raw counts in one,
degrees in the other — which puts the effective limits an order of magnitude apart depending on
which you read. Flagged unresolved by both efforts that found it; the single most likely thing to be
wrong in that module.

### 2.5 Pins and relays

**triangulated — the heater relay sits on GPIO2, a boot strapping pin**, and is driven late —
relays are created and driven off only after logger, filesystem, NVS, I²C and the display are up, so
the strapping sample is not guaranteed correct. The steam LED is on **GPIO1, UART0 TX**; the comment
says it "was moved" and it was not. GPIO16/17 are not strapping pins.

**single-source — four panel switches sit on input-only pins 34/35/36/39** with `pinMode(INPUT)` and
no internal pull, so the board must supply external pull-ups. Original-ESP32-specific: the S3 has no
input-only block.

**single-source — relay coils must not draw from the dev board's 3.3 V rail.** The datasheet asks for
≥500 mA and three coils plus an SSR exceed it. Outside what firmware can verify.

---

## 3. Found only by building and running

Nothing in this section appears in any planning document. All device-verified, on `rewrite/rust`.

### 3.1 Two defects that together meant the machine could not heat

**The water-tank switch was inverted.** `SwitchBank::water_tank_full` read `tank_fitted && raw`.
`raw` returns false when **no** float is fitted — the opposite of what the field docs, the method
docs and the boot log all said an absent sensor should do ("Assume full initially",
`SensorCoordinator.h:260`). The machine sat in `WaterTankEmpty` forever and `should_pid_be_enabled`
cleared the PID every tick. Findable only because a boot-log line printed directly above a
contradicting `tank_full=false`.

**The PID never computed.** `Control` caches the controller mode and calls `set_mode` only on a
change. The cache was seeded from **`runtime_pid`** (intent) while `Controller::new` starts in
**Manual**, so the transition was never detected and `compute()` returned false every tick: right
state, right setpoint, 7 K of error, permanently zero duty — visible as `P=0.0 I=0.0 D=0.0`.

Both share a shape worth carrying: **each was type-correct, compiling and host-tested-looking, with
the defect living in a value no type could see.** The PID fix is an API that cannot be seeded
wrongly, not a patched assignment.

### 3.2 Memory, stacks and bring-up

**device-verified — the 1 KB framebuffer on the control task's 8 KB stack** reset the chip with
"A stack overflow in task pthread" on the first display frame. Allocate the scratch once in bring-up:
a large buffer on the heap shows up in the free-heap report, where a stack allocation is invisible
until it overflows.

**device-verified — `app_main`'s stack is 3584 B; the startup sequence needs ~11 KB.** It overflows
into DRAM and surfaces as an allocator assert inside ESP-IDF's `tlsf` that names nothing useful.
Derive stack sizes from `--dwarf=frames`, not from a feeling.

**device-verified — radio bring-up was skipped with no SSID stored, so `esp_netif_init` never ran**,
and the HTTP server's first request hit `assert failed: tcpip_send_msg_wait_sem (Invalid mbox)` —
lwIP's TCP/IP thread does not exist. Call `init_stack()` unconditionally, before the radio and before
the server: a machine with no network still needs its console and API.

**device-verified — ESP-IDF's `httpd` is a single task for the whole server** (`httpd_main.c:533`),
so a handler that loops is a server that loops. One SSE client took the API offline: 150/150 requests
timed out. The research doc blamed chunked transfer-coding; `EspHttpConnection::write` *already is*
`httpd_resp_send_chunk`. The fault is **whose task the write happens on**. Match the C++ shape —
handler sets headers, detaches, registers, returns; a broadcaster task writes.

**device-verified — `esp_restart()` drops unflushed UART0 output.** Bytes in the TX FIFO and anything
in a software buffer are lost. Guard `uart_wait_tx_done` on the driver being installed: calling it
unconditionally makes every reboot print a spurious driver error immediately before it goes down,
hiding the real last line.

**device-verified — `esp_restart()` and `abort()` report different reset reasons.** `esp_restart()`
sets the register so the reason reads `SW`; `abort()` reaches `esp_restart_noos()` and does not. A
resume rule narrowed to `ESP_RST_SW` turned one failing test into an endless loop. Use "anything
except POWERON/EXT".

### 3.3 Tests that were checked and never executed

**68 `#[test]`s in `cc-hal-esp32` were type-checked and run by nothing** — `cargo test` cannot build
the crate (it names `esp_idf_hal`), and the portable-test job does not list it. Two real device bugs
shipped through that gap. Running them found more:

- `mqtt::PlanView::plan()` **panicked on the first publish** — per-group lengths used as absolute
  offsets made `&items[3..2]` out of range for any `from_config` registry. Enabling MQTT would have
  reset the chip. The covering test had vacuous assertions on an empty slice.
- A fake reader returning its first chunk forever meant end-of-stream never arrived, and the buffer
  grew until a **192 KB** allocation took the chip down.
- The provisioning password window **lasted zero milliseconds**: `now.wrapping_sub(opened + WINDOW) >=
  WINDOW` is immediately true because the argument is negative for the window's whole life, so every
  password line parsed as a command and was rejected — behind a success-looking `ok ssid accepted`.
  Host-tested and green; fixed by reverting to the broken form to show it failing, then restoring.

**`cargo test` cannot work on this target.** `panic = "abort"` is forced; unwinding does not link on
Xtensa LX6. Build the harness around *"a failed assert resets the chip"*: write the resume index to
NVS **before** each case (storing on success leaves it pointing at the last case that finished, so a
failing one re-runs forever), and reconstruct the outcome host-side from the line stream.

**Guard it in CI.** `just test-audit` fails on a bare `#[test]` in a device crate (deleted by the
compiler, unreachable by the runner), a test missing from the registry, a stale entry, a duplicate,
and any `#[ignore]`. Self-tested: each failure mode demonstrated to exit non-zero.

### 3.4 The port reproducing a C++ defect in new code

The first HX711 build **armed the signal watchdog on the first conversion**. With no scale fitted,
`note_ready` never fires, so `is_faulted` stays false forever — a machine reporting a healthy scale
while measuring nothing. That is exactly the C++'s defect, reimplemented. The C++ avoids it only by
accident.

Arm from driver start instead. Measured on real pins: one `SIGNAL_TIMEOUT` 120 ms after start, no
hang, no crash, no reset — a state the C++ cannot express, where `HX711Scale::init` spins forever.

**Generalise:** a ported defect is not retired by being understood. It is retired by a test that fails
without the fix.

### 3.5 Found by simulation, not hardware

The TSIC decoder has two protocol details that fail **silently** when wrong, caught only by walking
all 2048 codes: the stop bit is a window of HIGH and produces no edges, so the decoder must detect a
two-window gap between falling edges 9 and 10 — that gap *is* the stop-bit check; and packet 1's three
significant bits sit at positions 5, 6, 7, not 0, 1, 2, with both orderings giving valid parity and a
plausible number.

### 3.6 A test that proved nothing

The exhaustive `state × event` table **collapsed to a single guard**: with every predicate true at
once, guard 1 always won, so 4 140 pairs reduced to 3 outcome names and only ever exercised
*emergency*. The test now asserts that it has not collapsed — pair count, distinct outcome count, and
every guard name present.

### 3.7 Two traps about the instruments themselves

**A timing instrument that has never disagreed with a result is not known to be working.** The first
tick-cost probe read its timestamp *after* the delay and reported 431 ms for a 400 ms period — the
period, not the work. It would have hidden the real overrun rate, and a scale criterion passed
*because* of it. The only reason it was caught is that 431 is not plausible. Take timestamps at the
top of the loop.

**A gate that fails on formatting teaches people to ignore gates.** The device-test audit's import
regex matched only single-line `use crate::{...}`, and rustfmt wraps that list past 100 columns, so
adding an eleventh module produced a spurious "no source file". Fixed to span lines, and re-verified
by introducing a genuinely stale import.

---

## 4. Config and API contracts

**triangulated — the old importer reports success if ≥ 1 parameter updated.** Five valid of 96
produce HTTP 200 "Configuration validated and applied successfully." Import returns a **report** —
accepted / rejected-with-reason / unknown key / out of range — and applies atomically or not at all.

**triangulated — range validation exists on `set()` and not on the load path**, so a corrupt or
hand-edited blob puts an arbitrary setpoint into live control. Re-check the range on decode as well
as on write.

**triangulated — string fields have no length limit and all eight `*_MAX_LENGTH` constants are dead.**
`CONFIG_REFERENCE.md` disagrees with `defaults.h` on five fields. `String::toInt()/toDouble()` return 0
on garbage, and routing a double through Arduino `String(double)` turns `0.005` into `0.01`.

**single-source — the scale calibration is a divisor whose range includes zero, and the negative half
is legitimate** (the shipped config has −1750.05, because an inverted load cell really reads
negative). Narrowing the range to exclude zero excludes real devices; a separate `forbid_zero` flag
is the fix.

**single-source — the exporter wrote keys its own importer rejects.** It emitted each parameter's
full dotted key as a leaf name inside its group object while the import format nests, so 16 keys per
real export named parameters the schema does not have. Under stricter validation **every export the
port produced was refused by its own importer**.

**single-source — `format_version` was rejected by the importer**, so **every real C++ export — the
only file a migrating user has — was refused outright**. It survived two review passes because the
repo's own `config.json` lacks the key. Refuse an unread version **by name and in full, before any
field is looked at**: half-applying a newer document is how a downgrade destroys a configuration.

**triangulated — validate cross-parameter, not per-parameter.** `steam.setpoint = 140` with
`safety.emergency_temp = 120` is a machine that cannot be steamed, and the C++ accepts it. Also
refuse a `LOW_TRIGGER` heater relay — see [open-decisions.md](open-decisions.md) §2 — and discard an
unsafe stored config rather than running it.

**single-source — parameters that look live and are not.** `trigger_type` is registered but inert
where the port wires relays active-low. `space2` ships a test that fails when a schema key is neither
read by the machine nor explicitly listed as inert; that is the mechanism for catching this class.

**single-source — the C++ persists configuration from an HTTP handler.** Every `ParamDef` writes
itself to NVS from its own setter, so a save happens on the request path, on the network task's
stack, and one power cut mid-loop leaves a mix of new and old values. For a machine that heats to
150 °C, "some new" is a configuration nobody ever chose. Persist as **one atomic blob** from a
dedicated low-priority task.

**single-source — the API auth middleware never calls `setAuthType()`**, so `allowed()` returns true
for every request, including factory-reset, restart, wifi-reset and all OTA routes, even with
`system.auth.enabled` set. Four plaintext secrets are exported verbatim with CORS reflecting any
origin, and the shipped defaults are real credentials.

**single-source — `docs/api/openapi.yaml` disagrees with the shipped code in 26 places**, including
a `POST /api/config` that does not exist, a `/api/scale/calibrate` that never existed, and no
`securitySchemes` at all.

**single-source — `/api/steam`, `/api/pid`, `/api/backflush` are toggles that ignore the request
body**, so callers cannot set an absolute state. Reproduce the semantics; they look like a bug but
are the contract. `POST /api/parameters` silently skips empty values, so a string cannot be cleared.

**triangulated — the two repository JSON files disagree with each other and with the code** —
different sensor type (TSIC vs DS18B20), different calibrations (−1750.05 vs 1000.0). The root
`config.json` is not the LittleFS seed file.

---

## 5. PID and state machine

**triangulated — the output clamp lets `NaN` through.** `if (output > outMax) … else if (output <
outMin)` fails both comparisons for `NaN`. The port is therefore a *parity decision*: `f64::clamp`
**panics** on `NaN`, and `min`/`max` differ again.

**device-verified — integer division by zero.** `PID_v1.cpp:85` divides by `SampleTime / 1000` as
`unsigned long`, so any window under 1000 ms gives a zero divisor, ±inf derivative and `NaN` output.
The shipped firmware escapes only because sample time equals the window equals 1000 — **any
heater-output change trips it.** Pinned by a parity scenario that keeps the C++'s `NaN` deliberately.

**single-source — the shipped gains make the controller behave like bang-bang.** `kd = Tv·Kp = 713`;
a 3.1 °C change moves the D term by ~22 000 against a 1000-wide window. Preserved faithfully, flagged
for a human.

**single-source — the brew-detection gains always overwrite the regular gains.**
`calculateDerivedValues()` computes `aggKi`/`aggKd` from `pid.regular.*`, stores them, then
immediately recomputes from `pid.bd.*` and stores over the top. In the shipped config the BD `tn` is
0, so **the brew PID has no integral action at all**. The integrator limit is hardcoded 55 at boot
while the config sets 75, effective only after the first state transition.

**triangulated — the anti-windup dead band exists only in `P_ON_E`.** In `P_ON_M`, the mode the
brew-detection tuning uses, there is no conditional integration and the integrator accumulates while
saturated. A single unconditional gate is wrong in one mode or the other. Reproduced: the strict
interior test freezes the integrator at exactly 0.0 on the first compute after start-up.

**single-source — window size, sample time and output limits are one number in three roles**, set
from `processWindowSize()`. Changing the chop window changes the derivative divisor and the duty
scale together.

**single-source — the Arduino-PID input filter starts at 0**, so the first compute sees a derivative
transient near −14 000, output clamps to 0, and the heater does not start for a full sample period.
A symptom that reads as a sensor fault.

**triangulated — the C++ state-machine tests are far thinner than the suite count suggests.**
`test_state_machine` exercises gMock plumbing, `test_pid_state_transitions` tests hand-written mock
states, and two suites never include the real state sources. The counts are unreconciled (303 macros /
340 cases / 280 / 234 / 50 / 33). Treat "303 tests" as a marketing number, not a safety net — which
is why the port replaces it with an exhaustive table (see §3.6).

**triangulated — `P_ON_M`/`P_ON_E` are inverted between the firmware header and the host-test
stub**, so any existing host test exercises the opposite branch. Fix the stub before capturing any
golden vector, or the vector certifies the wrong branch.

---

## 6. Display parity

Only an oracle finds these; each was a bug in the port's own first attempt.

**triangulated — `U8G2_BALANCED_STR_WIDTH_CALCULATION` is on by default**, so `getStrWidth` adds the
first glyph's x-offset back in and every string starting with an inset glyph measures 1–2 px narrow.
Every centred and right-aligned label moves.

**triangulated — U8g2's unsigned coordinate wrap means a glyph above the display is *clipped*, not
dropped.** `u8g2_is_intersection_decision_tree` returns intersecting for `v0 > v1` in both branches.
Signed coordinates with "reject if off-screen" render blank where C++ renders a partial glyph.

**triangulated — the degree sign is one Latin-1 byte**, which a Rust `&str` cannot hold as a lone
`0xB0`; `"\u{b0}"` is two bytes and measures 30 px against 25. Every temperature readout is affected
and nothing in the compiler flags it.

**single-source — `drawCircle` draws 8 section pixels and `drawDisc` draws 8 lines**, not 6 and 12.

**single-source — the bare and `prepareDisplay`-prepared displays differ.** `ExtendedText` uses
`ascent_para` 7 against `ascent_A` 6, and without `pos top` the Modern template's `fub20` readout at
y=14 renders at rows −9..13, clipped off the top.

**single-source — `kFontHeightFub20 = 23` and `kFontHeightProfont17 = 15` are hand-measured literals**
for the drawn glyph subset, not font metrics (`max_char_h` is 36 and 17). Port the literals.

**single-source — a run buffer is unnecessary, and a fixed 128-entry array ate the bottom of every
large `fub*` glyph** (163 px).

**device-verified — the display costs 63.8 KB of font tables and ~1.5 KB of RAM.** Pixel-identical
rendering is the point of the port and a missing glyph is a layout regression, so the font cost is
irreducible. It is a flash cost, not a RAM cost — which matters, because RAM is the binding
constraint.

**Format beats abstraction here.** As `embedded-graphics` `ImageRaw` the same fonts cost 177 723 B
against 42 722 B as raw RLE — **135 KB** more on a size-constrained target. `rewrite` built an own
framebuffer and logged `DrawTarget` as OPEN, because "we did not need it" is not "we decided not to".

---

## 7. Unexplained

**device-verified · unexplained — the control tick overruns its 10 ms budget in ~62 % of ticks**
(worst 32 ms). The scale is ruled out by measurement: with the sampler disabled the overrun is *more*
frequent (111 of 136 against 86 of 138) with the same worst case. The project acceptance criterion
"zero ticks > 10 ms" fails today.

Relaxing `TICK_BUDGET_MS` would convert a measurement into a pass. The C++ per-iteration histogram is
recorded, so whether the C++ overruns too is checkable — that is the first thing to establish.

---

## 8. Verification discipline

The transferable lessons, in rough order of value:

**A test that asserts the wrong direction hides the bug it was written for.** `space2`'s interlock
test originally asserted *equality* between the states that command the valve and the states allowed
to hold it — precisely the assertion that conceals a missing state (D56). Assert only the safety
direction: a state that commands the valve must be allowed to hold it.

**A missing test command is a shipped bug.** See §3.3. The gap was type-checking, not testing.

**An empty baseline is an error, not a skip.** A directory of port-produced observations labelled
`cpp/` "would be a lie that the parity runner then confirms on every future run". `rewrite`'s runner
reports `BASELINE-MISSING` and exits 2.

**A repo fixture can hide the bug it should catch.** The repository's own `config.json` lacks
`format_version` and the `safety.*` keys, so it passes an importer that rejects every real export.
Take fixtures from the old firmware.

**Claim what the evidence supports.** A synthesised waveform proves arithmetic, not a sensor — the
TSIC-306 host tests are exactly this, and the branch says so in the module docs. Multi-drop 1-Wire
enumeration is unverified and acceptable only because a real machine has one sensor. A target is not
supported because it builds.

**Safety rules that hold regardless:**

- One firmware on the chip at a time. Log C++ for ten minutes, log Rust for the same ten, diff
  offline.
- Every task that energises anything carries a written procedure: element physically isolated,
  observed at the relay input pin, duty swept 0 → 100 → 500 → 1000 → 0, power-cut means.
- Make the harness structurally safe rather than conventionally safe: the parity crate has no GPIO
  and no HAL crate in its tree, so actuator-energising scenarios run on the host; every scenario
  closes with an assertion that duty is 0, the pump is off, both valves are closed and the latch is
  clear.
- Where parity would require removing a safety check, that is a defect in the plan, not a decision.
  Escalate.
- If validation fails three times, or the hardware is unavailable, record that and make no success
  commit.

---

## 9. Sediment in the plans themselves

All three planning corpora carry the same error classes, which is worth knowing before trusting any
of them:

- **Stale counts after a late correction.** 18 states "not 19", 96 params "not 108", 16 crates "not
  17", 96 vs 99 vs 100 parameters. Adversarial review found six such errors in one branch, all in the
  same direction.
- **Superseded decision text left in place under a strike-through** rather than rewritten.
- **A conclusion and its refutation living in different files with no cross-reference.** One branch's
  largest findings document has **two §17s, two §18s, two §19s and two §20s**; it was appended to
  rather than revised, and the earlier versions are wrong with in-file retractions.
- **A dependency graph that makes its own gate un-passable** — parity-baseline capture scheduled in
  Phase 1 while the runner sat in Phase 4; a task to delete the C++ tree whose stated prerequisite is
  "a parity gate that cannot pass".
- **A false hardware belief that changed a decision** — "the original ESP32 has no Bluetooth radio"
  is what made "drop the scales" look safe.

**Append-then-retract does not work in a planning corpus.** Corrections rewrite the section in place,
and any count asserted in prose needs an owner and a test. Assume this corpus drifts and re-derive
before relying on it.
