#!/usr/bin/env python3
"""Capture the ESP32 boot log over UART0, driving the auto-reset lines.

Why this exists: `espflash monitor` needs a TTY for its key handler, so it
cannot be used from a non-interactive shell, a CI job or an agent. The
ESP32-DevKitC V4 has no native USB — the cable is a UART bridge and BOOT/EN are
wired to DTR/RTS through two transistors — so resetting into a normal boot is
just a DTR/RTS pulse (which is what `espflash reset` does, but that needs the
port to itself).

Usage:
    python3 scripts/serial-log.py /dev/cu.usbserial-XXXX [seconds] [--no-reset]

Requires a python with `pyserial`; the ESP-IDF virtualenv that the
`esp-idf-sys` build creates has one
(`.embuild/espressif/python_env/idf*_env/bin/python`).
"""

import sys
import time

import serial

BAUD = 115200


def pulse_reset(port: serial.Serial) -> None:
    """Classic esptool auto-reset: EN (RTS) low, BOOT (DTR) low, then release."""
    port.dtr = False
    port.rts = True
    time.sleep(0.15)
    port.rts = False
    time.sleep(0.1)


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    no_reset = "--no-reset" in sys.argv
    if not args:
        print(__doc__, file=sys.stderr)
        return 2
    port_name = args[0]
    seconds = float(args[1]) if len(args) > 1 else 20.0

    port = serial.Serial(port_name, BAUD, timeout=0.2)
    if not no_reset:
        pulse_reset(port)

    deadline = time.time() + seconds
    captured = 0
    out = sys.stdout
    while time.time() < deadline:
        chunk = port.read(4096)
        if chunk:
            captured += len(chunk)
            out.write(chunk.decode("utf-8", "replace"))
            out.flush()
    port.close()
    print(f"\n[serial-log: {captured} bytes from {port_name} at {BAUD} baud]", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
