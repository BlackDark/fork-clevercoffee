# Rust migration — evidence base

What three independent efforts established about the C++ → Rust migration, tiered by how the
knowledge was earned. This is an evidence base, not a plan: it exists so a porting or review agent
can tell **what is proven** from **what is merely agreed**, and so an open decision gets settled
against measurements rather than arguments.

Regenerate the source material from any of the three branches with the `git archive` commands in
[Provenance](#provenance).

## Reach for

| When you are | Read | Grade |
|---|---|---|
| Choosing between `esp-idf-svc` and bare-metal `esp-hal`, or deciding scope, parity policy or OTA | [open-decisions.md](open-decisions.md) | steps + criteria |
| Porting, reviewing or changing a subsystem | [findings.md](findings.md) | reference, tiered |
| Sizing flash, RAM, stack, tick budget or carrier frequency | [measurements.md](measurements.md) | reference, flat |
| Tracing a claim to the file and line it came from | [digests/](digests/) | disclosed reference |

Every finding lives in exactly one place. Other documents point here rather than restating.

## Grades

A fact's grade is how it was earned, not which branch claimed it. Three efforts ran independently
with no shared context, so independent agreement is real evidence.

- **device-verified** — reproduced on the attached ESP32, with the number or failure recorded in a
  code comment and a test pinning it.
- **triangulated** — found independently by two or more of the three efforts, from source.
- **single-source** — one effort, from source. Sound, unconfirmed.
- **unexplained** — observed, cause unknown. Do not treat as understood.
- **untested** — asserted, or modelled against a synthetic input. The weakest grade; a green test
  run here proves arithmetic, not hardware.

Grades appear as a leading token on every finding in [findings.md](findings.md). `single-source`
and `untested` are where drift starts: they decay silently, because nothing fails when they go
wrong.

## Headline

All three efforts converged on the same C++ defects, the same original-ESP32 hardware traps, the
same migration path and the same evidence discipline. They disagree on the platform stack and on
scope. Full reasoning in [open-decisions.md](open-decisions.md).

Two traps were flagged by all three from source reading — the FP-in-ISR fault and the LEDC
spin-lock — and then **shipped as bugs and panicked on the device** before being fixed. The
LEDC decision and its reversal are two commits apart in `rewrite/rust`.

The build also produced findings no document predicted, including two independent defects that
together meant **the machine could not heat at all**. Details in
[findings.md](findings.md) §3.

## The branches are not peers

| Branch | Commit | Rust LOC | Device-verified findings |
|---|---|---|---|
| `rewrite/rust` | `c62a894` | 72 644 | yes — 111 device tests on the board |
| `refactor/space2` | `393035b` | 30 201 | no — board crates have never seen a compiler |
| `feat/rust-migration-design` | `45023ba` | 51 | no — crate skeletons |

Only `rewrite/rust` has ever compiled for the device or run on one, so it is the sole source of
**device-verified** facts. Treat every grade below that as inference from source, however confident
the prose reads.

`refactor/space2` records this about itself: its findings are "found by reading the C++ and the HAL
contract", not "found by running it", because nothing has been run.

## Provenance

| Branch | Commit | Committed |
|---|---|---|
| `feat/rust-migration-design` | `45023ba9a15e08e2b31df34a694448f8876cee6c` | 2026-09-29 |
| `refactor/space2` | `393035b0a6a9ebd279edda8cf9cc37fc8caf7a84` | 2026-09-29 |
| `rewrite/rust` | `c62a89455f8fcfc068f1f1c8794a00c039ddc166` | 2026-09-30 |
| `main` (base) | `1cf607ded700723c1c8f2a45bfe4e7a671e119df` | 2026-09-28 |

```sh
git fetch origin
# documents (~1.3 MB)
git archive origin/feat/rust-migration-design docs examples .agents | tar -x -C design/
git archive origin/refactor/space2          docs examples .agents | tar -x -C space2/
git archive origin/rewrite/rust             docs examples .agents | tar -x -C rewrite/
# Rust source
git archive origin/refactor/space2  crates spikes tools scripts justfile Cargo.toml rust-toolchain.toml .cargo | tar -x -C space2/
git archive origin/rewrite/rust     crates tools scripts just justfile Cargo.toml rust-toolchain.toml .cargo rust  | tar -x -C rewrite/
```

No branch was checked out to produce this; exports used `git archive` into a scratch tree.

## Digests

Six per-branch digests, each claim carrying a `file:line` citation. These are the disclosed tier —
consult them to audit a claim, not to learn the finding.

- `digests/{design,space2,rewrite}.md` — mined from the **design documents**
- `digests/code-rewrite-core.md`, `code-rewrite-peripherals.md`, `code-space2.md` — mined from the
  **Rust source**: problems, measurements, and which rules a test pins

The `code-*.md` digests mark each finding comment-only where no test protects it. Those are live
drift risks: a well-meaning refactor re-breaks them silently.

## Redactions

Quoted bench-device data, masked before commit:

- **1-Wire ROM codes** truncated to `2869...af41` / `0x41af...7928`. A ROM is a unique 64-bit
  hardware identifier; the full value is a test constant in `rewrite/rust`. The truncation preserves
  the LSB-reversal observation, which is the finding.
- No credentials, SSIDs, MAC addresses, IPs, keys or flash dumps appear here. Shipped secrets are
  named, never quoted — the branches' own rule is that a flash dump must never enter the repository.
