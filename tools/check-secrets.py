#!/usr/bin/env python3
"""Fails if a tracked file contains something that looks like a real credential.

The rules come from CLAUDE.md and the migration skill: `.env` holds WIFI_SSID and WIFI_PASS and
must never reach git, an example, a log, or a commit message. This check is deliberately blunt,
because a false positive costs one line to fix and a missed secret costs a leaked credential.

What it looks for:

- a tracked `.env`, or any tracked file whose name is `.env` or `*.env` outside an example
- a `WIFI_PASS` or `WIFI_SSID` assignment with a value that is not a placeholder
- a `system.wifi.password` or `mqtt.password` JSON value that is not a placeholder
- a private key block

Placeholders are the strings that appear in documentation on purpose: `xxx`, `""`, `changeme`,
`your-`, `<`, `***`, anything starting with `placeholder` or `example`, and anything under four
characters, which cannot be a real SSID.
"""

from __future__ import annotations

import re
import subprocess
import sys

# `placeholder` and `example` are allowed with a suffix, so a test fixture can be self-describing
# ("placeholder-value", "example-network") without becoming a false positive. The word still has
# to be there: a value that merely looks unusual is exactly what this check is for.
PLACEHOLDER = re.compile(
    r"^(xxx+|placeholder[-\w]*|example[-\w]*|changeme|change-me|your[-_ ].*|<.*>|\*+|-+|0+|none|null|\"\"|''|\s*)$",
    re.IGNORECASE,
)

# Values the C++ firmware ships as compiled-in defaults in
# include/clevercoffee/defaults.h. They are published in the repository already and are not
# per-device secrets, but they are worth naming here rather than pattern-matching around: a
# real device whose ota_password is still "otapass" is a finding for the migration guide, and
# the C++ code itself ships AUTH_USERNAME/AUTH_PASSWORD "admin"/"admin".
KNOWN_DEFAULTS = {
    "otapass",   # defaults.h: OTAPASS
    "admin",     # defaults.h: AUTH_USERNAME, AUTH_PASSWORD
    "silvia",    # defaults.h: MQTT_USERNAME, MQTT_PASSWORD, HOSTNAME
    "CleverCoffee",  # defaults.h: WM_PASS
}

# The value stops at a backslash as well as at whitespace, because a dotenv assignment inside a
# Rust string literal is written "WIFI_PASS=placeholder\n" and the two characters `\n` are not
# whitespace to a regex: without this the "value" swallows the rest of the source line and every
# fixture becomes a false positive.
#
# The value is at least four characters, matching the rule the docstring above already claimed. A
# shorter value cannot be a real SSID, and a one-character "value" here is a slice of a source line.
ASSIGNMENT = re.compile(
    r"\b(WIFI_SSID|WIFI_PASS)\b\s*[=:]\s*[\"']?([^\s\"'#\\]{4,})", re.IGNORECASE
)
JSON_SECRET = re.compile(
    r"\"(?:password|ota_password)\"\s*:\s*\"([^\"]*)\"", re.IGNORECASE
)
PRIVATE_KEY = re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")

BINARY_SUFFIXES = (".png", ".jpg", ".webp", ".gif", ".ico", ".zip", ".gz", ".bin", ".woff", ".woff2", ".ttf", ".pdf")

# JSON secret fields are only meaningful in a JSON document. Scanning every text file matched an
# HTML input type attribute.
JSON_SUFFIXES = (".json",)


def tracked_files() -> list[str]:
    """Every file the commit is about to contain.

    `git ls-files` alone lists the index, so a file that is new and not yet staged is invisible
    to the scan that is meant to catch a secret in new code. This returns the union of the index
    and the untracked-but-not-ignored files, which is exactly the set the next commit adds.
    """
    tracked = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, check=True
    ).stdout.split("\0")
    untracked = subprocess.run(
        ["git", "ls-files", "-z", "--others", "--exclude-standard"],
        capture_output=True, text=True, check=True,
    ).stdout.split("\0")
    files = [f for f in (*tracked, *untracked) if f and not f.endswith(BINARY_SUFFIXES)]
    return sorted(set(files))


def main() -> int:
    problems: list[str] = []

    for path in tracked_files():
        try:
            text = open(path, encoding="utf-8", errors="strict").read()
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue

        base = path.rsplit("/", 1)[-1]
        if base == ".env" or (base.endswith(".env") and base != ".env.example"):
            problems.append(f"{path}: a dotenv file is tracked")

        if PRIVATE_KEY.search(text):
            problems.append(f"{path}: contains a private key block")

        is_json = path.endswith(JSON_SUFFIXES)
        for lineno, line in enumerate(text.splitlines(), 1):
            patterns = [(ASSIGNMENT, 2)]
            if is_json:
                patterns.append((JSON_SECRET, 1))
            for pattern, value_group in patterns:
                for m in pattern.finditer(line):
                    value = m.group(value_group)
                    if not value:
                        continue
                    if PLACEHOLDER.match(value) or value in KNOWN_DEFAULTS:
                        continue
                    problems.append(
                        f"{path}:{lineno}: {m.group(0)[:40]} looks like a real value"
                    )

    if problems:
        print("possible committed secrets:")
        for p in sorted(set(problems)):
            print(f"  {p}")
        print("\nIf a value is a real secret, rotate it. Removing it from the file is not enough.")
        return 1

    print(f"no committed secrets: {len(tracked_files())} tracked files scanned")
    return 0


if __name__ == "__main__":
    sys.exit(main())
