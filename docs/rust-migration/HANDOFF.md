# Handoff: state of the Rust port at this commit

Read this first, then `docs/rust-migration/task-list.md`, which is the authoritative task list.
This file exists because the work is being done across sessions and the task list alone does not
say *why* something stopped or what the next session should know that the code does not.

## Where the work stopped

Everything through **T-09** is implemented, tested and committed. **T-10 and T-11** were completed
in the same batch as this handoff and are green.

**T-12 through T-22 have not been started.**

Every completed task is verified two ways: host tests, and a build for all three chips. Nothing has
been run on hardware, because no board is connected. See "Verification levels" below for what that
does and does not mean.

## The current numbers

- 16 crates, 414 host tests in the workspace, 55 in the provisioning tool.
- `mise exec -- just check` passes: format, clippy with `-D warnings`, tests, dependency layering,
  the secret scan, and the provisioning tool's own tests.
- `mise exec -- just check-fw esp32`, `esp32s3`, `esp32c6` all pass.

## How to run the gates

```sh
. .espup-env.sh          # chip builds need the esp toolchain on PATH
mise exec -- just check           # format, lint, host tests, deps, secrets
mise exec -- just check-fw esp32  # also: esp32s3, esp32c6
```

`check-fw` is per target on purpose. A `cargo --workspace` build unifies features across the
members, which would enable all three boards at once and fail inside the esp metadata.

## Task status

| Task | State | Notes |
| --- | --- | --- |
| T-01 workspace and guards | done | 16 crates, feature guards, CI, `check-deps`, `check-secrets` |
| T-02 domain | done | state machine, PID, sensor fusion, emergency stop |
| T-03 hal-traits | done | `Actuators`, sensors, display, switch, scale, storage, provisioning |
| T-04 config schema | done | 99 parameters, types, ranges, secrets |
| T-05 config region | done | A/B slots, CRC, digest, wraparound-safe generation |
| T-06 config import | done | transactional, the real `config.json` is a fixture |
| T-07 http | done | parser, router, SSE, connection loop over an injected stream |
| T-08 provisioning tool | done | protocol split from the serial port |
| T-09 one-wire and DS18B20 | done, with one gap | see "Known gap" below |
| T-10 pressure (ABP2) | done | no blocking delay, unlike the C++ |
| T-11 scale (HX711) | done | bounded init, `Option` weight |
| T-12 display | not started | framebuffer, font metrics, six templates |
| T-13 board profiles | not started | three BSP crates exist as stubs |
| T-14 control tasks | not started | the state machine wiring; safety-critical |
| T-15 provisioning on device | not started | the device half of the T-08 protocol |
| T-16 web API | not started | 30 routes, see `api-contract.md` |
| T-17 MQTT and HA discovery | not started | |
| T-18 telnet logging | not started | ship a plain line server |
| T-19 end-to-end import | not started | blocked on a device |
| T-20 parity check | not started | |
| T-21 display on device | not started | blocked on a device |
| T-22 remove the C++ tree | not started | do this last |

## What the next session should do first

**T-14, the control tasks and the state machine wiring.** Everything so far is a library: the
domain crate computes a transition, the HAL traits declare what a pump is, and the HTTP crate
parses a request. Nothing yet connects them into a machine that runs. T-14 is where the safety
work becomes real, and it is where the pieces already written either fit together or do not.

It also has the best test story available: `RecordingActuators` from `hal-traits` records the exact
sequence of commands, so a brew scenario can be driven end to end on the host and asserted as a
command sequence rather than as a set of flags. Do that before writing any task wiring, because a
scenario test that can be run on the host is what makes T-16 and T-20 cheap afterwards.

Suggested order after that: T-12 (display, self-contained and testable), T-16 (web API, large but
mechanical against `api-contract.md`), T-15 and T-17 and T-18 (each self-contained), then T-13,
then T-20, then T-22.

## Known gap, stated rather than papered over

**Multi-drop ROM enumeration is not verified.** The 1-Wire search algorithm is Stoffregen's and its
wire shape is asserted against a loopback bus: 64 bit pairs, 64 branch decisions, 8 CRC bits, and
a refusal to accept a device whose transmitted checksum does not match. What is *not* proven is
which devices a search returns on a bus with several of them, because doing that in simulation
requires every device to drive the line at the same time and the simulated bus here does not model
that correctly.

A machine has one sensor on its bus, so this is a gap in the tests rather than a path the firmware
takes. It is recorded in `docs/rust-migration/task-list.md` under T-09. Do not paper over it with a
weaker assertion that reads like more than it is; fix the simulation instead.

## Verification levels

From `docs/rust-migration/verification-levels.md`:

- `repo-verified`: what the tests assert, and they pass.
- `build-verified`: compiles and lints for esp32, esp32s3 and esp32c6.
- `device-verified`: runs on hardware. **Nothing in this port is device-verified.**

The hardware-dependent tasks are T-09 (real bit timings), T-11, T-12 (layout on the panel), T-13,
T-19, T-21. Until a board is connected, treat those as untested and say so.

## Things a new session will trip over

These cost time to rediscover, so they are written down.

- **`Value` has a lifetime.** `schema::Value<'a>` borrows its text, because a parsed string is not
  `&'static str`. `ResolvedDoc` owns its text bytes inline instead of borrowing a parse buffer; a
  value aliasing a reused buffer would change under the user on the next parse.
- **`resolve_cycle_advance` is the only place the backflush cycle arithmetic happens.** It uses
  `saturating_add` because a `u8` at 255 wraps in a release build and would look like "back to the
  first cycle" forever.
- **The scale calibration is signed and zero is forbidden.** The shipped config uses -1750.05 for
  an inverted cell, so the range cannot be narrowed to exclude zero. That is what `forbid_zero` on
  `Param` is for.
- **A faulted `TemperatureFilter` yields `None`, not its last mean.** Returning a stale mean is the
  exact shape of C++ defect D03.
- **The ABP2 frame is twelve bytes, not the seven the C++ read.** Two six-byte words, each three
  status and three data. The C++ took its temperature count from bytes 4 to 6, one of which is a
  status byte, so its temperature was wrong by construction. Recorded as D54.
- **`Response` is under 700 bytes and `SseStream` about 1.2 KB**, both asserted. Sizing either to
  the request header bound made every exchange cost kilobytes of stack it never used.
- **A raw string literal is `r#"..."#`.** Writing `r#"{"#` gives `{`, not `{"`, and the missing
  quote is invisible until a parse test fails somewhere unrelated.
- **`f64::round`, `f64::fract` and `ToString` are not available in `no_std`.** Use truncation with a
  comment, or `core::fmt::Write` into a `heapless::String`.
- **Test-only modules in a library are not visible to another crate's tests.** The 1-Wire simulated
  bus is behind a `test-support` feature for that reason. `extern crate std;` inside a `#[cfg(test)]`
  module works for the same crate but not across crates.
- **`check-secrets.py` scans tracked files only.** A new file is invisible to it until it is added
  to the index, so a fixture with a `WIFI_PASS=` assignment will only fail the gate once staged.
- **Provisioning fixtures must use `placeholder-` or `example-` prefixed values**, for the same
  reason, and the secret scanner's value pattern stops at a backslash so a Rust `\n` inside a
  dotenv fixture does not become part of the value.

## Defects found and fixed so far

The register is `docs/rust-migration/defects-register.md`. Fixed in the port so far: D03, D11,
D12, D13, D14, D23, D26, D27, D29, D33, D41, D42, D45, D46, D47, D49, D50, D51, D52, D54. Defects
recorded but not yet addressed are listed in the register and in each task's "Fixes" line.

D54 was found while porting the pressure sensor and was not in the original register.

## Process notes

- Each task ends with a review subagent before it is committed, then a commit whose subject is the
  task ID, then a push.
- `docs/rust-migration/architecture.md` holds the execution model and the defect-to-fix table. Read
  it before changing crate boundaries; `tools/check-deps.py` enforces the layering mechanically.
- Secrets come from `.env` and never appear in output, logs, commits or examples.
