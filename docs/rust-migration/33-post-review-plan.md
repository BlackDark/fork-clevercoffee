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

| # | item | commit | note |
| --- | --- | --- | --- |
| 3.1 | 3 status LEDs drive no GPIO | `488433a7`, `0d2c4f18` | status (26) + brew (19) work; **steam deliberately unwired**, see below |
| 3.2 | Telnet has a ring buffer but no transport | `66d72e3f` | port 23; constants taken from the C++, not chosen |
| 3.4 | `/api/parameter-help` returns a stub with **HTTP 200** | `1ce694b2` | real per-parameter help; 200/404/422 now match the C++ |
| 3.8 | No server-wide 404 handler | `1ce694b2` | JSON 404 for `/api/*` |
| 3.3 | OTA endpoints answer `501` | ❌ **blocked** — the async subagent backend failed to launch any child after the cc-mqtt run, and OTA needs a worker. See "Blocked" below. |
| 4.1b | `mqtt.rs`'s pure half (`Topics`, `Registry`, `interval_for`, `is_configured`) | ✅ `5e31b7a2` + `156b50e9` — `cc-mqtt`, and the split it needed |
| 4.4 | `cc-domain` had grown from vocabulary to all-portable-logic | ✅ `156b50e9` — **split after all**; see "The decision reversed" below |
| 4.7 | `main.rs` is ~3,700 lines and unnamed | ❌ **blocked** with OTA |

### The steam LED — a parity claim that did not survive checking

The C++ puts the steam LED on GPIO 1, which is also UART0 TX. That was briefed as "reproduce the
C++'s conflict". Checking it first turned up something stronger:

- `Peripherals::take()` hands out each pin field **exactly once**, and `unsafe_code` is denied
  workspace-wide, so a pin cannot be shared at all. It is a sole-ownership conflict, not a wire one.
- **The C++ has the same bug.** `HardwareManager.cpp:117-125` calls `GPIOPin(1, OUT)` while
  `Serial.begin()` has UART0 on GPIO1 — both drive the line, last attach wins. And `pinmapping.h:45`
  contradicts *itself*: the comment says "Moved from pin 1 (UART TX - conflicts with serial logging)"
  while the `#define` on that same line is still `1`.

So "parity" would have meant matching a C++ bug, paid for with the machine's documented recovery path
(a machine on a nonexistent network cannot be fixed any other way). The user's chosen option was
overruled on that evidence and **option A** taken: the steam LED is not driven, GPIO1 stays with the
provisioning console. Recorded as `intentional-diffs` §27 and row 18 of the new
[`34-known-differences.md`](./34-known-differences.md).

The LED worker then **corrected a false claim in its own first draft** — it had written "there is no
free GPIO left"; in fact 10 of 28 are free, 13 and 14 genuinely so. The corrected conclusion is more
useful than the wrong one: GPIO 32 was never available not because the chip ran out of pins but
because that specific pin is `SCALE_DATA_1` — so a person *can* do it, and 13/14 is where the wire goes.

## Phase 6 — splits and OTA 🔄

| # | item | status |
| --- | --- | --- |
| 4.1b | `mqtt.rs`'s pure half | ✅ `5e31b7a2` — `cc-mqtt`, the seventh portable crate. 19 device-only tests became host tests; `mqtt.rs` 2,033 → 875 lines. |
| 4.4 | `cc-domain` split into vocabulary / protocol / net-policy | 🔄 running |
| 4.7 | `main.rs` (~3,700 lines) → `probe.rs` + `config_io.rs` | 🔄 running |
| 3.3 | OTA, with the S8 safety requirement | 🔄 running |

## Phase 7 — documentation 🔜

| # | item |
| --- | --- |
| 2.9 | `tick_allocations.rs` is titled "the control tick must not touch the heap" but proves only the *reducer* |
| 4.4 | `cc-domain` grew from "vocabulary" to "all portable logic"; its "read this in one sitting" claim is stale |
| 7.1 | AGENTS.md/CLAUDE.md said 1,074 host tests — **now 1,191 host tests plus 109 registered device cases**, counted from a fresh clone at the end of the work. Fixed in both files. |
| 7.2 | The migration README's "Not done" list omits LEDs, telnet and `parameter-help` |
| 7.3 | README says `/events` is broken with `Content-Length: 0`; it was fixed and never updated |
| 7.4 | `docs/ci.md`'s warm table says the esp toolchain install is 71 s; its own Caches section says no-op |

---

## Blocked — and one thing that had to be repaired first

### The branch did not build from a clean checkout, and the gate did not say so

`53c99df2` (cc-web) and `5e31b7a2` (cc-mqtt) were committed importing `cc_protocol` and
`cc_netpolicy` — crates that existed only in a worker's **uncommitted** working tree. `cargo` could
not resolve either one, so `just test` and `just gate` both failed on a fresh clone. Every gate run
during that period passed **only because the missing crates were sitting unstaged in the same tree**.

This is the most important thing that happened in this effort, and it is worth keeping: `just gate`
was green, the commit history looked clean, and the branch was still broken. Nothing in the gate
distinguishes "builds" from "builds because something is lying in my working directory". The only
check that caught it was cloning to `/tmp` and running the gate there — which is not in the project's
own procedure, and should be.

`156b50e9` completes the split and repairs it. `just test` now passes from a fresh clone.

### The decision on 4.4 reversed, because it was already half-done

I had decided **docs only, do not split `cc-domain`**. That was the right call for the reason I gave
(the layering was already sound, so splitting added churn without fixing a boundary). It stopped being
the right call the moment `cc-mqtt` had already moved `mqtt`/`wifi`/`resilience`/`history` out —
at which point the split was not a proposal, it was an unpaid debt that two committed crates were
already leaning on.

It is now complete: `cc-domain` is the vocabulary, `cc-protocol` is the protocol stacks plus
`http_auth`/`provisioning`, `cc-netpolicy` is the link and publish policy with **no dependencies at
all**. The audit property the review leaned on is now more literal than before — `cc-domain` depends
on nothing, so `cc-safety`, which depends only on it, cannot acquire a peripheral.

### Still open

| # | item | why |
| --- | --- | --- |
| 3.3 | OTA (three endpoints, and S8's pump/valve-off requirement) | The async subagent backend failed to launch **any** child — even a trivial one — so no worker could be given it. OTA needs a worker: it is the largest remaining item and the one most in need of a second pair of eyes. |
| 4.7 | Split `main.rs` (~3,700 lines) into `probe.rs` + `config_io.rs` | Same cause. |
| 8.4 | `CONFIG_REFERENCE.md:455` documents `pid.regular.i_max` as `0.0-999.0`; schema is `0.0..=100.0` | Same cause. |
| 8.5 | `docs/ci.md`'s warm figure for the esp toolchain install contradicts its own Caches section | Same cause. |
| 3.5 | No C++ parity baseline | Decided: leave it. |
| 3.7 | Acaia BLE scale | Decided: out of scope. |

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
