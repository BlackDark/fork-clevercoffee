# Agent rules — the CleverCoffee firmware fork

This file is the only place in this repository where a rule is stated.
Everything else links here. If you need a rule that is not here, add it here --
not to a document, not to a skill, not to `CLAUDE.md`.

**Rules are numbered. Cite them; never restate them.**

## Scope

`AG-ORACLE-*`
: the C++ tree -- `src/`, `include/`, `lib/`, `test/`, `platformio.ini`, and the
  C++-owned root `partitions_4M.csv`.

`AG-RUST-*`
: the Rust port -- `crates/`, `Cargo.*`, `justfile`, `.cargo/`,
  `rust-toolchain.toml`, `ui/`.

`AG-REPO-*`
: both trees, and the repository around them.

Pick the prefix by **what you touched**, not by which firmware you were thinking
about. A rule that genuinely covers both is `AG-REPO-*` and says so.

Three documents carry the repository's navigational load, and none states a rule:

- [`docs/index.md`](docs/index.md) — **the map.** One row per document, grouped
  by situation. Every document in the repository appears there exactly once; if
  you write one, the table is out of date until it is in it.
- [`docs/status.md`](docs/status.md) — the **only** page permitted to claim what
  works. Dated, named owner, every line a pointer to a commit or a measurement.
- [`docs/cpp-oracle.md`](docs/cpp-oracle.md) — where the C++ lives, why it is
  frozen, what may still be built from it, and what must never be done to it.

---

## 1. Both trees

### Before you plan anything

**AG-REPO-1.** **The Rust port has a state machine, a PID and brewing on the
device.** R4-01 landed and was exercised on hardware on 2026-09-30: the
`cc-machine` reducer is wired into the 10 ms control task, effects are applied
through `cc-hal-esp32::actuators` in the same tick, and the machine boots to
`PID_NORMAL` with the PID driving the heater. Do not re-implement the control
loop. The page of record for what works is
[`docs/status.md`](docs/status.md); this rule exists because an agent that
believed the opposite claim cost a review cycle.

**AG-REPO-2.** A status claim in **any** document is unverified until you have
run `git log --oneline -- <file>` on it. This repository's own history contains
several confidently-wrong status claims that were caught only by an independent
review.

**AG-REPO-3.** Start at
[`docs/rust-migration/README.md`](docs/rust-migration/README.md) and read
["Where the migration actually is"](docs/rust-migration/README.md#where-the-migration-actually-is)
**before planning any work**. The execution procedure for agents lives in
[`.agents/skills/esp32-rust-migration/SKILL.md`](.agents/skills/esp32-rust-migration/SKILL.md).
Neither document may restate a rule from this file; if one needs to, it links
here instead.

**AG-REPO-4.** `docs/status.md` is the only page permitted to claim what works.
If you change behaviour, update it **in the same commit** (AG-REPO-20).

**AG-REPO-5.** This file is the rulebook. `CLAUDE.md` is a five-line pointer to
it and must stay one — two copies of the same rules means one of them is always
wrong, and it already was.

**AG-REPO-6.** **Do not commit until all applicable checks pass.** Commits
without verification are not acceptable.

**AG-REPO-7.** **This repository has two firmwares. Run the gate for the one you
touched** — `AG-ORACLE-6` for the C++ tree, `AG-RUST-1`/`AG-RUST-2` for the Rust
port. If a step fails, fix it, re-run *all* applicable steps, and only then
commit.

**AG-REPO-8.** Never assume tests pass without running them.

**AG-REPO-9.** **The two firmwares are not interchangeable, and that is why their
hostnames differ.** The Rust device answers to `test-cc-rust`
(`cc_config::schema::DEFAULT_HOSTNAME`), not `silvia`; the C++ default is
`silvia` (`include/clevercoffee/defaults.h:14`) and the C++ is unchanged. Change
the Rust name **only** in `cc_config::schema::DEFAULT_HOSTNAME`, and keep
[`docs/example_config.json`](docs/example_config.json) in step -- an import test
parses that exact file, so the two cannot drift apart. `mqtt.password` also
defaults to `silvia`; that is a **credential, not a name**, and is deliberately
left alone. The port diverges from the C++ on pump timeouts, the steam-valve
whitelist and the PID divide, so a hostname that says which firmware answered is
load-bearing. Full reasoning:
[`intentional-diffs.md` §12](docs/rust-migration/intentional-diffs.md).

**AG-REPO-10.** The target is the **original ESP32** (Xtensa), not an S3 or C6.
`esp32_usb` refers to the USB-to-UART cable; the chip has no native USB.

**AG-REPO-11.** **Cross-platform pitfalls.** macOS has a case-insensitive
filesystem and CI (Ubuntu) does not: `#include <String.h>` resolves to
`WString.h` on macOS and fails on Linux, so always use the exact filename casing.
macOS and CI may ship different `clang-format` versions that disagree on
alignment -- wrap intentionally aligned blocks in `// clang-format off/on`. CI
runs on Ubuntu/GCC while local macOS builds use Clang, and the two treat certain
warnings differently, so verify test compilation conceptually against both.

**AG-REPO-12.** **ESP32 heap awareness.** The ESP32 has ~320 KB of RAM.

- Static buffers in singletons (the Logger ring buffer, history arrays) must be
  sized conservatively; always calculate the total static RAM cost.
- In the C++ tree, large JSON responses must use `AsyncJsonResponse` (chunked
  streaming), never an intermediate `String` serialized and then copied into
  `request->send()`.
- Wi-Fi logging (telnet) must be shed under heap pressure -- disconnect the
  client rather than crash the device.
- After any change to buffer sizes or response handling, verify
  `/api/parameters?filter=all` still returns full JSON with telnet connected.

**AG-REPO-13.** Avoid introducing new external dependencies unless absolutely
necessary. If one is required, state the reason.

**AG-REPO-14.** Add documentation only where necessary.

**AG-REPO-15.** Tools available here: `gh` for GitHub, `jq` for JSON, `rg`
(ripgrep) for search. For the project layout, build and test commands, coding
standards and TDD practices as they stood for the C++ tree, see
[`docs/archive/cpp/REPOSITORY_SUMMARY.md`](docs/archive/cpp/REPOSITORY_SUMMARY.md) -- it describes the C++, say so
when you cite it.

**AG-REPO-16.** If you are working with a new library or tool, look up its
documentation from its website, its repository, or the relevant `llms.txt` before
relying on pre-trained knowledge. Accurate current documentation beats accurate
recalled documentation; `https://llmstxt.site/` and
`https://directory.llmstxt.cloud/` index collections.

**AG-REPO-17.** **Integration testing** is
[`docs/operations/integration-checklist.md`](docs/operations/integration-checklist.md). When the user asks for a
full integration test flow: run **every** section in order; for each item execute
the check (a `curl`, a `pio` command, a browser action); record PASS/FAIL with
the actual output; **stop at the first FAIL** and diagnose before continuing;
report a summary table at the end.

**AG-REPO-18.** **Keep that checklist current.** When you discover a new critical
scenario -- a crash, an OOM, an endpoint failure, a timing bug -- add it to
[`docs/operations/integration-checklist.md`](docs/operations/integration-checklist.md) immediately, in the same
commit, rather than waiting for a separate task. The checklist must reflect every
known failure mode. Examples of what belongs there: an API endpoint that handles
large payloads; a concurrency scenario that caused a crash; a new OTA or upload
path; a hardware interaction that can hang the device.

**AG-REPO-19.** **Do not leave placeholder code, TODOs or silent scope changes**
in a committed tree.

**AG-REPO-20.** Update [`docs/status.md`](docs/status.md) in the same commit as
any behaviour change, and **never claim a validation you did not run**.

### Hardware control invariants (CRITICAL)

These bind in **both** trees. They are the rules that matter when the machine is
powered and wired, and a regression here is not a review nit.

**AG-REPO-21.** **Every state that activates pump or valve MUST deactivate them in
`onExitImpl`** -- the next state's `onEntry` may not run if an error interrupts
the transition.

**AG-REPO-22.** **`valveSafetyShutdownCheck()` runs every loop** -- it must
whitelist ALL states that legitimately need the valve open (brew, manual flush,
active backflush filling). If you add a new water-flow state, update this check.

**AG-REPO-23.** **`BaseState::checkTransitions` enforces PID disable for active
operations** -- if you add a new operational state, ensure the `constexpr`
exclusion list in `BaseState.h` is correct.

**AG-REPO-24.** **Drain stale request flags.** States that cannot act on action
requests (`PID_DISABLED`, `STANDBY`, error states) must drain incoming flags to
prevent unexpected transitions on recovery.

**AG-REPO-25.** **State entry must be idempotent for hardware** -- `update()`
should reinforce the desired hardware state (e.g. keep the pump enabled), because
`valveSafetyShutdownCheck` or another safety mechanism may have turned things off
between cycles.

**AG-REPO-26.** **Never poke relays directly.** Use `HardwareManager` /
`MachineStateContext` methods (`enablePump`, `disablePump`, `openWaterValve`,
`closeWaterValve`, `enableHeater`, `disableHeater`). Direct `relay->on()`/`off()`
or `setRelayState()` bypasses the internal bookkeeping (`valveState_`,
`pumpEnabled_`, `heaterEnabled_`) and can leave hardware stuck -- `openWaterValve()`
is a no-op while the relay is off. Legitimate exceptions: `HardwareManager`'s own
internals, and the PID ISR heater PWM in `isr.h` (documented).

**AG-REPO-27.** **Compare against the original.** The reference implementation is
at `/Users/marbaced/projects/clevercoffee`. When in doubt about hardware
behaviour, check `src/main.cpp` (`handleMachineState`), `src/brewHandler.h` and
`src/hotWaterHandler.h` -- and read the behaviour catalogue in
[`09-cpp-findings.md`](docs/rust-migration/09-cpp-findings.md) before concluding
the C++ does something surprising.

---

## Documentation structure

**AG-REPO-28.** **`docs/index.md` is the map, and it is exhaustive.** One row per
document, grouped by the four situations -- new here / changing behaviour / at
the machine / reading history. Every document in the repository appears there
**exactly once**. A document that is not in that table has no reachable entry
point, which makes it invisible; if you write one, the table is out of date until
it is in it.

**AG-REPO-29.** **`docs/archive/` is preserved history, not a source of truth.**
Material is moved there, never deleted, and never merged into a live document
(the other way round defeats the point). Each archived file carries a banner
saying what it was, when, and whether the Rust port supersedes it. **Any claim
quoted out of `archive/` into a live document must be re-verified against live
evidence** -- code, a test, a measurement, or [`docs/status.md`](docs/status.md) --
and the archive citation is recorded as *provenance only*, never as the proof. The
concrete failure this prevents: a retracted claim ("the ESP32 has no Bluetooth
radio") was lifted out of a September survey and treated as today's constraint.
[`docs/archive/README.md`](docs/archive/README.md) states the rule and says what is
in the archive and why.

**AG-REPO-30.** **`docs/rust-migration/` does not move.** Rust doc comments link
into it with rustdoc link syntax and `just lint` runs `rustdoc -D warnings`, so
moving a cited document is a **build break**, not a link cleanup. The machine-read
fixtures there (`size-baseline.json`, `size-records.jsonl`, `scenarios/*.yaml`) are
opened by code and CI by path. `intentional-diffs.md` is additionally *parsed* by
`cc-parity` and its fenced `ledger` blocks live inside the prose on purpose, so the
two cannot drift -- do not split them out.

---

## 2. The C++ oracle

Everything in this section is scoped `AG-ORACLE-*` and applies to `src/`,
`include/`, `lib/`, `test/`, `platformio.ini` and the root `partitions_4M.csv`.
[`docs/cpp-oracle.md`](docs/cpp-oracle.md) is the long form; this section is the
rule.

**AG-ORACLE-1.** **The C++ tree stays in production and is the parity baseline for
the whole migration. Do not change its behaviour.** It is not being cleaned up.
If you change its behaviour you have changed the thing the Rust port is measured
against, and that decision belongs in
[`intentional-diffs.md`](docs/rust-migration/intentional-diffs.md).

**AG-ORACLE-2.** **Never flash the C++ image.** It runs its own control loop on a
powered, wired machine.

**AG-ORACLE-3.** **Do not reformat the C++ tree as a side effect of a Rust
change.** `just fmt-cpp` is the formatter for `src/` and `include/`, and it is
only correct when a C++ change is the deliberate subject of the commit.

**AG-ORACLE-4.** Nothing in the Rust migration touches the `pio` tooling, the
partition table, or the C++ build until the corresponding task in
[`06-migration-task-list.md`](docs/archive/migration/06-migration-task-list.md) says
so.

**AG-ORACLE-5.** Source code lives in `src`, `lib`, `include`. The `pio` binaries
are at `~/.platformio/penv/bin`.

**AG-ORACLE-6.** **The C++ gate**, from the repository root unless noted. Run all
of it, in order:

1. **Format**, always before commit:
   `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`
2. **Build firmware**, always before commit:
   `~/.platformio/penv/bin/pio run -e esp32_usb -s`
3. **Native tests**, always before commit:
   `~/.platformio/penv/bin/pio test -e native_test`
4. **Frontend**, required when `ui/` files changed: from `ui/packages/frontend`,
   `pnpm test:run` and `pnpm tsc`; from `ui/`, `pnpm lint` (Biome check +
   format) and `pnpm format` (apply fixes).

Add `-s` to a `pio` command to silence it; remove it for verbose output.

**AG-ORACLE-7.** **Before you start editing, confirm the project builds** with
the build command above. Starting edits on a tree that does not build makes every
later failure ambiguous.

**AG-ORACLE-8.** **Common pitfall:** native tests include source `.cpp` files
directly (`test_build_src=false`). A new hardware or library include in shared
code -- pulling in BLE headers, for example -- can break unrelated native tests
through a stub, so transitive includes matter and `pio test -e native_test` runs
after firmware changes.

### C++ style, for anything C++ you write here

**AG-ORACLE-9.** **Focus areas.** Modern C++ (C++11/14/17/20/23); RAII and smart
pointers (`unique_ptr`, `shared_ptr`); template metaprogramming and concepts;
move semantics and perfect forwarding; STL algorithms and containers;
concurrency with `std::thread` and atomics; exception-safety guarantees. Note
that ESP32 (Arduino framework) does **not** provide concepts, `expected`,
`format`, or `__cpp_consteval` (`std::is_constant_evaluated()`).

**AG-ORACLE-10.** **Approach.** Prefer stack allocation and RAII over manual
memory management; use smart pointers when heap allocation is necessary; follow
the Rule of Zero/Three/Five; use const correctness and `constexpr` where
applicable; leverage STL algorithms over raw loops; profile with `perf` and
VTune.

**AG-ORACLE-11.** **Expected output of C++ work.** Modern C++ following best
practices; a `CMakeLists.txt` with the appropriate C++ standard; headers with
`#pragma once`; unit tests using Google Test or Catch2;
AddressSanitizer/ThreadSanitizer-clean output; performance benchmarks using
Google Benchmark; clear documentation of template interfaces. **Do not keep any
backward compatibility** -- clean modern new code is the goal.

**AG-ORACLE-12.** Follow the C++ Core Guidelines, and prefer compile-time errors
over runtime errors.

### OLED display layout (mandatory)

Binds whenever you change anything under `include/clevercoffee/display/`, the
display templates, or OLED drawing code.

**AG-ORACLE-13.** **Verify fit.** All text, icons, bars and bitmaps must fit fully
within **128x64** (`DISPLAY_WIDTH` x `DISPLAY_HEIGHT`). Nothing may clip at the
edges.

**AG-ORACLE-14.** **Verify spacing.** No overlapping rows or elements. Compute Y
positions from **U8G2 bbox heights** with `setFontPosTop()` (Y = top of the glyph
box), not from the font name alone.

**AG-ORACLE-15.** **Stable numeric fields.** Counting values (time, temperature,
weight) must not shift when the digit count changes (`9` -> `10`, `9.9` ->
`10.0`). Reserve a **fixed pixel width** per field using the widest expected
string (a `getStrWidth` probe), then draw inside that box, typically
right-aligned. Center composite blocks once; do not re-center a whole line every
frame from the live string width.

**AG-ORACLE-16.** **Alignment.** Center composite elements as visual units
(horizontally on screen when appropriate). Paired controls -- a progress bar and
its value label -- share the same **vertical midline** in their row: vertically
center the bar with the label, do not bottom-edge-align them to mismatched
heights.

**AG-ORACLE-17.** **Double-check before finishing.** Re-read the row map after
edits; anchor bottom rows from `DISPLAY_HEIGHT` where possible. Prefer
`DisplayLayoutUtils.h` for fixed-width and bar-plus-label cluster layout. See
[`docs/handbook/display.md`](docs/handbook/display.md) for which of the three
display documents answers which question, then
[`docs/handbook/display-modern-layout.md`](docs/handbook/display-modern-layout.md) and
[`docs/handbook/display-architecture.md`](docs/handbook/display-architecture.md).

**AG-ORACLE-18.** **Layout regressions are blocking.** Cut-off text, overlapping
rows, shifting numbers and misaligned bar/label pairs must be fixed before the
task is done.

---

## 3. The Rust port

Everything in this section is scoped `AG-RUST-*` and applies to `crates/`,
`Cargo.*`, `justfile`, `.cargo/`, `rust-toolchain.toml` and `ui/`.

**AG-RUST-1.** **`just check`** runs the host gate: fmt-check, clippy with
`-D warnings` (pedantic), rustdoc with `-D warnings`, the host test suite, the
parity harness, and the device-test audit.

**AG-RUST-2.** **`just check` is not sufficient.** It does not compile
`cc-hal-esp32` or `cc-firmware`. **For a change that can affect the firmware
image, run `just gate`**, which adds the device clippy, the Xtensa release build
and the size budget. The device steps use the `esp` toolchain and need
`just setup` to have run. `just doctor` checks the device toolchain;
`just doctor-host` checks only what a host-only machine can assert.

**AG-RUST-3.** **`just check` deliberately runs on STABLE**, not on the Espressif
`esp` toolchain: the portable crates are `#![no_std]` plain Rust and nothing in
them touches an Xtensa pin. Override the channel with `CC_RUST_TOOLCHAIN=stable`.
The justfile's default stays `esp`, because a just `export` beats an environment
variable, and because a device recipe must never silently compile with the wrong
compiler. Running the host gate on a stock toolchain is also what makes the
workspace's `rust-version = "1.82"` claim verifiable instead of decorative.

**AG-RUST-4.** **No document in this repository states a test count.** It drifted
on every phase of the 2026-10-03 review because the count is a function of the
tree, and a number in a doc goes stale silently. Count it yourself with
`just test` if you need it.

**AG-RUST-5.** What the CI pipeline is, what each job costs, and why the
toolchain pins and cache keys are shaped as they are:
[`docs/handbook/ci.md`](docs/handbook/ci.md).

**AG-RUST-6.** **The web UI must be built before the firmware will link.**
`cc-hal-esp32/build.rs` deliberately panics without
`ui/packages/frontend/dist`. `just build-esp32` and `just lint-esp32` depend on
the `ui:` recipe for exactly this; a bare `cargo build` will not do it.

**AG-RUST-7.** The toolchain is the Espressif **`esp` nightly fork**, pinned by
`rust-toolchain.toml`. `mise` deliberately does **not** install Rust -- rustup
and `espup` own the compiler and `just setup` bootstraps them. `just doctor`
checks that the pins agree.

**AG-RUST-8.** `cargo fmt --all` is the formatter for `crates/`.

**AG-RUST-9.** `.cargo/config.toml` has **no** `[build] target` on purpose: bare
`cargo` means the host. Every device recipe passes `--target` itself.

**AG-RUST-10.** Do not add `#[allow]`, `#[expect]` or a `macro_rules!` without
reading why the existing ones are there. `just lint` runs `clippy::pedantic` as
`deny`.

**AG-RUST-11.** **The control tick must not allocate the heap.**
`crates/cc-machine/tests/tick_allocations.rs` asserts zero allocations per tick
and `just bench` reports the number.

**AG-RUST-12.** **A display frame must not allocate either.** `cc-display` is
`no_std` with no `alloc` in the device build, so an allocation there is a link
error on the chip.
