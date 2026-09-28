# Rust migration: C++ to ESP32 Rust firmware

The C++ Arduino firmware is being replaced by a Rust firmware on `esp-hal` with `embassy`,
targeting the original ESP32, ESP32-S3 and ESP32-C6. This directory holds the plan.

**No hardware was available for this planning run.** Every target is `build-verified`, none is
`device-verified`. See [verification-levels.md](verification-levels.md).

## Read in this order

1. [inventory.md](inventory.md) — what the C++ firmware does, traced from `setup()` to the main
   loop: hardware, timing, state machine, libraries, workflows, problem features.
2. [api-contract.md](api-contract.md) — every web route, payload and status code, derived from
   the C++ handlers, with the 26 places the shipped OpenAPI spec disagrees with them.
3. [config-export-schema.md](config-export-schema.md) — every config field with its type, range
   and default, and its reconciliation with the repository's two JSON files.
4. [defects-register.md](defects-register.md) — 43 C++ defects with severity and the fix the Rust
   port implements.
5. [decision-record.md](decision-record.md) — `esp-hal` versus `esp-idf-hal`, the DS18B20 driver,
   the HTTP server, USB provisioning, storage and OTA.
6. [compatibility-matrix.md](compatibility-matrix.md) — what is verified, per target.
7. [architecture.md](architecture.md) — execution model, tasks, crates, storage, web backend,
   config import, USB provisioning, and how each defect is fixed.
8. [task-list.md](task-list.md) — 22 tasks in 7 phases with acceptance criteria.
9. [tooling.md](tooling.md) — `.mise.toml`, `justfile`, CI, and the host requirements.
10. [verification-levels.md](verification-levels.md) — the evidence standard every claim uses.

## How this connects to the rest of the repo

| Path | Relationship |
| --- | --- |
| [../state-machine-architecture.md](../state-machine-architecture.md) | the C++ state machine reference that `domain` is ported from |
| [../display-architecture.md](../display-architecture.md) | the C++ display split that `display` is ported from |
| [../integration-tests.md](../integration-tests.md) | the manual checklist; extended by the migration phases |
| [../api/openapi.yaml](../api/openapi.yaml) | regenerated from `api-contract.md` in task T-22 |
| [../CONFIG_REFERENCE.md](../CONFIG_REFERENCE.md) | regenerated from the schema crate in task T-22 |
| [../../spikes/](../../spikes) | the buildable capability spikes behind the compatibility matrix |
| [../../justfile](../../justfile) | every recipe, including `just setup` and `just spike` |
| [../../.agents/skills/esp32-rust-migration/SKILL.md](../../.agents/skills/esp32-rust-migration/SKILL.md) | the execution skill an agent follows to work through the task list |

## The one-paragraph version

The current firmware is a 26 000-line Arduino program on a single unsynchronised loop with a
10 ms ISR driving the heater and a separate network task from AsyncTCP. It has three critical
defects: an OTA can leave the pump and valve energized, the heater shutdown bookkeeping is inert
because a flag is never set, and a disconnected temperature sensor is never detected, so the PID
can sit at 100 percent indefinitely. The API is 30 routes with no tests, its authentication
middleware never authenticates, and it exports four plaintext secrets. The plan ports the
functionality to Rust on `esp-hal` with `embassy`, fixes each defect by construction, owns the
HTTP server and the DS18B20 driver because no adequate Rust crate exists, replaces NVS and the
Wi-Fi portal with a versioned config region and a USB provisioning channel, and drops the OTA and
SSRF surfaces entirely. The only bridge to the old firmware is the config JSON: export, flash
over USB, import.
