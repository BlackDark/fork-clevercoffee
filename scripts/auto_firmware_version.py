###
# CleverCoffee Auto-version Script
###

import os
import sys

# noinspection PyUnresolvedReferences
Import("env")

sys.path.insert(0, os.path.join(env.get("PROJECT_DIR"), "scripts"))
from clevercoffee_version import resolve_clevercoffee_version

version = resolve_clevercoffee_version()
print(f"CleverCoffee VERSION: {version}")

env.Append(BUILD_FLAGS=[f'-D VERSION=\\"{version}\\"'])
