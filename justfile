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

# Everything CI checks.
check: fmt-check lint test firmware-lint ui-check

# --- host ---------------------------------------------------------------------------

# Formats host crates and firmware.
fmt:
    cargo fmt --all
    cd firmware && {{esp_env}} && cargo fmt

# Fails on unformatted code.
fmt-check:
    cargo fmt --all --check
    cd firmware && {{esp_env}} && cargo fmt --check

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

# Clippy on the firmware, smallest and largest feature set.
firmware-lint:
    cd firmware && {{esp_env}} && cargo clippy --release $(just _features minimal) -- -D warnings
    cd firmware && {{esp_env}} && cargo clippy --release $(just _features bench) -- -D warnings

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
    espflash save-image --chip esp32 --partition-table firmware/partitions.csv {{elf}} target/app.bin
    @size=$(stat -c %s target/app.bin); limit=$((0x1E0000 * 9 / 10)); \
      echo "app: $size bytes, limit $limit"; [ "$size" -le "$limit" ]

# Flashes over USB and opens the serial monitor.
flash p=profile: (build p)
    espflash flash --chip esp32 --partition-table firmware/partitions.csv --monitor {{elf}}

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

# Smoke tests against a running device: `just bench 192.168.1.50`.
bench host:
    cargo run -q -p bench -- {{host}}
