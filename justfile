# CleverCoffee developer recipes.
#
# Normative source: docs/rust-migration/05-tooling-and-workflows.md §4.
# The recipes below are that document's, with three corrections that the R1-01
# build proved are required. Each is marked DEVIATION and says why:
#
#   D1. The host recipes pass `--target {{host_target}}` explicitly.
#       `.cargo/config.toml` sets `[build] target = "xtensa-esp32-espidf"`, and
#       that applies to *every* cargo invocation in the repository — including
#       `cargo test` for the portable crates, which would then try to build
#       `std` for the Xtensa target. 05 §4's `just test` has no `--target` and
#       therefore cannot work as written.
#   D2. The device recipes source `.rust-esp-env.sh`, which puts the Xtensa GCC
#       on `PATH`. ESP-IDF's link line references `xtensa-esp32-elf-gcc`, so
#       without it the build fails with "linker `xtensa-esp32-elf-gcc` not
#       found". 05 §2 relies on `source $HOME/exports` (espup's export file)
#       for this; `just env-file` regenerates that indirection.
#   D3. `flash` uses `cargo espflash` (not `espflash`) and passes the
#       partition table explicitly. The app image does NOT contain the partition
#       table — the first byte of the app is the 0xE9 app magic, and the table
#       is a separate image at 0x8000 — so the table must be flashed or the
#       bootloader cannot find app0.
#
# `just --list` prints every recipe. Arguments are POSITIONAL: `KEY=value` is
# make syntax and just would pass the literal string through to espflash.

# Device build knobs. SINGLE source of truth: `.cargo/config.toml` and the
# `firmware` CI job env (05 §6) must agree with these. `just doctor` asserts them.
export ESP_IDF_VERSION := "v5.5.5"
export RUSTFLAGS := "--cfg espidf_time64"

# A just `export` assignment is injected into EVERY recipe's environment, so
# this single line is what makes `cargo`, `rustup`, `espflash`, `cargo-espflash`
# and `ldproxy` reachable. They are cargo-installed and are NOT on PATH in a
# plain login shell, which is how R1-01 left half the recipes failing with
# "command not found". Do NOT prefix individual recipes with this.
export PATH := env_var_or_default("PATH", "") + ":" + home_dir() + "/.cargo/bin"

# REQUIRED. A virtual workspace has no root crate, and esp-idf-sys reads
# `[[package.metadata.esp-idf-sys]]` (which carries `extra_components`, e.g. the
# LittleFS managed component) ONLY from the root crate
# (esp-idf-sys/build/config.rs:92-122). Without this it prints
# "could not identify the root crate" and SILENTLY IGNORES the metadata, so
# `svc::fs::littlefs` would later fail with a confusing error.
# Found 2026-09-28 during R1-01.
export ESP_IDF_SYS_ROOT_CRATE := "cc-firmware"

mcu_esp32 := "esp32"
mcu_esp32s3 := "esp32s3"
mcu_esp32c6 := "esp32c6"

tgt_esp32 := "xtensa-esp32-espidf"
tgt_esp32s3 := "xtensa-esp32s3-espidf"
tgt_esp32c6 := "riscv32imac-esp-espidf"

# The one binary is named `firmware` (04 §6), so this is the artifact path.
bin_esp32 := "firmware"

# DEVIATION D1. Resolved at run time, because the host triple depends on the
# machine (macOS arm64 in CI and on this laptop, x86_64 elsewhere).
# Backticks run in a shell WITHOUT the `export PATH` above applied, so this must
# not depend on cargo being found. Hardcode the current host with a runtime
# override; `just doctor` prints it so a mismatch is visible.
host_target := env_var_or_default("CC_HOST_TARGET", "aarch64-apple-darwin")

# The five portable crates. The three device crates do not compile for a host
# target, so `cargo test --workspace` / `cargo clippy --workspace` are wrong.
host_crates := "-p cc-domain -p cc-safety -p cc-machine -p cc-display -p cc-config"
dev_crates := "-p cc-hal-esp32 -p cc-provisioning -p cc-firmware"

# Source the generated environment file (D2) without failing when it is absent.
env_prefix := "[ -f .rust-esp-env.sh ] && . ./.rust-esp-env.sh || true; "

# ---------------------------------------------------------------- setup / env

# Show what mise will install and assert the device build knobs are set.
doctor:
    @test -n "{{ESP_IDF_VERSION}}" || (echo "ESP_IDF_VERSION unset"; exit 1)
    @echo "ESP-IDF pinned: {{ESP_IDF_VERSION}}"
    @echo "RUSTFLAGS:      {{RUSTFLAGS}}"
    @echo "host target:    {{host_target}}"
    @rustup run esp rustc --version || (echo "the esp Xtensa toolchain is missing — run: just setup"; exit 1)
    @just env-file
    @command -v just >/dev/null && just --version
    @command -v espflash >/dev/null && espflash --version
    @command -v ldproxy >/dev/null && echo "ldproxy present"
    @command -v mise >/dev/null && (mise ls || echo "mise not installed; skipping `mise ls`")

# First-time setup, in the right order.
setup:
    mise trust
    mise install
    mise run xensa-toolchain
    @just doctor

# Regenerate `.rust-esp-env.sh` (git-ignored, generated). It is sourced by every
# device recipe.
env-file:
    #!/usr/bin/env bash
    set -euo pipefail
    gcc_bin=""
    # Preferred: espup's own Xtensa GCC (what espup's export file would add).
    for c in "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin; do
        [ -x "$c/xtensa-esp32-elf-gcc" ] && { gcc_bin="$c"; break; }
    done
    # Fallback: the GCC that ESP-IDF installed for itself. Same compiler family,
    # and it is the one that built the archives we link against.
    if [ -z "$gcc_bin" ]; then
        for c in .embuild/espressif/tools/xtensa-esp-elf/*/xtensa-esp-elf/bin; do
            [ -x "$c/xtensa-esp32-elf-gcc" ] && { gcc_bin="$PWD/$c"; break; }
        done
    fi
    if [ -z "$gcc_bin" ]; then
        echo "no Xtensa GCC found. Run 'mise run xensa-toolchain' (espup) or build once so esp-idf-sys installs it." >&2
        exit 1
    fi
    printf 'export PATH="%s:$PATH"\n' "$gcc_bin" > .rust-esp-env.sh
    echo "wrote .rust-esp-env.sh with $gcc_bin"

# ----------------------------------------------------------------- format/lint

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# DEVIATION D1: --target {{host_target}} is required, see the header.
lint:
    cargo clippy {{host_crates}} --all-targets --target {{host_target}} -- -D warnings

lint-esp32:
    {{env_prefix}} MCU={{mcu_esp32}} cargo clippy {{dev_crates}} --all-targets \
        --target {{tgt_esp32}} -Zbuild-std=std,panic_abort -- -D warnings

# Only after R4-07, and only once that target has been flashed and exercised.
lint-esp32s3:
    {{env_prefix}} MCU={{mcu_esp32s3}} cargo clippy {{dev_crates}} --all-targets \
        --target {{tgt_esp32s3}} -Zbuild-std=std,panic_abort -- -D warnings

# Only after R4-08.
lint-esp32c6:
    {{env_prefix}} MCU={{mcu_esp32c6}} cargo clippy {{dev_crates}} --all-targets \
        --target {{tgt_esp32c6}} -Zbuild-std=std,panic_abort -- -D warnings

# ---------------------------------------------------------------------- tests

# Host tests for every portable crate. NOT --workspace: the device crates do not
# compile for a host target.
# DEVIATION D2: `--features cc-display/scenarios`, see the header. The display
# crate is `no_std` with no `alloc`, so the scenario runner behind the parity
# oracle and the goldens sits behind an off-by-default feature. Naming it as
# `cc-display/scenarios` rather than plain `scenarios` keeps this working for the
# whole host crate list.
test:
    cargo test {{host_crates}} --features cc-display/scenarios --target {{host_target}}

test-domain:
    cargo test -p cc-domain -p cc-safety --target {{host_target}}

# Regenerate OLED golden images (host).
#
# `--features scenarios` is required: `cc-display` is `no_std` with no `alloc`, and
# the scenario runner that backs the parity oracle needs both. The feature is off
# by default and is not enabled for the device build, so `cargo build -p
# cc-firmware` still cannot pull a heap into the display crate.
#
# Read the diff before committing a regenerated golden: every pixel is supposed to
# stay put.
snapshot-display:
    cargo test -p cc-display --features scenarios --target {{host_target}} -- --ignored render_goldens

# Display parity against the real U8g2 the firmware links. Needs the U8g2 tree
# from `pio run -e esp32_usb`, and takes about a minute (it rebuilds the oracle).
test-display-parity:
    cargo test -p cc-display --features scenarios --target {{host_target}} --test parity -- --ignored

# ---------------------------------------------------------------------- build

# `just` cannot parameterise a recipe dependency, so there are three explicit
# recipes rather than one parameterised `build` plus aliases.
build-esp32:
    {{env_prefix}} MCU={{mcu_esp32}} cargo build --release -p cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32}} -Zbuild-std=std,panic_abort

build-esp32s3:
    {{env_prefix}} MCU={{mcu_esp32s3}} cargo build --release -p cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32s3}} -Zbuild-std=std,panic_abort

build-esp32c6:
    {{env_prefix}} MCU={{mcu_esp32c6}} cargo build --release -p cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32c6}} -Zbuild-std=std,panic_abort

# build-esp32s3 / build-esp32c6 are deliberately NOT included: a target that
# builds is not a supported target (06 R4-07/R4-08, skill rule 5).
build-all:
    @just build-esp32

# ------------------------------------------------- diagnostic build (unstripped)

# DIAGNOSTIC ONLY. Identical codegen to `--release` (same opt-level, same fat
# LTO, same panic=abort, same overflow-checks) but `strip = "none"` and
# `debug = 2`, so a panic backtrace resolves to symbol names. The release
# profile is NOT modified -- `just size` and the shipped image depend on it.
# Never flash this to a machine you care about: DWARF is dead weight in flash.
diag-build:
    {{env_prefix}} MCU={{mcu_esp32}} cargo build --profile diagnostic -p cc-firmware \
        --bin {{bin_esp32}} --target {{tgt_esp32}} -Zbuild-std=std,panic_abort

# Flash the unstripped build. Same partition table and chip as `just flash`.
diag-flash port:
    {{env_prefix}} cargo espflash flash --profile diagnostic --package cc-firmware \
        --bin {{bin_esp32}} --target {{tgt_esp32}} --port {{port}} --chip {{mcu_esp32}} \
        --partition-table rust/partitions_4M.csv

# Resolve backtrace addresses against the diagnostic ELF. `just diag-addr2line
# 0x40112379 0x400d6dbe` (or paste a whole `Backtrace:` line).
diag-addr2line *addresses:
    #!/usr/bin/env bash
    set -euo pipefail
    elf="target/{{tgt_esp32}}/diagnostic/firmware"
    if [ ! -f "$elf" ]; then echo "no $elf -- run: just diag-build"; exit 1; fi
    a2l=""
    for c in .embuild/espressif/tools/xtensa-esp-elf/*/xtensa-esp-elf/bin \
             "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin; do
        [ -x "$c/xtensa-esp32-elf-addr2line" ] && { a2l="$c/xtensa-esp32-elf-addr2line"; break; }
    done
    [ -n "$a2l" ] || { echo "no xtensa addr2line found"; exit 1; }
    "$a2l" -pfiaC -e "$elf" {{addresses}}

# ----------------------------------------------------------------------- size

# Image size vs the app slot, and the delta vs the previous gate. Run after
# every task (skill rule 5c).
size:
    @just --working-directory . --justfile just/size.just size

# CI gate: fail if the image exceeds the budget.
size-check:
    @just --working-directory . --justfile just/size.just check

# Record a phase-gate datapoint: just size-record esp32 gate-1
size-record mcu="esp32" label="":
    @just --working-directory . --justfile just/size.just record {{mcu}} "{{label}}"

# --------------------------------------------------------------------- flash

# Diagnostic only. NO recipe ever globs /dev/cu.*.
list-ports:
    espflash list-ports

# Confirm the chip before flashing. ALWAYS run this first.
identify port:
    @echo "Refusing to flash an unidentified device."
    espflash board-info --port {{port}}
    @echo "Confirm the reported chip matches the MCU you intend to build for."

# Flash the production target. PORT is positional and REQUIRED — a bare
# `just flash` fails with a usage message rather than guessing a device.
flash port:
    {{env_prefix}} cargo espflash flash --release --package cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32}} --port {{port}} --chip {{mcu_esp32}} \
        --partition-table rust/partitions_4M.csv

# Wipe and flash. Destroys NVS — but NVS is rewritten anyway (no cross-version
# compatibility, 06 R3-08), so this is about a known-clean state, not data loss.
reflash port:
    #!/usr/bin/env bash
    printf 'Erase {{port}}? type ERASE: ' && read ans && [ "$ans" = "ERASE" ] || { echo aborted; exit 1; }
    espflash erase-flash --port {{port}}
    @just flash {{port}}

# -------------------------------------------------------------------- monitor

# Open the serial monitor. Needs a TTY for espflash's key handler; over a
# non-interactive shell use `just mon-headless` instead.
mon port:
    espflash monitor --port {{port}} --baud 115200

# Monitor without asserting BOOT.
mon-noreset port:
    espflash monitor --port {{port}} --baud 115200 --no-reset

# Headless boot log for CI and for agents: reset the chip, then dump UART0.
# Requires a python with pyserial; the ESP-IDF virtualenv created by the
# esp-idf-sys build has one.
# Found 2026-09-28: the recipe expanded `$VIRTUAL_ENV` under `set -u` and aborted with
# "VIRTUAL_ENV: unbound variable" on a shell that is not inside an activated ESP-IDF
# virtualenv -- which is every shell an agent runs. The ESP-IDF venv created by the
# esp-idf-sys build already has pyserial, so the expansion bought nothing and cost the
# recipe.
mon-headless port seconds="20":
    #!/usr/bin/env bash
    set -euo pipefail
    py=""
    for c in .embuild/espressif/python_env/*/bin/python python3; do
        [ -x "$c" ] && "$c" -c 'import serial' 2>/dev/null && { py="$c"; break; }
    done
    [ -n "$py" ] || { echo "no python with pyserial found"; exit 1; }
    "$py" scripts/serial-log.py {{port}} {{seconds}}

logs host:
    telnet {{host}} 23

# --------------------------------------------------------------------- parity

# The parity scenario runner is created by 06 R1-08, BEFORE any parity gate.
parity port host:
    ./scripts/parity/run.sh {{port}} {{host}}

# ------------------------------------------------------------------ benchmark

# Host micro-benchmarks (reducer, layout).
bench:
    cargo bench --bench reducers -p cc-machine --target {{host_target}}
    cargo bench --bench layout -p cc-display --target {{host_target}}

# Control-loop timing on the device. Placeholder until R2-09b.
size-bench mcu="esp32":
    ./scripts/parity/loop-timer.sh {{mcu}}
