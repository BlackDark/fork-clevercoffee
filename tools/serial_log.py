#!/usr/bin/env python3
"""Reset the board and capture the serial log for N seconds.

    /tmp/venv/bin/python tools/serial_log.py 25 [--reset]
"""
import sys
import time

import serial

PORT = "/dev/cu.usbserial-224140"


def main() -> None:
    seconds = float(sys.argv[1]) if len(sys.argv) > 1 else 20.0
    do_reset = "--reset" in sys.argv

    s = serial.Serial(PORT, 115200, timeout=0.2)
    # Pulse EN low then high: the auto-reset circuit is unreliable, so do it by
    # hand rather than trusting DTR/RTS (which the CH340 maps to its own pins).
    if do_reset:
        s.dtr = False
        s.rts = True
        time.sleep(0.1)
        s.rts = False
        time.sleep(0.6)

    end = time.time() + seconds
    while time.time() < end:
        line = s.readline()
        if line:
            sys.stdout.write(line.decode("utf8", "replace"))
            sys.stdout.flush()


main()
