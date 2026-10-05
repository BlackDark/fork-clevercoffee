#!/usr/bin/env bash
# Print this machine's host triple, or nothing if it cannot be determined.
#
# Why a script and not a backtick in the justfile:
#
# `host_target := \`rustc -vV | sed -n 's/^host: //p'\`` LOOKS fine and is a
# trap. just evaluates that backtick while PARSING the justfile, before any
# recipe runs, and a failing backtick is a parse error -- so on a machine where
# the toolchain `rust-toolchain.toml` names is not installed, `just` refuses to
# run AT ALL. That is not a degraded experience, it is a total one, and it is
# what broke the CI host job: the runner has stable Rust and does not have the
# Espressif `esp` nightly fork, so `rustc -vV` failed and every `just` command in
# that job failed with it.
#
# So this script is written to fail QUIETLY. It prefers `stable` explicitly --
# the host triple is a property of the machine, not of the toolchain, so asking
# a channel that is not the device's avoids depending on the device toolchain
# existing. It falls back to whatever `rustc` resolves to, and prints nothing
# rather than dying. An empty result is caught and explained by
# `just doctor-host`, which is the right place for a diagnostic.
set -uo pipefail

host=$(RUSTUP_TOOLCHAIN=stable rustc -vV 2>/dev/null | sed -n 's/^host: //p') ||
    host=""
if [ -z "$host" ]; then
    # No stable toolchain (a machine provisioned only with `esp`): ask whatever
    # rustc resolves to, still without failing the justfile parse.
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p') || host=""
fi

# One word, the triple, or nothing.
[ -n "$host" ] || exit 0
case "$host" in
    # A triple is three dash-separated components and nothing else. Anything
    # else means rustc printed something we did not expect, and passing it to
    # `cargo --target` would be worse than printing nothing.
    *-*-*) printf '%s\n' "$host" ;;
    *) exit 0 ;;
esac
