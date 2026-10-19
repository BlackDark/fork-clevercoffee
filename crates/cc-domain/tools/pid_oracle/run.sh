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
#
# One line of the output is host-dependent: the `%.17g` column prints the NaN
# that scenario D produces with the compiler's own sign (`nan` on one host,
# `-nan` on another, and the bit patterns differ in the sign bit). The Rust side
# compares NaN to NaN, so the regeneration is otherwise exact.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../../../.." && pwd)"

# The reference implementation, pinned: the vectors were produced from this
# commit, so the oracle must compile it and not whatever the branch holds today.
# Fetched rather than vendored, the way the display oracle fetches U8g2.
pid_repo="https://github.com/rancilio-pid/Arduino-PID-Library"
pid_commit="d6d3c6967e62f477be116ea34afc17104fd428a9"
lib_dir="${CC_PID_LIBRARY_DIR:-${repo_root}/target/pid-library}"

if [ ! -f "${lib_dir}/PID_v1.cpp" ]; then
    git clone --quiet "${pid_repo}" "${lib_dir}"
    git -C "${lib_dir}" checkout --quiet "${pid_commit}"
fi

actual="$(git -C "${lib_dir}" rev-parse HEAD 2>/dev/null || true)"
if [ "${actual}" != "${pid_commit}" ]; then
    echo "pid_oracle: the reference library at ${lib_dir} is not the pinned" >&2
    echo "  commit ${pid_commit}." >&2
    echo "  Remove that directory and re-run, or point CC_PID_LIBRARY_DIR at" >&2
    echo "  a checkout of it." >&2
    exit 1
fi

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
