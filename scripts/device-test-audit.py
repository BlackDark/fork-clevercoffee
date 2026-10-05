#!/usr/bin/env python3
"""Fail if a device-crate unit test exists that the on-target runner cannot run.

The bug this guards against is a process one, not a code one. `just lint-esp32`
compiles `cc-hal-esp32`'s `#[test]` functions with `--all-targets`, which
type-checks them. Nothing ran them: `cargo test` cannot build the crate for a
host target, and `just test` lists only the portable crates. So the suite was
green and two real device bugs shipped through it -- a provisioning password
window that lasted zero milliseconds, and console lines lost across
`esp_restart()` -- each with a test that had never been executed.

Three ways a test can now end up neither run nor visible, and one check catches
all of them:

  1. a new `#[test]` in a device crate that nobody added to
     `cc_hal_esp32::device_tests::CASES`, so the runner silently skips it;
  2. a registry entry for a test that has been deleted or renamed, so the runner
     fails to link or panics on a name that no longer exists;
  3. a test marked `#[ignore]` "to make the runner pass", which is the move this
     whole exercise exists to prevent.

The check is three greps and a set comparison: it runs in well under a second,
needs no device and no cargo, and is wired into `just lint-esp32` so it cannot be
skipped by running the lint on its own.

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

# The crates that cannot be tested by `cargo test` on a host target. Each one
# listed here is a crate whose `#[test]` functions are ONLY executable through
# the on-target runner.
DEVICE_CRATES = ("cc-hal-esp32", "cc-firmware", "cc-device-tests")

# A test that carries `#[cfg_attr(test, test)]` instead of a bare `#[test]` is
# reachable from `device_tests::CASES`. A bare `#[test]` is NOT, and is a bug.
CALLABLE_TEST = re.compile(r"^\s*#\[cfg_attr\(test, test\)\]\s*$")
BARE_TEST = re.compile(r"^\s*#\[test\]\s*$")
IGNORED = re.compile(r"^\s*#\[ignore")
TEST_FN = re.compile(r"^\s*pub fn ([A-Za-z0-9_]+)\(")
# `DOTALL`, not `MULTILINE`: rustfmt wraps a long `use` across several lines
# once the list stops fitting in 100 columns, and the module list then contains
# newlines of its own. Matching only the single-line form made this audit
# report `imports ``, which has no source file` — a false failure caused purely
# by formatting, which is the worst kind: it teaches people to ignore the gate
# that exists to stop tests going unrun.
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


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    hal = root / "crates" / "cc-hal-esp32"
    registry_path = hal / "src" / "device_tests.rs"

    failures: list[str] = []

    for crate_name in DEVICE_CRATES:
        crate = root / "crates" / crate_name
        if not crate.is_dir():
            failures.append(f"crates/{crate_name} does not exist; update DEVICE_CRATES")
            continue
        _, bare, ignored = find_tests(crate)
        for name in bare:
            failures.append(
                f"crates/{crate_name}/{name} is a bare `#[test]`: `#[test]` is "
                "deleted by the compiler unless the crate is built with --test, "
                "so the on-target runner cannot call it. Use "
                "`#[cfg_attr(test, test)]` and register it in "
                "cc_hal_esp32::device_tests::CASES"
            )
        for where in ignored:
            failures.append(
                f"crates/{crate_name}/{where} is #[ignore]d. An ignored device "
                "test is a test that does not run, which is the failure this "
                "harness exists to end: fix the test, or move it out of the "
                "device crates with a comment saying why"
            )

    callable_names, _, _ = find_tests(hal)
    if not registry_path.is_file():
        failures.append(
            "crates/cc-hal-esp32/src/device_tests.rs is missing, so nothing "
            "makes the device tests callable. `just test-esp32` needs it."
        )
        print("\n".join(f"[device-test-audit] {f}" for f in failures), file=sys.stderr)
        return 1

    text = registry_path.read_text()
    registered = {(m.group(1), m.group(2)) for m in REGISTRY_ENTRY.finditer(text)}
    declared_modules = {
        name.strip()
        for group in REGISTRY_MODULES.findall(text)
        # Split on commas *and* newlines: a rustfmt-wrapped `use` puts one
        # module per line, with a trailing comma and leading indentation.
        for name in re.split(r"[,\n]", group)
        if name.strip()
    }
    present_modules = {p.stem for p in walk_sources(hal)}

    for module in sorted(declared_modules - present_modules):
        failures.append(
            f"crates/cc-hal-esp32/src/device_tests.rs imports `{module}`, which "
            "has no source file. A stale import usually means a module was "
            "renamed and the registry was not updated"
        )
    for module in sorted(present_modules - declared_modules):
        # Not every file is a module (lib.rs and device_tests.rs are not
        # imported that way); only complain about one that holds tests.
        if any(name.startswith(f"{module}::") for name in callable_names):
            failures.append(
                f"crates/cc-hal-esp32/src/{module}.rs holds tests but "
                "device_tests.rs does not import it"
            )

    on_disk = {tuple(name.split("::", 1)) for name in callable_names}
    for module, function in sorted(on_disk - registered):
        failures.append(
            f"{module}::{function} is #[cfg_attr(test, test)] but is not in "
            "cc_hal_esp32::device_tests::CASES, so `just test-esp32` will never "
            "run it"
        )
    for module, function in sorted(registered - on_disk):
        failures.append(
            f"cc_hal_esp32::device_tests::CASES registers {module}::{function}, "
            "which is not a test in this crate any more"
        )
    if registered and len(registered) != len(
        [m for m in REGISTRY_ENTRY.finditer(text)]
    ):
        failures.append("cc_hal_esp32::device_tests::CASES has a duplicate entry")

    if failures:
        print("\n".join(f"[device-test-audit] {f}" for f in failures), file=sys.stderr)
        print(
            f"[device-test-audit] {len(failures)} problem(s). Every device-crate "
            "test must be reachable from `just test-esp32`.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[device-test-audit] {len(callable_names)} device unit tests, all "
        f"registered with cc_hal_esp32::device_tests::CASES "
        f"({len(DEVICE_CRATES)} device crates checked, 0 ignored)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
