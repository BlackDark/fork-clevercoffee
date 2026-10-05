#!/usr/bin/env python3
"""Fail if any relative markdown link in this repository is broken.

Added 2026-10-04. Sixteen relative links had rotted unnoticed -- nine of them in
`.agents/skills/esp32-rust-migration/SKILL.md`, the file every agent reads first,
which is the worst place for a link that leads nowhere. A doc cleanup that moves
files cannot be reviewed by eye alone, and this is the check that makes such a
move safe to do again.

Vendor trees are excluded: `.embuild/` is a checkout of ESP-IDF, `lib/` is
third-party C++, and neither is ours to fix.

Deliberately checks *existence*, not content. A link to a file that exists but
says the wrong thing is a documentation defect, not a broken link, and conflating
the two would make this check unreliable.
"""

from __future__ import annotations

import os
import re
import sys

SKIP_DIRS = {
    ".git", "node_modules", "target", ".pio", ".embuild", "lib", "ui",
    ".venv", "__pycache__",
}
# `[text](target)` — the target is the first group; angle brackets and titles are
# handled by the two cleanups below.
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
SCHEMES = ("http://", "https://", "mailto:", "tel:", "#")


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    checked = 0
    broken: list[tuple[str, str]] = []

    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if not name.endswith(".md"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8", errors="replace") as handle:
                text = handle.read()
            for match in LINK.finditer(text):
                raw = match.group(1).strip()
                if raw.startswith("<") and ">" in raw:
                    raw = raw[1 : raw.index(">")]
                target = raw.split("#", 1)[0].split(" ", 1)[0].strip()
                if not target or target.startswith(SCHEMES):
                    continue
                checked += 1
                if not os.path.exists(os.path.normpath(os.path.join(dirpath, target))):
                    broken.append((os.path.relpath(path, root), raw))

    if broken:
        print(f"FAIL: {len(broken)} broken relative markdown link(s) of {checked} checked")
        for source, target in sorted(set(broken)):
            print(f"  {source} -> {target}")
        return 1

    print(f"PASS: {checked} relative markdown links, none broken")
    return 0


if __name__ == "__main__":
    sys.exit(main())
