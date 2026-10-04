# What the Rust firmware does differently from the C++

**One page. Every behavioural difference between this firmware and the C++ it replaces.**

The C++ in `src/` + `include/` is the parity oracle and the definition of feature scope. It is never
modified and never flashed. This document is the **index** of where the Rust port departs from it.

[`intentional-diffs.md`](./intentional-diffs.md) is the **detail**: each entry there has the C++'s
behaviour, the port's, the reasoning, and what pins it. This page is what you read when you want to
know "will this surprise me?" — deliberately short, and it says nothing the ledger does not.

Nothing here is accidental. Where the port is *safer* than the C++, that is stated as plainly as
where it differs.

---

## The five that can surprise an operator

These change what the machine does, or what a tool sees, in ways a user could notice.

| # | difference | direction | detail |
| --- | --- | --- | --- |
| 1 | **Both pump safety timeouts are armed.** A brew the C++ would run indefinitely now stops at 300 s. | stricter | §1 |
| 2 | **The device answers to `test-cc-rust`, not `silvia`.** Both firmwares run on one network during the migration and are not interchangeable — the port diverges on pump timeouts, the steam whitelist and the PID divide. | deliberate | §12 |
| 3 | **The four operator switches default to `enabled: true`, not `false`.** A machine that ignores its buttons now does not. | behavioural | §13 |
| 4 | **`POST /api/setpoint` takes the schema's 20–110 °C range, not the C++'s 0–150.** A value outside it is a `400`, not a silent clamp. | stricter | §23 |
| 5 | **HTTP Basic authentication is implemented**, and is applied at boot. The C++ had no auth at all. | new | §20 |

## The three that are additions with no C++ counterpart

Features that simply do not exist in the C++ firmware.

| # | difference | detail |
| --- | --- | --- |
| 6 | **Scale support exists at all.** The C++'s `HX711Scale` was never constructed, so by-weight brewing could never stop a shot. The port ships a working HX711 driver. | §11 |
| 7 | **The steam valve is whitelist-gated**, by a rule (`S5'`) the C++ does not have and its own comment admits it lacks. | §2 |
| 8 | **`POST /api/config/upload` exists**, and takes `application/json`. | §19 |

## The safety changes, all in the safe direction

| # | difference | detail |
| --- | --- | --- |
| 9 | The **water valve** is gated on the water tank, which the C++ did not do. | §3 |
| 10 | The **water valve's interlock** now consults the S5 whitelist too, not just the effect ordering. | §26 |
| 11 | **S1's over-temperature debounce counts probe samples, not control ticks** — the C++ tripped in ~30 ms where it meant ~1.2 s, latching emergency stop on a probe spike mid-brew. | §16 |
| 12 | **The reboot request shuts the hardware down** before its 500 ms pause. The C++ slept inside the control task with no interlock and the heater ISR still chopping. | §17 |
| 13 | **`brew.by_weight` stops the shot**, and a configuration where nothing *can* stop it is refused at load. | §24 |
| 14 | **The Dallas path applies `isValidTemperature`'s range**, which the C++ ignored. | §8 |
| 15 | **A dead temperature probe now reaches `SENSOR_ERROR`.** The C++ computed this and then dropped it on the floor, so the PID regulated a frozen reading indefinitely. | Phase 1, `3794bef6` |
| 16 | **A pump-watchdog trip logs why it happened.** The C++ left both watchdogs inert; the port arms them, and the trip says so. | §1 + `f4a6341b` |
| 17 | **A sleep request is honoured with the PID disabled**, rather than ignored. | §13 (duplicate numbering in the ledger) |

## The known deviation we cannot fix in firmware

| # | difference | why | detail |
| --- | --- | --- | --- |
| 18 | **The steam LED is not driven.** | GPIO1 is the UART provisioning console's TX line, and a pin cannot be shared on this HAL. **The C++ has the same conflict** — `pinmapping.h:45` says *"Moved from pin 1 (UART TX - conflicts with serial logging)"* while the `#define` on that same line is still `1`, so the C++ drives both and the last attach wins. The comment describes the move as done; it was not. The port keeps the console, because it is the documented recovery path for a machine on a nonexistent network. | §27 |

The C++'s own suggested alternative is GPIO 32, which is `SCALE_DATA_1` — so moving the steam LED
there is a **hardware** change on the machine, not a firmware one. The cost analysis is in §27.

## The API and protocol changes

| # | difference | detail |
| --- | --- | --- |
| 19 | `/api/ota/status` sends `status` as a string, not an integer. | §14 |
| 19b | **`/api/ota/{firmware,filesystem}` are implemented and are stricter than the C++:** refused while brewing or steaming, and the full safe hardware shutdown (pump, valve, heater) rather than a heater disable alone. `/api/ota/url` is registered and answers `501`. | §28 |
| 20 | `/api/status` reports `steamMode`, and keeps `brewing` as an addition. | §21 |
| 21 | CORS preflight is answered; the C++'s per-response `Access-Control-Allow-Origin: *` is not. | §22 |
| 22 | MQTT actually runs — it is implemented and driven from the control task, not stubbed. | §18 |
| 23 | `pid.regular.i_max = 0` disables integral action, rather than being silently rejected and leaving a ±100 integrator. | §25 |

## The implementation differences

Behaviour is the same; the mechanism is not. Listed so nobody re-investigates them as bugs.

| # | difference | detail |
| --- | --- | --- |
| 24 | The heater is chopped by a 10 ms `GPTimer` ISR. An earlier attempt used LEDC hardware PWM and was reversed (§5, superseded). | §9 |
| 25 | The PID derivative is taken over real elapsed time, not `SampleTime / 1000` — an integer division that yields `NaN` at any sub-second window. | §4 |
| 26 | The ABP2 pressure read no longer blocks, and checks values the C++ ignored. | §10 |
| 27 | Both temperature sensors are implemented (DS18B20 and TSIC-306). | §7 |
| 28 | `cc-display` does not implement `embedded-graphics::DrawTarget`. | §6 |
| 29 | Four display layout re-pitches, all fixing text that was clipped or overlapping. | §14, §15 |
| 30 | The Scale template's rows are re-pitched so the setpoint stops disappearing during a brew. | §15 (duplicate numbering) |

---

## How to use this with the parity harness

[`cc-parity`](../crates/cc-parity) reads `intentional-diffs.md` directly and classifies every scenario
difference against it.

**What that does today: nothing is measured.** `docs/rust-migration/baseline/cpp/` holds only
`.gitkeep`, so all **17** parity scenarios — one file each under
[`scenarios/`](./scenarios) — report `BASELINE-MISSING` and no difference is ever compared against
anything. Every classification on this page rests on reading the two codebases, **not** on running
them side by side.

**So an undeclared difference does not fail the harness today, and this page is not kept honest by a
machine.** Two things would have to exist first, and neither does:

1. **A baseline.** Capturing one means running the C++, which runs its own control loop on a powered,
   wired machine. That was a deliberate decision, not an oversight.
2. **A matcher per entry.** `intentional-diffs.md` carries 5 machine-readable `ledger` blocks against
   ~30 prose entries, so even with a baseline, most differences documented above would have nothing to
   classify against and would surface as unexplained rather than as intentional.

Treat "intentional" in this document as *reviewed and reasoned*, not *measured*.

## Adding to this document

If a change makes the firmware behave differently from the C++, it needs **both**:

1. a section in [`intentional-diffs.md`](./intentional-diffs.md) with the reasoning and what pins it,
   and
2. a row above, filed under the category that matches how a reader would encounter it.

A ledger entry with no row here means an operator could meet the difference without warning.
