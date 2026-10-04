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

## Phase 2 — cheap correctness ✅

| # | item | commit |
| --- | --- | --- |
| 2.3 | Task priorities never implemented; lwIP (prio 18) preempts control | `b195edd0` |
| 2.8 | Up to four NVS erase-and-writes inside one 10 ms tick | `b195edd0` |
| 2.4 | Pin map duplicated, neither copy validated | `b195edd0` |
| 2.5 | S5 water-valve whitelist had one enforcement point | `8553c220` |
| 2.6 | `pid.regular.i_max = 0` silently rejected | `8553c220` |
| 2.7 | MQTT time budget equals the whole control period | `8553c220` |

Two of these did not go the way the finding anticipated, and both were right:

- **2.4** was solved better than specified. Instead of making `main.rs` derive its pins from a
  `PinMap`, the tree now has one `cc-hal-esp32/src/pins.rs` plus an `assert_wiring(&peripherals)`
  checking all 16 constants against the pins `main.rs` actually wired. That is a **runtime**
  assertion rather than a `const` one, because a `const` assertion cannot inspect a `Peripherals` —
  so the wiring stays readable where it is and cannot silently disagree.
- **2.5** exposed **finding 8.2**: the missing interlock check had been *hiding* an ordering bug.
  `actuators.set_state(control.state())` ran before `Control::tick`, so the facade cached the state
  the tick was **leaving**. Fixing 2.5 on its own would have made `may_open_water` refuse the
  `OpenWaterValve` that `BrewPreinfusionState::onEntryImpl` emits on the very tick the machine
  enters `BREW_PREINFUSION` — a 10 ms delay and a spurious refusal at every brew start. The safety
  fix was only safe because the ordering bug was found and fixed alongside it.

## Phase 3 — maintainability 🔜

| # | item | note |
| --- | --- | --- |
| 4.3 | Three levels of `dyn` in the safety applier | ✅ **done** — generic + `?Sized`; `.flash.text` −280 B |
| P2-2 | `Diagnostics` — 11 optional methods, 4 implemented | ✅ **done** — 11 → **5**, not deleted |
| 4.6 | `ConfigStore` — one impl, never used as a bound | ✅ **done** — trait and `MemStore` deleted; methods are inherent on `BlobConfigStore<B>` |
| 4.2 | Three blocking mutexes in the 10 ms tick | 🔄 **in progress** — highest risk, unverifiable on hardware |
| 8.1 | `on_log` unimplemented, so a pump-watchdog trip logged nothing | ✅ `f4a6341b` |

Also in this phase: finding **8.3** — the obvious fix for 2.6 (raise the `i_max` schema floor above 0)
was **ruled out rather than declined**, because Home Assistant's `aggIMax` entity publishes that
bound (C++ `MQTTManager.cpp:869` → `discovery::bounds` reads `spec.min`). A floor above 0 would
diverge from the C++ *and* from this firmware's verified MQTT discovery surface.

**On P2-2.** The trait was **not** deleted, because the collapse leaves five
genuinely-distinct responsibilities, not fewer than three. What went were the six
methods no implementation anywhere had ever written
(`on_clear_action_requests`, `on_clear_stale_stop_requests`,
`on_reset_standby_timer`, `on_reset_mqtt_reconnect_count`, `on_wake_display`, and
the proposed `on_clear_*`-as-mandatory move). Their five `Effect`s still exist and
still reach `apply`, where they are now one documented no-op arm instead of five
default-bodied trait methods that read as "an implementor may do this" and in
practice never did. What stayed is exactly what an implementation exists for: the
two state-transition log lines, the two flag mirrors, and `on_log` — which
**nothing** implements, so it is filed as finding 8.1 rather than deleted, because
`intentional-diffs.md` §1 is a claim about a log line that the firmware does not
currently emit.

The proposed reshape ("move the five flag mirrors into `MachineChannels` as
mandatory methods") was **not** done, and the module docs at `applier.rs:29-47`
are the reason: they state the dividing line as *whether the reducer has already
applied the change to `Machine`*, and every flag mirror is by definition already
applied. Moving them to the mandatory half would put log lines on the side of the
split whose entire justification is that a missing implementation loses a
*change*, and would re-create exactly the "silent `{}` bodies" shape that
`on_record_brew` shipped. It would also have made every one of the four test
doubles implement three no-ops it has no reason to have.

**On 4.6.** The trait went, and so did the test fake that existed to implement
it. `MemStore` stored a `Config` in an `Option<Config>` rather than bytes, so
its seven callers in `tests/config_schema.rs` asserted that *the fake* worked;
six had a twin in `src/blob_store.rs` that drives the real `BlobConfigStore` over
`MemoryBackend`, which is a fake of the *medium* and therefore exercises the
*format*. `BlobBackend` was already the seam that gave the substitution, so
nothing was lost but the indirection. `reset_to_defaults` was a provided method
whose only caller in the workspace was its own test — `Command::FactoryReset` is
still an unwired stub (`cc-firmware/src/main.rs:2669`, R3-16), so it had no
production caller and did not become an inherent method.

## Phase 4 — `cc-web` extraction ✅

The one restructure that pays back "test device-independent things fast". **Done** in `53c99df2`.

| | |
| --- | --- |
| crate | `cc-web`, `#![no_std]` + `alloc`, depends on `cc-domain` + `cc-config` (+ `heapless`, `log`, both already in the graph) |
| moved | `Telemetry`, `Command`, `Auth`, every `*_json` renderer that is actually pure, `mime_for`, `ParameterPost`, `classify_parameters`, `needs_reboot`, three request limits, five request-parsing helpers |
| stayed | route registration, `respond*`, `Sse` + broadcaster + `web_async`, `Shared`/`Snapshot`, `routes()`, `resolve_ui`/bundle, `history_json`, the httpd `Configuration` |
| **result** | **host tests 1,075 → 1,124 (+49); device cases 169 → 123.** `web.rs` 4,669 → 2,893 lines |

It is in `default-members`, in the justfile's `host_crates`, **and in `scripts/portable-purity.py`'s
`PORTABLE` list** — the last one is the load-bearing part, since a purity list that did not name the
new crate would leave the boundary unenforced.

**Three claims in the plan were wrong, and the worker said so rather than forcing them:**

1. "Every `*_json` renderer is a pure function of a Config and a snapshot" — false for two.
   `status_json` and `nvs_debug_json` interpolate `free_heap()` via FFI. They now take the readings
   as arguments; the bytes emitted are unchanged.
2. "Depending only on `cc-domain` + `cc-config`" — `Telemetry::ip` must stay a
   `heapless::String<15>` (that *is* the `unsafe impl Sync` argument), and `Auth::from_config`
   reproduces the C++'s loud warning, so `heapless` and `log` were needed.
3. `history_json` is not pure — it takes `&Shared`, and the copy-out-under-lock *is* its point.

**The `Snapshot` problem resolved for free.** `Snapshot<T>` was already generic over its payload, so
only `Telemetry` had to move; `Snapshot<cc_web::Telemetry>` type-checks with zero change to the type,
its hand-written `Sync`, or the take-and-restore fix. A genuinely portable `Snapshot` would have
needed either a one-impl trait (banned here) or a `Sync` whose safety argument no longer holds.

**The move found a third broken test.** `the_status_steam_mode_agrees_with_the_steam_toggle_response`
asserted `/api/status` contains `{"success":true,"steamMode":…}`, which it never has — it could only
ever fail, and never did because nothing runs device tests in the gate. First instance of that gap
being a broken test rather than an untested bug.

## Phase 4B — finding 4.2 ✅ measured, no refactor

The one item where the honest answer was **not to change the code**. See commit `2a2d6fef` and
`32-findings` §4B. The finding said three blocking mutexes per tick against a rule forbidding it;
measured, it was substantially overstated — the two per-tick locks cost under a microsecond on both
sides, the third is once a *second* (inside the SSE gate, not the tick), and httpd runs at the *same*
priority as control with no priority field in `esp-idf-svc` to change it. The rule was amended to
what the code actually guarantees, with every number recorded so a reader need not re-derive them.

## Phase 5 — feature scope 🔄

| # | item | note |
| --- | --- | --- |
| 3.1 | 3 status LEDs drive no GPIO | 6 schema params already exist and are UI-visible |
| 3.4 | `/api/parameter-help` returns a stub with **HTTP 200** | error object returned as success |
| 3.8 | No server-wide 404 handler | C++ returns JSON for `/api/*` |
| 3.2 | Telnet has a ring buffer but no transport | `HeapShed` policy + ring already exist |
| 3.3 | OTA endpoints answer `501` | **the partition table already has `app0`/`app1`/`ota_0`/`ota_1`/`otadata`** — no partition change needed. `Effect::SafeHardwareShutdown` exists and is unused: it is the S8 hook. |
| 4.7 | `main.rs` is 3,671 lines and unnamed | extract `probe.rs`, `config_io.rs` |
| 4.1b | `mqtt.rs`'s pure half (`Topics`, `Registry`, `interval_for`, `is_configured`) | the same extraction as `cc-web`; deliberately deferred because half-done is worse than not started |

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
