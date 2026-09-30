# Open decisions

Six decisions the migration turns on. Each carries the evidence, what it costs, and the condition
that closes it. Facts referenced here live in [findings.md](findings.md); numbers in
[measurements.md](measurements.md).

**Settle these with measurements, not with argument.** Where a decision was already settled by
measurement on the bench, the evidence is marked device-verified and the decision is closed.

---

## 1. Platform stack — OPEN, highest leverage

Everything downstream follows: image size, whether the partition table must be rebalanced, who owns
HTTP, whether TSIC-306 capture is even possible.

**Two answers, three efforts, both defensible.**

| | `esp-idf-svc` + `std` (`design`, `rewrite`) | `esp-hal` + `embassy` (`space2`) |
|---|---|---|
| Coverage | HTTP, OTA, NVS, filesystem, TCP/IP all present | all four must be owned in-project |
| Wi-Fi | the stack the device is proven on | `esp-radio` pre-1.0; appliance-hostile issues open upstream |
| Verification | `esp-idf-svc` self-reports **no HIL tests** and lag behind stable IDF | per-chip support is **machine-enforced in CI** with HIL runners |
| Concurrency | FreeRTOS; `embassy-executor` is **disqualified** — its `critical-section` is a recursive mutex, not ISR-safe | one auditable executor; the same critical-section property is what `embassy` needs |
| Measured size | 997 648 B = 58.5 % of the 1.625 MB slot (`design` SPIKE-1) | 99 728 B app image (`space2`) |

Note the disagreement is **not** about `embassy`. `design` and `rewrite` rejected `embassy-executor`
for exactly the ISR-safety reason `space2` depends on it for; on `esp-idf-hal` that property is
absent, on `esp-hal` it is present.

**Cost of `esp-hal`:** roughly 600–900 lines of owned glue per subsystem — HTTP server, 1-Wire /
DS18B20, config store, provisioning transport — and each becomes host-tested-only until the board
crates reach a compiler.

**Done when** a build of the chosen stack exists on the bench carrying every capability the
firmware needs, or a measured gap list explains what it cannot carry. `space2` has the first for
its stack on paper and **not** the second: its board crates have never seen a compiler.

---

## 2. Heater relay polarity — OPEN, gates the application layer

A hardware fact, not a design choice. An undriven GPIO during reset energises a 2 kW heater, and
no firmware can prevent it.

`design` escalated it as BLOCKED, "must not be implemented as written". `rewrite` adopted a
**refusal** of `LOW_TRIGGER` from the recovered oracle — the C++ honours `trigger_type` and permits a
configuration that is unsafe on reset. `space2` moves the heater off the strapping pin
(GPIO2 → GPIO4) but the polarity question stands.

Gates the heater PWM, which gates the wiring task, which gates the whole application layer. Nothing
downstream is parallelisable past it.

**Done when** a meter on the relay coil, boiler disconnected, reports the polarity that the installed
machine is actually wired for — recorded in this directory as device-verified.

---

## 3. Heater output mechanism — CLOSED by measurement

Recorded here because the reasoning matters more than the answer.

| Option | Outcome |
|---|---|
| LEDC at 1 Hz | **Panics on every boot.** `ledc_ll_set_duty_start` spins inside `portENTER_CRITICAL` waiting for the last duty change — original ESP32 only — masking interrupts for up to a full carrier period. ~1 s against a 300 ms watchdog, and it panics at duty 0 because the wait precedes the duty value. |
| 100 Hz software carrier | Wrong by 100×. The ISR rate is not the switching rate: the C++'s level changes **twice per second**, not a hundred times. |
| **10 ms GPTimer ISR** | **Shipped.** What the C++ and the recovered firmware both use. |
| Hardware PWM (`space2`) | Unreached. Removes the interrupt entirely, and `space2` holds the heater *off* rather than driving the pin when idle — a machine that ignores its PID is less noticeable than one that does not heat. |

The LEDC commit and its reversal are **two commits apart** in `rewrite/rust`. device-verified.

`LedcPwm` is retained behind a `const` assert so no future edit can reach a duty write. Its
`set_duty` silently clamps rather than erroring, and at `Resolution::Bits20` full power becomes
indistinguishable from off — both pinned by tests.

---

## 4. Target scope — OPEN

`design` and `rewrite` port the original ESP32 only. `space2` targets ESP32 + S3 + C6 and found the
**C6-DevKitC-1 cannot fit the pin budget**: 23 exposed → 21 less USB → 15 less the module's six
SDIO flash pins → **14 usable against 17 needed**. The vendor guide contradicts its own J3 table on
whether the flash pins are broken out. Resolution there was a user-approved feature reduction
(3 indicator LEDs, 2nd HX711 cell disabled).

Two further facts transfer regardless of the decision: the **S3-DevKitC-1 v1.0 and v1.1 differ** —
the RGB LED is GPIO48 on v1.0 and GPIO38 on v1.1, so a pin map written against one hits the LED on
the other — and **neither chip has a 1-Wire peripheral** (C6 datasheet v1.5 lists none; the claim
that S3 has one was refuted).

Note the S3 pins no input-only block, so the external-pull-up requirement on the four panel switches
[GPIO34/35/36/39] is original-ESP32-specific.

**Done when** either the board list is fixed and each board's pin budget is measured, or the
project scopes to the original ESP32 in writing.

---

## 5. Parity policy — OPEN, mostly settled by practice

**Bug-for-bug parity is rejected by all three.** The C++ has blocking safety defects and a faithful
port carries them.

What differs is the boundary, and the boundary is the decision:

- `rewrite` enumerates 11 safety paths (S1–S11) and fixes the 5 that are broken.
- `space2` bounds parity to functional, control and API behaviour only, exempting defect fixes,
  and keeps the C++ tree live as a rollback path.
- `design` requires every behaviour change declared in the commit message and the task list.

**Settled by practice:** parity needs an explicit divergence register, because the word alone hides
changes. `rewrite` ships `intentional-diffs.md` with machine-readable `ledger` blocks and a runner
that exits non-zero on any diff the ledger does not explain — and a test proving a synthetic
*undeclared* diff fails while a declared one passes. `space2` ships a `parity-report.md` row per
behaviour (`pass`/`fixed`/`deviation`/`gap`/`unverified`) naming the test that is the evidence.
`design` records its own *absence* of such a register as a gap.

**Done when** a divergence ledger exists and its runner fails on an undeclared diff.

**Open sub-question:** the C++ baseline is **not captured** — flashing the C++ runs its own control
loop, so 13 scenarios report `BASELINE-MISSING` and the runner exits 2. Nothing was fabricated,
which is the correct state; the capture method is unsolved.

---

## 6. Config migration path — OPEN, the highest-risk single artifact

All three severed NVS, partitions, assets and OTA. The only surviving bridge is:

> export `config.json` from the old UI → flash over USB → import into the new firmware

One JSON round-trip carries the migration, and the three efforts disagree on how it went:

- `rewrite` — **works.** The ported blob serialises to 2 077 B against the lost oracle's logged
  2 071 B for a 98-key schema; a 6-byte delta, treated as corroboration of the parameter shape.
- `space2` — **the thing most likely to break.** Porting found two defects that would each have
  refused *every* export: the exporter wrote dotted keys as leaf names, and `format_version` was
  rejected outright. Both survived two review passes because the repo's own `config.json` lacks that
  key.
- `design` — flags the round-trip as unverified and requires a **report** (accepted / rejected with
  reason / unknown key / out of range) rather than a boolean.

Full detail in [findings.md](findings.md) §4.

**Done when** a real C++ export fixture imports with every parameter accounted for and re-exports
equivalently. **The fixture must come from the old firmware, not the repository's `config.json`** —
that file is precisely what hides these bugs.

---

## Watch: unexplained

**[findings.md](findings.md) §7** — the control tick overruns its 10 ms budget in ~62 % of ticks
(worst 32 ms) and the cause is **unknown**. The scale is ruled out by measurement: with the sampler
disabled the overrun is *more* frequent, same worst case. The project's own acceptance criterion
"zero ticks > 10 ms" currently fails, and relaxing the budget is the wrong response. Whoever picks
this up owns an open investigation, not a regression.
