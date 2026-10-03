"""clang-format driver for the C++ parity oracle.

Two entry points, ONE file list, ONE clang-format version:

  * Inside PlatformIO (`pio run -t check-format` / `-t format`, which is what
    `just fmt-cpp` calls). SCons injects `env`; the Docker-first /
    mise-fallback resolution below applies.
  * Standalone (`python3 scripts/run_clangformat.py [--apply]`), which is what
    CI runs. No SCons, no PlatformIO, no Docker: it uses the `clang-format`
    already on PATH, which CI pins with `pip install clang-format==<pin>`.

Both paths collect the same files and pass the same flags, so a clean CI run
means a clean local run.

# ONE version, declared once, asserted here.
#
# `[vars] clang_format_version` in `.mise.toml` is the only place it is written.
# `format.yml` reads it and `pip install`s that version; `just fmt-cpp` and the
# pre-commit hook both come through this file. And this file REFUSES to format
# anything unless the binary on PATH reports exactly that version -- which is
# the part that matters, because `--apply`/`-i` rewrites the C++ parity oracle.
#
# Before the assertion there were four pins that agreed only by luck: `.mise.toml`
# said 23.1.1, format.yml said 22.1.2, this file preferred a `clang-tools:22`
# Docker image, and the pre-commit hook used whatever was on the developer's PATH.
# They agreed because 22.1.x and 23.1.x produce byte-identical output here --
# measured over all 143 files, a 0-line diff across 28,632 formatted lines. That
# is luck, not design: a developer with clang-format 18 on PATH had a hook that
# ran `-i`, and 21.1.8 is measured to change two files of the oracle.
#
# Docker is gone. It was a way to pin a version on a machine that has none, and
# `pip install clang-format==<v>` is a statically linked ~2 MB wheel (measured
# ~1.8 s) -- so mise for local development and pip for CI, one script, one
# assertion.
"""

import subprocess
import os
import sys

# The ONE version. Read from .mise.toml [vars]; see the module docstring.
SOURCE_SUFFIXES = (".c", ".cpp", ".h", ".hpp", ".cc", ".cxx", ".hxx", ".hh")

# `Import` is a SCons builtin, injected into this file's globals when
# PlatformIO loads it as an `extra_scripts` entry. It does not exist when CI
# runs this file directly, which is how the two entry points are told apart.
try:
    Import("env")  # noqa: F821  (injected by SCons)
    IN_PLATFORMIO = True
except NameError:
    IN_PLATFORMIO = False


def _mise_var(name):
    """Read a `name = "value"` entry from `.mise.toml`'s `[vars]` table.

    Whitespace-tolerant: `[vars]` entries are indented, and a `^`-anchored
    pattern silently matched nothing once already in this repository -- it handed
    espup an empty version and cost a CI run.
    """
    manifest = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".mise.toml"
    )
    if not os.path.isfile(manifest):
        return ""
    with open(manifest, encoding="utf-8") as handle:
        for line in handle:
            stripped = line.strip()
            if not stripped.startswith(name + " "):
                continue
            _, _, value = stripped.partition("=")
            return value.strip().strip('"')
    return ""


def _expected_version():
    """The one declared version, or None if it cannot be read."""
    return _mise_var("clang_format_version") or None


def _installed_version():
    """The version of the `clang-format` on PATH, or None if there is none."""
    try:
        out = subprocess.run(
            ["clang-format", "--version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    # "clang-format version 23.1.2 (...)" and conda-forge's
    # "Ubuntu clang-format version 22.1.2 (1ubuntu1)" both contain the token we
    # want as a bare dotted version; take the first token starting with a digit.
    for token in out.stdout.decode().strip().split():
        if token and token[0].isdigit():
            return token
    return None


def _require_pinned_version():
    """Die unless the clang-format on PATH is the declared one.

    This assertion is what makes a single pin mean anything. Every caller of this
    script can REWRITE the C++ parity oracle (`-i`), and the tree is only correct
    under the pin -- 21.1.8 is measured to change two of its files. An unpinned
    `-i` on a developer machine with an older clang-format would do exactly that,
    silently, in a commit about something else.
    """
    expected = _expected_version()
    if not expected:
        print(
            "run_clangformat: cannot read clang_format_version from "
            ".mise.toml [vars]",
            file=sys.stderr,
        )
        return False
    actual = _installed_version()
    if actual is None:
        print(
            "run_clangformat: no clang-format on PATH.\n"
            "  mise install                    # uses the pinned [tools] version\n"
            f"  pip install clang-format=={expected}",
            file=sys.stderr,
        )
        return False
    if actual != expected:
        print(
            f"run_clangformat: clang-format {actual} on PATH, but this "
            f"repository pins {expected}.\n"
            "  The C++ tree under src/ include/ lib/ is the firmware's PARITY "
            "ORACLE\n"
            "  and is only correct under the pin -- 21.1.8 changes two of its "
            "files.\n"
            "  Not formatting anything. Fix with:\n"
            "    mise install                    # or: pip install "
            f"clang-format=={expected}",
            file=sys.stderr,
        )
        return False
    return True


def _collect_source_files():
    """Collect every C/C++ source file the formatter is responsible for."""
    if IN_PLATFORMIO:
        folders = [env.get("PROJECT_INCLUDE_DIR"), env.get("PROJECT_SRC_DIR")]
        libfolder = os.path.join(env.get("PROJECT_DIR"), "lib")
    else:
        project_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        folders = [os.path.join(project_dir, "include"), os.path.join(project_dir, "src")]
        libfolder = os.path.join(project_dir, "lib")

    if os.path.isdir(libfolder):
        folders.append(libfolder)

    file_list = []
    for folder in folders:
        for root, dirs, files in os.walk(folder, topdown=True):
            dirs[:] = [d for d in dirs if not d.startswith(".")]
            files = [f for f in files if f[0] != "."]
            for file in files:
                if file.endswith(SOURCE_SUFFIXES):
                    file_list.append(os.path.join(root, file))

    return folders, sorted(file_list)


def _run_with_system(file_list, apply):
    """Run the pinned `clang-format` on PATH over the file list.

    No `mise exec --` wrapper: that call would silently run WHATEVER mise
    resolves, which is how the pin stopped meaning anything. The version is
    asserted instead, before this point.
    """
    if not _require_pinned_version():
        env.Exit(1)
    dry_run = " --dry-run " if not apply else " "
    files_arg = " ".join(f'"{f}"' for f in file_list)

    cmd = f"clang-format --Werror{dry_run}-i {files_arg}"

    if env.Execute(cmd):
        env.Exit(1)


def check_format_callback(*args, **kwargs):
    """PlatformIO `pio run -t check-format`. Dry run: report, change nothing."""
    _formatting_callback(apply=False)


def apply_format_callback(*args, **kwargs):
    """PlatformIO `pio run -t format`. Rewrite the tree in place."""
    _formatting_callback(apply=True)


def _formatting_callback(apply):
    folders, file_list = _collect_source_files()
    print("clang-format:", _clang_format_version())
    print("Formatting" if apply else "Checking", "the following source dirs:", folders)
    print(f"{len(file_list)} files")

    dry_run = " --dry-run " if not apply else " "
    files_arg = " ".join(f'"{f}"' for f in file_list)
    if env.Execute(f"clang-format --Werror{dry_run}-i {files_arg}"):
        env.Exit(1)


def _clang_format_version():
    """Print the version actually being used, so a CI log records the pin."""
    try:
        out = subprocess.run(
            ["clang-format", "--version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=True,
        )
        return out.stdout.decode().strip()
    except (OSError, subprocess.CalledProcessError):
        return "NOT FOUND on PATH"


def _main(argv):
    """Standalone entry point: use the clang-format already on PATH.

    This is the CI path. It deliberately does NOT shell out to `mise` or to
    Docker -- CI installs a pinned `clang-format` wheel and puts it on PATH,
    which is why this job needs no PlatformIO, no SCons and no espressif32.
    """
    apply = "--apply" in argv[1:]
    if not _require_pinned_version():
        return 1
    folders, file_list = _collect_source_files()

    print("clang-format:", _clang_format_version())
    print("Formatting" if apply else "Checking", "the following source dirs:", folders)
    print(f"{len(file_list)} files")

    # --dry-run makes this a check; -i keeps it consistent with the
    # PlatformIO path above, which passes both. Same flags, same order.
    cmd = ["clang-format", "--Werror"]
    if not apply:
        cmd.append("--dry-run")
    cmd += ["-i", *file_list]

    return subprocess.call(cmd)


if IN_PLATFORMIO:
    env.AddCustomTarget(
        "check-format",
        None,
        check_format_callback,
        title="Check clang-format",
        description="Check Source Code Formatting",
    )

    env.AddCustomTarget(
        "format",
        None,
        apply_format_callback,
        title="Apply clang-format",
        description="Run Source Code Formatting",
    )

if __name__ == "__main__":
    if IN_PLATFORMIO:  # pragma: no cover
        raise SystemExit("use `pio run -t check-format` / `-t format`, not direct python")
    raise SystemExit(_main(sys.argv))
