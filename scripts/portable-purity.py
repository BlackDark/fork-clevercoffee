#!/usr/bin/env python3
"""The portable crates must not name ESP-IDF -- in CODE.

`cc-domain`, `cc-protocol`, `cc-netpolicy`, `cc-safety`, `cc-config`,
`cc-machine`, `cc-display`, `cc-web` and `cc-mqtt` are ``#![no_std]`` and
host-testable, which is the property that lets 1,191 tests run in seconds
without a board. An ``esp_idf_*`` item anywhere in them breaks every one of
those tests *and* every IDE's background check, and it does so invisibly: the
crate still compiles for the chip, it just stops compiling for the host.

Why this is a script and not a ``grep``
--------------------------------------
The first version of this check was ``rg 'esp_idf_(hal|svc|sys)'`` over the five
crates, and it fired on its own documentation:

    crates/cc-domain/src/lib.rs:11:  //! * This crate must never name `esp_idf_svc`, ...

Which is the wrong answer twice over -- the sentence is the rule, not a violation
of it, and silencing it would mean deleting the reason the rule exists. Comments
and doc-comments are stripped here before the search, so a portable crate can
still EXPLAIN why it does not use ESP-IDF, and only real code counts.

Exit codes: 0 = clean, 1 = a real reference, 2 = bad invocation.
"""

from __future__ import annotations

import pathlib
import re
import sys

# The crates 04 §6 calls portable, and the one grep they all share.
PORTABLE = (
    "cc-domain",
    "cc-protocol",
    "cc-netpolicy",
    "cc-safety",
    "cc-config",
    "cc-machine",
    "cc-display",
    "cc-web",
    "cc-mqtt",
)

BANNED = re.compile(r"\besp_idf_(?:hal|svc|sys)\b|\besp-idf-(?:hal|svc|sys)\b")


def strip_comments_and_strings(text: str) -> str:
    """Remove ``//`` and ``/* */`` comments and string literals.

    Not a full lexer, and it does not need to be: the goal is to stop a PROSE
    mention from counting, and both comment forms are handled. A ``//`` inside a
    string literal is a false negative here -- a line like ``let s = "// esp_idf_hal";``
    would lose the rest of the line. Nothing in these crates does that, and a
    false negative in a CI hygiene check is the safe direction.
    """
    out: list[str] = []
    i, n = 0, len(text)
    while i < n:
        two = text[i : i + 2]
        if two == "//":
            while i < n and text[i] != "\n":
                i += 1
        elif two == "/*":
            depth, i = 1, i + 2
            while i < n and depth:
                if text[i : i + 2] == "/*":
                    depth, i = depth + 1, i + 2
                elif text[i : i + 2] == "*/":
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            out.append(" ")  # keep the line count stable for error messages
        elif text[i] == '"':
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                elif text[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            out.append('""')
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def main(argv: list[str]) -> int:
    root = pathlib.Path(argv[1] if len(argv) > 1 else ".")
    offenders: list[str] = []

    for crate in PORTABLE:
        src = root / "crates" / crate / "src"
        if not src.is_dir():
            print(f"no crates/{crate}/src under {root}", file=sys.stderr)
            return 2
        for path in sorted(src.rglob("*.rs")):
            code = strip_comments_and_strings(path.read_text(encoding="utf-8", errors="replace"))
            for lineno, line in enumerate(code.splitlines(), start=1):
                if BANNED.search(line):
                    offenders.append(
                        f"{path.relative_to(root)}:{lineno}: {line.strip()[:100]}"
                    )

    if offenders:
        print(
            "[portable-purity] the portable crates are #![no_std] and host-testable;\n"
            "an esp_idf_* item in their CODE breaks every host test and every IDE\n"
            "check. Prose is fine -- comments and doc-comments are stripped before\n"
            "this search, because a portable crate must be able to explain why it\n"
            "does not use ESP-IDF.",
            file=sys.stderr,
        )
        for line in offenders:
            print(f"  {line}", file=sys.stderr)
        return 1

    print(
        f"[portable-purity] {', '.join(PORTABLE)}: no esp_idf_* in code "
        "(comments and doc-comments excluded, so the reasoning is preserved)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
