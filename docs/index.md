# Documentation index

**One row per document.** Every document in the repository appears here exactly
once (`AG-REPO-28`). If you write one, this table is out of date until it is in
it.

Rows are grouped by your situation. If none of them fits, that is a defect in
this page, not in your question.

## Start here

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`README.md`](../README.md) | What this repository is, and how to build, gate and flash it. | You just opened the repository. |
| [`GLOSSARY.md`](../GLOSSARY.md) | The words this codebase uses that mean nothing outside it: divergence, tick, effect, dormant. | Any word in another document confuses you. Read it first; it is one page. |
| [`docs/index.md`](index.md) | This page. | You want to know which document to open. |
| [`docs/architecture.md`](architecture.md) | **What the firmware is.** The three boundaries that shape it, the 10 ms loop, where the 320 KB of RAM goes, and what each crate owns. | You want the shape of the thing before you change anything. |
| [`docs/status.md`](status.md) | **The only page permitted to claim what works.** Dated, owned, every line a pointer to a commit or a measurement. | You want to know what this firmware actually does today. |
| [`docs/attention.md`](attention.md) | **The only task list.** Problems, potential work, and dated later checks. An empty Problems section means stop. | You are about to start work. |

## The rules

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`AGENTS.md`](../AGENTS.md) | **The rulebook.** Numbered, scoped rules (`AG-REPO-*`, `AG-DISPLAY-*`, `AG-RUST-*`). Citable, never restated. | Before you change anything. |
| [`CLAUDE.md`](../CLAUDE.md) | A pointer to `AGENTS.md` and nothing else, deliberately. | Your tool looks for it. Nothing else. |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | Formatting, the gates, and the pre-commit setup. | You are about to open a pull request. |

## Changing behaviour

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/scenarios.md`](scenarios.md) | The scenario-file format `cc-parity` parses, and how a `dry_run` scenario runs the real reducer in process. | You add or change a scenario. |
| [`docs/config/reference.md`](config/reference.md) | Every configuration key, its type, range, default and bounds. | You need a parameter name or what it may be set to. |

## Architecture decisions

Cross-cutting. Each records a decision that outlived the port, and what was
rejected.

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/adr/0001-display-subsystem-architecture.md`](adr/0001-display-subsystem-architecture.md) | Accepted (2026-05-22), reimplemented in `cc-display`: one render pipeline, shared defaults with template overrides, one source of truth for thresholds. | You want the *why* behind the display structure. |
| [`docs/adr/0002-wifi-logging-ota-memory-architecture.md`](adr/0002-wifi-logging-ota-memory-architecture.md) | Accepted: Wi-Fi logging, OTA admission, and the memory budget. | You are touching networking, OTA, or the heap. |
| [`docs/adr/0003-state-machine-hardware-control-contract.md`](adr/0003-state-machine-hardware-control-contract.md) | Accepted: the pump, valve and heater ownership contract, enforced in `cc-machine` and `cc-safety`. Energise on entry, reinforce on update, release on exit. | **Adding a state, or touching anything that moves water.** |

## By subsystem

**Control** — the state machine, the PID, and what is allowed to move water.

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/control/state-machine.md`](control/state-machine.md) | The 18 states and what each energises, what a tick actually is, and the checklist for adding a state. | You are changing what the machine does next. |
**Display** — the 128×64 panel. Layout rules are `AG-DISPLAY-1` through `AG-DISPLAY-6` and are blocking.

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/display/overview.md`](display/overview.md) | Entry point for the display: which of the four documents to read. | You are changing anything on the screen. |
| [`docs/display/layout-rules.md`](display/layout-rules.md) | The binding layout rules: font to pixel mapping, fixed-width numeric fields, the row maps, bar and label pairing. | You are placing pixels. |
| [`docs/display/rendering.md`](display/rendering.md) | How the Rust renderer works: frame lifecycle, I²C chunking, shared versus template ownership. | You need to know when a frame reaches the panel. |
| [`docs/display/parity.md`](display/parity.md) | The display checks, what each caught, and plainly what they do **not** prove. | You are about to claim the display is correct, or regenerate a golden. |
**Hardware** — pins, relays and buses.

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/hardware/pins.md`](hardware/pins.md) | The GPIO map, the two traps in it, and what the table is transcribed from. | You are touching a pin, a relay, or a sensor. |
| [`docs/hardware/bench-setup.md`](hardware/bench-setup.md) | What to attach to a spare ESP32 to test without a machine, and exactly what an LED on a pin does not prove. | You are setting up a bench, or about to claim the water path works. |

**Protocols** — the wire.

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/protocols/sensors.md`](protocols/sensors.md) | The four sensor protocols written as state machines over bytes, why the TSIC-306 was the risk, and what has never run. | You are touching a sensor, or adding a protocol. |

**Web** — HTTP, MQTT and the UI.

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/web/http-and-ui.md`](web/http-and-ui.md) | Routing, the event stream, authentication, the OTA hole, and why the UI costs 0 B of RAM. | You are changing an endpoint, the stream, or the UI's delivery. |
| [`docs/api/openapi.yaml`](api/openapi.yaml) | The API contract, 28 paths. **Checked against the routes the firmware serves by `scripts/check-openapi.py`**; the check runs in `just check`. | You are calling or changing an endpoint. |
| [`ui/packages/frontend/README.md`](../ui/packages/frontend/README.md) | The React app's own build and test instructions. | You are changing the UI. |
| [`ui/packages/mock-server/README.md`](../ui/packages/mock-server/README.md) | Running the UI against a mock device. | You are working on the UI without hardware. |

## At the machine

| Document | What it is for | Read it if |
| --- | --- | --- |
| [`docs/operations/runbook.md`](operations/runbook.md) | **The runnable pre-release checklist**, section by section, with the exact command or `curl` for each check. Procedures only. | You are at the hardware, or you are about to cut a release. |
| [`docs/operations/ci.md`](operations/ci.md) | What CI runs, what each job costs, and why the caches are hand-keyed. | You want to know whether CI will catch something, or what it will cost. |
| [`AUDIT.md`](../AUDIT.md) | A dated repository audit: scorecard, per-finding evidence and fix, tool coverage, and the review verdicts. | You want the defect list, or you are about to re-run the audit and must not duplicate it. |
| [`docs/THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) | Vendored third-party notices. Checked by CI with `test -f`. | Legal, or adding a vendored dependency. |

## Not documentation — read by code or CI, do not move

Moving any of these is a build or CI break, not a link cleanup (`AG-REPO-31`).

| Path | Who reads it |
| --- | --- |
| [`docs/example_config.json`](example_config.json) | A working configuration, imported unchanged by `cc-config`'s test. |
| [`crates/cc-parity/scenarios/`](../crates/cc-parity/scenarios) | The scenario set `just parity-test` runs. |
| [`docs/operations/image-size/size-baseline.json`](operations/image-size/size-baseline.json) · [`size-records.jsonl`](operations/image-size/size-records.jsonl) | The image-size budget, read by `just/size.just`. Never read these by eye. |
