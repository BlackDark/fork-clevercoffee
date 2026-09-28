---
name: esp32-rust-migration
description: Execute the C++ to Rust firmware migration for the CleverCoffee ESP32 espresso machine, one task at a time from docs/rust-migration/task-list.md. Use when implementing, validating or committing any task in that list, or when touching crates/, firmware/, the justfile or the Rust CI workflow.
---

# ESP32 C++ to Rust migration

You are continuing a migration that is already planned. The plan is not yours to
re-derive. Your job is to execute one task from it, validate the result honestly,
and record what actually happened.

This firmware drives a **heating element, a pump and a solenoid valve** on a live
espresso machine. Unexpected actuation can scald a person or start a fire. Every
rule below about actuators is there for that reason.

## 1. Read before doing anything

In this order. Do not skip ahead to code.

1. `docs/adr/0004-rust-migration-platform-selection.md` — the platform decision,
   what it accepts, and the list of **deliberate behaviour changes**.
2. `docs/rust-migration/architecture.md` — the execution model, component
   boundaries, hardware ownership, crate split and safe-startup order.
3. `docs/rust-migration/inventory.md` — what the C++ firmware actually does, and
   §7, which is the list of contradictions and latent bugs you must not
   accidentally "fix" or accidentally reproduce.
4. `docs/rust-migration/compatibility-matrix.md` — per-capability verdicts, and §5,
   everything still unverified.
5. `docs/rust-migration/tooling.md` — how to build, test, flash and provision.
6. `docs/rust-migration/task-list.md` — find your task and **read its
   prerequisites and dependencies**. Confirm every prerequisite is actually done,
   not merely listed.

Also read, for any task touching the state machine or actuators:
`docs/adr/0003-state-machine-hardware-control-contract.md` and
`docs/state-machine-architecture.md`. ADR 0003 lists four regressions a previous
refactor introduced; they are the traps for this one.

## 2. Take one task

One task, within its stated scope. Concretely:

- Change only the files the task names. If you need to touch another, say why in
  the commit message; if it is a different concern, file a separate task.
- A task is either **research** or **implementation**, never both. If an
  implementation task turns out to need research, **stop, report, and propose the
  research task.** Do not investigate your way through it and then implement on the
  same commit.
- Do not "improve" adjacent code, comments or formatting. Do not refactor things
  that are not broken. Match the existing style.
- Do not add features, abstractions, configurability or error handling the task did
  not ask for.

## 3. Implement, then validate

Run these, in this order, and **read the output**:

```bash
just fmt          # format
just lint         # clippy, warnings denied, host crates AND every target
just test         # host tests
just build esp32  # target build
just size esp32   # app image vs the app partition
```

Then **review your own diff** with `git diff` before you even think about
committing. Look for: debug leftovers, `todo!()`/`unimplemented!()`, commented-out
code, anything you touched that the task did not name.

Rules that are not negotiable:

- **No blanket `allow`s.** If Clippy fires, fix it, or annotate the specific line
  with a reason. Never add a crate-level `#![allow(...)]`.
- Host tests are the main correctness surface. All control logic lives in
  `cc-domain`, which has no platform dependency — **if your logic cannot be tested
  on the host, it is in the wrong crate.**
- If `just size` reports above 85 %, stop and report. The C++ build sits at 90.4 %
  and that was only discovered by measuring.

## 4. Hardware tasks and phase gates

### Always identify the device before flashing

```bash
just board-info <port>     # read-only
```

`just flash <target> <port>` refuses to run unless the connected chip matches the
target. **Do not work around that guard.** If it refuses, the answer is to find out
why, not to bypass it.

Check `docs/rust-migration/task-list.md` prerequisite **P0** first: as of the last
update, the attached device is not in a re-flashable state without a user decision.
If P0 is still open, hardware tasks are blocked — say so and pick a `HW: no` task.

### Energizing an actuator

**You may not energise the heater, pump or valve unless the task contains a written
safe test procedure and you follow it.** If a task needs actuation and has no such
procedure, write one, get it approved, and only then proceed.

A safe procedure states, at minimum:
- which actuator is involved and **how it is physically isolated** (element
  disconnected, relay board unpowered — verified, not assumed);
- how the result is observed without energising the load (measure at the relay input
  pin, or use an LED);
- the exact sequence of commanded values, starting and ending at off;
- how power is cut if it misbehaves.

Before any flash of a device wired to a real machine, take a backup:

```bash
espflash read-flash --port <port> 0x8000 0x1000 backup-ptable.bin
espflash read-flash --port <port> 0x9000 0x5000 backup-nvs.bin
espflash read-flash --port <port> 0xe000 0x2000 backup-otadata.bin
```

### At phase boundaries

Run the phase's exit gate from the task list: build, host tests, the integration
checks, and a flash-and-test on the device **where it is safe to do so**. If the
device is unavailable or the test would be unsafe, record that and do not claim the
gate passed.

## 5. Record what actually happened

In the commit message and, for research tasks, in the relevant doc:

- The **exact commands** you ran and their real results. Not "tests pass" — the
  count, and the size figure.
- What you could **not** verify, and why. Compile-level success is not runtime
  success; say which one you have.
- Any **limitation** you hit.
- Every **deliberate behaviour change**, stated plainly. ADR 0004 lists the ones
  already agreed; if you make a new one, it must be declared, not smuggled in. If it
  is a user-facing behaviour change that was not pre-agreed, **stop and ask.**

## 6. Commit rules

- **Commit only after validation passes.** No exceptions.
- The **task ID goes in the commit subject**: `DOMAIN-2: make heater interlocks
  structural`.
- Body: what changed, the validation results, and any declared behaviour change.
- Never add yourself as co-author.
- Do not commit to `main` (pre-commit blocks it, and CI targets `main` only).
- If validation **fails**, or hardware is unavailable, or a spike fails with no
  viable alternative: **record the blocker and the next action, and make no success
  commit.** A commit that says a task is done when it is not is worse than no commit.

## 7. Secrets

- `.env` holds `WIFI_SSID` and `WIFI_PASS`. Load them only from inside tooling
  (`just provision` does this). Never pass a credential as a command-line argument —
  argv is visible to every process via `ps`.
- No credential in terminal output, logs, commits, example files or shell history.
- `scripts/nvs_inspect.py` prints key names, types and sizes only, never values.
  Keep it that way. Use `--digest` to compare values without seeing them.
- If you dump flash, treat the dump as secret: NVS contains Wi-Fi credentials.
  Write dumps under the gitignored `research/`, never into the repo proper.

## 8. When findings contradict the plan

This will happen. The plan was built from evidence but not all of it is verified.

**Update the documents first, then continue.** In order:
1. Record the finding where it belongs — `compatibility-matrix.md` §5 for an
   unverified capability, `inventory.md` §7 for a C++ behaviour, ADR 0004 for a
   platform consequence.
2. Update `task-list.md` — add, re-scope or reorder tasks.
3. Only then write code against the corrected plan.

Do not paper over a contradiction. If something looks weird, wrong, or impossible
to verify, **report it instead of guessing.** The inventory and matrix are full of
such reports; that is the standard, not an exception.

## 9. Stop and ask when

- Prerequisite P0 is unresolved and your task needs hardware.
- A task would energise an actuator without an approved procedure.
- A behaviour change is user-facing and was not pre-agreed — for example the missing
  manual-brew, steam and manual-flush timeouts, the water-tank switch polarity, or
  whether a scale fault should keep taking the heater down.
- A spike fails and the alternatives all deviate from what was agreed.
- The same approach has failed twice. Diagnose the root cause and change approach
  rather than tweaking.
- You are about to do something hard to reverse: erasing a partition, burning an
  efuse, force-pushing, or flashing a device wired to a live machine.

## 10. Quick reference

```bash
just doctor                    # toolchain sanity; catches the mise/rustup trap
just test                      # host tests — the fast loop
just build esp32               # target build
just size esp32                # image vs partition
just board-info <port>         # what is connected (read-only)
just flash esp32 <port>        # chip-guarded
just monitor <port>
just nvs-report <port>         # device config, values never printed
just provision-check <port>    # build a provisioning image, write nothing
just cpp-build / just cpp-test  # the parity oracle — keep it green
```

The C++ firmware stays in the tree and buildable for the whole migration. **It is
the oracle.** When you need to know what the firmware does, measure it against the
C++ rather than trusting a document — including these ones.
