#!/usr/bin/env python3
"""Fail if any relative markdown link in this repository is broken.

Added 2026-10-04. Sixteen relative links had rotted unnoticed -- nine of them in
the migration execution skill (since removed), the file every agent read first
at the time, which is the worst place for a link that leads nowhere. A doc
cleanup that moves files cannot be reviewed by eye alone, and this is the check
that makes such a
move safe to do again.

Vendor trees are excluded: `.embuild/` is a checkout of ESP-IDF, `lib/` is
third-party C++, and neither is ours to fix.

Deliberately checks *existence*, not content. A link to a file that exists but
says the wrong thing is a documentation defect, not a broken link, and conflating
the two would make this check unreliable.

BACKTICKED PATHS ARE NOW CHECKED. Existence and anchors cover markdown link
syntax; they cannot see `` `scripts/foo.py` `` in prose. That is how a live skill
note went on citing a deleted script with every gate green. Only paths under the
repository's own directories are checked, and only when they look like a real
path: a `git show <rev>:<path>` form, or anything with an ellipsis, is a citation
of history rather than a pointer at the working tree. `docs/archive/` and
`docs/history/` are excluded entirely — their whole job is to record what was
true when, and AG-REPO-29 and AG-REPO-30 say so.

FRAGMENTS ARE NOW CHECKED. Existence alone was not enough: the divergence ledger
is renumbered as the port finds more divergences, and 143 references into it were
`§N` prose pointing at a section number. Renumbering made 19 of
`differences.md`'s 31 pointers wrong, and this check passed throughout, because it
stripped the fragment and never looked. It now resolves `#anchor` against the
target file's headings and fails if there is no such heading, which is what makes
the anchored links `scripts/check-divergence-refs.py` writes actually mean
something.
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
HEADING = re.compile(r"^#{1,6}\s+(.*)$", re.MULTILINE)
# Directories whose documents cite history by design (AG-REPO-29, AG-REPO-30).
# A path named in an archived document is evidence of what was true then, not a
# pointer at the working tree, so the prose check does not apply there.
HISTORY_DIRS = (os.path.join("docs", "archive"), os.path.join("docs", "history"))

# A backticked path into the repository's own tree.
CODE_PATH = re.compile(
    r"`((?:scripts|crates|just|docs|ui|tools|\.agents|\.github)/[\w./-]+"
    r"\.(?:py|sh|rs|just|md|ya?ml|json|jsonl|ts|tsx|c|h|cpp))`"
)
STATUS_WORDS = re.compile(
    r"\s*(?:🔴|🟡)?\s*(?:closed|added|fixed|changed|new|reversed|open|known)\s*$",
    re.IGNORECASE,
)


def slugify(title: str) -> str:
    """GitHub's anchor algorithm, as far as this repository's headings need it."""
    s = re.sub(r"[🔴🟡]", "", title).strip()
    s = STATUS_WORDS.sub("", s).strip().lower()
    s = s.replace("`", "").replace("*", "").replace("~~", "")
    s = re.sub(r"[^\w\s-]", "", s, flags=re.UNICODE)
    return re.sub(r"\s", "-", s.strip())


def anchors_in(path: str) -> set[str]:
    """Every anchor a markdown file offers, including explicit `{#id}` ones."""
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            text = handle.read()
    except OSError:
        return set()
    found: set[str] = set()
    for m in HEADING.finditer(text):
        title = m.group(1)
        explicit = re.search(r"\{#([^}]+)\}", title)
        found.add(explicit.group(1) if explicit else slugify(title))
    # A non-heading target may still carry an HTML anchor.
    for m in re.finditer(r'id="([^"]+)"', text):
        found.add(m.group(1))
    return found


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    checked = 0
    fragment_checked = 0
    broken: list[tuple[str, str]] = []
    bad_fragment: list[tuple[str, str, str]] = []
    bad_path: list[tuple[str, str]] = []
    anchor_cache: dict[str, set[str]] = {}

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
                if "#" in raw:
                    target_part, fragment = raw.split("#", 1)
                    fragment = fragment.split(" ", 1)[0].strip()
                else:
                    target_part, fragment = raw, ""
                target = target_part.split(" ", 1)[0].strip()
                if not target or target.startswith(SCHEMES):
                    continue
                for cp in CODE_PATH.findall(text):
                    # `git show <rev>:<path>` and truncated forms cite history.
                    if ":" in cp or ".." in cp or cp.endswith("/"):
                        continue
                    rel_from_root = os.path.relpath(path, root)
                    if rel_from_root.startswith(HISTORY_DIRS):
                        continue
                    checked += 1
                    # A backticked path is written the way a human says it, so
                    # it resolves against the repository root even when the file
                    # citing it lives three directories down.
                    if os.path.exists(os.path.join(root, cp)):
                        continue
                    if os.path.exists(os.path.normpath(os.path.join(dirpath, cp))):
                        continue
                    bad_path.append((os.path.relpath(path, root), cp))

                checked += 1
                resolved = os.path.normpath(os.path.join(dirpath, target))
                if not os.path.exists(resolved):
                    broken.append((os.path.relpath(path, root), raw))
                    continue
                if fragment and resolved.endswith(".md"):
                    fragment_checked += 1
                    if resolved not in anchor_cache:
                        anchor_cache[resolved] = anchors_in(resolved)
                    if fragment.lower() not in anchor_cache[resolved]:
                        bad_fragment.append(
                            (os.path.relpath(path, root), fragment,
                             os.path.relpath(resolved, root))
                        )

    if broken or bad_fragment or bad_path:
        if broken:
            print(f"FAIL: {len(broken)} broken relative markdown link(s) of {checked} checked")
            for source, target in sorted(set(broken)):
                print(f"  {source} -> {target}")
        if bad_fragment:
            print(
                f"FAIL: {len(bad_fragment)} link(s) point at a heading that does not exist "
                f"({fragment_checked} fragment(s) checked)"
            )
            for source, fragment, target in sorted(set(bad_fragment)):
                print(f"  {source} -> {target}#{fragment}")
        if bad_path:
            uniq = sorted(set(bad_path))
            print(
                f"FAIL: {len(uniq)} backticked path(s) in prose do not exist "
                f"(markdown links cannot see these)"
            )
            for source, cp in uniq:
                print(f"  {source} -> {cp}")
        return 1

    extra = f", {fragment_checked} heading anchors resolved" if fragment_checked else ""
    print(f"PASS: {checked} relative markdown links, none broken{extra}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
