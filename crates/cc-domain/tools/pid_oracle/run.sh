#!/usr/bin/env bash
# Regenerate the PID parity vector.
#
#   crates/cc-domain/tools/pid_oracle/run.sh > crates/cc-domain/tools/pid_oracle/expected.txt
#
# Then copy the `C` lines into `crates/cc-domain/src/pid_parity.rs` (or better:
# `sed` them in — see the module doc there). The Rust test replays the same
# scenario and compares against the values below.
#
# Compiled at -O0 with no -ffast-math and no FMA contraction, matching the
# `cargo test` (dev profile) build of the Rust controller, so the two sides
# execute the identical sequence of IEEE-754 double operations.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../../../.." && pwd)"
lib_dir="${repo_root}/lib/Arduino-PID-Library"

# Build into the target directory so the source tree stays clean and the binary
# is never near the committed expected values.
out_dir="${CARGO_TARGET_DIR:-${repo_root}/target}/pid-oracle"
mkdir -p "${out_dir}"

cxx="${CXX:-c++}"

"${cxx}" \
    -std=c++17 -O0 -Wall -Wextra \
    -I "${here}" \
    -I "${lib_dir}" \
    -o "${out_dir}/pid_oracle" \
    "${here}/pid_oracle.cpp"

"${out_dir}/pid_oracle"
