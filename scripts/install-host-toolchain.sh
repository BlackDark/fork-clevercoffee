#!/usr/bin/env bash
# Install the PINNED host toolchain and tell the caller how to select it.
#
# Why pinned, not `stable`: a GitHub runner ships whatever `stable` was when its
# image was built, and in PR #96 that was old enough that `clippy::assert_is_empty`
# (added in clippy 1.99) was unknown to it. A lint gate whose lint SET depends on
# the runner image's build date is not a gate -- it broke here, and it would
# equally let a brand-new lint through one week and fail the next.
#
# The version is read from `.mise.toml`'s `[vars]`, the one place it is written,
# and asserted non-empty: a silent empty would otherwise surface much later as a
# confusing link error.
#
# Prints `CC_RUST_TOOLCHAIN=<version>` on stdout and nothing else, so a caller can
# do:
#     echo "CC_RUST_TOOLCHAIN=$(./scripts/install-host-toolchain.sh)" >> "$GITHUB_ENV"
#
# WHY `RUSTUP_TOOLCHAIN=stable` on the rustup call: `rust-toolchain.toml` says
# `channel = "esp"`, which on a machine that has not run `just setup` does not
# exist, and a bare `rustup toolchain install` would resolve through it. (The
# justfile's `CC_RUST_TOOLCHAIN` switch would not help -- rustup never reads it.)
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${repo_root}/.mise.toml"

# Whitespace-tolerant: the key lives in `[vars]` and is therefore indented. A
# `^`-anchored pattern matched nothing once already, in this repo, and the empty
# result reached espup as an argument.
version="$(sed -n 's/^[[:space:]]*host_toolchain = "\(.*\)"/\1/p' "$manifest")"
if [ -z "$version" ]; then
    echo "install-host-toolchain: cannot read host_toolchain from ${manifest}." >&2
    echo "  It lives in the [vars] table, so it is indented -- if that table" >&2
    echo "  moved again, fix this pattern rather than the value." >&2
    exit 1
fi

if rustup toolchain list | grep -q "^${version}"; then
    echo "${version}"
    exit 0
fi

echo "installing host toolchain ${version}" >&2
RUSTUP_TOOLCHAIN=stable rustup toolchain install "${version}" \
    --profile minimal --component clippy,rustfmt >&2
echo "${version}"
