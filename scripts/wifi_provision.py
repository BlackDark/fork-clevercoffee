#!/usr/bin/env python3
"""Put the Wi-Fi credential from `.env` on the device, over the UART console.

    python3 scripts/wifi_provision.py /dev/cu.usbserial-XXXX

**This writes NVS through the firmware's own code path.** It does not craft the
blob: it types `wifi set <ssid>`, `wifi pass <password>` and `wifi apply` at
`cc_hal_esp32::provisioning`, which parses them, hands a `Pending` to the control
task, and persists it with `ConfigStore` — the same path the web UI uses. Writing
the NVS blob directly would couple this script to the blob's schema version and
its JSON shape.

**The password goes on the same line as a command** — `wifi pass <value>` — rather
than on the line after `wifi set`. That form arms no window and computes no
deadline, so nothing here depends on the next line arriving inside 30 s or on the
console task's poll rate. The next-line form still works on the device for an
operator typing by hand; this script just does not use it.

**The credential is never printed.** Not by this script, not by the device's
replies: everything the device says is filtered through both values before it
reaches the terminal, because a serial session echoes what it receives. The
values are read from `.env` and never from `sys.argv`, so they cannot land in a
shell history or a process listing.
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time

REPLY = "CCWIFI"
SETTLE = 0.4
DRAIN = 3.0


def read_env(path: pathlib.Path) -> tuple[str, str]:
    """`WIFI_SSID` and `WIFI_PASS` out of `.env`, or a die explaining."""
    if not path.is_file():
        die(f"no {path} — it must define WIFI_SSID and WIFI_PASS")
    values: dict[str, str] = {}
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key.strip()] = value
    ssid, password = values.get("WIFI_SSID", ""), values.get("WIFI_PASS", "")
    if not ssid or not password:
        die(f"{path} must define a non-empty WIFI_SSID and WIFI_PASS")
    return ssid, password


def die(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port", help="the device's serial port")
    parser.add_argument(
        "--env",
        default=".env",
        help="the file holding WIFI_SSID and WIFI_PASS (default: .env)",
    )
    args = parser.parse_args()

    try:
        import serial  # noqa: PLC0415 — optional, and the error is actionable
    except ImportError:
        die("this needs pyserial: pip install pyserial")

    ssid, password = read_env(pathlib.Path(args.env))
    secrets = (ssid.encode(), password.encode())

    def scrub(raw: bytes) -> str:
        """The device's words, with both secrets removed."""
        for secret in secrets:
            raw = raw.replace(secret, b"<redacted>")
        return raw.decode("utf-8", "replace")

    def replies(text: str) -> str:
        """Only the console's own lines.

        The machine logs continuously, and some of those lines contain the text
        this script looks for — the interlock summary ends in a refusal counter, so
        a substring test for `err` matches the *periodic log*, not the answer to
        a command. Every verdict here is read from `CCWIFI` lines alone.
        """
        return " ".join(
            line.strip()
            for line in text.replace("\r", "\n").split("\n")
            if REPLY in line
        )

    port = serial.Serial(args.port, 115200, timeout=0.3)
    time.sleep(0.4)
    port.reset_input_buffer()

    def send(line: str, settle: float = SETTLE) -> None:
        # **Drain first.** The console answers every command and the machine logs
        # continuously, so without this a reply left over from the previous command
        # is read back as the answer to this one — which is how `wifi status`'s
        # text came to be reported as the verdict for `wifi apply`.
        port.reset_input_buffer()
        port.write(line.encode() + b"\n")
        time.sleep(settle)

    def collect(seconds: float) -> str:
        deadline = time.time() + seconds
        out = bytearray()
        while time.time() < deadline:
            chunk = port.read(4096)
            if chunk:
                out.extend(chunk)
        return scrub(bytes(out))

    # **`wifi status` probes readiness, not the banner.** The banner is printed
    # once, when the task arms; a script that opens the port after boot waits for
    # a line that has already gone by. `wifi status` always answers.
    send("wifi status")
    status = collect(DRAIN)
    if REPLY not in status:
        die(
            "the device did not answer on the provisioning console.\n"
            "It arms only when it has no SSID — so there may be nothing to do.\n"
            "Use `just mon <port>` to look, or POST /api/wifi-reset to forget one."
        )
    if "ssid_set=true" in status:
        # Not a refusal any more: the console arms whether or not a credential is
        # stored, precisely so a **wrong** network can be replaced over the cable.
        # That used to be impossible — the console only armed when unprovisioned,
        # and the only way to change a stored credential was `POST
        # /api/wifi-reset`, which needs a machine that is already online.
        print(
            "wifi: a credential is already stored; this will replace it.",
            file=sys.stderr,
        )

    send(f"wifi set {ssid}", settle=1.0)
    answered = replies(collect(1.5))
    if answered.startswith(f"{REPLY} err"):
        die(f"the device rejected the SSID: {answered}")
    if "ssid accepted" not in answered:
        die(f"no reply to `wifi set` — the console said: {answered or 'nothing'}")

    # **The password is an argument, not the next line.** `wifi pass <value>` is
    # a command in its own right, so there is no 30 s window between `wifi set`
    # and the credential: no deadline to miss, and nothing that depends on the
    # console task's loop rate. (The device also still accepts the password on
    # the next line, for an operator typing by hand.)
    send(f"wifi pass {password}", settle=1.0)
    answered = replies(collect(1.5))
    if answered.startswith(f"{REPLY} err"):
        die(f"the device rejected the password: {answered}")
    if "password accepted" not in answered:
        die(f"no reply to `wifi pass` — the console said: {answered or 'nothing'}")

    # **`wifi apply` deliberately answers nothing.** With a pending credential it
    # closes the window and sets an action (`provisioning.rs`,
    # `Reply::Accepted(Accepted::Apply)`); the control task then stores the blob
    # and **reboots the machine** so the radio comes up on it. So there is no
    # `CCWIFI` line to wait for, and a script that judges success by one reports
    # success while nothing was stored — which is what the first version of this
    # file did.
    #
    # The confirmation is therefore the firmware's own log line, and a reset is
    # corroborating evidence.
    send("wifi apply")
    stored = False
    refused = ""
    deadline = time.time() + 9.0
    while time.time() < deadline and not stored and not refused:
        chunk = scrub(port.read(4096))
        if f"{REPLY} err" in chunk:
            refused = chunk
        if "credential from the console was stored" in chunk:
            stored = True
    tail = collect(1.5)
    if "credential from the console was stored" in tail:
        stored = True

    if refused:
        print(refused.strip()[:400])
        die("`wifi apply` was refused by the device — its words are above.")
    if not stored:
        print(tail.strip()[-400:] or "(the device said nothing)")
        die(
            "no confirmation that the credential was stored. `wifi apply` answers "
            "nothing by design, so this script waits for the firmware's own "
            "'stored' log line; not seeing it means the write did not happen."
        )

    print("wifi: credential accepted and stored.")
    print("The machine reboots itself now; give it a few seconds to associate.")


if __name__ == "__main__":
    main()