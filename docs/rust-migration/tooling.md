# Rust firmware tooling

**Status:** Implemented and verified on this host (A4)
**Last updated:** 2026-09-28
**Related:** [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md) · [inventory.md](inventory.md) · [compatibility-matrix.md](compatibility-matrix.md) · [architecture.md](architecture.md) · [ADR 0004](../adr/0004-rust-migration-platform-selection.md) · [task-list.md](task-list.md) · [execution skill](../../.agents/skills/esp32-rust-migration/SKILL.md)

Everything here has been run on this machine. Where something is unverified it
says so.

---

## 1. Quick start

**Install rustup first.** It is a prerequisite that mise cannot manage — see §2.2.

```bash
brew install rustup     # or https://rustup.rs
rustup default stable   # populates ~/.cargo/bin with the cargo proxy

just setup                                  # once
just doctor                                 # verify the toolchain
just test                                   # host tests — the fast loop
just build esp32                            # build firmware for a named target
just board-info /dev/cu.usbserial-204140     # what is actually connected
just flash esp32 /dev/cu.usbserial-204140    # refuses on a chip mismatch
just monitor /dev/cu.usbserial-204140
```

`just --list` is the full recipe list. **Target and port are always explicit.**
There is no default board and no default serial port, because picking one wrong
means flashing the wrong image at an actuator-driving device.

---

## 2. Toolchain, and the one trap in it

### 2.1 What manages what

| Managed by | Tools |
|---|---|
| **mise** (`.mise.toml`) | node 24, pnpm, python 3.14.7, clang-format 23.1.2, just 1.58.0 |
| **rustup** (`rust-toolchain.toml`) | the `esp` channel — the Espressif Rust fork. **Install rustup yourself; `just setup` requires it and will not install it.** |
| **espup** (pinned 0.17.1) | Xtensa Rust 1.97.0.0, `xtensa-esp-elf` GCC, Xtensa LLVM |
| **cargo-binstall**, pinned | `espflash` 4.6.0, `espup` 0.17.1, `ldproxy` 0.3.5 |
| **`.venv`**, pinned | `platformio` 6.2.0, `esp-idf-nvs-partition-gen` |
| **`esp-idf-sys`** (`ESP_IDF_VERSION`) | ESP-IDF v5.3.6, downloaded and built automatically |

### 2.2 Host requirements mise cannot manage

The task brief asks for these to be named, so:

1. **Rust itself must come from rustup, not mise, and rustup must be installed
   before `just setup` runs.** This is not a preference, it is a hard
   incompatibility that was hit and diagnosed during SPIKE-1.

   ```bash
   brew install rustup     # or https://rustup.rs
   rustup default stable
   ```

   Homebrew's formula installs only `/opt/homebrew/bin/rustup`. The `cargo` and
   `rustc` proxies live in `~/.cargo/bin`, so **that directory must be on `PATH`** —
   `rustup --version` working is not sufficient. `just setup` checks for both and
   fails with instructions rather than guessing; `just doctor` reports which `cargo`
   is actually resolving.

   mise's `rust` tool installs a non-rustup Rust and puts a `cargo` shim on `PATH`
   that shadows rustup's. With that shim active, `rust-toolchain.toml` is ignored,
   so the `esp` channel is never selected and the build fails with:

   ```
   'esp32' is not a recognized processor for this target (ignoring processor)
   error[E0463]: can't find crate for `core`
   ```

   which points at a missing target rather than at the real cause. `just doctor`
   detects this specific situation and tells you to run `mise unuse rust`.

2. **`espflash`, `espup` and `ldproxy` cannot be mise-managed either.** They are
   only available through mise's `cargo:` backend, which would require mise to own
   Rust — the very thing that breaks — and which also shims `espflash` so it
   shadows the real binary. They are installed by `just setup` via cargo-binstall
   at pinned versions instead.

3. **`curl` and a C compiler** for the rustup bootstrap and the ESP-IDF build.

4. **A filesystem that supports symlinks.** ESP-IDF cannot build without them.

5. **Roughly 5 GB of disk** for ESP-IDF, its tools and the build directory.
   `ESP_IDF_TOOLS_INSTALL_DIR = "global"` in `.mise.toml` keeps the ESP-IDF
   checkout in `~/.espressif` rather than inside the repo.

6. **`~/export-esp.sh`**, written by espup. Every recipe that touches the target
   sources it automatically; you do not need to source it by hand.

One more sharp edge, from the upstream docs and not yet hit here: if you move
`CARGO_TARGET_DIR` without also setting `CARGO_WORKSPACE_DIR`, `sdkconfig.defaults`
is **silently ignored**.

---

## 3. Workspace layout

Two Cargo workspaces, deliberately. This is what keeps the test loop fast.

```
Cargo.toml              host workspace  — cc-domain, cc-hal, cc-drivers
crates/
  cc-domain/            no_std, pure logic, all the control behaviour
  cc-hal/               no_std, platform traits, no implementations
  cc-drivers/           no_std, device drivers over embedded-hal 1.0
firmware/
  Cargo.toml            target workspace — esp32, cc-board, cc-app
  .cargo/config.toml    target triple, ldproxy, build-std, MCU, ESP_IDF_VERSION
  esp32/                the binary
  cc-board/             esp-idf implementations, pin map, NVS store
  cc-app/               task wiring, HTTP, MQTT, OTA, provisioning
```

The root workspace `exclude`s `firmware` and `research`, so `cargo test
--workspace` at the repo root **never** pulls in anything that needs the ESP
toolchain or hardware. `firmware/` depends on `../crates/*` by path — those are
dependencies, not members, which is why they can sit outside its directory.

Two consequences worth knowing:

- `.cargo/config.toml` is read from the **current directory upward**, not from
  `--manifest-path`. So every target recipe `cd`s into `firmware/` first. Using
  `--manifest-path firmware/Cargo.toml` from the repo root silently skips that
  config and fails with `can't find crate for core` — the same misleading error as
  the mise trap.
- Cargo requires workspace members to be hierarchically below the workspace root,
  which is why `cc-board` and `cc-app` live under `firmware/` rather than beside the
  host crates.

---

## 4. Recipes

| Recipe | What it does |
|---|---|
| `setup` | rustup, espup, pinned ESP tools, mise tools, `.venv` |
| `doctor` | versions for everything, target availability, and the mise-shim check |
| `fmt` / `fmt-check` | `cargo fmt` over both workspaces |
| `lint` | Clippy `-D warnings` over host crates **and** every target |
| `test` | `cargo test --workspace` — host only, no hardware, no ESP-IDF |
| `build <target>` | release build for one named target |
| `build-all` | every target in the table |
| `size <target>` | app image size vs the `app0` size parsed from `partitions_4M.csv` |
| `board-info <port>` | read-only chip identification |
| `flash <target> <port>` | chip-guarded flash |
| `monitor <port>` | serial monitor at 115200 |
| `provision <port>` | Wi-Fi credentials from `.env` into NVS over USB |
| `provision-check <port>` | build and verify a provisioning image, write nothing |
| `nvs-report <port>` | dump and summarise a device's NVS, values never printed |
| `cpp-build` / `cpp-test` | the C++ parity oracle |

### 4.1 The target table

One table, in the justfile, is the single source of truth:

```
#   target | chip | rust triple | flash method
    esp32    esp32  xtensa-esp32-espidf  espflash-uart
```

Adding a board means adding a row here and a pin-map module in `cc-board`. CI's
matrix mirrors it. **A target counts as supported only once it has been built *and*
validated on that hardware** — CI proves it compiles, not that it works.

### 4.2 The flash guard

`flash` depends on `_assert-chip`, which runs `espflash board-info` on the port,
parses the chip type and **refuses to proceed on a mismatch**:

```
$ just flash esp32 /dev/cu.usbserial-204140
==> chip check OK: esp32 v3.0 on /dev/cu.usbserial-204140
```

On a mismatch it prints what the target expects, what the port reports, and exits
non-zero without building or writing anything. It also fails closed when the port
cannot be read or the output cannot be parsed, rather than assuming.

The flash method is per-chip because it has to be. The original ESP32 has **no
native USB**: flashing, monitoring and provisioning all go through the board's
external USB-serial bridge over UART. There is no USB-serial-JTAG, no USB CDC and
no DFU on this chip, so `espflash-uart` is the only method in the table today.

### 4.3 Verified on this host

```
just doctor      -> all green: rustup 1.29.1 (homebrew), just 1.58.0,
                    mise 2026.9.15, cargo 1.97.0-nightly (1.97.0.0) resolving to
                    ~/.cargo/bin/cargo, espflash 4.6.0, rustc +esp 1.97.0-nightly,
                    xtensa-esp32-espidf available, pio 6.2.0
just fmt-check   -> clean, both workspaces
just lint        -> clean, host crates and esp32, warnings denied
just test        -> passes (skeleton crates, 0 tests yet)
just build esp32 -> Finished `release` profile in 3m 31s
just size esp32  -> app image 340,992 B = 20.0% of 1664 KiB app partition
just board-info  -> esp32 revision v3.0
just provision-check -> image built and verified, nothing written
just nvs-report  -> device NVS read and summarised
```

`just flash` has **not** been run: it would erase an app someone else put on the
attached board (see §7).

---

## 5. CI

`.github/workflows/rust.yml`, actions SHA-pinned to match the existing workflows.
Three jobs:

1. **`host`** — `cargo fmt --check`, `cargo clippy --workspace --all-targets
   --all-features -- -D warnings`, `cargo test --workspace --all-features`. No ESP
   toolchain, so it is fast, and since all control logic lives in the host crates
   this is the job that actually gates correctness.
2. **`firmware`** — matrix over every target. Installs rustup + pinned
   espup/ldproxy/espflash, then fmt-check, Clippy `-D warnings` and a release
   build **for each target**. Then it saves the app image and **fails if it exceeds
   the app partition**, warning above 85 %. That check exists because the C++ build
   sits at 90.4 % and nobody noticed until it was measured.
3. **`cpp-baseline`** — keeps `pio run -e esp32_usb` and `pio test -e native_test`
   green. The C++ firmware is the parity oracle; if it stops building we lose the
   ability to check the port against real behaviour.

**No blanket allows.** Clippy runs with `-D warnings` and there is no
`#![allow(...)]` at crate level anywhere. `Cargo.toml` declares the lint intent in
`[workspace.lints]` so it lives in the repo rather than only in a workflow file.
If a lint fires, fix it or annotate the specific line with a reason.

**Not yet run in GitHub Actions** — the workflow has only been exercised locally via
the equivalent `just` recipes. The espup install step in particular is untested on
a runner and may need a cache warm-up before it is quick.

---

## 6. Wi-Fi provisioning over USB

### 6.1 Mechanisms evaluated

Since [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md),
USB is the *only* supported way to get firmware onto a device, so provisioning over
USB is the primary path rather than a convenience.

| Mechanism | Needs firmware support | Works unprovisioned | Verdict |
|---|---|---|---|
| **Direct NVS write over USB** | no | yes | **chosen for v1 — verified feasible here** |
| Serial console command | yes | yes | good for reprovisioning; task NET-5 |
| `/config.json` seed on LittleFS | existing | yes | works today, but needs a filesystem image build and only applies on first boot (`_seeded` gate) |
| Captive portal (AP + DNS hijack) | yes | yes | user-facing path; ~200–400 LOC, no crate |
| ESP-IDF `wifi_provisioning` component | yes | yes | reachable only via raw `esp-idf-sys` FFI; needs a phone app |

The current C++ firmware has **no serial provisioning path at all**, so nothing
could be tested against it. What *was* verified is the mechanism that needs no
firmware cooperation.

### 6.2 What was verified, and how

The Rust firmware reads Wi-Fi credentials from NVS namespace `wifi`, keys `ssid`
and `pass`, as plain strings — deliberately outside the configuration blob, so a
host tool can write them before any firmware has run without understanding the
configuration schema. `scripts/provision.py` builds a real NVS image with
**Espressif's own** `esp-idf-nvs-partition-gen` (rather than hand-rolling NVS page
state and CRCs) and writes it with `espflash write-bin 0x9000`.

This is simpler than it was before ADR 0005: there is no hash derivation to
reproduce, so there is nothing to get subtly wrong.

Verified end-to-end on this host, short of the final write:

```
$ just provision-check /dev/cu.usbserial-204140
==> NVS namespace wifi, keys ssid + pass
==> generator: -m esp_idf_nvs_partition_gen
==> built NVS image, 20480 bytes
==> image verified: both keys present as type str
==> dry run, nothing written
```

The generated image was independently parsed by `scripts/nvs_inspect.py`, which
confirmed namespace `wifi` with both keys as NVS `str` entries. The read half of the transport
(`espflash read-flash`) was separately exercised against the real device, so both
directions of the mechanism are proven.

**Not verified:** the final `espflash write-bin`, and whether firmware then
connects with those credentials. Both are blocked on the device state in §7.

### 6.3 Validation, storage, reprovisioning, recovery

- **Validation.** The SSID and password are checked for presence and non-emptiness
  before anything runs, and the generated image is re-parsed and both keys
  confirmed before it is written. Length limits are *not* enforced yet — the C++
  firmware does not enforce them either (it has `HOSTNAME_MAX_LENGTH` and friends
  but only uses them for UI metadata), so this matches current behaviour rather
  than silently diverging. Worth fixing in the port; noted in the task list.
- **Secure storage.** Credentials land in NVS, which on this device is **not
  encrypted** — `espflash board-info` reports `Security features: None`, so there
  is no flash encryption and no secure boot. Anyone with physical access can read
  them out, exactly as today. Enabling flash encryption is a product decision with
  a one-way efuse burn attached, so it is not something this migration does
  unilaterally.
- **Reprovisioning.** Re-running `just provision` works, but **regenerating an NVS
  image replaces the whole partition**, which would erase the configuration blob
  alongside the credentials. The tool therefore reads the device's NVS first and **refuses to write
  if it is not blank**, telling you to inspect it and pass `--merge-anyway` if that
  is really what you want. A true merge, or the serial-console command in task
  NET-5, is the better long-term answer.
- **Recovery.** Three independent routes, none needing the network: the USB NVS
  write above; the existing `/config.json` LittleFS seed; and a full erase plus
  re-flash. The captive portal remains the user-facing route once implemented.

### 6.4 Secret handling

`.env` holds `WIFI_SSID` and `WIFI_PASS` and is gitignored. The rules the tooling
enforces:

- Credentials are read **inside** the recipe (`set -a; . ./.env; set +a`) and passed
  to the script through the environment, **never as argv** — argv is visible to any
  process via `ps`.
- Nothing prints a credential. `just provision` reports character counts and the
  derived key hashes only.
- The temporary CSV that `nvs_partition_gen` consumes contains the plaintext
  password and is deleted in a `finally` block, whatever happens. The generated
  image lives in a `TemporaryDirectory` and goes away with it.
- `scripts/nvs_inspect.py` prints key names, types and sizes only. **There is no
  flag to make it print a value.** `--digest` gives a truncated SHA-256 when you
  need to compare values without seeing them.
- No credential appears in any file committed by this work, in any log, or in any
  example. `docs/example_config.json` keeps the existing `Wokwi-GUEST` placeholder.

---

## 7. Device state

The attached device is not running this repo's C++ firmware. Evidence in
`research/device/FINDINGS.md`.

Read from the device:

- **App descriptor at `0x10000`** (valid magic `0xabcd5432`): `project_name:
  libespidf`, `idf_ver: v5.5.5`, version `v1.3.3-27-g51fa96c-dirty`, built
  `21:48:33 Sep 28 2026` local. `libespidf` is the project name an **esp-idf-sys
  (Rust + ESP-IDF)** build produces, so a Rust binary was flashed shortly before
  this session.
- **Its partition table matches neither the C++ table nor the new Rust table:**

  | | app0 | app1 | filesystem |
  |---|---|---|---|
  | On device | `0x010000`, 1792 KB | `0x1d0000`, 1792 KB | `0x390000`, 384 KB, label `littlefs` |
  | C++ (`partitions_4M.csv`) | `0x010000`, 1664 KB | `0x1b0000`, 1664 KB | `0x350000`, 640 KB, label `spiffs` |
  | **Rust (`partitions_rust_4m.csv`)** | `0x010000`, 1664 KB | `0x1b0000`, 1664 KB | `0x350000`, 640 KB, label **`ccfs`** |

- **NVS is completely blank** — all 20480 bytes `0xFF`, 0 of 5 pages written, no
  namespaces.

### What [ADR 0005](../adr/0005-no-backward-compatibility-usb-flash-migration.md) changed here

Most of what used to be blocking is not any more:

- The blank NVS **no longer matters**. Nothing needs preserving, and the device would
  be provisioned and configured from scratch regardless.
- The partition-table mismatch **no longer needs resolving** — we define our own, and
  `just flash` writes it alongside the app.
- Because this device's table has no `ccfs` partition, the Rust firmware would
  correctly refuse to boot on it as it stands. That is the layout guard working, and
  it makes a useful negative test for SPIKE-8.

**What still needs a nod:** flashing erases the Rust app someone put there an hour
before this session, along with its 384 KB `littlefs` region. The migration model
implies that is expected, but it destroys someone's work, so confirm before the first
`just flash`. Tracked as task-list prerequisite **P0.1**.

Nothing has been flashed.

---

## 8. Files

| Path | Purpose |
|---|---|
| `justfile` | every recipe; the target table |
| `partitions_rust_4m.csv` | the Rust partition table; `ccfs` replaces `spiffs` (ADR 0005) |
| `.mise.toml` | node, pnpm, python, clang-format, just; `ESP_IDF_VERSION`; why Rust is excluded |
| `rust-toolchain.toml` | pins the `esp` channel |
| `Cargo.toml` | host workspace, lint intent, release profile |
| `crates/cc-{domain,hal,drivers}/` | host-testable crates |
| `firmware/Cargo.toml` | target workspace |
| `firmware/.cargo/config.toml` | triple, ldproxy, `build-std`, `MCU`, `ESP_IDF_VERSION` |
| `.github/workflows/rust.yml` | host gates, per-target builds, size check, C++ baseline |
| `scripts/provision.py` | NVS key derivation, image generation, guarded write |
| `scripts/nvs_inspect.py` | read-only NVS inspection that never prints values |
| `tests/fixtures/config-export-cpp.json` | real C++ `config.json` export, credential fields scrubbed — the ORACLE-4 fixture |
