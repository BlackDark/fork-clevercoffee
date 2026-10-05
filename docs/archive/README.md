# Archive — preserved, not a source of truth

**Everything under `docs/archive/` is preserved history and nothing else.** It is
not deleted, so a claim can still be traced to the commit or the review that
produced it, but **no file in here states anything true today.** Each carries a
one-paragraph banner saying so; this page is what makes that banner a rule rather
than a convention.

**Any claim quoted out of `archive/` into a live document must be re-verified
against live evidence** — the code, a test, a measurement, or `docs/status.md` —
and only then written down. The archive citation is recorded as *where the claim
came from*, never as its proof. A number lifted from here is a claim until
something current re-establishes it.

## Why this rule exists, in a claim that really happened

This repository has already produced a confidently-wrong, confidently-argued claim
that was later retracted. A September 2026 hardware survey asserted that **the
ESP32 has no Bluetooth radio** and treated it as a constraint on the port's
design. It is false: the original ESP32 does have a Bluetooth radio, and the
port's own
[compatibility matrix](../rust-migration/02-research-compatibility-matrix.md) is
where the correction lives. The claim survived as long as it did because it was
written down confidently, in prose, in a document that read like a reference.

That is exactly the failure mode an archive creates: 11,000 lines of
well-written, dated, self-consistent prose that a reader cannot tell from a
page written last week. **The banner reduces the odds; it does not eliminate
them.** The re-verification clause is what eliminates them.

## What is here, and why each item is

### `migration/` — the Rust migration's own planning documents

Dated 2026-09 to 2026-10, from the C++→Rust port. Each was superseded while the
port was being built; the work they planned is recorded in
[`docs/status.md`](../status.md) and in
[`33-post-review-plan.md`](migration/33-post-review-plan.md) instead.

| File | What it was | Why it is here |
| --- | --- | --- |
| [`03-decision-record.md`](migration/03-decision-record.md) | ADR-0004: the platform and concurrency decision (`esp-idf` vs bare metal), what was rejected, and what would make it wrong. | The decision is done and shipped. Its **consequences** are still binding and live in `AGENTS.md` (`AG-RUST-*`); the reasoning that produced them is history. `docs/adr/0003` is the part of it that is still an active contract. |
| [`05-tooling-and-workflows.md`](migration/05-tooling-and-workflows.md) | The `mise` setup, the `just` recipes, the flashing rules, Wi-Fi provisioning, CI. | Superseded by the tooling itself: the `justfile`, `just/size.just`, `mise.toml` and `.github/workflows/rust.yml` are now the specification, and [`docs/handbook/ci.md`](../handbook/ci.md) documents the CI. The `justfile` header still records the three deviations it forced. |
| [`06-migration-task-list.md`](migration/06-migration-task-list.md) | The R0–R4 phased task list, with dependencies, gates and acceptance criteria. | Every task is now either done, superseded, or explicitly not done — and the not-done list is [`docs/status.md`](../status.md), which is dated and owned. A task list that is 90 % complete is worse than no task list: it reads as current. |
| [`07-image-size-budget.md`](migration/07-image-size-budget.md) | The 154 KiB headroom problem, the partition-rebalance arithmetic, the drop order, the per-gate size report. | The **numbers in it are measured 2026-09-28 and have moved.** The size gate that enforces the limit is `just/size.just`, reading `size-baseline.json` and appending to `size-records.jsonl` — both live, both still in `docs/rust-migration/`. |
| [`31-findings-2026-10-01.md`](migration/31-findings-2026-10-01.md) | The findings index for the 2026-10-01 session, with a status per item. | Superseded by [`32-findings-2026-10-03.md`](../rust-migration/32-findings-2026-10-03.md) and then by the fixes. Kept because it records *when* each finding was made, which is the provenance a reviewer needs. |
| [`33-post-review-plan.md`](migration/33-post-review-plan.md) | The master tracker for the work out of the 2026-10-03 independent review. | Same reason. Every item's fate is visible in `git log`; the plan document itself is a point-in-time artefact. |

### `cpp/` — documents that describe the **C++ firmware only**

Superseded: these describe the **C++ firmware**, which was deleted when the Rust
port became the product. They are archived because the source they describe is
only reachable in git history now, and its behaviour is recorded in
[`../handbook/pins.md`](../handbook/pins.md) and
[`../rust-migration/09-cpp-findings.md`](../rust-migration/09-cpp-findings.md).

| File | What it is | Superseded for the Rust port? |
| --- | --- | --- |
| [`REPOSITORY_SUMMARY.md`](cpp/REPOSITORY_SUMMARY.md) | The C++ tree's layout, its coding standards, its build and test commands. `AGENTS.md` `AG-REPO-15` cites it and says plainly that it describes the C++. | **Yes, entirely.** The tree it describes no longer exists. Cite it only for what the C++ did. |
| [`wokwi.md`](cpp/wokwi.md) | Running the **C++** firmware in the Wokwi simulator (`pio run -e esp32_usb -t wokwi`, `wokwi.toml`, `diagram.json`). | **Yes, entirely.** The Rust port has no Wokwi simulator recipe; `just` has no equivalent target. |
| [`backflush-reminder.md`](cpp/backflush-reminder.md) | The C++ `MaintenanceCoordinator` backflush shot counter: its NVS key, its OLED/web surfacing, its threshold semantics. | **In mechanism only.** The counter, the qualification rule and the threshold **are** ported (`cc_machine::maintenance`, `cc_hal_esp32::nvs`) — but there is no `MaintenanceCoordinator`, and the port persists under its own NVS key `cc.maint.shots`, not the C++'s `maintenance`/`shots_since_bf`. Every C++ type and path below is the C++'s. |
| [`state-machine-architecture.md`](cpp/state-machine-architecture.md) | The C++ state machine in full: the state diagram, the per-state pump/valve/heater table, the flag lifecycle, the safety layers. | **Yes as a description of the port.** Its *rules* are live and normative in [`docs/adr/0003`](../adr/0003-state-machine-hardware-control-contract.md) and in `AGENTS.md` `AG-REPO-21` … `AG-REPO-26` — that is what this file was archived for, and it is why archiving it loses nothing. The Rust states are documented where they live, in `crates/cc-machine/src/states.rs`. |

## What is deliberately NOT in the archive

- **[`docs/status.md`](../status.md)** — the only page permitted to claim what
  works. Dated, owned, and updated in the same commit as any behaviour change
  (`AG-REPO-4`, `AG-REPO-20`).
- **[`docs/rust-migration/`](../rust-migration/)** — deliberately left in place.
  Rust doc comments link into it with rustdoc link syntax and `just lint` runs
  `rustdoc -D warnings`, so moving any of those documents is a **build break**,
  not a link cleanup. That includes `intentional-diffs.md`, which `cc-parity`
  also *reads* as a path argument.
- **The machine-read fixtures** — `size-baseline.json`, `size-records.jsonl`,
  `scenarios/*.yaml`, [`../example_config.json`](../example_config.json) and
  [`../THIRD_PARTY_LICENSES.md`](../THIRD_PARTY_LICENSES.md). These are not
  documentation; code and CI open them by path.
- **[`docs/rust-migration/baseline/`](../rust-migration/baseline/)** — an
  *empty* directory with a README explaining that empty is the honest state.
  Capturing a C++ baseline means flashing the C++ onto a powered, wired machine,
  which the C++'s own freeze forbade and which is now impossible: the tree is
  deleted. Cleaning this up would destroy a deliberate honesty signal, so it
  stays empty and stays visible.
