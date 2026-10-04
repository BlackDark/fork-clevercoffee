# Tooling and Developer Workflows

> **ARCHIVED — non-normative. Dated 2026-09/10, preserved for provenance.**
> The tooling it describes is now the tooling, and the tooling is the
> specification: the `justfile`, `just/size.just`, `mise.toml` and
> `.github/workflows/rust.yml` are what actually run. The recipes here were
> written before the R1-01 build and **three of them did not work as written** —
> the deviations are recorded in the `justfile` header, which is live. For what
> CI runs and what it costs, read [`docs/handbook/ci.md`](../../handbook/ci.md). Do
> not follow this file's commands. See [`docs/archive/README.md`](../README.md).

Everything the migration needs, installed **from the repository** via `mise`, driven by a
root `justfile`. No undocumented global installs.

Related: [03 — Decision record](./03-decision-record.md) (platform choice),
[02 §7](../../rust-migration/02-research-compatibility-matrix.md) (toolchain evidence),
[06 — Task list](./06-migration-task-list.md) (who does what, when).

---

## 1. What mise can and cannot own

| Component | mise? | How | Evidence |
| --- | --- | --- | --- |
| `just` | ✅ | built-in backend (`aqua:casey/just`) | `mise-versions.jdx.dev/tools/just` — 146 versions |
| `rustup` + a stable RISC-V toolchain | ✅ | built-in Rust backend; honours `MISE_RUSTUP_HOME` / `MISE_CARGO_HOME` to isolate from the system rustup | `mise.jdx.dev/lang/rust.html` |
| `rustfmt`, `clippy` | ✅ | Rust backend `components` | same |
| `espflash`, `cargo-espflash` | ✅ | `cargo:` backend (uses `cargo-binstall` for prebuilt binaries) | `mise.jdx.dev/dev-tools/backends/cargo.html`, page dated 2026-09-07 |
| `ldproxy` | ✅ | `cargo:ldproxy` — **it IS on crates.io (0.3.5)**. Do **not** curl the `esp-rs/embuild` GitHub release: its latest is v0.3.2 (2022) and its asset names use Rust triples, so the URL 404s. | crates.io; embuild releases |
| `espup` | ✅ | `cargo:espup` (downloads its own compiler artifacts) | crates.io |
| **`esp` (Xtensa) Rust toolchain** | ❌ **cannot** | `espup` writes to `~/.rustup/toolchains/esp`, outside mise's `MISE_RUSTUP_HOME`. mise has **no `esp`/`esp-idf` backend** | backends index and registry searched 2026-09-28, none found |
| **ESP-IDF** | not needed | `esp-idf-sys` downloads and configures ESP-IDF itself; `IDF_TOOLS_PATH` is explicitly ignored by it | `esp-idf-sys` README |
| C++ toolchain (existing) | ✅ | existing `espressif32` platform in `~/.platformio` | — |

**The one gap.** The Xtensa compiler is installed by `espup` into a rustup toolchain named
`esp`, and `export-esp.sh` must be sourced to set `LIBCLANG_PATH`, `CLANG_PATH`, and
`PATH`. mise cannot do either. The mitigation is a `mise.toml` **task** that runs
`espup install` and writes the exports to a repository-local file the `justfile` sources —
so the requirement is declared and reproducible in the repo, even if the install action
itself is a shell script. This is documented as a host-level exception, per the
requirement that host requirements mise cannot reasonably manage be written down.

`cargo:` backend caveat: it declares `get_dependencies → vec!["rust"]`, so `rust` must be
installed first, and setting `features` or `default-features = false` **disables**
`cargo-binstall` and forces a source build.

---

## 2. `mise.toml` additions

The existing `.mise.toml` is **extended, not replaced** — the `[settings]` block is kept.

> **Correction (2026-09-28).** An earlier draft said `ldproxy` ships *only* as a GitHub
> release asset and prescribed curling it. Both halves were wrong: `ldproxy` **is** on
> crates.io (0.3.5), and the embuild GitHub releases top out at **v0.3.2 (2022)**, which
> would install a four-year-old linker proxy next to a 2026 ESP-IDF build. The curl recipe
> also 404'd, because release assets are named by **Rust target triple**
> (`aarch64-apple-darwin`), not `uname` output (`darwin-arm64`). Use `cargo:ldproxy`.

```toml
min_version = "2024.1.0"

[tools]
  # --- existing C++ / frontend tooling (unchanged) ---
  node = "24"
  pnpm = "latest"
  python = "3.14.7"
  clang-format = "23.1.1"

  # --- Rust / migration tooling ---
  just                = "latest"
  rust                = "stable"      # host-side cargo tools. DEVICE toolchain is `esp`.
  "cargo:espflash"    = "4.6.0"
  "cargo:cargo-espflash" = "4.6.0"
  "cargo:espup"       = "latest"
  "cargo:ldproxy"     = "0.3.5"       # on crates.io; NOT the 2022 GitHub release

[settings]
experimental = true
jobs = 4

[tasks.xensa-toolchain]
description = "Install/refresh the esp-rs Xtensa toolchain — the one step mise cannot own"
run = [
  # NOTE: each array element is its OWN shell, so these must stay separate commands
  # and must not rely on variables set by a previous element.
  "cargo install --locked espup --force || true",
  "espup install --targets esp32,esp32s2,esp32s3",
  "printf '%s\\n' 'source \"$HOME/exports\"' > .rust-esp-env.sh",
]
```

`.rust-esp-env.sh` is git-ignored (generated). The `esp` toolchain must be installed into
the **same `RUSTUP_HOME` mise uses** — `espup` otherwise writes to
`~/.rustup/toolchains/esp` while `rust-toolchain.toml`'s `channel = "esp"` resolves
against mise's rustup home, and cargo then tries to download a nonexistent upstream `esp`
toolchain. If `rustup run esp rustc --version` fails after setup, that is the cause;
`just doctor` asserts it.

**Alternative if the two-homes problem proves fragile:** drop `rust` from `[tools]` and use
a plain rustup install. The device toolchain is what matters; the host Rust only installs
cargo tools.

`rust-toolchain.toml` in the repo pins `channel = "esp"`, so `cargo` in the repo always
selects the right toolchain and the global `rustup default` does not matter.

`ldproxy` must match the `embuild` release the pinned `esp-idf-sys` wants.
`esp-idf-svc = "=0.53.0"` pins `embuild = "0.33.5"`; `ldproxy` 0.3.5 is current. R2-01
re-verifies this rather than establishing it.

---

## 3. Flashing: the original ESP32 is not the S3

**This is the most likely place for a future agent to break the workflow.** The dev board
in use is an ESP32-DevKitC-V4, which has **no native USB**. The USB cable is a
CP210x-class bridge on UART0, and the auto-reset circuit is DTR→GPIO0, RTS→EN through two
transistors.

| Chip | USB | `espflash` path | Reset method |
| --- | --- | --- | --- |
| **ESP32 (in use)** | none — UART bridge | esptool protocol over `/dev/cu.usbserial-*` | auto (DTR/RTS) **if** the board has the 100 nF EN↔GND cap |
| ESP32-S3 | native USB-Serial-JTAG | USB-JTAG, or esptool over the bridge | native |
| ESP32-C6 | native USB-Serial-JTAG | USB-JTAG, or esptool over the bridge | native |

`espflash` speaks the esptool protocol for the original ESP32, so it works over a plain
bridge — but cheap DevKitC boards without the EN↔GND capacitor famously require a manual
**BOOT + RST** dance. `espflash hold-in-reset` and `espflash list-ports` exist for exactly
this. **R2-01 must confirm this on the physical board before anything is flashed
automatically.**

Safety rules baked into every recipe:

- **The port must be named explicitly.** No recipe globs `/dev/cu.*`; `list-ports` is a
  separate diagnostic recipe. Flashing the wrong device destroys a machine.
- **The MCU is a variable, not a default.** `just flash /dev/cu.usbserial-110`.
- `flash` and `erase-flash` are separate recipes, and `erase-flash` requires the same
  explicit `PORT` plus a typed confirmation.
- Partition tables are **not** set via `sdkconfig.defaults`. They are flashed explicitly
  with `--partition-table partitions_4M.csv`, or auto-detected by `cargo espflash` from the
  build script. Setting `CONFIG_PARTITION_TABLE_CUSTOM=y` is documented as **not working**
  and a relative `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` **breaks the build**
  ([esp-idf-sys#395](https://github.com/esp-rs/esp-idf-svc/issues/395)).

---

## 4. `justfile`

> **Validated.** The recipes below were run against `just` 1.58.0 before being written
> down. Three constructs in an earlier draft were invalid and are now avoided:
> `{{ fn(args) }}` (just has no user functions — use module-level variables), a recipe
> dependency with arguments (`build-esp32: build mcu="esp32"` is a parse error), and
> `KEY=value` arguments (that is `make` syntax; just passes `MCU=esp32` through as a
> literal string).

### Argument convention

**Positional arguments only.** `just flash esp32 /dev/cu.usbserial-110`.
`KEY=value` is *not* just syntax and would silently pass the literal string
`MCU=esp32` to `espflash` at the moment an agent writes to real hardware.

`PORT` is required and has **no default**, so `just flash` alone fails with a usage
message instead of guessing.

### Module-level variables

```makefile
# Device build knobs. These are the SINGLE source of truth: the CI workflow, the
# mise task, and .cargo/config.toml must all agree with them. `just doctor`
# asserts they are set.
export ESP_IDF_VERSION := "v5.5.5"
export RUSTFLAGS       := "--cfg espidf_time64"

mcu_esp32   := "esp32"
mcu_esp32s3 := "esp32s3"
mcu_esp32c6 := "esp32c6"

tgt_esp32   := "xtensa-esp32-espidf"
tgt_esp32s3 := "xtensa-esp32s3-espidf"
tgt_esp32c6 := "riscv32imac-esp-espidf"

# `cargo test --workspace` and `cargo clippy --workspace` do NOT work on the host:
# the three device crates do not compile for a host target (04 §6). These lists
# are the portable crates only.
host_crates := "-p cc-domain -p cc-safety -p cc-machine -p cc-display -p cc-config"
```

### Set the repo up

```makefile
# Show what mise will install and assert the device build knobs are set.
doctor:
    @mise doctor
    @mise ls
    @test -n "{{ ESP_IDF_VERSION }}" || (echo "ESP_IDF_VERSION unset"; exit 1)
    @echo "ESP-IDF pinned: {{ ESP_IDF_VERSION }}"
    @echo "RUSTFLAGS:      {{ RUSTFLAGS }}"
    @rustup run esp rustc --version || (echo "the esp Xtensa toolchain is missing — run: just setup"; exit 1)

# First-time setup, in the right order.
setup:
    mise trust
    mise install                    # node, pnpm, python, clang-format, just, rust, espflash, ldproxy
    mise run xensa-toolchain        # the one step mise cannot own (see §1)
    @just doctor
```

### Format and lint

```makefile
fmt:                                ## Format all Rust sources
    cargo fmt --all

fmt-check:                          ## CI gate: formatting
    cargo fmt --all -- --check

lint:                               ## Clippy (host crates), warnings are errors
    cargo clippy {{ host_crates }} --all-targets -- -D warnings

dev_crates := "-p cc-hal-esp32 -p cc-firmware"

lint-esp32:                         ## Clippy for the production device target
    MCU=esp32 cargo clippy {{ dev_crates }} --all-targets \
        --target {{ tgt_esp32 }} -Zbuild-std=std,panic_abort -- -D warnings

lint-esp32s3:                       ## Only after R4-07
    MCU=esp32s3 cargo clippy {{ dev_crates }} --all-targets \
        --target {{ tgt_esp32s3 }} -Zbuild-std=std,panic_abort -- -D warnings

lint-esp32c6:                       ## Only after R4-08
    MCU=esp32c6 cargo clippy {{ dev_crates }} --all-targets \
        --target {{ tgt_esp32c6 }} -Zbuild-std=std,panic_abort -- -D warnings
```

> **On `clippy::pedantic`:** enabled via `[workspace.lints]` in `Cargo.toml`
> (`-D clippy::pedantic`), which is the normative statement — 04 §1.7. Do **not** also
> pass `-D clippy::pedantic` on the command line; the two are not equivalent and
> double-flagging hides the real config. During the initial port, `pedantic` findings
> are triaged per crate. A **named module-level** `#[allow(clippy::pedantic)]` with a
> justification comment is acceptable during the port; a crate-root blanket allow is a
> CI failure.

### Test

```makefile
test:                              ## Host tests for every portable crate
    cargo test {{ host_crates }}

test-domain:                       ## Just the pure domain + safety crates
    cargo test -p cc-domain -p cc-safety

snapshot-display:                  ## Regenerate OLED golden images (host)
    cargo test -p cc-display -- --ignored render_goldens
```

### Build

`just` cannot parameterise a recipe dependency, so there are three explicit recipes
rather than one parameterised `build` plus aliases.

```makefile
build-esp32:                        ## Build for the production target
    MCU=esp32 cargo build --release -p cc-firmware --bin firmware \
        --target {{ tgt_esp32 }} -Zbuild-std=std,panic_abort

build-esp32s3:                      ## Only claimed supported after hardware validation
    MCU=esp32s3 cargo build --release -p cc-firmware --bin firmware \
        --target {{ tgt_esp32s3 }} -Zbuild-std=std,panic_abort

build-esp32c6:                      ## Only claimed supported after hardware validation
    MCU=esp32c6 cargo build --release -p cc-firmware --bin firmware \
        --target {{ tgt_esp32c6 }} -Zbuild-std=std,panic_abort

build-all:
    @just build-esp32
    # build-esp32s3 / build-esp32c6 are deliberately NOT included.
    # A target that builds is not a supported target. See 06 R4-07/R4-08.
```

### Image size budget — run after every task

See [07 — Image size budget](./07-image-size-budget.md) for the full policy. These are
the mechanics.

```makefile
size:                              ## Report image size vs the app slot, and diff vs baseline
    @just --justfile just/size.just size

size-check:                        ## CI gate: fail if the image exceeds the budget
    @just --justfile just/size.just check

size-record mcu="esp32" label="":   ## Record a phase-gate datapoint (see 07)
    @just --justfile just/size.just record {{ mcu }} "{{ label }}"
```

### Flash — hardware required

```makefile
# Diagnostic only. NO recipe ever globs this.
list-ports:
    espflash list-ports

# Confirm the chip before flashing. ALWAYS run this first.
identify port:
    @echo "Refusing to flash an unidentified device."
    espflash board-info --port {{ port }}
    @echo "Confirm the reported chip matches the MCU you intend to build for."

# Flash the production target. PORT is positional and required.
flash port:                        ## ⚠ HARDWARE — overwrites the device
    espflash flash --release -p cc-firmware --bin firmware \
        --target {{ tgt_esp32 }} --port {{ port }} \
        --partition-table rust/partitions_4M.csv

# Wipe and flash. Destroys NVS — but note NVS is rewritten anyway (no
# cross-version compatibility, 06 R3-08), so this is about a known-clean state,
# not about losing data.
reflash port:                      ## ⚠⚠ HARDWARE + DESTRUCTIVE
    @printf 'Erase {{ port }}? type ERASE: ' && read ans && [ "$$ans" = "ERASE" ] || (echo aborted; exit 1)
    espflash erase-flash --port {{ port }}
    @just flash {{ port }}
```

### Monitor

```makefile
mon port:                          ## ⚠ HARDWARE — opens the serial port
    espflash monitor --port {{ port }} --baud 115200

mon-noreset port:                  ## ⚠ HARDWARE — monitor without asserting BOOT
    espflash monitor --port {{ port }} --baud 115200 --no-reset

logs host:                         ## Telnet log server, needs Wi-Fi up
    telnet {{ host }} 23
```

### Parity

```makefile
# The parity scenario runner is created by 06 R1-08, BEFORE any parity gate.
parity port host:                  ## ⚠ HARDWARE — replay scenarios against both firmwares
    ./scripts/parity/run.sh {{ port }} {{ host }}
```

### Benchmark

```makefile
bench:                             ## Host micro-benchmarks (reducer, layout)
    cargo bench --bench reducers -p cc-machine
    cargo bench --bench layout  -p cc-display

size-bench mcu="esp32":            ## ⚠ HARDWARE — control-loop timing on device
    ./scripts/parity/loop-timer.sh {{ mcu }}
```

### Recipe → acceptance-command map

> The normative table is the one in **§4**. Kept here only as a pointer so there is
> exactly one source of truth.

--- | --- |
| Set up the toolchain | `just setup` |
| Check the environment | `just doctor` |
| Format | `just fmt` |
| Verify formatting | `just fmt-check` |
| Lint (host crates) | `just lint` |
| Lint (device) | `just lint-esp32` |
| Host tests | `just test` |
| Build the production target | `just build-esp32` |
| **Image size report** | `just size` |
| **Image size CI gate** | `just size-check` |
| **Record a size datapoint** | `just size-record mcu=esp32 label=gate-2` |
| Find the device | `just list-ports` |
| Confirm the device's chip | `just identify /dev/cu.usbserial-110` |
| Flash | `just flash /dev/cu.usbserial-110` |
| Wipe and flash | `just reflash /dev/cu.usbserial-110` |
| Watch logs over USB | `just mon /dev/cu.usbserial-110` |
| Watch logs over telnet | `just logs esp32.local` |
| Parity against C++ | `just parity /dev/cu.usbserial-110 esp32.local` |
| Host benchmarks | `just bench` |
| Regenerate display goldens | `just snapshot-display` |

## 4b. R1-01 corrections (verified 2026-09-28, on real hardware)

Everything in §4 above is the *pre-R1-01* text. Building and flashing a real image on
2026-09-28 proved six things wrong or missing. The repository's `justfile`,
`.cargo/config.toml` and `crates/cc-firmware/build.rs` already implement the fixes;
this section records them so the text above is not read as authoritative.

| # | What actually happens | Required configuration |
| --- | --- | --- |
| 1 | `esp-idf-sys` publishes its link arguments as *`links` metadata*, and Cargo does **not** forward a dependency's `cargo:rustc-link-arg` to the binary package. Without this, the final link contains no ESP-IDF archives and fails with undefined `pthread_create`, `write`, `abort`, `sched_yield`, … | `crates/cc-firmware/build.rs` = `embuild::espidf::sysenv::output();` and `[build-dependencies] embuild = "=0.33.5"`. Same as `esp-idf-template/cargo/build.rs`. |
| 2 | The link line contains `--ldproxy-linker` / `--ldproxy-cwd`, which the bare Xtensa `gcc` rejects. | `[target.xtensa-esp32-espidf] linker = "ldproxy"` in `.cargo/config.toml`. The key is **`linker`**, not `rustc-linker` (cargo 1.97 rejects the latter there). |
| 3 | rustc resolves the linker from `PATH`; ESP-IDF's line names `xtensa-esp32-elf-gcc`. | `just env-file` writes `.rust-esp-env.sh`, which every device recipe sources. §2's `source "$HOME/exports"` covers the espup case; `env-file` also covers the case where only the ESP-IDF-installed GCC exists. |
| 4 | The flashable app image does **not** contain the partition table — the first byte is the 0xE9 app magic. | The flash recipe must pass `--partition-table rust/partitions_4M.csv`. §3's "flash it explicitly" is load-bearing. |
| 5 | A **virtual** workspace has no "root crate", and `esp-idf-sys` reads `[[package.metadata.esp-idf-sys]]` from the root crate's manifest only (`esp-idf-sys/build/config.rs:92-122`). It then prints `cargo:warning=could not identify the root crate and ESP_IDF_SYS_ROOT_CRATE not specified` and **ignores `extra_components`**. | `ESP_IDF_SYS_ROOT_CRATE = "cc-firmware"` in `.cargo/config.toml [env]`. Until this is set, 04 §6's LittleFS component is silently absent. |
| 6 | The binary is named `firmware`, so the artifact is `target/<triple>/release/firmware`. | The upload path in §6's `firmware` job (`.../release/cc-firmware`) is wrong. |

Two more environment facts:

- **`espup install` needs `--toolchain-version` on this host.** Without it the first
  step (a `api.github.com` "latest release" query) fails. `--skip-version-parse`
  *requires* `--toolchain-version`; they go together. The `.mise.toml`
  `xensa-toolchain` task now passes `--toolchain-version 1.97.0.0`.
- **This host's network path intermittently drops outbound TLS.** The same URL
  succeeds and fails minutes apart, for `espup` (reqwest), the `esp` toolchain's
  `cargo`, and `python` — while `curl` and `git` succeed throughout. Every
  provisioning step needs retries; one failure is not evidence of a blocker.
- `espflash monitor` requires a TTY, so it is unusable from CI or an agent. Use
  `just mon-headless <port>`, which drives DTR/RTS itself and dumps UART0.

## 5. Wi-Fi provisioning

### The hard constraint

**The original ESP32 has no USB peripheral.** There is no CDC, no TinyUSB, no
USB-Serial-JTAG, and no way for the host to enumerate the device as a USB peripheral. The
USB cable is a UART bridge. Therefore Espressif's `wifi_provisioning` **"USB Serial"
transport is unavailable** — that transport needs the native USB-Serial-JTAG found on
S2/S3/C3/C6/H2. This is a property of the silicon, not of Rust.

Additionally, the host tooling requirement is *"Never place credentials in source code,
build logs, shell history, version control, or unencrypted examples"*, which rules out a
`just provision ssid=… pass=…` recipe that echoes to a terminal or lands in `~/.bash_history`.

### Options

| Option | Mechanism | Effort | Verdict |
| --- | --- | --- | --- |
| **P1** | **SoftAP captive portal** (behaviour parity with the current tzapu WiFiManager) | Low | **Baseline.** `esp-wifi-provisioning` 0.1 is esp-idf-svc-native, but pinned to `esp-idf-svc ^0.51` — needs a version bump or vendoring (spike R1-06). |
| **P2** | **UART0 line protocol** on the same 115200 stream the logger uses | Medium | Viable fallback. A framed, marker-prefixed command channel (`CC:PROV …`) demultiplexed from log output. Must be disabled the moment valid credentials exist, and must never print the SSID or password at any log level. |
| **P3** | **`wifi_provisioning` over USB-Serial-JTAG** | Low *if* the hardware changes | Requires an **ESP32-S3** (or C6) board. Unavailable today. A hardware decision, tracked separately from this migration. |

### Credential storage — improve on the status quo

Today all credentials sit in **plaintext NVS** under FNV-1a-hashed keys
(`Config.h:318-332`; no `nvs_encryption` key, no encryption partition). For a device on a
home network that is defensible, and migrating it is out of scope for a port — but the
Rust design must at minimum:

1. **Never log a credential at any level.** A typed `Secret<T>` wrapper whose `Debug` and
   `Display` impls print `[redacted]`, so no `log::info!("{cfg:?}")` can leak it. Enforced
   by a host unit test that asserts the formatted output contains no plaintext.
2. **Never accept a credential on a command line.** Recipes read from stdin or a
   0600-mode file, never from an argument that lands in shell history.
3. **Never accept a credential over the API without auth.** The current firmware has
   `system.auth` off by default and CORS set to `*` with a "for development" comment
   (`WebServerManager.cpp:274`). Provisioning endpoints must be auth-gated or reachable
   **only** while the device is in provisioning mode.
4. Keep NVS plaintext for parity, and record the `nvs_encryption` upgrade as a follow-up
   rather than silently changing storage format during a port.

### The crate, and why there is not one yet

There was a `crates/cc-provisioning` workspace member holding option P1. It was
**deleted**, not implemented: 17 lines, all of them `//!` docs and `#![no_std]`, with
zero construction sites, and — because it was a workspace member that `cc-firmware`
depended on — every device build and every one of the three device clippy passes
resolved a dependency graph for a crate with no code in it. A placeholder crate is
worse than no crate: it makes the workspace claim a component that does not exist,
and the empty `lib.rs` is the only thing anyone finds when they go looking for the
portal. It comes back at **R4-11** with code.

The design and its rules are this section, and they are the whole reason it was worth
reading the deleted file before deleting it:

* **`wifi_provisioning`'s "USB Serial" transport is unavailable**, because the original
  ESP32 has no USB peripheral at all and the cable is a CP2102N UART bridge. Option P3
  is not an option *on this hardware*; it is a silicon property, not a Rust property.
* **No credential may be logged at any level, be accepted as a command-line argument,
  or be accepted over an unauthenticated endpoint.** Those are the three rules above,
  and they are constraints on the implementation, not aspirations.
* **The portal only runs while no valid credentials exist**, so it cannot collide with
  the SPA server for port 80 (04 §3, "Cancellation and backpressure").

### Provisioning workflow (P1, the baseline)

```
just flash /dev/cu.usbserial-110              # device has no credentials
just mon /dev/cu.usbserial-110                  # OLED shows "Starting Portal AP"
# user joins the AP from a phone and submits the form in a browser
# device validates, stores to NVS, reboots
just logs esp32.local                           # confirm it connected
```

Reprovisioning on an already-provisioned device: `POST /api/wifi-reset` (auth-gated) or
hold the power switch for the long-press reboot. Both are `just`-able:

```makefile
wifi-reset host user:            ## ⚠ HARDWARE — clears stored Wi-Fi credentials
    # The password is read from a TTY prompt and piped to curl on stdin via
    # --config, so it never appears in argv (world-readable in `ps`) or in shell
    # history. `just`'s default shell is `sh`, and `read -s` is a BASH builtin,
    # so the recipe must declare `#!/usr/bin/env bash`.
    @printf 'auth password: ' && read -rs PW && echo
    @printf 'user = "%s:%s"\n' "{{ user }}" "$$PW" | curl -fsS --config - \
        -X POST "http://{{ host }}/api/wifi-reset" && echo
```

`@read -rs` keeps the password off the terminal and out of the process table.

**None of this is implemented in this run.** R1-06 validates feasibility first; R4-11
implements it.

---

## 6. CI

Extends the existing four workflows. Format, lint, host tests, and target builds run on
every push; hardware jobs are manual.

```yaml
# .github/workflows/rust.yml
name: Rust
on:
  push: { branches: [main] }
  pull_request:
  workflow_dispatch:

jobs:
  host:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: extractions/setup-just@v2
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      # 1. formatting — no blanket allowances, so this also catches lint-suppression drift
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      # 2. no hardware dependencies may leak into portable crates.
      #    A TEXT grep is not enough: it matches `esp_idf_svc` in code but NOT the
      #    `esp-idf-svc` dependency name in Cargo.toml, so a real dependency would
      #    slip through. Check the actual dependency graph.
      - name: Portable crates must not depend on esp-idf
        run: |
          for c in cc-domain cc-safety cc-machine cc-display cc-config; do
            # NOTE: no 2>/dev/null — a failing `cargo tree` must fail the gate,
            # not silently pass it.
            if cargo tree -p "$c" -e normal --prefix none \
               | grep -qE '^esp-idf-(svc|hal|sys)'; then
              echo "::error::$c depends on an esp-idf crate"; exit 1
            fi
          done
      - name: Portable crates must not mention esp-idf symbols (secondary hint)
        run: |
          ! grep -rnE 'esp_idf_(svc|hal|sys)' \
               crates/cc-domain/src crates/cc-safety/src crates/cc-machine/src \
               crates/cc-display/src crates/cc-config/src
      # 3. no blanket lint suppressions
      - name: No blanket lint suppression
        run: |
          ! grep -rnE '#!\[(allow|expect)\(' crates/*/src
          ! grep -rnE '#\[allow\((clippy::(all|pedantic|restriction)|warnings)\)\]' \
               crates/*/src/lib.rs crates/*/src/main.rs

  firmware:
    runs-on: ubuntu-latest
    # ESP-IDF is compiled twice; give it room and disk.
    timeout-minutes: 60
    # REQUIRED. Without these the build silently uses esp-idf-sys's default IDF
    # and omits the time64 cfg, diverging from local builds. `just` does NOT read
    # mise.toml, so these must be set on the job itself.
    env:
      ESP_IDF_VERSION: v5.5.5
      RUSTFLAGS: "--cfg espidf_time64"
      MCU: esp32
    steps:
      - uses: actions/checkout@v4
      - uses: extractions/setup-just@v2
      - name: Free disk (ESP-IDF std + no_std)
        run: sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/lib/android /usr/local/share/boost
      # This is the esp-rs CI recipe, copied deliberately: it is the only
      # configuration proven to build these crates, and unlike
      # dtolnay/rust-toolchain@xtensa its downloads are retriable.
      - name: espup (Xtensa toolchain)
        run: |
          host=$(rustup show active-toolchain | cut -d- -f2-)   # e.g. x86_64-unknown-linux-gnu
          for attempt in 1 2 3; do
            curl -sSfL https://github.com/esp-rs/espup/releases/latest/download/espup-$host \
                 -o ~/.cargo/bin/espup && break
            [ "$attempt" -lt 3 ] && sleep 20 || exit 1
          done
          chmod +x ~/.cargo/bin/espup
          export ESPUP_EXPORT_FILE="$HOME/exports"
          for attempt in 1 2 3; do
            rm -rf "$HOME/.rustup/toolchains/esp"
            espup install -l debug --targets esp32,esp32s2,esp32s3 && break
            [ "$attempt" -lt 3 ] && sleep 20 || exit 1
          done
          source "$ESPUP_EXPORT_FILE"
          rustup default esp
          echo "$LIBCLANG_PATH" >> "$GITHUB_ENV"
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: Swatinem/rust-cache@v2
        with: { key: esp32, shared-key: esp }
      - name: ldproxy (from crates.io, not the 2022 GitHub release)
        run: |
          host=$(rustup show active-toolchain | cut -d- -f2-)   # Rust triple, not uname
          curl -sSfL -o /tmp/espup "https://github.com/esp-rs/espup/releases/latest/download/espup-$host"
          chmod +x /tmp/espup
          /tmp/espup toolchain install ldproxy --ensure
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - run: just fmt-check
      - run: just build-esp32
      # `build-esp32s3` / `build-esp32c6` are added only when their gates pass.
      - uses: actions/upload-artifact@v4
        with: { name: firmware-esp32, path: target/xtensa-esp32-espidf/release/cc-firmware }
```

The three greps in the `host` job are the mechanism that keeps "no blanket lint
suppressions" and "portable crates stay portable" from degrading into conventions nobody
enforces. They are cheap, they fail loudly, and they are the only defence that survives
agent turnover.

`extractions/setup-just` and `actions-rust-lang/setup-rust-toolchain` are used for the
host job; the Xtensa step is the esp-rs recipe verbatim.

---

## 7. Recipe → acceptance-command map

| Need | Command |
| --- | --- |
| Set up the toolchain | `just setup` |
| Check the environment | `just doctor` |
| Format | `just fmt` |
| Verify formatting | `just fmt-check` |
| Lint (host) | `just lint` |
| Lint (device) | `just lint-esp32` |
| Host tests | `just test` |
| Build the production target | `just build-esp32` |
| Find the device | `just list-ports` |
| Confirm the device's chip | `just identify /dev/cu.usbserial-110` |
| Flash | `just flash /dev/cu.usbserial-110` |
| Wipe and flash | `just reflash /dev/cu.usbserial-110` |
| Watch logs over USB | `just mon /dev/cu.usbserial-110` |
| Watch logs over telnet | `just logs esp32.local` |
| Parity against C++ | `just parity /dev/cu.usbserial-110 esp32.local` |
| Regenerate display goldens | `just snapshot-display` |

---

## 8. Known environment gaps (2026-09-28)

| Gap | Impact | Resolution |
| --- | --- | --- |
| No ESP32 device attached to this machine | No flashing, monitoring, or hardware validation possible | R2-01 is blocked until one is connected |
| `espressif32` PlatformIO platform was not installed; it has now been installed and `pio run -e esp32_usb` **succeeds** (`firmware.bin` = 1,546,240 B), as does `pio test -e native_test` (**340/340 in 55 s**) | none — the C++ baseline is green and usable as the parity reference | — |
| `firmware.bin` is 1,546,240 B against a 1,703,936 B `app0` slot | ~154 KB headroom; a Rust esp-idf image will not fit | R1-01 measures; R2-03 rebalances the partition table |
| `.mise.toml` needed `mise trust` | mise commands errored until trusted | `just setup` runs `mise trust` first |
| node, pnpm, python, clang-format are declared but not installed | the C++ frontend build and format target are unavailable | `mise install` |
| No ESP-IDF on the host | none — `esp-idf-sys` self-provisions | — |
