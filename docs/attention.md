# Attention

The only task list. Read this before starting work.

**Problems** are faults someone noticed and did not investigate or fix. An empty section means stop. Do not invent a fault.

**Potential** is not a fault. List it when asked what is open. Start a row only when it is named. Do not drop a row until that is decided.

**Later** is dated. Skip a row until its date.

What works is [`status.md`](status.md). Closed findings are [`history/outstanding-findings.md`](history/outstanding-findings.md).

## Problems

None.

## Potential

- **Rotary encoder.** GPIO 4, 3 and 5 are declared and unused.
- **Zero-crossing dimmer.** GPIO 18 is declared and unused.
- **Acaia scale.** Measured, and it does not fit. Needs a decision before any build. [`history/divergences.md` §12](history/divergences.md#d12).
- **TSIC-306.** None is fitted. The silence-latch arm has never run.
- **Contactor timing.** The contactor's minimum on/off time and the realised heater-pin duty have never been measured.
- **HX711.** The code is in. No scale is fitted.
- **Steam LED.** The rule is tested. No pin is free: GPIO 1 is the UART console, GPIO 32 is the scale data line. Find another pin, or leave it. [`history/divergences.md` §30](history/divergences.md#d30).
- **Modern layout against a C++ frame.** Checked against the layout rules, not against a rendered C++ frame. [`display/parity.md`](display/parity.md).

## Later

- **2027-01.** Recheck the DS18B20 and blocking cross-task wakes before moving the probe off the control task. On 2026-10-09 `esp-idf-hal` 0.47.0 still entered one process-global `interrupt::free` lock, and the RMT driver's CRC helpers were still `todo`. [`history/outstanding-findings.md`](history/outstanding-findings.md) #9 and #10.
