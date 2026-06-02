import os
import subprocess
import sys

# noinspection PyUnresolvedReferences
Import("env")

"""
Build the frontend and copy it into the data directory for LittleFS.
Injects firmware/UI version env vars so the About page matches the firmware build.
"""


def main():
    scripts_dir = os.path.join(env["PROJECT_DIR"], "scripts")
    sys.path.insert(0, scripts_dir)
    os.environ["PLATFORMIO_PROJECT_DIR"] = env["PROJECT_DIR"]

    from clevercoffee_version import version_export_env

    build_env = version_export_env()
    print(
        "Building frontend with "
        f"CLEVERCOFFEE_VERSION={build_env['CLEVERCOFFEE_VERSION']} "
        f"VITE_APP_VERSION={build_env['VITE_APP_VERSION']}"
    )
    subprocess.check_call(
        "cd ui && pnpm install && pnpm prepare-esp",
        shell=True,
        env=build_env,
        cwd=env["PROJECT_DIR"],
    )


if "buildfs" in sys.argv or os.environ.get("PROJECT_TASK") == "buildfs":
    main()
