#!/usr/bin/env python3
"""Every ``unsafe`` in the workspace carries a written reason.

The workspace sets ``unsafe_code = "deny"`` (``Cargo.toml``), so an ``unsafe``
block does not compile unless it opts back in. This script checks the *second*
half of that bargain: that each opt-in has a ``reason =`` explaining it.

Why a script and not a lint: ``clippy::undocumented_unsafe_blocks`` exists, but
it is nightly-only and it does not understand a module-level
``#![allow(unsafe_code, reason = "...")]`` — which is the form three of the four
device-side files use, because the module's whole design *is* the reason.

How it decides (the rule is deliberately simple enough to argue with):

* find each ``unsafe`` site (``unsafe {``, ``unsafe fn``, ``unsafe impl``, …);
* walk **backwards** to the nearest enclosing ``#[allow(`` / ``#![allow(``,
  stopping at the ``)]`` that closes any attribute we passed on the way — a
  reason string can wrap for thirty lines, so a fixed line window would
  produce false positives;
* accept it if the attribute text names ``unsafe_code`` **and** ``reason =``;
* also accept a module-level ``#![allow(unsafe_code, reason = "...")]`` declared
  in the first 200 lines of the same file, which is how a whole module of
  device FFI declares itself.

Everything else is reported. Exit 0 = clean, 1 = at least one bare opt-in.
"""

from __future__ import annotations

import pathlib
import re
import sys

UNSAFE_SITES = re.compile(
    r"^\s*(pub(\([^)]*\))?\s+)?unsafe\s*(\{|fn\b|impl\b|trait\b|extern\b|auto\b)"
)
ALLOW_OPEN = re.compile(r"#!?\[\s*(allow|expect)\s*\(")
ATTR_CLOSE = re.compile(r"^\s*\)\s*\]?\s*$")
NEEDS_UNSAFE_CODE = re.compile(r"\bunsafe_code\b")
NEEDS_REASON = re.compile(r"\breason\s*=")

# A module-level declaration lives in the header of a file, never 200 lines in.
MODULE_HEADER_LINES = 200

SKIP_DIRS = {"target", ".embuild", "__pycache__", "node_modules", ".git"}


def _attribute_ending_at(lines: list[str], open_index: int) -> str:
    """The full text of a ``#[...]`` attribute that starts at ``lines[open_index]``.

    Reason strings here wrap for thirty lines, so an attribute has to be read to
    its closing ``)]`` rather than to the end of its first line -- but it must
    ALSO stop at the first line that begins in column 0, because that is the
    start of the next item and anything read past it is not part of this
    attribute. (Without that bound a single-line ``#[must_use]`` swallows the
    rest of the file and "finds" an ``allow`` that belongs to another item.)
    """
    buf = [lines[open_index]]
    if lines[open_index].rstrip().endswith(")]"):
        return buf[0]
    for line in lines[open_index + 1 : open_index + 200]:
        buf.append(line)
        if ATTR_CLOSE.match(line):
            break
        if line and not line[0].isspace():
            break
    return "\n".join(buf)


def _governing_allow(lines: list[str], site_index: int) -> str | None:
    """The nearest ``allow``/``expect`` attribute that governs ``lines[site_index]``.

    Walks backwards, skipping comments and doc text, to the first attribute
    opener. If that attribute is an ``allow``, it is the governor; if it is
    anything else (``#[cfg]``, ``#[inline]``), the site has no allow of its own
    and the scan continues past it.
    """
    for i in range(site_index - 1, max(-1, site_index - 1 - 400), -1):
        line = lines[i].lstrip()
        if not line.startswith("#"):
            # A comment, a doc line, or code. Not an attribute boundary, so keep
            # walking -- this is what lets a 30-line `reason =` string work.
            continue
        if not line.startswith("#["):
            continue
        text = _attribute_ending_at(lines, i)
        if ALLOW_OPEN.search(text):
            return text
        # Some other attribute: it does not grant the allow, so look further out.
    return None


def _module_allow(lines: list[str]) -> str | None:
    """A module-level ``#![allow(unsafe_code, reason = ...)]`` declared in a header."""
    for i, line in enumerate(lines[:MODULE_HEADER_LINES]):
        if line.lstrip().startswith("#![") and ALLOW_OPEN.search(line):
            return _attribute_ending_at(lines, i)
    return None


def main(argv: list[str]) -> int:
    root = pathlib.Path(argv[1] if len(argv) > 1 else ".")
    crates = root / "crates"
    if not crates.is_dir():
        print(f"no crates/ under {root}", file=sys.stderr)
        return 2

    offenders: list[str] = []
    sites = 0
    justified = 0

    for path in sorted(crates.rglob("*.rs")):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        module_allow = _module_allow(lines)
        module_ok = bool(
            module_allow
            and NEEDS_UNSAFE_CODE.search(module_allow)
            and NEEDS_REASON.search(module_allow)
        )
        for i, line in enumerate(lines):
            if not UNSAFE_SITES.match(line):
                continue
            sites += 1
            governing = _governing_allow(lines, i)
            ok = bool(
                governing
                and NEEDS_UNSAFE_CODE.search(governing)
                and NEEDS_REASON.search(governing)
            ) or module_ok
            if ok:
                justified += 1
            else:
                try:
                    where = f"{path.relative_to(root)}:{i + 1}"
                except ValueError:
                    where = f"{path}:{i + 1}"
                offenders.append(f"{where}: {line.strip()}")

    print(f"[unsafe-audit] {sites} unsafe sites in crates/")
    print(f"[unsafe-audit] {justified} carry a written reason")

    if offenders:
        print(f"[unsafe-audit] {len(offenders)} without a reason:", file=sys.stderr)
        for line in offenders:
            print(f"  {line}", file=sys.stderr)
        print(
            '[unsafe-audit] every `unsafe` needs '
            '`#[allow(unsafe_code, reason = "...")]` naming why it is sound',
            file=sys.stderr,
        )
        return 1

    print("[unsafe-audit] every unsafe site is justified in writing")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
