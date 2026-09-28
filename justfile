# Development recipes. Every recipe runs inside the mise environment, so a fresh clone with
# only mise and just on the host reaches a working build after `just setup`.

set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list --unsorted

# --- setup -----------------------------------------------------------------

# Install every pinned tool, then the Xtensa toolchain that mise cannot express.
setup:
    mise install
    @just espup-install

# Idempotent. Installs the Espressif Xtensa rustc fork, Xtensa LLVM and Xtensa GCC into the
# rustup `esp` toolchain, and the RISC-V targets into `stable`.
espup-install:
    #!/usr/bin/env bash
    set -euo pipefail
    if rustup toolchain list | grep -q '^esp'; then
        echo "esp toolchain already installed"
    else
        espup install --export-file "$PWD/.espup-env.sh"
    fi
    rustup target add --toolchain stable riscv32imac-unknown-none-elf
    @echo "run 'source .espup-env.sh' or use 'just' recipes, which do it for you"

# Source the espup environment for recipes that invoke cargo directly.
_esp_env:
    #!/usr/bin/env bash
    if [ -f .espup-env.sh ]; then source .espup-env.sh; fi

# --- quality ---------------------------------------------------------------

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Host tests only. The hardware crates are compile-checked by `just check`.
test:
    cargo test --workspace --exclude fw

# Format, lint, host tests and a compile check of every target.
check: fmt-check lint test

# --- build -----------------------------------------------------------------

# Compile-check the firmware for one target without producing a flashable image.
check-fw target="esp32":
    #!/usr/bin/env bash
    set -euo pipefail
    source .espup-env.sh 2>/dev/null || true
    triple=$(just --quiet _triple "{{target}}")
    cargo build --release --target "$triple" -p fw --features "{{target}}"

# Produce the flashable image. Refuses to continue if the connected chip is not the target.
build target="esp32" port="":
    #!/usr/bin/env bash
    set -euo pipefail
    source .espup-env.sh 2>/dev/null || true
    triple=$(just --quiet _triple "{{target}}")
    cargo build --release --target "$triple" -p fw --features "{{target}}"
    espflash save-image --chip "{{target}}" \
        "target/$triple/release/fw" "target/fw-{{target}}.bin"
    @echo "image: target/fw-{{target}}.bin"

# --- flash and monitor -----------------------------------------------------

# Flash over USB. Wipes flash: the new partition table is written and all old data is erased.
flash target="esp32" port="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{port}}" ]; then
        echo "usage: just flash <esp32|esp32s3|esp32c6> <port>" >&2
        exit 2
    fi
    echo "This erases flash on {{port}}: the partition table is replaced and all stored data is lost."
    read -r -p "Type FLASH to continue: " answer
    [ "$answer" = "FLASH" ] || { echo "aborted"; exit 1; }
    just --quiet _assert-chip "{{target}}" "{{port}}"
    espflash flash --chip "{{target}}" --port "{{port}}" \
        "target/{{target}}-bootloader.bin" "target/partition-table.bin" \
        "target/fw-{{target}}.bin"

monitor port="":
    #!/usr/bin/env bash
    set -euo pipefail
    espflash monitor --port "{{port}}"

# --- provisioning ----------------------------------------------------------

# Write WIFI_SSID and WIFI_PASS from .env to the device over USB. No network, no web UI, no
# access point. Never prints a credential. Reports a status only.
wifi port="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{port}}" ]; then
        echo "usage: just wifi <port>" >&2
        exit 2
    fi
    cargo run --quiet --release --manifest-path tools/provision/Cargo.toml -- wifi --port "{{port}}"

# Write a JSON config, either the old C++ export or this repository's config.json, to the
# device over USB. Validated on-device against the same schema the HTTP API uses.
config-import port="" file="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{port}}" ] || [ -z "{{file}}" ]; then
        echo "usage: just config-import <port> <file>" >&2
        exit 2
    fi
    cargo run --quiet --release --manifest-path tools/provision/Cargo.toml -- config --port "{{port}}" --file "{{file}}"

factory-reset port="":
    cargo run --quiet --release --manifest-path tools/provision/Cargo.toml -- factory-reset --port "{{port}}"

# Read the device's runtime status over USB. No secrets.
status port="":
    cargo run --quiet --release --manifest-path tools/provision/Cargo.toml -- status --port "{{port}}"

# --- frontend --------------------------------------------------------------

frontend-build:
    cd ui && pnpm install --frozen-lockfile && pnpm prepare-esp

frontend-lint:
    cd ui && pnpm lint

frontend-test:
    cd ui && pnpm test:run && pnpm tsc

# --- helpers ---------------------------------------------------------------

# Map a target name to its rustup triple.
_triple target:
    #!/usr/bin/env bash
    case "{{target}}" in
        esp32)   echo xtensa-esp32-none-elf ;;
        esp32s3) echo xtensa-esp32s3-none-elf ;;
        esp32c6) echo riscv32imac-unknown-none-elf ;;
        *) echo "unknown target {{target}}" >&2; exit 2 ;;
    esac

# Read the connected chip's model and refuse to flash when it is not the target.
_assert-chip target port:
    #!/usr/bin/env bash
    set -euo pipefail
    info=$(espflash board-info --port "{{port}}" 2>&1) || true
    detected=$(printf '%s' "$info" | grep -oP 'Chip ID:\s*\K\w+' | head -1 || true)
    if [ -z "$detected" ]; then
        detected=$(printf '%s' "$info" | grep -oP 'esp32[a-z0-9]*' | head -1 || true)
    fi
    if [ -z "$detected" ]; then
        echo "could not read the chip on {{port}}. Connect the device and retry." >&2
        exit 1
    fi
    if [ "$detected" != "{{target}}" ]; then
        echo "refusing to flash: port {{port}} reports $detected but the target is {{target}}" >&2
        exit 1
    fi
    echo "chip check ok: $detected on {{port}}"

# --- spikes ----------------------------------------------------------------

# Rebuild the capability spikes for all three targets. Proves the toolchain and the HAL.
spike:
    #!/usr/bin/env bash
    set -euo pipefail
    source .espup-env.sh 2>/dev/null || true
    for spec in "hal-smoke esp32 xtensa-esp32-none-elf" \
                "hal-smoke esp32s3 xtensa-esp32s3-none-elf" \
                "hal-smoke esp32c6 riscv32imac-unknown-none-elf" \
                "stack-smoke esp32 xtensa-esp32-none-elf" \
                "stack-smoke esp32s3 xtensa-esp32s3-none-elf" \
                "stack-smoke esp32c6 riscv32imac-unknown-none-elf" \
                "usb-smoke esp32s3 xtensa-esp32s3-none-elf" \
                "usb-smoke esp32c6 riscv32imac-unknown-none-elf"; do
        set -- $spec
        echo "== spikes/$1 for $2 =="
        (cd "spikes/$1" && cargo build -Zbuild-std=core,alloc --release \
            --target "$3" --features "$2")
    done
