# CleverCoffee developer recipes.
#
# Normative source: docs/archive/migration/05-tooling-and-workflows.md §4.
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

# One error semantic for the whole file. Every recipe runs under
# `bash -euo pipefail -c`, so:
#   * a failing command aborts the recipe. Before, only the six recipes that
#     declared their own `#!/usr/bin/env bash` + `set -euo pipefail` did -- which
#     is how `reflash` erased the chip and then died on a stray `@just flash`
#     (fixed by `e4ec70bd`, which added the `set shell` line below);
#   * an unset variable is an error rather than an empty string;
#   * a pipe fails if any stage fails.
# The per-recipe shebangs and `set -euo pipefail` lines this made redundant were
# deleted in the same commit.
set shell := ["bash", "-euo", "pipefail", "-c"]

# `.env` holds WIFI_SSID / WIFI_PASS for `wifi-provision`. mise already loads it
# (`mise doctor` lists it under env_files); just did not, so a recipe that wanted
# the variable saw nothing. `scripts/wifi_provision.py` reads `.env` itself
# (defence in depth, so a credential is never in a recipe's environment by
# accident), so this is about honesty, not about making that one recipe work.
set dotenv-load := true

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

# Which toolchain the recipes compile with.
#
# The DEFAULT is `esp`, and it stays the default: `rust-toolchain.toml` pins
# `channel = "esp"`, and rustup gives an AMBIENT `RUSTUP_TOOLCHAIN` precedence
# over that file. If a shell (or CI, or an agent's environment) happens to export
# `RUSTUP_TOOLCHAIN=stable`, every device recipe would fail with "the -Z flag is
# only accepted on the nightly channel of Cargo" -- and the host recipes would
# silently run on the wrong compiler instead. Found 2026-09-28. Setting it here
# makes the recipes immune to ambient state.
#
# It is overridable by `CC_RUST_TOOLCHAIN`, because a just `export` ALWAYS beats
# an environment variable -- which is the point, and also the problem. Verified:
#
#     $ RUSTUP_TOOLCHAIN=stable just show
#     in recipe: RUSTUP_TOOLCHAIN=esp      # the export won
#
# So before this was overridable there was no way at all to run the host gate
# without the Xtensa toolchain, and CI's host job could not run: the runner has
# stable Rust, does not have `esp`, and `rustc -vV` failed, so `just` refused to
# even PARSE. The host gate is `cargo fmt`, clippy, rustdoc and the portable
# tests -- none of which touch an Xtensa pin. Gating them on a multi-hundred-
# megabyte nightly fork downloads that toolchain on every CI run and on every
# new contributor's first `just test`, to compile code that does not care.
#
# So: device work takes no override and gets `esp`; host work sets
# `CC_RUST_TOOLCHAIN=stable`. `just doctor-host` reports which one is in effect,
# and `just doctor` refuses anything but `esp` before a device recipe runs.
export RUSTUP_TOOLCHAIN := env_var_or_default("CC_RUST_TOOLCHAIN", "esp")

mcu_esp32 := "esp32"
mcu_esp32s3 := "esp32s3"
mcu_esp32c6 := "esp32c6"

tgt_esp32 := "xtensa-esp32-espidf"
tgt_esp32s3 := "xtensa-esp32s3-espidf"
tgt_esp32c6 := "riscv32imac-esp-espidf"

# The one binary is named `firmware` (04 §6), so this is the artifact path.
bin_esp32 := "firmware"

# The host triple, for the recipes that build, lint and test the *portable*
# crates on the build machine.
#
# WAS `env_var_or_default("CC_HOST_TARGET", "aarch64-apple-darwin")` -- the
# original author's laptop -- so `just test` died on any non-Apple host with
# `cc: error: unrecognized command-line option '-arch'`, and `just doctor`
# printed the wrong triple without failing on it (fixed by `3d6295f4`).
#
# Two overrides, in order:
#   1. `CC_HOST_TARGET`, for a host chosen on purpose.
#   2. Otherwise ask `rustc`. Backticks run in a shell WITHOUT the
#      `export PATH` above, so this deliberately does NOT depend on the project
#      toolchain -- `rustc` on PATH is the only assumption, satisfied by rustup,
#      by mise, or by a system install.
# NOTE: a backtick here is a trap -- just evaluates it while PARSING the
# justfile, and a failing one is a parse error, so a machine without the `esp`
# toolchain cannot run `just` at all. `scripts/host-target.sh` fails quietly and
# `just doctor-host` turns an empty result into a diagnostic.
host_target := env_var_or_default("CC_HOST_TARGET", `./scripts/host-target.sh`)

# The NINE portable crates. The device crates do not compile for a host target,
# so `cargo test --workspace` / `cargo clippy --workspace` are wrong.
#
# `cc-web` was the sixth (finding 4.1): the HTTP application tier, which was
# inside `cc-hal-esp32` and therefore reachable only by flashing a board.
# `cc-mqtt` was the seventh (finding 4.1b): the same extraction for MQTT's topic
# layout, registry and plan slices, whose 19 assertions ran only under
# `just test-esp32`. `cc-protocol` and `cc-netpolicy` are the eighth and ninth.
# All four are here because `scripts/portable-purity.py` names them as portable,
# and naming them anywhere else would leave those surfaces untested on the host.
host_crates := "-p cc-domain -p cc-protocol -p cc-netpolicy -p cc-safety -p cc-machine -p cc-display -p cc-config -p cc-web -p cc-mqtt"

# `--locked` on EVERY cargo invocation that resolves the dependency graph --
# including `cargo doc` and `cargo test -p cc-parity`, which are not obvious
# and were both missed the first time round. `cargo doc --no-deps` still resolves
# and still rewrites a drifted lock without it, and `cc-parity` depends on
# serde_json/serde_yaml_ng, so neither is a no-dependency case. `cargo fmt` is the
# only one that does not resolve the graph.
#
# Cargo will otherwise UPDATE Cargo.lock to satisfy a manifest and carry on, so a
# green CI run can be a run against a dependency set nobody reviewed and nobody
# committed. `Cargo.lock` is committed here precisely so it is the input; `--locked`
# is what makes it actually be the input. `cargo fmt` does not resolve the graph
# and is left alone.
# `cc-device-tests` is in here because `--all-targets` type-checks it like the
# other device crates. It is the runner, NOT the firmware; see `test-esp32`.
dev_crates := "-p cc-hal-esp32 -p cc-firmware -p cc-device-tests"

# The on-target test image. NEVER flashed as the firmware; `just flash` is
# hard-wired to `{{bin_esp32}}` so there is no recipe that can confuse them.
bin_tests := "firmware-tests"

# Source the generated environment file (D2) without failing when it is absent.
#
# `CARGO_UNSTABLE_BUILD_STD` is NOT set here. `cargo espflash` reads the config
# table directly and ignores the env form, so the flash recipes build the image
# with `cargo` (which takes `-Zbuild-std`) and flash the ELF with plain
# `espflash` -- see `flash-elf`.
env_prefix := "[ -f .rust-esp-env.sh ] && . ./.rust-esp-env.sh || true; "

# ---------------------------------------------------------------- setup / env

# Show what mise will install and assert the device build knobs are set.
# The checks that need NOTHING but a Rust toolchain: no Xtensa GCC, no esp
# channel, no board. This is what a CI runner that has stable Rust can run, and
# what a contributor should run before they have run `just setup`.
#
# It exists as a separate recipe because `doctor` cannot be the CI host gate:
# `doctor` asserts the DEVICE toolchain, and a host job has no business
# downloading a multi-hundred-megabyte Xtensa nightly fork to check that
# `cargo fmt` works.
[script]
doctor-host:
    # The host triple is what every host recipe passes to `cargo --target`, and
    # it used to be hardcoded to the original author's laptop, with `doctor`
    # PRINTING a mismatch instead of failing on it -- which is why `just test`
    # failed on every non-Apple host. `host_target` is now derived by
    # `scripts/host-target.sh`; this asserts the derivation actually produced
    # something, because an empty value would otherwise reach `cargo --target `.
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
      x86_64-unknown-linux-gnu|arm64-unknown-linux-gnu|aarch64-apple-darwin|x86_64-apple-darwin) ;;
      *)
        echo "unexpected host triple '{{host_target}}'."
        echo "The host recipes pass it to 'cargo --target', so it must be one of"
        echo "the four triples this project actually builds for. Extend the list"
        echo "above if you are adding a target."
        exit 1
        ;;
    esac

    # The firmware build knobs. `ESP_IDF_VERSION` is a just `export`, i.e.
    # unconditionally set, so the old `test -n "$ESP_IDF_VERSION"` was a
    # tautology that could not fail. What is worth asserting is that it still
    # agrees with the lockfile esp-idf-sys resolves, because those two drifting
    # apart is how a "works on my machine" build happens. just's interpolation
    # has no `#` strip operator, so the strip happens in the shell.
    idf_version=$(echo '{{ESP_IDF_VERSION}}' | sed 's/^v//')
    if ! grep -qE "^    version: ${idf_version}$" components_esp32.lock; then
      echo "justfile ESP_IDF_VERSION={{ESP_IDF_VERSION}} does not match components_esp32.lock"
      exit 1
    fi
    echo "ESP-IDF pinned: {{ESP_IDF_VERSION}} (matches components_esp32.lock)"
    echo "RUSTFLAGS:      {{RUSTFLAGS}}"

    # The OWNERSHIP RULE, asserted against the manifest rather than the install
    # directory (`mise ls` also lists leftovers from an older manifest): mise
    # must not declare `rust`, because it installs no compiler here -- it
    # symlinks ~/.cargo/bin and hands you back rustup.
    if grep -qE '^  rust = ' .mise.toml; then
      echo "mise declares rust, but rust-toolchain.toml owns the compiler."
      echo "Remove the 'rust = ...' line from .mise.toml (see its header)."
      exit 1
    fi
    echo "compiler owner: rust-toolchain.toml; mise does not declare rust"

    # The trap that has cost the most CI runs, stated as a check so it cannot
    # come back unnoticed: a bare `cargo` -- which is what mise execs to install
    # its `cargo:` backend tools -- resolves through `rust-toolchain.toml` and so
    # demands the `esp` channel. On a machine that has not run `just setup` yet,
    # `mise install` fails three times over with "custom toolchain 'esp' ...
    # is not installed".
    #
    # This only prints advice, because `just doctor` itself runs AFTER the
    # toolchain exists and `just doctor-host` has to work on a host-only machine
    # where `esp` is legitimately absent -- demanding it here would make the host
    # gate unsatisfiable. The real enforcement is `RUSTUP_TOOLCHAIN=stable` on the
    # mise-running job, which is why that is at job level rather than on one step.
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

    # `channel = "esp"` is a FLOATING rustup channel, so the version that matters
    # is the one .mise.toml pins for espup. Assert the two agree, or a toolchain
    # bump silently moves under the size budget.
    grep -q 'channel = "esp"' rust-toolchain.toml
    # `[[:space:]]*`, not `^`: these keys live in `.mise.toml`'s `[vars]` table, so
    # they are indented by two spaces. A `^`-anchored pattern silently matched
    # NOTHING and this printed an EMPTY pin -- for several commits, and through
    # one CI run, because it only ever PRINTED the value. Asserting the pin
    # non-empty is the fix; the tolerant pattern is why it now works at all.
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
    # The host pin must not be OLDER than the device pin.
    #
    # `unknown_lints = "allow"` exists because the two channels have different
    # lint SETS -- a lint the newer one has, the older has never heard of, and
    # naming it in a suppression would otherwise be a build failure. That is a
    # manageable gap. The reverse ordering is not: if the host channel is OLDER,
    # then a lint that fires on it cannot be suppressed at all, so a gate built
    # there would be weaker than the one the device job runs.
    #
    # `cut -d. -f2` for the MINOR: `${pin%%.*}` would strip from the FIRST dot
    # and yield "1" for every 1.x, comparing equal for ever -- which is how the
    # first version of this check silently passed with the pins three years of
    # releases apart.
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

    # The device recipes default to `esp`; a caller who overrode it wants the
    # host gate, not a firmware build. Say so here rather than failing later
    # inside cargo with something about -Z flags.
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

# First-time setup, in the right order: host tools, then the device toolchain
# mise cannot install, then the web UI the firmware embeds.
#
# `CC_RUST_TOOLCHAIN=stable` is scoped to the two steps that need it, and both
# need it for the same reason: `mise`'s `cargo:` backend and `espup` are ordinary
# `cargo install`s, while `rust-toolchain.toml` already names `esp` — a channel
# that does not exist until `espup` installs it. Bootstrapping with the thing
# being bootstrapped is a chicken-and-egg, and it fails as
# "custom toolchain 'esp' is not installed".
setup:
    # mise's cargo backend shells out to `cargo`, so it has to be reachable. On a
    # machine where rustup lives somewhere unusual this is the one place that
    # says so, instead of a bare "No such file or directory" from inside mise.
    command -v cargo >/dev/null || { echo "cargo is not on PATH; install Rust with rustup first: https://rustup.rs"; exit 1; }
    mise trust
    # RUSTUP_TOOLCHAIN, NOT CC_RUST_TOOLCHAIN. mise execs a bare `cargo install`
    # for its `cargo:` backend tools, and a bare cargo resolves through
    # rust-toolchain.toml -- which says `esp`, a channel that does not exist yet
    # on a fresh machine. CC_RUST_TOOLCHAIN is the justfile's own switch and
    # rustup never sees it, so using it here is a no-op that fails confusingly.
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

# ----------------------------------------------------------------- format/lint

# ---------------------------------------------------------------------- web UI

# The firmware embeds the built React bundle, and `cc-hal-esp32/build.rs` PANICS
# if it is absent -- deliberately, so `cargo build` can never succeed and ship a
# firmware whose `/ui` is a placeholder string.
#
# Before this recipe existed nothing built it: no recipe, no CI step, no `just`
# invocation of `pnpm`. So `just build-esp32`, `lint-esp32`, `size`,
# `size-check` -- and therefore the whole `gate` chain -- failed on a clean
# checkout with a panic out of build.rs. Fixed by `e4ec70bd`, which added this
# recipe.
#
# `--frozen-lockfile` so a contributor cannot silently resolve a different
# dependency set than CI does.
# STAMPED, so the three device recipes that depend on it (lint-esp32,
# build-esp32, size-check) do not each pay for pnpm + vite. Measured on the green
# run: pnpm install 3.27 s + vite build 1.42 s in the first, and 0.41 + 1.23 in
# the second -- about 6 s for work already done.
#
# The stamp is keyed on the things the build actually reads, so a change to any
# of them re-runs the build:
#   * `ui/pnpm-lock.yaml`   -- the resolved dependency set
#   * every file under `ui/packages/frontend/src` and its config -- the bundle
#   * the gzipped `dist` marker itself -- the frontend's `postbuild` gzips every
#     asset and then rimrafs the plain ones, so `dist/index.html.gz` is the one
#     path that is always present. (Stamping `dist/index.html` silently never
#     matched anything, which is how this was found: the recipe was not
#     skipping.)
# `--frozen-lockfile` still holds: the stamp skips the WORK, it does not relax
# the lock.
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
lint:
    cargo clippy --locked {{host_crates}} --all-targets --target {{host_target}} -- -D warnings

# The audit runs FIRST and unconditionally, because `lint-esp32` is the recipe
# that made 67 device tests look green while nothing ever executed them. It is
# three greps, needs no device and no cargo, and it is what stops that from
# recurring: a new device-crate `#[test]` that nobody registered with the
# on-target runner is a lint failure, not a test that silently never runs.
lint-esp32: test-audit ui
    {{env_prefix}} MCU={{mcu_esp32}} cargo clippy --locked {{dev_crates}} --all-targets \
        --target {{tgt_esp32}} -Zbuild-std=std,panic_abort -- -D warnings

# Only after R4-07, and only once that target has been flashed and exercised.
lint-esp32s3:
    {{env_prefix}} MCU={{mcu_esp32s3}} cargo clippy --locked {{dev_crates}} --all-targets \
        --target {{tgt_esp32s3}} -Zbuild-std=std,panic_abort -- -D warnings

# Only after R4-08.
lint-esp32c6:
    {{env_prefix}} MCU={{mcu_esp32c6}} cargo clippy --locked {{dev_crates}} --all-targets \
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
    cargo test --locked {{host_crates}} --features cc-display/scenarios --target {{host_target}}

test-domain:
    cargo test --locked -p cc-domain -p cc-safety --target {{host_target}}

# THE RECURRENCE GUARD. Fails if any device-crate test exists that the on-target
# runner cannot execute: a bare `#[test]` (the compiler deletes it unless the
# crate is built with --test), a `#[cfg_attr(test, test)]` that is missing from
# `cc_hal_esp32::device_tests::CASES`, a registry entry with no matching test,
# or any `#[ignore]`. Runs in well under a second and needs no hardware, which
# is what makes it cheap enough to be a dependency of `lint-esp32` and of
# `test-esp32` rather than a thing someone remembers to run.
test-audit:
    @python3 scripts/device-test-audit.py .

# ------------------------------------------- on-target (device) unit tests
#
# The suite that `just test` cannot reach. `cc-hal-esp32` does not build for a
# host target, so its 67 `#[test]` functions are executable only by flashing
# `firmware-tests` and reading the console.
#
# Exits NON-ZERO when any case fails. That is the whole point: the recipe that
# replaced "nothing ran" must not be a recipe that always exits 0.
#
# PORT is positional and REQUIRED, as for `flash`. Run `just identify <port>`
# first. It flashes the test image and leaves it on the chip, so run
# `just flash <port>` before putting the machine back into service.
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
    # Put the firmware back.
    #
    # `cc-device-tests` is flashed over the top of the running firmware, so after
    # this recipe the board is running the **test runner**: 142 cases, then a
    # blank panel and no Wi-Fi until something is flashed. That is not an
    # inconvenience -- it is a machine that looks broken, and the first time this
    # ran nobody (including the person who wrote the recipe) worked out that the
    # board was fine and simply had the wrong image on it.
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
    just test
    just parity-test
    # `doc-links` was in `check` and **not** here, so a run of this recipe could
    # push four broken links while reporting green -- which is what happened on
    # 2026-10-05. It needs no hardware and takes under a second, so there was
    # never a reason for the two recipes to disagree about it.
    just doc-links
    just build-esp32
    just size-check

# Everything a gate must run, on ONE toolchain.
#
# `gate` above is what CI runs (plus the device-test audit `lint-esp32` already
# depends on). This is the same list minus the device steps, which is what
# `CC_RUST_TOOLCHAIN=stable just check` selects: the host gate compiling the
# portable crates on a stock toolchain is a real check, not a consolation
# prize, and it is what makes the workspace's `rust-version = "1.82"` claim
# verifiable instead of decorative.
check:
    just doctor-host
    just fmt-check
    just lint
    just doc
    just test
    just parity-test
    just test-audit
    just doc-links

# Every relative markdown link resolves.
#
# Added 2026-10-04, after a documentation restructure left sixteen rotted links
# in place -- nine of them in `.agents/skills/esp32-rust-migration/SKILL.md`, the
# file every agent reads first. All sixteen predated the move; the move did not
# introduce one. It checks EXISTENCE only: a link to a file that exists but says
# the wrong thing is a documentation defect, not a broken link, and conflating the
# two would make this check unreliable.
doc-links:
    @python3 scripts/check-doc-links.py
    @python3 scripts/check-openapi.py .
    @python3 scripts/check-divergence-refs.py .

# Every screen on every template, as one PNG contact sheet. Host only, no
# hardware: `just screens` then open the file. This is the check a golden image
# cannot be -- it shows what a person would see, which is how the clipped `°C`,
# the cut-off uptime `m` and the missing brew timer were found.
screens:
    cargo run --locked -p cc-display --features scenarios --target {{host_target}} \
        --example screens -- /tmp/cc-screens.png
    @echo "wrote /tmp/cc-screens.png -- open it"

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
    cargo test --locked -p cc-display --features scenarios --target {{host_target}} -- --ignored render_goldens

# The U8g2 tree the display oracle and `extract_fonts.py` read, pinned at the
# same upstream tag the firmware linked before the C++ tree was removed. Fetched
# into `target/`, never vendored: it is third-party source the oracle links, not
# something the firmware ships.
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

# Display parity against the real U8g2 the firmware links. `just u8g2` fetches
# that tree; `CC_U8G2_DIR` points the oracle at a checkout you already have.
test-display-parity: u8g2
    cargo test --locked -p cc-display --features scenarios --target {{host_target}} --test parity -- --ignored

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

# build-esp32s3 / build-esp32c6 are deliberately NOT included: a target that
# builds is not a supported target (06 R4-07/R4-08, skill rule 5).
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

# Image size vs the app slot, and the delta vs the previous gate. Run after
# every task (skill rule 5c).
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
# `cargo espflash flash --package ...` is NOT usable from a recipe: it builds the
# binary itself, accepts no `-Z`, and reads the build-std setting from the cargo
# config table rather than the environment. `e4ec70bd` removed
# `[unstable] build-std` from `.cargo/config.toml` on the grounds that every
# device recipe passes `-Zbuild-std` on the command line -- true for
# `build-esp32` and `lint-esp32`, false for these four, which have been dead
# ever since ("'build-std' not configured"). So the image is built by the
# `build-*` recipe, which can pass the flag, and flashed from its ELF here.
#
# D3 still holds: the app image does not contain the partition table. `espflash`
# reads `rust/partitions_4M.csv` out of the ESP-IDF metadata cargo embeds in the
# ELF, which is why no `--partition-table` is passed here; the flash log prints
# the table it used.
[script]
flash-elf port elf:
    [ -f {{elf}} ] || { echo "no {{elf}} -- build it first"; exit 1; }
    espflash flash --port {{port}} --chip {{mcu_esp32}} {{elf}}

# Flash the production target. PORT is positional and REQUIRED — a bare
# `just flash` fails with a usage message rather than guessing a device.
[script]
flash port:
    just build-esp32
    just flash-elf {{port}} target/{{tgt_esp32}}/release/{{bin_esp32}}

# Wipe and flash. Destroys NVS — but NVS is rewritten anyway (no cross-version
# compatibility, 06 R3-08), so this is about a known-clean state, not data loss.
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
# `wifi apply` at `cc_hal_esp32::provisioning`, which parses them, hands a
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
# Requires a python with pyserial; the ESP-IDF virtualenv created by the
# esp-idf-sys build has one.
# Found 2026-09-28: the recipe expanded `$VIRTUAL_ENV` under `set -u` and aborted with
# "VIRTUAL_ENV: unbound variable" on a shell that is not inside an activated ESP-IDF
# virtualenv -- which is every shell an agent runs. The ESP-IDF venv created by the
# esp-idf-sys build already has pyserial, so the expansion bought nothing and cost the
# recipe.
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

# --------------------------------------------------------------------- parity

# The parity scenario runner (06 R1-08). DEVIATION from the original recipe: the
# host target is passed explicitly, for the same reason as `lint`/`test` — see D1
# in the header. `scripts/parity/run.sh` passes it through.
parity port host:
    ./scripts/parity/run.sh {{port}} {{host}}

# The harness's own tests. NOT part of `just test`: `cc-parity` is the migration's
# measuring instrument, not firmware, and its tests are the thing that decides
# whether a phase gate can be claimed. They read `docs/rust-migration/scenarios/`
# and the real `intentional-diffs.md`, so they also check that the scenario set
# loads, that every dry_run scenario meets its own assertions, that S1-S11 are all
# covered, and that a synthetic *undeclared* diff fails the runner.
parity-test:
    cargo test --locked -p cc-parity --target {{host_target}}

# ------------------------------------------------------------------ benchmark

# Host micro-benchmarks (reducer, layout).
#
# These measure **heap allocations**, not nanoseconds, and that is deliberate.
# The device has ~320 KB of RAM; what a control tick or a display frame costs in
# allocator pressure matters far more than how many host cycles it took. The
# pre-fix control tick allocated 4.00 times per 10 ms tick (before `284ad17a`) and
# `cc-display` has always been allocation-free because it is `no_std` with no
# `alloc` in the device build. Both facts are now measured rather than assumed.
#
# Both targets carry the same measurement as a `#[test]`, so `just test` fails
# if either count ever moves off zero. The bench exists so the number is a thing
# a human watches over time.
bench:
    cargo bench --locked --bench allocations -p cc-machine --target {{host_target}}
    cargo bench --locked --bench layout -p cc-display --target {{host_target}}

# Control-loop timing, on the device if one is attached, on the host otherwise.
#
# It used to be `./scripts/parity/loop-timer.sh {{mcu}}`, and **that script does
# not exist** — `scripts/parity/` contains only `run.sh`. So the recipe failed
# with "No such file or directory" every time anyone tried the one thing that
# would answer "does the control tick fit its budget?" (fixed by `284ad17a`).
#
# On the device the answer does not come from here. The control task already
# measures itself, in the firmware, every `TICK_REPORT_INTERVAL_MS` and logs
# `control tick: worst … budget 10 ms` — that is the number that counts, because
# it is the one taken on the chip with the sensor, display and network tasks
# running. This recipe is the host approximation, and it says so.
#
# With a port, capture the device's own report over a minute of idle and a brew:
#     just mon-headless <port> 60 | grep 'control tick'
size-bench:
    cargo bench --locked --bench allocations -p cc-machine --target {{host_target}}
    @echo
    @echo "host approximation only. The device number the firmware itself logs is"
    @echo "'control tick: worst ... budget 10 ms' — capture it with:"
    @echo "    just mon-headless <port> 60 | grep 'control tick'"

# ------------------------------------------------------------------- hygiene

# `cargo doc` with warnings as errors. The workspace sets `missing_docs =
# "warn"` but nothing ever promoted it, and no recipe or CI step ran rustdoc at
# all, so the 83 broken intra-doc links accumulated silently (fixed by `cc1b06de`).
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps {{host_crates}} --target {{host_target}}

clean:
    cargo clean
    rm -rf target/{{tgt_esp32}} target/{{tgt_esp32s3}} target/{{tgt_esp32c6}}
