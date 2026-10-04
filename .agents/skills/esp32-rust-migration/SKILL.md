---
name: esp32-rust-migration
description: Execute the CleverCoffee C++-to-Rust firmware migration task by task, safely. Use when asked to continue, resume, or work on the ESP32 Rust migration (tasks R0-* through R4-*), port firmware features to Rust, or work on the Rust workspace under crates/cc-*. Covers per-task validation, hardware safety, parity checking against the C++ baseline, and the rules for never claiming a target is supported because it merely builds.
---

# ESP32 Rust Migration — Execution Skill

You are executing a phased, safety-critical migration of an ESP32 coffee-machine
controller from C++/Arduino to Rust. The plan already exists. **Your job is to execute
it faithfully, one task at a time, and to report honestly — not to redesign it, not to
skip it, and not to claim success you have not observed.**

---

## 0. Read this first, every session

Before touching anything, read these in order. Do not skip to the task list.

| Document | What you need from it |
| --- | --- |
| [`docs/rust-migration/01-feature-inventory.md`](../../docs/rust-migration/01-feature-inventory.md) | Every feature, its source, its hardware, and the **11 safety-critical control paths (S1-S11)**. Also §10 records what is verified vs. unknown on this hardware. |
| [`docs/rust-migration/02-research-compatibility-matrix.md`](../../docs/rust-migration/02-research-compatibility-matrix.md) | Which crates cover which feature, and the **10 unverified assumptions (U1-U10)** with their spike tasks. |
| [`docs/archive/migration/03-decision-record.md`](../../docs/archive/migration/03-decision-record.md) | ADR-0004: the platform decision, what was rejected and why, and the flash-size problem. |
| [`docs/rust-migration/04-target-architecture.md`](../../docs/rust-migration/04-target-architecture.md) | Crate boundaries, task/priority table, the single-ownership rules, and the startup/shutdown contract. |
| [`docs/archive/migration/05-tooling-and-workflows.md`](../../docs/archive/migration/05-tooling-and-workflows.md) | The `just` recipes, mise setup, and flashing rules. |
| [`docs/archive/migration/06-migration-task-list.md`](../../docs/archive/migration/06-migration-task-list.md) | **The task list, dependencies, gates, and acceptance criteria.** |
| [`docs/operations/integration-checklist.md`](../../docs/operations/integration-checklist.md) | The integration checklist to run at phase gates. |
| [`CLAUDE.md`](../../CLAUDE.md) | Repository-wide rules — including the OLED layout rules, which apply to `cc-display` too. |

Also read [`notes.md`](./notes.md) in this skill directory: it records the current state,
what has been done, and what is blocked. **Update it when you finish a task.**

### The device you are working on

| | |
| --- | --- |
| Board | ESP32-DevKitC V4, **ESP32-WROOM-32E** = the original ESP32 (Xtensa LX6, rev v3.0) |
| MAC | `ec:62:60:76:b5:3c` |
| Serial port | `/dev/cu.usbserial-224140` (WCH **CH340**, not CP2102N). **⚠ This changed on 2026-09-30** — it was `-204140` until then, and the device re-enumerates when it is unplugged, so the port is **not** stable across sessions. `just identify` is the only authority; if it disagrees with this table, the table is wrong. |
| Flash | 4 MB, DIO @ 40 MHz, no PSRAM |
| **`system.hostname`** | **`test-cc-rust`** |
| Link speed | 115200. **Unreliable above ~460800** — use `just mon`, not a raw high-rate reader. |

**The device answers to `test-cc-rust`, not `silvia`.** The C++ default is `silvia`
(`include/clevercoffee/defaults.h:14`) and the C++ firmware is unchanged and still uses
it. Both firmwares share a network during the migration, so the name distinguishes them.
The single definition is `cc_config::schema::DEFAULT_HOSTNAME` — change it there, never at
a use site, and change `docs/example_config.json` with it (an import test parses that exact
file, which is what keeps the two in step). Rationale:
[`intentional-diffs.md` §12](../../../docs/rust-migration/intentional-diffs.md).

`mqtt.password` also defaults to `"silvia"`. That is a **credential, not a name** — leave
it.

### Where the work actually is

**R4-01 has landed and was exercised on hardware on 2026-09-30.** The `cc-machine`
reducer IS wired into the 10 ms control task (`crates/cc-firmware/src/control.rs`), its
effects are applied through `cc-hal-esp32::actuators` in the same tick, and the machine
boots to `PID_NORMAL` with the PID driving the heater. There IS a state machine, a PID
and brewing on the device.

**Treat any document in this skill — including this one — as potentially stale, and check
`git log --oneline -- <file>` before acting on a status claim.** This repository has a
documented history of confidently-wrong status text that survived because it was never
checked against the tree; an independent review on 2026-10-03 caught three copies of a
claim that the control loop had never run on hardware, months after it had.
Do not assume a task is complete because its description reads as though it is; see
["Where the migration actually is"](../../../docs/rust-migration/README.md#where-the-migration-actually-is)
for the full done / not-started / deliberately-absent split.

---

## 1. The three facts that catch people out

**1. "ESP32 v4" is a board revision, not a chip.** The board is an **ESP32-DevKitC V4**
with an **ESP32-WROOM-32E** module = the **original ESP32** — Xtensa LX6 dual core, 40
GPIOs, 4 MB flash, no PSRAM. It has **WiFi + Bluetooth/BLE**, and **no USB peripheral** —
the Micro-USB port is a **CP2102N USB-to-UART bridge**, and the Boot/EN buttons give the
manual flash path. There is no S3, C3, or C6 anywhere in this project. If someone says
"just use the S3", that is a **hardware change** requiring rewiring, not a firmware
tweak. Confirmed by the user 2026-09-28.

> An earlier draft claimed the ESP32 "has no Bluetooth radio". **That was wrong** and is
> fixed. If you find that claim anywhere, it is a stale copy.

**1b. Decided 2026-09-28 — do not re-litigate:**
- Updates are a **forced full flash over USB**.
- **No NVS backward compatibility.** Rust owns its `cc.`-prefixed key namespace; config
  starts from defaults once (06 R3-08).
- **HTTP OTA is optional**; `espota` desirable if cheap. If the image gets tight, HTTP
  OTA is the first thing to drop (07 §3).
- **The React UI in `ui/` stays as-is** unless R1-05 forces WebSocket instead of SSE.
- Target is the **original ESP32**. S3/C6 out of scope unless asked.

**2. There is no ESP32 connected to this machine (as of 2026-09-28).** `ioreg -p IOUSB`
and `/dev/cu.*` show no Espressif device. Every hardware task is blocked until one is
attached. Re-verify with `just list-ports` before assuming; do not assume the other way.

**3. This is a coffee machine.** Pump, valve, and a boiler heater. A regression can burn
a house. The safety paths in 01 §6 are not nice-to-haves — they are the product. When in
doubt, err toward the safe state and stop.

---

## 2. Non-negotiable rules

1. **One task at a time.** Do not start R2-08 while R2-07 is unfinished. Do not
   opportunistically refactor something outside the current task. If you spot an
   out-of-scope problem, record it and move on.
2. **Never change the C++ firmware's behaviour.** `src/` and `include/clevercoffee/` are
   the production system and the parity baseline. You may add Rust alongside. Only
   change C++ if a task explicitly says to.
3. **Never flash an unidentified device.** Run `just list-ports`, then
   `just identify <port>`, and confirm the reported chip matches the MCU you are
   building. If the chip does not match, **stop**.
4. **Never run an actuator-energising test without a written safe test procedure.** If
   the task list has one (R1-03, R1-07, R3-04, R4-04), follow it exactly. If it does not
   and the test needs one, **write the procedure and get it reviewed before running it**.
   Never improvise a safety procedure.
5. **Never claim a target is supported because it builds.** A build is a build. Support
   means flashed and exercised. See §6.
5b. **Never modify the root `partitions_4M.csv`.** It is C++-owned until R4-10. The Rust
   table is `rust/partitions_4M.csv`.
5c. **Record the image size at every gate** — `just size`, delta attributed per crate
   (07). >10 % growth without a justification fails `just size-check`.
6. **Never commit validation you did not observe.** If a command did not run, or failed,
   or you could not run it, say so in the commit and in `notes.md`.
7. **Never put a credential anywhere it can leak.** Not in source, not in a command-line
   argument (shell history), not in a log, not in a commit, not in an unencrypted
   example file. Use stdin or a 0600 file. The Rust config layer has a `Secret<T>` wrapper
   whose `Debug`/`Display` redact — use it rather than re-implementing redaction.
8. **No blanket lint suppression.** `-D clippy::pedantic` via `[workspace.lints]` in
   `Cargo.toml` (normative — 04 §1.7); **do not** also pass it on the command line, the
   two are not equivalent. An `#[allow]` must name a single lint and carry a comment
   explaining why. A crate-root `#![allow(…)]` is a CI failure. A **named module-level**
   `#[allow(clippy::pedantic)]` with a justification is acceptable during the initial
   port.
9. **Do not weaken a safety path to make a test pass.** If parity requires removing a
   safety check, that is a bug in the plan — escalate.

---

## 3. Starting a task

1. **Check prerequisites.** Every task in 06 lists `Depends on`. If any prerequisite is
   not complete, stop.
2. **Check the gate.** Phase 1 has **Gate 1**, Phase 2 **Gate 2**, and so on. Do not
   proceed past a gate until every gate criterion is met *and recorded*.
3. **Read the whole task entry**, including `Uncertainty`. If the uncertainty has
   materialised as a surprise, that is data — record it and, if it invalidates the
   approach, **stop and escalate** rather than pushing through.
4. **Announce the task ID and its acceptance criteria** before you start.

## 4. Implementing

Follow the architecture in 04. The most common mistakes:

- **Putting a hardware dependency in a portable crate.** `cc-domain`, `cc-safety`,
  `cc-machine`, `cc-display`, and `cc-config` must never name `esp_idf_svc`,
  `esp_idf_hal`, or `esp_idf_sys`. CI greps for this; do not wait for CI to catch it.
- **Driving a relay outside `Actuators`.** Nothing but `Actuators` (pump, valve) and
  `HeaterOutput` (heater) may touch those pins. The C++ code's `heaterEnabled_` drift is
  exactly the bug this design removes.
- **Adding a water-flow state without updating `water_flow_allowed`.** In Rust this is a
  `match` with no `_` arm, so it is a **compile error** — that is intentional. Fix the
  compile error by making a deliberate decision about the whitelist.
- **Adding a task "for responsiveness".** Three task boundaries are justified in 04 §2.
  A fourth needs a written justification. Measure (R2-09) before optimising.
- **Leaking the C++ `Config` singleton pattern.** Configuration is a value plus
  `cc_config::blob_store::BlobConfigStore<B>`, whose `load`/`save`/`erase_all`
  are **inherent methods over a `BlobBackend`** — there is no `ConfigStore`
  trait, because every caller names the concrete store and nothing is generic
  over one (finding 4.6). Do not reintroduce it. No global mutable state.
- **Porting `LoopManager::update()` as an 8-step function.** That god-function reaching
  into ten-plus subsystems is the defect the migration exists to remove (04 §3.1). The
  control loop is **functional core, imperative shell**: pure
  `reduce(state, ctx, event) -> (state, Vec<Effect>)`, and one `applier.apply()` that is
  the only caller of `Actuators` / `HeaterOutput`.
- **Routing safety effects through a queue.** `Event -> Effect -> actuator` is a **direct
  call in the same tick**. Do not add a scheduling hop on the overtemp path — the C++
  firmware trips it same-loop and that latency is the safety budget. Queues are only for
  slow consumers *outward* (display, MQTT, web).

## 5. Validating, per task

Run, in this order:

```bash
just fmt          # format
just fmt-check    # must be clean
just lint         # clippy, host portable crates, -D warnings
just test         # host tests, portable crates. NOT --workspace: the device crates
                  # do not compile for a host target.
just lint-esp32   # device clippy, -D warnings
just build-esp32  # or build-esp32s3 / build-esp32c6
just size         # image size vs app slot, delta vs baseline   <-- required
```

Plus, for hardware tasks:

```bash
just identify /dev/cu.usbserial-XXXX     # BEFORE any flash
just flash     /dev/cu.usbserial-XXXX    # ⚠ overwrites the device
just mon       /dev/cu.usbserial-XXXX
```

> **`just` takes POSITIONAL arguments.** `just flash MCU=esp32 PORT=...` is `make` syntax;
> just passes the literal string `MCU=esp32` through to `espflash`. An earlier draft of
> this skill used that form throughout — do not reintroduce it. `PORT` has no default, so
> a bare `just flash` fails with a usage message rather than guessing.

And for a **phase gate**, additionally:

```bash
just size-check                         # fail if the image exceeds the budget
just parity /dev/cu.usbserial-XXXX esp32.local
# then run every section of docs/operations/integration-checklist.md in order
```

Rules:

- **Every command must actually be run.** A validation you did not run is not a
  validation.
- Stop at the **first FAIL**. Diagnose and fix before continuing; do not push through a
  failure to keep momentum.
- Run `just fmt` last so the committed tree is formatted.

## 6. Hardware rules

- **`PORT` is always explicit.** No recipe globs `/dev/cu.*`. If you do not know the port,
  run `just list-ports` and ask.
- **Confirm the chip before flashing.** `just identify` prints it. If it is not the chip
  you are building for, stop — do not "try it and see".
- **`just reflash` is destructive** (it erases NVS, which holds all config and Wi-Fi
  credentials). It requires a typed `ERASE`. Do not run it to "fix" a problem without
  understanding the consequence.
- **Auto-reset works on this board** (measured 2026-09-28: 5/5 first-try connects, no
  manual BOOT+EN needed). Do not assume a manual dance is required. If flashing fails,
  check `espflash board-info` and the cable first — **not** the firmware. The link is
  flaky above 460800 baud; stay at the default rate.
- **A target is supported only after it is flashed and exercised.** Until then, a
  successful build for ESP32-S3 or ESP32-C6 proves nothing. When you do verify one, update
  01 §1 with exactly what was tested.
- **Wi-Fi provisioning over USB is not available on this hardware** — the original ESP32
  has no USB peripheral. Do not plan around it. See 05 §5.

## 7. Committing

Only after the task's applicable validation has **passed**.

```
<task-id>: <short imperative summary>

<what changed and why, in 1-3 lines>

Validation:
  - <command> — pass/fail, actual result
  - <command> — NOT RUN: <reason>       (if applicable)

Hardware: <not required | board used, what was exercised>
```

Examples:

```
R2-05: add cc-safety with compile-time water-flow whitelist

Implements the S1-S5 safety verdicts as a pure function of telemetry,
config, and state. water_flow_allowed is a match with no wildcard arm,
so adding a water-flow state without updating the whitelist is a
compile error.

Validation:
  - just fmt-check — pass
  - just lint — pass
  - just test — pass (63 tests)
  - just lint-esp32 — pass

Hardware: not required
```

If validation failed or hardware was unavailable, **do not create a success commit.**
Either fix it and re-run, or record the blocker in `notes.md` and in the task entry in 06
and stop. A commit that says "done" when it is not is worse than no commit.

## 8. Phase gates

At the end of each phase, before starting the next one:

1. Every task in the phase has passed its own validation.
2. **Run the phase's gate checks.** For Gate 1 this is: R1-01, R1-02, R1-03, R1-07 all
   pass, and ADR-0004 moves from *Proposed* to *Accepted* with the R1-02 and R1-07 results
   filled in.
3. **Run the integration checks** in [`docs/operations/integration-checklist.md`](../../docs/operations/integration-checklist.md),
   in order, on the connected device, where applicable and safe. Record PASS/FAIL with
   actual output.
4. **Run `just parity`** and require **zero unexplained diffs**. Any diff is either a
   documented intentional change (list it in the release notes per R4-09) or a bug.
5. **Update** the task list (mark tasks complete), ADR-0004, and this skill's `notes.md`.
6. **Add any newly discovered failure mode to `docs/operations/integration-checklist.md`** in the same
   commit. The checklist must reflect reality.
7. Only then start the next phase.

## 9. Reporting results

After every task, report:

- **Task ID** and whether it is complete.
- **Validation actually run**, with the real output — not a summary of intent.
- **Unresolved limitations.** If something did not work, say so plainly. Name the
  specific unverified assumption from 02 §8 that it corresponds to, if any.
- **Blockers**, with the specific next action needed and from whom.

Never soften a failure into "mostly working". A future agent depends on this record being
accurate; an inaccurate one costs more than the original failure.

## 10. If the plan is wrong

You will find things the research could not have known. When that happens:

- **Small surprise** (a version moved, a recipe needs a flag, a pin differs on the real
  board): adapt, record what you found in `notes.md` and in the relevant document, and
  continue.
- **Design-level surprise** (TSIC-306 cannot be decoded reliably; the image does not fit
  even after a rebalance; the executor choice inverts): **stop.** Do not improvise an
  architecture. Update the decision record with the evidence, state the options, and
  escalate. ADR-0004 is explicitly conditional on its spikes for exactly this reason.
- **Anything touching safety** (S1-S11): stop and escalate, always. Even a "small"
  safety change needs a human decision.

## 11. Support files

- [`notes.md`](./notes.md) — current state, completed tasks, open blockers. Update it.
- [`../../docs/rust-migration/08-recovered-oracle.md`](../../docs/rust-migration/08-recovered-oracle.md)
  — **read before designing anything.** A complete Rust firmware previously ran on this
  board; its source is gone but the binary was recovered from flash. It contains a
  **deadman heartbeat**, **config-time cross-parameter safety validation**, a **refusal of
  `LOW_TRIGGER` heater relays**, the working **UART Wi-Fi provisioning** protocol, and a
  known-good partition layout. The plan has been revised to adopt these — do not reinvent
  them.
- [`checklists.md`](./checklists.md) — copy-paste validation checklists per phase and per
  task type.
