# Documentation index

One row per document. Find the row that matches your situation.

| Document | What it is for | Read it if |
| --- | --- | --- |
| **New here** | | |
| [`README.md`](../README.md) | What this repository is, the two firmwares, the three build commands. | You just opened the repository. |
| [`docs/index.md`](index.md) | This page: one row per document, grouped by situation. | You want to know which document to open. |
| [`AGENTS.md`](../AGENTS.md) | **The rulebook.** Numbered, scoped rules (`AG-REPO-*`, `AG-ORACLE-*`, `AG-RUST-*`). Citable, never restated. | Before you change anything. |
| [`CLAUDE.md`](../CLAUDE.md) | A five-line pointer to `AGENTS.md`, deliberately. | Your tool looks for it. Nothing else. |
| [`docs/status.md`](status.md) | **The only page permitted to claim what works.** Dated, owned, every line a pointer to a commit or a measurement. | You want to know what this firmware actually does today. |
| [`docs/cpp-oracle.md`](cpp-oracle.md) | The C++ tree is frozen: where it lives, why, what may still be built from it, what must never be done to it. Includes the pin table. | You are about to touch, build, or flash anything in `src/` or `include/`. |
| [`docs/rust-migration/README.md`](rust-migration/README.md) | "Where the migration actually is" — the navigation page for the Rust port. | You are new to the Rust side and want one map. |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | Which firmware, how to run the formatter, and the C++ style rules. | You are about to open a pull request. |
| [`CONFIG_REFERENCE.md`](../CONFIG_REFERENCE.md) | Every configuration key, its type, range, default and which firmware reads it. | You need a parameter name or its bounds. |
| **Changing firmware behaviour** | | |
| [`docs/handbook/differences.md`](handbook/differences.md) | **The one-page answer to "will this surprise me?"** Every behavioural difference from the C++, filed by how a reader would meet it. | Anything looks wrong and you need to know whether it is intentional. |
| [`docs/rust-migration/intentional-diffs.md`](rust-migration/intentional-diffs.md) | The ledger: for each divergence, the C++'s behaviour, the port's, the reasoning, and what pins it. **Read by `cc-parity` at a hard-coded path — do not move it or split its `ledger` blocks.** | You are justifying a behaviour change and need the reasoning, not the summary. |
| [`docs/handbook/display.md`](handbook/display.md) | Entry point for the display: which of the three display documents to read, and the layout rules condensed. | You are changing anything on the 128×64 screen. |
| [`docs/handbook/display-modern-layout.md`](handbook/display-modern-layout.md) | The binding layout rules: font→pixel mapping, fixed-width numeric fields, the row maps, bar/label pairing. | You are placing pixels. Blocking if violated. |
| [`docs/handbook/display-architecture.md`](handbook/display-architecture.md) | How the **C++** renders: frame lifecycle, the deferred-flush buffer truth table, shared-vs-template ownership. | You need to know when a frame reaches the panel, or why SSE stalls. |
| [`docs/handbook/display-parity.md`](handbook/display-parity.md) | The three display checks, what each caught, and — plainly — **what they do not prove**. | You are about to claim the display is correct, or regenerating a golden. |
| [`docs/adr/0001-display-subsystem-architecture.md`](adr/0001-display-subsystem-architecture.md) | Accepted: one render pipeline, shared defaults with template overrides, one source of truth for thresholds. | You want the *why* behind the display structure. |
| [`docs/adr/0002-wifi-logging-ota-memory-architecture.md`](adr/0002-wifi-logging-ota-memory-architecture.md) | Accepted: Wi-Fi logging, OTA admission, and the memory budget. | You are touching networking, OTA, or heap. |
| [`docs/adr/0003-state-machine-hardware-control-contract.md`](adr/0003-state-machine-hardware-control-contract.md) | Accepted: the pump/valve/heater ownership contract — energise on entry, reinforce on update, release on exit. | **Adding a state, or touching anything that moves water.** |
| [`docs/rust-migration/09-cpp-findings.md`](rust-migration/09-cpp-findings.md) | The per-feature catalogue of every bug and ambiguity found in the C++, each pinned by a named parity test. Duplicate section numbers are evidence a finding was corrected — do not renumber. | You are porting or auditing a feature and want the traps. |
| [`docs/rust-migration/08-recovered-oracle.md`](rust-migration/08-recovered-oracle.md) | The only surviving record of a Rust firmware that ran on this board. **Normative** for the fail-closed `LOW_TRIGGER` rule. | You touch heater-relay pin selection. |
| [`docs/rust-migration/01-feature-inventory.md`](rust-migration/01-feature-inventory.md) | What the C++ does, feature by feature, and what the port therefore has to have. | You need the scope, not the narrative. |
| [`docs/rust-migration/02-research-compatibility-matrix.md`](rust-migration/02-research-compatibility-matrix.md) | Per-dependency evidence: crate exists, licence, MSRV, what was verified and what was not. | You add or evaluate a crate. |
| [`docs/rust-migration/04-target-architecture.md`](rust-migration/04-target-architecture.md) | The intended crate boundary and startup/shutdown contract. Partly superseded — check what it says before trusting it. | You want the intended shape, not the shipped one. |
| [`docs/rust-migration/10-scenario-format.md`](rust-migration/10-scenario-format.md) | The scenario-file format `cc-parity` and the display oracle both parse, written twice on purpose. | You add a parity or display scenario. |
| **At the machine** | | |
| [`docs/operations/integration-checklist.md`](operations/integration-checklist.md) | **The runnable pre-release checklist**, section by section, with the exact command or `curl` for each check and the recorded failure modes. | You are at the hardware, or you are about to cut a release. |
| [`docs/rust-migration/scenarios/`](rust-migration/scenarios) | The 17 parity scenario files, read by `cc-parity`. **Not documentation — do not move.** | You are running `just parity`. |
| [`docs/rust-migration/baseline/`](rust-migration/baseline) | Deliberately **empty**, and the README says why. Capturing a baseline means flashing the C++ onto a powered, wired machine. | You wondered why `just parity` reports `BASELINE-MISSING`. |
| **Reading history** | | |
| [`docs/archive/README.md`](archive/README.md) | **The archive rule.** Preserved, not a source of truth; any claim quoted out must be re-verified against live evidence. Says what is in the archive and why. | Before you quote anything dated. |
| [`docs/archive/migration/`](archive/migration) | The migration's own planning: ADR-0004, tooling, the R0–R4 task list, the size budget, two findings/plan sessions. | You want to know why a decision was made, or what a task was. |
| [`docs/archive/cpp/`](archive/cpp) | Documents that describe the **C++ firmware's implementation**: its repository summary, Wokwi setup, the backflush-reminder design, the C++ state machine. | You are working in `src/`/`include/` and want the old map. |
| [`docs/rust-migration/32-findings-2026-10-03.md`](rust-migration/32-findings-2026-10-03.md) | The independent review's findings, one per row, with status. The most recent hard-eyed record. | You want to know what has already been looked at and found wanting. |
| [`docs/handbook/ci.md`](handbook/ci.md) | What CI runs, what each job costs, and why the caches are hand-keyed. | You want to know whether CI will catch something, or what it will cost. |
| [`docs/THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) | Vendored third-party notices. Checked by CI with `test -f`. | Legal, or adding a vendored dependency. |
| **Agent-facing** | | |
| [`.agents/skills/esp32-rust-migration/SKILL.md`](../.agents/skills/esp32-rust-migration/SKILL.md) | The execution procedure for an agent picking up migration work. | An agent is resuming this migration. |
| [`.agents/skills/esp32-rust-migration/checklists.md`](../.agents/skills/esp32-rust-migration/checklists.md) | Per-phase checklists the agent runs. | An agent is closing out a phase. |
| [`.agents/skills/esp32-rust-migration/notes.md`](../.agents/skills/esp32-rust-migration/notes.md) | Session notes and working records. | An agent needs the history of a specific session. |
| **Not documentation — read by code or CI, do not move** | | |
| [`docs/example_config.json`](example_config.json) | A working configuration, imported unchanged by `cc-config`'s test. | You need a valid config file. |
| [`docs/rust-migration/size-baseline.json`](rust-migration/size-baseline.json) · [`size-records.jsonl`](rust-migration/size-records.jsonl) | The image-size baseline and its append-only history, read by `just/size.just`. | Never. `just size-check` reads them. |
| [`ui/packages/frontend/README.md`](../ui/packages/frontend/README.md) · [`ui/packages/mock-server/README.md`](../ui/packages/mock-server/README.md) | The web UI's own build and test instructions. | You are changing the React app. |
