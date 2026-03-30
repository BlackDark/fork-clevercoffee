import os

import subprocess
import sys


"""
This script creates the new frontend and copies it into the data directory.
"""


def main():
    print("Running frontend build commands and copy to data directory ..")
    subprocess.check_call("cd ui && pnpm install && pnpm prepare-esp", shell=True)


if "buildfs" in sys.argv:
    main()

if os.environ.get("PROJECT_TASK") == "buildfs":
    main()
