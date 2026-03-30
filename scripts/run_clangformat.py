Import("env")

import subprocess
import os
import sys

# Pin clang-format version via Docker image to ensure consistent formatting
# across macOS (local) and Linux (CI) environments.
CLANG_TOOLS_IMAGE = "xianpengshen/clang-tools:21"


def _docker_available():
    """Check if Docker is available and running."""
    try:
        subprocess.run(
            ["docker", "info"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
        )
        return True
    except (FileNotFoundError, subprocess.TimeoutExpired, subprocess.SubprocessError):
        return False


def _collect_source_files():
    """Collect all C/C++ source files from project directories."""
    folders = [env.get("PROJECT_INCLUDE_DIR"), env.get("PROJECT_SRC_DIR")]
    libfolder = os.path.join(env.get("PROJECT_DIR"), "lib")
    if os.path.isdir(libfolder):
        folders.append(libfolder)

    file_list = []
    for folder in folders:
        for root, dirs, files in os.walk(folder, topdown=True):
            dirs[:] = [d for d in dirs if not d.startswith(".")]
            files = [f for f in files if not f[0] == "."]
            for file in files:
                if file.endswith(
                    (".c", ".cpp", ".h", ".hpp", ".cc", ".cxx", ".hxx", ".hh")
                ):
                    file_list.append(os.path.join(root, file))

    return folders, file_list


def check_format_callback(*arg, **kwargs):
    return formatting_callback(arg, kwargs)


def apply_format_callback(*arg, **kwargs):
    print("Formatting Source Code")
    return formatting_callback(arg, kwargs, apply=True)


def formatting_callback(*arg, **kwargs):
    apply = kwargs.get("apply", False)
    folders, file_list = _collect_source_files()

    print(
        "Formatting" if apply else "Checking",
        "the following source dirs:",
        folders,
    )

    use_docker = _docker_available()
    if use_docker:
        print(f"Using Docker image: {CLANG_TOOLS_IMAGE}")
        _run_with_docker(file_list, apply)
    else:
        print("Docker not available, falling back to system clang-format")
        _run_with_system(file_list, apply)


def _run_with_docker(file_list, apply):
    """Run clang-format inside a Docker container with a pinned version."""
    project_dir = env.get("PROJECT_DIR")
    # Convert absolute host paths to container-relative paths (/src/...)
    container_files = []
    for f in file_list:
        rel = os.path.relpath(f, project_dir)
        container_files.append(f"/src/{rel}")

    dry_run = "" if apply else "--dry-run "
    files_arg = " ".join(f'"{f}"' for f in container_files)

    cmd = (
        f"docker run --rm -v {project_dir}:/src"
        f" {CLANG_TOOLS_IMAGE}"
        f" clang-format --Werror {dry_run}-i {files_arg}"
    )

    if env.Execute(cmd):
        env.Exit(1)


def _run_with_system(file_list, apply):
    dry_run = " --dry-run " if not apply else " "
    files_arg = " ".join(f'"{f}"' for f in file_list)

    cmd = f"mise exec -- clang-format --Werror{dry_run}-i {files_arg}"

    if env.Execute(cmd):
        env.Exit(1)


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
