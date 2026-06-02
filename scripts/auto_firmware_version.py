###
# CleverCoffee Auto-version Script
###

import importlib.metadata
import os
import sys

# noinspection PyUnresolvedReferences
Import("env")

os.environ["PLATFORMIO_PROJECT_DIR"] = env["PROJECT_DIR"]
sys.path.insert(0, os.path.join(env["PROJECT_DIR"], "scripts"))

required_pkgs = {"dulwich"}
installed_pkgs = {dist.metadata["Name"] for dist in importlib.metadata.distributions()}
missing_pkgs = required_pkgs - installed_pkgs

if missing_pkgs:
    env.Execute('$PYTHONEXE -m pip install "dulwich[pure]"')

from clevercoffee_version import resolve_clevercoffee_version, resolve_firmware_revision

version = resolve_clevercoffee_version()
revision = resolve_firmware_revision()

print(f"CleverCoffee VERSION: {version}")
print(f"Firmware revision: {revision}")

env.Append(BUILD_FLAGS=[f'-D VERSION=\\"{version}\\"'])
