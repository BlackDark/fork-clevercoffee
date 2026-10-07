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
| 12 | **A configuration the validator refuses costs the whole configuration, not the offending value.** `safety.emergency_temp=120` with the default `steam.setpoint=120` is refused (`EmergencyTempTooLowForSteam`), and the recovery is `stored but unsafe — DISCARDED`: every one of the 98 parameters reverts to its compiled-in default. Only the Wi-Fi credential is preserved, deliberately, so the machine stays reachable. | It is the recovery path of a safety feature, and nothing smaller is safe by accident: the machine must not run a configuration it has declared unsafe. The defect is that the remedy is not scoped to the value that made it unsafe. | Decide the scope, then implement it. The two candidates are *revert only the offending values to defaults and keep the rest*, and *refuse the **write** rather than punishing the next boot* — the second is better still, because the operator learns at the moment they push it. Both need the failure to name which keys were dropped. **Until then: on any machine you care about, read the boot line `the boot decision was …` before trusting anything else.** |
| 13 | **An OTA upload put the machine in `SENSOR_ERROR` for the rest of the session.** 9 s into the write the machine went `PID_NORMAL -> SENSOR_ERROR` and stayed there until the reboot; `/api/temperatures` then reported `currentTemp: NaN` with `heaterPower: 0`. The DS18B20 driver returned `Started`/`Waiting` and never reached a `Reading` — not even a fault line — across two `/api/restart` cycles **and** a full USB reflash with a chip reset. Only a USB power cycle would be expected to clear it; the board was recovered by writing `hardware.sensors.temperature.type=1` after #12 had reverted it to the wrong protocol, so #13's own recovery path is **not yet measured**. | It is a candidate symptom of the finding already on file that the DS18B20 bit-bang is the sole user of a process-global critical section, and a 11 s flash write with interrupts masked is exactly the wrong thing to bit-bang through. Guessing at the mechanism in the source is how the last four findings in this file were found. | A profile that shows where the 1-Wire read is issued relative to the flash erase, and whether the driver has a recovery path when a conversion never completes. If it does not, that is a second defect underneath this one: a machine that OTA-updates itself can come back unable to read its boiler, and the heater off, until someone unplugs it. |
| 14 | **Steam never opens the valve, in this port and in the deleted C++.** The steam whitelist and the gate are implemented and tested, but nothing ever *asks* for the valve: `SteamRunningState`'s tick (`cc-machine/src/states.rs:520`) only manages the water-injection pump, and in the C++ both `HardwareManager::openSteamValve()` (`9fa8c834:src/hardware/HardwareManager.cpp:397`) and `MachineStateContext::openSteamValve()` (`:556`) are defined and **called from nowhere**. Measured on the bench: in `SteamRunning`, `valve=Closed … pins valve=false` with `refused steam=0` — nothing was refused, it was never requested. | It is parity, and parity is not automatically correct: a machine that cannot open its steam valve cannot steam. Deciding whether `STEAM_RUNNING` should emit `OpenSteamValve` is a product decision about what steaming means on this machine, and it needs the real machine to answer, not a bench. | A decision, then one line in the reducer's `SteamRunning` tick, then steam on the machine. **Owner: Eduard Marbach.** Recorded here and in [`divergences.md` §35](divergences.md#d35), not in the ledger, because it is a question rather than a divergence. |

**Also confirmed on hardware, 2026-10-07**, and therefore no longer a guess:
the deleted bring-up inhibit really was what held the water shut (§35.2). With
it gone and LEDs on GPIO2/17/27, a brew switch press lights **both** the pump
LED and the valve LED and drops the heater LED — pump, valve and heater all under
the reducer's control on real silicon.

| 15 | **A brew pressed during the emergency latch fires when the latch clears.** Measured 2026-10-07 on a bench ESP32 with a bench build: the machine tripped into `EMERGENCY_STOP`, refused the brew press (`latched 1`, pump LED dark), then — once the probe cooled below the threshold — left `EMERGENCY_STOP` and **started that brew**, with nobody pressing anything. `EmergencyStop`'s tick (`cc-machine/src/states.rs:557`) re-asserts the shutdown and disables the PID and nothing else; no request is drained, so `requests.brew_start` survives the whole latch. The C++ behaves the same (`EmergencyStopState::performEmergencyShutdown` is `emergencyShutdown()` + `setPidRuntimeState(false)`, and no handler clears `brewStartRequested_`), **so this is parity — and it is also a direct violation of this repository's own `AG-REPO-24`**, which requires a state that cannot act on action requests to drain them so a later state cannot. | It is a one-line-class fix and the rule already says to do it, so what is actually blocking it is that it is a **deliberate-looking divergence** from the C++ and needs a recorded decision, not a code change. The shape of the hazard: an operator taps brew while the machine is over-temperature, walks away, and the boiler cools below 100 °C and the machine starts a brew with nobody there. On a machine the water is at brewing temperature and the reservoir is not empty. **FIXED and VERIFIED ON HARDWARE 2026-10-07** by
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

| 18 | **`BACKFLUSH_FILLING` never re-asserts its pump or valve, and that is now a silent stall.** Its `on_entry` emits `EnablePump` + `OpenWaterValve` and its `update` re-asserts neither, so one tank-interlock refusal on entry leaves the fill incomplete for the whole cycle: the user sees a backflush start and nothing happens, and the one `REFUSED` line scrolls away. It fails safe and is recoverable by hand. `AG-REPO-25` asks for the re-assert; the C++ does not do it. **It is deliberately unchanged**, because preserving it is a *recorded decision* (`cpp-findings.md` §13 — "Rust: preserved. Pinned by `s13_*`") and this branch was not asked to revisit it. What changed is the cost: before the inhibit's removal a refused pump was indistinguishable from correct behaviour. | A deliberate divergence from the C++ that needs the owner, and a one-line change plus a rewrite of `s13_backflush_filling_never_re_asserts_its_hardware`. Recorded in [`divergences.md` §37](divergences.md#d37) as explicitly *not* taken. | **Owner: Eduard Marbach.** Decide whether `AG-REPO-25` outranks the recorded preservation. |
