# CleverCoffee developer recipes. `just --list` prints them all. Arguments are
# POSITIONAL: `KEY=value` is make syntax and would reach espflash verbatim.
#
# Three deviations from a plain cargo workflow, marked DEVIATION where they apply:
#
#   D1. Host recipes pass `--target {{host_target}}`, so host artifacts live in
#       `target/<host triple>/` and never share `target/debug` with the device
#       build's build scripts, which a different toolchain would keep rebuilding.
#   D2. Device recipes source `.rust-esp-env.sh` for `xtensa-esp32-elf-gcc`,
#       which ESP-IDF's link line names (`just env-file` regenerates it).
#   D3. `flash` uses `cargo espflash` and passes the partition table: the app
#       image does not contain it, and without it the bootloader finds no app0.

# Every recipe fails on the first failing command, unset variable or pipe stage.
set shell := ["bash", "-euo", "pipefail", "-c"]

# `.env` holds WIFI_SSID / WIFI_PASS for `wifi-provision`.
set dotenv-load := true

# Device build knobs. `.cargo/config.toml` and the CI device job must agree;
# `just doctor` asserts it.
export ESP_IDF_VERSION := "v5.5.5"
export RUSTFLAGS := "--cfg espidf_time64"

# cargo-installed tools (espflash, ldproxy, ...) are not on a login shell's PATH.
export PATH := env_var_or_default("PATH", "") + ":" + home_dir() + "/.cargo/bin"

# Required: a virtual workspace has no root crate, and esp-idf-sys reads its
# metadata (the LittleFS `extra_components`) only from the root crate. Without
# this it warns and silently ignores the metadata.
export ESP_IDF_SYS_ROOT_CRATE := "cc-firmware"

# The recipes' toolchain: `esp` unless `CC_RUST_TOOLCHAIN` says otherwise.
#
# An export beats an ambient `RUSTUP_TOOLCHAIN`, so a shell that exports
# `stable` cannot silently build the firmware with the wrong compiler. Host work
# (fmt, clippy, rustdoc, portable tests) sets `CC_RUST_TOOLCHAIN=stable` and
# needs no Xtensa toolchain; `just doctor` refuses anything but `esp` for device
# work, and `just doctor-host` reports what is in effect.
export RUSTUP_TOOLCHAIN := env_var_or_default("CC_RUST_TOOLCHAIN", "esp")

mcu_esp32 := "esp32"
mcu_esp32s3 := "esp32s3"
mcu_esp32c6 := "esp32c6"

tgt_esp32 := "xtensa-esp32-espidf"
tgt_esp32s3 := "xtensa-esp32s3-espidf"
tgt_esp32c6 := "riscv32imac-esp-espidf"

bin_esp32 := "firmware"

# The host triple: `CC_HOST_TARGET`, else `rustc`'s. A script, not an inline
# backtick: just evaluates backticks while parsing, and a failing one would make
# every recipe unusable on a machine without `esp`. `just doctor-host` reports
# an empty result.
host_target := env_var_or_default("CC_HOST_TARGET", `./scripts/host-target.sh`)

# The portable crates: the same list as `scripts/portable-purity.py`. The device
# crates do not compile for a host target, so `--workspace` is wrong.
host_crates := "-p cc-domain -p cc-protocol -p cc-safety -p cc-machine -p cc-display -p cc-config -p cc-net"

# Host-only tools, linted and documented with the portable crates but not part of
# `just test`: `cc-parity` has `just parity-test`, `cc-alloc-count` has no tests.
host_tools := "-p cc-parity -p cc-alloc-count"

# Every cargo invocation that resolves the graph (all but `cargo fmt`, including
# `cargo doc`) passes `--locked`: otherwise cargo rewrites a drifted Cargo.lock
# and CI passes against dependencies nobody committed.

# `cc-device-tests` is the on-target runner, not the firmware; see `test-esp32`.
dev_crates := "-p cc-hal-esp32 -p cc-net-esp32 -p cc-firmware -p cc-device-tests"

# The on-target test image. NEVER flashed as the firmware; `just flash` is
# hard-wired to `{{bin_esp32}}` so there is no recipe that can confuse them.
bin_tests := "firmware-tests"

# Source the generated environment file (D2) without failing when it is absent.
# `cargo espflash` ignores `CARGO_UNSTABLE_BUILD_STD`, so the flash recipes build
# with `cargo -Zbuild-std` and flash the ELF with `espflash` (see `flash-elf`).
env_prefix := "[ -f .rust-esp-env.sh ] && . ./.rust-esp-env.sh || true; "

# ---------------------------------------------------------------- setup / env

# The checks that need only a Rust toolchain: no Xtensa GCC, no esp channel, no
# board. The CI host gate and a contributor before `just setup` run this;
# `doctor` asserts the device toolchain.
[script]
doctor-host:
    # An empty value would otherwise reach `cargo --target `.
    if [ -z "{{host_target}}" ]; then
      echo "host_target is empty: scripts/host-target.sh could not determine this"
      echo "machine's triple from rustc. Set CC_HOST_TARGET=<triple> explicitly."
      exit 1
    fi
    echo "host target:    {{host_target}}"

    # Which channel are we compiling on, and is that the one we meant?
    echo "toolchain:      $(rustc --version)"
    echo "  via:          RUSTUP_TOOLCHAIN=${CC_RUST_TOOLCHAIN:-<default: esp>}"
    case "{{host_target}}" in
      x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|aarch64-apple-darwin|x86_64-apple-darwin) ;;
      *)
        echo "unexpected host triple '{{host_target}}'."
        echo "The host recipes pass it to 'cargo --target', so it must be one of"
        echo "the four triples this project actually builds for. Extend the list"
        echo "above if you are adding a target."
        exit 1
        ;;
    esac

    # `ESP_IDF_VERSION` must agree with the version the component lockfile pins.
    idf_version=$(echo '{{ESP_IDF_VERSION}}' | sed 's/^v//')
    if ! grep -qE "^    version: ${idf_version}$" components_esp32.lock; then
      echo "justfile ESP_IDF_VERSION={{ESP_IDF_VERSION}} does not match components_esp32.lock"
      exit 1
    fi
    echo "ESP-IDF pinned: {{ESP_IDF_VERSION}} (matches components_esp32.lock)"
    echo "RUSTFLAGS:      {{RUSTFLAGS}}"

    # rustup owns the compiler; mise must not declare `rust` (AG-RUST-7).
    if grep -qE '^  rust = ' .mise.toml; then
      echo "mise declares rust, but rust-toolchain.toml owns the compiler."
      echo "Remove the 'rust = ...' line from .mise.toml (see its header)."
      exit 1
    fi
    echo "compiler owner: rust-toolchain.toml; mise does not declare rust"

    # Advice only: a host-only machine legitimately lacks `esp`, but any bare
    # `cargo` (including mise's `cargo:` backend) resolves through
    # rust-toolchain.toml and needs it.
    if ! rustup run esp rustc --version >/dev/null 2>&1; then
      echo ""
      echo "NOTE: the esp toolchain is not installed."
      echo "  Any bare 'cargo' here -- including the one mise execs for its"
      echo "  cargo: backend tools -- resolves through rust-toolchain.toml and"
      echo "  will fail. Run 'mise install' with RUSTUP_TOOLCHAIN=stable, or"
      echo "  'just setup'. (CI sets RUSTUP_TOOLCHAIN=stable at job level for"
      echo "  exactly this reason.)"
    fi

    just --version
    mise --version
    echo "host checks ok -- run 'just setup' for the device toolchain, or 'just doctor' to check it"

# Everything `doctor-host` checks, plus the DEVICE toolchain: the esp channel and
# a Xtensa GCC. The device recipes should not be reachable without this.
[script]
doctor:
    just doctor-host

    # `esp` is a floating channel; the version that matters is .mise.toml's pin.
    # The keys are indented under `[vars]`, and an empty match must fail.
    grep -q 'channel = "esp"' rust-toolchain.toml
    esp_pin=$(sed -n 's/^[[:space:]]*x86_64_toolchain_version = "\(.*\)"/\1/p' .mise.toml)
    espup_pin=$(sed -n 's/^[[:space:]]*espup_version = "\(.*\)"/\1/p' .mise.toml)
    host_pin=$(sed -n 's/^[[:space:]]*host_toolchain = "\(.*\)"/\1/p' .mise.toml)
    if [ -z "$esp_pin" ]; then
      echo ".mise.toml has no readable x86_64_toolchain_version pin."
      echo "It lives in [vars], so it is indented -- if that table moved again,"
      echo "fix this pattern rather than the pin."
      exit 1
    fi
    if [ -z "$espup_pin" ]; then
      echo ".mise.toml has no readable espup_version pin"
      exit 1
    fi
    if [ -z "$host_pin" ]; then
      echo ".mise.toml has no readable host_toolchain pin"
      exit 1
    fi
    echo "esp toolchain pin: $esp_pin   espup pin: $espup_pin   host pin: $host_pin"
    # The host pin must not be older than the device pin, or the host gate would
    # be weaker than the device one. Compare minors (`cut -f2`); `${pin%%.*}`
    # yields "1" for every 1.x.
    esp_minor="$(echo "$esp_pin" | cut -d. -f2)"
    host_minor="$(echo "$host_pin" | cut -d. -f2)"
    # For the message: `-f1,2` keeps the major, so it reads "1.97" and not "97".
    esp_series="$(echo "$esp_pin" | cut -d. -f1,2)"
    host_series="$(echo "$host_pin" | cut -d. -f1,2)"
    if [ "${host_minor:-0}" -lt "${esp_minor:-0}" ]; then
      echo "toolchain pins are ordered wrong: esp ${esp_pin} vs host ${host_pin}"
      echo "  (rustc ${esp_series}.x is the device channel, ${host_series}.x the host one)"
      echo "The host channel must not be OLDER than the device channel: a lint"
      echo "that fires on the older one cannot be suppressed by name at all, so"
      echo "the host gate would be strictly weaker than the device gate."
      exit 1
    fi
    echo "channels: device rustc ${esp_series}.x, host rustc ${host_series}.x (lint gap absorbed by unknown_lints = \"allow\")"
    rustup run esp rustc --version

    # Fail here rather than later inside cargo with an error about -Z flags.
    if [ "${CC_RUST_TOOLCHAIN:-}" != "" ] && [ "${CC_RUST_TOOLCHAIN}" != "esp" ]; then
      echo "CC_RUST_TOOLCHAIN=${CC_RUST_TOOLCHAIN}, but a device build needs 'esp'."
      echo "Unset it (or set it to esp) to build or flash the firmware."
      exit 1
    fi

    just env-file
    cargo espflash --version
    # The flash recipes shell out to the standalone binary too (see `flash-elf`),
    # so a missing one has to fail here rather than at the flash.
    espflash --version
    command -v ldproxy >/dev/null && echo "ldproxy present"

# First-time setup: host tools, the device toolchain mise cannot install, then
# the web UI the firmware embeds.
setup:
    command -v cargo >/dev/null || { echo "cargo is not on PATH; install Rust with rustup first: https://rustup.rs"; exit 1; }
    mise trust
    # `RUSTUP_TOOLCHAIN`, which rustup reads (`CC_RUST_TOOLCHAIN` is this file's
    # own switch): mise's bare `cargo install` would otherwise demand `esp`,
    # which does not exist until this step installs it.
    RUSTUP_TOOLCHAIN=stable mise install
    mise run setup-esp
    @just ui
    @just doctor

# Regenerate `.rust-esp-env.sh` (git-ignored, generated). It is sourced by every
# device recipe.
[script]
env-file:
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
        echo "no Xtensa GCC found. Run 'mise run setup-esp' (espup) or build once so esp-idf-sys installs it." >&2
        exit 1
    fi
    printf 'export PATH="%s:$PATH"\n' "$gcc_bin" > .rust-esp-env.sh
    echo "wrote .rust-esp-env.sh with $gcc_bin"

# ---------------------------------------------------------------------- web UI

# The React bundle the firmware embeds. `cc-net-esp32/build.rs` panics without
# it, so no build can ship a placeholder `/ui` (AG-RUST-6).
#
# Skipped when `dist/index.html.gz` (the one file `postbuild` always leaves) is
# newer than the lockfile and every source and config file, so the device
# recipes that depend on it pay once. `--frozen-lockfile` holds either way.
ui:
    #!/usr/bin/env bash
    set -euo pipefail
    stamp=ui/packages/frontend/dist/index.html.gz
    if [ -f "$stamp" ] && [ -z "$(find ui/pnpm-lock.yaml ui/packages/frontend/src \
        ui/packages/frontend/*.ts ui/packages/frontend/*.json -newer "$stamp" 2>/dev/null)" ]; then
      echo "ui: bundle is up to date ($stamp)"
      exit 0
    fi
    pnpm --dir ui install --frozen-lockfile
    pnpm --dir ui --filter @clevercoffee/frontend build

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# DEVIATION D1: --target {{host_target}} is required, see the header.
# `host_tools` are not portable, but they are host code: outside this recipe they rot.
lint:
    cargo clippy --locked {{host_crates}} {{host_tools}} --all-targets --target {{host_target}} -- -D warnings

# The audit runs first: a device-crate test the on-target runner cannot reach is
# a lint failure, not a test that silently never runs.
lint-esp32: test-audit ui
    {{env_prefix}} MCU={{mcu_esp32}} cargo clippy --locked {{dev_crates}} --all-targets \
        --target {{tgt_esp32}} -Zbuild-std=std,panic_abort -- -D warnings

# Unsupported until the target has been flashed and exercised.
lint-esp32s3:
    {{env_prefix}} MCU={{mcu_esp32s3}} cargo clippy --locked {{dev_crates}} --all-targets \
        --target {{tgt_esp32s3}} -Zbuild-std=std,panic_abort -- -D warnings

# Unsupported until the target has been flashed and exercised.
lint-esp32c6:
    {{env_prefix}} MCU={{mcu_esp32c6}} cargo clippy --locked {{dev_crates}} --all-targets \
        --target {{tgt_esp32c6}} -Zbuild-std=std,panic_abort -- -D warnings

# ---------------------------------------------------------------------- tests

# Host tests for every portable crate. NOT --workspace: the device crates do not
# compile for a host target.
test:
    cargo test --locked {{host_crates}} --target {{host_target}}

# Fails on a device-crate test the on-target runner cannot execute: a bare
# `#[test]`, a `#[cfg_attr(test, test)]` missing from
# a crate's `device_tests::CASES`, a registry entry with no test, or any
# `#[ignore]`. No hardware, under a second.
test-audit:
    @python3 scripts/device-test-audit.py .

# ------------------------------------------- on-target (device) unit tests
#
# The suite `just test` cannot reach: the device crates' tests run only by
# flashing `firmware-tests` and reading the console. Exits non-zero when any case
# fails. PORT is required; run `just identify <port>` first. The firmware is
# flashed back at the end.
[script]
test-esp32 port: test-audit
    just build-tests-esp32
    just flash-elf {{port}} target/{{tgt_esp32}}/release/{{bin_tests}}
    py=""
    for c in .embuild/espressif/python_env/*/bin/python python3; do
        [ -x "$c" ] && "$c" -c 'import serial' 2>/dev/null && { py="$c"; break; }
    done
    [ -n "$py" ] || { echo "no python with pyserial found" >&2; exit 1; }
    "$py" scripts/device-tests.py {{port}}
    # Without this the board keeps the test runner: a blank panel and no Wi-Fi,
    # which looks exactly like a broken machine.
    echo ""
    echo "device-tests finished; restoring the firmware on {{port}}"
    just flash {{port}}
    echo "firmware restored -- the panel and the API are back"

# Build the on-target test image without flashing. Same opt-level, same
# panic=abort, same overflow-checks as the release profile, so what is measured
# here is what runs on the chip.
build-tests-esp32: ui
    {{env_prefix}} MCU={{mcu_esp32}} cargo build --locked --release -p cc-device-tests \
        --bin {{bin_tests}} --target {{tgt_esp32}} -Zbuild-std=std,panic_abort

# Everything a gate must run that does not need hardware. `test-esp32` is NOT
# in the chain because it needs a board and a port; the guard against "the device
# tests never run" is `test-audit`, which is in `lint-esp32`, which is here.
gate:
    just doctor-host
    just fmt-check
    just lint
    just lint-esp32
    just doc
    just doc-esp32
    just test
    just parity-test
    just doc-links
    just build-esp32
    just size-check

# `gate` minus the device steps, for `CC_RUST_TOOLCHAIN=stable just check`: the
# portable crates on a stock toolchain (AG-RUST-1, AG-RUST-3).
check:
    just doctor-host
    just fmt-check
    just lint
    just doc
    just test
    just parity-test
    just test-audit
    just doc-links

# Every relative markdown link resolves (existence only), and the OpenAPI spec
# matches the routes the firmware serves (AG-REPO-31).
doc-links:
    @python3 scripts/check-doc-links.py
    @python3 scripts/check-openapi.py .

# Every screen on every template, as one PNG contact sheet. Host only, no
# hardware: `just screens` then open the file. This is the check a golden image
# cannot be -- it shows what a person would see, which is how the clipped `°C`,
# the cut-off uptime `m` and the missing brew timer were found.
screens:
    cargo run --locked -p cc-display --target {{host_target}} \
        --example screens -- /tmp/cc-screens.png
    @echo "wrote /tmp/cc-screens.png -- open it"

# Regenerate OLED golden images (host).
#
# Read the diff before committing a regenerated golden: every pixel is supposed to
# stay put.
snapshot-display:
    cargo test --locked -p cc-display --target {{host_target}} -- --ignored render_goldens

# The U8g2 tree `extract_fonts.py` reads, pinned at the same upstream tag the
# firmware linked before the C++ tree was removed. Fetched into `target/`, never
# vendored: the font streams it produces are what the firmware ships, and this is
# how they are re-derived.
U8G2_REPO := "https://github.com/olikraus/u8g2.git"
U8G2_TAG := "2.36.18"

[script]
u8g2:
    if [ -d target/u8g2/csrc ]; then
      echo "target/u8g2 already present"
      exit 0
    fi
    if [ -e target/u8g2 ]; then
      echo "target/u8g2 exists but is not a U8g2 checkout -- rm it and re-run"
      exit 1
    fi
    git clone --quiet --depth 1 --branch {{U8G2_TAG}} {{U8G2_REPO}} target/u8g2
    echo "U8g2 {{U8G2_TAG}} fetched into target/u8g2"

# ---------------------------------------------------------------------- build

# `just` cannot parameterise a recipe dependency, so there are three explicit
# recipes rather than one parameterised `build` plus aliases.
build-esp32: ui
    {{env_prefix}} MCU={{mcu_esp32}} cargo build --locked --release -p cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32}} -Zbuild-std=std,panic_abort

build-esp32s3:
    {{env_prefix}} MCU={{mcu_esp32s3}} cargo build --locked --release -p cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32s3}} -Zbuild-std=std,panic_abort

build-esp32c6:
    {{env_prefix}} MCU={{mcu_esp32c6}} cargo build --locked --release -p cc-firmware --bin {{bin_esp32}} \
        --target {{tgt_esp32c6}} -Zbuild-std=std,panic_abort

# Not esp32s3 / esp32c6: a target that builds is not a supported target.
build-all:
    just build-esp32

# ------------------------------------------------- diagnostic build (unstripped)

# DIAGNOSTIC ONLY. Identical codegen to `--release` (same opt-level, same fat
# LTO, same panic=abort, same overflow-checks) but `strip = "none"` and
# `debug = 2`, so a panic backtrace resolves to symbol names. The release
# profile is NOT modified -- `just size` and the shipped image depend on it.
# Never flash this to a machine you care about: DWARF is dead weight in flash.
diag-build: ui
    {{env_prefix}} MCU={{mcu_esp32}} cargo build --locked --profile diagnostic -p cc-firmware \
        --bin {{bin_esp32}} --target {{tgt_esp32}} -Zbuild-std=std,panic_abort

# Flash the unstripped build. Same partition table and chip as `just flash`.
diag-flash port:
    just diag-build
    just flash-elf {{port}} target/{{tgt_esp32}}/diagnostic/{{bin_esp32}}

# Resolve backtrace addresses against the diagnostic ELF. `just diag-addr2line
# 0x40112379 0x400d6dbe` (or paste a whole `Backtrace:` line).
[script]
diag-addr2line *addresses:
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

# Image size vs the app slot, and the delta vs the previous record.
size: ui
    just --working-directory . --justfile just/size.just size

# CI gate: fail if the image exceeds the budget.
size-check: ui
    just --working-directory . --justfile just/size.just check

# Record a phase-gate datapoint: just size-record esp32 gate-1
size-record mcu="esp32" label="":
    just --working-directory . --justfile just/size.just record {{mcu}} "{{label}}"

# --------------------------------------------------------------------- flash

# Diagnostic only. NO recipe ever globs /dev/cu.*.
list-ports:
    cargo espflash list-ports

# Confirm the chip before flashing. ALWAYS run this first.
identify port:
    @echo "Refusing to flash an unidentified device."
    cargo espflash board-info --port {{port}}
    @echo "Confirm the reported chip matches the MCU you intend to build for."

# Flash a built ELF onto a port.
#
# - Built by a `build-*` recipe, not `cargo espflash`: that builds the binary
#   itself and cannot take `-Zbuild-std`.
# - The partition table is passed explicitly: esp-idf-sys does not embed
#   `rust/partitions_4M.csv`, and without it the chip keeps its previous table
#   (measured: one factory slot, no OTA slots, OTA answering 500).
# - Both app slots are written: espflash writes `app0` and ignores `otadata`, so
#   a device that had taken an OTA would keep booting the old image from `app1`.
#   `app1`'s offset is read from the partition table so the two cannot drift.
#   Overwriting the running slot is safe; the ROM loader runs from IRAM.
[script]
flash-elf port elf:
    [ -f {{elf}} ] || { echo "no {{elf}} -- build it first"; exit 1; }
    # Read and check the app1 offset BEFORE anything is written to the device.
    # Doing it after `espflash flash` left a half-flashed board behind whenever
    # the CSV had lost its app1 row: new bootloader, new table, new app0, and
    # then exit 1 with app1 stale and otadata still naming whatever it named.
    app1="$(awk -F, '/^app1,/ { gsub(/[ \t]/, "", $4); print $4 }' rust/partitions_4M.csv)"
    case "$app1" in
        0x*) ;;
        *) echo "rust/partitions_4M.csv gave app1 offset '$app1', which is not an offset; refusing to flash"; exit 1 ;;
    esac
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    espflash save-image --chip {{mcu_esp32}} {{elf}} "$tmp/app.bin" >/dev/null
    espflash flash --port {{port}} --chip {{mcu_esp32}} \
      --partition-table rust/partitions_4M.csv {{elf}}
    # Writing the slot the chip is currently running from is safe while the write
    # is in flight: the chip executes the ROM loader from IRAM, not the app.
    # It is NOT safe if the write is *interrupted* -- the selected slot is left
    # corrupt, there is no rollback, and recovery is another `just flash` over the
    # cable you are already holding. That is the price of "a USB flash always
    # wins".
    espflash write-bin --port {{port}} --chip {{mcu_esp32}} "$app1" "$tmp/app.bin"

# Flash the production target. PORT is positional and REQUIRED — a bare
# `just flash` fails with a usage message rather than guessing a device.
[script]
flash port:
    # A bench image has the over-temperature floor compiled out. `bench-flash`
    # scopes the variable to its own build; this refuses to build at all if it is
    # in the ambient environment, where `export`, a `.cargo/config.toml [env]`,
    # or a stray CI layer would otherwise produce a relaxed image from the
    # ordinary recipe with nothing but a boot warning to show for it.
    [ -z "${CC_BENCH_UNSAFE_TEMPERATURES:-}" ] || { echo "CC_BENCH_UNSAFE_TEMPERATURES is set in this shell. This is a BENCH build; use 'just bench-flash', and never on a machine."; exit 1; }
    just build-esp32
    just flash-elf {{port}} target/{{tgt_esp32}}/release/{{bin_esp32}}

# Build and flash a BENCH image: the emergency-threshold floor and the two
# headroom checks are compiled out, so `safety.emergency_temp=30` is accepted and
# a hand-warmed DS18B20 trips S1. That is the only way to run runbook §13.1/§13.2
# on a bench, because the real floor is 120 °C and a bench boiler sits at 23 °C.
#
# The flag is baked in by `option_env!` — there is no runtime switch and no HTTP
# route that can turn it on — and the firmware prints a `SAFETY BENCH BUILD`
# warning at boot.
#
# `cc-safety`'s suite is not run against a bench build: its headroom tests fail
# by design there. `just check` and `just gate` run it against the production
# image.
#
# **Never flash this to the coffee machine.** On a machine it removes the check
# that stops the emergency threshold being set where the PID would trip it during
# normal brewing or steaming.
[script]
bench-flash port:
    printf 'Bench build (SAFETY CHECKS RELAXED) to %s - not the machine.\n' {{port}}
    CC_BENCH_UNSAFE_TEMPERATURES=1 just build-esp32
    just flash-elf {{port}} target/{{tgt_esp32}}/release/{{bin_esp32}}

# Back to the production image. Run this the moment the bench work is done.
[script]
unbench-flash port:
    [ -z "${CC_BENCH_UNSAFE_TEMPERATURES:-}" ] || { echo "CC_BENCH_UNSAFE_TEMPERATURES is set in this shell; unbench-flash must build the production image. Unset it."; exit 1; }
    just build-esp32
    just flash-elf {{port}} target/{{tgt_esp32}}/release/{{bin_esp32}}


# Wipe and flash, for a known-clean state. Erases NVS: the stored configuration,
# credentials and shot counter go with it.
[script]
reflash port:
    printf 'Erase {{port}}? type ERASE: ' && read ans && [ "$ans" = "ERASE" ] || { echo aborted; exit 1; }
    cargo espflash erase-flash --port {{port}}
    just flash {{port}}

# ------------------------------------------------------- Wi-Fi provisioning

# Put the Wi-Fi credential on the device, from `.env`, over the UART console.
#
#     just wifi-provision /dev/cu.usbserial-XXXX
#
# `.env` holds `WIFI_SSID` and `WIFI_PASS`. It is **gitignored** (`.gitignore:21`)
# and **never printed**: the values go in as a line on the wire and come back out
# of `scripts/wifi_provision.py` filtered, so neither a shell history nor this
# terminal ever sees a credential.
#
# **This is the correct location by construction.** The credential is not written
# to NVS by hand: the script types `wifi set <ssid>`, `wifi pass <password>` and
# `wifi apply` at `cc_net_esp32::provisioning`, which parses them, hands a
# `Pending` to the control task, and persists it with `BlobConfigStore` — the same
# path the web UI uses. Crafting the NVS blob would couple this recipe to the
# blob's schema version and its JSON shape.
#
# The password is an *argument* (`wifi pass <value>`) and not the line after
# `wifi set`, so no 30 s password window is open across this script's serial
# session. The device still accepts the next-line form for an operator typing
# by hand.
#
# The machine arms this console when it has **no** SSID, which is a fresh flash
# or a configuration that was refused at boot. With a credential already stored
# the recipe says so and changes nothing.
[script]
wifi-provision port:
    py=""
    for c in .embuild/espressif/python_env/*/bin/python python3; do
        [ -x "$c" ] && "$c" -c 'import serial' 2>/dev/null && { py="$c"; break; }
    done
    [ -n "$py" ] || { echo "no python with pyserial found" >&2; exit 1; }
    exec "$py" scripts/wifi_provision.py {{port}}

# -------------------------------------------------------------------- monitor

# Open the serial monitor. Needs a TTY for espflash's key handler; over a
# non-interactive shell use `just mon-headless` instead.
mon port:
    cargo espflash monitor --port {{port}} --baud 115200

# Monitor without asserting BOOT.
mon-noreset port:
    cargo espflash monitor --port {{port}} --baud 115200 --no-reset

# Headless boot log for CI and for agents: reset the chip, then dump UART0.
# Requires a python with pyserial; the ESP-IDF virtualenv the esp-idf-sys build
# creates has one, so no virtualenv needs to be active.
[script]
mon-headless port seconds="20":
    py=""
    for c in .embuild/espressif/python_env/*/bin/python python3; do
        [ -x "$c" ] && "$c" -c 'import serial' 2>/dev/null && { py="$c"; break; }
    done
    [ -n "$py" ] || { echo "no python with pyserial found"; exit 1; }
    "$py" scripts/serial-log.py {{port}} {{seconds}}

logs host:
    telnet {{host}} 23

# ------------------------------------------------------------------- scenarios

# The scenario harness: its binary contract and every scenario in
# `crates/cc-parity/scenarios/`, in process, no device.
parity-test:
    cargo test --locked -p cc-parity --target {{host_target}}

# ------------------------------------------------------------------ benchmark

# Host micro-benchmarks (reducer, layout).
#
# Heap allocations first, nanoseconds second: on ~320 KB of RAM, allocator
# pressure matters more than host cycles. `just test` already fails if either
# count leaves zero (AG-RUST-11, AG-RUST-12); this prints the numbers.
bench:
    cargo bench --locked --bench allocations -p cc-machine --target {{host_target}}
    cargo bench --locked --bench layout -p cc-display --target {{host_target}}
    @echo
    @echo "host approximation only. The device number the firmware itself logs is"
    @echo "'control tick: worst ... budget 10 ms' — capture it with:"
    @echo "    just mon-noreset <port> 60 | grep 'control tick'"

# Control-loop timing on the chip, without resetting a running machine:
#     just mon-noreset <port> 60 | grep 'control tick'

# ------------------------------------------------------------------- hygiene

# `cargo doc` with warnings as errors, so a broken intra-doc link fails the gate.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps {{host_crates}} {{host_tools}} --target {{host_target}}

# The same for the device crates. Without `cc-device-tests`, so the test-only
# feature is off and the shipped code is what gets documented.
doc-esp32: ui
    {{env_prefix}} MCU={{mcu_esp32}} RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps \
        -p cc-hal-esp32 -p cc-net-esp32 -p cc-firmware \
        --target {{tgt_esp32}} -Zbuild-std=std,panic_abort

clean:
    cargo clean
    rm -rf target/{{tgt_esp32}} target/{{tgt_esp32s3}} target/{{tgt_esp32c6}}
