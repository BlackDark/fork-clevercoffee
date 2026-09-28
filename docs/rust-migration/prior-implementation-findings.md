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


---

# Part 2 — full extraction (2026-09-28)

The sections above were written from their commit messages. This part comes from
reading their documents in full: `01`–`09`, `intentional-diffs.md`,
`display-parity.md`, their `SKILL.md`, `checklists.md`, `notes.md`, justfile and
Cargo configuration. Raw extraction in `research/prior-impl-extract.txt`.

Two items contradict this plan's own safety design and are escalated in §7.1 and
§7.2. Read those first.

---

## 7. Design conflicts that need a decision

### 7.1 🔴 `LOW_TRIGGER` heater relay — this plan's default may be actively unsafe

[architecture.md §4.1](architecture.md) step 1 and task **BOARD-2** both specify
reading the relay trigger type from NVS and **defaulting to `LOW_TRIGGER` (drive
HIGH)** as the fail-safe default before config init.

The lost oracle firmware **refused a `LOW_TRIGGER` heater configuration outright**,
logging verbatim:

> `A LOW_TRIGGER heater relay cannot be made safe in firmware: an undriven GPIO would energise the heater on every reset`

Their branch adopted the same refusal (`intentional-diffs.md` C2), pinned in
`cc-safety::tests/safety_paths.rs`.

The argument is sound and it is about wiring, not firmware: between power-on and the
first instruction the GPIO is undriven, and on a low-trigger board that means the
2 kW heater is energised. No firmware can prevent it, which is exactly the open
question [architecture.md §4.1](architecture.md) already flags and cannot answer.

So this plan's "fail-safe default" is arguably the opposite. **Two unresolved points:**

1. **Which polarity is this machine actually wired for?** Undetermined on both sides.
   The only signal is the oracle firmware's own belief (`GPIO2 (active high)`); the
   C++ config default is `HIGH_TRIGGER`. Their `01 §10.2 row 11` states plainly that
   closing this needs a multimeter on the coil, the boiler disconnected, a person at
   the machine and a written procedure — "must not be improvised".
2. **Refusing `LOW_TRIGGER` has a cost:** a machine genuinely wired that way becomes
   unrunnable on the new firmware. That is a product decision, not ours.

**Action:** do not implement BOARD-2's default as written. Escalate. If refusal is
adopted, it belongs in ADR 0005's declared-changes list and in
[architecture.md §4.2](architecture.md).

### 7.2 🔴 The `esp-wifi-provisioning` crate would silently break three planned tasks

`esp-wifi-provisioning` depends on `esp-idf-hal` with a **non-optional**
`features = ["rmt-legacy"]`. Cargo feature unification then enables `rmt-legacy`
**graph-wide** — and `not(feature = "rmt-legacy")` is precisely what gates
`pub mod onewire` (`esp-idf-hal/src/lib.rs:54-59`) and `pub mod timer` (`:131-144`).

Adopting that crate therefore **removes `hal::onewire` and the GPTimer module from
the entire dependency graph**, which breaks:

- **BOARD-3**, the heater PWM, which is built on GPTimer;
- **DRV-1**, DS18B20 over 1-Wire;
- **DRV-3**, the TSIC RMT glue.

Check before adopting, not after:

```bash
cargo tree -e features -p esp-idf-hal | grep rmt
```

If `rmt-legacy` appears, do not use the crate — hand-build the portal and record why.
Their `06 R1-06` makes this "step 0". Added to **SPIKE-7**.

Related, and it closes one of this plan's evaluated options by hardware rather than
effort: the ESP-IDF `wifi_provisioning` component's **"USB Serial" transport does not
exist on the original ESP32** — it requires native USB-Serial-JTAG (S2/S3/C3/C6/H2).

### 7.3 Control-task core assignment — direct conflict, neither measured

This plan pins `control` to **core 0** and `net` to core 1, matching the C++ build's
`CONFIG_ASYNC_TCP_RUNNING_CORE=1`. Theirs pins `control` to **core 1** with the
inverse rationale: "so it shares nothing with the Wi-Fi ISR-heavy work that IDF keeps
on core 0". Both are asserted; neither is measured. One experiment settles it.

### 7.4 Task count — theirs is three, this plan's is five

They have `control` (which also renders the display and publishes MQTT),
`provisioning` (exits on success) and `housekeeping`, plus the heater ISR. No `ui`
task and no `storage` task, on the explicit grounds that "their combined cost against
the 10 ms budget is measured before anything is split", and their skill lists "adding
a task for responsiveness" as a mistake requiring written justification.

Their NVS writes are **synchronous inside the control task, rate-limited**, with a
concrete trigger for changing that: introduce an async writer only if measurement
shows it blocks the loop **> 5 ms**.

Worth taking seriously — this plan's five tasks are justified by reasoning, not
measurement, and a `storage` task exists mainly to keep flash writes off the network
task. Their point is that the control task is not the network task either.

---

## 8. Platform constraints and API limitations not previously captured

### 8.1 The task watchdog cannot be shared — this plan's design does not compile

- **`TWDTDriver` is `Send` but not `Sync`** (`esp-idf-hal/src/task.rs:755` has only
  `unsafe impl Send`), and `WatchdogSubscription` is
  `PhantomData<&'s mut ()>` with `feed(&mut self)`. So `main` cannot own the driver
  and hand out `&TWDTDriver`, and there is no shareable feed handle.
- Consequence: the driver must be **moved into** the control task, which subscribes
  itself. That makes "only the control task can feed the watchdog" **structural**
  rather than conventional — a better property than this plan's arrangement.
- **[architecture.md §2.6](architecture.md) subscribes `control` *and* `ui`. That
  cannot be built as described.** Needs revising to a single subscriber. Their
  argument for single-subscriber is also sound on its own: "a network stall must not
  feed it, otherwise a network deadlock hides a control fault."
- `TWDTConfig` has a third required field, `subscribed_idle_tasks: EnumSet<Core>`,
  which defaults from `CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU0/1`. **IDF subscribes
  the idle tasks unless those are set to `n`**, so "only the control task is
  subscribed" is not automatic and must be configured.
  Note their own gap: they identify those Kconfigs as required but **never set them** —
  no `sdkconfig.defaults` exists in their repo, so the property is not yet true on a
  real build.
- `TWDTDriver::new` calls `esp_task_wdt_reconfigure` rather than `init` when the TWDT
  is already up, and `feed()` returns a `Result` — a failed feed is a value, not a panic.

### 8.2 Cross-task messages must be `Copy`

`hal::task::queue::Queue<T>` is bounded by `T: Copy` (`task.rs:980`). **No `String`,
`Vec` or `Box` in any cross-task message** — use `heapless::String<N>`, an index, or a
small `Copy` struct, and encode it as a bound on the message enum so the compiler
enforces it.

Also: `heapless::Deque` **cannot** be a cross-task channel — no interior mutability,
so a producer in another task cannot push without a lock. They hit this as a
contradiction in their own draft.

This affects [architecture.md §2.3](architecture.md)'s `Command` queue, which is
described as `heapless::spsc` "or an `esp-idf-svc` queue" without noting the `Copy`
bound.

### 8.3 `-D warnings` is not achievable on the device target as CI specifies

`esp-idf-sys` 0.38 emits roughly **1300 `esp_idf_*` cfgs** (`cargo:rustc-cfg=…`,
`esp-idf-sys/build/common.rs:239`) but cannot emit matching `rustc-check-cfg`. Every
use is therefore an `unexpected_cfgs` warning, so **`-D warnings` on the device target
fails** until each cfg actually used is declared in `check-cfg`. They declare three.

[tooling.md §5](tooling.md) and `.github/workflows/rust.yml` both specify Clippy
`-D warnings` for every target. That gate will not pass as written once real device
code exists. Noted in tooling.md.

### 8.4 LittleFS needs a managed component, and fails silently without it

`svc::fs::littlefs` requires the `joltwallet/littlefs` managed component:

```toml
[[package.metadata.esp-idf-sys.extra_components]]
remote_component = { name = "joltwallet/littlefs", version = "^1.22" }
```

Their mount code is `cfg`-gated on `esp_idf_comp_joltwallet__littlefs_enabled` so it
**does not compile** when the component is absent — a deliberate mitigation for what
would otherwise be a silent unavailability. Combine with the
`ESP_IDF_SYS_ROOT_CRATE` trap already recorded in §1.3: in a virtual workspace,
`extra_components` is **silently ignored** without it.

### 8.5 Other API facts

| Fact | Why it matters |
|---|---|
| **`esp-idf-hal`'s `critical-section` impl is a FreeRTOS *recursive mutex*, not disable-interrupts**, so `embassy-executor` — which synchronises through `critical-section` — is **not ISR-safe here**. `wake-from-isr` is documented as "only enable if you plan to use `edge-executor`". | [compatibility-matrix.md §2.4](compatibility-matrix.md) rates `embassy-executor` "needs prototype". It is a **concrete disqualifier**, not an unknown. |
| The `embassy-time-isr-queue` feature was **removed in 0.48.0**; async wrappers are native since then. `embassy-time-driver` is backed by the ESP-IDF timer service (1 µs tick). | Any guide telling you to enable it is stale. |
| Executor choice is **unverified as of 2026-09**: the maintainer says in [esp-idf-svc#630](https://github.com/esp-rs/esp-idf-svc/issues/630) he personally uses `embassy-executor`; `async-executor` and `edge-executor` ≤ 0.4.1 had priority-inversion and hang reports; `edge-executor` 0.5.0 is unassessed. | Their mitigation is worth copying: keep the architecture executor-agnostic behind `spawn_*` seams. |
| **Partition tables cannot be set from `sdkconfig.defaults`.** `CONFIG_PARTITION_TABLE_CUSTOM=y` is documented as not working, and a *relative* `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` **breaks the build** ([esp-idf-svc#395](https://github.com/esp-rs/esp-idf-svc/issues/395)). The build needs only the table *offset*. | Their workaround: `[idf] partition_table = "…"` in `.cargo/config.toml` plus `--partition-table` at flash time. Not recorded in [tooling.md](tooling.md). |
| **`IDF_TOOLS_PATH` is explicitly ignored** by `esp-idf-sys`; the knob is `ESP_IDF_TOOLS_INSTALL_DIR`. | This plan sets the latter without noting the former is a no-op. |
| **`RUSTFLAGS = "--cfg espidf_time64"` is mandatory** — without it the build silently uses 32-bit `time_t` and diverges from other builds. They set it in **two** places (justfile `export` and `.cargo/config.toml`), and cargo ignores the config value when the env var is set, so the two must be hand-synchronised — a live duplicate-source-of-truth trap. | This plan sets it once, in `firmware/.cargo/config.toml`. One place is better. |
| **Cargo does not forward a dependency's `cargo:rustc-link-arg` to the binary package**, so without `embuild::espidf::sysenv::output()` in the *binary* crate's `build.rs` the final link contains **no ESP-IDF archives** and fails on undefined `pthread_create`, `write`, `abort`, `sched_yield`. | A first-build blocker with a misleading error. This plan already has the `build.rs`; now the reason is recorded. |
| **The `.cargo/config.toml` key is `linker`, not `rustc-linker`** (cargo 1.97 rejects the latter there), and embuild only *warns* when `$RUSTC_LINKER` is missing, so the failure surfaces late as "unrecognized command-line option '--ldproxy-linker'". | Exact key plus why the diagnostic is delayed. |
| **The flashable app image does not contain the partition table** — its first byte is the `0xE9` app magic; the 3,072-byte table is a separate image at `0x8000`. | The mechanical reason `--partition-table` is load-bearing, and why **an app-only flash can never fix a layout mismatch** — which is the same failure the BOOT-1 guard exists to catch. |
| **`espflash` 4.6.0 auto-detects esp-idf apps**: if the package depends on `esp-idf-sys` it uses the bootloader and partition table from the build script. Bootloaders come from IDF `release/v5.5`. | Explains why `[idf] partition_table` works at all. |
| **First `esp-idf-sys` build compiles all of ESP-IDF twice** (std + no_std) — roughly 15–25 minutes and several GB. Their CI frees disk by deleting `/usr/share/dotnet`, `/opt/ghc`, `/usr/local/lib/android`, `/usr/local/share/boost`, and sets `timeout-minutes: 60`. | CI planning; this plan's workflow sets no timeout. |
| `esp-idf-hal` has **no `mcpwm`** and **no `esp_timer`** module (confirms §1.4). The original ESP32 is the only chip with LEDC **high-speed** mode. | |
| **`espup` needs `--toolchain-version 1.97.0.0 --skip-version-parse` together** on this host: without the version the GitHub latest-release query times out, and `--skip-version-parse` is rejected unless the version is also given. | This plan's `just setup` calls `espup install --targets esp32` with neither. Worth adding if it fails. |
| **`ldproxy` must come from crates.io** (0.3.5), not the `esp-rs/embuild` GitHub release, which tops out at v0.3.2 (2022) and names assets by Rust target triple rather than `uname`, so a constructed curl URL 404s. | This plan already binstalls it. |
| **This host intermittently drops outbound TLS** — the same URL succeeds and fails minutes apart for `espup`, cargo and python, while `curl` and `git` keep working. **A single failure is not evidence of a blocker; retry.** Their CI wraps espup in 3-attempt loops with `sleep 20` and `rm -rf ~/.rustup/toolchains/esp` between tries. | Explains a class of spurious "no network" conclusions. |
| **`espflash monitor` requires a TTY**, so it is unusable from CI or an agent. They drive DTR/RTS themselves via a script using the pyserial inside `.embuild/espressif/python_env/*/bin/python`. | [tooling.md](tooling.md)'s `just monitor` will not work non-interactively. Worth a headless variant. |
| **`cargo bloat` does not work against a stripped release profile** — "symbols section is missing". Fallback is `xtensa-esp32-elf-size -A` plus the link map at `target/<triple>/release/build/esp-idf-sys-*/out/build/libespidf.map`. | Affects any size-attribution work. |
| **`just`'s default shell is `sh` and `read -s` is a bash builtin**, so any recipe reading a secret must declare `#!/usr/bin/env bash`. | This plan's `set shell := ["bash", "-uc"]` covers it globally. |

### 8.6 The heater switching-rate trap — import this reasoning regardless

The C++ ISR fires at **100 Hz**, but the relay level changes only about **twice per
second**. The predicate `pidOutput > counter` is monotone over the window, so there is
one falling edge inside the window and one rising edge at the wrap — and **no change
at all at duty 0 or full duty**. A square wave makes `2f` changes per second, so the
equivalent carrier is `f ≤ 1 Hz`.

This is what made an earlier revision of their plan specify a **100 Hz** carrier,
which would have switched a 2 kW contactor **200 times per second — a hundred times
the C++'s mechanical duty**. Anyone reading the ISR rate as the switching rate gets it
100× wrong.

Still unmeasured on both sides, and deliberately not guessed: the contactor's minimum
on/off time, whether a hardware-PWM output is acceptable to the coil at 1 Hz at all,
the realised frequency and duty on the pin (no scope has been attached), and whether
1 Hz is the right point on the wear-versus-resolution curve.

---

## 9. Hardware facts measured on this board

From their `01 §1`, `§10`. Several correct or sharpen this plan's
[inventory.md §1](inventory.md).

| Fact | Consequence |
|---|---|
| **The USB bridge is a WCH CH340**, VID `0x1A86` / PID `0x7523`, `iProduct = "USB Serial"`. **No Silicon Labs `0x10C4` device is present at all** — the CP2102 assumption in their own earlier docs was wrong for this board. macOS derives `usbserial-204140` from `locationID 0x20414000`. | Driver guidance differs (CH34x). This plan says only "external USB-UART bridge", so it did not inherit the wrong claim — but the positive identification is worth having. |
| **Auto-reset works: 5/5 first-try connects, no manual BOOT+EN dance**, each ending `Hard resetting via RTS pin`. Reset circuit is DTR→GPIO0, RTS→EN through two transistors. | Non-interactive flashing is viable. Keep a hold-in-reset fallback but do not design around it. |
| **The serial link is flaky above 460800 baud — stay at the default rate.** | Concrete and measured. |
| **If flashing fails, suspect the cable and `board-info` first, not the firmware.** | |
| **Flash is DIO @ 40 MHz, JEDEC `0xD8`/`0x4016`, 4 MB.** ESP-IDF logs `spi_flash: detected chip: generic` because `0xD8` is not in its vendor table. **SFDP could not be read** — esptool 4.11.0 aborts with "Reading more than 32 bits back from a SPI flash operation is unsupported" — so **QIO support is unverified and the part is unidentifiable from software**. | **Do not assume a GD25Q32 and do not "upgrade" to QIO.** It looks like free speed and is not safe to change. |
| **No PSRAM**, evidenced by zero `psram`/`spiram` occurrences in a full boot log and by `heap_init` listing only DRAM/D-IRAM/IRAM (6 KiB + 150 KiB DRAM, 14 KiB + 111 KiB D/IRAM, 31 KiB IRAM) — **with the honest caveat that ESP-IDF only probes PSRAM when `CONFIG_SPIRAM` is set.** Cheapest closure: call `esp_psram_get_size()` in the first Rust `main` and assert. | This plan asserts "no PSRAM" without an evidence chain. |
| **The module is an ESP32-WROOM-32E** on four independent signals, including `Flash voltage set by a strapping pin to 3.3V`, which **excludes WROVER-E** (1.8 V VDD_SDIO). Residual gap: nobody read the silkscreen. | Matters because a WROVER would put GPIO16/17 on in-package PSRAM. |
| **GPIO16 and GPIO17 are *not* strapping pins.** The strapping pins are exactly GPIO0, GPIO2, MTDI/12, MTDO/15, GPIO5 (Datasheet v5.3 Table 3-1). GPIO16/17 are flash/PSRAM pins (Table 2-5), unused inside a WROOM-32E, and both measured working (valve on 17, 1-Wire on 16). | Retires a phantom risk with a citation. This plan correctly flags GPIO2 and says nothing about 16/17. |
| **Silicon revision v3.0 is the newest original-ESP32 silicon**, so the errata concern is moot and only RF is even in question. ESP32 has no OTP chip ID, which is why `esptool` reports the MAC instead. | |
| **`debug_tool = esp-prog` (`platformio.ini:47`) is probably wrong for this board** — it needs an FT2232H probe and the board has only a CH340. Upload is unaffected, but **`pio debug`/gdb is expected to fail. Untested — do not promise a gdb workflow.** | |
| **Relay active-high versus active-low is UNVERIFIED and cannot be determined from a laptop.** | This is the physical fact §7.1 turns on. |

### 9.1 🔴 A DS18B20 is fitted, not a TSIC-306 — this invalidates SPIKE-4 as written

The probe on the board answered 1-Wire: **ROM `0x41af78cdaa376928`, family `0x28`,
11-bit, read every 400 ms, live 22.88–23.25 °C**. A ZACwire sensor would not answer
1-Wire at all.

Meanwhile `include/clevercoffee/Config.h:1087` defaults to
`TemperatureSensorType::TSIC_306`, and `HardwareManager::initializeTemperatureSensor`
builds whichever the config names **without detecting the mismatch**. The lost oracle
firmware logged a substitution rather than refusing:

> `config asks for Tsic306 but only the DS18B20 driver exists; reading the 1-Wire bus anyway`

Three consequences:

1. **SPIKE-4** is written as "ZACwire decode via RMT against real TSIC hardware" —
   aimed at a sensor that is **not attached**. It cannot run as specified. Corrected in
   the task list.
2. An operator cannot currently tell which sensor feeds the over-temperature
   interlock. Their port selects the driver from a board constant and logs the
   configured value beside the answering one — worth copying.
3. Their caveat, which this plan should keep: the measurement came from a *Rust* image
   that does not print the probe pin, so GPIO16 is inferred. **Re-confirm under the C++
   firmware.**

### 9.2 Exact C++ artefact sizes

| Artefact | Bytes |
|---|---|
| `firmware.bin` | **1,546,240** (5 segments: DROM 394,996 / DRAM 26,096 / IRAM 37,636 / IROM 1,035,684 / IRAM 51,204) |
| `bootloader.bin` | 17,536 |
| `partitions.bin` | 3,072 (magic `0x50AA`) |
| `firmware.elf` | 51,335,052 |
| Headroom in the C++ table | **157,696 B = 154.0 KiB (9.25 %)** |

PlatformIO reports 1,539,657 B; the 7,583-byte delta is the 24-byte image header plus
8-byte segment padding plus a trailing SHA-256. [inventory.md §2.2](inventory.md) has
only the PlatformIO figures.

**Live LittleFS occupancy from the device: 225,280 of 393,216 bytes used**, i.e.
167,936 free — the only real datapoint on the gzipped bundle's size. This plan says
only "640 KB is the hard ceiling".

Also: **the frontend cannot be built on this host** (`pnpm install` fails,
`registry.npmjs.org` returns HTTP 000 while `github.com` returns 200, and
`packageManager` pins pnpm 11.25.0 against the installed 12.6.0), so `buildfs` cannot
run and the real filesystem image size is unknown. Read this together with the flaky-TLS
finding in §8.5 — their own notes call it a flaky path and say explicitly not to record
a single failure as "no network".

### 9.3 The 20 % figure

`delay(ABP2_READ_DELAY_MS)` = 10 ms against `PRESSURE_UPDATE_INTERVAL_MS` = 50 ms
means **20 % of the control loop is asleep, permanently, brewing or not**. That is the
headline performance number for the migration and the basis of its only quantified
improvement claim. [inventory.md §4.2](inventory.md) records the 10 ms blocking read
but never draws the ratio.

Non-obvious porting detail: their split-phase driver anchors the next command on the
previous **command**, not on the completed read, so the period stays 50 ms rather than
becoming 60 ms. A naive split-phase port silently changes the sample rate.

---

## 10. The recovered oracle — a previous Rust firmware for this project

Unique information that exists nowhere else, from their `08`.

**What it was.** A complete, sophisticated Rust firmware for *this* project already
ran on this board: `cc_firmware`, ESP-IDF v5.5.5, version `v0.0.1-3-gfb0564a-dirty`,
built **2026-09-26 22:39:26**. **Its source is gone** — `fb0564a` is not a git object
in the repo, there is no `Cargo.toml` for it anywhere on the machine, and no remote
branch carries it. The binary is the only record, so it is a **parity oracle, not a
codebase**. Note it predates the `51fa96c` image identified on the board earlier, so
two different Rust firmwares have run here.

**How facts were recovered**, all from a full 4 MB `read_flash` dump taken before the
device was erased, plus a live UART boot log: the partition table parsed at `0x8000`;
image size inferred from the last non-`0xFF` byte in the app0 slot; string extraction
over the app image (50 route strings, 97 config keys, log formats, error messages —
**the extractor truncates at 200 chars**, so several are cut off mid-sentence); log
tags used to reconstruct the module split; the boot log for runtime config; the
LittleFS partition inspected for filenames; `esptool flash_id` for JEDEC bytes.

> **Handling rule, and it is load-bearing:** the dump contains **Wi-Fi credentials in
> plaintext NVS**, so it lived only in a scratch directory outside the project and
> "do not commit a flash dump" is a banner rule. [The skill](../../.agents/skills/esp32-rust-migration/SKILL.md)
> §7 has the weaker "write dumps under gitignored `research/`". The stronger framing —
> never in the repo at all, and the reason — is now added.

**Its partition table** is the geometry on the board today and the one their branch
adopted: `nvs` `0x9000`/`0x5000`, `otadata` `0xE000`/`0x2000`, `app0` `0x10000`/`0x1C0000`
(**1,835,008 B**), `app1` `0x1D0000`/`0x1C0000`, `littlefs` `0x390000`/`0x60000`
(**393,216 B**), `coredump` `0x3F0000`/`0x10000`. Versus the C++ table: **+128 KB per
app slot, −256 KB filesystem**, with nvs/otadata/coredump byte-identical and only moved.

That geometry was **independently derived twice** — once by the lost oracle's author,
once by their branch from a rebalance formula. **ADR 0005's table keeps the C++'s
1664 KB app slots and only renames the filesystem partition**, so the two plans
disagree on slot size. Theirs is better evidenced.

**Config storage format** — the substantive finding, previously captured here only as a
footnote: **a single nested JSON blob in one NVS namespace, 2,071 bytes stored**, from
the boot line `cc_firmware: config: nvs (2071 B stored)`. It **kept the C++ dotted key
names** (97 keys: `pid.*`, `brew.*`, `safety.*`, …) rather than inventing a prefix. And
`safety.emergency_temp` / `safety.emergency_hysteresis` **are present and persisted** —
that author independently found and fixed the same registration bug.

This corroborates [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md)'s
single-versioned-blob decision, reached independently. One difference worth noting: ADR
0005 uses `postcard`; the oracle used JSON. JSON is larger but human-readable in a dump,
which is how these facts were recovered in the first place.

### 10.1 Behavioural and safety rules recovered from it

These are not derivable from the C++ source. The first two are the valuable ones.

| # | Rule | Why it matters |
|---|---|---|
| **O1** | **Deadman heartbeat latch.** The supervisor beats; if it stops beating, the heater is de-energised. Telemetry carries `deadman=armed` / `deadman_tripped`. | Strictly stronger than watchdog-only: the heater drops within the interlock period (500 ms) instead of at the next 5 s TWDT reset. **This plan has no equivalent concept** — "deadman" appears nowhere. |
| **O2** | **A latching software gate in front of the heater at boot:** `heater interrupt running on GPIO2 (active high), output held off until the supervisor beats`. | The output is not energised at all until the first heartbeat. [architecture.md §4.1](architecture.md) safes the outputs in `main` but has no "stay off until a live supervisor proves itself" gate. |
| **O3** | **`interlock 500 ms`** — a configured, logged periodic actuator re-assert interval. | A named, tunable safety period. Their own `08` judges it *weaker* than re-asserting every control tick, so carry it as an upper bound, not a target. |
| **O4** | Config-time cross-parameter validation: `safety.emergency_temp` must exceed `steam.setpoint + safety.emergency_hysteresis`, *"or the machine would stop itself while…"* (string truncated). Margins: **10 °C** at defaults, **25 °C** at range maxima. | Without it the machine trips emergency stop during normal steam use. |
| **O5** | **A `LOW_TRIGGER` heater relay is refused outright.** | See §7.1 — a direct conflict with this plan. |
| **O6** | **Fail-closed config handling**, three verbatim strings: `(configuration is unsafe to run: …) → discarding all of it and running defaults`; `refusing to store an unsafe configuration: …`; `unsafe stored configuration discarded: …`. | An unsafe stored config is discarded wholesale in favour of defaults, and an unsafe config is refused on store. **Neither this plan nor theirs has this** — both would happily run a config that trips emergency stop on every steam shot. |
| **O7** | The supervisor subscribes *itself* to the TWDT (`esp_task_wdt_add -> 0`). | Independently confirms the ownership design forced by §8.1. |
| **O8** | Runtime config: FreeRTOS tick **1000 Hz**, heater window **1000 ms**, heater on **GPIO2 via interrupt, active high** — it reproduced the C++ 1 Hz / 100-step chopper and **did not** move to LEDC. | The parity baseline, and the author reached the correct answer before LEDC was proven impossible. |
| **O9** | Telemetry schema including `commanded_duty_ms`, `last_window_on_ticks`, `observed_on_fraction`, `deadman_armed`, `deadman_tripped`. Sample: `PidDisabled temp=22.62 setpoint=95.0 duty=0ms last_window_on=0/100 on_fraction=0.000 deadman=armed events=133`. | `observed_on_fraction` is **measured heater on-time fed back into telemetry** — a self-check that the commanded duty was actually delivered. **Neither plan has output verification.** |
| **O10** | An `EmergencyReport` type with `trip_temperature_c`, `threshold_c`, `active`. | The C++ `EmergencyStopManager` has no such report type; useful for the web and MQTT surfaces. |
| **O11** | **A fault-injection debug surface, 18 routes:** `/debug/panic`, `/debug/fault`, `/debug/hang-supervisor`, `/debug/hang-server`, `/debug/brew/{start,stop}`, `/debug/steam/{on,off}`, `/debug/flush/{on,off}`, `/debug/hotwater/{on,off}`, `/debug/backflush/{start,stop}`, `/debug/tank/{empty,full}`, `/debug/ignore-sensor`, `/debug/use-sensor`. | **This is how you test a deadman and a supervisor hang without a boiler.** Neither plan has any fault-injection endpoints. The actuator ones are dangerous by construction and belong behind a build feature. |
| **O12** | **UART Wi-Fi provisioning, a working protocol:** `wifi set <ssid>` with **the password on the next line**, plus `wifi clear`, `wifi status`, `wifi apply`. | Password on a separate line rather than in argv sidesteps exactly the leak this plan worries about. Shapes task **NET-5**. |
| **O13** | It also shipped an on-device captive portal (`/wifisave`, `/paramsave`, `/param`, `/erase`, `/update`, `/close`, `/restart`, `/exit`, `/info`, `/status`, `/wifi`, `/0wifi`). | **Both** provisioning options are proven on this hardware. |
| **O14** | REST additions over C++ parity: `POST /api/wifi`, `POST /api/wifi/clear`, **`GET /download/coredump`**; HA discovery prefixes `/binary_sensor/`, `/button/`, `/number/`, `/sensor/`. | `/download/coredump` is a cheap, high-value diagnostic given the 64 KB coredump partition already exists. |
| **O15** | **Web UI delivery:** a `…/ui/littlefs` route string exists but **no HTML/JS/CSS filenames were found in the LittleFS partition**, while the boot log reports `littlefs: 225280 of 393216 bytes used`. | Inference: the bundle was **embedded in the binary**, with LittleFS serving user content. Explains part of the large app image and is direct evidence for the "embed the SPA" option. |
| **O16** | **TSIC-306 was aspirational, never implemented** — it logged the substitution quoted in §9.1. | Reframes TSIC from "the spike that can invalidate the approach" to "decide what the config value does". The oracle's own fail-closed validation is the precedent: reject it at config time. |

**What `08` records as still unknown:** the source is gone, so no design intent, tests
or history; QIO support unverified (§9); PSRAM not positively excluded; MQTT/HA payloads
and display templates unrecoverable from strings; the OLED font approach unknown.

⚠️ **One figure not to quote.** `07 §1` claims "a Rust esp-idf image with `std`,
measured: ≥ 1,835,008 B — it filled a 1,835,008 B slot". That is an **inference from the
last non-`0xFF` byte**, not a measured image size, and a string literal landing exactly
at the slot boundary is suspicious. It sits against their own 382,528 B minimal build
and this plan's 974 KB spike. **The 974 KB figure is the better-founded number.** Their
own doc says to size the table around the (still unmeasured) SPA instead.

---

## 11. Additional C++ findings

Beyond §2. Severity theirs; `[verified]` means the extraction confirmed it against the
source in this checkout.

### 11.1 Safety

- 🔴 **Steam mode can be flipped over HTTP with no state change, and it moves the
  boiler setpoint.** `WebServerManager.cpp:444-445` calls
  `setSteamModeActive(!isSteamModeActive())` directly, and
  `ProcessController.cpp:119` feeds `isSteamModeActive()` into `updateSetpoint`.
  **[verified]** So an HTTP request raises the boiler setpoint to `steam.setpoint`
  from **any** state — with no transition, no steam timeout
  ([inventory.md §5.11](inventory.md)) and no steam over-temperature protection —
  while web auth is off by default behind wildcard CORS
  ([inventory.md §7.3](inventory.md)). **A compound, unauthenticated path to steam
  temperature.** It is also the case that proves a *mode*-based safety gate is wrong:
  the gate must key on machine **state**.
- **The un-whitelisted steam-valve open exists at two independent sites**, both with
  the same emergency-only check: `HardwareManager.cpp:397-400` and
  `MachineStateContext.cpp:556-558`. §2 implies one call site.
- **A short 1-Wire scratchpad read is undetectable** — nothing checks the returned
  byte count, and only the all-`0xFF`/`0x07` case is caught via the sentinel path.

### 11.2 PID

- 🔴 **The output clamp lets `NaN` through.** `PID_v1.cpp:122-125` is
  `if (output > outMax) … else if (output < outMin) …`; `NaN` fails both, so the `NaN`
  from the `SampleTime / 1000` divide reaches `*myOutput` **unclamped**. **[verified]**
  This is *why* the divide is not self-limiting. Note for the port: `f64::clamp`
  **panics** on `NaN` and `min`/`max` propagate differently again, so **the clamp is
  itself a parity decision**, not a detail.
- **The anti-windup dead band exists only in `P_ON_E`.** `PID_v1.cpp:70` is
  `if (!pOnE || (…))`; the `!pOnE` arm short-circuits, so in **`P_ON_M` — the
  brew-detection tuning set** — there is no conditional integration at all and the
  integrator accumulates while saturated, bounded only by the `0..aggIMax` clamp.
  **[verified]** A port implementing one unconditional gate is wrong in one mode or
  the other.
- **Window size, PID sample time and output limits are one number in three roles**,
  set only at `SystemInitializer.cpp:551-552` from `processWindowSize()`.
  **[verified]** Changing the chop window changes the derivative divisor *and* the duty
  scale at once.

### 11.3 Temperature

- 🔴 **`TempSensorTSIC`'s rate-limit latch is a function-local `static` in a `const`
  member function** (`TempSensorTSIC.cpp:28-29`) — process-global, never reset, shared
  across instances. **[verified]** Once latched, a probe reconnect starts in the strict
  5 °C/sample regime instead of the permissive 200.
- 🔴 **The TSIC no-signal timeout is 100 ms against a 10 Hz sensor** (`ZACwire.h:29`,
  used at `ZACwire.cpp:111`). **[verified]** One transmission period, zero margin, on a
  safety input: a single jittered frame reports a working probe as disconnected. The
  field is `uint8_t`, so nothing above 255 ms is even expressible. Their port uses
  250 ms.
- 🔴 **`RUNTIME_CHANGERATE` is used in two different units in two adjacent files.**
  `ZACwire.cpp:58` computes a gradient from the **raw 11-bit count**;
  `TempSensorTSIC.cpp:38-39` compares in **degrees**. **[verified]** Under the count
  reading the effective limits are ≈19.5 and ≈0.49 °C/sample; under the degree reading
  200 and 5. Their own doc calls this *"the single most likely thing to be wrong in the
  module"*, unresolved for want of hardware.
- 🔴 **The TSIC change-rate filter becomes more permissive the longer it rejects.**
  `ZACwire.cpp:58-61` divides by `heartbeat`, a `volatile uint8_t` incremented per ISR
  frame and reset **only on an accepted read**; `prevTemp` likewise. So every rejected
  frame grows the divisor, shrinking the computed gradient, making the next frame easier
  to accept — and `heartbeat` wraps at 256, after which the divisor is meaningless.
  **[verified, and recorded in neither project's docs before now]** The filter's time
  base is a frame counter, not time: a port that divides by elapsed time is a
  *different filter*.
- **TSIC's range reject is half dead and the live half cannot fire at its stated
  bound.** `TempSensorTSIC.cpp:59` is `temp <= 0.0 || temp >= 180.0`. **[verified]**
  `>= 180` cannot fire on a TSIC-306 (11-bit span ends at 150 °C), and `<= 0.0` cannot
  fire at exactly 0.00 °C because 0.00 is off the 11-bit grid. The constant is shared
  with the TSIC-506, which is why their port left it alone. Note **a 150.00 °C boiler
  reading is accepted** by both.
- **All six Dallas faults are reported as "Temperature sensor not connected"**, because
  `rawToCelsius` folds every raw ≤ −7040 to −127 and can never produce the
  −254/−253/−252 the second check looks for. **[verified]** The *decision* is right;
  only the diagnostic is wrong, so it is a free fix. A probe brownout currently sends an
  operator to inspect wiring.
- **`rawToCelsius` cannot represent −55 °C** — `DEVICE_DISCONNECTED_RAW = -7040` is
  exactly −55 °C in 1/128 units and the test is `raw <= …`, a **range** check rather
  than a sentinel check, so the bottom of the DS18B20's physical range reads as
  disconnected. A genuine library off-by-one.

### 11.4 Pressure

- **Percentage and bar use different divisors, so full scale reads 90 %.**
  `pressureSensor.h:55` divides by the full 24-bit range for the percentage while
  `:57-58` divides by the output span for bar. **[verified]** At 10 bar the percentage
  is exactly 90 %. Preserved in their port because the percentage is only a log field.
- **Counts below `outputmin` give a negative bar reading and the C++ carries it on.**
  **[verified]**
- 🔴 **A short read converts the *previous* sample as fresh.** `Wire.requestFrom`'s
  return is discarded and `ABP2_data` are file-scope globals, so a short read leaves the
  prior bytes in place. **[verified]** This is a *quiet* failure, distinct from the loud
  0xFF-filled one [inventory.md §4.2](inventory.md) records.

### 11.5 State machine

- **`BREW_PREINFUSION_PAUSE` is "brew active" but its `update()` disables the pump**
  (`BrewStates.cpp:173`). **[verified]** Any pump-runtime watchdog keyed on
  `isBrewActive()` over-counts pause time as pump time; their port excludes the state
  from watchdog arming for exactly that reason.
- **The two pump watchdogs act by two different mechanisms**: the brew timer *requests*
  a stop via a flag consumed by the next `checkTransitions`, while the hot-water timer
  calls `disablePump()` directly. Their log strings —
  `"Pump timeout - stopping for safety"` and
  `"Hot water pump timeout - stopping for safety"` — were **also recovered verbatim from
  the lost oracle firmware**, so they double as oracle evidence.

---

## 12. Display: U8g2 behaviours a Rust port gets wrong by default

From their `display-parity.md`. All new here, and §3.2's open question is now closed.

### 12.1 `setFontPosTop` — RESOLVED, this plan's reading was correct

**`src/ui/OledDriver.cpp:42` calls `setFontRefHeightExtendedText()` and `:44` calls
`setFontPosTop()`** — verified directly in this checkout. Their
`display-parity.md` agrees. The commit message quoted in §3.2 was about their **Rust**
`Display::prepare_display` failing to mirror the C++, not about the C++ lacking the
call.

**§3.2 is closed and [compatibility-matrix.md §4.2](compatibility-matrix.md)'s
anchor-shim analysis stands.** One correction: the file is `src/ui/OledDriver.cpp`, not
`src/display/OledDriver.cpp`.

### 12.2 The behaviours

| # | Behaviour | Consequence for a port |
|---|---|---|
| **B1** | **`U8G2_BALANCED_STR_WIDTH_CALCULATION` is defined unconditionally** at `u8g2.h:175` **[verified]**, so `getStrWidth` adds the **first** glyph's x-offset back in (upstream issue #1561). | Without it, every string starting with an inset glyph — `!`, `(`, `i`, `"` in `profont10` — measures 1–2 px narrow, which moves **every centred and right-aligned label**. |
| **B2** | **`u8g2_uint_t` wraps, and a wrapped range counts as intersecting.** `u8g2_is_intersection_decision_tree` has an explicit `if (v0 > v1) return 1` arm in **both** branches, and `v0 > v1` is exactly what an underflowed unsigned coordinate looks like. | **The highest-value display item.** A glyph box starting above the display is **clipped, not dropped**: a 30 px digit at baseline y=12 renders its bottom 12 rows. A Rust port using signed coordinates with a "reject if off-screen" test renders **blank** where the C++ renders a partial glyph. |
| **B6** | **The degree sign is one Latin-1 byte.** `drawStr` walks **bytes** and the firmware's strings are Latin-1 (`static_cast<char>(176)`). | **A guaranteed Rust-specific defect:** a Rust `&str` cannot hold a lone `0xB0`, so `"\u{b0}"` is two bytes and the byte walk measures **30 px instead of 25** in `profont10`. Every temperature readout carries a degree sign, and nothing in the compiler will flag it. Needs a code-point→Latin-1 fold. |
| **B3** | **`draw_glyph` needs no run buffer.** An earlier version of their port buffered RLE runs in a fixed 128-entry array and dropped the overflow, silently eating the bottom of every large `fub*` glyph — **163 wrong pixels**. A `fub30` glyph emits several hundred runs. | Stream the runs instead of buffering. |
| **B7** | `setFontRefHeightExtendedText()` uses `ascent_para` where the default mode uses `ascent_A` — **7 vs 6 px in `profont10`** — and without `setFontPosTop` a `y` is a **baseline**. | Combined, the Modern template's `fub20` readout at y=14 would land at rows −9..13, clipped off the top. This is the effect their commit described, in their Rust mirror. |
| **B4 / B5** | `drawCircle` draws **eight** pixels per section, not six (the second is the diagonal partner `(x0 ± y, y0 ∓ x)`); `drawDisc` draws **eight** vertical lines, not twelve. | Minor, but both were bugs in their own first port. |
| **B10** | **Font storage, measured:** ten fonts as raw U8g2 RLE = **42,722 bytes**; as `embedded-graphics` `ImageRaw` = **177,723 bytes**. | A **135,001-byte** saving on a size-constrained target — relevant to this plan's image budget and to the `embedded-graphics` choice. |

### 12.3 What their display parity does **not** prove

Worth importing verbatim, because "pixel-exact display parity" is easy to over-read:

- **The six normal layouts are never compared against the C++.** The C++ templates are
  methods on `UICoordinator` pulling values through `SystemContext`; running them
  off-device means standing up a context graph, at which point the oracle measures a
  `SystemContext` reimplementation rather than U8g2. **Their 48 committed goldens record
  what the *Rust* does.** "The Modern layout puts its temperature at y=14" is a claim
  about `docs/display-modern-layout.md`, not a measured comparison.
- **The goldens are only as good as the review that first read the C++** — a misreading
  is recorded and then protected.
- `embedded-graphics::DrawTarget` is not implemented (§12.4).
- **SH1106's 2-column transfer offset** is modelled and documented, but there is no
  driver and no I²C code.
- **The oracle builds for the host** — U8g2 compiled natively, not cross-compiled for
  Xtensa, so no claim about ESP32 codegen.

Their stated next step: a `SystemContext` stub small enough to be obviously faithful.

### 12.4 They deliberately did not implement `DrawTarget`

Their own plan called for it; they did not, on three grounds: `no_std` with **no
`alloc`** and a fixed `[u8; 1024]` page buffer; U8g2 behaviour that does not survive
being expressed as `embedded-graphics` primitives — specifically B2's coordinate wrap
and B1's balanced-width quirk, which would have to be re-derived on the far side; and
B10's 135 KB font saving. Logged as **open** in their `intentional-diffs.md`, because
"we did not need it" is not "we decided not to".

**[architecture.md §6.3](architecture.md) specifies `embedded-graphics` behind one
`DrawTarget`.** That is a live conflict. Their recommendation, which seems right: either
record that the display layer is U8g2-specific by design, or add a thin `DrawTarget`
adapter **off** the drawing path. **Decide before the templates are finished.**

### 12.5 Harness techniques

- The scenario format is implemented **twice**, once in C++ and once in Rust,
  deliberately: a parser disagreement surfaces as a failing diff rather than as two
  copies of the same mistake.
- The two number-detection rules must match **exactly** — a token is a number only if
  unquoted **and** parsing wholly as an integer, or `printf1 0 32 93.5` silently becomes
  `93` with an empty value slot.

---

## 13. Process machinery worth adopting

Their gate and skill machinery is heavier than this plan's and several pieces earn it.

### 13.1 Two normative definitions this plan lacks

- **"Parity"**: behavioural equivalence on the committed scenario set, **excluding the
  divergences listed in `intentional-diffs.md`**; a `just parity` run exits non-zero on
  any diff not explained there. This plan uses "parity" throughout with no definition
  and no exclusion register.
- **"Known-safe state"**: heater duty 0, pump off, water valve closed, steam valve
  closed, solenoid closed, all three LEDs off, **and the emergency latch NOT set** —
  i.e. `safe_hardware_shutdown`, explicitly *not* `emergency_shutdown`. "The latch must
  not be set by a routine shutdown." That prevents exactly the class of bug
  [inventory.md §5.8](inventory.md) found in the C++, where a latched emergency mode
  makes every later `enable*` silently no-op.

### 13.2 Mechanisms

- **`intentional-diffs.md` as a first-class artefact**, created early and seeded with
  the C++ bugs the port deliberately fixes, consulted whenever a parity diff appears.
  This plan has "a documented list of accepted behaviour differences" as a Phase-5 exit
  line — no file, no seeding, no creation task.
- **A running `notes.md`** with a current-state table, a verified-on-this-machine list,
  a blocked table (blocker → blocks → needs), completed tasks with measurements, **open
  questions for a human with a default-if-unanswered column**, and a numbered
  "corrections made — do not reintroduce" list. That default-if-unanswered column is
  the mechanism that keeps an agent moving without inventing a decision.
- **A defined scenario set**: YAML `[{ at_ms, kind: rest|mqtt|button|ota, method, path,
  body, expect }]` plus a capture spec, and **12 named scenarios** — `cold_boot`,
  `brew_by_time`, `brew_aborted_mid_preinfusion`, `brew_aborted_mid_flow`,
  `overtemp_trip`, `overtemp_recovery`, `water_tank_empty_mid_brew`,
  `backflush_full_cycle`, `steam_on_off`, `standby_wake`, `ota_start_from_idle`,
  **`ota_start_during_brew`** — with every safety path carrying at least one.
- **A C++ test-suite ownership map**, 33 suites → owning task, with the rule that "a
  suite with no owner is a silent regression waiting to happen" and that a
  not-ported decision must be recorded with a reason.
- **Quantified performance gates**: the reducer step **≤ 50 µs at p99** across the full
  state×event table, committed as a baseline ("do not optimise before measuring"); and a
  **24 h soak** with worst-case tick ≤ 5 ms, mean ≤ 2 ms, **zero ticks > 10 ms**, heater
  duty error ≤ 1 %, compared against a C++ histogram as a before/after table — with
  explicit permission that **"if no improvement is demonstrated, say so — a regression
  here is a finding, not a failure"**. This plan has no numeric performance target and
  no soak task.
- **A delta-based size gate at every gate**, not a flat ceiling: `just size` recorded
  with **per-crate attribution**, plus **>10 % growth over the recorded baseline fails**
  unless justified, alongside the absolute limit. Backed by `size-baseline.json` and an
  append-only `size-records.jsonl`. This plan has only "fail at 100 %, warn at 85 %",
  which catches a regression far too late. Their measured quantity is
  `espflash save-image` output — the image as laid out, padding included — and `app0` is
  parsed from the CSV at run time so the gate cannot drift from the table.
- **A per-gate checklist** requiring static RAM (`data`+`bss`) to be recorded too,
  because ADR 0002's 30 KB heap-shed threshold depends on it.
- **Feature drop order decided in advance**, so it is not litigated under pressure:
  HTTP OTA → MQTT + HA discovery → telnet log → DS18B20 → pressure → water tank
  (**flagged: dropping it removes an interlock**) → BLE. And a **never-droppable** list:
  emergency stop, the water-tank interlock, valve fail-safe, watchdog, actuator
  ownership, the 18-state machine, the heater output path — "excluded by policy, not by
  budget".

### 13.3 Skill rules with teeth

- **"Never flash an unidentified device"**, with a `list-ports` discovery step first.
  This plan's machine-enforced `_assert-chip` is stronger, but it lacks the discovery
  step and the rule that **no recipe may ever glob `/dev/cu.*`** — with two candidate
  ports on one host, that matters. Also: **stop and escalate if more than one candidate
  port appears and you cannot tell them apart.**
- **A destructive-erase recipe requires a typed `ERASE`** and is documented as wiping
  NVS including Wi-Fi credentials. This plan's pre-flash **backup** rule is the
  complement; adopt both.
- **Only one firmware is on the chip at a time** — do not try to read the sensor from
  both firmwares at once. Flash C++, record a 10-minute reference log, flash Rust,
  record the same, diff offline. The correct method for any sensor-parity comparison on
  a single-sensor board.
- **A temperature-sensor spike counts as an actuator risk** — "a temperature sensor on a
  live boiler can act on a real heater". That makes SPIKE-4 a safety task, not a
  read-only one.
- Their actuator procedure adds two things to this plan's: **never run brew, backflush
  or manual flush during a spike**, and **a person with a hand on the power switch** if
  a real boiler is genuinely required.
- **"Do not weaken a safety path to make a test pass. If parity requires removing a
  safety check, that is a bug in the plan — escalate."** A realistic agent failure mode
  this plan's skill does not name.
- **Safety effects must never be routed through a queue** — `Event → Effect → actuator`
  is a direct call in the same tick, because the C++ trips over-temperature in the same
  loop and that latency *is* the safety budget. Queues are for slow consumers outward
  only.
- **"A target is not supported because it builds."** Support requires flashed **and
  exercised**; other targets are excluded from `build-all` rather than listed as
  supported.
- **A structured commit template** with a `Validation:` block listing each command and
  its real result, an explicit **`NOT RUN: <reason>`** line, and a `Hardware:` line.
- **Validation order with `just fmt` last**, so the committed tree is formatted, and
  **stop at the first FAIL** rather than pushing through for momentum. This plan runs
  `fmt` first; theirs is better.
- **A three-tier escalation taxonomy**: *small surprise* (a version moved, a recipe
  needs a flag) → adapt, record, continue; *design-level surprise* (a sensor is
  undecodable, the image will not fit) → **stop, do not improvise an architecture**,
  update the ADR with evidence, state options, escalate; *anything touching safety* →
  stop and escalate, always.
- **Lint policy precisely**: `-D clippy::pedantic` declared in `[workspace.lints]` and
  **not also** on the command line, "because the two are not equivalent and
  double-flagging hides the real config"; an `#[allow]` must name a single lint and carry
  a why-comment; a crate-root `#![allow]` is a CI failure; a **named module-level**
  allow with justification is acceptable during the initial port. That last escape hatch
  is what stops an agent either fighting Clippy forever or nuking it.
- **Portable-crate purity enforced in CI**, plus the two ways their own enforcement was
  broken: the grep matched underscores rather than hyphens so it **could not catch a real
  dependency** (replaced with `cargo tree`), and `cargo tree … 2>/dev/null | grep -q`
  **failed open** — a failing `cargo tree` silently passed the gate. A general lesson
  about fail-open checks.
- **Anti-drift rules**: no god-function port of `LoopManager::update()`; **a fourth task
  boundary needs written justification**; no `Config` singleton — a value plus a
  `ConfigStore` trait, no global mutable state.
- **Credential rules** adding two details to this plan's: a **0600 file** as an
  alternative to stdin, and the `read -s`/bash-shebang point. Their concrete defect
  behind it: a recipe leaked an auth password into `curl`'s argv, now piped via
  `--config -`.

---

## 14. Corrections to Part 1

- **§3.2 is closed.** The `setFontPosTop` contradiction resolves in this plan's favour —
  see §12.1. The C++ does call it, at `src/ui/OledDriver.cpp:44`.
- **§5's hardware-verification claim was too strong.** It said "flashed, booted, ISR
  verified at exactly 100 Hz, heater never energised". Their own `intentional-diffs.md`
  #9 carries a "⚠ Not yet verified" note: the GPTimer build **panicked on first
  bring-up** on `gptimer_set_alarm_action` rejecting `alarm_count == reload_count`, the
  fix is **in source and not confirmed on hardware** (their two-flash budget was spent),
  and the scope-and-duty measurement against a dummy load was **not done**. What is
  confirmed: both boots reached the control loop's setup with no interrupt-watchdog
  panic. The 100 Hz figure comes from the later `aa54861` commit message; treat the
  two as separate claims.
- **Their C++ test count is 340 cases passing, from 33 suites**, verified twice
  (55.3 s cold, 22.4 s warm). [inventory.md §2.5](inventory.md) counts **41 suite
  directories and 303 `TEST`/`TEST_F` macros** and notes `.pioignore` excludes ~460
  lines. Both can be true — macros are not runtime cases and parameterised suites
  expand — but **the two numbers are not reconciled**, and neither should be quoted as
  "the" count without saying which it is.
- **Their citations drift by a few lines** against this checkout and one is simply
  wrong (`TempSensorTSIC.cpp:38-42` is the *latch*, not the range reject, which is at
  `:59`). Their content held up on every spot-check; treat line numbers as approximate.
- **`09-cpp-findings.md` has colliding section numbers** — §17–§20 each appear twice
  with different content, and cross-references from `intentional-diffs.md` are ambiguous
  as a result. **Resolve their references by content, never by number.**
- **Their RMT silence is not a negative result.** Their docs contain no RMT analysis at
  all, so §1.4's open question stands as open rather than as "they tried it and it
  failed".
