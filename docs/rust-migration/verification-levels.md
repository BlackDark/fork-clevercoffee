# Verification levels

How to read every claim in these documents, and what would have to happen to raise a level.

Cross-links: [compatibility-matrix.md](compatibility-matrix.md), [inventory.md](inventory.md),
[decision-record.md](decision-record.md), [task-list.md](task-list.md).

---

## The levels

| Level | Definition | Evidence required |
| --- | --- | --- |
| `repo-verified` | Read directly from the source in this repository | A `file:line` citation |
| `build-verified` | Compiled and linked for the target from this repository | The build command and its result |
| `device-verified` | Built, flashed and exercised on physical hardware | The device, the commands, and the observed output |
| `needs confirmation` | Inferred, assumed, or from a source that was not read | Stated as an assumption, not a fact |

Only `device-verified` counts as fully supported. A capability that builds is not a capability
that works.

## State as of 2026-09-28

**No hardware was connected for this run.** Consequently:

- Nothing in these documents is `device-verified`.
- The original ESP32 is `build-verified` for the toolchain and the capability spikes.
- ESP32-S3 and ESP32-C6 are `build-verified` for the same spikes.
- Every claim about runtime behaviour, timing, electrical behaviour and physical rendering is
  `needs confirmation`.

## What was actually built, and how

Three spikes under [`spikes/`](../../spikes), built for all three targets with
`cargo build -Zbuild-std=core,alloc --release --target <triple>`:

| Spike | Targets | What it proves |
| --- | --- | --- |
| `hal-smoke` | esp32, esp32s3, esp32c6 | `esp-hal` init, the embassy executor, a hardware timer source, a periodic task |
| `stack-smoke` | esp32, esp32s3, esp32c6 | flash access, partition table read, I2C master, UART0, Wi-Fi station through `esp-radio`, a linked `embassy-net` stack |
| `usb-smoke` | esp32s3, esp32c6 | USB Serial/JTAG CDC as an async read/write channel |

Plus two host tools, verified by running them:

| Tool | What it proves |
| --- | --- |
| `espflash save-image --chip esp32 <elf> <out>` | a 2 571 412-byte ELF produces a 99 728-byte app image at offset `0x10000` |
| `tools/provision` | enumerates serial ports, opens one, and reports `FAIL OPEN_PORT` with a non-zero exit when it cannot |
| `just _assert-chip esp32 /dev/null` | the flash guard refuses to proceed when the chip cannot be read |

## What each unverified claim would need

| Claim | What would settle it |
| --- | --- |
| The DS18B20 bit timings are correct | flash `hal-smoke` plus the driver on the bench ESP32, read against a known-temperature probe |
| 11-bit DS18B20 conversion is 375 ms | the DS18B20 datasheet, or a measured conversion |
| The OLED renders within 128x64 with the computed layout | T-21, a real panel |
| The relay active levels are correct | a bench with relays, or the user's schematic |
| The switch debounce of 20 ms is enough for the real switches | the user's hardware |
| The heater PWM at a 10 ms window is stable | a bench with a heater, and the user's approval |
| Wi-Fi station mode associates and holds a connection | the bench device plus a router |
| `just wifi` and `just config-import` work end to end | T-19 on the bench device |
| USB Serial/JTAG works as a provisioning channel on S3 and C6 | those boards |
| The S3 and C6 pin maps | a schematic or a board from the user |
| The pressure sensor is the assumed ABP2 variant | the part marking, from the user |
| The C++ firmware's `config.json` is representative of a real export | a real export from the user's device |
| Dropping the Wi-Fi captive portal is acceptable | the user |

## Rules for updating these documents

- Raising a level requires the evidence named above, in the same commit as the claim.
- Lowering a level requires a reason.
- "Should work" is not a level. If a level is wrong, say so rather than leaving it optimistic.
