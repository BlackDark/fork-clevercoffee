# Forked CleverCoffee code

This is fork from [CleverCoffee](https://github.com/rancilio-pid/clevercoffee) which includes some internal refactorings and new features for testing and ideas.

`esptool.py --chip esp32 merge_bin -o merged-flash.bin --flash_mode dio --flash_size 4MB 0x1000 bootloader.bin 0x8000 partitions.bin 0x10000 firmware.bin`

> Note: the command above omits `littlefs.bin`, so a device flashed this way has no
> web UI. See `.github/workflows/release.yml` for the full set of images and offsets.

## Rust migration (in progress)

The firmware is being rewritten in Rust. The C++ firmware in this repository keeps
building and remains the reference for behaviour while that happens.

Start here: **[docs/rust-migration/](docs/rust-migration/)** — inventory,
compatibility matrix, architecture, tooling and task list, plus
[ADR 0004](docs/adr/0004-rust-migration-platform-selection.md) (platform choice) and
[ADR 0005](docs/adr/0005-no-backward-compatibility-usb-flash-migration.md)
(no backward compatibility).

**There is no upgrade path from the C++ firmware to the Rust one.** Migration is
manual and deliberate:

1. **Export your configuration first** — in the old web UI, download `config.json`.
   Nothing is read from the old device automatically, so skipping this step loses
   your settings with no way to recover them.
2. Flash the Rust firmware **over USB**. OTA from the old firmware is not supported
   and the new firmware refuses to boot on the old partition layout.
3. Provision Wi-Fi over USB.
4. Import the `config.json` you exported.

### Prerequisites

Toolchain versions are pinned in [`.mise.toml`](.mise.toml), with two exceptions you
must install yourself first:

- **[mise](https://mise.jdx.dev)** — manages node, pnpm, python, clang-format, just.
- **[rustup](https://rustup.rs)** — **required before `just setup`, and mise cannot
  manage it.** mise's own `rust` tool installs a non-rustup Rust whose `cargo` shim
  shadows rustup's; `rust-toolchain.toml` is then ignored and the Xtensa build fails
  with the misleading error `can't find crate for `core``. If you already have
  `mise use rust`, remove it with `mise unuse rust`.

  ```bash
  brew install rustup     # or: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup default stable   # populates ~/.cargo/bin with the cargo and rustc proxies
  ```

  Homebrew's formula installs only `/opt/homebrew/bin/rustup`; the `cargo` and
  `rustc` proxies live in `~/.cargo/bin`, which must be on your `PATH`.

Then:

```bash
just setup     # Xtensa Rust fork via espup, pinned ESP tools, python venv
just doctor    # verifies all of the above, including the mise/rustup conflict
just test      # host tests, no hardware needed
```

`just --list` shows everything. Target and port are always explicit — there is no
default board and no default serial port. Full detail in
[docs/rust-migration/tooling.md](docs/rust-migration/tooling.md).

## Changes made

- Internal refactoring of the configuration and how data is stored
- Internal refactoring to kind of centralize the global variables which are used across the code
  - Easier to get used to the code to see what types of data are shared across files. Still not the best but maybe some starting point. Probably best this should be located in some kind of Singleton so we get some power to test things without having to actually always flash the whole device
- Persistence of configurations across updates and flashes including new UI updates
  - Data is stored in the so called `NVS` which for ESP32 are just dedicated flash block and not the default FileSystem range
  - https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/storage/nvs_flash.html
- Brand new UI based on react, precompiled and fast
- Hostname configuration during setup of the ESP for direct access with new DNS name
- OTA support in the web app: update by uploading a binary or providing a link to the binary

## New Frontend / UI

- Dark mode support
- New HomePage
![alt text](.github/images/frontend-home.webp)
- Conditional parameter selection with change detection
![alt text](.github/images/frontend-param.webp)
- System Page
![alt text](.github/images/frontend-system.webp)

## How to try out?

Like before and described in the documentation you can just build the binaries and flash on your device.
Easy going.
