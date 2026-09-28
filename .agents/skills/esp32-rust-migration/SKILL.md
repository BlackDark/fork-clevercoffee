---
name: esp32-rust-migration
description: Use when executing a task from docs/rust-migration/task-list.md, porting the CleverCoffee ESP32 firmware from C++ to Rust. Covers task scoping, validation commands, hardware safety, the defect register, and the commit protocol.
---

# ESP32 Rust migration

The C++ firmware is being replaced by a Rust firmware on `esp-hal` with `embassy`, targeting
the original ESP32, ESP32-S3 and ESP32-C6. Read the design before touching code.

## 1. Read first, every time

| Document | What it tells you |
| --- | --- |
| `docs/rust-migration/architecture.md` | Tasks, ownership, safety states, crates, storage, import, provisioning |
| `docs/rust-migration/decision-record.md` | Why `esp-hal` and not `esp-idf-hal`, and why we own the HTTP server and the 1-Wire driver |
| `docs/rust-migration/inventory.md` | What the C++ firmware does, and where each piece lives |
| `docs/rust-migration/defects-register.md` | Every C++ bug, its severity, and the fix the Rust port implements |
| `docs/rust-migration/task-list.md` | Your task's ID, prerequisites, files, acceptance criteria |
| `docs/rust-migration/compatibility-matrix.md` | What is device-verified, build-verified or unverified, per target |
| `docs/rust-migration/api-contract.md` | Every route, payload and status code |
| `docs/rust-migration/config-export-schema.md` | Every config field, range and default |
| `docs/rust-migration/tooling.md` | The recipes and the host requirements |

Check the task's prerequisites are all committed before starting. If a prerequisite is not
committed, stop and report.

## 2. One task at a time, inside its stated scope

- Implement only what the task lists. A neighbouring improvement goes in the report, not in the
  diff.
- A task either researches or implements. A research task produces a document; it does not
  change production code. An implementation task does not go and read upstream source.
- **Report anything contradictory, weird, oversized, buggy or unverifiable instead of
  guessing.**
- When you find a new defect, add it to `docs/rust-migration/defects-register.md` in the same
  commit, with location, impact, severity and fix.
- When you find a new problem feature (unsupported in Rust, too large for one task, risky, or
  doubtful value), add it to the problem-feature table in
  `docs/rust-migration/inventory.md` with a size estimate and options, and report it. Do not
  quietly scope it down.
- When reality contradicts the design, update `docs/rust-migration/architecture.md` and the
  task list in the same commit as the code, and say why in the commit message.

## 3. Validate

Run all of these from the repository root. `mise` and `just` must be on the host; run
`just setup` once per clone.

```
just fmt-check
just lint
just test
just check-fw esp32
just check-fw esp32s3
just check-fw esp32c6
```

A task's own acceptance criteria in the task list override the generic list. If a command fails,
fix the cause and re-run all of them. Never assume a command passed.

Then read your own diff:

```
git status --short
git diff
```

Look for: a secret, a leftover `TODO`, a `unwrap()` in firmware code, a blocking call in a
handler, an `.await` in the control path without a timeout, and any change outside the task's
stated files.

## 4. Hardware tasks and phase gates

Before flashing anything:

1. Identify the device. `espflash board-info --port <port>`.
2. Confirm the chip matches the target. `just flash` enforces this and refuses on a mismatch;
   never work around that check.
3. `just flash <target> <port>` writes the new partition table and erases old data. Say so to
   the user before you do it.

Safety rules, all of them absolute:

- **Never energize pump, valve or heater without the user's explicit approval for that specific
  test, and a written safe test procedure in the report first.**
- The default for any bench run is the `mock-actuators` feature, which removes the real GPIO
  outputs. If a test needs real hardware, it needs its own task and its own approval.
- A cold machine with no water in it must never be allowed to run a heating test.
- Provisioning and OTA both force actuators off first. Verify that before relying on it.

Put Wi-Fi on the device with `just wifi <port>`. Never write a credential by hand, never echo
one, and never put one in a log, a commit, an example file, or a file under a path that is not
gitignored.

## 5. Record what you actually validated

Put the real result in the report: which commands ran, what they printed, and what you could not
verify. A command that was not run is `unverified`, not "should work". A capability that compiles
for a target is `build-verified`, not `device-verified`.

Update `docs/rust-migration/compatibility-matrix.md` when a row's verification level changes.
Only `device-verified` counts as fully supported.

## 6. Commit and push

Commit only after validation passes. The task ID goes in the subject.

```
commit save <paths> -s "T-09: one-wire transport and DS18B20 command layer" \
  -p "<why this change was needed, and what it replaces>"
```

- `git status --short` and `git diff` first, in one shell invocation, along with
  `git diff --cached` if the index is not empty.
- If the index already holds exactly the intended commit, omit the paths.
- If the index holds something else, stop and ask.
- Then `git push`.
- If validation fails three times, or the hardware is unavailable, **make no success commit**.
  Record the blocker and the next action in the report and stop.

## 7. Phase boundaries

At the end of a phase, before starting the next one:

1. Run the full validation set from section 3 for every target.
2. Flash the bench device and run the phase's integration checks, in mock-actuator mode unless
   approval exists for a real load.
3. Update `docs/rust-migration/compatibility-matrix.md`.
4. Report the phase result, including everything unverified.

`docs/integration-tests.md` holds the manual checklist; extend it whenever a new failure mode is
discovered, in the same commit that discovered it.

## 8. Secrets

- `.env` holds `WIFI_SSID` and `WIFI_PASS`. It is gitignored. Read it only inside a recipe or a
  script.
- A secret never appears in terminal output, a log, a commit message, a doc, an example file,
  or a file outside a gitignored path.
- An intermediate artifact that contains a secret lives in a gitignored temp path and is deleted
  immediately after use.
- `just wifi` and `just config-import` report a status only. If you find yourself wanting to
  print a value to debug something, stop: add a non-secret diagnostic instead.

## 9. Storage and OTA

Designed for the new system alone. The Rust firmware owes no compatibility to the C++ firmware
for flash layout, NVS keys, storage format, OTA scheme or bootloader.

The only migration path is the config JSON import. Users export from the old UI, flash over USB,
import. Say exactly that in the user guide and nowhere else, because it is the whole procedure.
