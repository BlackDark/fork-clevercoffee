# Handoff: state of the Rust port at this commit

Read this first, then `docs/rust-migration/task-list.md`, which is the authoritative task list.
This file exists because the work is being done across sessions and the task list alone does not
say *why* something stopped or what the next session should know that the code does not.

## Where the work stopped

**T-01 through T-12, T-14, T-15, T-16, T-17 and T-18 are implemented and host-tested. T-13 is
partly implemented and cannot be compiled in this checkout. T-19, T-20 and T-21 need a device, and
T-22 is blocked behind T-20.**

The one thing to understand before reading any of it: **the chip toolchain is unavailable here.**
`just espup-install` downloads an x86-64 `espup` binary and this host is aarch64, so it exits 126
and the `esp` rustc fork is never installed. `just check-fw` cannot run at all. Everything that is
`build-verified` in the compatibility matrix got there on an earlier host; nothing in this session
was compiled for a chip.

## The current numbers

- 17 crates, 557 host tests in the workspace, 55 in the provisioning tool.
- Host gate, all green: `cargo +stable fmt --all -- --check`, `cargo +stable clippy --workspace
  --exclude clevercoffee-fw --exclude clevercoffee-bsp-* --all-targets -- -D warnings`, the
  workspace tests, `tools/check-deps.py`, `tools/check-secrets.py`, and the provisioning tool's
  tests.
- **`just check-fw esp32c6` passes**, and `cargo build --release --target
  riscv32imac-unknown-none-elf -p clevercoffee-fw --features board-esp32c6,prov-usb-cdc` links a
  1.76 MB image. The `check-fw` recipe now falls back to the stable toolchain when the `esp` fork
  is absent, which is what makes a RISC-V target checkable on a host that cannot install it.
- `just check-fw esp32` and `just check-fw esp32s3`: **cannot run here.** The Xtensa chips need the
  Espressif fork of rustc and `just espup-install` fetches an x86-64 `espup`, which exits 126 on
  this aarch64 host.
- `mise exec -- just check` therefore still fails at its first recipe, because `fmt-check` shells
  out to bare `cargo`, which reads `rust-toolchain.toml` and finds no `esp` toolchain. Run the host
  pieces with `cargo +stable` until the toolchain exists.

## Task status

| Task | State | Notes |
| --- | --- | --- |
| T-01 workspace and guards | done | 17 crates, feature guards, CI, `check-deps`, `check-secrets` |
| T-02 domain | done | state machine, PID, sensor fusion, emergency stop |
| T-03 hal-traits | done | `Actuators`, sensors, display, switch, scale, storage, provisioning |
| T-04 config schema | done | 99 parameters, types, ranges, secrets |
| T-05 config region | done | A/B slots, CRC, digest, wraparound-safe generation |
| T-06 config import | done | transactional, the real `config.json` is a fixture |
| T-07 http | done | parser, router, SSE, connection loop over an injected stream |
| T-08 provisioning tool | done | protocol split from the serial port |
| T-09 one-wire and DS18B20 | done, with one gap | multi-drop enumeration unverified, see below |
| T-09b TSIC | done | both temperature sensors kept, as the user asked |
| T-10 pressure (ABP2) | done | no blocking delay, unlike the C++ |
| T-11 scale (HX711) | done | bounded init, `Option` weight |
| T-12 display | done | framebuffer, one 5x7 font at two scales, six templates, 63 tests |
| T-13 board profiles and the firmware | **partly done** | pin maps done and host-tested; the C6 compiles and links; the ESP32 and S3 glue is uncompiled |
| T-14 control tasks | done | 21 whole-machine scenarios against a recorder |
| T-15 provisioning on device | done | the device half, host-tested end to end |
| T-16 web API | done, handlers only | 31 route tests; the socket layer is not written |
| T-17 MQTT and HA discovery | generator and parser done | the client is not chosen and the socket is not written |
| T-18 telnet logging | ring buffer and server done | the listener is not written |
| T-19 end-to-end import | not started | blocked on a device |
| T-20 parity check | partly done | the evidence is the per-task suites; no checklist document, no device legs |
| T-21 display on device | not started | blocked on a device |
| T-22 remove the C++ tree | not started | and it should not be: its prerequisite is a parity gate that cannot pass |

## What the next session should do first

**Get a chip toolchain and compile the board crates.** That is the whole of the remaining risk in
this port. The logic is host-tested and the pin maps are checked; what is unchecked is about four
hundred lines of `esp-hal` glue in `crates/bsp-*` and `crates/fw`, written against the HAL's
source but never fed to a compiler. On an x86-64 host: `just setup`, then `just check-fw esp32`,
`just check-fw esp32s3`, `just check-fw esp32c6`, and fix what does not compile. Expect errors in
the pin-construction macros and in the UART and USB transport signatures; expect the logic to be
fine.

The first thing to try, because it is free: `just check-fw esp32c6` already passes, and the same
recipe with the ESP32 or S3 will report exactly what is wrong with those two board crates on an
x86-64 host. Expect the pin-range macro and the UART transport to need work; the C6 needed about
ten rounds to compile, and most of them were the same class of mistake.

After that, in this order:

1. **The sensor drivers in the firmware.** The `bsp` crates build a machine that reads the panel
   switches and the water tank and nothing else. The DS18B20, ABP2 and HX711 drivers exist and are
   host-tested; nothing calls them.
2. **The network stack and the socket layer.** `esp-radio` and `embassy-net` are build-verified in
   the spikes. The API handlers, the MQTT generator, the line server and the provisioning protocol
   are all written and all take an injected stream, so this is wiring rather than design.
3. **T-20's checklist document**, which is a writing task and needs no device for the functional
   and API legs.
4. T-19 and T-21 when a board exists.

## Three defects found in this session

- **D56**, the valve interlock: `BrewHandler::valveSafetyShutdownCheck` closed the water valve in
  `PID_NORMAL` and `STEAM_RUNNING`, which are the two states the hot-water dispense runs inside, so
  hot water pumped with the valve shut. The interlock list now names them, and the domain's test
  asserts the one direction that is a safety property rather than the equality that hid it.
- The service mode was expressed as "the PID is off" in the transition input, so starting a
  provisioning session or an OTA ejected the machine from whatever it was doing. The machine lost
  its brew and had to be restarted by the user.
- The emergency stop was evaluated on the *filtered* temperature. The filter is a fifteen-sample
  mean, so a genuine over-temperature took six seconds to move it far enough to trip.

## Known gaps, stated rather than papered over

**Multi-drop ROM enumeration is not verified.** The 1-Wire search algorithm is Stoffregen's and its
wire shape is asserted against a loopback bus. What is not proven is which devices a search returns
on a bus with several of them, because the simulated bus does not model several devices driving the
line at once. A machine has one sensor, so this is a gap in the tests rather than a path the
firmware takes.

**The font metrics are a new input.** The C++ drew with U8G2 fonts whose bitmaps live in a library
PlatformIO fetched, so the metrics could not be read from this repository. The port ships one 5x7
table at two integer scales, which is legible and measurable and almost certainly not what the
panel looked like before. T-21 is the task that checks it, and it needs a device.

**The ESP32 and S3 board crates have never been compiled.** The C6's have, and they are close
enough in shape that a shared mistake is unlikely, but the pin ranges and the UART transport are
per-chip and neither has seen a compiler. Said here and in the compatibility matrix because it is
the fact most likely to be forgotten by the next reader.

## Verification levels

From `docs/rust-migration/verification-levels.md`:

- `repo-verified`: what the tests assert, and they pass.
- `build-verified`: compiles and lints for esp32, esp32s3 and esp32c6.
- `device-verified`: runs on hardware. **Nothing in this port is device-verified.**

The hardware-dependent tasks are T-09 (real bit timings), T-11, T-12 (layout on the panel), T-13,
T-19, T-20, T-21.

## Things a new session will trip over

- **`rust-toolchain.toml` pins the `esp` channel**, so *every* bare `cargo` command fails until the
  toolchain is installed. Use `cargo +stable` for host work in the meantime.
- **`Value` has a lifetime**, `resolve_cycle_advance` is the only place the backflush arithmetic
  happens, the scale calibration is signed with zero forbidden, and a faulted `TemperatureFilter`
  yields `None` rather than its last mean. All unchanged from the last handoff, all still true.
- **`format!` does not exist in a `no_std` crate's own tests.** `crates/app/tests/*.rs` are separate
  crates and *do* have `std`; `crates/app/src/*.rs` tests do not, and use the `tstr!` macro or
  `core::fmt::Write`.
- **`Response` borrows its body**, so a handler that builds a payload returns an owned
  `app::api::Reply` and the socket layer converts it. That is why `api.rs` has a `Reply` type at
  all.
- **`heapless::String` has no `last()` and no `pop()` on a borrowed form in the way `std` does**,
  and `push` returns a `Result` that must be handled.
- **`check-deps.py` needs `clevercoffee-board-profiles`** for the `bsp-*` crates, since the pin maps
  moved out of them.

## Defects found and fixed so far

The register is `docs/rust-migration/defects-register.md`. Fixed in the port so far: D03, D11,
D12, D13, D14, D23, D26, D27, D29, D33, D41, D42, D45, D46, D47, D49, D50, D51, D52, D54, D56. D54
and D56 were found while porting and were not in the original register.

## Process notes

- Each task ends with a review, then a commit whose subject is the task ID, then a push.
- `docs/rust-migration/architecture.md` holds the execution model and the defect-to-fix table; read
  it before changing crate boundaries. `tools/check-deps.py` enforces the layering mechanically.
- Secrets come from `.env` and never appear in output, logs, commits or examples.
