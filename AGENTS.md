# CleverCoffee (Rust)

Espresso machine PID firmware for the ESP32 DevKitC v4: no_std Rust on Embassy, web UI embedded in the image.

## Verify

`mise run check` (= `just check`) green before every commit: fmt, clippy, host tests, firmware clippy (minimal and full), UI lint/tsc/tests. After display changes also `just screens` and look at `target/screens/index.html`. After firmware changes `just size full`.

## Stack

Follows https://github.com/BlackDark/agent-presets/blob/main/stack/core.md.
Deviations:
- Firmware toolchain is espup's `esp` channel (Xtensa), pinned in `firmware/rust-toolchain.toml`; host crates use stable from `rust-toolchain.toml`.
- UI is BrewUI (Svelte 5, TanStack Query), taken from `main` and trimmed to firmware-only updates; no size-limit, `just size` guards the whole image.

## Map

- `crates/core`: the machine. `Machine::tick(&mut Config, &Inputs) -> Events`, all states, PID use, switches, scale, standby. No hardware, no time source: `now_ms` comes in.
- `crates/core/src/config.rs`: every parameter in one `define_params!` table (key, type, limits, default, UI metadata). Adding a parameter = one row.
- `crates/api`: JSON/form/multipart/provisioning formats. `crates/web`: routes over the `Backend` trait (tokio-tested in `tests/http.rs`). `crates/mqtt`: entities and HA discovery.
- `crates/display`: all screens, `render(&mut Frame, &View)`; `tests.rs` fails on any clipped pixel or missing glyph.
- `crates/{pid,tsic,onewire,hx711,abp2,oled,blescale}`: drivers and protocols, hardware behind embedded-hal traits.
- `firmware/`: separate workspace (Xtensa). Only wiring: tasks, pins, flash, network. Logic belongs in a crate.
- `tools/`: `screens` (PNG contact sheet), `provision` (cc_prov blob), `bench` (device smoke tests).
- Decisions: `docs/decisions/`. Terms: `GLOSSARY.md`. Open work: `docs/todo.md`.

## Every change

Same PR, only what the change touches:
- host tests for changed behaviour; behaviour that differs from the C++ firmware gets a `fix_*` test and a line in `docs/decisions/0004-behaviour-changes.md`
- `docs/` pages that describe it; a new decision gets an ADR
- new HTTP route: `crates/web` test + `ROUTES`; new MQTT entity: `crates/mqtt` test
- new failure mode found on the device: a check in `tools/bench` or a line in `docs/bench.md`

## Code style

- Data in, data out: pure functions, side effects (I/O, time, randomness) at the edges and passed in.
- Expected failures are values (`Result`); panic for bugs only. The firmware panic hook switches relays off, it is not error handling.
- Composition over inheritance: no class hierarchies. Rust traits are fine.
- Small units: split a file past ~400 lines, a function past ~60 (the `define_params!` table and screen code are exempt).
- Names explain what, comments explain why.
- Refactors keep behaviour and land in their own commit.

## Firmware rules

- Heater, pump and valve are driven only by `control.rs` from `Machine::outputs()`. Nothing else touches those pins (the panic hook excepted).
- `shared::with` is a critical section: copy out, never render, format large JSON or do I/O inside it.
- Big buffers go on the heap (reclaimed RAM), not in task futures: a configured Bluetooth scale takes 64 KB of main RAM in `full` (ADR 0001, 0008). Allocations a request can trigger are fallible: a failed allocation is a panic and a reboot. `cc_web::MIN_FREE_HEAP` admits a request only with 8 KB free; a route that allocates more checks for itself (`try_reserve`).
- A task that can block must not run on the control executor.

## Git

- Conventional Commits. One topic per PR.
- Merge: rebase when the commits are logically separate, squash otherwise.
- PR already merged → branch anew from `origin/main`, open a new PR.
- Force-push: `--force-with-lease` on own branches only.

## Boundaries

- `sudo`, system packages, global installs: the user runs them; ask with what and why.
- Flashing a device or running `tools/bench` needs the user's hardware: say what to run.
- Language: code, comments, docs, commits in English; UI and display in English, German, Spanish.

## Gotchas

- espup needs `--toolchain-version 1.99.0.0 --skip-version-parse` where the GitHub API is rate-limited (CI, sandboxes).
- `esp-rom-sys` misses BT ROM symbols: `firmware/ld/rom-fixups.x` provides them. Remove when upstream fixes it.
- espflash rejects custom data partition subtypes: `cc_cfg`/`cc_prov` are `undefined` and found by label.
- `firmware/build.rs` embeds `ui/packages/frontend/dist`; without `just ui` a placeholder page is served (build warning).
- GPIO1 (steam LED) is UART TX: enabling that LED silences the serial log.
- GPIO34–39 have no pulls: switches need external resistors, floating inputs read as pressed.
- `just flash` erases `otadata`: after an OTA update a plain `espflash flash` writes ota_0 and the board keeps booting ota_1.
- `firmware/vendor/` holds patched `xtensa-lx-rt` (boot), `smoltcp` and `embassy-net` (listen backlog); each patch is marked `CleverCoffee patch`, `just vendor-test` runs smoltcp's tests. Remove when upstream fixes them (docs/todo.md).
