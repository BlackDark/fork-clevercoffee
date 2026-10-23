# 0002: Toolchain and commands

Status: accepted, 2026-10

## Decision

- Host crates: stable Rust from `rust-toolchain.toml` (rustup). The classic ESP32 is Xtensa, which upstream Rust does not support: `firmware/` is its own Cargo workspace with `rust-toolchain.toml` channel `esp`, installed by `espup` (pinned in `mise.toml`, version pinned in `just setup`). Precompiled; no LLVM build.
- Other tools (just, node, pnpm, espup, espflash) from `mise.toml`.
- `just` owns every command; `mise run check` calls `just check` so the agent-presets convention holds.
- Two workspaces keep host builds fast and free of `build-std`; firmware depends on the host crates by path.
- Wokwi simulation is dropped: the host tests cover logic and layout; the bench covers hardware.

## Consequences

- CI installs espup's toolchain once per cache key (~1 min cold).
- `just` recipes are the documentation of how things are run; docs link to them instead of repeating commands.
