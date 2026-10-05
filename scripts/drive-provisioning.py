#!/usr/bin/env python3
"""Drive the UART Wi-Fi provisioning protocol (08 §5.2) and report the outcome.

The credential is read from `.env` and written to the serial port. It is NEVER
printed, never written to the captured log file, and never compared in the
clear: every assertion here is "did this expected reply appear", so the
only thing this script can tell you about the credential is whether the machine
accepted it.

Usage:
    python3 drive-provisioning.py /dev/cu.usbserial-XXXX [seconds]
"""

import re
import sys
import time

import serial

BAUD = 115200
REPLY_PREFIX = "CCWIFI "


def env_credential() -> tuple[str, str]:
    """WIFI_SSID and WIFI_PASS from `.env`, or exit. Values are never printed."""
    values: dict[str, str] = {}
    with open(".env", encoding="utf-8") as handle:
        for line in handle:
            m = re.match(r"^\s*(WIFI_SSID|WIFI_PASS)\s*=\s*(.*?)\s*$", line)
            if m:
                values[m.group(1)] = m.group(2).strip("'\"")
    for key in ("WIFI_SSID", "WIFI_PASS"):
        if not values.get(key):
            print(f"{key} is not set in .env", file=sys.stderr)
            sys.exit(2)
    return values["WIFI_SSID"], values["WIFI_PASS"]


class Session:
    """A read/write session that keeps every secret out of what it prints."""

    def __init__(self, port: serial.Serial, secrets: list[str]) -> None:
        self.port = port
        self.secrets = [s for s in secrets if s]
        self.log: list[str] = []

    def redact(self, text: str) -> str:
        for secret in self.secrets:
            text = text.replace(secret, "<redacted>")
        return text

    def drain(self, seconds: float) -> str:
        deadline = time.time() + seconds
        out = []
        while time.time() < deadline:
            chunk = self.port.read(4096)
            if chunk:
                text = chunk.decode("utf-8", "replace")
                self.log.append(text)
                out.append(text)
        return "".join(out)

    def send(self, line: str) -> None:
        # The bytes go out raw; only what comes back is redacted for display.
        self.port.write((line + "\n").encode())
        self.port.flush()

    def expect(self, needle: str, seconds: float) -> bool:
        seen = ""
        deadline = time.time() + seconds
        while time.time() < deadline:
            seen += self.drain(0.3)
            if needle in seen:
                return True
        return False


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not args:
        print(__doc__, file=sys.stderr)
        return 2
    port_name = args[0]
    seconds = float(args[1]) if len(args) > 1 else 90.0

    ssid, password = env_credential()
    port = serial.Serial(port_name, BAUD, timeout=0.2)
    s = Session(port, [ssid, password])

    # No reset: the machine is already up and its provisioning task is already
    # reading UART0, which is the only reason this can work without a reboot.
    s.drain(1.0)

    results: list[tuple[str, bool]] = []

    s.send("wifi status")
    results.append(("`wifi status` answered", s.expect("CCWIFI ok status:", 10.0)))

    s.send("wifi apply")
    results.append((
        "`wifi apply` with nothing staged is refused",
        s.expect("CCWIFI err nothing to apply", 10.0),
    ))

    s.send(f"wifi set {ssid}")
    results.append((
        "`wifi set <ssid>` opens the password window",
        s.expect("CCWIFI ok ssid accepted", 10.0),
    ))

    s.send(password)
    results.append((
        "the password line was accepted",
        s.expect("CCWIFI ok password accepted", 10.0),
    ))

    s.send("wifi apply")
    results.append((
        "`wifi apply` staged the credential and reset the machine",
        s.expect("CCWIFI ok accepted", 15.0),
    ))

    # The reboot and the bring-up that follows it.
    s.drain(seconds)

    text = "".join(s.log)
    results.append((
        "the control task reported the credential as stored",
        "a wifi credential from the console was stored" in text,
    ))
    results.append(("the machine rebooted", "rst:0x" in text))
    results.append((
        "the reboot came back with the credential stored in NVS",
        bool(re.search(r"config: stored \(cc/cc\.config: schema v1", text)),
    ))
    results.append((
        "the radio associated with the configured network",
        "wifi: associated" in text,
    ))
    results.append((
        "the provisioning task did NOT restart (a credential is stored)",
        "the provisioning task is not started" in text,
    ))
    port.close()

    print("=== provisioning protocol ===")
    for name, ok in results:
        print(f"  {'PASS' if ok else 'FAIL'}  {name}")

    print()
    print("=== the machine's own log, credentials redacted ===")
    print(s.redact(text))
    return 0 if all(ok for _, ok in results) else 1


if __name__ == "__main__":
    sys.exit(main())
