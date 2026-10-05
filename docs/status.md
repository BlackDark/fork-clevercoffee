# Status

**Dated 2026-10-05. Owner: Eduard Marbach** (`mail@eduard-marbach.de`), who also
owns the wired machine. Re-verify with `git log --oneline -1` and `just gate`
before trusting a line below.

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
["Where the migration actually is"](./rust-migration/README.md#where-the-migration-actually-is).

- **The state machine, the PID and brewing are on the device.** R4-01, `4c4e072`.
  The reducer is wired into the 10 ms control task
  (`crates/cc-firmware/src/control.rs`) and effects are applied through
  `cc-hal-esp32/src/actuators.rs` in the same tick. At a 30 °C target with a
  ~7 K error the duty settles at ~48 %; at 95 °C with a 72 K error it goes to
  100 %. Both measured on the board, reproducible from the web UI.
- **The display runs and does not clip.** `present=true`, 125 frames per 60 s,
  `failed=0`. The SSD1306 shares the I²C bus with the ABP2 behind a `Mutex` and
  the frame is chunked into 8 bus writes, not 64, so the pressure sensor is not
  starved.
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
  board.** [`intentional-diffs.md` §29](./rust-migration/intentional-diffs.md).
- **The web UI is served from flash and renders.** `cc-hal-esp32/build.rs` embeds
  the gzip bundle with `include_bytes!`, which is why a 199,270 B bundle costs
  0 B of RAM. A deep link to a client-side route (`/ui/config/behavior`) boots the
  configuration page with 104 live parameters, verified in Chrome on the device.
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
  original ESP32 rev 3.0: **111 passed, 0 failed, 1 lost, 0 hung**, firmware
  restored afterwards. Three cases failed on the first run and all three were
  defects in the cases, not in the firmware — two table-wide invariants in
  `web.rs` that the deliberate `/api*` preflight wildcard trips, and an OTA
  session test that claimed a second session without releasing the first.
- **Wi-Fi provisioning over the UART console is exercised.** `just
  wifi-provision /dev/cu.usbserial-224140` stores the credential from `.env`,
  the machine reboots, associates at `10.0.1.168` and serves the API. The
  script also accepts the reset as confirmation, because the firmware's
  `credential from the console was stored` line does not reach UART0 — see the
  open item below.
- **The gate is green.** `just gate`: fmt-check, clippy (host and
  device) with `-D warnings`, rustdoc `-D warnings`, the host suite, the parity
  harness, the device-test audit, the Xtensa release build, and the size budget.
  **1,699,872 B**, which fits the 1,835,008 B app0 slot with +135,136 B to spare
  and is **+9.00 %** against `size-baseline.json`, inside the 10 % limit.
  Re-measured 2026-10-05.
- **The two firmware trees have not diverged by accident.** Zero lines changed in
  `src/`, `include/`, `lib/`, `platformio.ini` or the root `partitions_4M.csv`
  since the branch point `2006b710`. That is checkable:
  `git diff --stat 2006b710..HEAD -- src/ include/ lib/ platformio.ini partitions_4M.csv`
  prints nothing.
- **Every deliberate difference from the C++ is written down.**
  [`34-known-differences.md`](./handbook/differences.md) is the
  one-page index; [`intentional-diffs.md`](./rust-migration/intentional-diffs.md)
  is the detail, and 5 machine-readable `ledger` blocks are what `just parity`
  classifies against.

---

## What is explicitly NOT done

Accurate as of the date above, and each taken from code or a findings document
rather than from memory.

- **There is no C++ parity baseline.** `docs/rust-migration/baseline/cpp/` holds
  only `.gitkeep`, by decision: capturing one means flashing and running the C++,
  which owns its own control loop on a powered, wired machine. `just parity`
  reports `BASELINE-MISSING` for all 17 scenarios rather than pretending. See
  [`baseline/README.md`](./rust-migration/baseline/README.md).
- **`/api/ota/url` answers `501`.** The route is registered and says why:
  `cc-hal-esp32/src/web.rs:1917-1934`. The two upload endpoints and
  `/api/ota/status` **are** implemented and stricter than the C++'s — see
  [`intentional-diffs.md` §28](./rust-migration/intentional-diffs.md). The URL
  route needs an HTTP client and a second long-lived task, for something a
  browser upload already reaches.
- **There is no bootloader rollback.**
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` is absent from this build's sdkconfig,
  so a new image that boots and misbehaves stays selected. The C++ behaves
  identically.
- **The steam LED is unwired, by decision.** GPIO1 is the UART provisioning
  console's TX line, which is the documented recovery path for a machine on a
  nonexistent network, and a pin cannot be shared on this HAL. GPIO32 — the C++'s
  own suggested alternative — is `PIN_HXDAT`. Moving it is a hardware change. The
  *rule* is implemented and tested; only the pin is absent.
  [`intentional-diffs.md` §27](./rust-migration/intentional-diffs.md).
- **The Acaia BLE scale is out of scope.** It was measured and does not fit; it
  needs a decision. [`06-migration-task-list.md` R3-18](./archive/migration/06-migration-task-list.md).
  The HX711 **is** implemented, and no scale is fitted to the board.
- **The OTA has not been exercised on hardware.** Verified by reading ESP-IDF
  v5.5.5, not on a board, and the bootloader's fallback-to-factory behaviour on a
  power cut during the `otadata` write was **not** verified.
- **Neither the rotary encoder nor the zero-crossing dimmer is ported.**
  `PIN_ROTARY_DT`/`_CLK`/`_SW` and `PIN_ZC` are declared in
  `pinmapping.h` and unwired in the port. See [`cpp-oracle.md`](./cpp-oracle.md).
- **The PlatformIO build is deprecated but deliberately kept**, for one release
  cycle, as a rollback path (task R4-10).
- **There is no configuration upgrade path from a C++-flashed machine.** The two
  firmwares use different NVS namespaces (`config`, `defaults.h:13`, against this
  port's `cc`), so nothing is lost and nothing is deleted — the previous
  firmware's settings are still on the chip. They are simply not read, and the
  firmware runs the compiled-in defaults, which carry no SSID. **A first boot
  with an empty `cc` namespace now opens `config` read-only and prints one line
  saying so, naming both namespaces and telling the operator to re-enter the
  SSID and password** (`cc_config::predecessor::startup_notice` for the words,
  `cc_hal_esp32::nvs::probe_predecessor` for the check). A deliberate migration
  was declined: see [`intentional-diffs.md` §29](./rust-migration/intentional-diffs.md).
  **The re-provisioning itself has now been exercised on hardware** (2026-10-05,
  a bench ESP32): `just wifi-provision` stores the credential, the machine
  reboots onto the network and serves the API. What is still unexercised is the
  *migration* — no C++ `config` namespace was present on that board.

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
   And `intentional-diffs.md` carries 5 `ledger` blocks against ~30 prose entries,
   so most differences would surface as unexplained even once a baseline exists.
   Treat "intentional" as *reviewed and reasoned*, not *measured*.
3. **`FrameSlot` is verified by review, not by test.** It lives in `cc-firmware`,
   which `cc-hal-esp32` cannot depend on, so the device-test registry cannot
   reach it. Its fix is verified by caller audit and review.
4. **`main.rs`'s weight wiring is verified by review, not by test**, for the same
   reason. The two behaviours it restores are pinned host-side.
5. **Hardware timing has never been measured** — the contactor's minimum on/off
   time and the realised duty on the heater pin.
6. **`TEST_ONLY_INHIBIT` holds the pump and valve off in this build**
   (`crates/cc-firmware/src/main.rs:1613`), so the water path has never been
   exercised against real hardware. The heater is live. Returning to a machine
   that can brew is a one-line change, gated on R4-04's safety-path procedures.
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
10. **Two open defects found on the bench 2026-10-05, both unfixed** and recorded
   in [`integration-checklist.md`](./operations/integration-checklist.md):
   an unknown `/api/...` path answers ESP-IDF's `405` in `text/html` instead of
   the JSON `404` the C++ returns, because the `/api*` preflight wildcard makes
   ESP-IDF treat the URI as matched-but-wrong-method; and a `POST
   /api/parameters` that changes a PID tuning is persisted and reported back but
   does not reach the running PID until the machine changes state.

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
- For the C++ tree's frozen status, see [`cpp-oracle.md`](./cpp-oracle.md).
