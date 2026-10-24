# CleverCoffee: every command humans, agents and CI run. `just` lists them.
# Host crates use rustup's stable toolchain (rust-toolchain.toml); firmware/ uses the
# Xtensa "esp" toolchain from espup (`just setup`).

set shell := ["bash", "-euo", "pipefail", "-c"]

# Firmware image flavours: minimal (no MQTT, no display), standard (default), full.
profile := "standard"
esp_env := "if [ -f \"$HOME/export-esp.sh\" ]; then . \"$HOME/export-esp.sh\"; fi"
elf := "firmware/target/xtensa-esp32-none-elf/release/clevercoffee"
# Address of the cc_prov partition (firmware/partitions.csv).
prov_offset := "0x3F0000"

_default:
    @just --list --unsorted

# One-time: tools from mise.toml, the Xtensa toolchain, UI dependencies.
setup:
    mise install
    espup install --targets esp32 --toolchain-version 1.99.0.0 --skip-version-parse
    cd ui && pnpm install --frozen-lockfile

# Everything CI checks (CI runs the parts as separate jobs).
check: host-check firmware-lint ui-check

# Host crates: format, clippy, tests.
host-check: fmt-check lint test

# --- host ---------------------------------------------------------------------------

# Formats host crates and firmware.
fmt:
    cargo fmt --all
    cd firmware && {{esp_env}} && cargo fmt

# Fails on unformatted host code (firmware: `firmware-lint`).
fmt-check:
    cargo fmt --all --check

# Clippy on the host crates, warnings are errors.
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Host tests (core logic, API, MQTT, drivers, display layout, HTTP end to end).
test:
    cargo test --workspace --all-features

# Renders every OLED screen to target/screens/index.html for layout review.
screens:
    cargo run -q -p screens -- target/screens
    @echo "open target/screens/index.html"

# --- firmware -----------------------------------------------------------------------

_features p=profile:
    @case "{{p}}" in \
      minimal) echo "--no-default-features --features esp32";; \
      standard) echo "";; \
      full) echo "--features full";; \
      bench) echo "--features full,bench";; \
      *) echo "unknown profile {{p}} (minimal|standard|full|bench)" >&2; exit 1;; \
    esac

# Format check and clippy on the firmware, smallest and largest feature set.
firmware-lint: vendor-check
    cd firmware && {{esp_env}} && cargo fmt --check
    cd firmware && {{esp_env}} && cargo clippy --release $(just _features minimal) -- -D warnings
    cd firmware && {{esp_env}} && cargo clippy --release $(just _features bench) -- -D warnings

# Fails when a vendored crate (firmware/vendor) is not what the build uses: after a
# dependency bump Cargo drops an unmatched `[patch]` with only a warning.
vendor-check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd firmware && {{esp_env}}
    for crate in smoltcp xtensa-lx-rt; do
      used=$(cargo tree --locked --features full --invert "$crate" --depth 0 2>&1 || true)
      if ! grep -q "(.*firmware/vendor/$crate)" <<<"$used"; then
        echo "vendored $crate is not used: $used" >&2
        echo "rebase the CleverCoffee patch onto the new version or drop it (docs/todo.md)" >&2
        exit 1
      fi
    done
    echo "vendored crates in use: smoltcp, xtensa-lx-rt"

# Builds the firmware; embeds ui/packages/frontend/dist (run `just ui` first).
build p=profile:
    cd firmware && {{esp_env}} && cargo build --release $(just _features {{p}})

# Merged image (bootloader + partitions + app) at target/clevercoffee-<profile>.bin, flashed at 0x0.
image p=profile: (build p)
    mkdir -p target
    espflash save-image --chip esp32 --merge --partition-table firmware/partitions.csv {{elf}} target/clevercoffee-{{p}}.bin
    @ls -l target/clevercoffee-{{p}}.bin

# Fails when the app outgrows 90 % of its OTA slot (0x1E0000 bytes).
size p=profile: (build p)
    espflash save-image --chip esp32 --partition-table firmware/partitions.csv {{elf}} target/app-{{p}}.bin
    @size=$(wc -c < target/app-{{p}}.bin | tr -d " "); limit=$((0x1E0000 * 9 / 10)); \
      echo "app: $size bytes, limit $limit"; [ "$size" -le "$limit" ]

# Flashes over USB and opens the serial monitor. Erases `otadata`: after an OTA update
# the bootloader would otherwise keep starting the other slot, not the image just flashed.
flash p=profile: (build p)
    espflash flash --chip esp32 --partition-table firmware/partitions.csv --erase-parts otadata --monitor {{elf}}

# Serial monitor.
monitor:
    espflash monitor --chip esp32

# Settings applied at next boot: `just provision --ssid Home --password secret system.hostname=silvia`.
provision *args:
    cargo run -q -p provision -- {{args}} -o target/provision.bin
    espflash write-bin --chip esp32 {{prov_offset}} target/provision.bin

# Erases the whole flash (settings included).
erase:
    espflash erase-flash --chip esp32

# --- web UI -------------------------------------------------------------------------

# Builds the UI the firmware embeds.
ui:
    cd ui && pnpm install --frozen-lockfile && pnpm --filter @clevercoffee/frontend build

# Biome, TypeScript and Vitest on the UI.
ui-check:
    cd ui && pnpm install --frozen-lockfile && pnpm lint
    cd ui/packages/frontend && pnpm tsc && pnpm test:run

# UI against the mock API (no device needed).
ui-dev:
    cd ui && pnpm dev

# --- bench --------------------------------------------------------------------------

# Bench profile (docs/bench.md) plus WiFi, applied at next boot.
bench-provision ssid password:
    just provision --ssid "{{ssid}}" --password "{{password}}" system.hostname=cc-bench \
      pid.enabled=1 hardware.switches.brew.enabled=1 hardware.switches.brew.type=0 \
      hardware.switches.steam.enabled=1 hardware.switches.hot_water.enabled=1 \
      hardware.switches.power.enabled=0 \
      hardware.sensors.temperature.type=1 hardware.sensors.watertank.enabled=1 \
      hardware.sensors.watertank.mode=0

# Smoke tests against a device: `just bench cc-bench.local [--hang] [--ota target/app-bench.bin]`.
bench host *args:
    cargo run -q -p bench -- {{host}} {{args}}
