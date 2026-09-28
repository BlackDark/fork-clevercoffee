# Image-size reporting for the Rust firmware.
#
# Normative policy: docs/rust-migration/07-image-size-budget.md
# Driven by the root justfile (`just size`, `just size-check`, `just size-record`).
# Every phase gate must run `just size` and paste the output into the gate record.

mcu := "esp32"

# The single binary is named `firmware` (04 §6), so the artifact is
# `target/<triple>/release/firmware` — NOT `.../cc-firmware`.
tgt := "xtensa-esp32-espidf"
elf := "target/" + tgt + "/release/firmware"

part_table := "rust/partitions_4M.csv"
baseline := "docs/rust-migration/size-baseline.json"
records := "docs/rust-migration/size-records.jsonl"

# Report image size, per-section breakdown and headroom.
size:
    #!/usr/bin/env bash
    set -euo pipefail
    export LC_ALL=C
    elf="{{elf}}"
    if [ ! -f "$elf" ]; then echo "no image at $elf — run: just build-esp32"; exit 1; fi

    # 1. The number that matters: the flashable app image, as espflash builds
    #    it (padding included). This is what has to fit the app0 slot.
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    espflash -S save-image --chip {{mcu}} "$elf" "$tmp/app.bin" >/dev/null
    image="$(stat -f%z "$tmp/app.bin" 2>/dev/null || stat -c%s "$tmp/app.bin")"

    # 2. The app0 slot, straight out of the Rust partition table.
    slot_hex="$(awk -F, '/^app0/ {gsub(/[ \t]/,"",$5); print $5}' "{{part_table}}")"
    slot=$((slot_hex))

    echo
    echo "target:      {{tgt}} ({{mcu}})"
    echo "image:       $elf -> $image B   (flashable app image, as espflash lays it out)"
    echo "app slot:    {{part_table}} app0 $slot_hex -> $slot B"
    headroom=$((slot - image))
    pct="$(awk -v h="$headroom" -v s="$slot" 'BEGIN{printf "%.2f", h*100/s}')"
    echo "headroom:   +$headroom B ($pct %)"
    echo

    # 3. Per-section split, so code growth is distinguishable from embedded
    #    asset growth (07 §4). The Xtensa `size` is whichever toolchain is
    #    installed: prefer the ESP-IDF one (it is the compiler that built the
    #    archives), fall back to the espup one.
    size_bin=""
    for c in "${XTTENSA_SIZE:-}" xtensa-esp32-elf-size; do
        if [ -n "$c" ] && command -v "$c" >/dev/null 2>&1; then size_bin="$(command -v "$c")"; break; fi
    done
    if [ -z "$size_bin" ]; then
        for c in .embuild/espressif/tools/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32-elf-size \
                 "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32-elf-size; do
            [ -x "$c" ] && { size_bin="$c"; break; }
        done
    fi
    if [ -n "$size_bin" ]; then
        echo "per-section  ($size_bin -A)"
        "$size_bin" -A "$elf" | awk 'NR==1 || /\.flash\.(text|rodata|appdesc)|\.iram0\.(text|vectors)|\.dram0\.(data|bss)|Total/'
        echo
        ram=$("$size_bin" -A "$elf" | awk '/\.iram0\.(text|vectors)/ {s+=$2} /\.dram0\.(data|bss)/ {s+=$2} END {print s+0}')
        echo "static RAM (iram0 + dram0.data + dram0.bss): $ram B"
    else
        echo "per-section: SKIPPED — no xtensa-esp32-elf-size found. Set XTTENSA_SIZE=/path/to/size."
    fi
    echo

    # 4. Delta vs the previous recorded datapoint.
    if [ -f "{{baseline}}" ]; then
        prev="$(python3 -c 'import json;print(json.load(open("{{baseline}}"))["image_bytes"])' 2>/dev/null || echo "")"
        if [ -n "$prev" ]; then
            echo "baseline:    {{baseline}} ($prev B, $(python3 -c 'import json;print(json.load(open("{{baseline}}")).get("label","?"))' 2>/dev/null))"
            echo "delta:      $(awk -v a="$image" -v b="$prev" 'BEGIN{printf "%+d B", a-b}')"
        fi
    else
        echo "baseline:    none yet — run: just size-record mcu={{mcu}} label=<gate>"
    fi

# CI gate: the image must fit the slot, and must not have grown >10 % without a
# recorded justification (07 §4/§5).
check:
    #!/usr/bin/env bash
    set -euo pipefail
    export LC_ALL=C
    elf="{{elf}}"
    [ -f "$elf" ] || { echo "no image at $elf"; exit 1; }
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    espflash -S save-image --chip {{mcu}} "$elf" "$tmp/app.bin" >/dev/null
    image="$(stat -f%z "$tmp/app.bin" 2>/dev/null || stat -c%s "$tmp/app.bin")"
    slot_hex="$(awk -F, '/^app0/ {gsub(/[ \t]/,"",$5); print $5}' "{{part_table}}")"
    slot=$((slot_hex))
    fail=0
    if [ "$image" -gt "$slot" ]; then
        echo "FAIL: image $image B exceeds the app0 slot $slot B by $((image-slot)) B (07 §3 drop order applies)"
        fail=1
    else
        echo "PASS: image $image B fits the app0 slot $slot B (+$((slot-image)) B)"
    fi
    if [ -f "{{baseline}}" ]; then
        prev="$(python3 -c 'import json;print(json.load(open("{{baseline}}"))["image_bytes"])')"
        growth="$(awk -v a="$image" -v b="$prev" 'BEGIN{ if (b==0) {print 0} else {printf "%.2f", (a-b)*100.0/b} }')"
        if awk -v g="$growth" 'BEGIN{exit !(g>10.0)}'; then
            echo "FAIL: image grew $growth % vs {{baseline}} (>10 %). Justify in docs/rust-migration/07-image-size-budget.md and re-record."
            fail=1
        else
            echo "PASS: growth $growth % vs {{baseline}} (limit 10 %)"
        fi
    fi
    exit $fail

# Record a phase-gate datapoint (07 §4). `just size-record esp32 gate-1`
record mcu="esp32" label="":
    #!/usr/bin/env bash
    set -euo pipefail
    export LC_ALL=C
    elf="{{elf}}"
    [ -f "$elf" ] || { echo "no image at $elf — run: just build-{{mcu}}"; exit 1; }
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    espflash -S save-image --chip {{mcu}} "$elf" "$tmp/app.bin" >/dev/null
    image="$(stat -f%z "$tmp/app.bin" 2>/dev/null || stat -c%s "$tmp/app.bin")"
    slot_hex="$(awk -F, '/^app0/ {gsub(/[ \t]/,"",$5); print $5}' "{{part_table}}")"
    python3 scripts/size-record.py "$image" "$((slot_hex))" "{{label}}" "{{records}}"
