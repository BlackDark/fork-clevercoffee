# CleverCoffee (fork) — ESP32 espresso-machine firmware

This is a fork of [CleverCoffee](https://github.com/rancilio-pid/clevercoffee)
with internal refactorings and new features. It currently carries **two
firmwares**, and picking the wrong one is the most common way to waste an
afternoon here.

**The machine is an ESP32-DevKitC V4 / ESP32-WROOM-32E** — the original ESP32,
Xtensa LX6. There is no S3, C3 or C6 in this project. `esp32_usb` in the
PlatformIO environment name refers to a USB-to-UART cable; the chip has no
native USB.

| | **Rust** — the port | **C++** — the parity oracle |
| --- | --- | --- |
| Source | `crates/cc-*`, `justfile`, `rust-toolchain.toml` | `src/`, `include/`, `lib/`, `test/`, `platformio.ini` |
| Build | `just setup` once, then `just build-esp32` | `pio run -e esp32_usb` |
| Gate | `just check` (no hardware) · `just gate` (full) | `pio run --target format -e esp32_usb -s` · `pio test -e native_test` |
| Status | the port; booting, regulating and serving on hardware | what `release.yml` publishes today |
| What works | [`docs/status.md`](docs/status.md) | [`docs/archive/cpp/REPOSITORY_SUMMARY.md`](docs/archive/cpp/REPOSITORY_SUMMARY.md) |

> ### ⚠️ Never flash the C++ image
>
> It runs **its own control loop** on a powered, wired machine — pump, three-way
> valve, a 2 kW boiler. Flashing it is not a build step, it is putting the real
> machine's control loop back on the board. The same is why the C++ parity
> baseline is deliberately empty. What may and may not be done to that tree is
> stated once, in [`docs/cpp-oracle.md`](docs/cpp-oracle.md), and in the numbered
> `AG-ORACLE-*` rules.

## Start here

| Your situation | Read |
| --- | --- |
| **New here** | [`README.md`](README.md) (this page), then [`docs/index.md`](docs/index.md) for the map |
| **I am about to change firmware behaviour** | [`AGENTS.md`](AGENTS.md) — the rulebook — then [`docs/handbook/differences.md`](docs/handbook/differences.md) |
| **I am at the machine** | [`docs/operations/integration-checklist.md`](docs/operations/integration-checklist.md) |
| **I want the history** | [`docs/archive/README.md`](docs/archive/README.md) |

**[`docs/index.md`](docs/index.md) is the map**: one row per document, grouped by
those four situations. [`docs/status.md`](docs/status.md) is the only page in
this repository permitted to claim what works — it is dated, owned, and every
line in it is a pointer to a commit or a measurement. Agent rules are numbered
in [`AGENTS.md`](AGENTS.md); [`CLAUDE.md`](CLAUDE.md) is a pointer to it.

## Building the Rust firmware

```sh
just setup        # once: mise tools, the Espressif Xtensa toolchain, the web UI
just check        # host gate — fmt, clippy, rustdoc, tests, parity — no hardware
just gate         # the above + device clippy, Xtensa release build, size budget
```

- **`just check`** is the fast gate. It runs on **stable**, not the Espressif
  `esp` toolchain, because the crates it covers are plain `#![no_std]` Rust that
  touches no Xtensa pin — which is also what makes the workspace's
  `rust-version = "1.82"` claim verifiable. It needs no device toolchain and no
  hardware.
- **`just gate` is the real gate.** `just check` does **not** compile
  `cc-hal-esp32` or `cc-firmware`, so any change that can affect the firmware
  image needs `just gate`: device clippy, the Xtensa release build, and the size
  budget.
- `channel = "esp"` in `rust-toolchain.toml` is an Espressif **nightly fork**,
  installed by `espup` — mise deliberately does not install Rust.
  `just doctor` checks the device toolchain; `just doctor-host` checks only what
  a host-only machine can assert.
- **The web UI must be built before the firmware will link.**
  `cc-hal-esp32/build.rs` deliberately panics without
  `ui/packages/frontend/dist`; `just build-esp32` depends on the `ui:` recipe.

Flashing needs a board, so it is never a default recipe:

```sh
just identify <port>     # ALWAYS first — confirm the chip before writing to it
just flash <port>
```

What CI runs and what it costs: [`docs/handbook/ci.md`](docs/handbook/ci.md).

## Building the C++ firmware (the parity oracle)

Only if you have decided to. See the warning above first.

```sh
pio run -e esp32_usb
pio test -e native_test
```

`esptool.py --chip esp32 merge_bin -o merged-flash.bin --flash_mode dio --flash_size 4MB 0x1000 bootloader.bin 0x8000 partitions.bin 0x10000 firmware.bin`

The C++ tree is the **parity baseline for the whole migration**: it is not being
deleted and not being cleaned up, and the port's test suite is measured against
it. Every deliberate divergence is recorded in
[`intentional-diffs.md`](docs/rust-migration/intentional-diffs.md) — start there
when a behaviour looks wrong.

## What this fork changed

- Internal refactoring of the configuration and how data is stored
- Internal refactoring to centralize the global variables used across the code
- Persistence of configuration across updates and flashes, in the ESP32's
  dedicated **NVS** flash blocks rather than the filesystem
  ([ESP-IDF docs](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/storage/nvs_flash.html))
- A brand new React UI, precompiled and served from flash
- Hostname configuration during setup, for direct access by DNS name
- OTA in the web app: upload a binary or give it a URL

### New frontend / UI

- Dark mode
- New home page — ![home](.github/images/frontend-home.webp)
- Conditional parameter selection with change detection
  ![parameters](.github/images/frontend-param.webp)
- System page — ![system](.github/images/frontend-system.webp)

## How to try it out

Build the binaries and flash your device, as the C++ documentation describes.
Easy going — but flash the **Rust** image.
