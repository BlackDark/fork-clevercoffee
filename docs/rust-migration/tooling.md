# Tooling

All tools are pinned in `.mise.toml`. A fresh clone with only `mise` and `just` on the host
reaches a working build after `just setup`.

Cross-links: [task-list.md](task-list.md), [compatibility-matrix.md](compatibility-matrix.md),
[decision-record.md](decision-record.md).

---

## Pinned tools

| Tool | Version | Backend | Notes |
| --- | --- | --- | --- |
| `just` | 1.58.0 | mise registry | recipe runner |
| `espflash` | 4.6.0 | mise `http` backend, sha256-pinned | flash, save-image, write-bin, board-info, monitor |
| `espup` | 0.17.1 | mise `http` backend, sha256-pinned | installs the Xtensa toolchain |
| `node` | 24.21.0 | mise core | frontend build |
| `pnpm` | 11.25.0 | mise core | frontend build |
| `python` | 3.14.7 | mise core | PlatformIO, removed in the final phase |
| `clang-format` | 23.1.2 | mise core | C++ formatting, removed in the final phase |
| `rustc` (Xtensa) | 1.97.0.0 Espressif fork | via `espup` | ESP32 and ESP32-S3 |
| `rustc` (RISC-V) | stable 1.98.1 | `rustup` | ESP32-C6 |
| `esp-hal` | 1.2.2 | crates.io | |
| `esp-rtos` | 0.4.0 | crates.io | replaces `esp-hal-embassy` |
| `esp-radio` | 1.0.0-beta.1 | crates.io | Wi-Fi and BLE |
| `esp-storage` | 0.10.0 | crates.io | flash and partitions |
| `esp-bootloader-esp-idf` | 0.6.0 | crates.io | partition table and OTA slot switching |
| `esp-alloc` | 0.11.0 | crates.io | heap |
| `esp-backtrace` | 0.20.0 | crates.io | panic handler |
| `esp-println` | 0.18.0 | crates.io | log transport |
| `embassy-executor` | 0.10.0 | crates.io | |
| `embassy-net` | 0.9.1 | crates.io | TCP/IP |
| `serialport` | 4.10 | crates.io | host-side provisioning tool |

### Why `http` and not `ubi` for espflash and espup

`espflash` is not in the mise registry, and the `ubi` backend fails against its releases: the
release ships one zip per target triple plus a second family for `cargo-espflash`, and the backend
downloads a file whose contents do not match the executable it is looking for
(`could not find any files matching [espflash*] in the downloaded archive file`). The `http`
backend pins the exact `x86_64-unknown-linux-gnu` asset by sha256, which is auditable and
reproducible. `espup` is installed the same way.

This pins the host architecture. A macOS or aarch64 host needs the matching asset hash added; the
recipe names the platform it is verified on and that limitation is stated rather than hidden.

### The `espup` step cannot be a mise tool

`espup` installs a rustup toolchain named `esp` containing the Espressif Xtensa fork of rustc,
plus the Xtensa LLVM fork and the Xtensa GCC toolchain, and it writes an export file that sets
`PATH` and `LIBCLANG_PATH`. None of that is expressible as a mise tool, so `just setup` runs
`mise install` and then `just espup-install`, which runs `espup install` inside the mise
environment. The recipes source the generated `.espup-env.sh`.

## Recipes

| Recipe | What it does |
| --- | --- |
| `just setup` | `mise install` then `just espup-install` |
| `just espup-install` | idempotent toolchain install |
| `just fmt` / `just fmt-check` | `cargo fmt` |
| `just lint` | `cargo clippy` with warnings denied, over the host-testable crates only |
| `just deps` | the dependency-direction check, `tools/check-deps.py` |
| `just secrets` | the committed-secret scan, `tools/check-secrets.py` |
| `just test` | host tests for the eleven host-testable crates |
| `just check` | fmt-check, lint, test, deps, secrets |
| `just check-fw <target>` | clippy the firmware and the board crates for one target |
| `just build <target>` | build and produce `target/fw-<target>.bin` |
| `just flash <target> <port>` | erase flash, write bootloader, partitions and image |
| `just monitor <port>` | attach the log console |
| `just wifi <port>` | write `WIFI_SSID` and `WIFI_PASS` from `.env` over USB |
| `just config-import <port> <file>` | write and validate a JSON config over USB |
| `just factory-reset <port>` | clear the config region |
| `just status <port>` | read runtime status, no secrets |
| `just frontend-*` | frontend build, lint, test |
| `just spike` | rebuild all capability spikes for all three targets |

### Target and port are always explicit

Every hardware-touching recipe takes both a target and a port, or a port alone where only one
port can be present. There is no default port and no autodetection of the chip for flashing.

`just flash` refuses to write when the connected chip is not the target. It reads the chip with
`espflash board-info` and exits non-zero on a mismatch:

```
$ just flash esp32c6 /dev/ttyACM0
refusing to flash: port /dev/ttyACM0 reports esp32s3 but the target is esp32c6
```

It also states, before writing, that the partition table is replaced and all stored data is
lost, and requires the operator to type `FLASH`. Verified locally with no device attached: the
guard reports `could not read the chip on /dev/null` and exits non-zero rather than proceeding.

### Flash methods per chip

| Target | Transport | Tool |
| --- | --- | --- |
| esp32 | USB-serial bridge (UART), `esp-prog` compatible | `espflash flash --chip esp32 --port <port>` |
| esp32s3 | native USB Serial/JTAG | `espflash flash --chip esp32s3 --port <port>` |
| esp32c6 | native USB Serial/JTAG | `espflash flash --chip esp32c6 --port <port>` |

`espflash` auto-resets and auto-detects the stub on all three, so the recipe is uniform; the
transport differs, not the command.

### `just wifi` and `just config-import`

Both work on a freshly flashed device and on a running one, because the provisioning task runs
from boot and is not gated on machine state.

Both read credentials from `.env` inside the recipe and never print them. `.env` is gitignored.
The only output is a status line: `OK WIFI_SET credentials written...` or `FAIL WIFI_PASS ...`.
The host tool is `tools/provision`, a Rust binary depending only on `serialport`, `base64` and
`crc32fast`, so mise manages it and it needs no Python package.

Verified locally with no device attached: the tool enumerates ports, opens a port when permitted,
and on a port it cannot open it reports `FAIL OPEN_PORT cannot open /dev/ttyS0: Permission denied`
and exits non-zero. Nothing is written to a temporary file at any point.

## Host requirements mise cannot cover

| Requirement | Why | Install |
| --- | --- | --- |
| `rustup` | installs the `esp` and `stable` toolchains | upstream installer |
| A C toolchain (`gcc`, `build-essential`) | `espup` and the linker need it | distribution package manager |
| `pkg-config` | some crates link against system libraries | distribution package manager |
| Serial port access | `just wifi`, `just config-import`, `just flash` | group membership in `dialout`, or `sudo` |
| `libudev` | only for the `serialport` crate's udev feature | not required: the crate is built with `default-features = false` and the port is given by name |

PlatformIO and the C++ toolchain are host requirements **only** until the final phase deletes
them.

## CI

`.github/workflows/rust.yml`, added in T-01. Three jobs, `permissions: contents: read`
repository-wide, every third-party action pinned by commit SHA.

| Job | Runs | Gate |
| --- | --- | --- |
| `host (fmt)` | `cargo fmt --all -- --check` | fails on any diff |
| `host (lint)` | `cargo clippy --workspace` with the four chip crates excluded, `-D warnings` | no blanket allows |
| `host (test)` | `cargo test --workspace` with the same exclusions | all tests pass |
| `host (deps)` | `python3 tools/check-deps.py` | the layering table holds |
| `host (secrets)` | `python3 tools/check-secrets.py` | no committed credential |
| `firmware` | `cargo clippy` then two builds per chip: normal and `mock-actuators` | all three chips compile |
| `spikes` | `just spike` | all eight spike configurations still build |

The Xtensa and RISC-V toolchains are cached on `Cargo.lock` plus `rust-toolchain.toml`, and the
spike job has its own cache keyed on the spike lockfiles, because the spikes pin their own
dependency versions.

Two details that are not obvious and cost time if they are got wrong:

- **The host jobs must exclude the `bsp-*` crates.** Cargo unifies features across a
  `--workspace` build, so including all members enables three chips at once and fails inside
  `esp-metadata-generated`. The justfile and the workflow exclude the same four crates.
- **`build-std` is set per recipe, not in `.cargo/config.toml`.** A global `build-std` also
  applies to host builds and then collides with the host's prebuilt `core`. The chip recipes
  export `CARGO_UNSTABLE_BUILD_STD=core,alloc` instead.

## Known tooling limitations

| Limitation | Consequence |
| --- | --- |
| `espflash` and `espup` are pinned to the `x86_64-unknown-linux-gnu` asset | another host architecture needs the matching sha256 added to `.mise.toml` |
| The log transport follows the board, not the operator's port choice | on an S3 or C6, logs are on the native USB port even when provisioning runs over the UART bridge. Cargo allows one version of a crate per build, so this could not be a second dependency. |
| `cargo test --workspace` cannot include the chip crates | the three chips are build-verified by CI and by `just check-fw`, not by tests |
| `esp-rtos` 0.4.0 is pre-1.0, and `esp-radio` is `1.0.0-beta.1` | both may change shape; the spikes exist to catch that |

## Toolchain commands that actually work

Recorded because each one cost a build cycle to find.

| Command | Note |
| --- | --- |
| `cargo build -Zbuild-std=core,alloc --release --target xtensa-esp32-none-elf` | `-Zbuild-std` is required: the Xtensa target has no prebuilt `rust-std` |
| `esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0)` | the published `esp-rtos` 0.4.0 takes a second argument, unlike the `main`-branch example |
| `I2c::new(i2c, config)?.with_sda(pin).with_scl(pin)` | the pin setters are on `I2c`, not on `Config` |
| `embassy_net::new(driver, config, &mut resources, seed)` | argument order is driver, config, resources, seed |
| `esp_hal::usb::usb_serial_jtag::UsbSerialJtag::new(peripherals.USB_DEVICE).into_async().split()` | the peripheral is `USB_DEVICE`, not `USBTMC`, and the module is under `usb`, not at the crate root |
| `espflash save-image --chip esp32 <elf> <out>` | produced a 99 728-byte image from the 2 571 412-byte ESP32 ELF |
