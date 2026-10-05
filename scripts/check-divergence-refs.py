#!/usr/bin/env python3
"""Rewrite `§N` pointers into `divergences.md` into title-anchored links.

WHY THIS EXISTS
---------------
The ledger `docs/history/divergences.md` gets renumbered — it already has been
twice — and every prose `§N` reference to it silently rots when that happens.
The renumber that produced 34 sections updated 5 ledger `heading` fields and two
references in one other file, and left 19 of `differences.md`'s 31 pointers
pointing at the wrong section. Nothing caught it: `cc-parity` only checks that a
ledger entry's `heading` string still appears in the prose, and only 5 entries
have one.

So `§N` is removed as a linkable form. A reference becomes

    §13                          (was: rot-prone, unreadable if stale)
    [§13](./divergences.md#13-the-devices-default-hostname-is-test-cc-rust-not-silvia)

and the existing `scripts/check-doc-links.py` verifies the anchor forever. A
renumber can then break nothing, because nothing depends on the number.

WHY A NUMBER AT ALL, THEN
-------------------------
Because `§13` is what a human says out loud. The number is the handle; the
anchor is what the document is pinned by. Keeping both means a wrong number is
visible in review rather than silent.

    python3 scripts/check-divergence-refs.py --fix .
    python3 scripts/check-divergence-refs.py .          # report only

Idempotent: rewriting an already-anchored reference is a no-op.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

LEDGER = Path("docs/history/divergences.md")

# Both heading forms exist: `## 7. Title` and `## 31 — Title`. A regex that
# only accepts the period form silently loads 30 of 34 sections and then reports
# the other four as nonexistent, which looks like a broken reference rather than
# a broken parser.
HEADING_RE = re.compile(r"^## (\d+)\s*(?:\.|—)\s*(.*)$", re.MULTILINE)

# A bare `§N` that is NOT already inside a markdown link. Deliberately narrow:
# it must not touch `§11` references to cpp-findings, which use a different
# document, or `§4` of some other file.
BARE_REF_RE = re.compile(r"(?<![\[/\w])§(\d+)(?![0-9])")

# The documents that reference the ledger.
#
# DELIBERATELY NOT LISTED: `target-architecture.md`, `feature-inventory.md`,
# `dependency-evaluation.md`, `cpp-findings.md`. Those files are themselves
# organised in numbered sections, so a bare `§N` in them is overwhelmingly a
# SELF-reference (`§3.1` means §3.1 of that document). An earlier version of this
# script assumed otherwise and rewrote those into ledger links, which pointed
# readers at entirely unrelated divergences. `cpp-findings.md` is listed because
# its `§N` uses are all cross-document, but it also keeps duplicate numbers, so
# its OWN headings carry explicit `{#cfNN}` anchors instead.
CONSUMERS = [
    "docs/differences.md",
    "docs/status.md",
    "docs/architecture.md",
    "docs/glossary.md",
    "docs/config/reference.md",
    "docs/control/state-machine.md",
    "docs/display/overview.md",
    "docs/display/layout-rules.md",
    "docs/display/parity.md",
    "docs/hardware/pins.md",
    "docs/protocols/sensors.md",
    "docs/web/http-and-ui.md",
    "docs/operations/runbook.md",
    "docs/operations/ci.md",
    "AGENTS.md",
    "README.md",
    "CONTRIBUTING.md",
    "GLOSSARY.md",
    "docs/index.md",
    "docs/history/cpp-findings.md",
    "docs/history/recovered-oracle.md",
    "docs/history/scenario-format.md",
    "docs/history/review-2026-10-03.md",
    "docs/history/outstanding-findings.md",
    "docs/history/cpp-behaviour-comparisons.md",
    "docs/history/README.md",
]


STATUS_WORDS = re.compile(
    r"\s*(?:🔴|🟡)?\s*(?:closed|added|fixed|changed|new|reversed|open|known)\s*$",
    re.IGNORECASE,
)


def github_slug(title: str) -> str:
    """GitHub's heading anchor algorithm, as far as these titles need it.

    The trailing status word (`closed`, `added`, `fixed`, …) is metadata that
    rides along on the heading, not part of the section's name, and the emoji is
    two codepoints that GitHub renders inconsistently. Both are stripped so the
    slug is deterministic and a hand-written link can be checked against it.
    """
    s = re.sub(r"[🔴🟡]", "", title).strip()
    s = STATUS_WORDS.sub("", s).strip()
    s = s.lower()
    # Strip markdown emphasis, ticks and strikethrough markers.
    s = s.replace("`", "").replace("*", "").replace("~~", "")
    # Drop everything that is not alphanumeric, space, hyphen or underscore.
    s = re.sub(r"[^\w\s-]", "", s, flags=re.UNICODE)
    s = re.sub(r"\s", "-", s.strip())
    return s


def load_sections(root: Path) -> dict[int, tuple[str, str]]:
    """Map each section number to its title and its GitHub anchor.

    The anchor is built from the WHOLE heading line, number included: GitHub
    slugs `## 22. Title` to `22-title`, so a slug built from the title alone
    points at nothing. Getting this wrong fails in a way that looks like a broken
    link rather than a wrong slug, which is why it is asserted by
    `check-doc-links.py` rather than trusted.
    """
    text = (root / LEDGER).read_text(encoding="utf-8")
    out = {}
    for m in HEADING_RE.finditer(text):
        number = int(m.group(1))
        # `## 31 — Title` leaves a leading separator in the capture depending on
        # how the dash is encoded. Strip it here rather than in the pattern, so
        # the slug is right whichever form the heading uses.
        title = re.sub(r"^[\s.—–-]+", "", m.group(2)).strip()
        out[number] = (title, github_slug(f"{number}. {title}"))
    return out


def rewrite(text: str, sections: dict, rel: str, apply: bool) -> tuple[str, int, list[str]]:
    """Replace bare `§N` with an anchored link, when N is a ledger section."""
    problems: list[str] = []
    count = 0

    def repl(m: re.Match) -> str:
        nonlocal count
        n = int(m.group(1))
        if n not in sections:
            problems.append(f"{rel}: §{n} does not exist in the ledger")
            return m.group(0)
        _, slug = sections[n]
        # relpath from the referencing file's own directory, not a fixed prefix.
        # `../` + the full ledger path is wrong from every depth: it resolves to
        # a sibling of the repository root from docs/, and to nowhere at all
        # from the root itself.
        rel_dir = os.path.dirname(rel) or "."
        target = os.path.relpath(LEDGER.as_posix(), rel_dir).replace(os.sep, "/")
        target = f"{target}#{slug}"
        count += 1
        return f"[§{n}]({target})"

    # Skip regions that are already links or that reference another document.
    out = []
    for line in text.split("\n"):
        # `09 §N` and `05 §N` are cpp-findings / archive references, not ledger.
        if re.search(r"\b(0[0-9]|archive|ADR)[^§]{0,4}§\d", line):
            out.append(line)
            continue
        out.append(BARE_REF_RE.sub(repl, line))
    return "\n".join(out), count, problems


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("root", nargs="?", default=".")
    ap.add_argument("--fix", action="store_true", help="rewrite instead of report")
    args = ap.parse_args()
    root = Path(args.root).resolve()

    sections = load_sections(root)
    if not sections:
        print("check-divergence-refs: no sections found in the ledger", file=sys.stderr)
        return 1

    changed_files = 0
    total = 0
    problems: list[str] = []
    for rel in CONSUMERS:
        f = root / rel
        if not f.is_file():
            continue
        text = f.read_text(encoding="utf-8")
        new, n, probs = rewrite(text, sections, rel, args.fix)
        problems.extend(probs)
        total += n
        if new != text:
            changed_files += 1
            if args.fix:
                f.write_text(new, encoding="utf-8")

    if problems:
        for p in problems:
            print(f"::error::{p}")
        return 1
    print(
        f"check-divergence-refs: OK ({len(sections)} sections, "
        f"{changed_files if args.fix else 0} file(s) rewritten, {total} reference(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
