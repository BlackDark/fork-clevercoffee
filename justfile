# CleverCoffee Rust firmware — task runner.
#
# Target and port are ALWAYS explicit: there is no default board and no default
# serial port. `flash` refuses to run unless the chip actually connected matches
# the target you named.
#
# See docs/rust-migration/tooling.md.

set shell := ["bash", "-uc"]

# Target table, pasted into recipes that need it. Adding a board means adding a
# row here and a pin-map module in cc-board. A target counts as supported only
# once it has been built AND validated on that hardware — see task-list.md.
#
#   target | chip | rust triple | flash method
_t := """
target_table() {
  cat <<'TBL'
esp32 esp32 xtensa-esp32-espidf espflash-uart
TBL
}
_field() { target_table | awk -v t="$1" -v n="$2" '$1==t {print $n; ok=1} END {if(!ok) exit 3}'; }
chip_of()   { _field "$1" 2 || { echo "unknown target: $1" >&2; exit 2; }; }
triple_of() { _field "$1" 3 || { echo "unknown target: $1" >&2; exit 2; }; }
method_of() { _field "$1" 4 || { echo "unknown target: $1" >&2; exit 2; }; }
all_targets() { target_table | awk '{print $1}'; }
esp_env() {
  [ -f "$HOME/export-esp.sh" ] && . "$HOME/export-esp.sh"
  export PATH="$HOME/.cargo/bin:$PATH"
}
"""

default:
    @just --list

# ── setup ────────────────────────────────────────────────────────────────────

# Install rustup, the Xtensa Rust fork, ESP host tools and the python venv.
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v rustup >/dev/null 2>&1; then
      echo "==> installing rustup (mise must NOT manage rust; see tooling.md)"
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --profile minimal
    fi
    export PATH="$HOME/.cargo/bin:$PATH"
    if command -v mise >/dev/null 2>&1; then
      echo "==> mise install"; mise install
    else
      echo "!! mise not found. Install it first: https://mise.jdx.dev" >&2; exit 1
    fi
    echo "==> rustup components for host crates"
    rustup component add rustfmt clippy
    echo "==> pinned ESP host tools (mise cannot manage these; see tooling.md)"
    command -v cargo-binstall >/dev/null 2>&1 || cargo install cargo-binstall --locked
    cargo binstall -y espup@0.17.1 espflash@4.6.0 ldproxy@0.3.5
    echo "==> espup install (Xtensa Rust fork + xtensa-esp-elf GCC)"
    espup install --targets esp32 --export-file "$HOME/export-esp.sh"
    echo "==> python tooling venv (.venv): platformio + nvs partition generator"
    python3 -m venv .venv
    ./.venv/bin/pip install -q 'platformio==6.2.0' esp-idf-nvs-partition-gen
    echo
    echo "Setup complete. Recipes source ~/export-esp.sh themselves."
    echo "Run 'just doctor' to verify."

# Verify the toolchain and report versions. Run this first when something is odd.
doctor:
    #!/usr/bin/env bash
    set -uo pipefail
    {{ _t }}
    fail=0
    check() {
      if out=$("${@:2}" 2>&1 | head -1); then printf '  %-22s %s\n' "$1" "$out"
      else printf '  %-22s MISSING\n' "$1"; fail=1; fi
    }
    esp_env
    echo "host:"
    check rustup    rustup --version
    check just      just --version
    check mise      mise --version
    check cargo     cargo --version
    check espflash  espflash --version
    check "rustc (esp)" rustc +esp --version
    echo "targets:"
    for t in $(all_targets); do
      triple=$(triple_of "$t")
      if rustc +esp --print target-list 2>/dev/null | grep -qx "$triple"; then
        printf '  %-10s %-22s available\n' "$t" "$triple"
      else
        printf '  %-10s %-22s MISSING — run: just setup\n' "$t" "$triple"; fail=1
      fi
    done
    echo "python venv:"
    if [ -x .venv/bin/pio ]; then check pio ./.venv/bin/pio --version
    else echo "  .venv                 MISSING — run: just setup"; fail=1; fi
    cargo_path=$(command -v cargo || true)
    case "$cargo_path" in
      *"/mise/"*)
        echo
        echo "!! cargo resolves to $cargo_path (a mise shim)."
        echo "   rust-toolchain.toml will be ignored and the Xtensa build will fail"
        echo "   with \"can't find crate for \\\`core\\\`\". Fix: mise unuse rust"
        fail=1 ;;
    esac
    exit $fail

# ── quality gates ────────────────────────────────────────────────────────────

# Format all Rust sources in place.
fmt:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    esp_env
    cargo fmt --all
    (cd firmware && cargo fmt --all)

# Check formatting without changing anything. CI gate.
fmt-check:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    esp_env
    cargo fmt --all -- --check
    (cd firmware && cargo fmt --all -- --check)

# Clippy with warnings denied, on host crates and every target. No blanket allows.
lint:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    esp_env
    echo "==> clippy: host crates"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    for t in $(all_targets); do
      triple=$(triple_of "$t")
      echo "==> clippy: $t ($triple)"
      (cd firmware && cargo clippy --workspace --target "$triple" --all-features -- -D warnings)
    done

# Host tests. This is the fast loop: no hardware, no ESP-IDF, no build-std.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    export PATH="$HOME/.cargo/bin:$PATH"
    cargo test --workspace --all-features

# ── build / flash / monitor ──────────────────────────────────────────────────

# Build the firmware for an explicit target, e.g. `just build esp32`.
build target:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    esp_env
    triple=$(triple_of {{ target }})
    echo "==> building {{ target }} ($triple)"
    (cd firmware && cargo build --release --target "$triple")

# Build every target in the table. CI gate.
build-all:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    for t in $(all_targets); do just build "$t"; done

# Report the app image size against the partition budget for a target.
size target:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    esp_env
    triple=$(triple_of {{ target }})
    chip=$(chip_of {{ target }})
    elf="firmware/target/$triple/release/clevercoffee"
    [ -f "$elf" ] || { echo "build first: just build {{ target }}" >&2; exit 1; }
    espflash save-image --chip "$chip" "$elf" "/tmp/cc-{{ target }}.bin"
    bytes=$(stat -f%z "/tmp/cc-{{ target }}.bin" 2>/dev/null || stat -c%s "/tmp/cc-{{ target }}.bin")
    part=$(awk -F, '/^app0/ {gsub(/ /,"",$5); print $5}' partitions_rust_4m.csv)
    ./.venv/bin/python - "$bytes" "$part" <<'PY'
    import sys
    n = int(sys.argv[1]); p = int(sys.argv[2], 16)
    print(f"app image {n:,} B = {n/1024:.1f} KiB = {100*n/p:.1f}% of {p/1024:.0f} KiB app partition")
    print(f"headroom  {(p-n)/1024:.1f} KiB")
    sys.exit(1 if n > p else 0)
    PY

# Identify whatever is on a port. Read-only, safe to run any time.
board-info port:
    #!/usr/bin/env bash
    set -euo pipefail
    export PATH="$HOME/.cargo/bin:$PATH"
    espflash board-info --port "{{ port }}"

# Hard guard: the chip on the port must match the target.
_assert-chip target port:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    export PATH="$HOME/.cargo/bin:$PATH"
    want=$(chip_of {{ target }})
    if ! info=$(espflash board-info --port "{{ port }}" 2>&1); then
      echo "!! cannot read board info from {{ port }}" >&2
      printf '%s\n' "$info" >&2
      exit 1
    fi
    got=$(printf '%s\n' "$info" | sed -n 's/^Chip type: *\([a-z0-9-]*\).*/\1/p' | head -1)
    if [ -z "$got" ]; then
      echo "!! could not parse chip type from board-info output" >&2
      printf '%s\n' "$info" >&2
      exit 1
    fi
    if [ "$got" != "$want" ]; then
      echo "!! REFUSING TO FLASH" >&2
      echo "   target {{ target }} expects chip '$want'" >&2
      echo "   port {{ port }} reports chip '$got'" >&2
      exit 1
    fi
    rev=$(printf '%s\n' "$info" | sed -n 's/^Chip type:.*revision \(v[0-9.]*\).*/\1/p' | head -1)
    echo "==> chip check OK: $got ${rev:-} on {{ port }}"

# Flash an explicit target to an explicit port; refuses on a chip mismatch.
#
# The guard exists because flashing an image built for one chip onto another
# bricks the boot until re-flashed, and because the flash method differs per chip.
flash target port: (_assert-chip target port) (build target)
    #!/usr/bin/env bash
    set -euo pipefail
    {{ _t }}
    esp_env
    triple=$(triple_of {{ target }})
    chip=$(chip_of {{ target }})
    method=$(method_of {{ target }})
    elf="firmware/target/$triple/release/clevercoffee"
    case "$method" in
      espflash-uart)
        # Original ESP32 has no native USB: UART via the board's USB-serial
        # bridge. No USB-JTAG, no DFU.
        # Writes bootloader + partition table + app in one operation, because the
        # Rust firmware refuses to boot without its own `ccfs` partition (ADR 0005)
        # and an app-only flash would leave a halted device.
        espflash flash --chip "$chip" --port "{{ port }}" \
          --partition-table partitions_rust_4m.csv "$elf" ;;
      *)
        echo "no flash method '$method' implemented" >&2; exit 2 ;;
    esac

# Serial monitor on an explicit port.
monitor port:
    #!/usr/bin/env bash
    set -euo pipefail
    export PATH="$HOME/.cargo/bin:$PATH"
    espflash monitor --port "{{ port }}" --baud 115200

# ── provisioning ─────────────────────────────────────────────────────────────

# Provision Wi-Fi credentials over USB from .env. Never prints them.
#
# Reads WIFI_SSID and WIFI_PASS from .env inside this recipe, derives the same
# NVS keys the firmware uses, builds a real NVS image and writes it. Credentials
# are never echoed, never passed as argv (visible via `ps`), and never logged.
# Refuses to write if the device already has NVS content.
provision port:
    #!/usr/bin/env bash
    set -euo pipefail
    export PATH="$HOME/.cargo/bin:$PATH"
    [ -f .env ] || { echo "!! .env not found; needs WIFI_SSID= and WIFI_PASS=" >&2; exit 1; }
    set -a; . ./.env; set +a
    : "${WIFI_SSID:?WIFI_SSID missing from .env}"
    : "${WIFI_PASS:?WIFI_PASS missing from .env}"
    echo "==> provisioning {{ port }} from .env (ssid ${#WIFI_SSID} chars, password ${#WIFI_PASS} chars, neither shown)"
    ./.venv/bin/python scripts/provision.py --port "{{ port }}"

# Build and verify a provisioning image without touching the device.
provision-check port:
    #!/usr/bin/env bash
    set -euo pipefail
    [ -f .env ] || { echo "!! .env not found" >&2; exit 1; }
    set -a; . ./.env; set +a
    ./.venv/bin/python scripts/provision.py --port "{{ port }}" --dry-run

# ── device inspection (read-only, never prints stored values) ────────────────

# Dump and summarise a device's NVS. Read-only; never prints stored values.
nvs-report port:
    #!/usr/bin/env bash
    set -euo pipefail
    export PATH="$HOME/.cargo/bin:$PATH"
    out=$(mktemp -t cc-nvs)
    espflash read-flash --port "{{ port }}" 0x9000 0x5000 "$out" >/dev/null
    ./.venv/bin/python scripts/nvs_inspect.py "$out" --digest
    rm -f "$out"

# ── C++ baseline (the parity oracle; keep it working) ────────────────────────

# Build the existing C++ firmware.
cpp-build:
    ./.venv/bin/pio run -e esp32_usb

# Run the existing C++ host tests.
cpp-test:
    ./.venv/bin/pio test -e native_test
