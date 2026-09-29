#!/usr/bin/env python3
"""Enforces the dependency direction the architecture specifies.

The rules, from docs/rust-migration/architecture.md section 2:

- No workspace crate except `fw` may depend on `esp-hal` or any other chip-specific crate. Only
  `fw` and the `bsp-*` crates know which chip they are for, and only `fw` may select a chip
  feature.
- A crate may not depend on a crate that sits to its right in the layering.
- `domain`, `config`, `storage`, `http`, `onewire`, `ds18b20` and the driver crates must not
  depend on `app` or on any `bsp-*`.

Exits non-zero with a list of violations, so CI fails rather than warning.
"""

from __future__ import annotations

import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Crate name -> the crates it is allowed to depend on, within the workspace.
LAYERS: dict[str, set[str]] = {
    # Bottom: pure logic, no I/O, no hardware.
    "clevercoffee-domain": set(),
    "clevercoffee-board-profiles": set(),
    "clevercoffee-hal-traits": {"clevercoffee-domain"},
    "clevercoffee-onewire": {"clevercoffee-hal-traits"},
    "clevercoffee-ds18b20": {"clevercoffee-hal-traits", "clevercoffee-onewire"},
    "clevercoffee-storage": set(),
    "clevercoffee-config": set(),
    "clevercoffee-http": set(),
    # The display templates switch on `domain::State`, so they need the state enum as well as the
    # traits. Both are below the display in the layering.
    "clevercoffee-display": {"clevercoffee-hal-traits", "clevercoffee-domain"},
    "clevercoffee-drivers-pressure": {"clevercoffee-hal-traits"},
    "clevercoffee-drivers-scale": {"clevercoffee-hal-traits"},
    "clevercoffee-drivers-tsic": {"clevercoffee-hal-traits"},
    "clevercoffee-app": {
        "clevercoffee-domain",
        "clevercoffee-hal-traits",
        "clevercoffee-config",
        "clevercoffee-storage",
        "clevercoffee-http",
    },
    "clevercoffee-bsp-esp32": {
        "clevercoffee-domain",
        "clevercoffee-hal-traits",
        "clevercoffee-board-profiles",
        "clevercoffee-app",
    },
    "clevercoffee-bsp-esp32s3": {
        "clevercoffee-domain",
        "clevercoffee-hal-traits",
        "clevercoffee-board-profiles",
        "clevercoffee-app",
    },
    "clevercoffee-bsp-esp32c6": {
        "clevercoffee-domain",
        "clevercoffee-hal-traits",
        "clevercoffee-board-profiles",
        "clevercoffee-app",
    },
    "clevercoffee-fw": {
        "clevercoffee-app",
        # For the `SensorSource` associated type on the loop. The firmware is the top of the
        # layering, so this is the one crate that may reach sideways for a trait it must name.
        "clevercoffee-hal-traits",
        "clevercoffee-storage",
        "clevercoffee-bsp-esp32",
        "clevercoffee-bsp-esp32s3",
        "clevercoffee-bsp-esp32c6",
    },
}

# Crates that talk to a chip. Everything else must be host-testable.
# The chip-selection features.
CHIP_FEATURES = ("esp32", "esp32s3", "esp32c6")

# The crates allowed to name a chip. `fw` selects the board at build time; each `bsp-*` crate is
# tied to exactly one chip by being a separate crate.
CHIP_OWNERS = {
    "clevercoffee-fw",
    "clevercoffee-bsp-esp32",
    "clevercoffee-bsp-esp32s3",
    "clevercoffee-bsp-esp32c6",
}

# The `bsp-*` crates exist precisely to hold a chip dependency, so they are allowed these. `fw`
# is allowed them too. Nothing else is.
HARDWARE_ALLOWED_BY = {"clevercoffee-bsp-esp32", "clevercoffee-bsp-esp32s3", "clevercoffee-bsp-esp32c6", "clevercoffee-fw"}

HARDWARE_CRATES = {
    "esp-hal",
    "esp-rtos",
    "esp-radio",
    "esp-alloc",
    "esp-backtrace",
    "esp-bootloader-esp-idf",
    "esp-println",
    "esp-storage",
    "embassy-executor",
    "embassy-time",
    "embassy-net",
    "embassy-sync",
    "embassy-usb-driver",
}


VIOLATIONS: list[str] = []


def workspace_deps() -> dict[str, set[str]]:
    """Maps each crate to the workspace crates it depends on, from the manifests."""
    deps: dict[str, set[str]] = {}
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
        name = parsed["package"]["name"]
        found: set[str] = set()
        sections = [parsed.get("dependencies", {})]
        for target in parsed.get("target", {}).values():
            sections.append(target)
        for section in sections:
            for dep_name, spec in section.items():
                if dep_name in LAYERS:
                    found.add(dep_name)
                elif dep_name in HARDWARE_CRATES and name not in HARDWARE_ALLOWED_BY:
                    violations.append(
                        f"{name}: depends on hardware crate {dep_name}, "
                        "which only a bsp crate or fw may do"
                    )
        deps[name] = found
    return deps


def check_feature_sets() -> list[str]:
    """Checks that no feature can select more than one chip.

    Passing two board features does fail the build, but it fails inside `esp-metadata-generated`
    with a duplicate-macro error, which says nothing about the cause. Cargo compiles
    dependencies before the crate that would carry a clear `compile_error!`, so a static check is
    the only place that can give a useful message.
    """
    out: list[str] = []
    fw = tomllib.loads((ROOT / "crates/fw/Cargo.toml").read_text(encoding="utf-8"))
    for feature, members in fw.get("features", {}).items():
        if not feature.startswith("board-"):
            continue
        selected = sorted(
            {m.split("/")[-1] for m in members if m.split("/")[-1] in CHIP_FEATURES}
        )
        expected = [feature.removeprefix("board-")]
        if selected != expected:
            out.append(
                f"clevercoffee-fw: feature {feature} selects chips {selected}, expected {expected}"
            )
    return out


def main() -> int:
    deps = workspace_deps()
    violations: list[str] = list(VIOLATIONS)

    # Every manifest on disk must be in LAYERS, so a new crate cannot skip the layering check by
    # simply not being listed.
    for name in deps:
        if name not in LAYERS:
            violations.append(f"{name}: no manifest on disk, and it is not in LAYERS")
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        name = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]["name"]
        if name not in LAYERS:
            violations.append(f"{name}: missing from LAYERS, so its layering is unchecked")

    for name, allowed in LAYERS.items():
        if name not in deps:
            violations.append(f"{name}: declared in LAYERS but no manifest found")
            continue
        for dep in sorted(deps[name] - allowed):
            violations.append(f"{name}: depends on {dep}, which is not in its allowed set")

    # Exactly one board feature may be selected, and only fw selects one. The `bsp-*` crates
    # declare their own board feature so the selection is explicit rather than implicit, and
    # their chip is fixed by which crate it is, so they are allowed to name it.
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
        name = parsed["package"]["name"]
        boards = [f for f in parsed.get("features", {}) if f.startswith("board-")]
        if name in CHIP_OWNERS:
            # `fw` declares all three boards so cargo can select one with --features; the mutual
            # exclusion is enforced by check_feature_sets below, and by crates/fw/src/checks.rs
            # for the combinations that reach it.
            pass
        elif boards and name not in CHIP_OWNERS:
            violations.append(
                f"{name}: declares a board feature {boards}, but only fw and the bsp crates may"
            )
        for section in [parsed.get("features", {}), parsed.get("dependencies", {})]:
            for key, value in section.items():
                flat = str(value)
                if name in CHIP_OWNERS:
                    continue
                if key.startswith("board-") or any(chip in flat for chip in CHIP_FEATURES):
                    violations.append(f"{name}: names a chip in {key}, but only fw and bsp may")

    violations += check_feature_sets()

    if violations:
        print("dependency-direction violations:")
        for v in violations:
            print(f"  {v}")
        return 1

    print(f"dependency direction ok: {len(deps)} crates, {len(LAYERS)} layering rules")
    return 0


if __name__ == "__main__":
    sys.exit(main())
