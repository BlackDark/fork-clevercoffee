# Common design — the C++ → Rust migration, as three agents independently understood it

Sources: `origin/feat/rust-migration-design`, `origin/refactor/space2`, `origin/rewrite/rust`.
Per-branch digests in `digests/`. No branch was checked out; the docs were exported with `git archive`.

> **This document describes what the three branches *wrote*.** For what they then *built and ran*,
> read `implementation-evidence.md` — it is the more trustworthy of the two, and it corrects several
> claims below. In particular: only `rewrite/rust` has ever compiled for the device or run on it
> (`design` is 51 lines of crate skeletons; `space2`'s 30 201 lines have never seen a compiler), and
> the build found problems **no document predicted** — including two independent bugs that meant
> the machine could not heat at all.

Three agents, ~1.3 MB of design documents, zero shared context. This document records only what
**at least two of them concluded independently**, and flags the places where they did not agree.

---

## 0. Verdict in one paragraph

The three efforts converged far more than they diverged. All three found the same blocking C++
defects, the same hardware landmines, the same migration path, and the same verification standard.
They disagree on **one** decision that matters — the platform stack — and on about six smaller
ones. Where they converge, the finding is very likely true. Where they diverge, the disagreement is
usually *scope*, not *understanding*: each branch was solving a slightly different problem
(original-ESP32-only port vs three-chip support; bug-for-bug parity vs fix-by-construction).

---

## 1. What all three agree is the target

### Hardware reality (unanimous, independently verified)

- The device is an **original ESP32 rev 3.0, Xtensa LX6, dual core, 4 MB flash, no PSRAM**, on an
  AZ-Delivery devkit. **"ESP32 v4" in the project means the PCB revision, not a chip variant.**
  There is no S3, C3 or C6 in the repo or in its 1875-commit history. *(all three)*
- The original ESP32 has **no USB peripheral** — the cable is a **WCH CH340** bridge on UART0.
  The PlatformIO env name `esp32_usb` is a misnomer. *(all three; two had it wrong first —
  "CP2102N" — and corrected in place)*
- Xtensa has no upstream stable Rust. `espup` + a forked `esp` rustc + `-Zbuild-std` is a permanent
  condition, not a transitional one. `mise` must not manage `rust` — its `cargo` shim shadows
  rustup and silently breaks the toolchain file. *(design, rewrite)*
- `GPIO2` (heater relay) is a **boot strapping pin**; `GPIO1` (steam LED) is **UART0 TX**. The
  comment says the LED "was moved"; it was not. *(design, space2)*

### Architecture (unanimous in shape, differing in crate count)

All three converge on the same skeleton:

```
pure domain (no_std, host-testable)   ← state machine, PID, config schema, safety policy
        ↓ traits
HAL seam (trait-only, no vendor types) ← DigitalOut, Clock, TempSource, KvStore, HeaterDuty …
        ↓ one crate
board crate (the only one that imports vendor crates)
        ↓
firmware binary: N tasks + one ISR
```

Shared commitments:

- **The domain is pure and host-testable.** All three split the workspace so that the state machine,
  PID, config schema and interlocks compile and test with no ESP toolchain. `just test` must stay fast.
- **The HAL seam is traits only**, so the platform decision stays reversible — switching means
  writing a second adapter crate, not rewriting the firmware. *(design, stated explicitly as the
  reason the seam exists)*
- **The state machine moves to the domain** as 18 explicit states with explicit discriminants; the
  numeric gaps and range predicates (`isBrewState` 31..34, `isBackflushState` 60..63) are
  **load-bearing** and get replaced by explicit sets, asserted against the ranges.
- **Safety is structural, not documentary.** Exhaustive `match` with no wildcard arm, so a 19th
  state is a compile error; the emergency latch is a precondition *inside* the actuator methods
  rather than a call-site convention. *(rewrite, space2; design does it via an interlock argument
  on the only `HeaterCommand` constructor)*
- **No backward compatibility with NVS.** All three drop FNV-1a key compatibility. The only bridge
  between old and new firmware is the `config.json` export/import round-trip.
- **The C++ tree stays, buildable, and is the oracle** for the whole migration. PlatformIO survives
  as the rollback path. *(rewrite, space2 explicitly; design keeps it as a CI-enforced baseline)*

### Concurrency (unanimous)

- **FreeRTOS tasks, not one loop.** Small task count: 5 (design), 3 (rewrite), 4-ish (space2).
  Each task exists for a stated timing reason; the `storage`/`housekeeping` task exists so a config
  write never happens on an HTTP handler — a thing the C++ actually does.
- **Safety effects never traverse a queue.** Interlock evaluation and the actuator write happen in
  the same tick by direct call. Sensor→Event and Event→Effect are direct calls, deliberately.
- **Queues carry only `Copy` payloads.** `hal::task::queue::Queue<T>` requires `T: Copy`, and
  `heapless::Deque` has no interior mutability so it **cannot** be a cross-task channel at all.
  No `String`, `Vec` or `Box` in a message. *(design and rewrite both hit this and corrected an
  earlier draft that specified a deque and forbade locking — a self-contradiction)*
- **`embassy-executor` is disqualified on `esp-idf-hal`** because its `critical-section` impl is a
  FreeRTOS recursive mutex, not ISR-safe. *(design, rewrite — the same finding, same reason)*

### Verification standard (unanimous, and the strongest convergence)

- **A capability that compiles is not a capability that works.** All three adopt an explicit
  evidence ladder: `repo-verified` / `build-verified` / `device-verified` / `needs confirmation`.
  Raising a level requires the named evidence, in the same commit as the claim. Only
  `device-verified` counts as supported.
- **Behavioural parity is proven against the real C++, not against prose.** The strongest versions:
  compile the **unmodified `PID_v1.cpp`** and replay identical sequences (rewrite); link the
  **actual U8g2 tree the firmware links** and require bit-identical framebuffers (rewrite).
- **Parity needs an explicit divergence register**, not the word "parity" alone. `rewrite` ships
  `intentional-diffs.md` with machine-readable `ledger` blocks and a runner that exits non-zero on
  any diff the ledger does not explain. `design` flags its own *absence* of such a register as a
  gap. `space2` ships a `parity-report.md` row per behaviour: `pass`/`fixed`/`deviation`/`gap`/
  `unverified`, each naming the test that is the evidence.
- **Fix C++ defects, do not replicate them** — all three, though under different rules. `space2`
  bounds parity scope to *functional, control and API behaviour only*, exempting defect fixes.
  `rewrite` lists 11 safety paths S1–S11 and fixes the 5 that are broken. `design` declares each
  behaviour change in the commit message and the task list, never smuggled in.
- **The rule that catches a whole class of lie:** a test that asserts the *wrong direction* hides a
  bug. `space2`'s interlock test originally asserted *equality* between the states that command the
  valve and the states allowed to hold it — precisely the assertion that concealed a missing state
  (D56). The fixed test asserts only the safety direction: a state that commands the valve must be
  allowed to hold it.

---

## 2. Where they diverge

Ordered by how much it matters.

### 2.1 The platform stack — a real 2–1 split 🔴

| Branch | Platform | IDF | Measured image |
|---|---|---|---|
| `design` | `esp-idf-svc` / `esp-idf-hal`, target `xtensa-esp32-espidf` | **v5.3.6** | 997,648 B = 58.5 % of the 1.625 MB slot |
| `rewrite` | `esp-idf-svc` / `esp-idf-hal`, target `xtensa-esp32-espidf` | **v5.5.5** | needs a partition rebalance; ~1.5–2.5 MB typical |
| `space2` | **`esp-hal` 1.2.2 bare metal + `embassy`** | — | 99,728 B app image |

The reasoning genuinely differs, not just the conclusion:

- **`design` and `rewrite` chose IDF** because it is the only stack that covers HTTP, OTA, NVS,
  filesystem and TCP/IP, and because the device is *proven* on that Wi-Fi stack. `design` then
  **closed the flash question by measurement** rather than argument (SPIKE-1 linked every needed
  subsystem at 58.5 % of the slot), which removed the gating risk it had originally identified.
- **`space2` chose bare metal** on a *verification* argument: `esp-hal`'s per-chip support is
  machine-enforced in CI with per-chip HIL runners, whereas `esp-idf-svc` self-reports "missing HIL
  tests" and lag behind stable IDF. For a control loop whose heater runs from an ISR, an auditable
  single-executor model and hardware-tested dependencies were worth more than free HTTP and OTA.
  The cost: it must **own the HTTP server, the DS18B20/1-Wire driver, the config store and the
  provisioning transport** — roughly 600–900 lines of glue per subsystem.

Note the disagreement is *not* about `embassy`: `design` and `rewrite` rejected `embassy-executor`
for the same reason `space2` adopted it. `space2` runs embassy on `esp-hal`, where the
`critical-section` impl is a real ISR-safe primitive; on `esp-idf-hal` it is a FreeRTOS recursive
mutex and is disqualified.

**This is the one decision that must be made deliberately.** Everything downstream — image size,
whether the port fits in the existing partition table, who owns HTTP, whether TSIC-306 is even
capturable — follows from it.

### 2.2 Target scope

`design` and `rewrite` port **the original ESP32 only**. `space2` targets **ESP32 + S3 + C6** and
found that the C6 **does not have enough pins** (23 exposed → 14 usable vs 17 needed), forcing a
user-approved feature reduction (3 indicator LEDs, 2nd HX711 cell). That finding is real and
portable knowledge regardless of which stack wins, as is the S3 **v1.0 vs v1.1** RGB-LED pin
difference (GPIO48 vs GPIO38).

### 2.3 Heater output

- `design` + `rewrite`: 10 ms **GPTimer ISR** software chopper (parity), with `LedcPwm` retained
  but unbrought-up.
- `space2`: **hardware PWM (MCPWM)**, no interrupt at all, with `Stage::HeldOff` holding the heater
  *off* rather than driving the pin — on the reasoning that a machine that does not heat is noticed
  and a machine that ignores its PID is not.

This is a genuine architectural disagreement about whether the port should reproduce the ISR
mechanism or replace it. `space2` found the reason to care: `setHeaterDuty` was a **no-op in the
C++** — every heater duty was 100 %, and only the state machine's actuator command drove the pin.

### 2.4 Storage, OTA, assets

| Concern | `design` | `rewrite` | `space2` |
|---|---|---|---|
| Config | `postcard` blob + `ver` u16 in NVS | one nested **JSON** blob (~2.1 KB) in NVS | custom **versioned CRC binary** region, 2 × 32 KB A/B |
| OTA | dual slots kept, **new→new yes, old→new prohibited** and enforced by a boot layout guard | forced **full USB flash** is primary; HTTP OTA is first to drop | **all four paths kept and corrected**, each gated on "machine idle → force actuators off" |
| Web assets | LittleFS `littlefs` partition | `include_bytes!` — removes `buildfs` entirely | not applicable (own display path) |
| Watchdog | moved **into** the control task, single subscriber | moved **into** the control task, idle tasks unsubscribed | separate `safety` task feeding every 50 ms |

**Settled by measurement since this table was written** (`implementation-evidence.md` §4): the
heater mechanism is the **10 ms GPTimer ISR**, not LEDC and not hardware PWM — LEDC panicked on
every boot and hardware PWM was never reached. The display draw target is an **own framebuffer**,
not `embedded-graphics`, because the fonts cost 135 KB more as `ImageRaw`. The config round trip
**works** on `rewrite/rust` (2,077 B serialised against the lost oracle's logged 2,071 B), while
`space2` found it was the single thing most likely to break.

### 2.5 Smaller divergences

- **Display**: `design` specifies `embedded-graphics` behind one `DrawTarget` but *its own
  cross-reference flags this as a live conflict*; `rewrite` rejects it (fonts cost 42,722 B as raw
  RLE vs 177,723 B as `ImageRaw` — a 135 KB penalty on a size-constrained target) and uses an
  own framebuffer with U8g2-compatible calls; `space2` ships new font tables at two integer scales
  and admits this makes its own layout tests self-referential.
- **Scales**: `design` and `space2` **defer** HX711 and Acaia BLE as dead code; `rewrite` **keeps
  and fixes** them, because the human owner declared the deadness their bug. Consequence worth
  noting: `rewrite` therefore has **no C++ parity baseline for the scale stack by construction**.
- **Embedders rejected for a reason nobody else states**: `esp-wifi-provisioning` forces
  `esp-idf-hal/rmt-legacy` non-optionally, and cargo feature unification switches it on for the
  whole graph — silently **removing** `hal::onewire` and the GPTimer module. Check with
  `cargo tree -e features` **before** adopting. *(design, rewrite)*

---

## 3. The migration path is the same everywhere

All three converge, independently, on this and it is the most important operational conclusion:

> **The only bridge from the old firmware to the new one is: export `config.json` from the old
> UI → flash over USB → import into the new firmware.**

Everything else is severed: NVS keys, partition layout, LittleFS assets, OTA. All three also agree
that this single path is where the migration will actually fail, and each found a distinct way it
breaks (see `common-pitfalls.md` §2).

Phase shape is also shared, with the same load-bearing ordering:

1. **Pure domain logic, no hardware** — the state machine, PID, config schema, safety policy.
   This is where the majority of the behavioural surface and the safety paths get proven.
2. **Feasibility spikes** — the tasks that can invalidate the design run *before* the design is
   built on, and a failed spike updates the matrix and the task list before Phase 3 starts.
3. **Board and drivers**, then **application**, then **parity**, then **cutover**.

The universally identified critical-path risks:

- **`ORACLE-1` must precede every PID task** — the host-test stub has `P_ON_M`/`P_ON_E` inverted
  relative to the firmware, so until it is fixed any golden vector certifies the wrong branch.
  *(design)*
- **The boot layout guard must precede every on-device task** — a half-migrated device boots and
  runs. *(design)*
- **The parity-baseline capture must precede the gate that requires it** — `rewrite` put baseline
  capture in Phase 1 while the runner was scheduled for Phase 4, making Gates 1–3 *un-passable as
  written*. A planning defect only visible by walking the dependency graph.
- **The control reducer is the critical path and has never run on hardware** in `rewrite` — the
  control task is a hand-rolled heuristic that acknowledges web commands and drops them, so at that
  point there is no state machine, no PID and no brewing on the device.
- **The relay-polarity question is a hardware fact, not a design decision**, and it gates the
  heater PWM, which gates the whole application layer. All three flagged it; `design` escalated it
  as BLOCKED and "must not be implemented as written".

---

## 4. What each branch found that the others did not

Worth carrying forward regardless of which plan wins.

**`design`**
- The **boot-time flash-layout guard (`BOOT-1`)** as the enforcement mechanism for "no backward
  compatibility" — absent from both other branches. The argument is stronger for a bigger-slot
  plan: an app-only OTA from a C++ device would boot the new firmware against the *old* table,
  with the heater still wired.
- A **recovered "oracle" firmware** — a previous Rust build for this exact board, source gone,
  recovered from a 4 MB flash dump and a boot log. Unique recovered rules: deadman heartbeat
  latch, output held off until the first supervisor beat, 500 ms interlock re-assert, the
  `LOW_TRIGGER` refusal, fail-closed config handling, an 18-route fault-injection debug surface, a
  working UART `wifi set` protocol, `/download/coredump`. *(`rewrite` recovered the same firmware
  independently and adopted the same rules — strong corroboration.)*
- An **ADR amendment trail**: ADR 0004 openly records that ADR 0005 *removed its strongest argument*
  and weakened its second, rather than leaving a stale record.
- An `fp`-in-ISR finding: a floating-point instruction anywhere in a level-1 ISR **panics** the
  original ESP32, and the C++ is safe only by accident (GCC soft-floats the same source the `esp`
  target compiles to hardware FP). *(rewrite found this too, independently)*

**`space2`**
- Two defects that would have **destroyed the migration path itself**, found by porting rather than
  review: the exporter wrote dotted keys as leaf names, so **every export the port produced was
  refused by its own importer**; and `format_version` was rejected outright, so **every real C++
  export — the only file a migrating user has — was refused**. Both survived two review passes
  because the repo's own `config.json` happens to lack the key.
- The **C6 pin-budget derivation** and the S3 v1.0/v1.1 LED difference.
- A **secret-handling doctrine built from property, not memory**: secrets are marked *in the
  schema* and the log sink redacts them by construction; `just flash` requires the operator to type
  `FLASH`.
- `esp-wifi-provisioning` and `esp-bootloader-esp-idf` are the two crates most likely to break the
  feature graph or the build.

**`rewrite`**
- **Static RAM, not flash, is the binding constraint**: 131,688 B = 41 % of 320 KB, roughly double
  the pre-network figure, and ADR-0002's 30 KB heap-shed threshold was tuned against ~75 KB. 94 KB
  of it is `.iram0.text` dominated by the prebuilt Wi-Fi MAC, which **no Rust-side change touches**.
- `std` pulls in **210 KB of backtrace symbolisation**, useless on a device with no symbol table.
  Removed with one line for **−162,656 B (−12.05 %)**. The RAM win was 1,440 B, not the 10 %
  implied — a template for a "measured rather than assumed" outcome.
- **mbedTLS (~94 KB flash, ~8 KB RAM) is not removable and is not our code** — it comes in via
  MQTT → TLS and WPA3/Enterprise. Cutting it is a *product* decision, not a size optimisation.
- **The Rust control tick already overruns its 10 ms budget in ~62 % of ticks** (worst 3.2×), and
  it is **not** the scale.
- **A timing instrument that has never disagreed with a result is not known to be working** — the
  first tick-cost probe took its timestamp *after* the delay and reported 431 ms for a 400 ms
  period. It would have hidden the real overrun, and a criterion passed *because* of it.
- **Three plan-level errors found by adversarial review, all stale counts**, and one outright false
  belief — "the original ESP32 has no Bluetooth radio" — **which is what made "drop the scales"
  look safe.**
- A skill written for **agent execution** rather than humans, listing the anti-patterns agents
  actually produce: `KEY=value` reaching `espflash` argv, a `grep -q` gate that **fails open**, a
  recipe that leaked the auth password into curl's world-readable argv, a CI job with no `env:`
  block silently using the default ESP-IDF.

---

## 5. A note on the three documents' own reliability

All three branches carry **self-contradictions in their own planning documents**, and in each case
the same class of error:

- **Stale counts after a late correction.** `design`: architecture says the watchdog has one
  subscriber, task-list still says two. `space2`: 96 vs 99 vs 100 parameters; sixteen vs seventeen
  crates; nineteen vs eighteen states. `rewrite`: 18 states "not 19", 96 params "not 108".
- **Superseded decision text left in place under a strike-through** rather than rewritten.
- **A conclusion and its own refutation living in different files**, with no cross-reference.
- **`rewrite` has two §17s, two §18s, two §19s and two §20s** in its largest findings document — it
  was appended to rather than revised, and the first versions are factually wrong with in-file
  retractions.

The lesson generalises: **append-then-retract does not work in a planning corpus.** Corrections must
rewrite the section in place, and any count asserted in prose needs an owner and a test.

---

## See also

- `common-pitfalls.md` — the shared defect register and the traps, cross-referenced by branch.
- `digests/design.md`, `digests/space2.md`, `digests/rewrite.md` — the per-branch digests, with
  file-level citations for every claim above.
