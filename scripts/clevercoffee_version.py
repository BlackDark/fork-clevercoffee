"""
CleverCoffee version resolution for firmware, UI, and CI.

Release tags: CLEVERCOFFEE_VERSION or GITHUB_REF tag name.
Otherwise: dev-<branch-slug-6>-<commit-6> (e.g. dev-renova-a1b2c3).
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

DEV_BRANCH_SLUG_LEN = 6
DEV_SHA_LEN = 6


def _repo_root() -> Path:
    project_dir = os.environ.get("PLATFORMIO_PROJECT_DIR")
    if project_dir:
        return Path(project_dir)
    try:
        return Path(__file__).resolve().parent.parent
    except NameError:
        return Path.cwd()


def sanitize_branch_name(branch_name: str) -> str:
    branch_name = branch_name.lower()
    branch_name = re.sub(r"[^a-z0-9]+", "-", branch_name)
    return branch_name.strip("-")


def branch_slug_max6() -> str:
    ci_slug = os.environ.get("GITHUB_REF_POINT_SLUG", "").strip()
    if ci_slug:
        return ci_slug[:DEV_BRANCH_SLUG_LEN] or "dev"

    try:
        branch = subprocess.check_output(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            cwd=_repo_root(),
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
        if branch == "HEAD":
            branch = os.environ.get("GITHUB_REF_NAME", "detached")
    except (OSError, subprocess.CalledProcessError):
        branch = "detached"

    slug = sanitize_branch_name(branch)
    return slug[:DEV_BRANCH_SLUG_LEN] or "dev"


def sha_short6() -> str:
    short = os.environ.get("GITHUB_SHA_SHORT", "").strip()
    if short:
        return short[:DEV_SHA_LEN]

    try:
        return subprocess.check_output(
            ["git", "rev-parse", f"--short={DEV_SHA_LEN}", "HEAD"],
            cwd=_repo_root(),
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "0" * DEV_SHA_LEN


def resolve_dev_version() -> str:
    return f"dev-{branch_slug_max6()}-{sha_short6()}"


def resolve_clevercoffee_version() -> str:
    explicit = os.environ.get("CLEVERCOFFEE_VERSION", "").strip()
    if explicit:
        return explicit

    ref = os.environ.get("GITHUB_REF", "")
    if ref.startswith("refs/tags/"):
        return ref.removeprefix("refs/tags/")

    return resolve_dev_version()


def main() -> int:
    print(resolve_clevercoffee_version())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
