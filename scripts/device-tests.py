#!/usr/bin/env python3
"""Run the on-target unit test suite and turn the serial log into an exit code.

Why this exists: `cc-hal-esp32`'s 67 `#[test]` functions used to be type-checked
by `just lint-esp32` and executed by nothing, because the crate does not build
for a host target. Two real device bugs shipped through that gap. This script is
the other half of the harness in `crates/cc-device-tests`: it resets the board,
reads UART0, and decides pass or fail.

The device is built with `panic = "abort"`, which the Xtensa target cannot
avoid (`unwinding` is unsupported here; `-Zbuild-std=std,panic_unwind` does not
link). A failing `assert!` therefore resets the chip instead of unwinding, so the
device persists a resume index and **all** accounting happens here, from the byte
stream. Nothing on the device can be trusted to reach the end of a run.

Exit codes:
    0  every registered case ran and passed
    1  at least one case failed, hung, or never ran
    2  the run could not be read at all (no port, no output, no completion)

Usage:
    python3 scripts/device-tests.py /dev/cu.usbserial-XXXX [--timeout SECONDS]

Requires a python with `pyserial`; the ESP-IDF virtualenv that the `esp-idf-sys`
build creates has one (`.embuild/espressif/python_env/*/bin/python`), and the
`just test-esp32` recipe finds it the same way `just mon-headless` does.

This script NEVER prints a credential. The only strings it echoes are the
device's own `CCTEST` lines -- case indices, case names and assertion messages.
None of the registered cases reads `.env` or a stored SSID, and the runner does
not load `.env` at all.
"""

from __future__ import annotations

import argparse
import sys
import time

import serial

BAUD = 115200

TAG = "CCTEST"
LIBC_MARKER = "CCTEST-MARKER-libc-buffered"
DIRECT_MARKER = "CCTEST-MARKER-direct"
REBOOT_CASE = "runner::the_console_reaches_the_wire_before_a_reboot"
# The pre-fix comparison. Its verdict is an observation, not a pass/fail: see
# `REBOOT_UNFLUSHED` in crates/cc-device-tests/src/main.rs.
UNFLUSHED_CASE = "runner::a_bare_esp_restart_drops_what_was_never_flushed"

LIBC_UNFLUSHED_MARKER = "CCTEST-MARKER-unflushed"
DIRECT_UNFLUSHED_MARKER = "CCTEST-MARKER-unflushed-direct"

# Per-case silence limit. The heaviest case is a JSON round trip; a second and a
# half is generous, and it is what turns a wedged test into a reported failure
# instead of a hung job.
CASE_TIMEOUT_S = 1.5


def pulse_reset(port: serial.Serial) -> None:
    """Classic esptool auto-reset: EN (RTS) low, BOOT (DTR) low, then release.

    Identical to `scripts/serial-log.py`, deliberately: a reset that differed
    between the two scripts would make one of them unable to reproduce the
    other. This produces `ESP_RST_POWERON`, which is what tells the on-target
    runner to start a fresh run rather than resume a previous one.
    """
    port.dtr = False
    port.rts = True
    time.sleep(0.15)
    port.rts = False
    time.sleep(0.1)


class Case:
    """One registered case and what happened to it."""

    def __init__(self, index: int, name: str) -> None:
        self.index = index
        self.name = name
        self.verdict = "not-run"
        self.detail = ""

def fields_of(rest: str) -> dict[str, str]:
    """Split `key=value key2=value2 <free text>` into a dict.

    The free text -- a case name, a panic message -- is kept under the empty key
    rather than dropped: it is the part worth showing when a case fails.
    """
    fields: dict[str, str] = {"": rest.strip()}
    for token in rest.split():
        key, sep, value = token.partition("=")
        if sep:
            fields[key] = value
    return fields


class Run:
    """Accumulated state across however many boots the run takes.

    Invariant: a case's verdict is only ever set from a line the device printed,
    and `settle` is the only other thing that may set one -- it accounts for the
    boot that ended without a verdict at all.
    """

    def __init__(self) -> None:
        self.cases: dict[int, Case] = {}
        self.actuator_line_seen = False
        self.total: int | None = None
        self.done = False
        self.notes: list[str] = []
        # Per-boot state, cleared on every `CCTEST begin`.
        self.running: dict[int, float] = {}
        self.markers: set[str] = set()
        self.expect_reboot: int | None = None
        self.expect_reboot_name: str | None = None
        # Case name -> what the wire actually showed for its markers. The
        # pre-fix reboot case is reported from here.
        self.observations: dict[str, str] = {}

    def case(self, index: int, name: str | None = None) -> Case:
        existing = self.cases.get(index)
        if existing is None:
            existing = Case(index, name or f"case-{index}")
            self.cases[index] = existing
        elif name and existing.name != name:
            self.notes.append(
                f"case {index} ran as both {existing.name!r} and {name!r}: the "
                "registry and the binary disagree"
            )
            existing.name = name
        return existing

    def settle(self) -> None:
        """Close out the previous boot.

        Anything still `running` when a new boot announces itself died without
        producing a result line -- a hard fault, a watchdog reset or a power
        event, none of which run the device's panic hook.
        """
        for index in self.running:
            case = self.cases.get(index)
            if case is not None and case.verdict == "running":
                case.verdict = "lost"
                case.detail = "the device rebooted with no result line"
        self.running.clear()

        if self.expect_reboot is not None:
            name = self.expect_reboot_name or ""
            if name == UNFLUSHED_CASE:
                # Not a verdict. Report what reached the wire and move on: this
                # case is the "before" half of the console-drain comparison, and
                # asserting that the marker was lost would be a test that only
                # passes while the bug is present.
                lost = sorted(
                    m
                    for m in (LIBC_UNFLUSHED_MARKER, DIRECT_UNFLUSHED_MARKER)
                    if m not in self.markers
                )
                arrived = sorted(
                    m
                    for m in (LIBC_UNFLUSHED_MARKER, DIRECT_UNFLUSHED_MARKER)
                    if m in self.markers
                )
                self.observations[name] = (
                    f"bare esp_restart(): {len(arrived)}/2 markers on the wire; "
                    f"lost {lost or 'nothing'}"
                )
                print(f"[device-tests] before/after: {self.observations[name]}")
            else:
                case = self.case(self.expect_reboot, REBOOT_CASE)
                missing = sorted(
                    m for m in (LIBC_MARKER, DIRECT_MARKER) if m not in self.markers
                )
                case.verdict = "failed" if missing else "passed"
                if missing:
                    case.detail = (
                        "never reached the wire before the reset: " + ", ".join(missing)
                    )
                    print(f"[device-tests] FAILED {case.name}: {case.detail}")
                else:
                    case.detail = "both markers arrived after the drain"
            self.expect_reboot = None
            self.expect_reboot_name = None

        self.markers.clear()

    def handle(self, line: str, now: float) -> None:
        if not line.startswith(TAG + " "):
            for marker in (LIBC_MARKER, DIRECT_MARKER):
                if marker in line:
                    self.markers.add(marker)
            return

        keyword, _, tail = line[len(TAG) :].strip().partition(" ")
        fields = fields_of(tail)
        index_text = fields.get("n", "")
        index = int(index_text) if index_text.isdigit() else -1

        if keyword == "begin":
            self.settle()
            self.total = int(fields["total"])
            start = int(fields["from"])
            print(f"[device-tests] boot: {self.total} cases registered, resuming at {start}")
        elif keyword == "actuator":
            self.actuator_line_seen = True
        elif keyword == "run":
            case = self.case(index, tail.split(" ", 1)[1] if " " in tail else None)
            if case.verdict != "lost":
                case.verdict = "running"
            self.running[index] = now
        elif keyword == "ok":
            case = self.case(index)
            # A case that already lost its first attempt stays lost. If the
            # device crashed it and came back round to re-run it, a plain
            # overwrite would report the second, lucky attempt and the run would
            # be green over a case that had already taken the chip down.
            if case.verdict == "lost":
                self.notes.append(
                    f"{case.name} crashed the device once and passed on a re-run; "
                    "the crash is the failure"
                )
            else:
                case.verdict = "passed"
                case.detail = fields.get("ms", "")
            self.running.pop(index, None)
        elif keyword == "panic":
            case = self.case(index)
            case.verdict = "failed"
            case.detail = f"{fields.get('at', '?')} {fields.get('msg', '')}".strip()
            self.running.pop(index, None)
            print(f"[device-tests] FAILED {case.name}: {case.detail}")
        elif keyword == "expect-reboot":
            # The device sends the case name positionally, then `drained=0|1`.
            # Take the first token that is not a `key=value` pair.
            name = next(
                (t for t in fields[""].split() if "=" not in t), ""
            )
            if name:
                self.case(index, name)
            self.expect_reboot = index
            self.expect_reboot_name = name or None
        elif keyword == "done":
            self.settle()
            self.done = True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port")
    parser.add_argument(
        "--timeout",
        type=float,
        default=180.0,
        help="overall budget for the whole run, across every boot (default: 180s)",
    )
    args = parser.parse_args()

    run = Run()
    deadline = time.time() + args.timeout
    last_seen = time.time()

    try:
        port = serial.Serial(args.port, BAUD, timeout=0.2)
    except serial.SerialException as error:
        print(f"[device-tests] cannot open {args.port}: {error}", file=sys.stderr)
        return 2

    with port:
        pulse_reset(port)
        buffer = ""
        while time.time() < deadline:
            chunk = port.read(4096)
            now = time.time()
            if chunk:
                last_seen = now
                buffer += chunk.decode("utf-8", "replace")
                while "\n" in buffer:
                    raw, _, buffer = buffer.partition("\n")
                    run.handle(raw.strip(), now)
                continue

            if run.running and now - max(run.running.values()) > CASE_TIMEOUT_S:
                for index in list(run.running):
                    case = run.cases.get(index)
                    if case is not None and case.verdict == "running":
                        case.verdict = "hung"
                        case.detail = f"no result within {CASE_TIMEOUT_S:g}s"
                        print(f"[device-tests] HUNG {case.name}")
                run.running.clear()
            elif now - last_seen > 20.0 and not run.done:
                run.notes.append("the device went quiet before finishing the run")
                break

    run.settle()

    buckets = {
        verdict: [c for c in run.cases.values() if c.verdict == verdict]
        for verdict in ("passed", "failed", "hung", "lost", "running", "not-run")
    }

    print()
    print("=" * 72)
    marks = {
        "passed": "ok  ",
        "failed": "FAIL",
        "hung": "HUNG",
        "lost": "LOST",
        "running": "????",
        "not-run": "----",
    }
    for name, note in sorted(run.observations.items()):
        print(f"  note  {name}")
        print(f"        {note}")
        print()
    for case in sorted(run.cases.values(), key=lambda c: c.index):
        suffix = f"  ({case.detail})" if case.detail and case.verdict != "passed" else ""
        note = run.observations.get(case.name)
        if note:
            suffix = f"  [observation: {note}]"
        print(f"  {marks[case.verdict]}  {case.name}{suffix}")
    print("=" * 72)
    print(
        f"  {len(buckets['passed'])} passed, {len(buckets['failed'])} failed, "
        f"{len(buckets['hung'])} hung, {len(buckets['lost'])} lost, "
        f"{len(buckets['not-run'])} never ran"
    )

    problems: list[str] = list(run.notes)
    if not run.done:
        problems.append("the run never reached 'CCTEST done'")
    if not run.actuator_line_seen:
        problems.append(
            "no 'CCTEST actuator ... INACTIVE' line: the boot log does not "
            "evidence that the pump, valve and heater were held inactive"
        )
    if run.total is not None and len(run.cases) != run.total:
        problems.append(
            f"the device reported {run.total} cases but {len(run.cases)} result "
            "lines were seen"
        )
    for name in (UNFLUSHED_CASE, REBOOT_CASE):
        if name not in run.observations and name not in {c.name for c in run.cases.values()}:
            problems.append(f"{name} did not run")
    for verdict in ("failed", "hung", "lost", "running", "not-run"):
        problems += [
            f"{verdict}: {c.name}"
            for c in buckets[verdict]
            # The pre-fix comparison is an observation, not a pass or a fail.
            if c.name != UNFLUSHED_CASE
        ]

    if problems:
        print()
        print("[device-tests] FAILED:")
        for problem in problems:
            print(f"  - {problem}")
        return 1

    print("[device-tests] PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
