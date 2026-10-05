#!/usr/bin/env python3
"""Per-bucket flash attribution for the device image, by symbol.

07-image-size-budget.md §8 requires that an increase in image size is
"attributed by crate and by largest symbol, so an increase is never
unexplained". The release ELF is stripped (`profile.release.strip =
"symbols"`), so the symbols come from the `diagnostic` profile, which is
codegen-identical (`just diag-build`).

The method is `nm --size-sort`: every symbol's size is taken from the ELF
symbol table, and the size is charged to the first bucket whose pattern
matches, so the buckets sum to less than the image (alignment padding, and
symbols no pattern claims).

Usage:
    just diag-build
    python3 scripts/size-buckets.py target/xtensa-esp32-espidf/diagnostic/firmware
"""

from __future__ import annotations

import re
import subprocess
import sys
from collections import Counter

# Order matters: the first match wins, so the specific patterns come before
# the general ones. `mbedtls` must precede `libc`/`LIBC` and the IDF
# components, because the IDF TLS shim (`esp-tls`) is mbedTLS in all but name.
BUCKETS: list[tuple[str, str]] = [
    ("std::backtrace + addr2line", r"backtrace|addr2line|__rust_(begin|end)_short"),
    ("gimli / object_rs DWARF reader", r"gimli|object_rs|4object|miniz_oxide|8rustc_demangle"),
    ("mbedTLS / TLS", r"mbedtls|esp_tls|4esp-tls|mbedx509|mbedcrypto|9wpa_supplicant|wpa_"),
    ("serde", r"5serde|10serde_json"),
    ("core::fmt", r"4core3fmt|3fmt|Formatter|4core3panicking"),
    ("printf family", r"printf|vprintf|vsnprintf|snprintf|sprintf|asprintf|strtod|strtof|puts"),
    ("http parser", r"http_parser|13http11_parser"),
    ("lwIP", r"lwip|4tcp4|4udp4|netconn|ip4_addr"),
    ("FreeRTOS", r"freertos|9tasks|10queue|list\.o|portmacro"),
    ("newlib / libc", r"newlib|esp-idf/[^/]*/lib/libc\.a|\b_p?rint"),
]


def symbol_sizes(nm: str, elf: str) -> list[tuple[int, str, str]]:
    out = subprocess.run(
        [nm, "--size-sort", "-S", "-r", elf],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    symbols = []
    for line in out.splitlines():
        parts = line.split(maxsplit=3)
        if len(parts) < 4:
            continue
        _addr, size, _kind, name = parts
        symbols.append((int(size, 16), parts[2], name))
    return symbols


def main() -> int:
    elf = sys.argv[1] if len(sys.argv) > 1 else "target/xtensa-esp32-espidf/diagnostic/firmware"
    nm = sys.argv[2] if len(sys.argv) > 2 else "xtensa-esp32-elf-nm"
    try:
        symbols = symbol_sizes(nm, elf)
    except FileNotFoundError:
        print(f"no {nm} on PATH; set XTTENSA_NM=/path/to/xtensa-esp32-elf-nm", file=sys.stderr)
        return 2

    compiled = [(name, re.compile(pat)) for name, pat in BUCKETS]
    bytes_by_bucket: Counter[str] = Counter()
    count_by_bucket: Counter[str] = Counter()
    for size, _kind, name in symbols:
        for bucket, pattern in compiled:
            if pattern.search(name):
                bytes_by_bucket[bucket] += size
                count_by_bucket[bucket] += 1
                break

    total = sum(size for size, _k, _n in symbols)
    print(f"{elf}")
    print(f"{'bucket':34} {'bytes':>10} {'symbols':>8}")
    for bucket, _pat in BUCKETS:
        if count_by_bucket[bucket]:
            print(f"{bucket:34} {bytes_by_bucket[bucket]:>10,} {count_by_bucket[bucket]:>8,}")
    print(f"{'--- named buckets subtotal':34} {sum(bytes_by_bucket.values()):>10,} "
          f"{sum(count_by_bucket.values()):>8,}")
    print(f"{'all symbols':34} {total:>10,} {len(symbols):>8,}")

    # Static RAM is the other number 07 §5 requires, and the same symbol table
    # carries it: `.bss` is zeroed at boot, `.data` is copied from flash.
    ram = Counter()
    ram_symbols = Counter()
    for size, kind, name in symbols:
        if kind in ("b", "B"):
            ram[".bss"] += size
            ram_symbols[".bss"] += 1
        elif kind in ("d", "D", "g", "G"):
            ram[".data"] += size
            ram_symbols[".data"] += 1
    print()
    print(f"{'static RAM (named symbols)':34} {sum(ram.values()):>10,} {sum(ram_symbols.values()):>8,}")
    for section in (".data", ".bss"):
        print(f"  {section:32} {ram[section]:>10,} {ram_symbols[section]:>8,}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
