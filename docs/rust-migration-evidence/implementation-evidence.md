# Implementation evidence — what building and running the code actually found

The first two documents in this set (`common-design.md`, `common-pitfalls.md`) were built from the
three branches' **planning documents**. This one is built from the **code**, the **git history**,
and the **on-device measurements** — and it is the more trustworthy of the two, because it is
evidence rather than intent.

**Branch status: the three branches are not peers.**

| Branch | Rust code | Host tests | Compiles for device | Run on a device |
|---|---|---|---|---|
| `feat/rust-migration-design` | 7 files, 51 lines (crate skeletons) | no | no | no |
| `refactor/space2` | 80 files, 30 201 lines | 613 + 55 | **no** — board crates have never seen a compiler | no |
| `rewrite/rust` | 135 files, 72 644 lines | 900+ | yes | **yes** — 111 device tests, board running production firmware |

**So there is exactly one source of *empirical* findings, and it is `rewrite/rust`.** Its 18
commits read as a lab notebook. Everything in §2 below was found by building or running something.

`space2`'s own code digest says the quiet part out loud: for that branch the findings are
overwhelmingly *"found by reading the C++ and the HAL contract"*, not *"found by running it"* —
because nothing has been run. Its risk is stated in its own handoff: the board crates are ~400 lines
of `esp-hal` glue written against the HAL's source and **never fed to a compiler**.

---

## 1. The evidence ladder, applied to our own conclusions

This is the most useful correction the code forces on the previous documents.

| Level | Meaning | Count here |
|---|---|---|
| **E4 — device-measured** | Observed on the attached board; the number or failure is in a code comment with a date | ~20 findings |
| **E3 — test-pinned** | A `#[test]` fails if the rule is broken | ~60 rules |
| **E2 — comment-only** | Known, but **no test protects it**; a well-meaning refactor will re-break it | ~15 findings |
| **E1 — host-tested against a model** | Proves the arithmetic, proves nothing about the real sensor | TSIC-306 (all of it) |
| **E0 — asserted** | A design document says so | everything in `common-pitfalls.md` |

**The E2 bucket is the actionable one.** Comment-only knowledge is knowledge that decays. Each item
below marked *comment-only* should get a test or be deleted.

---

## 2. Bugs that only running the code could find

None of these are in any planning document. All are in `rewrite/rust`.

### 2.1 The machine could not heat at all — two independent bugs, one invisible

**The water tank switch was inverted.** `SwitchBank::water_tank_full` read `tank_fitted && raw`.
`raw` returns **false** when no float switch is fitted — the exact opposite of what the field docs,
the method docs *and the boot log* said an absent sensor should do ("Assume full initially",
`SensorCoordinator.h:260`). The machine sat in `WaterTankEmpty` forever, `should_pid_be_enabled`
cleared the PID every tick, and heating was impossible.

> "Findable only because the boot log's own line printed directly above a contradicting
> `tank_full=false`." — commit `c62a894`

**The PID never computed.** `Control` caches the controller mode and only calls `set_mode` *on a
change*. The cache was seeded from **`runtime_pid`** (intent) while `Controller::new` starts in
**Manual**. The transition was therefore never detected, `set_mode(Automatic)` was never called, and
`compute()` returned `false` every tick: `PidNormal`, a live setpoint, 7 K of error, permanently
zero duty. The log showed a correct setpoint, a correct error sign, and `P=0.0 I=0.0 D=0.0`.

Both bugs share a shape worth naming: **each was a type-correct, compiling, host-tested-looking
piece of logic whose bug lived in a value nobody could see from the type.** The fix for the second
was an API that cannot be seeded wrongly (`Controller::in_automatic`, cache seeded *from the
controller*), not a patched assignment.

### 2.2 Hardware panics and resets

| Symptom | Root cause | Fix |
|---|---|---|
| **Guru Meditation, `Coprocessor exception, EXCCAUSE 0x4`, on the first ISR fire** | `AtomicChopper::tick` did `self.duty_ms() as f32`; LLVM emitted real `ufloat.s`/`ult.s`. Xtensa never saves coprocessor state across an interrupt, so the handler dereferences a null save area | `chopper_tick_level_ms` — an integer compare. Verified in emitted code: **zero** FP instructions in the ISR body. Tests sweep **1001 duties × 103 counters** against the f32 reference |
| **`interrupt wdt timeout` on every boot with the LEDC heater** | `ledc_ll_set_duty_start` spins inside `portENTER_CRITICAL` waiting for the last duty change — **original ESP32 only**. At 1 Hz that masks interrupts for ~1 s against a 300 ms watchdog, and **it panics at duty 0 too** | Reverted to the 10 ms GPTimer ISR. `LedcPwm` kept behind a `const` assert so no future edit can reach a duty write |
| **"A stack overflow in task pthread" in a loop, on the first display frame** | `refresh_display` built a 1 KB `Display` on the control task's **8 KB** stack | `Box` allocated once in `bring_up`. Also what ADR-0002 wants: a large buffer shows up in the free-heap report instead of being invisible until it overflows |
| **`assert failed: block_trim_free` (an allocator assert naming nothing useful)** | `app_main`'s stack is **3584 B**; the startup sequence needs **~11 KB**. It overflowed into DRAM | Bring-up moved to a **16 KB** thread. The size is **derived from `--dwarf=frames`**, not guessed (2400+1584+3088+5×~450+1712 ≈ 11 KB → ~40 % headroom) |
| **`assert failed: tcpip_send_msg_wait_sem (Invalid mbox)`** | With no SSID stored, radio bring-up was skipped and **`esp_netif_init` was never called** — lwIP's TCP/IP thread did not exist | `init_stack()` called **unconditionally**, before the radio and the HTTP server. "A machine with no network is a machine whose *console* and *API* still work" |

### 2.3 A whole subsystem taken offline by one design assumption

**ESP-IDF's `httpd` is a single task for the entire server** (`httpd_main.c:533`). The SSE
`/events` handler looped *inside* the handler, so one browser tab holding a stream took the whole
web API offline:

```
before:  150/150 requests TIMED OUT
after:   150/150 returned 200, 53 ms min / 67 ms p50,
         while the stream delivered 92 frames in the same window
```

The C++ does not have this, because `AsyncEventSource` returns from the handler and pushes from the
loop task. The fix is the C++'s *shape*: handler sets headers, detaches with
`httpd_req_async_handler_begin`, registers, **returns**; a dedicated broadcaster task does the writes.

**The research document was wrong about the cause.** `02 §4` blamed chunked transfer-coding.
`EspHttpConnection::write` *already is* `httpd_resp_send_chunk` — no FFI shim was ever needed. The
problem was never the framing; it was **whose task the write happens on**.

### 2.4 Tests that were type-checked and never executed

**68 `#[test]`s in `cc-hal-esp32` were checked by `just lint-esp32` and executed by nothing.**
`cargo test` cannot build the crate (it names `esp_idf_hal`); `just test` lists only the portable
crates. Two real device bugs had already shipped through exactly that gap. Running them found three
more:

- `mqtt::PlanView::plan()` **panicked on the first publish** — it stored per-group lengths and used
  them as absolute offsets, so `&items[3..2]` was always out of range for any `from_config` registry.
  **Enabling MQTT would have reset the chip.** The test that should have caught it had *vacuous*
  retain assertions on an empty slice.
- A fake reader returning its first chunk forever meant end-of-stream never arrived and the buffer
  grew until a **192 KB** allocation took the chip down.
- `esp_restart()` **does not flush UART0** — both markers were lost, *including one written straight
  to fd 1*. This had "a shipped device bug with a unit test that had never been run".

**`cargo test` cannot be used on this target at all.** `panic = "abort"` is forced; unwinding does
not link on Xtensa LX6. The harness is therefore built around *"a failed assert resets the chip"*:
the resume index is written to NVS **before** each case, and the host reconstructs the outcome from
the line stream. The rule is **"anything except POWERON/EXT"** — because `esp_restart()` reports
`SW` but `abort()` does not, so a narrow `ESP_RST_SW` rule made one failing case **loop forever**.

The recurrence guard is the transferable part: **`just test-audit` fails on a bare `#[test]` in a
device crate, a test missing from `CASES`, a stale entry, a duplicate, and any `#[ignore]`.** It is
self-tested — all failure modes demonstrated to exit 1. A gate that fails on formatting teaches
people to ignore gates; this one caught its own regex missing a rustfmt-wrapped import.

### 2.5 The subtlest bug in the set

**The UART provisioning password window lasted zero milliseconds.**
`now.wrapping_sub(opened + WINDOW) >= WINDOW` — for the whole life of the window the first argument
is *negative*, `wrapping_sub` turns that into ~`u32::MAX`, and the comparison is true immediately.
Every password line was parsed as a command and rejected as one, with a success-looking
`ok ssid accepted` on the console. Wi-Fi provisioning could never complete on the device.

It was host-tested and the test passed. The fix was **reverted to the broken form to show it
FAILING, then restored: 2 failed → 2 ok.**

### 2.6 A defect the port reproduced in new code

The first HX711 build **armed the signal watchdog on the first conversion**. With no scale fitted,
`note_ready` is never called, so `is_faulted` stayed `false` forever — a machine reporting a
healthy scale while measuring nothing. That is *exactly* the C++'s defect, reimplemented. The C++
avoids it only by accident. Now armed from driver start, pinned by
`a_cell_that_never_converts_is_faulted_from_the_moment_the_driver_starts`.

Measured on real pins, 120 ms after driver start:

```
I (871) scale: sampling task started at priority 6, stack 4096 B
E (991) scale: DOUT has been high for more than 100 ms -- the cell is not
               answering. ... The weight is reported as absent, not guessed.
```

No hang, no crash, no reset. **"The C++ cannot express this at all — `HX711Scale::init` spins on it
forever."**

### 2.7 Found by simulation, not by hardware

The TSIC-306 decoder has two protocol details that **fail silently** if wrong, and both were caught
only by walking all 2048 codes:

- **The stop bit is a window of HIGH and so produces no edges.** The decoder must detect a
  *two-window gap between falling edges 9 and 10*; that gap **is** the stop-bit check.
- **Packet 1's three significant bits are at positions 5, 6, 7 — not 0, 1, 2.** Both orderings give
  valid parity and a plausible number.

Independence of the simulator was *addressed, not asserted*: the encoder works in duty-cycle
percentages and never computes a strobe; the decoder derives its boundary from the waveform; edges
are found by scanning at 1 µs, so a rounding error becomes a genuinely shifted edge.

### 2.8 A test that proved nothing, and said so

**The exhaustive `state × event` table collapsed to a single guard.** On a machine where every
predicate is true at once, guard 1 always wins, so the whole 4 140-pair table reduced to 3 outcome
names and only ever exercised *emergency*. The test now asserts that it **hasn't** collapsed:
`the_table_covers_5_818_0_pairs` asserts 4 140 pairs, `seen.len() >= 15`, and every guard name must
appear.

### 2.9 The C++'s behaviours that only measurement revealed

- **The C++ heater ISR runs 100×/s but the contactor changes state 0 or 2 times per second.**
  Walking `isr.h:96-118` for a constant duty: 0 changes at duty 0 and 1000, **2** at 50/500/950. A
  100 Hz carrier would make 200 changes/s — **a hundred times the C++'s mechanical duty** on a 2 kW
  contactor. Pinned by `the_carrier_does_not_switch_the_contactor_more_than_the_cpp_does`, which
  asserts *equality* with the C++'s transition count.
- **The control tick already overruns its 10 ms budget in ~62 % of ticks** (86 of 138, worst 32 ms)
  — and it is **not** the scale: with the sampler disabled it is **111 of 136**, the same 32 ms
  worst. The overrun is pre-existing and its cause is still unknown. *"Do not fix it by relaxing
  `TICK_BUDGET_MS`."*
- **`FreeRtos::delay_ms(n)` does not sleep `n` ms.** `CONFIG_FREARTOS_HZ` is 100, so a delay is
  `ceil(n*100/1000)` ticks of a **10 ms** grid. The first device run of the clock suite measured
  **12 ms for a 20 ms request** and failed on it. The band was widened so quantisation is 5 %
  instead of 50 %. The device clock agrees with the host to **1.4 %** — and whether `delay_ms` is
  systematically short is **deliberately left unasserted**.
- **The fitted probe is a DS18B20** (family `0x28`, ROM `2869...af41`, measured 2026-09-28).
  The recovered oracle's boot log printed the same bytes **reversed** — 1-Wire is clocked out
  LSB-first. There is no TSIC-306 and no second probe on this board.
- **The DS18B20 powers on with 85 °C in the scratchpad** as "no conversion performed yet". A machine
  that boots believing the boiler is at 85 °C then holds a cold group head at 95 °C for a whole brew.
- **At 9/10/11-bit resolution the unused low bits of the raw register are stale bits from the
  previous conversion**, not a coarse reading. Reading all 16 bits gives a plausible, wrong, drifting
  number. Pinned by filling the dead bits with `0b111` and asserting the decode is still 90.0.
- **The tank reads empty for the first ~220 ms of a cold boot with a full tank** — `SensorCoordinator`
  seeds "assume full" while `IOSwitch` seeds `LOW`, plus a 200 ms rate limit and the debounce.
  Preserved on purpose; the contradiction is intentional.
- **A press shorter than the loop interval is invisible.** A 400 ms press at 1 Hz polling reports
  nothing at all.
- **The 32-bit millisecond clock wraps every 49.7 days**, and the heater runs across that boundary
  for the whole of it. `since` is wrapping, so the deadman must be too.
- **A bundle holding a reference into itself is not constructible** — only the *bus* travels in
  `ControlArgs`; the panel and the sensor are built inside the task from it.

---

## 3. Measured numbers

| Metric | Value | How measured | Source |
|---|---|---|---|
| **PID at 7 K error** | **473.9 ms of a 1000 ms window = 47.4 %**, P=449.5 I=8.6 D=15.7 | on the board | commit `c62a894` |
| Display | `present=true frames=125 failed=0` in 60 s (2.08 Hz vs a 100 ms target) | on the board | commit `c62a894` |
| ISR tick rate | 42 ticks per 420 ms = **exactly 100 Hz**, 3108 ticks, 0 panics, heater never energised | 30 s run | commit `aa54861` |
| Control tick | **400 ms** period, **10 ms** budget; 86 of 138 ticks over budget, worst 32 ms | on the board | `main.rs:280` |
| Static RAM | **131,688 B = 41 % of 320 KB**, up from 62 KB pre-network. 94,155 B of it is `.iram0` from the prebuilt Wi-Fi MAC — **untouchable** | link map | commit `77ee6f1` |
| Largest single RAM item | `POWER_OF_FIVE_128` = **10,416 B** in `.dram0.data` (7.9 % of static RAM) for one `f64::from_str` | link map | commit `77ee6f1` |
| Flash | 1,197,712 → **1,338,816 B** of a 1,835,008 B slot (**72.9 %**). +63.8 KB of U8G2 font tables | `just size` | commit `c62a894` |
| `std` backtrace symbolisation | **−162,656 B (−12.05 %)** from one line. RAM saving only **1,440 B (1.1 %)** — the ".bss buffer" theory was **not** borne out | `nm` symbol count 148 → 0 | commit `77ee6f1` |
| mbedTLS | **94,545 B / 680 symbols**, from `mqtt → tcp_transport → esp-tls` and `esp_wifi → wpa_supplicant`. **Deliberately not removed** — it changes *which access points the machine can join* | link map | commit `77ee6f1` |
| ABP2 blocking read | `delay(10)` on 50 ms = **20 % of the loop asleep** in C++; removed by making the 10 ms a deadline | C++ `pressureSensor.h:35` | `main.rs:1208` |
| Bring-up stack | **16 KB**, ~11 KB deepest chain, ~40 % headroom, 10 % of the 154 KB DRAM heap | `--dwarf=frames` | `main.rs:413` |
| Heap-shed floor | **30,000 B** | ADR-0002 | `heap.rs:35` |
| HTTP latency, one SSE tab | **55 of 60** requests timed out at 5 s (survivors 6–7 s); without a stream 60/60 `200` in 22–133 ms | on the board | `web.rs:51` |
| ZACwire sampling | 7 µs burst poll ≈ **143 kHz** (app note wants ≥128 kHz), ~40 000 polls/s, **~3 % duty** | arithmetic | `zacwire.rs:89` |
| 1-Wire timing headroom | tightest slot **13 µs against a 15 µs window = 2 µs of slack**; `t_RST` 480 µs **at the limit** | datasheet, const-asserted | `onewire.rs:1035` |
| Telnet ring | 16 × 304 B ≈ 5 KB, down from 64 × 576 B = 37 KB (**12 % of RAM**) | ADR-0002 | `telnet.rs:34` |
| Config blob | **2,077 B** serialised vs the lost oracle's logged **2,071 B** for a 98-key schema — a 6-byte delta, treated as corroboration of the parameter shape | both | `08 §3` |
| Tests | 900+ host, **111 device** on hardware | both | commit `c62a894` |

---

## 4. Decisions reversed by contact with reality

| Decision | First answer | After building it |
|---|---|---|
| Heater mechanism | LEDC at 1 Hz ("costs zero CPU") | **Panics every boot.** Reverted to the 10 ms GPTimer ISR — the *same commit sequence*, two commits apart. "The 'LEDC costs zero CPU' argument does not survive contact with this chip" |
| Heater carrier | "100 Hz reproduces the 10 ms-step / 1 Hz chopping exactly" | **Wrong by 100×.** The level changes twice per window, not a hundred times. Self-corrected in place, including the discarded resolution table |
| Service mode | "the PID is off" | Ejects the machine from whatever state it was in. Now enforced where the command is issued |
| Display draw target | `embedded-graphics` `DrawTarget` | **Not built.** A U8g2-specific framebuffer on `no_std`/size/parity grounds. Logged OPEN, because *"we did not need it" is not "we decided not to"* |
| HX711 driver | the `hx711` 0.7.0 crate | `retrieve()` blocks on `nb::block!` with **no timeout**, so it cannot satisfy "must report a fault, never a hang". ~25-clock bit-bang written directly |
| SSE transport | chunked transfer-coding was the problem | `EspHttpConnection::write` *already is* chunked. The problem was the task |
| Backtrace RAM | "a large backtrace buffer in `.bss`" | **Wrong.** 1,440 B, not the ~10 % implied |
| Bluetooth | "the original ESP32 has no Bluetooth radio" | **False** — and that false belief is what made "drop the scales" look safe |
| `standby.time = 0` | read as a boolean | Means "never idle"; read as a bool the machine idled on its **first tick** |
| The emergency monitor | fed the *filtered* temperature | A genuine over-temperature took **six seconds** to move a 15-sample mean far enough to trip. Now fed the raw reading — but a sensor *fault* deliberately goes to `SENSOR_ERROR` with a recovery delay instead, because "routing them through the emergency stop would make a recoverable fault need a power cycle" |

---

## 5. Still unverified, despite all of the above

Honesty requires this list to be as prominent as the findings.

- **The entire TSIC-306 path.** Host-tested against a *synthesised* waveform. *"A green test run here
  is not evidence that a TSIC-306 works."* The device-side capture is written and **never brought
  up**, because the pin it would capture is carrying 1-Wire traffic from the DS18B20 that is fitted.
  Unproven: clock tolerance, 31.25 µs pulses through a pull-up and a cable, brownout, and *"what it
  does when the supply dips — the one thing that actually matters for a heater."*
- **The heater contactor.** Minimum on/off time, whether a hardware-PWM square wave is acceptable to
  an inductive coil at all, and the realised frequency and duty on the pin. *"Everything above is
  arithmetic."* R1-07 stays open until someone with a scope, a dummy load and the boiler
  **disconnected** measures it.
- **The C++ parity baseline is not captured.** Flashing the C++ runs its own control loop, so
  **13 scenarios report `BASELINE-MISSING` and the runner exits 2.** Nothing was fabricated.
- **The six display template layouts are not verified against the C++.** The C++ templates are
  `UICoordinator` methods pulling values through `SystemContext`; running them off-device means
  reimplementing that, at which point the oracle would measure the reimplementation. The goldens
  record what the port does — *"and where I misread the C++ they will faithfully record and then
  protect the misreading."*
- **Coredump is not enabled.** The partition is allocated but
  `CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH` is `n`, so no coredump is captured today.
- **No weight was ever measured** — no load cell is attached. Accuracy, calibration, dual-cell
  summation and a live tare are all untested, and the **NVS tare write is untested**.
- **No telnet transport**, so ADR-0002's heap-shed consumer does not exist yet.
- `/api/parameters` returns 5 of the C++'s 10 fields; `/api/status` cannot see the radio.
- **Why the control tick overruns 62 % of the time is still unknown.** The scale is ruled out by
  measurement. R4-01b's "zero ticks > 10 ms" acceptance currently fails.

---

## 6. How this changes the earlier documents

**Confirmed by the code** — every ●●● item in `common-pitfalls.md` §3.1 (FP-in-ISR, LEDC spin, ISR
rate ≠ switching rate) is now not a code-reading claim but a **reproduced device failure with a
fix and a test**. The three-way convergence on those was correct.

**Corrected by the code:**
- `common-pitfalls.md` §1.4 said the scale stack's absence was a defect to fix. The port **reproduced
  it in new code** before catching it — the shape is contagious, which is an argument for the tests.
- `common-design.md` §1 described the config round-trip risk. The truth is sharper: on
  `rewrite/rust` the round trip **works** (the oracle's 2,077 B vs 2,071 B is the evidence), whereas
  `space2` found it was the thing most likely to break. Same risk, opposite outcomes.
- `common-design.md` §2.5 said `embedded-graphics` was a live open question. It is settled by
  measurement: **135 KB of fonts**, and the port did not build it.
- `common-design.md` §3 listed "relay polarity" as gating. The port's answer is a refusal adopted
  from the recovered oracle, and the board is running with it.

**New, and not in any planning document:**
- The single most transferable lesson in the whole corpus, from `space2`: **a test that asserts the
  wrong direction hides the bug it was written for.** Its interlock test asserted *equality* between
  the states that command the valve and the states allowed to hold it — precisely the assertion that
  concealed a missing state. The fixed test asserts only the safety direction.
- Second: **a timing instrument that has never disagreed with a result is not known to be working.**
  The first tick-cost probe read its timestamp *after* the delay and reported 431 ms for a 400 ms
  period. It would have hidden a 62 % overrun rate — and a scale criterion passed *because* of it.
  The only reason it was caught is that 431 is not plausible.
- Third: **the device-test audit is the highest-leverage guard in the project.** 68 tests were
  type-checked and never executed, and two real device bugs shipped through that gap before the gap
  was closed.

**What the three-way convergence is actually worth.** The design documents agreeing on a pitfall is
good evidence. The code agreeing with them is better. But note the asymmetry the code exposes:
`design` and `space2` both predicted the FP-in-ISR trap and the LEDC trap from source reading, and
`rewrite` **shipped both bugs and panicked on the device before fixing them**. Prediction is not
evidence. Only the E4 items in §1 have actually been tested.
