# Status

**Dated 2026-10-21. Owner: Eduard Marbach** (`mail@eduard-marbach.de`), who also
owns the wired machine. Re-verify with `git log --oneline -1` and `just gate`
before trusting a line below.

**The port is closed out as of 2026-10-06.** Five items that were open are now
settled decisions — the C++ comparison baseline, the water-path inhibit, the
configuration hand-off, the R4-04 procedures and the OTA surface. Each carries
its own entry below with a date, a named owner, the reasoning and its reversal
condition. The four claim
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

## PR review corrections — 2026-10-25

Owner: Codex. The still-applicable findings in
[PR #124](https://github.com/BlackDark/fork-clevercoffee/pull/124) are covered by
[Chart.test.ts](../ui/packages/frontend/src/components/Chart.test.ts),
[ParameterInput.test.ts](../ui/packages/frontend/src/components/ParameterInput.test.ts),
[parameter-save.test.ts](../ui/packages/frontend/src/lib/parameter-save.test.ts),
and [frontend.spec.ts](../ui/packages/frontend/e2e/frontend.spec.ts).
The regressions cover single chart construction and caught theme-rebuild errors,
empty numeric edits, visible accessible labels, absent numeric bounds, OTA file
selection resets, the 8 KB restore boundary, and useful HTTP/upload errors.

Local `pnpm --dir ui check`, the TypeScript 6 compatibility command, and the
mock-backed Playwright suite passed. Stable `just check`, device lint/rustdoc,
the release build, and the size check passed. The built image measured
1,691,744 B against a 1,835,008 B slot. This review follow-up was not flashed;
the hardware observations below belong to the earlier committed build.

## Frontend migration follow-up — 2026-10-25

Owner: Codex. Validation and hardware observations in this section were
recorded for [574534fc](https://github.com/BlackDark/fork-clevercoffee/commit/574534fc).
Official shadcn-svelte registry sources were imported with CLI
1.7.1 (Nova style); [components.json](../ui/packages/frontend/components.json)
records their configuration. Pages use the registry Button, Input, Label, Card,
Badge, Alert, Progress, Tabs, Dialog, Select, Switch, Tooltip, and Sonner
components. The custom toast store and control CSS were removed; custom CSS is
limited to application layout, typography, and theme accents. uPlot still owns
the graphs. AG-UI-1 records the component preference.

The added packages supply the registry's class merging/variants, animation
styles, shared state styles, and Sonner notifications. Bits UI was updated to
2.20.2 for the current registry; new dependencies are exactly pinned in
[package.json](../ui/packages/frontend/package.json).

`pnpm --dir ui check`, the production build, and the mock-backed Playwright
suite passed. The draft-navigation regression also passed repeated runs.
[frontend.spec.ts](../ui/packages/frontend/e2e/frontend.spec.ts) covers visible
checked switches, enum labels after repeated pointer/keyboard edits and route
changes, zero values, tooltip focus, secret masking, notifications, mobile
navigation, card placement, and the device's HTTP 202 counter-reset contract.
Local browser inspection covered the dashboard and System page; the registry's
shared Tailwind state styles are imported so checked/active states render.
The subsequent layout regression verifies direct-child warning icons beside
the description on desktop/mobile, icons on all System action buttons, and
shared main-container bounds for Home, System, and About. About's card uses
the full content width. System/About were also visually inspected locally. The final System layout
was also inspected in a fresh device-served tab after flashing.

Stable `just check` and the `esp` device lint, rustdoc, release build, and size
check passed. The release image measured 1,691,632 B against a 1,835,008 B slot.

Both app slots were flashed on bench ESP32 `ec:62:60:76:b5:3c`. `/ui/` and its
JS/CSS assets returned 200 and matched the tested local build byte-for-byte.
Status reported state 20, PID enabled, temperature 20.12 °C, and tank full; all
six bench-profile settings persisted. A fresh device tab rendered live
telemetry and the registry controls; Reset counter produced Sonner's success
notification without a schema error. The user's existing tab/draft was left
untouched.

## Frontend layout correction — 2026-10-21

Owner: Codex. Implementation and validation record: [9e747c04](https://github.com/BlackDark/fork-clevercoffee/commit/9e747c04).
Restored the original neutral light/dark palette, logo, header
navigation, two-panel machine overview, and cyan/purple/amber uPlot colors.
Navigation collapses below 768px into a burger menu; route selection, Escape,
and outside clicks close it. The theme follows the system until a selection is
saved. No sidebar remains.

`pnpm --dir ui check`, the production build, TypeScript 6 compatibility check,
and the mock-backed Playwright suite passed. The navigation regression in
[frontend.spec.ts](../ui/packages/frontend/e2e/frontend.spec.ts) covers collapse,
route selection, Escape focus return, and desktop header placement. Refreshed
screenshots are linked from the [README](../README.md). The clean production
asset set totals 105,685 bytes (95,083 bytes gzip JavaScript), measured by summing
`dist` files and verifying each gzip archive. Local browser execution
used the Chromium executable override described below. No firmware or hardware
verification was performed.

---

## Frontend migration — 2026-10-20

Owner: Codex. Frontend-only verification against the bundled mock API:
`pnpm --dir ui check`, the frontend production build, and
`pnpm --dir ui --filter @clevercoffee/frontend test:e2e` passed. The browser
suite is [frontend.spec.ts](../ui/packages/frontend/e2e/frontend.spec.ts).
Local browser execution used Chromium 153 through
`PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` because the Playwright browser download
returned an invalid archive in the execution environment.

The embedded asset set measures 106,363 bytes versus 211,182 bytes for the
React baseline built from `fc3efdc6`: gzip HTML/JS/CSS plus `logo.png`, measured
by summing the files in `dist`. Gzip files were also decompressed to verify the
archives. Screenshots are linked from the [README](../README.md).
The tooling follow-up retains TypeScript 6.0.3 for Svelte's compiler API and
uses TypeScript 7.0.2 through the `@typescript/native` alias and `--tsgo`.
The native check, TypeScript 6 fallback, frontend check, and production build
passed. A temporary invalid Svelte prop was rejected by the native checker and
the probe was removed. Each native check clears `.svelte-check` to prevent
stale diagnostics for deleted components. The packaged asset size is unchanged.

The navigation E2E checks also pass with explicit destination-screen waits and
category-scoped links, covering the navigation timing difference observed in
GitHub CI.

The Rust gate, firmware build, and physical-device behavior were not tested;
the user explicitly scoped this migration to the frontend.

---

## What works today

**The Rust firmware boots, regulates and serves.** Recorded 2026-09-30 and
2026-10-01 on the board. Open work is [`attention.md`](attention.md).

- **The state machine, the PID and brewing are on the device.** R4-01, `4c4e072`.
  The reducer is wired into the 10 ms control task
  (`crates/cc-firmware/src/control.rs`) and effects are applied through
  `cc-hal-esp32/src/actuators.rs` in the same tick. At a 30 °C target with a
  ~7 K error the duty settles at ~48 %; at 95 °C with a 72 K error it goes to
  100 %. Both measured on the board, reproducible from the web UI.
- **Operator switches drive the pins.** 2026-10-09, bench LEDs on GPIO 2, 17 and 27. The LED is the pin.
  Brew held: state 33, heater duty 0, pump and valve LEDs on. Release: state 20, heater duty 100 %.
  Hot water: state 20, pump LED on, valve LED off.
  Steam: state 51, heater duty 100 %, pump and valve LEDs off, steam screen on the panel.
  Power toggle: open is state 95, heater duty 0. Closed is on. Momentary, and a boot with the toggle off, were not in this pass. `power.enabled` set back to false. Reboot returned to state 20.
- **The display renders every frame, and `failed=0`.** `present=true`, frames
  tick over on schedule. The SSD1306 shares the I²C bus with the ABP2 behind a
  `Mutex` and the frame is chunked into 8 bus writes, not 64, so the pressure
  sensor is not starved.

  **Layout.** Findings #3–#6 (clipped EEPROM, German sensor, OTA title, 1 px
  row overlap) are fixed in host tests, not on the machine.
- **The display is checked by 48 goldens and three measured tables.** Each
  template is rendered and compared against a committed P4 image, and every glyph
  is checked against advances measured from real U8g2 2.36.18
  (`tests/support/glyph_widths.rs`, `string_widths.rs`, `fmt_oracle.txt`). The
  host comparison against a live U8g2 went with the C++ tree; what it covered, and
  what is no longer checked, is [`display/parity.md`](display/parity.md).
- **The documentation has one map, one architecture page and one glossary.**
  `docs/index.md` is exhaustive (`AG-REPO-28`): `docs/api/openapi.yaml`
  had been missing from it for the whole port, which is how a 759-line orphan
  survived. `docs/architecture.md` answers "what is this thing" in one page,
  `GLOSSARY.md` pins the repository's vocabulary, and
  `scripts/check-doc-links.py` resolves **heading anchors** as well as files, and
  **backticked paths in prose**, which markdown link syntax cannot see — that is
  how a live skill note went on citing a deleted script with every gate green.
- **The API spec is now checked against the routes.** `scripts/check-openapi.py`
  diffs `docs/api/openapi.yaml` against the `ROUTES` table in
  `cc-net-esp32/src/web.rs` in both directions and runs in `just check` and CI
  (`AG-REPO-31`). The spec had drifted: it covered 24 of 28 routes and omitted
  `/api/sleep`, `/api/wake` and `/events`, which are exactly the three carrying
  deliberate behaviour differences. All 28 are now present.
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
  `crates/cc-alloc-count`, which is what the numbers above come from.
- **98 parameters are writable over HTTP and survive a reboot.**
  `cc_config::schema::PARAM_COUNT` is 98 (`schema.rs:92`) and pinned by
  `assert_eq!(SCHEMA.len(), PARAM_COUNT)` (`schema.rs:1858`), by
  `crates/cc-config/tests/config_schema.rs:43` and by
  `crates/cc-net/src/web/help/tests.rs:91`.
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
  `cc_net::web::telnet::tests::the_predecessor_boot_lines_are_not_truncated_on_the_wire`,
  against the real formatter rather than a copied byte count.
  **Verified by test and by reading the code; the line has not been seen on a
  board.**
- **The web UI is served from flash and renders.** `cc-net-esp32/build.rs` embeds
  the gzip bundle with `include_bytes!`, which is why a 199,270 B bundle costs
  0 B of RAM. A deep link to a client-side route (`/ui/config/behavior`) boots the
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
- **Wi-Fi provisioning over the UART console is exercised.** `just
  wifi-provision /dev/cu.usbserial-224140` stores the credential from `.env`,
  the machine reboots, associates at `10.0.1.168` and serves the API. The
  script also accepts the reset as confirmation, because the firmware's
  `credential from the console was stored` line does not reach UART0 — see the
  open item below.
- **The gate is green.** `just gate`: fmt-check, clippy (host and
  device) with `-D warnings`, rustdoc `-D warnings`, the host suite, the scenario
  tests, the device-test audit, the Xtensa release build, and the size budget.
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
  (`213be5ee`, 249 files). That the deletion is what changed it is checkable:
  `git diff --stat 2006b710..HEAD -- src/ include/ lib/ platformio.ini
  partitions_4M.csv` reports **153 files changed, 29,023 deletions**, and every
  one of them is the deletion.

---

## What is explicitly NOT done

Accurate as of the date above, and each taken from code or a findings document
rather than from memory. **The four states are defined in
[`GLOSSARY.md`](../GLOSSARY.md) under "How a claim ends"** — done, closed by
decision, residual risk, not started. The close-out decisions of 2026-10-06 are
recorded below.

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
- **There is no C++ comparison baseline, by decision, permanently.** Capturing
  one means flashing and running the deleted C++ — reconstructible from
  `213be5ee^`, but only with its PlatformIO build reconstructed too — on a
  powered, wired machine, while it owns its own control loop. The comparison
  harness that would have used it was removed with the rest of the C++ tooling.
  **An intentional difference means reviewed and reasoned, never measured.**
- **`/api/ota/url` answers `501`.** The route is registered and says why, in
  `Web::start` (`cc-net-esp32/src/web.rs`). The two upload endpoints and
  `/api/ota/status` **are** implemented and stricter than the C++'s. The URL
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
  returned.
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
  route this port does not have (the recovered firmware's `/debug/hang-supervisor`
  was not ported). Tank-empty pump *kill* and valve fail-safe are about a real
  float, a real pump and a real valve. Not an agent task. Owner: Eduard Marbach.
- **There is no bootloader rollback.**
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` is absent from this build's sdkconfig,
  so a new image that boots and misbehaves stays selected. The C++ behaves
  identically.
- **The steam LED is unwired.** GPIO1 is the UART console TX. GPIO32 is the scale data line. Whether another pin is worth a solder joint is Potential. [`attention.md`](attention.md).
- **The Acaia BLE scale is out of scope.** It was measured and does not fit; it
  needs a decision (R3-18).
  The HX711 **is** implemented, and no scale is fitted to the board. Since
  `2ad8d15` a calibration request reads `hardware.sensors.scale.known_weight`
  live rather than the boot-time value; unverified on hardware for that reason.
- **A Wi-Fi bring-up error no longer aborts boot.** An `EspError` from
  `Sta::new` or `connect` used to end `bring_up` before HTTP, telnet, the
  provisioning console and the control task started; it now means offline mode,
  as an association timeout already did. Not exercised on hardware: no fault was
  injected.
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
- **The Rust release pipeline ran.** `v2.0.0-alpha.1` on `b2151ecd`, run `37925778055`. `firmware.bin` is 4,194,304 bytes. That file has not been flashed from the release asset onto the board. `just flash <port>` is the path that has.
- **An admitted OTA skips the probe and re-applies the shutdown until the session ends.** Measured 2026-10-08. [`operations/runbook.md` §13.4](operations/runbook.md).
- **Backflush fill and flush re-assert their pins each tick.** An empty tank still leaves the state.
- **There is no configuration upgrade path from a C++-flashed machine.** The two
  firmwares use different NVS namespaces (`config`, `defaults.h:13`, against this
  port's `cc`), so nothing is lost and nothing is deleted — the previous
  firmware's settings are still on the chip. They are simply not read, and the
  firmware runs the compiled-in defaults, which carry no SSID. **A first boot
  with an empty `cc` namespace now opens `config` read-only and prints one line
  saying so, naming both namespaces and telling the operator to re-enter the
  SSID and password** (`cc_config::predecessor::startup_notice` for the words,
  `cc_hal_esp32::nvs::probe_predecessor` for the check). A deliberate migration
  was declined.
  **The re-provisioning itself has now been exercised on hardware** (2026-10-05,
  a bench ESP32): `just wifi-provision` stores the credential, the machine
  reboots onto the network and serves the API. What is still unexercised is the
  *migration* — no C++ `config` namespace was present on that board.
- **The configuration moves by download and re-upload, and the hand-off is
  pinned by a test rather than assumed.** Decided 2026-10-06: an operator
  downloads `config.json` from the C++ UI and uploads it here. It works because
  the export and import key names are the C++'s own dotted names, and
  `every_cxx_config_key_is_still_a_key_the_schema_knows` holds the C++'s 96
  keys — recovered from `getAllConfigParams()` at `213be5ee^:src/Config.cpp:438`
  — and fails on a rename. The procedure is
  [`operations/runbook.md` §12](operations/runbook.md), and it says plainly
  that the downloaded file holds the Wi-Fi and MQTT passwords in cleartext.

---

## Residual risk — accepted, not closed

These are open by decision or by the limits of the harness. They are not bugs to
be filed and they are not fixed by the next green gate.

1. **Device code is type-checked, not executed.** The `CASES` registry is compiled
   by `just lint-esp32` and runs only under `just test-esp32 <port>` on real
   hardware. Two bugs shipped through that gap.
2. **Differences are documented, not measured.** Every intentional difference
   rests on reading the old code and the new code, not on running them side by
   side. The comparison harness and its baseline are gone; `cc-parity` now runs
   the Rust reducer against the scenario assertions. An *undeclared* difference
   therefore does not fail anything today. Treat "intentional" as *reviewed and
   reasoned*, not *measured*.
3. **`FrameSlot` is verified by review, not by test.** It lives in `cc-firmware`,
   which the device-test registries cannot reach. Its fix is verified by caller audit and review.
4. **`main.rs`'s weight wiring is verified by review, not by test**, for the same
   reason. The two behaviours it restores are pinned host-side.
5. **Hardware timing has never been measured** — the contactor's minimum on/off
   time and the realised duty on the heater pin.
6. **The water path is enabled in this build and has never been exercised
   against real hardware.** Decided 2026-10-06 by Eduard Marbach: R4-01's
   bring-up inhibit (`TEST_ONLY_INHIBIT`, which held the pump and valve off
   while the heater stayed live) is deleted, because the C++ and the recovered
   firmware both moved water: the recovered boot state has "pump on GPIO27,
   valve on GPIO17, both asserted off", and its debug surface includes
   `/debug/brew/start` and `/debug/hotwater/on` — so a build
   that cannot brew is a bring-up artifact, not parity. `Actuators` defaults to
   `Inhibit::NONE`, and the HAL keeps the `Inhibit` type and its device test for
   a future build that wants water held off.
   **The bench half is now measured (2026-10-07, LEDs on GPIO2/17/27):** a brew
   switch press lights the pump LED and the valve LED and drops the heater LED,
   and an OTA upload takes all three dark for the whole write and restores them
   after the reboot — [`operations/runbook.md` §13.4](operations/runbook.md).
   **2026-10-09 the pin is the accepted proof** (the bullet above). A reservoir,
   a float and a valve coil are not an open task. Three findings came out of the 2026-10-07 bench.
   Finding #12 refuses the write and repairs implicated keys on boot.
   Measured 2026-10-08: unsafe blob repaired `safety.emergency_temp` only;
   all 98 parameters matched the pre-plant snapshot; repeat POST answered `400`.
   [`operations/runbook.md` §13.1](operations/runbook.md).
   **Reversal: reinstate `actuators.set_inhibit` in
   `crates/cc-firmware/src/bring_up.rs` with pump and valve held.**
7. **The TSIC-306 arm of the F1 fix does not latch on total silence.** No TSIC is
   fitted and that arm has never run.
8. **Two pump watchdogs are armed that the C++ leaves inert.** Correct call, and
   a behaviour change: a 5-minute brew the C++ ran indefinitely now stops.
9. **The Wi-Fi telemetry defect was invisible to the test suite.** The device case
   `web::the_radio_fields_survive_a_machine_publish` asserts that a control-task publish leaves the radio fields
   alone — but it asserts it about a **helper defined in the test module**, not
   about the call site in `main.rs`, which is where the bug lived. The invariant
   is only observable at `/api/status` on real hardware, and `cc-net-esp32` cannot
   depend on `cc-firmware` to reach it. Treat every device case that reconstructs
   the production call in a test helper as coverage of the *contract*, not of the
   *code*.
10. **A retired claim, kept because it was wrong twice.** The bench session on
   2026-10-05 recorded a PID-tuning write as a defect in this port. It is not:
   `ProcessController.cpp:170` gates the retune on a **state** change in the
   C++ too, so the behaviour was parity. I checked the recovered firmware before changing
   it, which is the only reason this is item 10 and not a divergence nobody
   noticed. It is now a divergence **on request**, and it
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
