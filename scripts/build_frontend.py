import os
import subprocess
import sys

# noinspection PyUnresolvedReferences
Import("env")

"""
Build the frontend and copy it into the data directory for LittleFS.
Passes CLEVERCOFFEE_VERSION into Vite so the UI matches the firmware build.
"""


def main():
    sys.path.insert(0, os.path.join(env["PROJECT_DIR"], "scripts"))
    from clevercoffee_version import resolve_clevercoffee_version

    version = resolve_clevercoffee_version()
    build_env = os.environ.copy()
    build_env["VITE_APP_VERSION"] = version
    print(f"Building frontend with CLEVERCOFFEE_VERSION={version}")
    subprocess.check_call(
        "cd ui && pnpm install && pnpm prepare-esp",
        shell=True,
        env=build_env,
        cwd=env["PROJECT_DIR"],
    )


if "buildfs" in sys.argv or os.environ.get("PROJECT_TASK") == "buildfs":
    main()
