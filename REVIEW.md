# REVIEW.md — C++ → Rust port, `main` → `rewrite/rust`

**Reviewer:** senior Rust engineer / adversarial reviewer (read-only).
**Method:** orchestrator recon + 7 parallel specialist reviewers (A parity-C++, C idioms/safety, D over-engineering, E efficiency, F design, G build/deps/CI, H toolchain) + 1 parity-matching reviewer (B) + 1 adversarial verifier (Phase 2) that was instructed to **refute** every Critical/High finding.
**Baseline:** `main` (merge-base `2006b710`); port at `b11f8770`. Diff: **302 files, +180,312 / −40**.
**Target:** original ESP32 (ESP32-WROOM-32E, Xtensa LX6), **std via `esp-idf-hal` 0.47 / `esp-idf-svc` 0.53 / `esp-idf-sys` 0.38**, ESP-IDF v5.5.5, FreeRTOS. Portable core (`cc-domain`/`cc-safety`/`cc-config`/`cc-machine`/`cc-display`) is `#![no_std]` + `alloc` and host-testable.
**Constraint that matters:** flash size is binding — C++ image 1,546,240 B; Rust image recorded at 1,559,520 B against an 1,835,008 B app slot.

---

## 1. Verdict

**Not merge-ready as a replacement for the C++ firmware. Merge-ready as a work-in-progress branch *if* the release path is fenced off first.**

Confidence: **high** on the engineering quality of the port itself; **high** on the release/CI risk. This is an unusually disciplined port — `no_std` layering holds, 1,100 host tests pass, all 18 states and every safety guard from the C++ are reproduced, and the divergences are documented rather than hidden. It is also, in three specific ways, **not the product it claims to be**: a release tag from this branch ships the **C++** image, **MQTT/Home-Assistant is built and never driven**, and there is **one live use-after-free**.

Top 5 risks:

1. **Critical** — `.github/workflows/release.yml` triggers on `v*` tags with no branch filter and runs `pio run`, so tagging this branch publishes the **C++ firmware** plus a `littlefs.bin` at `0x350000` that does not exist in `rust/partitions_4M.csv`. Confirmed and reproduced.
2. **Critical** — **undefined behaviour**: the `Cell<T>` seqlock in `cc-hal-esp32/src/web.rs:362` is read while `network.rs:449` reassigns `Telemetry.ip` (a fresh `String`, dropping the old buffer) **every 10 ms tick**. That is a live use-after-free on the httpd task at 100 Hz, not a theoretical race. Confirmed against the actual call sites.
3. **High** — **zero CI for the Rust workspace.** `rg 'cargo|just ' .github/` → 0 hits. All four workflows run PlatformIO/pnpm only. `just gate` has never been invoked by anything.
4. **High** — **MQTT is implemented and never called.** `main.rs:937` constructs `Client::new`, reads one bool, and drops it — `esp_mqtt_client_destroy` runs microseconds later. Publish, subscribe, HA discovery and reconnect have zero call sites. `/api/status.mqttConnected` is structurally always `false`.
5. **High** — `just gate` **cannot pass on a clean checkout**: `cc-hal-esp32/build.rs:309` panics without `ui/packages/frontend/dist`, and no recipe or CI step builds it. Compounded by `justfile:73` hardcoding `host_target = aarch64-apple-darwin`, so `just test` fails outright on any non-Apple host.

Grades:

| Dimension | Grade | One-line justification |
|---|---|---|
| **Feature parity** | **B−** | 158 OK / 21 WARN / 14 MISSING / 3 declared-DROPPED. Every state, transition and safety guard matches; MQTT outbound, `/api/config/upload`, HTTP auth and the Logger ring are missing; `shots_since_backflush` is never incremented, so the backflush reminder is dead on the API *and* the OLED. |
| **Idiomatic Rust** | **A−** | `#![no_std]`/`forbid(unsafe_code)` layering is real, error handling is `Result`-based end to end, `Secret<T>` redacts at the type level, `Millis` uses `wrapping_sub` with the wrap argument written out. Deductions: 24 hand-written `unsafe` constructs where two are unsound, and `Vec<Effect>` allocated 4×/tick in the 10 ms loop. |
| **Simplicity** | **B−** | 2.2× the C++ code lines, but 3.6× the comment lines and a documented expansion of thin headers into full modules (`abp2.rs` 870 vs 74). Deducted for `cc-provisioning` (17 LOC, 4 deps, 0 uses), ~575 LOC of provably deletable code, two traits with zero impls, and a 1,094-line `control_task` function. |
| **Performance** | **C+** | No mutex on the machine path (a deliberate seqlock/queue design), display render is 0-alloc, `cc-display` uses `fixed_str` throughout. But `parameters_json()` — 420 allocations, 20 KB — runs **every 10 ms** instead of every second, and `Vec<Effect>` churns 4 allocations per tick. `just bench` is dead: the bench targets it names do not exist. |
| **Dev environment** | **D** | CI runs zero Rust. `just test` fails on Linux. `just bench` and `just size-bench` reference files that do not exist. `just gate` fails on a clean checkout. `.mise.toml` pins `rust = "stable"` next to `rust-toolchain.toml`'s `esp` (mise installs no compiler at all — it symlinks rustup). `release.yml` builds the wrong firmware. |

---

## 2. Tooling results

### Commands run by the orchestrator (this machine: Linux x86_64, `rustc 1.97.0-nightly` on the `esp` channel)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** (clean) |
| `just lint` (host clippy, `-D warnings`, workspace `clippy::pedantic` = deny) | **PASS** — but only because `[unstable] build-std` silently cross-built `std` for the *wrong* target triple |
| `just test` (5 portable crates) | **FAIL as shipped**: `cc: error: unrecognized command-line option '-arch'`. With `CC_HOST_TARGET=x86_64-unknown-linux-gnu`: **~1,100 tests, all pass** |
| `just parity-test` | **PASS** (71 tests) |
| `just lint-esp32` (device clippy, Xtensa) | **FAIL** — `cc-hal-esp32/build.rs:309` panic, UI bundle missing |
| `just build-esp32` / `just diag-build` | **FAIL** — same cause |
| `just size-check` | **FAIL** — no image (`target/xtensa-esp32-espidf/` has only `debug/`) |
| `just bench` | **FAIL** — `error: no bench target named 'reducers' in 'cc-machine' package`. No `benches/` directory exists anywhere in the repo |
| `just size-bench esp32` | **FAIL** — `./scripts/parity/loop-timer.sh` does not exist |
| `cargo doc --no-deps` (5 portable crates) | 83 warnings, 0 errors; all `rustdoc::broken_intra_doc_links` (e.g. `cc-machine/src/effect.rs:14` → `crate::applier::Applier`, `cc-domain/src/heater.rs:326` → `cc-hal-esp32::heater`). No `just` recipe or CI step runs it |
| `cargo tree -d --target xtensa-esp32-espidf -p cc-firmware` | 22 duplicated names; only **2** are actually linked into the image: `embedded-hal` 0.2.7+1.0.0 and `nb` 0.1.3+1.0.0, both forced by `esp-idf-hal` 0.47's dual support. The other 20 are build-dep/proc-macro/host-only. **Non-issue.** |
| `cargo tree -d` (host, portable crates) | nothing to print |
| `du -sh target/*` | whole `target/` **6.0 GB**; `x86_64-unknown-linux-gnu` alone **1.5 GB** (6 `librustc_std_workspace_*` rlibs); `aarch64-apple-darwin` **477 MB** — `std` cross-built for an Apple triple from Linux |
| `python3 scripts/device-test-audit.py .` | exit 0, "147 device unit tests, all registered". **Verified genuinely load-bearing**: the verifier built a synthetic tree in `/tmp` and showed a negative control (unregistered test → exit 1) and a positive control (→ exit 0) |
| `.env` present in the working tree | 46 bytes, `WIFI_SSID=`/`WIFI_PASS=` real values, correctly gitignored (`.gitignore:21`), never echoed by any recipe. **Not a finding.** |
| `.espup-env.sh` | untracked, referenced by nothing (`.gitignore` has `.rust-esp-env.sh`, not this). Leftover; delete it |

### Tools NOT installed on this host — not run, not attempted

`cargo-udeps`, `cargo-machete`, `cargo-audit`, `cargo-deny`, `cargo-geiger`, `cargo-bloat`, `cargo-miri`, `espflash`, `ldproxy`.
Consequence: **the repo declares no audit story at all** — no `deny.toml`, no `SECURITY.md`, no `.github/dependabot.yml`. `.github/renovate.json5` exists but its `customManagers` are PlatformIO-only and no workflow runs Renovate. `cargo miri` would not help for the two `unsafe` findings anyway (device-only crates; miri cannot model Xtensa/FreeRTOS).

### Baselines, C++ vs Rust

| | C++ (`main`) | Rust (HEAD) |
|---|---|---|
| src LOC | 28,823 (147 files) | **68,530** (10 crates) |
| test LOC | 10,241 | **13,563** in `tests/`+`examples/` (27,354 counting inline `#[cfg(test)]`) |
| code lines only (excl. comments/blanks) | 17,535 | **39,196** — **2.23×** |
| comment lines | 6,839 (23.5%) | **24,755 (36.3%)** |
| largest single file | `src/ota.cpp` 868 | `crates/cc-hal-esp32/src/web.rs` 3,695 |
| largest single function | — | `control_task` in `crates/cc-firmware/src/main.rs:2011-3104` = **1,094 lines** |
| third-party deps | 2 `lib_deps` in `platformio.ini` | 59 direct dep declarations, **205 packages** in `Cargo.lock` |
| image size | 1,546,240 B | **1,559,520 B** (recorded `f25-web-ui-embedded`), app slot 1,835,008 B |
| host tests | 340 (`pio test -e native_test`) | **~1,100** (`just test`) + 147 on-target + 71 parity |

The LOC gap is **not** mainly bloat: 36% of the Rust src is comments, and 2,807 of `cc-display`'s 12,965 is verbatim U8G2 glyph data the C++ gets free from a linked library. Net novel code is ~1.4× the C++, which is reasonable for a `no_std` + host-testable port of Arduino-framework code.

---

## 3. Dev environment verdict

### Is mise a good fit for this repo?

**Partly — but it must stop pretending to own Rust.** mise is a good fit for node/pnpm/python/clang-format/just and the cargo-installed *host* tools. It is the wrong owner for the **compiler**: `ls -la ~/.local/share/mise/installs/rust/` → `stable -> /home/claudy/.cargo/bin`, i.e. **`mise ls` reports `rust stable (symlink)` while installing no toolchain at all**, and `idiomatic_version_file_enable_tools` is unset so mise never reads `rust-toolchain.toml`. That is a false signal next to `channel = "esp"`.

**Recommendation: option (a)** — mise owns non-Rust tools, rustup/espup own the compiler, `rust-toolchain.toml` is the single pin.
- **Rejected (b)**: functionally near-identical but leaves `rust = "…"` in `[tools]` looking load-bearing, re-creating exactly the ambiguity above.
- **Rejected (c)**: the split cannot work here — `[unstable] build-std`, `-Zbuild-std` and `components_esp32.lock` (`target: esp32`) are all workspace-root-scoped, so a nested `rust-toolchain.toml` would need a second lockfile and break the single `just gate`.

### Observed active toolchain

```
$ rustup show active-toolchain
esp (overridden by '/home/claudy/workspace/fork-clevercoffee/rust-toolchain.toml')
$ mise exec -- rustup show active-toolchain
(identical)
$ cargo --version   → cargo 1.97.0-nightly (c980f4866 2026-06-30) (1.97.0.0)
$ mise --version    → 2026.9.16
$ just --version    → 1.58.0
```

### Toolchain-pin conflicts (all evidence-backed)

| Source | Value | Effective? | Verdict |
|---|---|---|---|
| `rust-toolchain.toml:2` | `channel = "esp"` | yes, unless an ambient `RUSTUP_TOOLCHAIN` outranks it | **Floating channel — pins nothing.** The real version (`1.97.0.0`) lives in a comment inside a `.mise.toml` task. |
| `.mise.toml:14` | `rust = "stable"` | **no** — installs nothing (symlink to rustup); mise does not read the version file | Dead + misleading → **remove**. |
| `justfile:55` | `export RUSTUP_TOOLCHAIN := "esp"` | yes, inside recipes | Load-bearing workaround for ambient state. Keep. |
| `.mise.toml:40` | `espup install … --toolchain-version 1.97.0.0` | yes on `just setup` | The real pin, buried in a task comment → hoist to a named variable. |
| `.mise.toml:17` **and** `.mise.toml:32` | `cargo:espup = "latest"` **and** `cargo install --locked espup --force \|\| true` | both, PATH-order dependent | Two owners of one binary; `--force` overwrites mise's install and `\|\| true` hides failure. → keep one. |
| `.mise.toml:15-16` | `cargo:espflash` **and** `cargo:cargo-espflash`, both 4.6.0 | both | `cmp` on the two `.crates.toml` → **different packages, same tool**, ~41 MB duplicated. The justfile uses both spellings. → keep `cargo-espflash`. |
| `.mise.toml:6,11` | `pnpm = "latest"`, `just = "latest"` | yes | `mise ls` shows **two pnpm majors** installed (11.25.0 and 12.6.0). Non-reproducible. |
| `.mise.toml:40` | `--targets esp32,esp32s2,esp32s3` | yes | Installs two Xtensa GCCs (~GB each) for targets R4-07/R4-08 explicitly deferred. → `--targets esp32`. |
| `.cargo/config.toml:8` | `[build] target = "xtensa-esp32-espidf"` | yes, for *every* bare cargo call | Redundant: all 8 device recipes already pass `--target` and `-Zbuild-std` explicitly (`just -n build-esp32` proves it). `cargo check -p cc-safety` with no `--target` currently dies with `E0463: can't find crate for core`. → **delete**. |
| `.cargo/config.toml:28` | `[unstable] build-std = ["std","panic_abort"]` | yes, host too | Every host test rebuilds `std` from source (9 rlibs / 258 MB for a single fresh `cargo test -p cc-safety`). Redundant with the recipes. → **delete**. |
| `.cargo/config.toml:38` | `build-std-features = ["std/backtrace-trace-only"]` | yes | The **only** `[unstable]` key with no CLI equivalent — drops gimli/addr2line from the image. → **keep**. |
| `.cargo/config.toml:33-34` | comment: "on a stable host cargo this section is ignored **with a warning** (verified)" | — | **The comment is wrong.** `RUSTUP_TOOLCHAIN=stable cargo tree -p cc-safety` → exit 0, **zero stderr**. `[unstable]` is *silently* ignored, so a developer on stable gets a silently wrong build instead of a diagnostic. |
| `justfile:73` | `host_target := env_var_or_default("CC_HOST_TARGET", "aarch64-apple-darwin")` | yes | **The blocker.** `just test` fails on this Linux host. → derive from `os_family()`. |
| `justfile:29` / `.cargo/config.toml:29` | `ESP_IDF_VERSION = "v5.5.5"` | yes, matches `components_esp32.lock:5` | Consistent ✅ — but `doctor`'s assertion (`test -n "$ESP_IDF_VERSION"` on an unconditional `export`) is a tautology. |
| `.pre-commit-config.yaml:19` vs `.mise.toml:8` | clang-format **v17.0.6** vs **23.1.1** | both | Three formatters over the same C++ files (pre-commit 17, dev 23.1.1, CI = espressif32's bundled). Guaranteed C++ churn. |
| `format.yml:40` | `jdx/mise-action` (install defaults to true) | yes | Installs espflash + cargo-espflash + espup + ldproxy to run **one** `pio run -t check-format`. ~10 min wasted CI. |

### Proposed `.mise.toml`

```toml
min_version = "2024.1.0"

# CleverCoffee toolchain manifest.
#
# OWNERSHIP RULE (deliberate):
#   mise owns every tool that is a plain versioned binary: node, pnpm,
#   python, clang-format, just, and the cargo-installed host utilities.
#   rustup + espup own the COMPILER. mise does not, because:
#     * the device compiler is Espressif's `esp` *nightly fork*, which mise's
#       rust backend cannot express (no versioned channel name);
#     * `rust-toolchain.toml` is the file rustup itself reads, so it is the
#       pin every IDE and CI job will resolve too;
#     * mise's rust backend installs nothing here anyway -- it symlinks
#       `mise/installs/rust/stable -> ~/.cargo/bin`, handing you back rustup
#       and letting rust-toolchain.toml decide. `rust = "stable"` in [tools]
#       therefore only advertises "stable" while the file beside it says
#       "esp", which is the wrong mental model for the single thing most
#       likely to break a size-budgeted device build.
#   `rust-toolchain.toml` is the single compiler pin; `[tasks.setup-esp]`
#   bootstraps it.
#
# NOTE: `justfile:37` appends ~/.cargo/bin to PATH, so rustup and cargo are
# reachable from every recipe even though mise installs no Rust.

[tools]
  # --- C++ parity oracle + web UI ---
  node = "24.21.0"
  pnpm = "11.25.0"
  python = "3.14.7"
  clang-format = "23.1.1"

  # --- Rust host tooling ---
  just = "1.42.4"

  # ONE package. `cargo-espflash` provides `cargo espflash ...`, which is the
  # only spelling any recipe in this repo uses after fix #9 below. The
  # separate crates.io package `espflash` is the same tool from a second
  # crate and cost ~20 MB for nothing.
  "cargo:cargo-espflash" = "4.6.0"
  # Wraps the Xtensa `ld` for the ESP-IDF link (see .cargo/config.toml).
  "cargo:ldproxy" = "0.3.5"

[settings]
  experimental = true

# --- Rust pins mise cannot express, hoisted out of the task body so they are
# greppable and reviewable in one place. ---

# The Rust release Espressif's `esp` toolchain is cut from. rustup's `esp`
# channel is FLOATING, so this is the only version pin in the repo for the
# device compiler; keep it and rust-toolchain.toml in lockstep.
x86_64_toolchain_version = "1.97.0.0"
espup_version = "0.17.1"

# ONLY the production target. esp32s2/esp32s3/esp32c6 are R4-07/R4-08 work
# that has not been flashed or exercised; their Xtensa GCC costs ~1 GB each.
x86_64_targets = "esp32"

# Install/refresh the esp-rs Xtensa toolchain -- the one step mise cannot own.
#
# Each array element runs in its OWN shell, so these must stay separate
# commands and must not rely on variables set by a previous element. The
# ${espup_version} / ${x86_64_toolchain_version} forms substitute from the
# keys above so the versions are not duplicated here.
[tasks.setup-esp]
  description = "Install the esp-rs Xtensa toolchain pinned by x86_64_toolchain_version"
  run = [
    # WHY cargo install and not "cargo:espup" in [tools]: espup must exist
    # before `mise install` can affect the compiler, and mixing a
    # mise-managed espup with `cargo install --force` gives two owners of one
    # binary (whichever is first on PATH wins, silently). The previous
    # version also had `|| true` here, which swallowed a failed install and
    # let this task die later at "espup: command not found".
    "cargo install --locked espup --version ${espup_version} --force",
    # `--toolchain-version` is mandatory and `--skip-version-parse` is what
    # makes it legal. VERIFIED 2026-09-28 on this host: without them,
    # `espup install` dies querying
    #   https://api.github.com/repos/esp-rs/rust-build/releases/latest
    # ("error sending request ... operation timed out").
    "espup install --targets ${x86_64_targets} --toolchain-version ${x86_64_toolchain_version} --skip-version-parse",
    "just env-file",
  ]
```

### Proposed `rust-toolchain.toml`

```toml
# The single compiler pin for the repository. rustup reads this file
# automatically for every cargo/rustc/rustfmt/clippy invocation under the
# workspace root, which is why it -- not .mise.toml and not the justfile --
# is the right place for it.
#
# Precedence (rustup): directory override > rust-toolchain.toml > default.
# An ambient RUSTUP_TOOLCHAIN env var outranks this file, so the justfile
# re-exports RUSTUP_TOOLCHAIN=esp to make every recipe immune to a stray
# export in a shell, a CI job, or an agent's environment.
#
# `esp` is a FLOATING channel -- rustup cannot express a version for it. The
# concrete version is pinned in `.mise.toml` as `x86_64_toolchain_version`,
# and the two must be bumped together (that is justfile fix #18 below).
#
# `components` must stay in sync with `just fmt-check` / `just lint`, which
# call `cargo fmt` and `cargo clippy` with no --toolchain override.
[toolchain]
channel = "esp"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

### Justfile fixes (numbered, in dependency order)

| # | Location | Change |
|---|---|---|
| **1** | `.github/workflows/release.yml:3-6` | **Add `if: github.ref == 'refs/heads/main'` to the `release` job** (or gate the whole workflow). Blocking: a `v*` tag on this branch currently publishes the C++ image. |
| **2** | `.github/workflows/` (new `rust.yml`) | Add a host job running `mise install && just fmt-check lint test parity-test test-audit build-esp32 size-check`, with the UI built first. Zero Rust is in CI today. |
| **3** | `justfile` (new recipe) | `ui:` → `pnpm install --frozen-lockfile && pnpm --filter @clevercoffee/frontend build`; make `build-esp32: ui`, `lint-esp32: test-audit ui`, `size: ui`, `size-check: ui`. Fixes `build.rs:309`. |
| **4** | `justfile:73` | `host_target := env_var_or_default("CC_HOST_TARGET", if os_family() == "macos" { "aarch64-apple-darwin" } else { "x86_64-unknown-linux-gnu" })`. Fixes `just test` on every non-Apple host. |
| **5** | `.cargo/config.toml:6-8` and `:28` | Delete `[build] target` (all recipes already pass `--target`) and `build-std` (all recipes pass `-Zbuild-std=std,panic_abort`). **Keep** `build-std-features` — it has no CLI equivalent. Also correct the inaccurate "ignored with a warning" comment at `:33-34`. |
| **6** | `justfile:364-368` (`reflash`) | Change `@just flash {{port}}` → `- just flash {{port}}` (just does **not** strip `@` inside a shebang recipe; verified with a `/tmp` repro → `@just: command not found`, exit 127), and add `set -euo pipefail` — it is the only shebang recipe missing it. |
| **7** | `justfile` top | Add `set shell := ["bash", "-euo", "pipefail", "-c"]`, then delete the 11 `#!/usr/bin/env bash` shebangs and the redundant `set -euo pipefail` lines. One error semantic. |
| **8** | `justfile:93,96` (`doctor`) | Delete the vacuous `test -n "$ESP_IDF_VERSION"`. Turn `echo "host target: …"` into a **comparison** against `rustc -vV`'s `host:` that exits non-zero on mismatch — the check that would have caught fix #4. |
| **9** | `justfile:347,352,411,415` | `espflash X` → `cargo espflash X`, then drop `"cargo:espflash"` from `.mise.toml`. |
| **10** | `justfile:189` + `:209,399,428` | The 3-line pyserial-discovery loop is copy-pasted 3×. Extract `scripts/find-pyserial.sh`; also drop the ambient `python3` in favour of the mise-pinned interpreter. |
| **11** | `justfile:457-461` (`bench`) | The bench targets `reducers`/`layout` **do not exist**. Either write them or delete the recipe. Add them before claiming any 10 ms-tick timing. |
| **12** | `justfile:463` (`size-bench`) | Points at `./scripts/parity/loop-timer.sh`, which does not exist. Delete or implement. |
| **13** | `justfile` (new) | `clean:` (`cargo clean`), `doc:` (`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p …`), `fmt-cpp:` (`pio run -t format -e esp32_usb`), `check:` (one command CI and humans both run). |
| **14** | `justfile:5` | Add `set dotenv-load := true` (mise already loads `.env`; just does not). Not needed for `wifi-provision` — `scripts/wifi_provision.py:37` reads `.env` itself — but the justfile comment at `:376` implies otherwise. |
| **15** | `justfile` (new `[group(...)]` headers) | `just --list` currently prints the **last** comment line of each recipe, so `build-all` shows "# builds is not a supported target (06 R4-07/R4-08…)" and `env-file` shows "# device recipe." Move real docs into `description:` attributes. |
| **16** | `just/size.just:11-12,115` | `record mcu="esp32"` accepts an mcu but uses it only in the error text — `just size-record esp32s3 x` silently measures the **esp32** image. Derive `tgt`/`elf` from `mcu`, or delete the parameter. |
| **17** | `.pre-commit-config.yaml:19` | Bump `mirrors-clang-format` to `v23.1.1` to match `.mise.toml:8`, or delete the hook and make `just fmt-cpp` the single entry point. AGENTS.md itself warns about clang-format version drift. |
| **18** | `justfile` header + `.mise.toml` | Add a `doctor` assertion that `x86_64_toolchain_version` in `.mise.toml` matches the installed `esp` toolchain, and that `rust-toolchain.toml` still says `esp`. Today the real version lives in a prose comment nobody reads. |
| **19** | `.github/workflows/format.yml:40` | `with: { install: false }` — a C++-only format job should not install four Rust tools. |
| **20** | `.gitignore` + repo root | Delete the untracked `.espup-env.sh` (referenced by nothing); add `__pycache__/` and `git rm --cached crates/cc-display/tools/__pycache__/`. |
| **21** | `README.md`, `REPOSITORY_SUMMARY.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `DEBUG_GUIDE.md` | Add a "Rust firmware" section: `just setup` once, `just gate` before commit. `git diff --stat main...HEAD -- .github` is **empty** and the top-level docs mention `pio run` 20× and `cargo` 1× — a new contributor builds the C++ image and never learns the port exists. |

---

## 4. Feature parity matrix

All ❌ and ⚠️ first. Counted over Subagent A's 202-item C++ inventory.

### ❌ MISSING — 14 items

| # | Feature | C++ location | Rust location | Notes |
|---|---|---|---|---|
| ❌1 | **MQTT publish pass never invoked** | `src/network/MQTTManager.cpp:201` publish loop, called from `src/core/LoopManager.cpp` | `crates/cc-hal-esp32/src/mqtt.rs` (all built), **no call site** | `main.rs:936-953` constructs `Client::new`, reads `client.ever_connected()` once, and drops it — `esp_mqtt_client_destroy` fires microseconds after `esp_mqtt_client_start`. `rg 'publish_pass\|discovery_due\|publish_online\|due_for_reconnect\|note_event' crates/ -g '!mqtt.rs'` → 0 hits. **Verified: actual client destruction, not just a missing call.** |
| ❌2 | **Home-Assistant discovery never published** | `src/network/MQTTManager.cpp:614-790` (~40 entities) | `cc-config/src/discovery.rs` (rich, inert); `mqtt.rs:576-581` `discovery_due()`/`mark_discovery_sent()` have **zero callers** | The whole HA integration is non-functional. Undeclared. |
| ❌3 | **`POST /api/config/upload`** | `src/network/WebServerManager.cpp:727` | none — `rg 'config/upload' crates/` → 0 code hits | Not theoretical: `ui/packages/frontend/src/pages/SystemPage.tsx:182` has a live "Upload configuration" button that will 404. |
| ❌4 | **CORS + HTTP Basic auth** | `src/network/WebServerManager.cpp:272-296` | none — `rg 'Access-Control\|WWW-Authenticate\|realm' crates/` → 0 | `system.auth.enabled/username/password` exist in `schema.rs:690-704`, are settable via `POST /api/parameters` (`assign.rs:314/364/368`) and readable via `GET /api/parameters` — and **do nothing**. Worse: `web.rs:1390` `routes()` advertises `("/api/status", Method::Options)` but no `fn_handler` ever registers it, so a preflight 404s. |
| ❌5 | **`Logger` ring buffer** | `src/Logger.cpp:79-98` (16 × 304 B lock-free ring) | none — `rg 'Logger\|RING_SIZE' crates/*/src` → 0 | `cc-hal-esp32/src/telnet.rs:38,72` is line-at-a-time with `LINE_BUFFER_BYTES = 256`; no ring, no burst buffering. ADR-0002 describes a ring the code does not have. |
| ❌6 | **`[ts] [LEVEL] msg` log format** | `src/Logger.cpp:200-232` | none | Log lines go straight to the `log` crate; no timestamp/level prefix, no `"..."` truncation marker, no `[NULL_MESSAGE]`. Anything scraping serial logs must be rewritten. |
| ❌7 | **Telnet MAX-8-flushes-per-update cap** | `include/clevercoffee/Logger.h:29` | none | — |
| ❌8 | **MQTT value-change dedupe** | `src/network/MQTTManager.cpp:512-556` | none — `rg 'last_sent\|dedupe' mqtt.rs` → 0 | The broker is flooded every 5 s regardless of change. |
| ❌9 | **MQTT publish intervals incl. 500 ms during brew** | `include/clevercoffee/network/MQTTManager.h:260-262` | `mqtt.rs:88-91` documents the omission; **no constant defined** | — |
| ❌10-13 | **OTA: POST `/api/ota/firmware`, `/filesystem`, `/url`; HTTP upload flow; ArduinoOTA** | `src/ota.cpp:404-625`, `:847-863` | `web.rs:1806-1820` — routes exist and honestly return `501 {"reason":"R3-15"}` | 🗑️ **Declared and DROP-justified** (`docs/rust-migration/README.md`, `intentional-diffs.md`). Reported as MISSING *function*, not as a defect — except that `GET /api/ota/status` keeps the C++ shape, so a client sees a plausible idle status for an OTA that can never start. |
| ❌14 | **`test_maintenance_coordinator` equivalent** | `test/test_maintenance_coordinator/` (10 cases) | none | Shot qualification by time-OR-weight, threshold boundary, persistence — the whole reminder feature is untested *and* broken (see ⚠9). |

### ⚠️ PARTIAL / BEHAVIOUR DIFFERS — 21 items

| # | Feature | C++ | Rust | Note |
|---|---|---|---|---|
| ⚠1 | `GET /api/status` field rename | `steamMode` (`WebServerManager.cpp:444`) | `brewing` (`web.rs:969`) | **Undeclared.** Any UI or script reading `steamMode` silently gets `undefined`. |
| ⚠2 | `/api/parameters` method handling | `HTTP_ANY` + explicit `405` arm (`:813`) | GET/POST registered only (`web.rs:1582,1630`), no catch-all | `PUT`/`DELETE` → 404 instead of 405. |
| ⚠3 | MQTT registered parameters/sensors | ~30 params + 12 sensors (`SystemInitializer.cpp:687-800`) | 3 sensors + 2 params + `pidON` + binaries (`mqtt.rs:256-296`) | Moot until ❌1 is fixed. |
| ⚠4 | `shots_since_backflush` never incremented | `MaintenanceCoordinator::recordBrewIfQualified` (`MaintenanceCoordinator.cpp:30-49`) reached from `BrewStates.cpp:306-312` | `Effect::RecordBrew` emitted (`states.rs:190`, correct parity) then **dropped**: `SideChannels::on_record_brew` has an empty default (`applier.rs:99`) and `FirmwareSide` (`actuators.rs:826-843`) does not override it | `rg 'shots_since_backflush' crates/` → 14 hits, **no `+=` anywhere**. `/api/status.shotsSinceBackflush` = 0 and `backflushReminderDue` = **false, permanently**. The C++ also persists the count to NVS; Rust has no persistence at all. **Undeclared.** |
| ⚠5 | OLED backflush reminder | `DisplayWidgets.h` `displayMaintenanceStatusBar` | `widgets.rs:648` gates on `input.backflush_reminder_due`, which `main.rs` **never assigns** | Dead for a second, independent reason. Undeclared. |
| ⚠6 | `machineState` numeric in `/api/status` | int (`MachineStateIds.h`) | int | OK. |
| ⚠7 | Bitmap count | 12 (`display/bitmaps.h:12-27`) | 11 (`cc-display/src/bitmaps.rs:47` `ALL: [Bitmap; 11]`) | One bitmap not ported. Which one needs a human answer. |
| ⚠8 | `POST /api/sleep` from `PID_DISABLED` | timer-only (`PidStates.cpp:112-146`) | honoured (`guards.rs` PidDisabled arm) | 🗑️ **Declared** (`intentional-diffs.md §13`). Justification **holds** (recorded hardware regression). Keep. |
| ⚠9 | Maintenance coordinator semantics | time ≥ 5 s **OR** weight ≥ 10 g, persisted | `maintenance.rs:43 qualifies_as_counted_shot` implemented, but only `#[cfg(test)]` callers | The rule exists and is unit-tested, and is called from nowhere in production. |
| ⚠10 | `GET /api/ota/status.status` | int | string | 🗑️ **Declared** (`intentional-diffs.md §14`). Justification holds. |
| ⚠11-21 | Telnet structural change (ring → line buffer), log levels/format, `number2string` thread_local buffers dropped for owned `String`, `round2` parity (tested via `fmt_parity.rs` against an 80,000-line oracle file), `computed.*`/`state.*` keys absent (19 keys — **verified dead in C++ too**: `Config.cpp:408-425` `getAllStateParams` is entirely commented out, zero callers) | — | — | Individually small; listed for completeness. |

### ✅ EQUIVALENT — cleared, do not re-review

All 18 states and IDs; `StateMachine` transition semantics; `StateFactory` name map; **`BaseState::checkTransitions` order and both exclusion lists** (`guards.rs:70-113`, `guards.rs:129`); **`valveSafetyShutdownCheck` whitelist** (`cc-safety/src/lib.rs:439-458`, matches `BrewHandler.h:105` including the `BREW_FINISHED` and `BACKFLUSH_IDLE/FINISHED` exclusions, expressed as an exhaustive `match` so a 19th state is a compile error); `ProcessController::shouldPIDBeEnabled` (`guards.rs:173-178`); `EmergencyStopManager` debounce (3 readings, immediate on invalid, clear ≤ 100 °C — pinned by `cc-safety/tests/safety_paths.rs`); `HardwareManager` pump guards + water-tank interlock + `emergencyShutdown` vs `safeHardwareShutdown` distinction; the shared-valve `ValveState` arbitration; ISR heater PWM via GPTimer (10 ms); watchdog 5 s; power long-press reboot (`handlers.rs:353` — note: README says "no hand-pressed switch" is done, which is **stale**); all 40 non-`state.*`/`computed.*` config keys with matching defaults and ranges (**zero Rust-only keys**); 21 of 25 HTTP routes; 6 display templates; 3 languages; 4 fullscreen modes; 600-point history; DS18B20 11-bit + sentinel handling; TSIC-306 222/221/180 sentinels; HX711 dual-cell sum; ABP2 conversion; Wi-Fi reconnect/backoff; telnet port 23 + banner + heartbeat + the 30 KB heap-shed floor.

**Score: 158 OK · 21 ⚠ · 14 ❌ · 3 declared-🗑️.**

---

## 5. Findings by severity

### Critical

**CR-1 · safety · verified · the `Cell<T>` seqlock is a live use-after-free**
`crates/cc-hal-esp32/src/web.rs:362-435` (definition + `set` + `get`), `crates/cc-hal-esp32/src/network.rs:449`, `crates/cc-firmware/src/main.rs:2868`.
```rust
struct Cell<T: Clone> { seq: AtomicUsize, value: UnsafeCell<T> }   // web.rs:362
*self.value.get() = value;                                          // web.rs:399  non-atomic write
let copy = unsafe { (*self.value.get()).clone() };                   // web.rs:424  non-atomic read
pub ip: Option<String>,                                              // web.rs:268  heap-owning payload
slot.ip = sta.ip().map(|ip| format!("{ip}"));                         // network.rs:449, EVERY 10 ms TICK
```
`publish_radio` runs unconditionally from the control tick (`main.rs:2868`), allocating a fresh `String` and **dropping the previous one** — i.e. `free()`ing the buffer the httpd task may be `memcpy`-ing from inside `clone()`. A torn `{ptr, len, cap}` read is a use-after-free at 100 Hz. The SAFETY comment's claim that "a torn read is detectable" is the classic seqlock fallacy: detection happens *after* the invalid read. Rust's memory model makes any data race on a non-atomic UB regardless of the retry.
**Impact:** memory unsafety on the httpd task, reachable from any HTTP request, on a machine that also drives a pump and a heater.
**Fix:** make the payload `Copy` POD — `ip: Option<heapless::String<15>>` — and the seqlock becomes sound. Better still, publish the whole `Telemetry` through a 3-slot ring whose index the producer never laps the consumer on (`slots.rs:135-147` correctly argues the producer *does* lap the consumer 9×/frame, so naive double-buffering is also racy). Do this **today**.
*Note: `crates/cc-firmware/src/slots.rs:105-160` is the same pattern but its payload `FrameRequest` is `#[derive(Clone, Copy)]` with no `String`/`Vec`/`Box` (`rg 'String|Vec<|Box<' cc-display/src/model.rs` → 0), so it is formally UB but not exploitable. `cargo miri` is not installed and would not model Xtensa/FreeRTOS.*

**CR-2 · build · verified · a `v*` tag on this branch publishes the C++ firmware**
`.github/workflows/release.yml:3-6,58,64,72,116-119`.
```yaml
on:
  push:
    tags:
      - "v*"          # no branch filter, no workflow_dispatch, no path filter
...
- run: pio run -e esp32_usb                 # :58
- run: pio run --target buildfs -e esp32_usb  # :64  -> littlefs.bin
```
Release notes tell users to flash `littlefs.bin` at `0x350000`; `partitions_4M.csv:6` defines that region (`spiffs,data,spiffs,0x350000,0xA0000`) but `rust/partitions_4M.csv:20` places `littlefs` at `0x390000` — **no region at 0x350000 exists in the Rust layout**. `git diff --stat main -- src include lib platformio.ini partitions_4M.csv` is empty, so the C++ genuinely still builds and the publish succeeds.
**Fix:** add `if: github.ref == 'refs/heads/main'` to the release job. Until then, **do not tag from this branch**.

### High

**H-1 · build · verified · zero CI for the Rust workspace**
`rg 'cargo|just ' .github/` → **0 hits**. `main.yml`, `format.yml`, `release.yml`, `frontend.yml` run only PlatformIO and pnpm. `just gate` (`justfile:240-248`) is invoked by nothing. `main.yml`'s `firmware` job goes green purely because the untouched C++ still compiles.
`docs/rust-migration/05-tooling-and-workflows.md:479` specifies `.github/workflows/rust.yml`; it was never written.

**H-2 · safety · verified · `just gate` cannot pass on a clean checkout**
`crates/cc-hal-esp32/build.rs:46,58-60,293-305` panics when `ui/packages/frontend/dist` is absent; `rg -n 'pnpm|frontend|dist' justfile just/*.just` → no build step; `frontend.yml:10-15` gates on `paths: ["ui/**"]`, builds into a job that uploads nothing, and lives in a different workflow. `.gitignore` in `ui/packages/frontend/` excludes `dist`, so it is never committed.
*Refutation attempted and partly granted:* `docs/rust-migration/size-records.jsonl` records `{"label":"f25-web-ui-embedded","image_bytes":1559520,…}` at git `0815710`, so the gate **has** been run end-to-end on a machine with the UI built. The correct characterisation is "cannot pass from a clean checkout", not "never run".

**H-3 · toolchain · verified · `just test` fails on every non-Apple host**
`justfile:73` `host_target := env_var_or_default("CC_HOST_TARGET", "aarch64-apple-darwin")`; `just --evaluate host_target` → `aarch64-apple-darwin` on this x86_64-Linux box; `just test` → `cc: error: unrecognized command-line option '-arch'`. `just doctor` prints the triple but never compares it. `target/aarch64-apple-darwin/debug/` exists as residue.

**H-4 · parity · verified · MQTT outbound is entirely unwired** — see ❌1/❌2. `/api/status.mqttConnected` is not merely stale: `esp_mqtt_client_start` is asynchronous, so the single read of `ever_connected()` at `main.rs:938` can never be `true`, and it is then fed into telemetry forever (`main.rs:2843` → `web.rs:995`).
*Severity note from the verifier:* `mqtt.enabled` defaults to `false` (`config.rs:1039`), so this is opt-in and has no safety impact → **High, not Critical**. But it is not "a 5-line omission": the client must move into the control task, be driven from the tick, and be given a cursor over the registry. `mqtt.rs:256-258` even says parameter binding waits on R3-16. **Undeclared.**

**H-5 · parity · verified · the backflush reminder is dead** — see ⚠4/⚠5. Two independent causes: the counter is never incremented, and the display input field is never assigned. Also the C++ persisted the count to NVS; Rust has no persistence.

**H-6 · parity · verified · `/api/config/upload` and HTTP auth are missing** — see ❌3/❌4. `system.auth.*` are writable and readable via the API and inert; the frontend has a live upload button that will 404; `routes()` advertises a `Method::Options` handler that is never registered.

**H-7 · perf · verified · `parameters_json()` runs every 10 ms, not every second**
`main.rs:155` `const CONTROL_PERIOD_MS: u32 = 10;` → `main.rs:121` `const HEARTBEAT_MS: u32 = CONTROL_PERIOD_MS;` → gate at `main.rs:2624`.
The comment directly above the gate (`main.rs:2621`) says *"at 2.5 ticks a second that is 245 copies a second"* — a **stale leftover from when the tick was 400 ms**, which self-documents the regression. Measured: **420 allocations and 20.5 KB per call** (4 `String`s per schema entry × 98 params, plus one 15,680-byte body) → **42,000 allocations/s and ~2 MB/s of allocator churn from the control task**, next to the heater deadman.
**Do not simply change `HEARTBEAT_MS`:** it also gates the boot-log text (`main.rs:2048`) and feeds `const _: () = assert!(HEARTBEAT_MS * 2 <= DEADMAN_TIMEOUT_MS)` at `main.rs:128-131`, where `DEADMAN_TIMEOUT_MS == 1000` — setting it to 1000 makes `2000 <= 1000` a **compile error**. Add `const PARAMETERS_PUBLISH_MS: u32 = 1_000;` and gate on that instead. Undeclared in `intentional-diffs.md`.

**H-8 · perf · verified · `Vec<Effect>` churns 4 allocations per control tick**
`cc-machine/src/lib.rs:129-138` (`reduce → (Machine, Vec<Effect>)`, `Vec::new()`), `handlers.rs:54`, `states.rs:436`, `cc-firmware/src/control.rs` (a second `Vec`), `main.rs:2166` (a third). Measured **4.00 allocs/tick, ~192 B/tick, ~96 ns/tick** in a `Control::tick`-shaped harness. The C++ wrote relays inline into member state with zero allocation. `heapless` is a dependency of `cc-hal-esp32` only — **not of `cc-machine`, where the hot path needs it**.
**Fix:** `type Effects = heapless::Vec<Effect, 16>;` and thread `&mut Effects` through `reduce`/`states::update`/`handlers` (~30 call sites, mechanical). Measured `Vec<Effect>` is bounded at 3–4 elements in `PID_NORMAL`, so 16 is generous.

**H-9 · taskrunner · verified (partially refuted) · `just reflash` erases and never reflashes**
`justfile:364-368`. The stray `@` on the last line of a shebang recipe is **not** stripped by just. Reproduced in `/tmp`: `@just: command not found`, **exit 127** — so the original "exits 0" claim is **wrong** and the failure is loud. The real defect: `reflash` is the **only** shebang recipe without `set -euo pipefail`, so the erase is not rolled back and the operator is left with an erased chip and a confusing message. `rg '#!/usr/bin/env bash'` → 11 sites; only `:368` has the stray `@`, so it is an isolated instance. **Medium-High, not Critical.**

**H-10 · build/compliance · verified · U8G2 BSD-2-Clause data committed with no licence notice**
`crates/cc-display/src/font/data.rs:1-30` (2,807 lines of verbatim U8G2 RLE font streams) and `bitmaps_data.rs`. `rg -i 'bsd|licen|copyright|redistribut'` on both → **0 hits**; `git ls-files | rg -i notice` → nothing; the only `LICENSE` is the repo's own GPL-3.0. BSD-2-Clause §3 requires the copyright notice and disclaimer to be retained in source redistributions. **This is a real compliance defect, not a nit.**
**Fix:** add the verbatim U8G2 notice to `data.rs`, a NOTICE line to `bitmaps_data.rs`, and `docs/THIRD_PARTY_LICENSES.md`; make `extract_fonts.py` emit the header so regeneration keeps it.

### Medium

**M-1 · build · verified · `.cargo/config.toml` breaks host tooling and wastes ~2 GB**
`[build] target = "xtensa-esp32-espidf"` (`:8`) and `[unstable] build-std` (`:28`) apply to every bare `cargo` call. `cargo check -p cc-safety` with no `--target` → `E0463: can't find crate for core`. Host tests rebuild `std` from source (9 `rustc_std_workspace` rlibs, 258 MB for a single fresh `cargo test -p cc-safety`); `target/x86_64-unknown-linux-gnu` = 1.5 GB, `target/aarch64-apple-darwin` = 477 MB of std cross-built for an Apple triple from Linux. **All 8 device recipes already pass `--target` and `-Zbuild-std` explicitly**, and no build script reads `TARGET` (`rg 'TARGET|env::var' crates/*/build.rs` → only `OUT_DIR`), so deleting both keys breaks nothing. **Keep `build-std-features`** (`:38`) — it is the only `[unstable]` key with no CLI equivalent, and it is what keeps gimli/addr2line out of the image.
*Refutation note:* the comment at `:33-34` claiming `[unstable]` is "ignored with a warning" on stable cargo is **wrong** — it is silently ignored (`RUSTUP_TOOLCHAIN=stable cargo tree` → exit 0, zero stderr), which is worse.

**M-2 · build · verified · MSRV 1.82 is unverifiable and vacuous**
`Cargo.toml:56` `rust-version = "1.82"`. The only toolchain is `channel = "esp"` = `rustc 1.97.0-nightly`, forced for host recipes too by `justfile:55`. Nothing has ever been compiled with 1.82. Either drop it or add a real MSRV job.

**M-3 · docs · verified · 83 `cargo doc` warnings, and no gate runs it**
`cc-domain` 43, `cc-display` 19, `cc-machine` 17, `cc-config` 4. All `rustdoc::broken_intra_doc_links`, e.g. `cc-machine/src/context.rs:11` (`Event::SensorUpdated`), `cc-machine/src/effect.rs:14` (`crate::applier::Applier`), `cc-domain/src/heater.rs:326` (`cc-hal-esp32::heater` — a device crate that is never in scope for a portable crate's doc build, so this one is dead by construction). `missing_docs = "warn"` is never promoted to `deny`. `rg 'cargo doc' justfile .github/workflows/` → 0.

**M-4 · over-engineering · verified · `cc-provisioning` is 17 lines of doc comment and 4 dependencies**
`crates/cc-provisioning/src/lib.rs` = 17 lines, all `//!` and `#![no_std]`. `rg cc_provisioning` → **0 hits** repo-wide. But it is a workspace member, `justfile:80` puts it in `dev_crates` for 3 device clippy passes, and `cc-firmware/Cargo.toml:32` depends on it — so every device build compiles a graph for a crate with no code and no consumer. **Delete it** (−17 LOC, −4 dep edges); re-add at R4-11.

**M-5 · over-engineering · verified · ~170 LOC of provably-dead `LedcPwm`**
`crates/cc-hal-esp32/src/heater.rs:303-435,664-679,702-720`. `LedcPwm::new(` → 0 construction sites; `new_ledc` → 1 hit (the definition). Self-documented as *"Not brought up, and not bringable on this chip"*. Because it is the second `HeaterDuty` impl, its presence makes a 1-impl trait look justified. **Delete it, `new_ledc`, and collapse `HeaterDuty` to `TimerIsrPwm`.**

**M-6 · over-engineering · verified · two traits with zero impls**
`cc-domain/src/sensor/probe.rs:219` `TemperatureProbe` and `cc-display/src/templates/mod.rs:236` `Template`: **0 impls, 0 uses as a bound or `dyn`** anywhere in the workspace. Worse, `cc-domain/src/sensor/mod.rs:35` documents a `&mut dyn TemperatureProbe` that `rg` cannot find — the doc is false. `Template`'s own rationale ("no virtual table is free") argues against its own existence; `render.rs:876` is a plain `match TemplateId`. **Delete both.**

**M-7 · dry · verified · three hand-maintained 98-key tables where C++ had one**
`cc-config/src/{schema.rs:268-965, assign.rs:217-394, json.rs:344-503}`. Each key appears 3× (SCHEMA, `set()`, `live_value()`); only two tests hold them together. The C++ has **one** table (`Config.cpp:438-563`, 96 `ConfigParamDef*` each carrying key+default+min+max+virtuals). Adding a key is 3 edits and a missed one is a silent `/api/parameters` gap.
**Fix:** put the accessor pair on `ParamSpec` (`get: fn(&Config)->LiveValue`, `set: fn(&mut Config,&LiveValue)->bool`). Kills ~1,030 lines and the drift risk.

**M-8 · perf · verified · `publish_radio` allocates 2× and does 4 FFI round-trips every 10 ms**
`cc-hal-esp32/src/network.rs:435-454`, called unconditionally at `main.rs:2868`. Its own comment at `:432` says *"the control task writes once per CONTROL_PERIOD_MS and the radio once per second"* — the second half is wrong; the caller is the tick. This is also the **write side of CR-1**. Move it under the existing `wifi_last_ms` (1 s) gate and cache the formatted `Ipv4Addr`.

**M-9 · perf · verified · `/api/parameters` clones an 8.8 KB body per request under a lock**
`cc-hal-esp32/src/task.rs:249` `live()` → `self.live.lock().ok().and_then(|s| s.clone())`, consumed at `web.rs:1584`, on the httpd task whose stack is 8 KB. **Fix:** publish an `Arc<String>` from the control task and clone the `Arc` — no mutex, no 8.8 KB copy.

**M-10 · design · verified · `SideChannels` has 14 methods and the production impl overrides 5**
`cc-machine/src/applier.rs:89-123`; `cc-hal-esp32/src/actuators.rs:826-843` implements only `on_enter_state`, `on_exit_state`, `on_pid_runtime`, `on_steam_mode`, `on_request_reboot`. Nine emitted effects are silently dropped — which is exactly how H-5 shipped. **Split** into `Actuators` (mandatory), a mandatory `SideChannels` (record-brew, wake, log, snapshot), and cosmetic events behind `fn events(&mut self) -> Option<&mut dyn Diagnostics>`; or minimally `debug_assert!` in `apply()` that the impl covers every emitted variant.

**M-11 · perf · verified · blocking waits inside the single-task ESP-IDF httpd**
`cc-hal-esp32/src/task.rs:315` polls `delay_ms(5)` up to 1,600 ms; `web.rs:571` waits up to 400 ms. `web_async.rs:22-30` states plainly that *"ESP-IDF's httpd is one task for the whole server"* — so a `POST /api/parameters` stalls every other route for up to 1.6 s. The doc admits the C++ had no such wait. In practice it is ~10–20 ms (one tick); the *bound* is what is wrong.

**M-12 · taskrunner · verified · three justfile defects worth fixing now**
`just bench` names bench targets that do not exist (`:457-461`); `just size-bench` points at `scripts/parity/loop-timer.sh`, which does not exist (`:463`); `just/size.just:11-12,115` accepts an `mcu` argument but uses it only in the error text, so `just size-record esp32s3 x` silently measures the esp32 image.

**M-13 · taskrunner · verified · mise/justfile hygiene**
`espflash` and `cargo-espflash` are **the same tool from two crates.io packages** (`cmp` on the two `.crates.toml` → different) and the justfile uses both spellings; `espup` has two owners (`"cargo:espup" = "latest"` and `cargo install --force … || true` in a task); `pnpm`/`just` are `"latest"` with two pnpm majors installed; `espup install --targets esp32,esp32s2,esp32s3` pulls ~GB of GCC for deferred targets; the pyserial-discovery loop is copy-pasted 3× (`justfile:209,399,428`); `format.yml:40`'s `mise-action` installs four Rust tools to run one `pio` command; `.pre-commit-config.yaml:19` pins clang-format **17** while `.mise.toml:8` pins **23.1.1**.

**M-14 · build · verified · checked-in Python bytecode**
`crates/cc-display/tools/__pycache__/extract_fonts.cpython-314.pyc` is tracked; `git check-ignore -v` shows no `__pycache__` rule (unlike `cc-domain/tools/pid_oracle/`, which has a `.gitignore`). The header of `font/data.rs` also says "regenerate with `tools/run.sh`" — `run.sh` is at `tools/oracle/run.sh`, and the `--check` CI guard it advertises is wired into nothing (`rg extract_fonts justfile .github` → 0).

**M-15 · test · verified · the two `unsafe` seqlocks have no tests at all**
`rg -c '#\[test\]' crates/cc-hal-esp32/src/web.rs crates/cc-firmware/src/slots.rs` → **zero**. These are the only hand-written concurrency in the firmware and neither the torn-read path, the `last_good` fallback, nor the odd/even retry is exercised. This is host-testable today: `AtomicUsize` + `UnsafeCell` are portable, and a two-thread producer/consumer publishing thousands of frames would have caught CR-1.
**Related:** `rg 'proptest|quickcheck|fuzz|criterion' crates Cargo.toml` → **0 hits**. The four parsers that take hardware bytes (TSIC-306 waveform decode, OneWire CRC, DS18B20 scratchpad, JSON config import) have no property or fuzz tests, despite the TSIC decoder's own docs arguing correctness from "every check is a bound".

**M-16 · test · verified · `docs/example_config.json` is not actually covered by an import test**
`AGENTS.md` states "an import test parses that file". `cc-config/tests/config_schema.rs:667-700` **inlines a hand-copied subset** labelled *"A representative subset of docs/example_config.json"*; `rg 'include_str!.*example_config'` → 0. That file is the user-facing download. **Fix:** `include_str!("../../../docs/example_config.json")`.

**M-17 · test · verified · one C++ suite has no Rust counterpart**
`test/test_maintenance_coordinator/` (10 cases: shot qualification by time **or** weight, threshold boundary, persistence, disabled-reminder still counts) → none. The feature it covers is also broken (H-5), so the two facts compound.

**M-18 · safety · verified · an `unreachable!` on an HTTP-reachable path, guarded only by a comment**
`cc-config/src/json.rs:644` `unreachable!("{key} is not an enumeration parameter")`, reached from `assign::parse` (`assign.rs:183`), reached from `POST /api/parameters`. Unreachable today only because `SCHEMA` is the input; the invariant is enforced by nothing, and `panic = "abort"` means a violated assumption is a bricked machine. `cc-display/src/fmt.rs:107` and `cc-config/src/schema.rs:997` have the same shape.
**Fix:** return `Result`/`false` for the unknown-key arm, and add a test that iterates `SCHEMA`, filters `ParamKind::Enum`, and asserts the call does not panic.

**M-19 · safety · verified · `deny` + per-item `allow` rather than `forbid` + `expect`**
`Cargo.toml:122` sets `unsafe_code = "deny"`; all five portable crates correctly add `#![forbid(unsafe_code)]`. The device crates opt back in with `#[allow(unsafe_code, reason = "…")]` — and every one of the 12 sites carries a `reason`, which is unusually disciplined. But `deny` + per-item `allow` means a **new** `unsafe` that forgets the attribute is a hard error while one that *remembers* it is silent; `forbid` + `#[expect(unsafe_code, reason=…)]` would make the new-unsafe case equally loud and would let the `reason` become a reviewable assertion.

### Low

- **L-1** · `cc-hal-esp32/src/time.rs:63,93` and `heap.rs:51,65` use `// Safety:` (lowercase) where the other 16 sites use `// SAFETY:`. `clippy::undocumented_unsafe_blocks` is not in the workspace lint set, so nothing enforces the convention.
- **L-2** · `crates/cc-firmware/src/main.rs:2011-3104` — `control_task` is **1,094 lines in one function** (539 code / 524 comment). 18 numbered steps. Untestable in sections; the 1:1 comment ratio keeps the shape readable but not the testability. Split into `sense_step`/`decide_step`/`actuate_step`/`publish_step`/`pump_commands_step` over the existing `ControlArgs`.
- **L-3** · `crates/cc-hal-esp32/src/web.rs:822-908` — `respond_large` and `respond_download` share ~20 identical lines, and the doc at `:856` claims one is "reused rather than reimplemented" — it is not. `chunks(512)` is hardcoded at `:807` while `UI_CHUNK_BYTES = 512` exists at `:1203`.
- **L-4** · 14 `pub` items with zero callers (`cc-config/src/config.rs:1355,1366,1419,1425,1443`; `cc-display/src/widgets.rs:715,784,822,833,841`; `cc-display/src/display.rs:1196,1215`; `cc-domain/src/units.rs:203`; `cc-domain/src/pid.rs:391`; `tsic306/ring.rs:128`; `hx711.rs:187` — `NotReady`, a 1-variant enum never constructed). `ota_enabled`/`ota_password` are doubly dead: OTA returns 501.
- **L-5** · 27 `pub` items in `cc-hal-esp32` with zero references outside their own file; the crate has one consumer. 204 `pub` vs 3 `pub(crate)`.
- **L-6** · Three byte-identical `macro_rules! from_raw` copies (`cc-domain/src/{hardware,process,system}.rs`, md5 `a4afbd86…`). `macro_rules` has no crate-private export without `#[macro_export]`, which is why they were copied — hoist or derive from the enum.
- **L-7** · `cc-domain/src/heater.rs:1204-1311` `IsrChopper` is used only from `#[cfg(test)]` (~110 LOC); `AtomicChopper` is what ships. The `the_atomic_chopper_and_the_reference_agree_for_a_whole_window` test justifies keeping it — borderline, but note it is the one *earned* near-duplicate in the codebase.
- **L-8** · `cc-hal-esp32/src/network.rs:456-471` — the same doc comment duplicated twice on `broadcast_temps`.
- **L-9** · `docs/rust-migration/README.md` says "any hand-pressed switch" is not done, but `handlers.rs:353` `long_press_reboot` + `switches.rs:82,269-275` implement the power-switch long press. **The doc is stale**, not the code.
- **L-10** · `intentional-diffs.md:860` admits only the *inbound* MQTT command path is absent and asserts "the web commands are **not** in that state: they reach the sampling task" — while the entire *outbound* pass (publish, discovery, reconnect) is unwired and undeclared. The framing inverts the actual gap.
- **L-11** · `.espup-env.sh` (untracked, referenced by nothing, hardcodes `$HOME`) should be deleted before someone runs `git add -A`.

---

## 6. Over-engineering and removal candidates

| Item | Location | Est. LOC saved | What breaks |
|---|---|---|---|
| `cc-provisioning` crate + its 4 dep edges + the `dev_crates` entry | `crates/cc-provisioning/`, `Cargo.toml:24`, `justfile:80`, `cc-firmware/Cargo.toml:32` | 17 + 4 edges | Nothing (0 `use` sites repo-wide). Re-add at R4-11. |
| `LedcPwm` + its two impls + `HeaterOutput::new_ledc`; collapse `HeaterDuty` | `cc-hal-esp32/src/heater.rs:303-435,664-679,702-720`, `lib.rs:102` | ~190 + 1 trait | Nothing (0 construction sites). Rationale is already recorded in `intentional-diffs.md §9`. |
| `IsrChopper` (test-only reference) | `cc-domain/src/heater.rs:1204-1311` | ~110 | 9 `#[test]` bodies rewrite against `AtomicChopper`. Keep only if the cross-check test is judged valuable. |
| `TemperatureProbe` (0 impls) | `cc-domain/src/sensor/probe.rs:219-249` | 31 | Nothing. Also fix the false doc at `sensor/mod.rs:35`. |
| `Template` trait (0 impls) | `cc-display/src/templates/mod.rs:236-244` | 9 | Nothing; `render.rs` already dispatches on `TemplateId`. |
| 14 zero-caller `pub` items | see L-4 | ~200 | Nothing. |
| 2 of 3 `from_raw!` copies | `cc-domain/src/{process,system}.rs` | ~24 | Hoist to `cc-domain/src/lib.rs`. |
| `sockfd` / `finish` (`#[allow(dead_code)]`) | `cc-hal-esp32/src/web_async.rs:168,248` | ~14 | Nothing. |
| Tracked `.pyc` | `crates/cc-display/tools/__pycache__/` | binary | Nothing. |
| **Config accessor tables collapsed onto `ParamSpec`** | `cc-config/src/{assign,config/schema,json}.rs` | **~1,030** | Mechanical: `spec.get(config)` / `spec.set(config, v)`. Removes the 3-edit-per-key drift risk (M-7). |
| `SideChannels` split (M-10) | `cc-machine/src/applier.rs:89-123` | net ~0 | Mechanical, but it is what makes the H-5 class of bug impossible. |
| `cc-display/src/font/data.rs` | — | 0 | **Keep.** Verified byte-faithful: `extract_fonts.py:47-58` names exactly the 10 fonts `git grep 'u8g2_font_'` finds in the C++ — no 11th, no unused range. Only the licence notice is missing (H-10). |
| `tsic306/simulator.rs` (642 LOC) | — | 0 | **Keep.** `mod.rs:123-124` `#[cfg(test)] mod simulator;` — verified **not compiled into the firmware**. Correctly scoped. |
| `cc-parity` (4,018 LOC) | — | 0 | **Keep.** `std` crate, not in `default-members`, no crate depends on it — it cannot reach a pin, by construction. It is the migration's measuring instrument. |
| `docs/rust-migration/` (~8,000 lines) + `.agents/skills/` (~1,200) | — | 0 | **Keep.** It is what makes `intentional-diffs.md` auditable — provided the ledger is actually consulted (L-10 shows it sometimes is not). |
| **Total safely deletable now** | | **~575** | |
| **Total with the config-table collapse** | | **~1,600** | |

**No unused third-party dependency was found.** The 22 duplicated crate names in `Cargo.lock` resolve to **2** actually linked into the image (`embedded-hal` 0.2.7+1.0.0, `nb` 0.1.3+1.0.0), both forced by `esp-idf-hal` 0.47's dual support and not the port's choice. The other 20 are build-deps, proc-macros, or host-only. **This is a non-issue and should be recorded as such.**

---

## 7. What is done well (evidence only)

1. **The safety whitelist is compiled, not commented.** `cc_safety::water_flow_allowed` (`cc-safety/src/lib.rs:439-458`) reproduces C++ `BrewHandler.h:105` exactly, including the two traps (`isBrewState(BREW_FINISHED)` is true; `BACKFLUSH_IDLE`/`FINISHED` are not flowing), and is called by **both** the reducer (`cc-machine/src/lib.rs:231,248`) and the HAL interlock (`cc-hal-esp32/src/actuators.rs:297`). Both are exhaustive `match`es with no wildcard — **a 19th state becomes a compile error in two places**. This is the single best thing in the port: it collapses the C++'s worst duplication into one source of truth and makes a regression a build failure.
2. **`no_std` layering actually holds.** All five portable crates are `#![no_std]` (+`alloc`), zero `std::` in non-doc code, zero `esp_idf_*` names in code (CI-enforced per the workspace comment). Dependencies are `cc-domain` and `serde` only. They are genuinely host-testable, and ~1,100 host tests run in ~5 s.
3. **`forbid(unsafe_code)` is used in 5/5 portable crates** (`cc-domain/src/lib.rs:17`, `cc-safety:61`, `cc-machine:68`, `cc-config:64`, `cc-parity/src/lib.rs:34`) — `forbid`, not `deny`, so it cannot be re-allowed. Verified.
4. **The 12 `unsafe` sites that do exist each carry a `reason` string.** Verified by the reviewer that the `[workspace.lints] unsafe_code = "deny"` mechanism actually fires (reproduced in `/tmp`). An escape hatch with a written justification on every use is the right discipline.
5. **Error handling is `Result`-based with no `unwrap`/`expect`/`panic!` in reachable firmware code.** A `#[cfg(test)]`-aware scan found **3** real non-test hits total (the two `unreachable!` in M-18 and one provably-unreachable `expect`). The "229 hits" figure is dominated by `#[cfg(test)]` modules. `Box<dyn Error>` appears only in `fn main`/task entry points, never in a library API.
6. **Overflow is reasoned about, not hoped for.** `Millis::has_reached` uses `wrapping_sub` with the 24.8-day wrap argument written out (`units.rs:190-198`); `history.rs:111` uses `%` rather than a masked increment; `main.rs:3089` uses `saturating_sub` with a comment recording that the deadline form shipped a 49-day sleep. Every C++ `millis()` wrap site checked was found and documented — **and `overflow-checks = true` in the release profile means a missed one aborts loudly rather than corrupting**, which is the right trade for a safety-critical machine.
7. **Secrets are redacted at the type level.** `cc_domain::secret::Secret` makes `Debug`/`Display` print `[redacted]`, so `Pending { ssid: [redacted], … }` is the only way to print one. The UART password window is muted in the log sink (`provisioning.rs:15-24`). `scripts/wifi_provision.py` takes the credential from an env file, never from `argv`, and the justfile never echoes it.
8. **Cross-task messages are `Copy` by construction.** `Command` (`web.rs:276-278`) carries no `String`/`Vec`/`Box`; the list-carrying `POST /api/parameters` travels by a separate `ParameterHandoff` instead, **and the reason is written down**. `drain_body_bounded` (`web.rs:2617`) caps HTTP bodies rather than growing them — the right call on a 320 KB heap.
9. **Display rendering allocates zero.** `cc-display` uses `fixed_str::String<24>` for all number formatting, hand-matched to C's `printf` rounding and verified against an **80,000-line oracle file** (`tests/fmt_oracle.txt`) generated from the real U8G2. Better than the C++, which used `snprintf` into heap `String`. Display variance is in the *coordinates* (`modern_layout.rs`), not in copied draw calls — the 6 templates are correctly DRY.
10. **The device-test registry guard is real and load-bearing.** `scripts/device-test-audit.py` was tested with a negative control (unregistered test → exit 1, names the test) and a positive control (→ exit 0). It is wired as a *dependency* of `lint-esp32` and `test-esp32`, so it cannot be bypassed, and it caught the real "147 tests that never ran" failure mode. **This is the best piece of engineering-process design in the repo.**
11. **Build scripts and profiles are deliberate and correct.** `opt-level="s"` + `lto="fat"` + `codegen-units=1` + `panic="abort"` + `overflow-checks=true`; a separate `diagnostic` profile with identical codegen and `strip="none"` so a release-only panic can still be resolved; `build-std-features = ["std/backtrace-trace-only"]` to keep gimli/addr2line out of the image; `[target.xtensa-esp32-espidf] linker = "ldproxy"` with a written explanation of why. All three `build.rs` emit correct `rerun-if-changed`. `Cargo.lock` is committed. Pinning is coherent: exact `=` on esp-idf, caret-free elsewhere, **no `"latest"`, no git dependencies**.
12. **The image budget is tracked and respected.** 1,559,520 B against an 1,835,008 B app slot — the Rust image (which embeds the whole React UI) is **13 KB larger than the C++'s** and 275 KB under budget. That is a real achievement against a brief that called the slot the biggest risk.
13. **The image extraction is byte-faithful.** `git grep 'u8g2_font_'` on the C++ finds exactly the 10 fonts `extract_fonts.py:47-58` extracts — no 11th, no unused glyph range carried over. 11 of 12 bitmaps ported (⚠7).
14. **The divergences are written down.** `intentional-diffs.md` is a 1,245-line ledger with numbered entries, severity markers, C++ vs Rust excerpts, and a stated justification for each. Several of them are **correct calls a naive port would have got wrong** — arming both pump safety timeouts, deriving a steam-valve whitelist the C++ never had, gating the water valve on the tank, taking the PID derivative over real elapsed time. The problem is not the document; it is that L-10, H-4, H-5, H-7 and H-8 exist precisely because the ledger is not consulted exhaustively.

---

## 8. Prioritized action plan

Each group is independently shippable. **Bold** = quick win.

### Group 1 — Fence off the release path *(do before anything else; ~30 min)*
1. **`if: github.ref == 'refs/heads/main'` on the `release` job** in `.github/workflows/release.yml`. *(CR-2)*
2. **`git rm --cached crates/cc-display/tools/__pycache__/`, add `__pycache__/` to `.gitignore`.** *(M-14)*
3. **Delete `.espup-env.sh`; add the U8G2 BSD-2-Clause notice to `font/data.rs`, a NOTICE line to `bitmaps_data.rs`, and `docs/THIRD_PARTY_LICENSES.md`.** *(H-10, L-11)*

### Group 2 — Fix the memory-safety defect *(~half a day)*
4. **Make `Telemetry` `Copy` POD** — `ip: Option<heapless::String<15>>` — so the `Cell<T>` seqlock becomes sound; or replace it with a 3-slot ring. **Then** move `publish_radio` under the 1 s `wifi_last_ms` gate (fixes CR-1 *and* M-8 together). *(CR-1)*
5. **Extract the seqlock into a small generic and host-test it** with a two-thread producer/consumer publishing thousands of frames. *(M-15 — this is what would have caught CR-1)*
6. Switch `unsafe_code` from `deny`+`allow` to `forbid`+`#[expect(reason=…)]` in `cc-hal-esp32`/`cc-firmware`/`cc-device-tests`; normalise the 4 `// Safety:` sites and add `clippy::undocumented_unsafe_blocks = "deny"`. *(M-19, L-1)*

### Group 3 — Make the gate real *(~1 day)*
7. **Add `.github/workflows/rust.yml`** running `just fmt-check lint test parity-test test-audit build-esp32 size-check`, with the UI built first and `CC_HOST_TARGET` derived. *(H-1)*
8. **Add a `ui:` just recipe** and make `build-esp32`/`lint-esp32`/`size`/`size-check` depend on it. *(H-2)*
9. **Derive `host_target` from `os_family()`** and make `just doctor` *compare* it to `rustc -vV` and exit non-zero on mismatch. *(H-3)*
10. **Delete `[build] target` and `build-std` from `.cargo/config.toml`; keep `build-std-features`; fix the wrong "ignored with a warning" comment.** *(M-1)*
11. **Rewrite `reflash`'s last line** (`- just flash {{port}}`) and add `set -euo pipefail`; declare `set shell := ["bash","-euo","pipefail","-c"]` and drop the 11 shebangs. *(H-9, justfile fix 7)*
12. **Add `just doc` with `RUSTDOCFLAGS="-D warnings"` to the gate** and fix the 83 broken intra-doc links. *(M-3)*

### Group 4 — Fix the two dead user-facing features *(~1–2 days)*
13. **Move the MQTT client into the control task** and drive it from the tick: `publish_pass` on the 5 s/500 ms/10 s cadence, `subscribe`, `publish_online`, `due_for_reconnect`, `discovery_due`. Publish `mqttConnected` from the live client rather than a one-shot read. *(H-4)*
14. **Implement `SideChannels::on_record_brew` in `FirmwareSide`** (and split the trait so the next omission is loud). Assign `DisplayInput::backflush_reminder_due`. Persist the counter as the C++ did. *(H-5, M-10)*
15. **Add `POST /api/config/upload`** — the frontend button is live and will 404. *(❌3)*
16. **Either implement HTTP auth or delete `system.auth.*` from the schema.** A key an operator can set that does nothing is worse than an absent key. Register the `Method::Options` handler `routes()` already advertises, or remove the entry. *(H-6, ❌4)*
17. **Rename `brewing` back to `steamMode`** in `/api/status`, or add both fields and declare the rename. *(⚠1)*

### Group 5 — Control-loop resource hygiene *(~half a day)*
18. **Add `const PARAMETERS_PUBLISH_MS: u32 = 1_000;`** and gate `parameters_json()` on it — **do not** change `HEARTBEAT_MS` (it feeds a `const` assertion that becomes a compile error). *(H-7)*
19. **Thread one `heapless::Vec<Effect, 16>` through `reduce`/`states::update`/`handlers`.** Kills 3 of the 4 per-tick allocations. *(H-8)*
20. **Serve `/api/parameters` from an `Arc<String>`** instead of cloning 8.8 KB under a mutex per request. *(M-9)*
21. **Move the httpd's 1.6 s `stage_and_wait` off the single httpd task**, or at minimum lower the bound and document it. *(M-11)*

### Group 6 — Honest documentation and tests *(~1 day)*
22. **`include_str!` `docs/example_config.json` into the import test.** *(M-16)*
23. **Add `proptest` (dev-dep, host-only)**: arbitrary bytes into the TSIC-306 decoder must never panic; `raw_to_celsius ∘ celsius_to_raw` is the identity; `serde_json` from arbitrary input never yields an out-of-schema value. *(M-15)*
24. **Port `test_maintenance_coordinator`.** *(M-17)*
25. **Replace `json.rs:644`'s `unreachable!` with a `Result`** and add the `SCHEMA`-iteration test that pins it. *(M-18)*
26. **Update the stale entry-point docs** — `README.md`, `REPOSITORY_SUMMARY.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `DEBUG_GUIDE.md` all describe only `pio run`. Add a Rust section; mark PlatformIO as the C++ parity oracle. *(justfile fix 21)*
27. **Fix `docs/rust-migration/README.md`'s stale "no hand-pressed switch" claim** and correct `intentional-diffs.md:860`'s inverted MQTT framing. *(L-9, L-10)*

### Group 7 — Toolchain and simplification *(~1 day)*
28. **Apply the proposed `.mise.toml` and `rust-toolchain.toml`** in §3: drop `rust = "stable"`, drop the duplicate `espflash`/`espup` owners, pin `pnpm`/`just`, `--targets esp32` only, hoist the version pin into a named variable. *(M-13)*
29. **Delete `cc-provisioning`, `LedcPwm`, `TemperatureProbe`, `Template`, `IsrChopper`, and the 14 zero-caller `pub` items.** ~575 LOC. *(M-4, M-5, M-6, L-4)*
30. **Collapse the three config accessor tables onto `ParamSpec`.** ~1,030 LOC and it removes the "add a parameter in 3 places" drift risk. *(M-7)*
31. **Write the `reducers` and `layout` benches, or delete `just bench`.** Delete or implement `just size-bench`. Make `just size-record`'s `mcu` actually select the image. *(M-12)*
32. **Split `control_task`** (1,094 lines) into sense/decide/actuate/publish steps over `ControlArgs`. *(L-2)*

---

## 9. Open questions for the human

**Ambiguous C++ behaviour — decisions needed, not review findings**

1. **The C++ `currBrewTime` unit mismatch.** A's inventory flags that `BrewStates.cpp:98,260` stores `currBrewTime` in **milliseconds** while `brew.by_time.target_time` is in **seconds**, and `:285` compares them with no conversion. This looks like a genuine C++ bug. Which did the port implement — the bug or the intent? The answer determines whether a 25 s brew-by-time target stops at 25 s or 25 ms. The parity harness should say; if it says "the C++", that is a decision to record, not inherit.
2. **`waterTankCountsNeeded = 3`** (`types/GlobalTypes.h:129`) is declared but appears to have no readers. Was debouncing on the water tank intended and lost, or is 3 a leftover?
3. **The 19 absent `state.*`/`computed.*` config keys.** Verified harmless — `Config.cpp:408-425` shows `getAllStateParams` is entirely commented out and nothing calls it. Should `CONFIG_REFERENCE.md` and `docs/example_config.json` now list them, or note their absence?
4. **One bitmap of twelve.** B found `bitmaps.h` defines 12 and `cc-display/src/bitmaps.rs:47` carries 11. Which one was dropped, and was that intentional?
5. **The Logger ring.** ADR-0002 describes a 16 × 304 B ring and `telnet.rs` implements a line buffer. Was the simplification a deliberate call, or a leftover? The 30 KB heap-shed floor *was* kept, so this looks like an oversight rather than a decision.

**Decisions on intentionally dropped features — confirm or escalate**

6. **OTA (R3-15).** The three mutating endpoints return `501 {"reason":"R3-15"}` while `GET /api/ota/status` keeps the C++ shape. A client sees a plausible idle status for an OTA that can never start. Should the status endpoint also report `unavailable`, or is the current shape the right "declared gap" signal?
7. **The Acaia BLE scale (R3-18).** Dropped as "does not fit". Accepted, or does it need a decision record? The C++ `BluetoothScale.h` + `BluetoothScale.cpp` have no Rust counterpart at all.
8. **`system.auth.*` while auth is unimplemented.** This is **not** currently in the intentional-diffs ledger. Either it is an oversight, or it should be an entry. It is a security-relevant key set, so I would want it declared rather than inferred.

**Could not verify — needs hardware, a build, or a tool I do not have**

9. **On-device behaviour of the whole port.** No ESP32 was attached to this machine. Everything about runtime correctness — the 10 ms tick actually hitting its budget, the heater ISR chopping correctly, the seqlock under real FreeRTOS scheduling, the flash size — rests on `docs/rust-migration/size-records.jsonl` and on code review. **A CR-1 fix must be validated on hardware, not just on the host.**
10. **`just lint-esp32`, `just build-esp32`, `just diag-build`** — could not run: `cc-hal-esp32/build.rs:309` panics on the missing UI bundle, and a full ESP-IDF/embuild build is hours and gigabytes. So **device clippy cleanliness (`-D warnings`, `clippy::pedantic`) is UNVERIFIED by me**, though `just lint-esp32`'s `test-audit` dependency did pass (147 tests registered).
11. **`cargo udeps` / `machete` / `audit` / `deny` / `bloat` / `geiger` / `miri`** — not installed, not attempted. The manual dependency sweep found no unused *third-party* dep and only one unused *path* dep (`cc-provisioning`), which is reassuring but is not `cargo-udeps`. **`cargo audit` was never run against `Cargo.lock`; there is no advisory scan and no `deny.toml`.**
12. **Whether `HEARTBEAT_MS`-gated 100 Hz publishing is actually harmful on the LX6.** The allocator counts are exact and target-independent (≈42,000 allocs/s, ~2 MB/s). The CPU cost is host-measured (~16 µs/call) and would be roughly 15–25× worse on the LX6 — about 3% of a 10 ms budget. The fix is obviously right regardless, but the device-side magnitude is an estimate. `main.rs:2998-3062` already logs worst/mean/period for the tick; run it before and after fix #18.
13. **`C-01`'s exact exploitability.** The use-after-free is *structurally* present and the write happens 100×/s, but I did not demonstrate an actual crash — I read the code and the call sites. Whether it manifests depends on allocator timing. **Treat it as a live defect, not as a theoretical one, and do not wait for a crash report.**
14. **The remaining ~21 ⚠ parity items** are individually small and each was verified by reading; several are cosmetic (log format, `number2string` buffers). I would not spend review budget on them until Groups 1–5 are done.
