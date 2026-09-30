# Rust migration — three-branch evidence review

Compares the design documents **and the implemented Rust** produced independently on three branches,
to establish what is actually known about the C++ → Rust migration — and to separate what was
*designed*, what was *predicted*, and what was *measured on a device*.

> **This directory is a snapshot, not living documentation.** It is kept as an evidence record of
> what three independent efforts found, and of the disagreements that remain open. It is not a
> plan to execute. Nothing here has been merged into the firmware.

## Provenance

Everything below was derived from these exact commits. To re-verify any claim, re-run the
`git archive` commands in [Method](#method) against these SHAs.

| Branch | Commit | Committed | Rust LOC | Ran on device |
|---|---|---|---|---|
| `feat/rust-migration-design` | `45023ba9a15e08e2b31df34a694448f8876cee6c` | 2026-09-29 | 51 (skeletons) | no |
| `refactor/space2` | `393035b0a6a9ebd279edda8cf9cc37fc8caf7a84` | 2026-09-29 | 30 201 | no |
| `rewrite/rust` | `c62a89455f8fcfc068f1f1c8794a00c039ddc166` | 2026-09-30 | 72 644 | **yes** |
| `main` (base of this branch) | `1cf607ded700723c1c8f2a45bfe4e7a671e119df` | 2026-09-28 | — | — |

**The branches are not peers.** Only `rewrite/rust` has ever been compiled for the device or run
on one, so it is the sole source of *empirical* findings. The other two are source-reading and
port-time inference.

## Contents

| File | What it is |
|---|---|
| **`implementation-evidence.md`** | What building and running the code actually found. The most trustworthy document here. |
| **`common-design.md`** | The design all three branches agree on, and the places they diverge. |
| **`common-pitfalls.md`** | The shared defect register and hard problems, cross-referenced by branch. |
| `digests/design.md`, `digests/space2.md`, `digests/rewrite.md` | Per-branch digests of the **design documents** |
| `digests/code-*.md` | Per-branch digests mined from the **Rust source** — problems, measurements, pinned tests |

The raw exported documents and Rust trees are **not committed here** — they are verbatim copies of
other branches and are regenerable with the two commands in [Method](#method). Only the synthesis
and the per-branch digests (with file-level citations) are versioned.

### Redactions

These documents quote real bench-device data. The following were masked before committing:

- **1-Wire ROM codes** are truncated to `2869...af41` / `0x41af...7928`. A ROM is a unique 64-bit
  hardware identifier; the full value is a test constant in `rewrite/rust`
  (`crates/cc-domain/src/sensor/ds18b20.rs`). The truncation preserves the LSB-reversal observation
  that is the actual finding.
- No Wi-Fi credentials, SSIDs, MAC addresses, IP addresses, keys or flash dumps appear anywhere in
  this directory. Device credentials such as the shipped `mqtt.password` default are referenced by
  *name* only, in keeping with the branches' own rule that a flash dump must never enter the
  repository.

## Method

No branch was checked out. The branch that was current during the review (`refactor/space2`) was
never switched; the exports were taken with `git archive` into a scratch directory.

```sh
git fetch origin
# documents
git archive origin/feat/rust-migration-design docs examples .agents | tar -x -C design/
git archive origin/refactor/space2          docs examples .agents | tar -x -C space2/
git archive origin/rewrite/rust             docs examples .agents | tar -x -C rewrite/
# Rust source
git archive origin/refactor/space2       crates spikes tools scripts justfile Cargo.toml rust-toolchain.toml .cargo | tar -x -C space2/
git archive origin/rewrite/rust          crates tools scripts just justfile Cargo.toml rust-toolchain.toml .cargo rust   | tar -x -C rewrite/
```

Two passes, because the branches are not peers:

1. **Documents** — ~1.3 MB across the three branches, read by a separate fresh-context subagent per
   branch, producing structured digests with file-level citations.
2. **Code** — the three branches' Rust trees (0 / 30 201 / 72 644 lines), mined by three more
   subagents for problems recorded in comments, test names and measured constants, plus the 18
   commits of `rewrite/rust`, whose bodies are effectively a lab notebook.

Every claim in the three synthesis documents is traceable to a digest, and headline numbers were
re-verified directly against the exported source.

This directory is persistent and lives outside the repository and outside `/tmp`. Nothing in
`fork-clevercoffee` was modified.

## Headline

The three efforts **converged much more than they diverged**. Independently, with no shared
context, they found:

- the same blocking C++ safety defects (dead pump timeouts, no steam-valve whitelist, unregistered
  emergency-stop parameters, inert heater-shutdown bookkeeping);
- the same original-ESP32 hardware landmines (FP-in-ISR panics, LEDC spin-lock panic, 20 %-of-loop
  blocking pressure read, 100 kHz I²C contention, strapping-pin relay);
- the same migration path — **the `config.json` round-trip is the only bridge** between old and new
  firmware, and each found a distinct way it breaks;
- the same verification standard — an explicit evidence ladder, C++ as the frozen oracle, and
  "a capability that compiles is not a capability that works."

**And the prediction was right.** The two hardware traps all three flagged by reading source —
FP-in-ISR and the LEDC spin-lock — were then **independently shipped as bugs and reproduced as
boot panics** on the real board before being fixed. All three branches predicted them; only one
branch had to find out the hard way.

The disagreements:

- **Platform.** `design` and `rewrite` chose `esp-idf-svc`/`std`; `space2` chose `esp-hal`/embassy
  bare metal, arguing that `esp-hal`'s per-chip support is machine-enforced with HIL CI while
  `esp-idf-svc` self-reports having none. The disagreement is not about `embassy` — the other two
  rejected `embassy-executor` for exactly the ISR-safety reason `space2` relies on it for.
- **Scope.** Original-ESP32-only vs three-chip support; bug-for-bug parity vs fix-by-construction.
- **Evidence.** The three branches are not peers. `design` is 51 lines of crate skeletons;
  `space2` has 30 201 lines that have **never been through a compiler**; `rewrite` has 72 644 lines
  with **111 device tests** running on a board. **Only one branch has empirical findings at all**,
  and it found problems no document predicted — including two independent bugs that meant the
  machine **could not heat at all** (an inverted water-tank switch and a PID mode cache seeded from
  intent instead of from the controller).

**Full detail: `common-design.md` §2 for the decision matrix, `implementation-evidence.md` for what
running the code actually found.**
