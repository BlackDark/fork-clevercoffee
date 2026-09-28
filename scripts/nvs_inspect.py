#!/usr/bin/env python3
"""Inspect an ESP-IDF NVS partition image: namespaces, keys, types, sizes.

Why this exists
---------------
The C++ firmware stores its whole configuration in NVS namespace ``config``
under keys derived as ``"p" + fnv1a_hash(dotted.path)`` in hex, with no schema
version and no migration path. The Rust port has to read that back byte-exactly.
This tool is how we check what is actually on a device instead of trusting the
code, and how we verify after a Rust write that the layout is unchanged.

It is read-only and it operates on a dumped image, never on the device.

Secrets
-------
NVS holds Wi-Fi credentials. This tool prints key names, types and value *sizes*
only. It never prints a value, and there is no flag to make it. If you need to
compare values, compare hashes with ``--digest``, which prints a truncated
SHA-256 of each value rather than the value.

Usage
-----
    espflash read-flash --port /dev/cu.usbserial-XXXX 0x9000 0x5000 nvs.bin
    python3 scripts/nvs_inspect.py nvs.bin
    python3 scripts/nvs_inspect.py nvs.bin --namespace config --digest
"""

from __future__ import annotations

import argparse
import hashlib
import struct
import sys
from dataclasses import dataclass, field

PAGE_SIZE = 4096
HEADER_SIZE = 32
BITMAP_SIZE = 32
ENTRY_SIZE = 32
ENTRIES_PER_PAGE = 126

# Entry states from the 2-bit-per-entry bitmap.
STATE_WRITTEN = 0b10

TYPES = {
    0x01: "u8",
    0x11: "i8",
    0x02: "u16",
    0x12: "i16",
    0x04: "u32",
    0x14: "i32",
    0x08: "u64",
    0x18: "i64",
    0x21: "str",
    0x41: "blob_idx",
    0x42: "blob",
    0x48: "blob_data",
}
PRIMITIVE_WIDTH = {"u8": 1, "i8": 1, "u16": 2, "i16": 2, "u32": 4, "i32": 4, "u64": 8, "i64": 8}
VARIABLE = {"str", "blob", "blob_data", "blob_idx"}


@dataclass
class Item:
    ns_index: int
    key: str
    type_name: str
    size: int
    digest: str | None = None


@dataclass
class Partition:
    namespaces: dict[int, str] = field(default_factory=dict)
    items: list[Item] = field(default_factory=list)
    pages_active: int = 0
    pages_total: int = 0


def _entry_state(bitmap: bytes, index: int) -> int:
    byte = bitmap[index // 4]
    return (byte >> ((index % 4) * 2)) & 0b11


def parse(image: bytes, want_digest: bool) -> Partition:
    part = Partition()
    part.pages_total = len(image) // PAGE_SIZE

    for page_no in range(part.pages_total):
        page = image[page_no * PAGE_SIZE : (page_no + 1) * PAGE_SIZE]
        (state,) = struct.unpack_from("<I", page, 0)
        if state == 0xFFFFFFFF:  # never written
            continue
        part.pages_active += 1

        bitmap = page[HEADER_SIZE : HEADER_SIZE + BITMAP_SIZE]
        body = page[HEADER_SIZE + BITMAP_SIZE :]

        i = 0
        while i < ENTRIES_PER_PAGE:
            if _entry_state(bitmap, i) != STATE_WRITTEN:
                i += 1
                continue

            raw = body[i * ENTRY_SIZE : (i + 1) * ENTRY_SIZE]
            if len(raw) < ENTRY_SIZE:
                break
            ns_index, type_code, span = raw[0], raw[1], raw[2]
            key = raw[8:24].split(b"\x00", 1)[0].decode("utf-8", "replace")
            data = raw[24:32]
            type_name = TYPES.get(type_code, f"0x{type_code:02x}")

            if not key or span == 0:
                i += 1
                continue

            # A namespace declaration: ns_index 0, the key is the namespace name,
            # the u8 value is the index other entries refer to.
            if ns_index == 0 and type_name == "u8":
                part.namespaces[data[0]] = key
                i += span
                continue

            if type_name in VARIABLE:
                (size,) = struct.unpack_from("<H", data, 0)
                payload = body[(i + 1) * ENTRY_SIZE : (i + 1) * ENTRY_SIZE + size]
            else:
                size = PRIMITIVE_WIDTH.get(type_name, 8)
                payload = data[:size]

            digest = hashlib.sha256(payload).hexdigest()[:12] if want_digest else None
            part.items.append(Item(ns_index, key, type_name, size, digest))
            i += max(span, 1)

    return part


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("image", help="NVS partition dump (espflash read-flash 0x9000 0x5000)")
    ap.add_argument("--namespace", help="only show this namespace")
    ap.add_argument(
        "--digest",
        action="store_true",
        help="show a truncated SHA-256 of each value instead of nothing (still never the value)",
    )
    args = ap.parse_args()

    with open(args.image, "rb") as fh:
        image = fh.read()
    if len(image) % PAGE_SIZE:
        print(f"warning: image is not a whole number of {PAGE_SIZE}-byte pages", file=sys.stderr)

    part = parse(image, args.digest)

    print(f"pages: {part.pages_active} written / {part.pages_total} total")
    print(f"namespaces: {len(part.namespaces)}")
    for idx, name in sorted(part.namespaces.items()):
        count = sum(1 for it in part.items if it.ns_index == idx)
        print(f"  [{idx}] {name}  ({count} keys)")
    print()

    wanted = None
    if args.namespace:
        matches = [i for i, n in part.namespaces.items() if n == args.namespace]
        if not matches:
            print(f"namespace {args.namespace!r} not found", file=sys.stderr)
            return 1
        wanted = matches[0]

    shown = [it for it in part.items if wanted is None or it.ns_index == wanted]
    hdr = f"{'ns':<14} {'key':<18} {'type':<10} {'bytes':>6}"
    if args.digest:
        hdr += "  sha256[:12]"
    print(hdr)
    print("-" * len(hdr))
    for it in sorted(shown, key=lambda x: (part.namespaces.get(x.ns_index, ""), x.key)):
        ns = part.namespaces.get(it.ns_index, f"?{it.ns_index}")
        line = f"{ns:<14} {it.key:<18} {it.type_name:<10} {it.size:>6}"
        if args.digest:
            line += f"  {it.digest}"
        print(line)
    print()
    print(f"{len(shown)} keys shown. Values are never printed by this tool.")

    # Type histogram: this is what tells us which read path the Rust port needs.
    hist: dict[str, int] = {}
    for it in shown:
        hist[it.type_name] = hist.get(it.type_name, 0) + 1
    print("value types: " + ", ".join(f"{k}={v}" for k, v in sorted(hist.items())))
    if hist.keys() & {"blob", "blob_data", "blob_idx"}:
        print(
            "note: Arduino Preferences stores putFloat/putDouble as BLOBs, so those\n"
            "      must be read with get_blob + f32/f64::from_le_bytes, not get_u32."
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
