#!/usr/bin/env python3
"""Fail if `docs/api/openapi.yaml` has drifted from the routes the firmware serves.

WHY THIS EXISTS
---------------
`openapi.yaml` sat in the repository for the whole port with no consumer, no CI
check, and no row in `docs/index.md`. It happened to be accurate for 24 of the
28 registered routes, and the four it missed were not random: `/api/sleep`,
`/api/wake` and `/events` are exactly the endpoints carrying deliberate
divergences. An API reference that omits the routes where the firmware behaves
differently *by design* is worse than none, because it looks complete.

This is the guard that stops the next omission from being invisible. It is cheap
and needs no toolchain: both sides are one regex away.

WHAT IT COMPARES
----------------
The `ROUTES` table in `crates/cc-hal-esp32/src/web.rs`, which is what the device
actually registers, against the `paths:` keys in the spec. Wildcard entries
(`/api*`, `/ui*`) are skipped on both sides: they are preflight and asset-serving
catch-alls, not endpoints a client calls.

A route present in one and not the other is a failure in BOTH directions. A spec
path with no route is a promise the firmware does not keep.

    python3 scripts/check-openapi.py .
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROUTE_RE = re.compile(r'\("(/[^"]*)",\s*Method::')
SPEC_PATH_RE = re.compile(r"^  (/[^\s:]*):", re.MULTILINE)


def routes_in_firmware(root: Path) -> set[str]:
    src = (root / "crates/cc-hal-esp32/src/web.rs").read_text(encoding="utf-8")
    return {r for r in ROUTE_RE.findall(src) if not r.endswith("*")}


def paths_in_spec(root: Path) -> set[str]:
    text = (root / "docs/api/openapi.yaml").read_text(encoding="utf-8")
    return set(SPEC_PATH_RE.findall(text))


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    routes = routes_in_firmware(root)
    paths = paths_in_spec(root)
    if not routes or not paths:
        print(
            "check-openapi: parsed nothing -- the ROUTES table or the spec moved shape",
            file=sys.stderr,
        )
        return 1

    missing = sorted(routes - paths)
    invented = sorted(paths - routes)
    if not missing and not invented:
        print(f"check-openapi: OK ({len(routes)} routes, spec matches)")
        return 0

    for r in missing:
        print(f"::error::docs/api/openapi.yaml is missing the route {r}")
    for p in invented:
        print(f"::error::docs/api/openapi.yaml documents {p}, which no route serves")
    print(
        f"check-openapi: {len(missing)} missing, {len(invented)} documented but unserved.\n"
        "                  Add the missing ones, delete the invented ones.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
