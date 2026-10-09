# CI in this repository

Three workflows. This file says what each one is for, what it costs, and — where
the answer is not obvious — why it is the way it is. Every number here was
measured from a real run; the run ids are in the commit history.

## The shape

```
rust.yml        host      (42 s)   fmt, clippy -D warnings, rustdoc, 1074 tests, parity, device-test audit, UI
                device   (370 s)   esp toolchain, device clippy, release build, image size budget
                invariants(15 s)   four cross-cutting greps and scripts
frontend.yml    frontend    (26 s)  the React UI: lint, types, tests, build
release.yml     release       --   on a v* tag only; builds the Rust image and publishes a
                                  merged full-flash binary. Ran on `v2.0.0-alpha.1`
                                  (run `37925778055`).
```

**Wall clock is the device job.** The other jobs run in parallel with it, so
the time a contributor waits is `device`, and everything else is runner-minutes.

| | wall clock | runner-minutes |
|---|---|---|
| before the C++ removal | 599 s (device job) | 988 s across 3 jobs — and the C++ workflows never ran on a PR to `rewrite/rust` |
| now | **370 s** | **~450 s across 4 jobs** |

## Two toolchains, and why they are separate jobs

`rust-toolchain.toml` pins `channel = "esp"`: an Espressif **nightly fork** that
only `espup` installs. It compiles the firmware for Xtensa.

The other five crates — `cc-domain`, `cc-safety`, `cc-config`, `cc-machine`,
`cc-display` — are `#![no_std]` and touch no Xtensa pin. Their tests are the host
test suite, and they run in the `host` job on a **pinned stable** (`host_toolchain`
in `.mise.toml`'s `[vars]`, installed by the workflow).

Three things forced this apart, each learned the hard way:

1. **A runner with only stock Rust cannot run `just` at all.** `host_target` is
   derived from a backtick, just evaluates backticks while *parsing* the
   justfile, and a failing backtick is a parse error — so `rustc -vV` failing to
   resolve `esp` takes down every recipe with a message that never mentions the
   toolchain. `scripts/host-target.sh` now fails quietly instead.
2. **CI's `stable` is not stable.** A GitHub runner ships whatever `stable` was
   when its image was built. That was old enough not to know a lint this repo
   had just started using, and it broke the clippy step. So the host channel is
   **pinned**, not floating, and `just doctor` asserts the pin is not older than
   the device pin.
3. **`cargo install` resolves through `rust-toolchain.toml`.** So mise's
   `cargo:` backend, and `cargo install espup`, both die on a machine that has not
   got `esp` yet — which is the machine you need `cargo` in order to get `esp`.
   Every bootstrap names the toolchain explicitly or sets
   `RUSTUP_TOOLCHAIN=stable`.

## The device job, step by step

| step | cold | warm | what it is |
|---|---|---|---|
| restore `.embuild` + `target/xtensa-esp32-espidf` | miss | **19 s** | the ESP-IDF clone, tools, python env, and the built archives |
| restore `~/.rustup/toolchains/esp` | miss | fast | 1.9 GB of Xtensa rustc + GCC + LLVM |
| restore `~/.cargo/registry` | miss | fast | 407 crates that were otherwise re-downloaded |
| `mise install` | 16 s | 16 s | host tools |
| ensure free disk | 1 s | 1 s | conditional; see below |
| install the esp toolchain | 71 s | ~0 s | espup (0.4 s) + `espup install`, skipped on a cache hit — see the Caches section, which is what the warm figure is drawn from |
| install the pinned host toolchain | 8 s | 8 s | needed only so the lint-name probe has both channels |
| lint-name probe | 1 s | 1 s | every `clippy::` in an allow/expect exists on ≥1 channel |
| **device clippy** | 240 s | **114 s** | |
| **release build** | 174 s | **111 s** | |
| size budget + record | 2 s | 2 s | |

The 126 s and 63 s came from the cache alone. The cause was that `esp-idf-sys`'s
build script compiles **the whole of ESP-IDF**, and it was doing so **twice per
run** — once for the `dev` profile's `OUT_DIR` (device clippy) and once for
`release` (the firmware build). Different `OUT_DIR`, different CMake build
directory, 107 `.a` archives each. The cache holds both, so the script re-runs
and finds its work already done.

`espup install` is still ~70 s even warm: 2.1 s is the download, the rest is
unpacking. The toolchain cache removes most of it — the run that first
populated it missed, because the commit that added the cache also changed
`.mise.toml`, which is in the key.

### Why `espup` is installed by `cargo-binstall`

    cargo install --locked espup --version 0.17.1 --force     139 s
    cargo-binstall --no-confirm --force espup@0.17.1             0.4 s

`cargo install` compiled a 12 MB release binary and ~180 dependencies on every
run. `cargo-binstall` fetches the release asset, which it can find because
espup's own `Cargo.toml` carries `[package.metadata.binstall]`. mise owns
`cargo-binstall` (prebuilt, via its `aqua:` backend) and binstall owns espup: one
owner each, and nothing compiles.

That changes the trust model, so it is pinned: `cargo install` verifies the
crates.io tarball against an index checksum and then *builds* the binary;
binstall downloads a prebuilt and espup publishes no checksum file. So
`.mise.toml` carries `espup_sha256_linux_x86_64` and the script verifies the
installed binary against it before running it, because that binary is what
installs your compiler. Verified both ways: a matching digest proceeds, a
corrupted one is fatal.

## Caches

Four rules, each learned from something going wrong.

1. **The keys name the real inputs.** `.embuild` and `target/` are keyed on
   `components_esp32.lock`, `Cargo.lock`, `.mise.toml`, `rust-toolchain.toml`,
   `.cargo/config.toml`, `rust/partitions_4M.csv`, **plus** `ESP_IDF_VERSION`,
   `ESP_IDF_SYS_ROOT_CRATE`, `RUSTFLAGS`, `runner.os` and `runner.arch`.
   Not decoration: `esp-idf-sys` builds ESP-IDF from `env.ESP_IDF_VERSION`
   (independent of `components_esp32.lock`, which is the *component* list);
   `ESP_IDF_SYS_ROOT_CRATE` picks the crate owning the `links` metadata and
   therefore the `OUT_DIR`; `.cargo/config.toml`'s `[idf] partition_table` is a
   CMake configure input; and `.embuild` carries a host `python_env` plus
   idf_tools cmake/ninja, so it is not portable across architectures.

2. **PR runs save AND restore.** An earlier version gated every save on
   `github.event_name != 'pull_request'`, on the reading that a PR-written cache
   is dead weight. Half right, and the wrong half expensive: a PR cache *is*
   visible to later runs of the **same** PR, which is the developer loop — push
   2..N of a branch is exactly where a 250 s rebuild is pure waste. Base-branch
   caches are visible to all PRs anyway, so a fresh branch is still warm.

3. **A cached `target/` cannot pass on old code, and the reason is worth
   knowing.** `actions/cache` uses tar and preserves mtimes; `actions/checkout`
   stamps every source file with checkout time, so sources are always newer than
   cached artifacts and cargo's mtime freshness rebuilds the workspace crates.
   Verified locally in both directions: with a newer source mtime cargo
   recompiles; `touch -d 2020-01-01` a source file and cargo accepts the stale
   rlib and reports `Finished in 0.00s`. So: do not add a cache path that
   restores artifacts without a checkout in the same job.

4. **Budget.** 10 GB per repository, LRU, and now billable above it. At the time
   of writing the repository holds ~6.8 GB, most of it PR-scoped. The host
   `target/` directory is deliberately **not** cached: 5.8 GB raw, 3.2 GB of it
   `debug/incremental`, to save ~20 s on a job that finishes long before the
   device job does. If eviction ever bites, sacrifice the cargo registry first
   and the ESP-IDF cache never.

## The disk step

It used to `rm -rf` dotnet/android/ghc unconditionally and cost 43–98 s of
`rm` I/O. An analysis argued for deleting it outright on the basis of "~77 GB
free" — a figure that appears in no log, because the step only ever ran `df`
**after** the delete, and which `rust.yml` itself contradicted with "~14 GB".

It measures now, and the measured value is `free disk: 79G` to `85G`. Against a
working set of ~12 GB (plus tarballs being downloaded *and* extracted on a cold
cache) that is ample, so the threshold is 30 GB: far above need, far below where
the `rm` would help. The `rm` is kept as a fallback, because the question it was
silently answering is real — a build job that dies of ENOSPC looks exactly like a
code failure — and a measured answer beats an unconditional one.

## A thing only CI can catch

Found by CI on a commit that was locally green, and now covered in-repo.

> Action pins are **not** checked in-repo and are not needed: `.github/renovate.json5`
> owns `actions/*`, `jdx/mise-action`, `pnpm/action-setup` and `softprops/*`, so
> they are bumped by a bot on a schedule. An action-pin checker lived here for
> one commit and was removed; the two invented SHAs it was written after are the
> reason it was not worth keeping — a hand-written 40-hex string is a thing a
> bot should never have to compete with.

## Measured and rejected

| idea | why not |
|---|---|
| `lint-esp32 --profile release`, to share one ESP-IDF `OUT_DIR` | −63 s cold, ~0 warm once cached, and it changes the lint gate's `cfg` surface |
| split the device job in two | ~30 s, at ~170 runner-s |
| cache the host `target/` | 5.8 GB, 3.2 GB of it `debug/incremental`, ~20 s on a non-bottleneck |
| drop the host job's UI block (duplicates `frontend.yml`) | 12 s, and it trades a real property — one workflow is the merge gate — for it |
| split `just test` into a matrix | it is seconds, not minutes |
| pin `ubuntu-latest` → `ubuntu-24.04` everywhere | reproducibility, but GitHub's security updates land on `latest` |
| delete the `refs/pull/N/merge` caches | worth doing (~5.8 GB is unrecoverable after merge) but it needs a deletion pass with a token, not a workflow edit |

## What a generic Rust CI template gets right, and wrong, here

A stock skeleton for a Rust project was offered as a reference. Most of it does
not fit this repository, for reasons that are specific rather than fussy — but
three parts of it were adopted.

### Adopted

**`cargo --locked`.** Cargo will otherwise *update* `Cargo.lock` to satisfy a
manifest and carry on, so a green CI run can be a run against a dependency set
nobody reviewed and nobody committed. `Cargo.lock` is committed here precisely so
it is the input; `--locked` is what makes it actually be the input. Now on all 15
cargo invocations in the justfile (`cargo fmt` does not resolve the graph and is
left alone).

Verified by removing one package entry from `Cargo.lock`:

    with --locked    error: cannot update the lock file ... because --locked
                     was passed to prevent this                        rc 101
    without --locked cargo re-adds the entry, silently, and carries on   rc 0

**`CARGO_INCREMENTAL: 0`.** Incremental compilation is dead weight in CI: the
workspace changes, so nearly everything rebuilds anyway, and the bookkeeping is
pure cost on top. It also accounted for **3.2 GB** of `debug/incremental` in the
host target directory — the largest single thing in a tree this repository
already declines to cache.

> **Correction.** This was originally justified with "12 s with, 10 s without" on
> a locally *warm* `target/`. That measurement does not transfer: the host job
> never has a warm `target/` — it is deliberately not cached — so there is no
> incremental state for the setting to save, and CI's host job in fact went 8 s
> *slower* in the run that adopted it (within the ±30% noise this pipeline
> shows). The setting is kept for the disk argument and because it is right in
> principle; the speed claim was not supported.
>
> It also does not invalidate the device `target/` cache, which was the worry:
> `CARGO_INCREMENTAL` *is* a dev-profile fingerprint input (verified in a scratch
> crate — toggling it forces a `Compiling`), but `actions/checkout` already
> stamps every source file with checkout time, so the cached workspace artifacts
> are unconditionally stale either way. The cache's real payload is `.embuild`,
> which is not a cargo unit.

**A bare `pull_request:` with no `branches:` filter.** This is the note's best
idea and it is the fix for the missed-gate bug in a more robust form than the one
I shipped. Enumerating `[main, rewrite/rust]` works until a third long-lived
branch appears, and forgetting it re-opens the hole silently — which is exactly
what happened. A bare filter is correct by construction and needs no maintenance.

### Rejected

**`Swatinem/rust-cache`.** A genuinely good idea — cargo does need a registry
cache, and this would have given it. But it caches `target/` wholesale, and here
that is 5.8 GB of which 3.2 GB is `debug/incremental`, for a host job that
finishes in 50 s while the device job takes 283 s. It also has no way to key on
the things the device caches actually depend on — `env.ESP_IDF_VERSION`,
`ESP_IDF_SYS_ROOT_CRATE`, `RUSTFLAGS`, `runner.arch` — so its `target/` entry
would restore a *wrong* ESP-IDF build rather than a stale one, which is worse than
not restoring. The two settings worth having from it (`CARGO_INCREMENTAL=0` and
the registry cache) are taken directly.

**`dtolnay/rust-toolchain@stable`.** This repository needs *two* toolchains: a
pinned stable for the portable crates and the Espressif `esp` fork for the
firmware. `rust-toolchain.toml` already resolves both, `rustup` reads it, and
`just doctor` asserts the pins. A third mechanism that installs `stable` on top
would reintroduce exactly the ambiguity this branch spent a day removing.

**`--all-features`.** It would enable `cc-display/scenarios` and
`cc-hal-esp32/device-tests` in the *same* clippy pass as the shipped build, so the
device-tests code would be linted under a different `cfg` from the one it ships
in. The gate is deliberately narrower than `--all-features`: `--all-targets`
without it, with `--features cc-display/scenarios` only on `just test`, which is
the one place that genuinely needs it.

**`actions/checkout@v4` by tag.** The repository's action SHAs are pinned, and
Renovate owns them — `.github/renovate.json5` extends `config:best-practices`
and the `renovate/*` branches in this repo show it is running as an app. So there
is nothing for an in-repo pin checker to add, and the one that was here for a
commit has been removed.

**`concurrency: group: ${{ github.workflow }}-${{ github.ref }}`.** Already the
shape here; `cancel-in-progress: true` is already set per workflow.

## A size failure has no automatic "what grew?"

When `just size-check` fails with *"image grew X % vs baseline"*, nothing in the
gate answers **what grew**. `just size` gives a section breakdown
(`.flash.text`, `.rodata`, `.iram0`, `.dram0`) and that is where you stop.

`scripts/size-buckets.py` is the per-crate, per-symbol attribution tool, and it
is **not wired into any recipe**. It is written and working; it is simply never
called, because [`archive/migration/07-image-size-budget.md`](../archive/migration/07-image-size-budget.md)
§8 requires attribution "so an increase is never unexplained" and the R1-09
follow-up to close that gap was never taken. Recorded here so the next reader
does not re-derive that the tool does not exist.

It needs a `diagnostic`-profile ELF, not the release one, because the release
profile is stripped. Build it first:

```sh
just diag-build && python3 scripts/size-buckets.py target/xtensa-esp32-espidf/diagnostic/firmware
```

## Reproducing the measurements

```sh
export GH_TOKEN=…
curl -s -H "Authorization: token $GH_TOKEN" \
  "https://api.github.com/repos/BlackDark/fork-clevercoffee/commits/$(git rev-parse HEAD)/check-runs"
curl -sL -H "Authorization: token $GH_TOKEN" \
  "https://api.github.com/repos/BlackDark/fork-clevercoffee/actions/jobs/<id>/logs"
```

The job log is plain text with a timestamp on every line, which is how the
run-to-run variance was characterised: the same step ranges from 43 s to 98 s
across runs (`rm -rf`), and `clippy (device)` from 174 s to 245 s. **Compare a
change against a run in the same runner-speed class, or not at all.**
