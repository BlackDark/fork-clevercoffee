# Outstanding findings

**What was found during hardware bring-up.** Recorded 2026-10-01. Items #1 and
#2 were fixed afterwards and are struck through; the rest were still open when
this file was last written. Check the date on a row before acting on it — this is
a record, not a live tracker.

It lives in `history/` rather than in the runbook because none of it is a
procedure. Nothing here is something you run before a release; it is what was
found, why it was left alone, and what a decision would take.

A reader who wants to *do* something should be in
[`../operations/runbook.md`](../operations/runbook.md). A reader who wants to
know what was decided to leave broken should be here.

Most items are justified by "C++ parity" — this firmware reproduces a C++
behaviour on purpose, and the reasoning for each is in
[`divergences.md`](divergences.md). The consolidated behaviour-by-behaviour
comparison is in [`cpp-behaviour-comparisons.md`](cpp-behaviour-comparisons.md).

---

| # | What | Why it is not fixed | What is needed |
| --- | --- | --- | --- |
| 1 | ~~**The Scale template's brew row erases the setpoint row.**~~ | **FIXED**, by re-pitching the rows to `13 / 22 / 31 / 40 / 49`. [`divergences.md` §21](divergences.md#d21). The rows no longer collide and the setpoint survives a brew. | Closed. |
| 2 | **The Scale value column is 50 px and four labels are wider**, including English `Pressure: ` at 60 px. | Same family as #1, and closed by the same change. Measured widths are pinned by `tests/languages.rs`. | Closed. |
| 3 | **The EEPROM error line is 185 px into a 128 px panel** (57 px cut), in all three languages. | C++ parity: the same string, the same `displayMessage`, the same per-glyph clipping. | Shorter text, or wrapping into the five slots the call already leaves empty. Both are deliberate divergences. |
| 4 | **The German sensor-error line is 153 px into 128** (25 px cut); English and Spanish are 111 px and fit. | C++ parity. | A shorter German string. |
| 5 | **The OTA error title is 150 px in `fub17`, centred at `x = -11`** — clipped at *both* edges at once. | C++ parity, and no bounds test of any kind can detect it. | A smaller font for that screen, or a shorter title. |
| 6 | **The sensor-error and EEPROM message screens overlap by a pixel** (a 10 px pitch with an 11 px font). | The pitch cannot grow: `displayMessage` is six lines and six at 11 px is 66 px into a 64 px panel. | `profont10` for those two screens, which changes six screens' typography. |
| 7 | **The control loop's 10 ms period is not met**: mean tick work ~16 ms, achieved period ~13–22 ms. The work is **not localised** — a bisect says the temperature poll is about half of it, which does not square with a scratchpad read at 2.4 Hz. | I have not localised it and will not guess in the source. | A profile that is not more `now_ms()` calls: a GPIO toggle captured by the idle task's accounting, or an external logic analyser. |
| 8 | **`CONFIG_FREERTOS_HZ`** — measured at 1000 and **reverted**: identical work and period, so the tick rate was never the limiter. | Reverted on evidence. | Revisit only if #7 lands under a millisecond. Note it is 100 because **ESP-IDF 5.5.5's own default** is 100 (`components/freertos/Kconfig:37`), not the widely-quoted 1000. |
| 9 | **A sensor task cannot own the DS18B20**: the bit-bang is the only user of `esp_idf_hal::interrupt::free`, a process-global cross-core critical section, and running it from a second task asserts inside the kernel. | The mechanism is in the `FreeRTOS`/hal layer, not in a design choice of ours. | An upstream fix, or a driver that does not need a global critical section. The display half of the split works and is in the firmware. |
| 10 | **The cross-flash deadman assert**: a `Queue` with a blocking receive, a `std::sync::Mutex` hand-off and an ESP-IDF task notification each assert on this build. | Same. The frame hand-off is lock-free instead. | As #9. |
| 11 | **A reboot into `PID_DISABLED` is the C++'s behaviour**, not a bug: the power switch is a `Toggle` and a toggle reading off at boot starts disabled. `pid.enabled` wins only when no power switch is configured. | Correct parity. | If the config should win, that is a *config default* change, and it is a product decision. |
