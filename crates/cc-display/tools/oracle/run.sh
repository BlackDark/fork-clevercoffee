#!/usr/bin/env bash
# Build and run the R2-10 display parity oracle.
#
#   crates/cc-display/tools/oracle/run.sh <scenario> <out.ppm>
#   crates/cc-display/tools/oracle/run.sh --all          # build only
#
# The oracle links the REAL U8g2 that the firmware links
# (`.pio/libdeps/esp32_usb/U8g2`, `olikraus/U8g2 @ 2.36.18` per
# `platformio.ini`) and the firmware's own `include/clevercoffee/display/bitmaps.h`.
# Only the Arduino platform underneath is shimmed
# (`tools/oracle/ArduinoShim.h`), same as `tools/pid_oracle/run.sh`.
#
# The build goes to `$CARGO_TARGET_DIR/display-oracle` so the source tree stays
# clean and no binary is ever left next to the committed artefacts.
#
# If U8g2 is not installed, run `pio run -e esp32_usb` (or
# `pio pkg install -e esp32_usb`) first; PlatformIO fetches it from the
# registry, and the oracle must link that exact tree rather than a download, or
# the parity claim is against the wrong library.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../../../.." && pwd)"

u8g2_dir="${repo_root}/.pio/libdeps/esp32_usb/U8g2"
if [ ! -d "${u8g2_dir}/src/clib" ]; then
    for candidate in "${repo_root}"/.pio/libdeps/*/U8g2; do
        if [ -d "${candidate}/src/clib" ]; then
            u8g2_dir="${candidate}"
            break
        fi
    done
fi
if [ ! -d "${u8g2_dir}/src/clib" ]; then
    echo "display_oracle: U8g2 not found under .pio/libdeps." >&2
    echo "  Run: pio run -e esp32_usb   (or: pio pkg install -e esp32_usb)" >&2
    exit 1
fi

out_dir="${CARGO_TARGET_DIR:-${repo_root}/target}/display-oracle"
mkdir -p "${out_dir}"
bin="${out_dir}/display_oracle"

cxx="${CXX:-c++}"
cc="${CC:-cc}"

# -O0 to match the `cargo test` (dev profile) build of the Rust side, so the two
# execute the identical sequence of IEEE-754 operations. See the same note in
# tools/pid_oracle/run.sh.
# U8g2's clib is C and must be compiled as C: `u8g2.h` is not valid C++ (it
# relies on C-only implicit conversions), and compiling it as C++ both warns and
# changes overload resolution in the `extern "C"` GPIO declarations. So each
# clib file goes through a separate C compiler invocation, and only
# `U8g2lib.cpp` is C++.
objects=()
for src in "${u8g2_dir}"/src/clib/*.c "${u8g2_dir}/src/U8g2lib.cpp"; do
    obj="${out_dir}/$(basename "${src}").o"
    objects+=("${obj}")
    case "${src}" in
        *.c) "${cc}" -std=c99 -O0 -w -DARDUINO=10819 -I "${here}" -I "${u8g2_dir}/src" -c "${src}" -o "${obj}" ;;
        *)   "${cxx}" -std=c++17 -O0 -w -DARDUINO=10819 -I "${here}" -I "${u8g2_dir}/src" -c "${src}" -o "${obj}" ;;
    esac
done

# Link to a unique name and move it into place. `cargo test` runs the tests in
# one binary concurrently, and both of them invoke this script; a fixed `-o`
# path makes the linker's `open(O_CREAT)` fail with EEXIST for one of them,
# which surfaces as "the oracle failed" and reads like a pixel difference.
# The object files above are content-stable and are shared deliberately --
# recompiling them per invocation would be far slower. `mv` within one
# directory is atomic, so a reader never sees a partial binary.
link_tmp="${bin}.$$"
trap 'rm -f "${link_tmp}"' EXIT
"${cxx}" \
    -std=c++17 -O0 -Wall \
    -DARDUINO=10819 \
    -I "${here}" \
    -I "${repo_root}/include" \
    -I "${u8g2_dir}/src" \
    -o "${link_tmp}" \
    "${here}/display_oracle.cpp" \
    "${objects[@]}"
mv -f "${link_tmp}" "${bin}"

if [ "${1:-}" = "--all" ]; then
    echo "built ${bin}"
    exit 0
fi

if [ $# -lt 2 ]; then
    echo "usage: run.sh <scenario> <out.ppm>   |   run.sh --all" >&2
    exit 2
fi

"${bin}" "$1" "$2"
