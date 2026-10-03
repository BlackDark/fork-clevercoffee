# CI in this repository

Five workflows. This file says what each one is for, what it costs, and — where
the answer is not obvious — why it is the way it is. Every number here was
measured from a real run; the run ids are in the commit history.

## The shape

```
rust.yml        host      (42 s)   fmt, clippy -D warnings, rustdoc, 1074 tests, parity, device-test audit, UI
                device   (370 s)   esp toolchain, device clippy, release build, image size budget
                invariants(15 s)   four cross-cutting greps and scripts
main.yml        firmware  (184 s)   the C++ firmware build -- the parity oracle
                native-tests(137 s) the C++ 340-case native suite
format.yml      format      (9 s)   clang-format over src/ include/ lib/
frontend.yml    frontend    (26 s)  the React UI: lint, types, tests, build
```

**Wall clock is the device job.** The other seven run in parallel with it, so
the time a contributor waits is `device`, and everything else is runner-minutes.

| | wall clock | runner-minutes |
|---|---|---|
| before this work | 599 s (device job) | 988 s across 3 jobs — and the C++ workflows never ran on a PR to `rewrite/rust` |
| now | **370 s** | **791 s across all 8 jobs** |

## Two toolchains, and why they are separate jobs

`rust-toolchain.toml` pins `channel = "esp"`: an Espressif **nightly fork** that
only `espup` installs. It compiles the firmware for Xtensa.

The other five crates — `cc-domain`, `cc-safety`, `cc-config`, `cc-machine`,
`cc-display` — are `#![no_std]` and touch no Xtensa pin. They are the 1,074 host
tests, and they run in the `host` job on a **pinned stable** (`host_toolchain`
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
| install the esp toolchain | 71 s | 71 s | espup (0.4 s) + `espup install` |
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

## C++ formatting: one pin, asserted

`format.yml` used to run `pio run -e esp32_usb --target check-format`, which
took **103 s**, 76 s of it PlatformIO installing espressif32, the Xtensa
toolchain, the Arduino framework, esptoolpy, SCons and 13 libraries to run a
formatter. Now it runs `python3 scripts/run_clangformat.py` directly. **9 s.**

The formatter version is declared once, in `.mise.toml`'s
`[vars] clang_format_version`, and `scripts/run_clangformat.py` **asserts** the
binary on PATH matches before it will format anything. That assertion is the
point. The pin used to live in four places — `.mise.toml`, `format.yml`, a
`clang-tools:22` Docker image, and a pre-commit hook that used whatever
clang-format the developer had — and they agreed only because 22.1.x and 23.1.x
produce byte-identical output here (0-line diff across 28,632 formatted lines;
21.1.8 and older change two files). The hook runs `-i`: on a machine with an older
clang-format it would have rewritten the **parity oracle**, silently, in a commit
about something else. Renovate owns `.mise.toml`, so a future 24.x would have
done the same.

> The earlier "105 files reformatted" panic was a **phantom**: with
> `.clang-format` out of scope, *every* version reformats *every* file. Losing
> `.clang-format` from the invocation, not a version difference.

## Two things that only CI can catch

Both were found by CI on a commit that was locally green, both in the same
place, and both are now checked in-repo.

* **`scripts/check-action-pins.py`** resolves every `uses: owner/repo@<sha>`
  through the API. It distinguishes a 40-hex commit sha (looked up), hex of the
  *wrong length* (**an error**), and a tag or branch (allowed). The
  wrong-length case exists because a hand-written 43-character "sha" slipped
  through a checker that required exactly 40 — a checker that is silently
  permissive about the thing it checks teaches you to trust a green tick.
* **`scripts/run_clangformat.py` has two entry points**, and the PlatformIO one
  only runs when `platformio.ini` loads the script as a `pre:` script. Deleting
  it took `check_format_callback` with it and every C++ firmware build in CI died
  at its first step — a job that had, until the trigger fix, never run on this
  branch at all. There is a simulation of the SCons load path in the commit that
  fixed it.

## Measured and rejected

| idea | why not |
|---|---|
| `lint-esp32 --profile release`, to share one ESP-IDF `OUT_DIR` | −63 s cold, ~0 warm once cached, and it changes the lint gate's `cfg` surface |
| split the device job in two | ~30 s, at ~170 runner-s |
| cache the host `target/` | 5.8 GB, 3.2 GB of it `debug/incremental`, ~20 s on a non-bottleneck |
| drop the host job's UI block (duplicates `frontend.yml`) | 12 s, and it trades a real property — one workflow is the merge gate — for it |
| split `just test` into a matrix | it is 11 s for 1,074 tests |
| pin `ubuntu-latest` → `ubuntu-24.04` everywhere | reproducibility, but GitHub's security updates land on `latest`; `format.yml` is pinned because it now runs a binary rather than a container |
| delete the `refs/pull/N/merge` caches | worth doing (~5.8 GB is unrecoverable after merge) but it needs a deletion pass with a token, not a workflow edit |

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
