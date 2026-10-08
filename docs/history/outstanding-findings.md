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

---

## Added 2026-10-07 — from a bench ESP32, with LEDs on the three actuator pins

Found while running the R4-04 procedures in
[`../operations/runbook.md`](../operations/runbook.md) §13 with the water path
live. Every one of these is measured, none is a bench artefact, and each row says
below whether it is still open.

| # | What | Why it is not fixed | What is needed |
| --- | --- | --- | --- |
| 12 | **Validator rejection used to wipe all 98 parameters.** `safety.emergency_temp=120` with `steam.setpoint=120` came back as `stored but unsafe — DISCARDED`, Wi-Fi credential kept, sensor type reset to TSIC-306. **Now the write is refused, and boot reverts only implicated keys.** Measured 2026-10-08, bench `10.0.1.168`: old image stored `120` (`200`); new image logged `reverted safety.emergency_temp`, `the repair is persisted`, `stored but unsafe` (not `DISCARDED`). All 98 parameters matched the pre-plant snapshot (`hardware.sensors.temperature.type` stayed `1`, 20.62 °C, `PID_NORMAL`). Repeat POST answered `400` naming `safety.emergency_temp`. Next `pid.enabled=1` answered `200`. `the boot decision was DiscardedUnsafe(...)` is the blob as loaded, before repair. [`divergences.md` §38](divergences.md#d38). | Closed on the bench. | — |
| 13 | **An OTA upload put the machine in `SENSOR_ERROR`.** 9 s into the write, `PID_NORMAL -> SENSOR_ERROR` until reboot. **The control tick now skips the probe and re-applies the shutdown while the session is busy.** Closed on the bench 2026-10-08. [`../operations/runbook.md` §13.4](../operations/runbook.md). | Closed on the bench. | — |
| 14 | ~~**Steam never opens the valve, in this port and in the deleted C++.**~~ **NOT A DEFECT — closed 2026-10-07.** I raised this as a possible defect on the reasoning that nothing ever opens the steam valve, and the owner corrected me: **steam is released by a hand-operated wand valve.** The steam switch heats the boiler to `steam.setpoint` (~120 °C), pressure builds in a closed system, and the operator turns the handle by hand; the water switch while steaming refills the boiler so more steam can be made. Opening GPIO17 during steam would give the pressure somewhere to go that is not the wand, so **nothing requesting it is the correct behaviour.** Verified in the deleted C++: `openSteamValve`/`closeSteamValve` appear eight times — three declarations, two definitions, one forwarder — and not one call site; `pinmapping.h:37-39` lists exactly three relays, so there is no steam solenoid to drive; and `applier.rs:74` records that `Effect::OpenSteamValve` is never emitted by the reducer. What *was* wrong was the **recorded reason** for `cc_safety::steam_flow_allowed`, which said an ungated steam valve is an ungated water valve. That is true of `ValveState`'s model and false of the machine; corrected in `pins.md`, `GLOSSARY.md` and the two history pages that repeated it. | — closed. | The gate stays: it is a guard against a future change driving the water relay as a steam outlet, and it costs one comparison per tick. |
[`divergences.md` §36](divergences.md#d36): `EMERGENCY_STOP` drains the action
requests on entry and on every latched tick, emitting
`Effect::ClearActionRequests`. Three tests in
`cc-machine/tests/emergency_latch_drains.rs`, all verified to fail against the
pre-fix code, plus the bench run: threshold 30 °C, probe warmed by hand, the
machine tripped, a brew press during the latch did nothing, and **when the probe
cooled and the latch cleared on its own, no brew started** and the heater came
back — which is the whole of the defect, gone. Three tests in
`cc-machine/tests/emergency_latch_drains.rs`, each verified to fail against the
pre-fix code. **Owner: Eduard Marbach.**

| 16 | **After an OTA, `just flash <port>` did not change the running image.** Measured 2026-10-07. Once `esp_ota_set_boot_partition` selects a slot, `otadata` keeps naming it, and `espflash flash` writes the application to the ELF's own app offset — `0x10000`, `app0` — regardless of which slot the chip is running from. So the flash reports success, the machine keeps booting the slot it was already on, and the change is invisible. Found because a bench build flashed this way did not take: the emergency-threshold floor was still 120 afterwards, and the image reached the board only when it was **uploaded over OTA**, which switched the slot correctly. **FIXED 2026-10-07** (`ac55c9ac`): `flash-elf` now writes **both** app slots — `espflash flash` for `app0`, then `espflash write-bin` at the `app1` offset, which is read out of `rust/partitions_4M.csv` so the recipe and the table cannot drift again. Writing the slot the chip is running from is safe: while flashing, the chip executes the ROM loader from IRAM, not the application. **Verified with the operation that found it:** on a board booting `app1` (`Loaded app from partition at offset 0x1d0000`), a bench build flashed over USB now moves `safety.emergency_temp`'s floor from 120 to 20, where the same flash before the fix left it at 120. |

| 18 | **Backflush fill and flush re-assert pump and valve each tick.** A refused apply on entry used to leave the fill running with both off. The C++ only logs in `update`. Closed 2026-10-08. [`divergences.md` §39](divergences.md#d39). | Closed. | — |
