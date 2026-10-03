#!/usr/bin/env bash
# Install the Espressif `esp` toolchain. Reads its own pins from `.mise.toml`.
#
# One script, because every way of doing this in the workflow or in mise has
# failed at least once, and the reason is always the same shape: `esp` is the
# toolchain this installs, so it cannot be the one that installs it.
#
# 1. CHICKEN AND EGG. `rust-toolchain.toml` says `channel = "esp"`. A bare
#    `cargo install espup` resolves through that override file and dies with
#        error: custom toolchain 'esp' specified in override file
#               '.../rust-toolchain.toml' is not installed
#    `cargo +stable` overrides the override file AND the environment.
#
# 2. THE JUSTFILE'S SWITCH DOES NOT HELP. `CC_RUST_TOOLCHAIN` picks what the
#    justfile puts in RUSTUP_TOOLCHAIN for a RECIPE. rustup never reads it, so a
#    bare `cargo` in a raw shell ignores it. Setting it looked right and was a
#    no-op.
#
# 3. mise STRIPS `~/.cargo/bin` FROM THE PATH A TASK GETS. With cargo plainly
#    on the caller's PATH:
#        $ mise exec -- sh -c 'command -v cargo'   # prints nothing
#        $ mise run setup-esp                       # sh: 1: cargo: not found
#    So cargo is located explicitly: `$CARGO` for a non-default CARGO_HOME, then
#    rustup's default.
#
# 3b. ... and the same is true of `rustup` itself, which espup shells out to. One
#     `PATH="${bin_dir}:$PATH"` fixes cargo, espup and rustup together; resolving
#     them one at a time just moves the error.
#
# 4. mise DOES NOT SUBSTITUTE `[vars]` IN A TASK'S `run` ARRAY. It passes
#    `${x86_64_toolchain_version}` through LITERALLY, so
#        espup install --toolchain-version ${x86_64_toolchain_version}
#    would hand espup the string "${x86_64_toolchain_version}". The pins are
#    therefore read from `.mise.toml` here, where they can be validated.
#
# Idempotent: espup is reinstalled (so a version bump takes) and `espup install`
# is re-entrant.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${repo_root}/.mise.toml"

if [ ! -f "$manifest" ]; then
    echo "setup-esp: no .mise.toml at ${repo_root}" >&2
    exit 1
fi

# One read function, whitespace-tolerant: the keys live in [vars] and are
# therefore indented. A `^`-anchored pattern silently matched nothing once and
# handed espup an empty --toolchain-version, which 404s with no explanation.
mise_var() {
    sed -n "s/^[[:space:]]*$1 = \"\\(.*\\)\"/\\1/p" "$manifest"
}

toolchain_version="$(mise_var x86_64_toolchain_version)"
espup_version="$(mise_var espup_version)"
targets="$(mise_var x86_64_targets)"

for pair in "x86_64_toolchain_version:${toolchain_version}" \
            "espup_version:${espup_version}" \
            "x86_64_targets:${targets}"; do
    if [ -z "${pair#*:}" ]; then
        echo "setup-esp: cannot read '${pair%%:*}' from ${manifest}." >&2
        echo "It lives in the [vars] table, so it is indented -- if that table" >&2
        echo "moved again, fix this pattern rather than the value." >&2
        exit 1
    fi
done

if command -v cargo >/dev/null 2>&1; then
    cargo_bin="$(command -v cargo)"
else
    cargo_bin="${CARGO:-${HOME}/.cargo/bin/cargo}"
fi
if [ ! -x "$cargo_bin" ]; then
    echo "setup-esp: no cargo at '${cargo_bin}'." >&2
    echo "Install Rust with rustup first: https://rustup.rs" >&2
    exit 1
fi

echo "setup-esp: cargo   $("$cargo_bin" +stable --version)"
echo "setup-esp: esp     ${toolchain_version}   (targets: ${targets})"
echo "setup-esp: espup   ${espup_version}"

"$cargo_bin" +stable install --locked espup --version "${espup_version}" --force

# THE GENERAL FIX, rather than resolving one binary at a time.
#
# cargo installed espup into `$CARGO_HOME/bin` and mise strips that directory
# from a task's PATH, so `espup` was "command not found". Resolving espup by path
# only moved the failure: espup then shells out to `rustup`, which is in the
# same stripped directory, and answered "Rust is not installed".
#
# Putting the directory back is the fix for both, and for anything else in it.
bin_dir="$(dirname -- "$cargo_bin")"
PATH="${bin_dir}:${PATH}"
export PATH
echo "setup-esp: restored ${bin_dir} on PATH (mise strips it from a task)"

# espup prepends the leading `v` ITSELF: given `--toolchain-version 1.97.0.0` it
# fetches `.../releases/download/v1.97.0.0/...` (espup-0.17.1,
# src/toolchain/rust.rs: `format!("{REPO}/v{version}/{dist_file}")`), and the
# esp-rs tags ARE `v1.97.0.0`. Passing `v1.97.0.0` would build `vv1.97.0.0`.
# `--toolchain-version` and `--skip-version-parse` are both required: without
# them espup's first step queries the GitHub releases API, which fails on a
# restricted network.
espup install \
    --targets "${targets}" \
    --toolchain-version "${toolchain_version}" \
    --skip-version-parse

echo "setup-esp: done -- $(rustup run esp rustc --version)"
