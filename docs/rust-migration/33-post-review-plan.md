# Post-review work plan — `rewrite/rust`

Master tracker for the work that came out of the 2026-10-03 independent review
([`32-findings-2026-10-03.md`](./32-findings-2026-10-03.md)). One line per item, so a reader
picking this up cold knows what is done, what is next, and why the order is what it is.

**Gate for every item:** `cargo fmt --all` then `just gate`. `just check` is **not** sufficient —
it does not compile `cc-hal-esp32` / `cc-firmware`, and half this work lives there.

**The C++ tree is the parity oracle and is never modified or flashed.** Verified unchanged across
this whole effort with `git diff 2cbeb4d0..HEAD -- src/ include/ lib/ test/ platformio.ini`.

---

## Decisions taken (2026-10-03)

| decision | answer | consequence |
| --- | --- | --- |
| Are the migration docs authoritative? | **No — they are claims to be checked.** Conflicts are surfaced as explicit open items, not silently resolved. | Items 2.3–2.6, 4.2 and 4.4 exist because the code disagrees with `04-target-architecture.md`. |
| Is restructuring allowed? | **Yes, if it buys a testability win.** | Phase 3 (the `cc-web` extraction) is the one restructure that earns its churn. |
| Hardware verification? | **Static + compile + audit.** No device runs; the C++ owns the wired machine. | Device-side code is covered by the on-target `CASES` registry, which is type-checked but not executed. Known residual risk, recorded below. |
| Order of restructure vs features? | **`cc-web` first**, so features land once in the right crate. | Phase 3 before Phase 4. |
| Over-abstraction cleanup? | **All of it.** | Phase 3. |
| Close the C++ parity-baseline gap? | **No.** | All 13 scenarios stay `BASELINE-MISSING`. Every "intentional diff" classification remains **unverified by measurement**. Accepted knowingly — see Residual risk. |
| Acaia BLE scale? | **Not in scope.** | Measured at +205 KB flash / +40 KB static RAM (54 % of the chip's RAM). Needs a product decision, not a fix. |

---

## Phase 1 — safety and data correctness ✅

| # | item | commit |
| --- | --- | --- |
| 1.1 | Dead temperature probe invisible; PID regulated a frozen reading | `3794bef6` |
| 1.2 | `POST /api/setpoint` bypassed the schema bound and persisted it | `ee717438` |
| 2.1 | `Snapshot::get` consumed the value it returned | `44c546db` |
| 2.2 | `brew.by_weight` could never stop a brew | `55f92c5b` |

Three of the four were fixed at a **shared invariant** rather than at the write path, so they cover
every writer (HTTP, MQTT, `/api/parameters`, NVS) rather than the route where the bug was found.

## Phase 2 — cheap correctness 🔄 in progress

| # | item | worker |
| --- | --- | --- |
| 2.3 | Task priorities never implemented; lwIP (prio 18) preempts control | P2a |
| 2.8 | Up to four NVS erase-and-writes inside one 10 ms tick | P2a |
| 2.4 | Pin map duplicated, neither copy validated | P2a |
| 2.5 | S5 water-valve whitelist had one enforcement point | P2b |
| 2.6 | `pid.regular.i_max = 0` silently rejected | P2b |
| 2.7 | MQTT time budget equals the whole control period | P2b |

## Phase 3 — maintainability 🔜

| # | item | note |
| --- | --- | --- |
| 4.3 | Three levels of `dyn` in the safety applier | make generic |
| P2-2 | `Diagnostics` — 11 optional methods, 4 implemented | collapse or delete |
| 4.6 | `ConfigStore` — one impl, never used as a bound | delete |
| 4.2 | Three blocking mutexes in the 10 ms tick | **highest risk**; unverifiable on hardware |

## Phase 4 — `cc-web` extraction 🔜

The one restructure that pays back "test device-independent things fast".
~1,500 lines of pure REST surface (`Telemetry`, `Command`, all `*_json` renderers,
`classify_parameters`, `resolve_ui`, `mime_for`, `routes`) move out of `cc-hal-esp32` into a `no_std`
crate `just test` can reach. **Two real device bugs already shipped through exactly this gap**
(a zero-millisecond Wi-Fi provisioning password window; console lines lost across `esp_restart()`).

Absorbs finding 4.5 (the duplicated `Telemetry` / `network::Reading` pair collapses once
`Telemetry` has one home).

## Phase 5 — feature scope 🔜

| # | item | note |
| --- | --- | --- |
| 3.1 | 3 status LEDs drive no GPIO | 6 schema params already exist and are UI-visible |
| 3.2 | Telnet has a ring buffer but no transport | `HeapShed` policy + ring already exist |
| 3.3 | OTA endpoints answer `501` | **the partition table already has `app0`/`app1`/`ota_0`/`ota_1`/`otadata`** — no partition change needed. `Effect::SafeHardwareShutdown` exists and is unused: it is the S8 hook. |
| 3.4 | `/api/parameter-help` returns a stub with **HTTP 200** | error object returned as success |
| 3.8 | No server-wide 404 handler | C++ returns JSON for `/api/*` |
| 4.7 | `main.rs` is 3,671 lines and unnamed | extract `probe.rs`, `config_io.rs` |

## Phase 6 — documentation 🔜

| # | item |
| --- | --- |
| 2.9 | `tick_allocations.rs` is titled "the control tick must not touch the heap" but proves only the *reducer* |
| 4.4 | `cc-domain` grew from "vocabulary" to "all portable logic"; its "read this in one sitting" claim is stale |
| 7.1 | AGENTS.md/CLAUDE.md say 1,074 host tests; real count is ~1,150 + 168 device cases |
| 7.2 | The migration README's "Not done" list omits LEDs, telnet and `parameter-help` |
| 7.3 | README says `/events` is broken with `Content-Length: 0`; it was fixed and never updated |
| 7.4 | `docs/ci.md`'s warm table says the esp toolchain install is 71 s; its own Caches section says no-op |

---

## Verified sound — do not "fix" these

Recorded so a later pass does not "correct" them back. Full detail in `32-findings` §6.

- **The PID port.** Deliberately reproduces the C++'s `if/else if` clamp chain so `NaN` passes
  through instead of being snapped, and the doc says why `f64::clamp` would be wrong. The
  `SampleTime / 1000` integer-division trap is closed with a real-time divisor.
- **The heater path.** `HeaterGate` has no armed constructor; gating runs last, after quantisation.
  `AtomicChopper::tick` is integer-only — a hard Xtensa requirement, since an FPU instruction in a
  level-1 ISR is a fatal coprocessor exception — proven equivalent to the `f32` reference over
  1001 × 103 inputs.
- **The portable/device boundary.** Enforced by `scripts/portable-purity.py`, zero leaks in code.
- **Flag drain hygiene**, **effect overflow counted not panicked** (`dropped() == 0` across all
  4,140 state × event pairs), **boot readback asserting every pin inactive**.
- **Six places the code beat its own plan**: deleted `TemperatureProbe` trait, no `Clock` trait,
  `heapless` `Effects`, deleted `cc-provisioning`, the two harness crates, and the wildcard-arm-free
  `water_flow_allowed` / `steam_flow_allowed` matches.
- **The stack choice.** ESP-IDF Rust, not bare-metal `esp-hal` — the latter's WiFi/BLE on the
  original ESP32 is still behind an `unstable` feature and has no NVS, HTTP or MQTT story.

---

## Residual risk (accepted, not closed)

1. **No C++ parity baseline.** Every "intentional diff" is classified by reading code, not by
   measurement. Accepted by decision on 2026-10-03.
2. **Device code is type-checked, not executed.** The `CASES` registry is compiled by
   `just lint-esp32` and run only by `just test-esp32 <port>` on real hardware. Two bugs shipped
   through that gap before `cc-web` existed.
3. **`FrameSlot` is untestable by the registry.** It lives in `cc-firmware`, which
   `cc-hal-esp32` cannot depend on. Its F3 fix is verified by caller audit and review, not by a test.
4. **`main.rs`'s weight wiring is unverified by test** for the same reason; the two behaviours it
   restores are pinned host-side.
5. **The TSIC-306 arm of the F1 fix does not latch on total silence.** `no_signal()` does not count
   as a failure (`tsic306/mod.rs:570`), so a fully silent TSIC line would still be invisible. No TSIC
   is fitted and that arm has never run; not widened in scope.
6. **Hardware timing never measured** — the contactor's minimum on/off time and the realised duty on
   the heater pin (R1-07's open procedure).
7. **The port arms two pump watchdogs the C++ leaves inert.** Correct call, but it is a behaviour
   change: a 5-minute brew the C++ would have run indefinitely now stops.
8. **`TEST_ONLY_INHIBIT` holds pump and valve off in this build**, so the water path has never been
   exercised against real hardware. The heater is live.
