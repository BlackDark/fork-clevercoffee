# Findings from the parallel implementation branch

**Status:** Cross-reference (2026-09-28)
**Source:** `/Users/marbaced/projects/fork-clevercoffee`, branch `rewrite/rust`, 11 commits `34bf308..aa54861`
**Related:** [inventory.md](inventory.md) · [compatibility-matrix.md](compatibility-matrix.md) · [architecture.md](architecture.md) · [task-list.md](task-list.md) · [ADR 0004](../adr/0004-rust-migration-platform-selection.md) · [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md)

Another agent carried the same migration considerably further in a separate
checkout: ~792 passing Rust tests, a bit-exact PID port, pixel-exact U8g2 display
parity, and hardware-verified sensors. This file records what that work proves,
where it contradicts these documents, and what each side has that the other does
not. It is a cross-reference, not a copy — the authoritative source is that branch.

**It also resolves the device mystery.** The app descriptor read off the attached
board (`v1.3.3-27-g51fa96c-dirty`) is their commit `51fa96c`, and the partition table
on the board matches their `rust/partitions_4M.csv` byte for byte. The board is
running **their** firmware, at the one commit whose own message says
`KNOWN BROKEN -- the ISR panics on its first fire`. `aa54861` fixed it afterwards, so
the board carries a known-panicking image.

---

## 1. Platform facts that would have bitten this plan

These were found by running code on the chip. Both invalidate a design assumption
that looked safe on paper.

### 1.1 A floating-point instruction in a level-1 ISR panics the original ESP32

Guru Meditation, `Coprocessor exception`, `EXCCAUSE 0x4`, core 0, on the first ISR
fire. Cause: the Xtensa FPU is coprocessor 0 and Xtensa **never saves coprocessor
state across an interrupt** — `_xt_coproc_exc` only moves the FP save area between
threads, and an ISR has none, so it dereferences a null save area and panics.
`CONFIG_FREERTOS_FPU_IN_ISR` is the sanctioned opt-in and defaults to `n`.

The trap: their duty comparison did `duty as f32`, which LLVM lowered to real
`ufloat.s`/`ole.s` instructions. **The C++ is safe only by accident** — GCC lowers the
equivalent `double` compare to soft-float library calls, while the `esp` Rust target
advertises `target_feature="fp"` so LLVM emits hardware FP from identical source.
There is no compile-time warning.

Fix: integer compare throughout the ISR. Both operands were already `u32 <= 1000`, so
the float round trip was lossless and redundant.

**Effect here:** [architecture.md §3](architecture.md) already specifies `AtomicU16`
duty and an integer compare, so the design is correct — but for a reason it did not
state. It now must state it, because "use an integer" looks like a style choice and is
actually a hard chip constraint. Their doc also notes a naive disassembly grep for
`.s` instructions false-positives on `divn.s`/`un.s`/`moveqz.s`.

### 1.2 LEDC cannot produce a 1 Hz carrier on the original ESP32

`components/hal/esp32/include/hal/ledc_ll.h:483-489` spin-waits
`while (hw->...conf1.duty_start);` — a loop that exists **on ESP32 only**; every other
chip reduced it to a single register write. It runs inside
`portENTER_CRITICAL(&ledc_spinlock)`, so a duty update masks interrupts for up to one
full carrier period. The interrupt watchdog is 300 ms; a 1 Hz carrier needs ~1 s, so
it panics on every boot — **including at duty 0**, because the wait precedes the duty
value.

They tried LEDC, hit this, and reverted to the 10 ms GPTimer ISR.

**Effect here:** [architecture.md §3](architecture.md) already chose the GPTimer ISR
and explicitly says "not LEDC". That choice is now *empirically* justified rather than
merely inherited from the C++, and the "LEDC costs zero CPU" argument must not be
revisited for this chip.

### 1.3 Smaller ESP-IDF and toolchain traps

| Finding | Consequence |
|---|---|
| `gptimer_set_alarm_action` rejects `alarm_count == reload_count` (`gptimer.c:317-321`) | `reload_count: 0` is the correct periodic configuration. Worth knowing before BOARD-3. |
| `ESP_IDF_SYS_ROOT_CRATE` must be exported for a **virtual workspace** | Otherwise `esp-idf-sys` cannot find `[[package.metadata.esp-idf-sys]]` and **silently ignores `extra_components`** — including the LittleFS component. Silent, so it looks like the component is broken. |
| `~/.cargo/bin` is not on `PATH` in a plain login shell | They fixed it with one `export PATH :=` in the justfile. Matches what [tooling.md](tooling.md) documents; their note that a **backtick assignment is evaluated at parse time, before `export PATH` applies**, is an additional trap this plan had not hit. |
| `Ets::delay_ns` rounds **up** | Use `delay_us` with the C++'s own integers to avoid rounding entirely. |
| `esp-idf-hal` 0.47 `hal::onewire` is unusable | Module doc still lists CRC and command helpers under `todo:`, no `attach_interrupt`, and `onewire_bus` absent from the local ESP-IDF. They bit-banged instead. |
| They used **ESP-IDF v5.5.5**, not v5.3.6 | Both work. [tooling.md](tooling.md) pins 5.3.6; theirs is the version actually proven on this hardware. |

### 1.4 Where they and this plan diverge on the TSIC edge capture

They built a **7 µs poller**, not an interrupt, reporting that `esp-idf-hal` 0.47
"exposes no safe timestamped per-edge callback: `PinDriver::enable_interrupt`
registers a private `handle_isr`, PCNT's `subscribe` is a counter rather than a
timestamp, and there is no `esp_timer` module".

That does not mention **RMT RX**, which
[compatibility-matrix.md §4.1](compatibility-matrix.md) recommends and verified exists
in `esp-idf-hal` 0.47 (`rmt::RxChannelDriver::new`, `receive()`/`receive_async()`,
1 µs default resolution). RMT gives hardware timestamps and one interrupt per reading
instead of a 7 µs poll at ~3 % duty. **Unresolved:** either RMT was not considered, or
it was and failed for a reason not recorded. Worth one look before SPIKE-4 — their
poller works, so this is an optimisation question, not a correctness one.

---

## 2. C++ findings they have and this inventory does not

All of these are new relative to [inventory.md §7](inventory.md). Severity is theirs.

| # | Finding | Why it matters |
|---|---|---|
| 11 | 🔴 **Both pump safety timeouts are dead code.** `PumpTimer::start()` is **never called anywhere** — the only references are two member declarations, two constructor initialisers and two `isExpired()` reads. So `isRunning_` is permanently false and `isExpired()` is unconditionally false. **The 5-minute brew pump limit and the 60-second hot-water limit can never fire.** Hold the switch and the pump runs forever on a machine with a heated boiler. | They call it the single most serious finding in their audit, and it is worse than this inventory's weaker "no maximum brew duration in manual mode" (§5.11), which described the symptom without finding the dead timer. Also uncovered: `MANUAL_FLUSH_RUNNING` and the backflush fill/flush phases run the pump and **neither C++ timer covers them even if armed**. |
| 2 | 🔴 **The steam valve has no safety whitelist at all.** `openSteamValve()` checks only `emergencyMode_`; there is no `steamSafetyShutdownCheck` in the tree. And `ValveState.h:8-11` records that steam and water **share one physical relay (GPIO17)** — so an ungated `openSteamValve()` was an ungated water valve. | This inventory documented the shared relay (§4.5) and the water-valve safety check (§5.11) but **missed that the steam path bypasses both**. A real gap, not a latent one. |
| 3 | **The water valve is not gated on an empty tank** — only `enablePump()` is. | This inventory said the pump is blocked by `waterTankEmpty_` (§5.7) and did not check the valve. |
| 4 | **Emergency stop keeps heating through its debounce window**, ~800 ms at the production interval. | This inventory noted the debounce counts loop iterations (§5.8) without drawing out that the heater stays on for its duration. |
| 12 | 🔴 **`SensorErrorState`'s recovery clock is measured from entry, not from the error clearing** — see §3.1 below. **This contradicts this inventory, and they are right.** | |
| 13 | **None of the four backflush states re-assert their hardware in `update()`.** `Filling` is worst — `onEntry` opens pump and valve and nothing re-asserts. `Flushing` fails inverted: its `onEntry` closes, so a safety check that opens the valve mid-flush is never re-closed. | ADR 0003 exists for exactly this class of bug. This inventory described the `onEntry` actions (§5.4) without checking whether `update()` re-asserts. |
| 14 | **The water switch cannot wake the machine from standby.** `hasUserActivity()` and `shouldExitStandby()` are hard `return false` stubs (`MachineStateContext.cpp:419-429`). | This inventory listed both as transition conditions for `STANDBY` (§5.4) without noticing they are stubs — so it described a wake path that does not exist. |
| 15 | **`powerOff()` shuts down before requesting standby**, leaving one loop in `PID_NORMAL` with hardware off while `update()` can re-enable the pump. | |
| 16 | **The C++ state-machine test coverage is far thinner than the case count suggests.** `test_state_machine` exercises only gMock plumbing (its own comment admits it), `test_pid_state_transitions` tests hand-written mock states rather than real ones, and `test_steam_water_injection` / `test_pid_mode_water_dispensing` never include the real state sources at all. | **Directly undercuts this inventory §2.5**, which counted 303 `TEST` macros and treated the suite as substantial existing coverage. It is not a safety net for the state machine. |
| 17 | 🔴🔴 **`TempSensorDallas` checks three fault sentinels that only a MAX31850 produces** (dead code) and **omits the −127 a DS18B20 actually produces**. Their doc carries a self-correction: the −251/−250 power-on sentinels *are* rejected after all. | |
| 18 | 🔴 **`TempSensor::isValidTemperature` is dead**, so the DS18B20 path has **no range check** and a sentinel can reach the PID and emergency-stop logic. | This inventory noted `isValidTemperature` is never called by the samplers (§4.1) but did not follow through to "a sentinel reaches the safety path". |
| 19 | **`update_moving_average` reads uninitialised timing and divides 0/0 on its first sample.** | This inventory found the `int` accumulator seed and the missing zero guard (§4.1); theirs adds the first-sample case. |
| 6 | **The anti-windup dead band can freeze the integrator at the output boundary.** | |
| 7 | **The shipped PID gains look like bang-bang control, not PID.** Observation, faithfully ported. | |
| 8 | **Config validation is per-parameter only**, so an unsteamable machine is accepted. They added the cross-parameter rule recovered from the oracle: `emergency_temp` must exceed `steam_setpoint + emergency_hysteresis`. | This inventory noted weak validation (§6.6) without the cross-parameter case. |
| 10 | **Duplicate `order` value in the config schema.** | |
| 5 | **Two dead 145 °C constants; the live default is 150 °C.** | This inventory found the dead constants (§5.8) but recorded the live value as coming from config without stating the default. |

### 2.1 Findings the two audits agree on independently

Worth noting because independent agreement is stronger evidence than either alone:
the integer `SampleTime / 1000` divide-by-zero; the 96-registered-versus-98-declared
config gap with `emergencyStopTemp`/`emergencyStopHysteresis` unregistered; the eight
unenforced string-length constants; and the self-transition skip in
`executeTransition` being load-bearing.

Their corroboration for the schema shape is neat: their ported default config
serialises to **2077 bytes** against the lost oracle firmware's logged **2071 B**.

---

## 3. Where this plan was wrong

### 3.1 `SENSOR_ERROR` recovery clock — corrected

[inventory.md §5.12](inventory.md) claimed the recovery delay is measured from the
error clearing, because `ErrorStates.cpp:49` resets `errorStartTime_` in the
error-still-present branch.

**That is wrong, and it was verified wrong against the source in this checkout.** The
reset sits inside `SensorErrorState::checkSpecificTransitions()`, but the sensor guard
at `BaseState.h:145-148` has **no `if constexpr` exclusion for `SENSOR_ERROR`**. So
while the probe is faulted, `checkTransitions` returns `SENSOR_ERROR` before ever
delegating, `executeTransition` discards the self-transition, and
`checkSpecificTransitions` — and therefore the reset — **never runs**. The delay is
measured from entry, so a fault that persists for an hour recovers immediately on
clear.

The C++ comment at `ErrorStates.cpp:47-50` explicitly claims the opposite. This
inventory believed the comment and the local code without tracing the guard that
prevents reaching it. Corrected in `inventory.md`.

### 3.2 `setFontPosTop` — unresolved, and it matters for display parity

[compatibility-matrix.md §4.2](compatibility-matrix.md) states that
`OledDriver.cpp:42-44` calls **both** `setFontRefHeightExtendedText()` and
`setFontPosTop()`, and builds the whole font-anchor analysis — including the measured
0/2/1/1 px deltas and the proposed shim — on that pairing.

Their commit `5b00913` states: *"`OledDriver::prepareDisplay` never called
`setFontPosTop`, so the Modern `fub20` readout at y=14 rendered at rows −9..13 and was
clipped off the panel."*

Both cannot be true. **Unresolved — do not build on either claim until it is checked
against `src/display/OledDriver.cpp` directly.** If they are right, the anchor-shim
analysis needs redoing and the C++ display has a real clipping bug. Their position
carries more weight because it came out of a pixel diff against real U8g2, not a read.

---

## 4. Method worth adopting

Three techniques in that branch are better than what this plan specified:

1. **Compiled-C++ oracles instead of captured golden files.**
   `crates/cc-domain/tools/pid_oracle/` compiles the *actual*
   `lib/Arduino-PID-Library/PID_v1.cpp` against a 35-line `Arduino.h` shim with a
   settable virtual clock and prints raw `f64` bit patterns; the Rust test compares
   `to_bits()` at all 47 steps, measuring max |delta| = 0.0. `cc-display/tools/oracle/`
   does the same for U8g2, linking the real library from `~/.platformio/libdeps` and
   asserting zero differing pixels across 11 scenarios, with an **anti-trivial-pass
   guard** (`total_ink > 10_000` lit pixels) so blank-versus-blank cannot pass.

   This plan's ORACLE-2 specified exporting golden vectors to CSV. A compiled oracle
   is strictly better: it cannot drift from the source, and it re-verifies on every
   test run. **Adopt this for ORACLE-2.**

2. **Exhaustive tables over sampled cases.** 18 states × 46 events × 5 machine
   flavours = 4140 pairs, each reduced once and then driven 64 ticks to a fixed point,
   plus all 4140 against an uninitialised machine. Purity checked by reducing twice
   *and* by a source grep for `Cell`/`RefCell`/`Atomic*`/`static mut`. Similarly, the
   ISR duty fix is verified across the entire input space — 1001 duties × 103 counters
   — rather than at sampled points.

3. **A negative compile test for the safety whitelist.** `water_flow_allowed` is a
   `match` with no wildcard arm; they verified the property by *adding a hypothetical
   19th state* and confirming the build breaks at the whitelist itself rather than
   incidentally elsewhere. [architecture.md §7](architecture.md) asks for a compile-fail
   test on `HeaterCommand`; this is how to make it meaningful.

Also worth copying: a `Secret<T>` newtype whose `Debug`/`Display` redact, with a test
asserting a known password string never appears in formatted output; and a
`[profile.diagnostic]` with identical codegen to release but `strip = "none"` and
`debug = 2`, so a panic can be traced without changing the shipped image.

---

## 5. What each side has that the other does not

The two efforts agree on the big decision — **`esp-idf-svc` / std on
`xtensa-esp32-espidf`** — reached independently, and on **no NVS backward
compatibility** (they recorded that decision on 2026-09-28, the same constraint given
here). They also independently concluded the partition table must change.

| | This checkout | `rewrite/rust` |
|---|---|---|
| Implementation | skeleton crates only, 0 tests | **~792 tests passing**, bit-exact PID, pixel-exact display, sensors on hardware |
| Hardware validation | none — nothing flashed | **flashed, booted, ISR verified at exactly 100 Hz, heater never energised** |
| Partition table | designed, unverified: 1664 KB slots + 640 KB `ccfs` | **device-verified**: 1792 KB slots + 384 KB `littlefs`, from a measured rebalance |
| C++ findings | ~42, broader on build/docs/security | 22, **deeper and more severe on safety** |
| Crates | `cc-domain`/`hal`/`drivers`/`board`/`app` | `cc-domain`/`safety`/`config`/`machine`/`display`/`provisioning`/`hal-esp32`/`firmware` |
| ESP-IDF | 5.3.6 | **5.5.5, proven on this board** |
| Compatibility-break decision | **ADR 0005: explicit, with the OTA-relocatability safety argument** | decided, recorded as a line item |
| **Boot layout guard** | **specified (BOOT-1)** | **absent** — verified: no `find_by_label`/`esp_partition` check anywhere in `crates/` |
| mise/rustup shim trap | **documented in three places** | hit the related `PATH` problem, fixed it in the justfile |
| Size measurement | 974 KB spike with Wi-Fi + HTTP + MQTT + OTA + NVS | 424 KB with sensors, no connectivity yet |

### The one thing this plan has that theirs needs

**The boot layout guard.** Verified absent from their branch — no `find_by_label`,
`esp_partition` or equivalent check exists in `crates/`. Their `boot_halt` covers
*init failure*, not *wrong flash layout*.

It matters more for their table than for the one designed here: their app slots are
1792 KB against the C++'s 1664 KB, and their 424 KB image fits comfortably in either.
So an app-only OTA from a C++ device **boots their firmware against the C++ partition
table**, where the `littlefs` partition does not exist — while the heater, pump and
valve stay wired. That is the half-migrated state
[ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md) was written
to make inert.

---

## 6. Recommendation

**Adopt `rewrite/rust` as the implementation base.** Starting from the skeleton here
would discard verified work — a bit-exact PID, a pixel-exact display port, and sensors
read on real hardware — and would re-derive the two chip constraints in §1 the hard
way.

What to carry across from this checkout:

1. **BOOT-1, the layout guard**, plus the OTA-relocatability argument in ADR 0005 that
   motivates it. This is the one substantive gap in their branch.
2. **ADR 0005 itself** as the written record of the compatibility break — they decided
   the same thing but did not argue it out, and the enforcement question follows from
   the argument.
3. The **mise/rustup shim** documentation and `just doctor` check.
4. The broader build, CI, docs and security findings in [inventory.md §7.3–7.4](inventory.md)
   (auth off by default with wildcard CORS on firmware upload, the unauthenticated
   port-23 log stream, the disabled tests in `.pioignore`, the three clang-format
   versions, the missing `platformio_extra.ini`).
5. The **RMT-versus-poller question** for the TSIC path (§1.4).

What to fix in this checkout either way:

- `inventory.md §5.12` — **done**, the `SENSOR_ERROR` clock claim was wrong.
- `compatibility-matrix.md §4.2` — the `setFontPosTop` contradiction (§3.2) must be
  settled against `src/display/OledDriver.cpp` before the font analysis is trusted.
- `architecture.md §3` — record that the integer duty comparison is a **hard chip
  constraint**, not a style choice.
