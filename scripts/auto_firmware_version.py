###
# CleverCoffee Auto-version Script
###

import os

# noinspection PyUnresolvedReferences
Import("env")

version = os.environ.get("CLEVERCOFFEE_VERSION", "dev").strip() or "dev"
print(f"CleverCoffee VERSION: {version}")

env.Append(BUILD_FLAGS=[f'-D VERSION=\\"{version}\\"'])
