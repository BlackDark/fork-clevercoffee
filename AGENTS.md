# Agent rules — the CleverCoffee firmware fork

This file is the only place in this repository where a rule is stated.
Everything else links here. If you need a rule that is not here, add it here --
not to a document, not to a skill, not to `CLAUDE.md`.

**Rules are numbered. Cite them; never restate them.**

## Scope

`AG-DISPLAY-*`
: the OLED layout rules — anything under `cc-display`, the display templates,
  or the 128×64 layout documentation.

`AG-RUST-*`
: the Rust port -- `crates/`, `Cargo.*`, `justfile`, `.cargo/`,
  `rust-toolchain.toml`, `ui/`.

`AG-REPO-*`
: the Rust tree, and the repository around it.

There is **one** firmware. The C++ tree (`src/`, `include/`, `lib/`, `test/`,
`platformio.ini`) was deleted once the Rust port became the product; pick the
prefix by **what you touched**, and everything left is `AG-RUST-*` or
`AG-REPO-*`.

Three documents carry the repository's navigational load, and none states a rule:

- [`docs/index.md`](docs/index.md) — **the map.** One row per document, grouped
  by situation. Every document in the repository appears there exactly once; if
  you write one, the table is out of date until it is in it.
- [`docs/status.md`](docs/status.md) — the **only** page permitted to claim what
  works. Dated, named owner, every line a pointer to a commit or a measurement.
- [`docs/hardware/pins.md`](docs/hardware/pins.md) — the GPIO map and the traps
  in it. The C++ header it was transcribed from is gone; see that page for what
  the transcription is and is not.

---

## 1. The repository

### Before you plan anything

**AG-REPO-1.** **The Rust port has a state machine, a PID and brewing on the
device.** R4-01 landed and was exercised on hardware on 2026-09-30: the
`cc-machine` reducer is wired into the 10 ms control task, effects are applied
through `cc-hal-esp32::actuators` in the same tick, and the machine boots to
`PID_NORMAL` with the PID driving the heater. The C++ firmware that preceded it
has been deleted. Do not re-implement the control loop, and do not reconstruct
the C++ tree. The page of record for what works is
[`docs/status.md`](docs/status.md); this rule exists because an agent that
believed the opposite claim cost a review cycle.

**AG-REPO-2.** A status claim in **any** document is unverified until you have
run `git log --oneline -- <file>` on it. This repository's own history contains
several confidently-wrong status claims that were caught only by an independent
review.

**AG-REPO-3.** Start at [`docs/history/README.md`](docs/history/README.md) and
read ["Where the migration actually is"](docs/history/README.md#where-the-migration-actually-is)
**before planning any work**. For what the firmware *is*, read
[`docs/architecture.md`](docs/architecture.md); for the vocabulary, read
[`GLOSSARY.md`](GLOSSARY.md). The execution procedure for agents lives in
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

**AG-REPO-7.** **Run the Rust gate — `AG-RUST-1` and `AG-RUST-2`.** If a step
fails, fix it, re-run *all* applicable steps, and only then commit.

**AG-REPO-8.** Never assume tests pass without running them.

**AG-REPO-9.** **The Rust device answers to `test-cc-rust`**
(`cc_config::schema::DEFAULT_HOSTNAME`), and it keeps that name even though the
C++ firmware it distinguished itself from is gone. It was chosen when two
firmwares could sit on one network and answer to the same name; the port
diverges on pump timeouts, the steam-valve whitelist and the PID divide, and
those divergences are the record of what was investigated, so the name is left
as the marker they hang on. Change it **only** in
`cc_config::schema::DEFAULT_HOSTNAME`, and keep
[`docs/example_config.json`](docs/example_config.json) in step -- an import test
parses that exact file, so the two cannot drift apart. `mqtt.password` also
defaults to `silvia`; that is a **credential, not a name**. Full reasoning:
[`intentional-diffs.md` §12](docs/history/divergences.md).

**AG-REPO-10.** The target is the **original ESP32** (Xtensa), not an S3 or C6.
`esp32_usb` refers to the USB-to-UART cable; the chip has no native USB.

**AG-REPO-11.** **Cross-platform pitfalls.** CI runs on Ubuntu with GCC while a
developer may build with Clang, and the two treat certain warnings differently,
so verify compilation conceptually against both. A lint or warning gate whose
result depends on the compiler rather than on the pin is not a gate — which is
why `AG-RUST-3` pins the host toolchain instead of saying `stable`.

**AG-REPO-12.** **ESP32 heap awareness.** The ESP32 has ~320 KB of RAM.

- Static buffers in singletons (the Logger ring buffer, history arrays) must be
  sized conservatively; always calculate the total static RAM cost.
- The web UI is served from flash and costs 0 B of RAM
  (`cc-hal-esp32/build.rs` embeds the gzip bundle with `include_bytes!`). Do not
  "improve" that by buffering it in RAM.
- The ABP2 pressure sensor and the SSD1306 share the I²C bus. A frame is chunked
  into 8 bus writes for exactly that reason; do not make it one write per pixel
  row.
- A display frame and a control tick must both allocate **zero** bytes
  (`AG-RUST-11`, `AG-RUST-12`).
- After any change to buffer sizes or response handling, verify
  `/api/parameters?filter=all` still returns full JSON with telnet connected.

**AG-REPO-13.** Avoid introducing new external dependencies unless absolutely
necessary. If one is required, state the reason.

**AG-REPO-14.** Add documentation only where necessary.

**AG-REPO-15.** Tools available here: `gh` for GitHub, `jq` for JSON, `rg`
(ripgrep) for search. For the project's layout, build and test commands as they
stood for the **deleted C++ firmware**, see
[`docs/archive/cpp/REPOSITORY_SUMMARY.md`](docs/archive/cpp/REPOSITORY_SUMMARY.md)
-- it describes the C++, say so when you cite it.

**AG-REPO-16.** If you are working with a new library or tool, look up its
documentation from its website, its repository, or the relevant `llms.txt` before
relying on pre-trained knowledge. Accurate current documentation beats accurate
recalled documentation; `https://llmstxt.site/` and
`https://directory.llmstxt.cloud/` index collections.

**AG-REPO-17.** **Integration testing** is
[`docs/operations/runbook.md`](docs/operations/runbook.md). When the user asks for a
full integration test flow: run **every** section in order; for each item execute
the check (a `curl`, a `pio` command, a browser action); record PASS/FAIL with
the actual output; **stop at the first FAIL** and diagnose before continuing;
report a summary table at the end.

**AG-REPO-18.** **Keep that checklist current.** When you discover a new critical
scenario -- a crash, an OOM, an endpoint failure, a timing bug -- add it to
[`docs/operations/runbook.md`](docs/operations/runbook.md) immediately, in the same
commit, rather than waiting for a separate task. The checklist must reflect every
known failure mode. Examples of what belongs there: an API endpoint that handles
large payloads; a concurrency scenario that caused a crash; a new OTA or upload
path; a hardware interaction that can hang the device.

**AG-REPO-19.** **Do not leave placeholder code, TODOs or silent scope changes**
in a committed tree.

**AG-REPO-20.** Update [`docs/status.md`](docs/status.md) in the same commit as
any behaviour change, and **never claim a validation you did not run**.

### Hardware control invariants (CRITICAL)

These bind whenever the machine is powered and wired, and a regression here is
not a review nit.

**AG-REPO-21.** **Every state that activates pump or valve MUST deactivate them
on exit** -- `cc_machine::states::on_exit`. The next state's `on_entry` may not
run if an error interrupts the transition.

**AG-REPO-22.** **`cc_safety::water_flow_allowed` runs every tick** -- it must
allow ALL states that legitimately need the valve open (brew, manual flush,
active backflush filling). If you add a new water-flow state, update it.
`cc_safety::steam_flow_allowed` is the same rule for steam, which shares one
relay with water.

**AG-REPO-23.** **The PID is disabled for active operations.** If you add a new
operational state, check the exclusion list the reducer applies to the PID and
confirm the new state is in it.

**AG-REPO-24.** **Drain stale request flags.** States that cannot act on action
requests (`PID_DISABLED`, `STANDBY`, error states) must drain incoming flags to
prevent unexpected transitions on recovery.

**AG-REPO-25.** **State entry must be idempotent for hardware** -- a state's
tick should reinforce the desired hardware state (e.g. keep the pump enabled),
because a safety mechanism may have turned things off between cycles.

**AG-REPO-26.** **Never poke the relays directly.** Go through the effect
types: return `Effect::EnablePump` / `Effect::CloseWaterValve` /
`Effect::SetHeaterDuty` from the reducer, and let `cc-hal-esp32::actuators`
apply them. Reaching into the GPIO layer bypasses the internal bookkeeping and
can leave hardware stuck -- the actuator for opening a valve is a no-op while
the relay is off. Legitimate exceptions: `actuators`' own internals, and the PID
ISR's heater PWM (documented at its definition).

**AG-REPO-27.** **The deleted C++ is in git history, not in the working tree.**
When a behaviour is surprising, `git log -- <path>` and `git show
HEAD~:<path>` recover the implementation this port replaced; the per-feature
catalogue of what it did and which bugs it had is in
[`09-cpp-findings.md`](docs/history/cpp-findings.md). Read it before
concluding the port does something surprising.

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

**AG-REPO-30.** **`docs/history/` is machine-read. Move it only deliberately.**
It was `docs/rust-migration/` until 2026-10-06, when the whole documentation tree
was restructured and this directory moved with it. That move was **not** a
link cleanup and it is the reason this rule now has an explicit exemption: the
paths that name this directory are resolved by code and by CI, not only by
prose.

- `crates/cc-parity/tests/runner.rs` reads `history/divergences.md` at a
  hard-coded path.
- `scripts/parity/run.sh` and `scripts/size-record.py` take it as input.
- `just/size.just` reads `size-baseline.json` and appends to
  `size-records.jsonl`; `rust.yml` uploads the latter.
- Rust doc comments link into it with rustdoc link syntax, and `just lint` runs
  `rustdoc -D warnings`, so a broken link there is a **build break**.

**Amending it means changing those paths in the same commit**, and the amendment
must be dated in this rule. `divergences.md` is additionally *parsed* by
`cc-parity`: its fenced `ledger` blocks live inside the prose on purpose, so the
reasoning and the machine-readable declarations cannot drift apart. Do not split
them out, and do not renumber a section without updating the `heading` field of
the ledger entry that names it — `cc-parity` checks that the heading still
appears in the prose, so a renumber that misses one fails the harness rather than
passing quietly.

**AG-REPO-31.** **`docs/api/openapi.yaml` is checked against the routes the
firmware serves.** `scripts/check-openapi.py` compares it against the `ROUTES`
table in `cc-hal-esp32/src/web.rs`, in both directions, and runs in `just
check` and in CI. It exists because the spec sat for the whole port with no
consumer and no check, and omitted exactly the three routes carrying deliberate
divergences (`/api/sleep`, `/api/wake`, `/events`). A reference that is silent
about where the firmware behaves differently *by design* looks complete and is
not.

---

## 2. The OLED display layout (mandatory)

Binds whenever you change anything under `crates/cc-display/`, the display
templates, or OLED drawing code.

**AG-DISPLAY-1.** **Verify fit.** All text, icons, bars and bitmaps must fit fully
within **128x64** (`DISPLAY_WIDTH` x `DISPLAY_HEIGHT`). Nothing may clip at the
edges.

**AG-DISPLAY-2.** **Verify spacing.** No overlapping rows or elements. Compute Y
positions from **U8G2 bbox heights** with `setFontPosTop()` (Y = top of the glyph
box), not from the font name alone.

**AG-DISPLAY-3.** **Stable numeric fields.** Counting values (time, temperature,
weight) must not shift when the digit count changes (`9` -> `10`, `9.9` ->
`10.0`). Reserve a **fixed pixel width** per field using the widest expected
string (a `getStrWidth` probe), then draw inside that box, typically
right-aligned. Center composite blocks once; do not re-center a whole line every
frame from the live string width.

**AG-DISPLAY-4.** **Alignment.** Center composite elements as visual units
(horizontally on screen when appropriate). Paired controls -- a progress bar and
its value label -- share the same **vertical midline** in their row: vertically
center the bar with the label, do not bottom-edge-align them to mismatched
heights.

**AG-DISPLAY-5.** **Double-check before finishing.** Re-read the row map after
edits; anchor bottom rows from `DISPLAY_HEIGHT` where possible. See
[`docs/display/overview.md`](docs/display/overview.md) for which of the three
display documents answers which question, then
[`docs/display/layout-rules.md`](docs/display/layout-rules.md)
and [`docs/display/rendering.md`](docs/display/rendering.md).

**AG-DISPLAY-6.** **Layout regressions are blocking.** Cut-off text, overlapping
rows, shifting numbers and misaligned bar/label pairs must be fixed before the
task is done.


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
[`docs/operations/ci.md`](docs/operations/ci.md).

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
error on the chip. It is a display rule as much as a memory rule: the layout
ones are `AG-DISPLAY-1` through `AG-DISPLAY-6`.
