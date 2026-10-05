# Validation Checklists

Copy-paste checklists from
[SKILL.md](./SKILL.md) §5 and §8. Use the one that matches the task; a task may need more
than one.

---

## A. Host-only task (Phase 2)

No hardware. Run in order; stop at the first failure.

```bash
just fmt
just fmt-check                # must be clean
just lint                     # clippy, host portable crates
just test                     # portable crates only (NOT --workspace)
just lint-esp32               # device clippy, -D warnings
just size                     # image size + delta, attributed  <-- required
git diff --stat               # inspect what actually changed
```

Extra, by task:

| Task | Extra check |
| --- | --- |
| R2-05 (`cc-safety`) | Every S1-S5 branch is a named test. Adding a water-flow state without updating the whitelist **must fail to compile** — verify by hand. Safety effects are applied **in the same tick**, not routed through a queue. |
| R2-06 (`cc-config`) | Round-trip, defaults, `Secret<T>` redaction, and the `safety.emergency_temp` regression test (must fail against the old behaviour). |
| R2-08 (state machine) | All 11 ported C++ suites pass. Every energising state disables in `on_exit` **and** re-asserts in `update`. Plus: an **exhaustive `state × event` table** over the reducer; `applier.apply()` is the only function calling `Actuators`; no subsystem is reached except via an `Event` or an `Effect`. |
| R2-10 (`cc-display`) | `just snapshot-display`; zero clipping at 128×64; no row overlap; fixed-width numeric fields; bar+label vertical midline. |

---

## B. Hardware task (Phase 3 / 4)

```bash
just list-ports                                     # find the port
just identify /dev/cu.usbserial-XXXX                # confirm the CHIP matches
just build-esp32                                    # (or s3/c6)
just flash     /dev/cu.usbserial-XXXX
just mon       /dev/cu.usbserial-XXXX                # observe the boot log
just fmt && just fmt-check && just lint && just test && just lint-esp32
just size                                             # record the size delta
```

**Positional arguments only.** (Illustrating the anti-pattern: `just flash MCU=esp32 PORT=...`
is make syntax and passes
the literal string `MCU=esp32` to `espflash`.

Stop and escalate if:

- `just identify` reports a different chip than the MCU being built.
- More than one candidate port appears and you cannot tell them apart.
- Flashing fails repeatedly — check the EN↔GND capacitor / manual BOOT+RST first, **not**
  the firmware.

---

## C. ⚠ Actuator-energising task (R1-03, R1-07, R3-04, R4-04)

**Do not run any of these without a reviewed safe test procedure.** For R1-07 and R3-04
the procedure is in
[06 §R1-07](../../../docs/archive/migration/06-migration-task-list.md#r1-07--heater-output-method).
Its essentials:

1. **Physically disconnect the boiler.** Do not rely on a software switch.
2. Confirm the device is in `PidDisabledState` and the heater command is zero.
3. Use a dummy load only.
4. Never run brew, backflush, or manual flush during a spike.
5. If a real boiler is genuinely required: written procedure, reviewed first, a person
   present, hand on the power switch.

For R1-03, the same constraint applies — a temperature sensor on a live boiler can act on
a real heater.

---

## D. Phase gate

In addition to every task checklist for the phase:

```bash
# 0. Size gate — image fits, delta attributed per crate
just size
just size-check
just size-record mcu=esp32 label=gate-N

# 1. Parity against the C++ baseline — zero unexplained diffs
just parity /dev/cu.usbserial-XXXX esp32.local

# 2. Integration checklist, in order, recording PASS/FAIL with real output
#    (docs/operations/integration-checklist.md)

# 3. 24 h soak for Gate 4 (R4-05)
```

Then, before starting the next phase:

- [ ] `just size` recorded with per-crate attribution; `just size-check` passes.
- [ ] Every task in the phase passed its own validation.
- [ ] Every gate criterion in 06 is met **and recorded**.
- [ ] ADR-0004 updated (status + any spike results).
- [ ] `notes.md` updated: completed tasks, blockers, new findings.
- [ ] `docs/operations/integration-checklist.md` updated with any newly discovered failure mode.
- [ ] `01-feature-inventory.md` §1 updated if a target was actually verified on hardware.

---

## E. Final review before committing

```bash
git status                     # only intended files
git diff                       # read the whole diff
git log --oneline -5           # match the repo's commit style
```

- [ ] Commit message starts with the task ID.
- [ ] Validation commands listed with **actual** results.
- [ ] "NOT RUN" honestly stated for anything skipped, with the reason.
- [ ] No credential, serial number, or port-specific secret in the diff.
- [ ] No `.pio/`, `target/`, or generated artifact staged.
- [ ] No change to `src/` or `include/clevercoffee/` unless the task said to.
