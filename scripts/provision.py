#!/usr/bin/env python3
"""Provision Wi-Fi credentials into a device's NVS over USB serial.

Mechanism
---------
The firmware reads Wi-Fi credentials from NVS namespace ``config`` under keys
derived as ``"p" + fnv1a32(dotted.path)`` in lowercase hex (Arduino
``String(hash, HEX)``, no zero padding). This tool derives the same keys, builds
a real NVS partition image with Espressif's own ``nvs_partition_gen.py``, and
writes it with ``espflash write-bin``. No firmware cooperation is required, so it
works on a device that has never been provisioned and on one whose firmware
cannot reach the network.

Credentials come from ``.env`` (``WIFI_SSID``, ``WIFI_PASS``) and are read by the
caller, never taken on the command line — argv is visible to every process on the
machine via ``ps``. Nothing here prints a credential, and the generated image is
written to a temporary file that is deleted on exit.

Safety
------
NVS generation replaces the whole partition. This tool therefore refuses to write
if the device's existing NVS is not blank, unless ``--merge-anyway`` is passed,
because clobbering it would erase every stored setting on the machine.

Usage
-----
    just provision /dev/cu.usbserial-XXXX      # the intended entry point

    # direct, with credentials exported by the caller:
    set -a; . ./.env; set +a
    python3 scripts/provision.py --port /dev/cu.usbserial-XXXX
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

NVS_OFFSET = 0x9000
NVS_SIZE = 0x5000
NAMESPACE = "config"

# Dotted config paths, from include/clevercoffee/Config.h.
SSID_PATH = "system.wifi.ssid"
PASS_PATH = "system.wifi.password"


def fnv1a32(text: str) -> int:
    """FNV-1a 32-bit, byte-for-byte as Config.h implements it."""
    h = 2166136261
    for byte in text.encode("utf-8"):
        h ^= byte
        h = (h * 16777619) & 0xFFFFFFFF
    return h


def nvs_key(dotted_path: str) -> str:
    """Reproduce ParamDef::generateNvsKey(): "p" + Arduino String(hash, HEX).

    Arduino's String(uint32, HEX) is lowercase and unpadded, so a hash with
    leading zero nibbles yields a shorter key. Do not zero-pad this.
    """
    return "p" + format(fnv1a32(dotted_path), "x")


def find_nvs_generator() -> list[str] | None:
    """Return a command that runs Espressif's NVS partition generator.

    Prefers the standalone PyPI package (esp-idf-nvs-partition-gen), because it
    does not require an ESP-IDF checkout. Falls back to the copy inside ESP-IDF,
    which since v5.3 is only a thin wrapper around that same package.
    """
    candidates = [sys.executable, str(Path(".venv/bin/python")), "python3"]
    for python in candidates:
        if python != sys.executable and not Path(python).exists():
            continue
        probe = subprocess.run(
            [python, "-c", "import esp_idf_nvs_partition_gen"], capture_output=True
        )
        if probe.returncode == 0:
            return [python, "-m", "esp_idf_nvs_partition_gen"]

    roots = []
    if idf := os.environ.get("IDF_PATH"):
        roots.append(Path(idf))
    roots += [Path.home() / ".espressif" / "esp-idf", Path(".embuild/espressif/esp-idf")]
    for root in roots:
        if not root.exists():
            continue
        hits = sorted(root.glob("**/nvs_partition_generator/nvs_partition_gen.py"))
        if hits:
            return [sys.executable, str(hits[-1])]
    return None


def device_nvs_is_blank(port: str, espflash: str) -> bool | None:
    """Read the device's NVS. True if blank, False if populated, None if unreadable."""
    with tempfile.TemporaryDirectory() as tmp:
        dump = Path(tmp) / "nvs.bin"
        try:
            subprocess.run(
                [espflash, "read-flash", "--port", port, hex(NVS_OFFSET), hex(NVS_SIZE), str(dump)],
                check=True,
                capture_output=True,
                timeout=180,
            )
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
            print(f"!! could not read existing NVS: {exc}", file=sys.stderr)
            return None
        data = dump.read_bytes()
        return set(data) == {0xFF}


def build_image(ssid: str, password: str, generator: list[str], out: Path) -> None:
    """Build an NVS image holding just the two credential keys, as strings."""
    csv = out.with_suffix(".csv")
    # `data,string` matches Preferences::putString -> nvs_set_str, which is how the
    # firmware stores these two parameters.
    csv.write_text(
        "key,type,encoding,value\n"
        f"{NAMESPACE},namespace,,\n"
        f"{nvs_key(SSID_PATH)},data,string,{ssid}\n"
        f"{nvs_key(PASS_PATH)},data,string,{password}\n",
        encoding="utf-8",
    )
    try:
        subprocess.run(
            [*generator, "generate", str(csv), str(out), str(NVS_SIZE)],
            check=True,
            capture_output=True,
        )
    finally:
        # The CSV holds the plaintext password. Remove it whatever happened.
        csv.unlink(missing_ok=True)


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--port", required=True, help="serial port, e.g. /dev/cu.usbserial-204140")
    ap.add_argument("--baud", type=int, default=115200, help="unused by write-bin; kept for symmetry")
    ap.add_argument(
        "--merge-anyway",
        action="store_true",
        help="write even though the device already has NVS content (ERASES all stored settings)",
    )
    ap.add_argument("--dry-run", action="store_true", help="build the image and verify it, do not write")
    args = ap.parse_args()

    ssid = os.environ.get("WIFI_SSID")
    password = os.environ.get("WIFI_PASS")
    if not ssid or not password:
        print(
            "!! WIFI_SSID and WIFI_PASS must be in the environment.\n"
            "   Use `just provision <port>`, which loads them from .env.",
            file=sys.stderr,
        )
        return 1

    espflash = shutil.which("espflash") or str(Path.home() / ".cargo/bin/espflash")
    if not Path(espflash).exists():
        print("!! espflash not found. Run `just setup`.", file=sys.stderr)
        return 1

    generator = find_nvs_generator()
    if generator is None:
        print(
            "!! NVS partition generator not found. Run `just setup`, which installs\n"
            "   esp-idf-nvs-partition-gen into .venv, or set IDF_PATH.",
            file=sys.stderr,
        )
        return 1

    print(f"==> NVS keys: {nvs_key(SSID_PATH)} (ssid), {nvs_key(PASS_PATH)} (password)")
    print(f"==> namespace: {NAMESPACE}   generator: {' '.join(generator[-2:])}")

    if not args.dry_run:
        blank = device_nvs_is_blank(args.port, espflash)
        if blank is None:
            return 1
        if not blank and not args.merge_anyway:
            print(
                "!! REFUSING TO WRITE: the device already has NVS content.\n"
                "   Writing a generated image replaces the whole partition and would\n"
                "   erase every stored setting on this machine.\n"
                "   Inspect it first:\n"
                f"     espflash read-flash --port {args.port} 0x9000 0x5000 nvs.bin\n"
                "     python3 scripts/nvs_inspect.py nvs.bin --namespace config\n"
                "   Then re-run with --merge-anyway if that is really what you want.",
                file=sys.stderr,
            )
            return 2
        print("==> existing NVS is blank; safe to write")

    with tempfile.TemporaryDirectory() as tmp:
        image = Path(tmp) / "nvs-provision.bin"
        build_image(ssid, password, generator, image)
        print(f"==> built NVS image, {image.stat().st_size} bytes")

        # Verify the image round-trips through our own parser before writing it.
        verify = subprocess.run(
            [sys.executable, "scripts/nvs_inspect.py", str(image), "--namespace", NAMESPACE],
            capture_output=True,
            text=True,
        )
        for key in (nvs_key(SSID_PATH), nvs_key(PASS_PATH)):
            if key not in verify.stdout:
                print(f"!! generated image does not contain {key}", file=sys.stderr)
                print(verify.stdout, verify.stderr, file=sys.stderr)
                return 1
        print("==> image verified: both keys present as type str")

        if args.dry_run:
            print("==> dry run, nothing written")
            return 0

        subprocess.run(
            [espflash, "write-bin", "--port", args.port, hex(NVS_OFFSET), str(image)],
            check=True,
        )

    print("==> NVS written. Power-cycle the device to pick up the credentials.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
