# Status

**Dated 2026-10-08. Owner: Eduard Marbach** (`mail@eduard-marbach.de`), who also
owns the wired machine. Re-verify with `git log --oneline -1` and `just gate`
before trusting a line below.

**The port is closed out as of 2026-10-06.** Five items that were open are now
settled decisions — the C++ parity baseline, the water-path inhibit, the
configuration hand-off, the R4-04 procedures and the OTA surface. Each carries
its own entry below with a date, a named owner, the reasoning and its reversal
condition; the reasoning is in
[`history/divergences.md` §35](history/divergences.md#d35), and the four claim
states this page uses are defined in [`GLOSSARY.md`](../GLOSSARY.md) under
"How a claim ends". **Closing the port found two hardware defects** — a flash
path that never wrote the partition table, and an OTA that never booted what it
wrote. **Both are fixed and verified on hardware**; each is recorded below.

**This is the only page in this repository permitted to claim what works.**
Everything else states rules ([`AGENTS.md`](../AGENTS.md), via `AG-*` numbers) or
evidence. A status claim anywhere else is unverified until you have run
`git log --oneline -- <file>` on it — `AG-REPO-2`.

Every line below is a **pointer**, not a claim: a commit, a test, or a
measurement. If you cannot follow the pointer, the line is wrong and so is this
page.

---

## What works today

**The Rust firmware boots, regulates and serves.** Recorded 2026-09-30 and
2026-10-01 on the board; the full record with the measurements is
["Where the migration actually is"](./history/README.md#where-it-is-now).

- **The state machine, the PID and brewing are on the device.** R4-01, `4c4e072`.
  The reducer is wired into the 10 ms control task
  (`crates/cc-firmware/src/control.rs`) and effects are applied through
  `cc-hal-esp32/src/actuators.rs` in the same tick. At a 30 °C target with a
  ~7 K error the duty settles at ~48 %; at 95 °C with a 72 K error it goes to
  100 %. Both measured on the board, reproducible from the web UI.
- **The display renders every frame, and `failed=0`.** `present=true`, frames
  tick over on schedule. The SSD1306 shares the I²C bus with the ABP2 behind a
  `Mutex` and the frame is chunked into 8 bus writes, not 64, so the pressure
  sensor is not starved.

  **Layout.** Findings #3–#6 (clipped EEPROM, German sensor, OTA title, 1 px
  row overlap) are fixed in host tests, not on the machine.
  [`history/divergences.md` §40](history/divergences.md#d40).
- **The display parity oracle still links the real U8g2, and the bitmaps it
  draws are the firmware's own.** The C++ tree it used to pull artwork from is
  gone, so `crates/cc-display/examples/emit_bitmaps.rs` generates the oracle's
  C bitmap header from `cc_display::bitmaps::ALL` at build time — one copy, and
  it is the copy that ships. U8g2 itself moved from PlatformIO's registry copy
  (`src/clib/`) to upstream tag `2.36.18` (`csrc/`, `cppsrc/`), fetched by
  `just u8g2`; it is the same version, a different packaging.
  `just test-display-parity`: 2 passed. `just snapshot-display`: 1 golden
  rendered, unchanged.
- **The documentation has one map, one architecture page and one glossary.**
  `docs/index.md` is exhaustive again (`AG-REPO-28`): `docs/api/openapi.yaml`
  had been missing from it for the whole port, which is how a 759-line orphan
  survived. `docs/architecture.md` answers "what is this thing" in one page,
  `GLOSSARY.md` pins the words inherited from the deleted C++, and
  `docs/history/README.md` tells the transformation as a narrative rather than as
  a hardware spec. `scripts/check-doc-links.py` and `scripts/check-divergence-refs.py` now
  also resolve **heading anchors**, not just files: the ledger was renumbered
  twice and 19 of `differences.md`'s 31 `§N` pointers rotted silently while the
  existence-only check passed. It now also resolves **backticked paths in prose**,
  which markdown link syntax cannot see — that is how a live skill note went on
  citing a deleted script with every gate green. 4,241 paths and 136 anchors,
  none broken. `docs/archive/` and `docs/history/` are excluded from the prose
  check: their job is to record what was true then (`AG-REPO-29`, `AG-REPO-30`).
- **The API spec is now checked against the routes.** `scripts/check-openapi.py`
  diffs `docs/api/openapi.yaml` against the `ROUTES` table in
  `cc-hal-esp32/src/web.rs` in both directions and runs in `just check` and CI
  (`AG-REPO-31`). The spec had drifted: it covered 24 of 28 routes and omitted
  `/api/sleep`, `/api/wake` and `/events`, which are exactly the three carrying
  deliberate divergences. All 28 are now present.
- **`extract_fonts.py check` passes again.** It compared the whole of
  `font/data.rs`, licence header included, against generator output that never
  emitted that header — so it could not pass, and had not been run since the
  header was added. It now compares only the generated region, and
  `font/data.rs` is byte-identical to upstream `csrc/u8g2_fonts.c` at `2.36.18`.
- **The no-allocation gates measure the code, not the test harness.** Rendering a
  frame and running a control tick both allocate zero heap bytes, and both are
  asserted by a `#[global_allocator]` that counts the **calling thread**. It used
  to count the process, so libtest's own allocations landed in the window:
  `cc-display`'s gate failed 62 of 80 runs under CPU load. Instrumenting the
  allocator showed all four offending allocations on the harness thread, none in
  `cc-display`. Measured after the fix: **0 of 80** under the same load.
  `crates/cc-display/tests/frame_allocations.rs`,
  `crates/cc-machine/tests/tick_allocations.rs`, and the shared
  `benches/alloc.rs` in each crate, which is what the numbers above come from.
- **98 parameters are writable over HTTP and survive a reboot.**
  `cc_config::schema::PARAM_COUNT` is 98 (`schema.rs:92`) and pinned by
  `assert_eq!(SCHEMA.len(), PARAM_COUNT)` (`schema.rs:1858`), by
  `crates/cc-config/tests/config_schema.rs:43` and by
  `crates/cc-web/src/help/tests.rs:91`.
- **A machine flashed from the C++ says why its Wi-Fi is gone.** On a boot where
  this firmware's own store is empty, it opens the C++'s `config` namespace
  read-only, takes one key name, and — if there was one — prints a single `warn`
  line naming both namespaces, stating that the previous settings were **not
  deleted** and are still on the chip, and telling the operator to re-enter the
  SSID and password. Nothing is read but a key name; nothing is written.
  `cc_config::predecessor::startup_notice` (the words, 4 host tests) and
  `cc_hal_esp32::nvs::probe_predecessor` (the check). Nothing is read but a key
  name; nothing is written. The line reaching a telnet reader untruncated is
  pinned by
  `cc_web::telnet::tests::the_predecessor_boot_lines_are_not_truncated_on_the_wire`,
  against the real formatter rather than a copied byte count.
  **Verified by test and by reading the code; the line has not been seen on a
  board.** [`divergences.md` [§32](history/divergences.md#d32)](./history/divergences.md).
- **The web UI is served from flash and renders.** `cc-hal-esp32/build.rs` embeds
  the gzip bundle with `include_bytes!`, which is why the bundle costs
  0 B of RAM. Embedded total measured from `ui/packages/frontend/dist` on
  2026-10-08: 195,436 B, of which the JS file is 179,038 B gzip. A deep link
  to a client-side route (`/ui/config/behavior`) boots the
  configuration page with its live parameters, verified in Chrome on the device.
- **`/api/status` reports the radio truthfully.** `wifiAssociated`, `wifiSignal`,
  `wifiOffline` and `ip` come from `network::publish_radio`, which runs on the
  1 s poll. The control task's own publish carries them **forward** rather than
  defaulting them, because `Shared::publish` replaces the whole slot: the
  control task publishes every 10 ms, so "the radio publishes second" only
  described the last microsecond of each second and the association flag was
  gone again before any client could read it. Measured before the fix on a bench
  ESP32 associated at −45 dBm with `10.0.1.168`: `/api/status` reported
  `wifiAssociated: false, wifiSignal: 0, ip: null` continuously while
  `publish_radio` was provably writing the true values every second. After it:
  `wifiAssociated: true, wifiSignal: 4, ip: "10.0.1.168"`.
- **`just flash` and `just test-esp32` work.** Both shelled out to
  `cargo espflash`, which builds the binary itself, accepts no `-Z`, and reads
  build-std from the cargo config rather than the environment — so both died
  with "'build-std' not configured" from `e4ec70bd` (2026-10-03) onwards. The
  image is now built by the `build-*` recipe and flashed from its ELF by the new
  `flash-elf` recipe, which `just doctor` asserts the standalone binary for.
- **The on-target suite is green on the board.** `just test-esp32` on an
  original ESP32 rev 3.0: **112 passed, 0 failed, 1 lost, 0 hung**, firmware
  restored afterwards. Three cases failed on the first run and all three were
  defects in the cases, not in the firmware — two table-wide invariants in
  `web.rs` that the deliberate `/api*` preflight wildcard trips, and an OTA
  session test that claimed a second session without releasing the first.
- **Every command endpoint on the homepage works.** Tested individually on the
  bench: `/api/pid`, `/api/steam` and `/api/backflush` toggle and report the
  device's own state; `/api/setpoint` keeps a fractional value; `/api/wake` and
  `/api/sleep` move the machine. Three were broken and are fixed — a truncated
  setpoint, a backflush mode that could not be turned off, and an unknown
  `/api/` path answered `405` instead of the C++'s JSON `404`.
  [`divergences.md` [§33](history/divergences.md#d33)](./history/divergences.md).
- **Wi-Fi provisioning over the UART console is exercised.** `just
  wifi-provision /dev/cu.usbserial-224140` stores the credential from `.env`,
  the machine reboots, associates at `10.0.1.168` and serves the API. The
  script also accepts the reset as confirmation, because the firmware's
  `credential from the console was stored` line does not reach UART0 — see the
  open item below.
- **The gate is green.** `just gate`: fmt-check, clippy (host and
  device) with `-D warnings`, rustdoc `-D warnings`, the host suite, the parity
  harness, the device-test audit, the Xtensa release build, and the size budget.
  **1,720,176 B**, which fits the 1,835,008 B app0 slot with +114,832 B to spare.
  Re-recorded 2026-10-08 as `ui-bundle-2026-10-08`. The 2026-09-30 baseline
  (1,559,520 B) was passed at +10.29 % by the rebuilt web bundle. The 10 %
  limit is unchanged.
  **The slot this is measured against is now the slot the device has.** Until
  `08f4312c` (2026-10-06) `just flash` never wrote the partition table, so no
  device was carrying `rust/partitions_4M.csv` at all — the figure was real
  arithmetic about a table that was on no chip. It is now true of a flashed
  device: the board boots `app0 0x10000+0x1C0000` out of the CSV, and that
  2026-10-06 flash log printed `App/part. size: 1,701,296/1,835,008`.
- **The C++ tree was frozen while the port ran, and then deleted.** It stood
  unchanged from branch point `2006b710` until it was removed on 2026-10-06
  (`a36ebc50`, 248 files). That the deletion is what changed it is checkable:
  `git diff --stat 2006b710..HEAD -- src/ include/ lib/ platformio.ini
  partitions_4M.csv` reports **153 files changed, 29,023 deletions**, and every
  one of them is the deletion.
- **Every deliberate difference from the C++ is written down.**
  [`docs/differences.md`](differences.md) is the one-page index;
  [`history/divergences.md`](history/divergences.md) is the detail, holding 34
  sections and 5 machine-readable `ledger` blocks that `cc-parity` classifies
  against. Every prose reference into it is anchored and machine-checked by
  `scripts/check-divergence-refs.py`, so a renumber cannot silently rot one.

---

## What is explicitly NOT done

Accurate as of the date above, and each taken from code or a findings document
rather than from memory. **The four states are defined in
[`GLOSSARY.md`](../GLOSSARY.md) under "How a claim ends"** — done, closed by
decision, residual risk, not started. The close-out decisions of 2026-10-06 are
[`history/divergences.md` §35](history/divergences.md#d35).

- **An OTA writes the new image and does not boot it. Measured 2026-10-06,
  fixed 2026-10-07 (`83b3c419`), verified on hardware.** On a bench ESP32
  carrying the project's partition table, `POST /api/ota/firmware` with a valid
  image answers `200 {"success":true, "message":"Update successful. Device will
  restart.","restart":true}` and the device rebooted **into `app0`, the slot it
  had just replaced**. Cause, read in ESP-IDF v5.5.5 and then confirmed on the
  board: `esp_ota_end` validates the image and **does not select the slot**
  (`esp_ota_ops.c:477-524`); the only writer of `otadata` on the write path is
  `esp_ota_set_boot_partition` (`:599`), and with
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` unset nothing else switches the slot
  either. `Writer::end` now calls it, and only after a successful
  `esp_ota_end` — a rejected image must leave `otadata` on the slot known to
  work. **Measured after the change:** the same upload answers `200` and the
  bootloader logs `Loaded app from partition at offset 0x1d0000` — `app1`, the
  slot it did *not* come from. **The cost, stated because it is the reason this
  was not done sooner:** with no rollback, a *bad* image in the selected slot is
  unbootable without USB. Before the fix a good update did nothing either.
- **There is no C++ parity baseline, by decision, permanently.**
  `docs/history/baseline/cpp/` holds only `.gitkeep`: capturing one means
  flashing and running the deleted C++ — reconstructible from `9fa8c834`, but
  only with its PlatformIO build reconstructed too — on a powered, wired
  machine, while it owns its own control loop. `just parity` reports
  `BASELINE-MISSING` for all 17 scenarios rather than pretending. **"An
  intentional difference" in `divergences.md` now means reviewed and reasoned,
  never measured.** [§35.1](history/divergences.md#d35).
  See [`baseline/README.md`](./history/baseline/README.md).
- **`/api/ota/url` answers `501`.** The route is registered and says why:
  `cc-hal-esp32/src/web.rs:1917-1934`. The two upload endpoints and
  `/api/ota/status` **are** implemented and stricter than the C++'s — see
  [`divergences.md` [§31](history/divergences.md#d31)](./history/divergences.md). The URL
  route needs an HTTP client and a second long-lived task, for something a
  browser upload already reaches.
- **A brew pressed during the emergency latch fired when the latch cleared.
  Measured 2026-10-07, fixed the same day.** The machine tripped, refused the
  press, then started that same brew on recovery, unprompted — C++ behaviour, and
  a violation of `AG-REPO-24` in this repository. `EMERGENCY_STOP` now drains
  the action requests on entry and on every latched tick, pinned by three tests
  that fail against the pre-fix code. **Verified on hardware
  2026-10-07:** threshold 30 °C on a bench build, probe warmed by hand, the
  machine tripped, the press during the latch did nothing, and **when the probe
  cooled and the latch cleared by itself no brew started** and the heater
  returned. [`divergences.md` §36](history/divergences.md#d36).
- **Four of the six R4-04 safety cases have bench procedures, and all four have
  been run on a bench ESP32** (2026-10-07): 13.1 over-temp trip **passed**,
  13.4 OTA actuator-off **passed**, 13.2 latch **half-passed**, 13.3 tank interlock
  **part-passed**. Two more are machine-only (tank-empty pump *kill*, valve
  fail-safe) and one — the watchdog reboot — cannot be run anywhere without a
  debug route this port lacks. [`operations/runbook.md` §13](operations/runbook.md)
  carries the procedures and the per-case results. **13.1** (over-temp trip) and **13.2** (the latch) need a
  `just bench-flash` build: `safety.emergency_temp` has a 120 °C floor and the
  steam-headroom check refuses a value at or below
  `steam.setpoint + safety.emergency_hysteresis`, so a legal configuration
  cannot reach S1 on a bench whose boiler sits at 23 °C. **Without** that build
  they are not runnable there at all. **13.3** (tank
  interlock) is part-passed on hardware: the request is drained when the
  tank is empty, both the pump **and** the water valve are refused on a
  mid-brew empty tank (`refused pump=1 water=2` — the water one being the
  divergence from the C++), and refilling the tank returns the machine to
  `PID_NORMAL` with no brew left pending. The tick timing of the drop is
  unmeasured. The bench profile it needs is in
  [`hardware/bench-setup.md`](hardware/bench-setup.md). The watchdog reboot needs a debug
  route this port does not have (the oracle's `/debug/hang-supervisor` was not
  ported), and tank-empty pump *kill* and valve fail-safe need the machine,
  because they are about a real float switch, a real pump and a real valve
  de-energised. Owner: Eduard Marbach. [§35.4](history/divergences.md#d35),
  [`outstanding-findings.md` #12–#14](history/outstanding-findings.md).
- **There is no bootloader rollback.**
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` is absent from this build's sdkconfig,
  so a new image that boots and misbehaves stays selected. The C++ behaves
  identically.
- **The steam LED is unwired, by decision.** GPIO1 is the UART provisioning
  console's TX line, which is the documented recovery path for a machine on a
  nonexistent network, and a pin cannot be shared on this HAL. GPIO32 — the C++'s
  own suggested alternative — is `PIN_HXDAT`. Moving it is a hardware change. The
  *rule* is implemented and tested; only the pin is absent.
  [`divergences.md` [§30](history/divergences.md#d30)](./history/divergences.md).
- **The Acaia BLE scale is out of scope.** It was measured and does not fit; it
  needs a decision. Open in [`history/divergences.md`](history/divergences.md#d12)
  (R3-18); the original task is [`archive/migration/06-migration-task-list.md`](archive/migration/06-migration-task-list.md).
  The HX711 **is** implemented, and no scale is fitted to the board.
- **`just flash <port>` now writes both app slots, so it always wins** — the
  defect it fixes (#16) is that it used to write only `app0` while `otadata` kept
  selecting whichever slot the last update chose, so on a board that had taken an
  OTA the flash reported success and the old image kept running. The `app1` offset
  is read from `rust/partitions_4M.csv` rather than hardcoded. Verified with the
  operation that found it: on a board booting `app1`, a bench build flashed over
  USB moved the emergency floor from 120 to 20, where the same flash had left it
  at 120.
- **The OTA was exercised on hardware on 2026-10-06, and it found two defects.**
  The upload path works end to end — the safe shutdown, the slot erase, the
  stream, the 200 — but **`just flash` had never written the partition table**
  (`esp-idf-sys` 0.38.1 does not consume a custom CSV; the build emitted
  ESP-IDF's default table), so the first attempt answered `500 Failed to begin
  update` with `ota: could not open the Firmware slot: ESP_ERR_INVALID_ARG`:
  `esp_ota_get_next_update_partition()` returns NULL on a table with no second
  app slot. Fixed by passing `--partition-table` in `flash-elf`; the device then
  boots `app0`/`app1`/`littlefs`/`coredump` and the upload returns 200. **The
  second defect is also fixed** — see the first bullet in this section.
- **Neither the rotary encoder nor the zero-crossing dimmer is ported.**
  GPIO 4/3/5 and GPIO 18 are declared and unwired in the port. See
  [`hardware/pins.md`](./hardware/pins.md).
- **There is no PlatformIO build.** The C++ firmware it built was deleted with
  the rest of that tree; there is no rollback image in this repository.
- **The Rust release pipeline has never run.** `release.yml` was rewritten to
  publish the Rust image and has not executed on a tag yet, so the release
  artifact path is unverified — including the `espflash save-image --merge`
  step and its 3.5 MB size assertion. Until 2026-10-07 the file also named an
  undefined expression variable (`${{ repository }}`), which GitHub rejects
  outright: every push produced a run with zero jobs and a "No jobs were run"
  mail, so nothing in the pipeline could execute. `actionlint` reports that
  class of error and the push that carried the fix produced no run at all.
  The build it wraps is not unverified:
  `just gate` runs `just build-esp32` and the size budget, and both are green.
  Flashing with `just flash <port>` is the verified path.
- **An admitted OTA skips the probe and re-applies the shutdown until the session ends.** Measured 2026-10-08. [`operations/runbook.md` §13.4](operations/runbook.md).
- **Backflush fill and flush re-assert their pins each tick.** An empty tank still leaves the state. [`divergences.md` §39](history/divergences.md#d39).
- **There is no configuration upgrade path from a C++-flashed machine.** The two
  firmwares use different NVS namespaces (`config`, `defaults.h:13`, against this
  port's `cc`), so nothing is lost and nothing is deleted — the previous
  firmware's settings are still on the chip. They are simply not read, and the
  firmware runs the compiled-in defaults, which carry no SSID. **A first boot
  with an empty `cc` namespace now opens `config` read-only and prints one line
  saying so, naming both namespaces and telling the operator to re-enter the
  SSID and password** (`cc_config::predecessor::startup_notice` for the words,
  `cc_hal_esp32::nvs::probe_predecessor` for the check). A deliberate migration
  was declined: see [`divergences.md` [§32](history/divergences.md#d32)](./history/divergences.md).
  **The re-provisioning itself has now been exercised on hardware** (2026-10-05,
  a bench ESP32): `just wifi-provision` stores the credential, the machine
  reboots onto the network and serves the API. What is still unexercised is the
  *migration* — no C++ `config` namespace was present on that board.
- **The configuration moves by download and re-upload, and the hand-off is
  pinned by a test rather than assumed.** Decided 2026-10-06: an operator
  downloads `config.json` from the C++ UI and uploads it here. It works because
  the export and import key names are the C++'s own dotted names, and
  `every_cxx_config_key_is_still_a_key_the_schema_knows` holds the C++'s 96
  keys — recovered from `getAllConfigParams()` at `9fa8c834:src/Config.cpp:438`
  — and fails on a rename. The procedure is
  [`operations/runbook.md` §12](operations/runbook.md), and it says plainly
  that the downloaded file holds the Wi-Fi and MQTT passwords in cleartext.
  [§35.3](history/divergences.md#d35).

---

## Residual risk — accepted, not closed

These are open by decision or by the limits of the harness. They are not bugs to
be filed and they are not fixed by the next green gate.

1. **Device code is type-checked, not executed.** The `CASES` registry is compiled
   by `just lint-esp32` and runs only under `just test-esp32 <port>` on real
   hardware. Two bugs shipped through that gap.
2. **The parity classification is unmeasured.** Every "intentional difference"
   rests on reading the two codebases, not on running them side by side. With an
   empty baseline, an *undeclared* difference does not fail the harness today.
   And `divergences.md` carries 5 `ledger` blocks against ~30 prose entries,
   so most differences would surface as unexplained even once a baseline exists.
   Treat "intentional" as *reviewed and reasoned*, not *measured*.
3. **`FrameSlot` is verified by review, not by test.** It lives in `cc-firmware`,
   which `cc-hal-esp32` cannot depend on, so the device-test registry cannot
   reach it. Its fix is verified by caller audit and review.
4. **`main.rs`'s weight wiring is verified by review, not by test**, for the same
   reason. The two behaviours it restores are pinned host-side.
5. **Hardware timing has never been measured** — the contactor's minimum on/off
   time and the realised duty on the heater pin.
6. **The water path is enabled in this build and has never been exercised
   against real hardware.** Decided 2026-10-06 by Eduard Marbach: R4-01's
   bring-up inhibit (`TEST_ONLY_INHIBIT`, which held the pump and valve off
   while the heater stayed live) is deleted, because the C++ and the recovered
   oracle both moved water — `recovered-oracle.md:92` records "pump on GPIO27,
   valve on GPIO17, both asserted off" as a *boot* state, and its debug surface
   includes `/debug/brew/start` and `/debug/hotwater/on` (`:209`) — so a build
   that cannot brew is a bring-up artifact, not parity. `Actuators` defaults to
   `Inhibit::NONE`, and the HAL keeps the `Inhibit` type and its device test for
   a future build that wants water held off.
   **The bench half is now measured (2026-10-07, LEDs on GPIO2/17/27):** a brew
   switch press lights the pump LED and the valve LED and drops the heater LED,
   and an OTA upload takes all three dark for the whole write and restores them
   after the reboot — [`operations/runbook.md` §13.4](operations/runbook.md).
   **What is still unexercised is the machine**: the reservoir, the real float
   switch, the real valve. Three new findings came out of the bench session —
   [`history/outstanding-findings.md` #12–#14](history/outstanding-findings.md).
   Finding #12 refuses the write and repairs implicated keys on boot.
   Measured 2026-10-08: unsafe blob repaired `safety.emergency_temp` only;
   all 98 parameters matched the pre-plant snapshot; repeat POST answered `400`.
   [`operations/runbook.md` §13.1](operations/runbook.md).
   **Reversal: reinstate `actuators.set_inhibit` in
   `crates/cc-firmware/src/main.rs` with pump and valve held.**
7. **The TSIC-306 arm of the F1 fix does not latch on total silence.** No TSIC is
   fitted and that arm has never run.
8. **Two pump watchdogs are armed that the C++ leaves inert.** Correct call, and
   a behaviour change: a 5-minute brew the C++ ran indefinitely now stops.
9. **The Wi-Fi telemetry defect was invisible to the test suite.** The device case
   `web.rs:3277` asserts that a control-task publish leaves the radio fields
   alone — but it asserts it about a **helper defined in the test module**, not
   about the call site in `main.rs`, which is where the bug lived. The invariant
   is only observable at `/api/status` on real hardware, and `cc-hal-esp32` cannot
   depend on `cc-firmware` to reach it. Treat every device case that reconstructs
   the production call in a test helper as coverage of the *contract*, not of the
   *code*.
10. **A retired claim, kept because it was wrong twice.** The bench session on
   2026-10-05 recorded a PID-tuning write as a defect in this port. It is not:
   `ProcessController.cpp:170` gates the retune on a **state** change in the
   C++ too, so the behaviour was parity. I checked the oracle before changing
   it, which is the only reason this is item 10 and not a divergence nobody
   noticed. It is now a divergence **on request** — see
   [`divergences.md` [§34](history/divergences.md#d34)](./history/divergences.md) — and it
   is verified only by measurement, because the call site is in `cc-firmware`
   and the device-test registry cannot reach it.
---

## How to keep this true

- **Update this file in the same commit as any behaviour change** — `AG-REPO-20`.
  A behaviour change with a stale status page is a regression in this repository's
  terms, even when every test passes.
- **Never claim a validation you did not run.** If a line says a thing was
  measured on hardware, either it was, or the line says "verified by reading the
  source". There is no third option, and the difference between those two is the
  difference between this page being useful and this page being the next
  confidently-wrong document.
- **Every "what works" line keeps its pointer.** Replace the pointer, never the
  assertion alone: a line whose commit is gone is not evidence of anything.
- **Re-date the top of the file** whenever you touch it, and re-run `just gate`
  before you claim it is green.
- **Rules are not here.** They are in [`AGENTS.md`](../AGENTS.md), numbered.
  This page may not restate one.
- For the pin map and the traps in it, see [`hardware/pins.md`](./hardware/pins.md).
