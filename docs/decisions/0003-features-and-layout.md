# 0003: Crate layout and build profiles

Status: accepted, 2026-10

## Decision

Logic in host-tested crates, hardware in `firmware/` only:

- `cc-core` (machine, config, scale, switches; no_std, no time source), `cc-pid`, `cc-api`, `cc-web`, `cc-mqtt`, `cc-display`.
- One crate per sensor or protocol (`cc-tsic`, `cc-onewire`, `cc-hx711`, `cc-abp2`, `cc-oled`, `cc-blescale`) over `embedded-hal` traits, tested with simulated buses.

Cargo features select optional parts: `mqtt`, `display`, `hx711-scale`, `ble-scale`, `pressure`, combined into profiles `standard` (default: mqtt + display), `full` (everything), `minimal` (none) and `bench` (full + debug route). Chip features (`esp32`) prepare S3/C6.

Parameters of absent parts stay in the registry: config files and the UI work across profiles.

## Consequences

- `just check` lints the smallest and the largest feature set; combinations in between are not built in CI.
- The rotary encoder and zero-crossing input are deferred to optional features (docs/todo.md).
