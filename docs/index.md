# Documentation index

One row per document. Find the row that matches your situation.

| Document | What it is for | Read it if |
| --- | --- | --- |
| **New here** | | |
| [`README.md`](../README.md) | What this repository is and how to build, gate and flash it. | You just opened the repository. |
| [`docs/index.md`](index.md) | This page: one row per document, grouped by situation. | You want to know which document to open. |
| [`AGENTS.md`](../AGENTS.md) | **The rulebook.** Numbered, scoped rules (`AG-REPO-*`, `AG-DISPLAY-*`, `AG-RUST-*`). Citable, never restated. | Before you change anything. |
| [`CLAUDE.md`](../CLAUDE.md) | A five-line pointer to `AGENTS.md`, deliberately. | Your tool looks for it. Nothing else. |
| [`docs/status.md`](status.md) | **The only page permitted to claim what works.** Dated, owned, every line a pointer to a commit or a measurement. | You want to know what this firmware actually does today. |
| [`docs/hardware/pins.md`](hardware/pins.md) | The GPIO map, the two traps in it, and what the table is transcribed from. | You are touching a pin, a relay, or a sensor. |
| [`docs/history/README.md`](history/README.md) | "Where the migration actually is" — the navigation page for the Rust port. | You are new to the Rust side and want one map. |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | How to run the formatter and the gates, and the Rust style rules. | You are about to open a pull request. |
| [`config/reference.md`](config/reference.md) | Every configuration key, its type, range, default and which firmware reads it. | You need a parameter name or its bounds. |
| **Changing firmware behaviour** | | |
| [`docs/differences.md`](differences.md) | **The one-page answer to "will this surprise me?"** Every behavioural difference from the C++, filed by how a reader would meet it. | Anything looks wrong and you need to know whether it is intentional. |
| [`docs/history/divergences.md`](history/divergences.md) | The ledger: for each divergence, the C++'s behaviour, the port's, the reasoning, and what pins it. **Read by `cc-parity` at a hard-coded path — do not move it or split its `ledger` blocks.** | You are justifying a behaviour change and need the reasoning, not the summary. |
| [`docs/display/overview.md`](display/overview.md) | Entry point for the display: which of the three display documents to read, and the layout rules condensed. | You are changing anything on the 128×64 screen. |
| [`docs/display/layout-rules.md`](display/layout-rules.md) | The binding layout rules: font→pixel mapping, fixed-width numeric fields, the row maps, bar/label pairing. | You are placing pixels. Blocking if violated. |
| [`docs/display/rendering.md`](display/rendering.md) | How the **Rust** renderer works: frame lifecycle, shared-vs-template ownership, the I²C chunking. | You need to know when a frame reaches the panel. |
| [`docs/display/parity.md`](display/parity.md) | The three display checks, what each caught, and — plainly — **what they do not prove**. | You are about to claim the display is correct, or regenerating a golden. |
| [`docs/adr/0001-display-subsystem-architecture.md`](adr/0001-display-subsystem-architecture.md) | Accepted: one render pipeline, shared defaults with template overrides, one source of truth for thresholds. | You want the *why* behind the display structure. |
| [`docs/adr/0002-wifi-logging-ota-memory-architecture.md`](adr/0002-wifi-logging-ota-memory-architecture.md) | Accepted: Wi-Fi logging, OTA admission, and the memory budget. | You are touching networking, OTA, or heap. |
| [`docs/adr/0003-state-machine-hardware-control-contract.md`](adr/0003-state-machine-hardware-control-contract.md) | Accepted: the pump/valve/heater ownership contract — energise on entry, reinforce on update, release on exit. | **Adding a state, or touching anything that moves water.** |
| [`docs/history/cpp-findings.md`](history/cpp-findings.md) | The per-feature catalogue of every bug and ambiguity found in the C++, each pinned by a named parity test. Duplicate section numbers are evidence a finding was corrected — do not renumber. | You are porting or auditing a feature and want the traps. |
| [`docs/history/recovered-oracle.md`](history/recovered-oracle.md) | The only surviving record of a Rust firmware that ran on this board. **Normative** for the fail-closed `LOW_TRIGGER` rule. | You touch heater-relay pin selection. |
| [`docs/history/feature-inventory.md`](history/feature-inventory.md) | What the C++ does, feature by feature, and what the port therefore has to have. | You need the scope, not the narrative. |
| [`docs/history/dependency-evaluation.md`](history/dependency-evaluation.md) | Per-dependency evidence: crate exists, licence, MSRV, what was verified and what was not. | You add or evaluate a crate. |
| [`docs/history/target-architecture.md`](history/target-architecture.md) | The intended crate boundary and startup/shutdown contract. Partly superseded — check what it says before trusting it. | You want the intended shape, not the shipped one. |
| [`docs/history/scenario-format.md`](history/scenario-format.md) | The scenario-file format `cc-parity` and the display oracle both parse, written twice on purpose. | You add a parity or display scenario. |
| **At the machine** | | |
| [`docs/operations/runbook.md`](operations/runbook.md) | **The runnable pre-release checklist**, section by section, with the exact command or `curl` for each check and the recorded failure modes. | You are at the hardware, or you are about to cut a release. |
| [`docs/history/scenarios/`](history/scenarios) | The 17 parity scenario files, read by `cc-parity`. **Not documentation — do not move.** | You are running `just parity`. |
| [`docs/history/baseline/`](history/baseline) | Deliberately **empty**, and the README says why. Capturing a baseline means flashing the C++ onto a powered, wired machine. | You wondered why `just parity` reports `BASELINE-MISSING`. |
| **Reading history** | | |
| [`docs/archive/README.md`](archive/README.md) | **The archive rule.** Preserved, not a source of truth; any claim quoted out must be re-verified against live evidence. Says what is in the archive and why. | Before you quote anything dated. |
| [`docs/archive/migration/`](archive/migration) | The migration's own planning: ADR-0004, tooling, the R0–R4 task list, the size budget, two findings/plan sessions. | You want to know why a decision was made, or what a task was. |
| [`docs/archive/cpp/`](archive/cpp) | Documents that describe the **deleted C++ firmware's** implementation: its repository summary, Wokwi setup, the backflush-reminder design, the C++ state machine. **Superseded** — cite only for what the C++ did. | You want to know why a decision was made about C++ behaviour. |
| [`docs/history/review-2026-10-03.md`](history/review-2026-10-03.md) | The independent review's findings, one per row, with status. The most recent hard-eyed record. | You want to know what has already been looked at and found wanting. |
| [`docs/operations/ci.md`](operations/ci.md) | What CI runs, what each job costs, and why the caches are hand-keyed. | You want to know whether CI will catch something, or what it will cost. |
| [`docs/THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) | Vendored third-party notices. Checked by CI with `test -f`. | Legal, or adding a vendored dependency. |
| **Agent-facing** | | |
| [`.agents/skills/esp32-rust-migration/SKILL.md`](../.agents/skills/esp32-rust-migration/SKILL.md) | The execution procedure for an agent picking up migration work. | An agent is resuming this migration. |
| [`.agents/skills/esp32-rust-migration/checklists.md`](../.agents/skills/esp32-rust-migration/checklists.md) | Per-phase checklists the agent runs. | An agent is closing out a phase. |
| [`.agents/skills/esp32-rust-migration/notes.md`](../.agents/skills/esp32-rust-migration/notes.md) | Session notes and working records. | An agent needs the history of a specific session. |
| **Not documentation — read by code or CI, do not move** | | |
| [`docs/example_config.json`](example_config.json) | A working configuration, imported unchanged by `cc-config`'s test. | You need a valid config file. |
| [`docs/history/size-baseline.json`](history/size-baseline.json) · [`size-records.jsonl`](history/size-records.jsonl) | The image-size baseline and its append-only history, read by `just/size.just`. | Never. `just size-check` reads them. |
| [`ui/packages/frontend/README.md`](../ui/packages/frontend/README.md) · [`ui/packages/mock-server/README.md`](../ui/packages/mock-server/README.md) | The web UI's own build and test instructions. | You are changing the React app. |
