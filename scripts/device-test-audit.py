#!/usr/bin/env python3
"""Fail if a device-crate unit test exists that the on-target runner cannot run.

Device crates cannot be tested by `cargo test` on the host; their tests run only
through `just test-esp32`, which calls each crate's `device_tests::CASES`. A test
can silently fall out of that run in three ways, all caught here:

  1. a `#[test]` nobody registered in its crate's `CASES`;
  2. a registry entry for a test that was deleted or renamed;
  3. a test marked `#[ignore]`.

Runs in under a second, needs no device and no cargo; `just lint-esp32` runs it.

Usage:
    python3 scripts/device-test-audit.py [workspace-root]

Exit codes:
    0  every device-crate test is registered, every registration resolves
    1  a mismatch, an unregistered test, or an ignored test
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# Crates whose tests run only on the device.
DEVICE_CRATES = ("cc-hal-esp32", "cc-net-esp32", "cc-firmware", "cc-device-tests")
# The device crates that carry a `src/device_tests.rs` registry.
REGISTRY_CRATES = ("cc-hal-esp32", "cc-net-esp32")

# A test that carries `#[cfg_attr(test, test)]` instead of a bare `#[test]` is
# reachable from `device_tests::CASES`. A bare `#[test]` is NOT, and is a bug.
CALLABLE_TEST = re.compile(r"^\s*#\[cfg_attr\(test, test\)\]\s*$")
BARE_TEST = re.compile(r"^\s*#\[test\]\s*$")
IGNORED = re.compile(r"^\s*#\[ignore")
TEST_FN = re.compile(r"^\s*pub fn ([A-Za-z0-9_]+)\(")
# `DOTALL`: rustfmt wraps a long `use` list across lines.
REGISTRY_MODULES = re.compile(r"use\s+crate::\{(.*?)\}\s*;", re.DOTALL)
REGISTRY_ENTRY = re.compile(
    r'name:\s*"([A-Za-z0-9_]+)::([A-Za-z0-9_]+)",\s*\n\s*run:\s*([A-Za-z0-9_]+)::tests::'
    r'([A-Za-z0-9_]+),'
)


def walk_sources(crate: Path) -> list[Path]:
    return sorted(p for p in (crate / "src").rglob("*.rs"))


def find_tests(crate: Path) -> tuple[list[str], list[str], list[str]]:
    """Return (callable, bare, ignored) test names for one crate."""
    callable_names: list[str] = []
    bare_names: list[str] = []
    ignored: list[str] = []
    for path in walk_sources(crate):
        lines = path.read_text().splitlines()
        for number, line in enumerate(lines):
            if CALLABLE_TEST.match(line) or BARE_TEST.match(line):
                # The function name is the next `fn` at a shallower indent.
                name = None
                for follower in lines[number + 1 : number + 4]:
                    match = TEST_FN.match(follower) or re.match(
                        r"^\s*(?:pub )?fn ([A-Za-z0-9_]+)\(", follower
                    )
                    if match:
                        name = match.group(1)
                        break
                if name is None:
                    continue
                if CALLABLE_TEST.match(line):
                    callable_names.append(f"{path.stem}::{name}")
                else:
                    bare_names.append(f"{path.stem}::{name}")
            elif IGNORED.match(line):
                ignored.append(f"{path.stem}:{number + 1}")
    return callable_names, bare_names, ignored


def audit_registry(root: Path, crate_name: str) -> tuple[int, list[str]]:
    """Check one crate's `device_tests::CASES` against its tests on disk."""
    crate = root / "crates" / crate_name
    registry = f"crates/{crate_name}/src/device_tests.rs"
    cases = f"{crate_name.replace('-', '_')}::device_tests::CASES"
    failures: list[str] = []

    callable_names, _, _ = find_tests(crate)
    registry_path = root / registry
    if not registry_path.is_file():
        return len(callable_names), [f"{registry} is missing; `just test-esp32` needs it"]

    text = registry_path.read_text()
    entries = [(m.group(1), m.group(2)) for m in REGISTRY_ENTRY.finditer(text)]
    registered = set(entries)
    declared_modules = {
        name.strip()
        for group in REGISTRY_MODULES.findall(text)
        for name in re.split(r"[,\n]", group)
        if name.strip()
    }
    present_modules = {p.stem for p in walk_sources(crate)}

    for module in sorted(declared_modules - present_modules):
        failures.append(f"{registry} imports `{module}`, which has no source file")
    for module in sorted(present_modules - declared_modules):
        # Only a module that holds tests must be imported.
        if any(name.startswith(f"{module}::") for name in callable_names):
            failures.append(
                f"crates/{crate_name}/src/{module}.rs holds tests but "
                "device_tests.rs does not import it"
            )

    on_disk = {tuple(name.split("::", 1)) for name in callable_names}
    for module, function in sorted(on_disk - registered):
        failures.append(
            f"{crate_name}: {module}::{function} is #[cfg_attr(test, test)] but is "
            f"not in {cases}, so `just test-esp32` will never run it"
        )
    for module, function in sorted(registered - on_disk):
        failures.append(
            f"{cases} registers {module}::{function}, which is not a test in "
            "this crate any more"
        )
    if len(registered) != len(entries):
        failures.append(f"{cases} has a duplicate entry")
    return len(callable_names), failures


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    failures: list[str] = []

    for crate_name in DEVICE_CRATES:
        crate = root / "crates" / crate_name
        if not crate.is_dir():
            failures.append(f"crates/{crate_name} does not exist; update DEVICE_CRATES")
            continue
        _, bare, ignored = find_tests(crate)
        for name in bare:
            failures.append(
                f"crates/{crate_name}/{name} is a bare `#[test]`, which the "
                "on-target runner cannot call. Use `#[cfg_attr(test, test)]` and "
                "register it in the crate's device_tests::CASES"
            )
        for where in ignored:
            failures.append(
                f"crates/{crate_name}/{where} is #[ignore]d: fix the test, or move "
                "it out of the device crates with a comment saying why"
            )

    total = 0
    for crate_name in REGISTRY_CRATES:
        count, crate_failures = audit_registry(root, crate_name)
        total += count
        failures += crate_failures

    if failures:
        print("\n".join(f"[device-test-audit] {f}" for f in failures), file=sys.stderr)
        print(
            f"[device-test-audit] {len(failures)} problem(s). Every device-crate "
            "test must be reachable from `just test-esp32`.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[device-test-audit] {total} device unit tests, all registered "
        f"({len(DEVICE_CRATES)} device crates checked, 0 ignored)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
