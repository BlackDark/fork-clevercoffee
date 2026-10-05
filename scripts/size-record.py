#!/usr/bin/env python3
"""Record a phase-gate image-size datapoint (docs/rust-migration/07 §4).

Appends one JSON object to docs/rust-migration/size-records.jsonl and rewrites
docs/rust-migration/size-baseline.json, which `just size` / `just size-check`
compare against.

Usage: size-record.py <image_bytes> <app_slot_bytes> <label> <records.jsonl>
"""

import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

BASELINE = Path("docs/rust-migration/size-baseline.json")


def git_rev() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], text=True
        ).strip()
    except Exception:  # noqa: BLE001 - a missing git must not block recording
        return "unknown"


def main() -> int:
    image, slot = int(sys.argv[1]), int(sys.argv[2])
    label, records = sys.argv[3], Path(sys.argv[4])
    if image > slot:
        print(f"refusing to record: image {image} B exceeds the app slot {slot} B")
        return 1

    record = {
        "label": label or "unlabelled",
        "recorded_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "git": git_rev(),
        "mcu": "esp32",
        "image_bytes": image,
        "app_slot_bytes": slot,
        "headroom_bytes": slot - image,
    }

    records.parent.mkdir(parents=True, exist_ok=True)
    with records.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record) + "\n")
    BASELINE.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(record, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
