"""
Shared CleverCoffee version resolution for firmware and UI builds.

Priority:
  1. CLEVERCOFFEE_VERSION environment variable (CI release tags, manual override)
  2. dev-YYYY-MM-DD (UTC) for local and non-tag CI builds
"""

from __future__ import annotations

import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

def _repo_root() -> Path:
    project_dir = os.environ.get("PLATFORMIO_PROJECT_DIR")
    if project_dir:
        return Path(project_dir)
    try:
        return Path(__file__).resolve().parent.parent
    except NameError:
        return Path.cwd()


REPO_ROOT = _repo_root()
FRONTEND_PACKAGE_JSON = REPO_ROOT / "ui" / "packages" / "frontend" / "package.json"


def sanitize_branch_name(branch_name: str) -> str:
    branch_name = branch_name.lower()
    branch_name = re.sub(r"[^a-z0-9]+", "-", branch_name)
    return branch_name.strip("-")


def resolve_clevercoffee_version() -> str:
    explicit = os.environ.get("CLEVERCOFFEE_VERSION", "").strip()
    if explicit:
        return explicit
    return f"dev-{datetime.now(timezone.utc).strftime('%Y-%m-%d')}"


def resolve_firmware_revision() -> str:
    """Detailed revision for build logs: <version>+<branch>.<commit>."""
    version = resolve_clevercoffee_version()
    try:
        from dulwich.porcelain import active_branch
        from dulwich.repo import Repo

        repo = Repo(str(REPO_ROOT))
        commit_hash = repo.head().decode("utf-8")[:7]
        try:
            branch_name = active_branch(repo).decode("utf-8")
        except (IndexError, KeyError):
            branch_name = os.environ.get("GITHUB_REF_NAME", "detached")
        sanitized_branch = sanitize_branch_name(branch_name)
        return f"{version}+{sanitized_branch}.{commit_hash}"
    except Exception:
        return version


def resolve_brewui_version() -> str:
    if FRONTEND_PACKAGE_JSON.is_file():
        data = json.loads(FRONTEND_PACKAGE_JSON.read_text(encoding="utf-8"))
        version = data.get("version")
        if isinstance(version, str) and version.strip():
            return version.strip()
    return "dev"


def version_export_env() -> dict[str, str]:
    env = os.environ.copy()
    env["CLEVERCOFFEE_VERSION"] = resolve_clevercoffee_version()
    env["VITE_FIRMWARE_VERSION"] = env["CLEVERCOFFEE_VERSION"]
    env["VITE_APP_VERSION"] = resolve_brewui_version()
    return env


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--revision":
        print(resolve_firmware_revision())
        return 0
    print(resolve_clevercoffee_version())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
