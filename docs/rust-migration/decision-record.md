# Decision Record: C++ to Rust migration on ESP32

Date: 2026-09-28. All source links were fetched on that date.

Cross-links: [inventory.md](inventory.md), [compatibility-matrix.md](compatibility-matrix.md),
[architecture.md](architecture.md), [api-contract.md](api-contract.md),
[config-export-schema.md](config-export-schema.md), [defects-register.md](defects-register.md),
[task-list.md](task-list.md), [tooling.md](tooling.md).

---

## Decision

**Use `esp-hal` 1.2.2 bare metal with `embassy`, not the ESP-IDF based `esp-idf-hal` /
`esp-idf-svc` crates.**

Supporting decisions:

1. **Per-chip support is machine-enforced for `esp-hal`, self-reported for `esp-idf-hal`.**
   `esp-hal`'s chip matrix lives in
   [`.github/chips.json`](https://raw.githubusercontent.com/esp-rs/esp-hal/main/.github/chips.json),
   which the CI workflow header describes as the single source that drives the HAL, docs, MSRV,
   toolchain and hardware-in-the-loop jobs. All three targets have a dedicated HIL runner;
   `esp32c6` and `esp32s3` additionally carry `"quick": true` and radio runners. The
   `esp-idf-hal` crate's own README states the crates "might be a bit lagging behind the latest
   stable ESP-IDF version" and "are (currently) missing HIL tests"
   ([esp-idf-svc README](https://raw.githubusercontent.com/esp-rs/esp-idf/master/esp-idf-svc/README.md)).

2. **Every capability the firmware needs builds for all three chips.** Verified by building
   [`spikes/stack-smoke`](../../spikes/stack-smoke) and
   [`spikes/usb-smoke`](../../spikes/usb-smoke) for `xtensa-esp32-none-elf`,
   `xtensa-esp32s3-none-elf` and `riscv32imac-unknown-none-elf`, exercising flash and partition
   table access, I2C master, UART0, the embassy executor, Wi-Fi station mode through `esp-radio`
   with `embassy-net`, and USB Serial/JTAG CDC.

3. **No usable HTTP server exists on the bare-metal path either**, so that gap is not a
   discriminator between the two approaches. It is handled as a project-owned crate.

4. **The provisioning story is better on bare metal.** The original ESP32 has no native USB, so
   the C++ firmware and the IDFF stack both provision over UART0 through the USB-serial bridge.
   `esp-hal` exposes UART as an ordinary peripheral, which is exactly what the UART provisioning
   protocol needs. On S3 and C6 `esp-hal` exposes the USB Serial/JTAG peripheral directly
   (verified compiling in `spikes/usb-smoke`), which gives a native CDC channel with no bridge
   chip.

### What is still unverified

`esp-hal` marks ADC, LEDC (PWM), RMT, timers, DAC and the USB module `unstable`. The heater
control path in this firmware needs a time-accurate PWM output; the architecture in
[architecture.md](architecture.md) does not depend on LEDC, because the C++ firmware's PWM is
software PWM in a 10 ms ISR and that is reproduced directly. `esp-storage` and
`esp-bootloader-esp-idf` are stable and were used in the spikes.

---

## Rejected: `esp-idf-hal` / `esp-idf-svc`

| Dimension | `esp-idf-svc` | `esp-hal` | Winner |
| --- | --- | --- | --- |
| Chip support evidence | target table in the docs.rs README body; no CI list retrieved | machine-enforced CI matrix plus per-chip HIL runners | esp-hal |
| Maintenance | "community effort", "no paid developer time", "missing HIL tests", self-reported lag | 2125 stars, pushed same day, MSRV 1.95, dual MIT/Apache | esp-hal |
| Memory | lwIP plus the IDF task system: an RTOS, a TCP/IP stack, a filesystem, Wi-Fi | bare metal plus `esp-radio` plus `embassy-net` on smoltcp; a hello-world links at 2.5 MB of flash, and the spike binary is 99 KB of app image | esp-hal |
| Blocking model | every driver is both blocking and async, and there is an IDF event loop under everything | one executor, one async model, drivers are futures | esp-hal, for auditability of a safety-critical control loop |
| Wi-Fi, NVS, OTA, HTTP, filesystem | all present in IDF, one dependency to add | must be built or found separately (Wi-Fi present, flash present, HTTP missing) | esp-idf-svc |
| Toolchain | ESP-IDF, several GB, per-chip; plus a C/C++ build | `espup` (Xtensa fork of rustc, Xtensa LLVM, Xtensa GCC) plus `rustup` for RISC-V | comparable, esp-hal slightly simpler |
| Language features | `std` available | `no_std` only | esp-idf-svc |

The disqualifying gap for `esp-idf-svc` is not capability, it is verification. A safety-critical
control loop whose heaters run from an ISR needs its concurrency model to be auditable and its
dependency to be tested on hardware. `esp-idf-svc` self-reports having no HIL tests and lagging
the IDF; `esp-hal` has per-chip HIL runners including radio.

The cost of choosing `esp-hal` is that we own the HTTP server, the 1-Wire and DS18B20 drivers,
the HX711 driver and the display layout engine. Those are scoped in
[architecture.md](architecture.md) and [task-list.md](task-list.md).

---

## Decision: DS18B20 and 1-Wire are ours to write

The only end-to-end DS18B20 crate is [`ds18b20` 0.1.1](https://crates.io/api/v1/crates/ds18b20),
published 2020-08-19. It depends on `one-wire-bus` 0.1.1 (published 2020-01-18) and
`embedded-hal` 0.2.3, while `esp-hal` 1.2.2 implements `embedded-hal` 1.0. The transport is
generic over `embedded_hal::digital::v2` input and output pins, so it is a GPIO bit-bang with
no hardware backend
([source](https://raw.githubusercontent.com/fuchsnj/ds18b20/master/src/lib.rs)).

`esp-hal` has no 1-Wire module. The published module list on `main` is `clock, gpio, i2c,
peripherals, rng, spi, system, time, uart, efuse, interrupt` plus unstable modules including
`analog, timer, ledc, rmt, usb`; there is no 1-Wire or `one_wire` entry
([lib.rs](https://raw.githubusercontent.com/esp-rs/esp-hal/main/esp-hal/src/lib.rs)).

The premise in the task brief that S3 and C6 have a dedicated 1-Wire peripheral is **not
supported by evidence**. The ESP32-C6 datasheet v1.5 peripheral table lists UART, SPI, I2C, I2S,
pulse count, USB Serial/JTAG, TWAIT, SDIO slave, LEDC, MCPWM, RMT, parallel IO, SAR ADC and an
internal temperature sensor, and no 1-Wire controller
([ESP32-C6 datasheet](https://www.espressif.com/sites/default/files/documentation/esp32-c6_datasheet_en.pdf)).
The S3 datasheet was not retrieved, so S3 is `unverified` on this point. Bit-banging is the only
route on every chip.

Decision: write `clevercoffee-onewire` (bit-bang transport, timing model, ROM search, CRC-8) and
`clevercoffee-ds18b20` (command layer) as two small crates in this workspace, both host-testable
against a simulated bit-level bus. Roughly 300 to 500 lines including tests.

---

## Decision: the HTTP server is ours to write

There is no `esp-rs/esp-http-server`; the repository returns 404. Searching the Rust
ecosystem for an embedded HTTP server that links to `embassy-net` returns small crates with low
adoption (`nanofish`, `nanooctopus`, `peaweb`). `picoserve` 0.20.1 is the most downloaded
(124 170) and is explicitly "an async no_std HTTP server suitable for bare-metal environments",
but it is not ESP-specific and has no track record on this hardware.

Decision: write `clevercoffee-http`, a minimal HTTP/1.1 server over `embassy-net` TCP, with
routing, SSE, and static asset serving from a flash region. Roughly 600 lines plus 800 lines of
host tests, where the tests run against an in-memory socket. This also removes the C++ firmware's
`ESPAsyncWebServer` dependency entirely, which is the single largest untested subsystem in the
current code.

---

## Decision: USB provisioning

Three mechanisms were compared.

| Mechanism | Original ESP32 | ESP32-S3 | ESP32-C6 | Needs our firmware running | Notes |
| --- | --- | --- | --- | --- | --- |
| USB CDC command/response | **not available**, no native USB | USB Serial/JTAG, verified to compile | USB Serial/JTAG, verified to compile | yes | Native CDC, no bridge chip. espflash has no provisioning verb, so the host side is a small script speaking our line protocol. |
| UART command/response over the USB-serial bridge | UART0, verified to compile | available | available | yes | Identical protocol on all three chips, so one implementation and one host script. |
| Prebuilt NVS image + `espflash write-bin` | works, chip-agnostic address | works | works | no | Requires a Python NVS image generator outside the mise toolchain, and the blob sits in plaintext on the host and in the flashing process. |
| Custom binary config region + `write-bin` | works | works | works | no at flash time, yes at read time | Same secret-handling problem, and the region is a raw byte blob. |

Decision: **a line-oriented text protocol over a serial channel, chosen per chip**:
UART0 on the original ESP32, USB Serial/JTAG on S3 and C6. The protocol is the same in both
cases, so only the transport differs and that difference is one `ProvisioningTransport` trait.

`espflash write-bin` is kept as a documented fallback for a device that will not accept a
console session, and it is how a factory image is written in CI.

Evidence that espflash can write an arbitrary region: `espflash write-bin <ADDRESS> <FILE>` with
a plain `u32` address, and `write-bin` support is listed for all three chips
([espflash CLI](https://raw.githubusercontent.com/esp-rs/espflash/main/espflash/src/cli/mod.rs),
[README](https://raw.githubusercontent.com/esp-rs/espflash/main/espflash/README.md), 4.6.0,
2026-09-10). `espflash save-image` produces an application image from a built ELF, verified
locally on the ESP32: a 2 571 412-byte ELF yields a 99 728-byte app image at offset `0x10000`.

The NVS-image path was rejected because the generator is an ESP-IDF Python module
(`nvs_partition_gen.py` is a three-line shim to `esp_idf_nvs_partition_gen`,
[esp-idf master](https://raw.githubusercontent.com/espressif/esp-idf/master/components/nvs_flash/nvs_partition_generator/nvs_partition_generator.py)),
its input format was not verified, and it would put credentials in a file on the host before
they ever reach the device. The console protocol keeps the secret in the device's stdin.

---

## Decision: a custom config partition, not NVS

`esp-storage` gives verified access to the flash peripheral and to the partition table
(`read_partition_table` and `OtaUpdater` are used in the spikes,
[esp-storage](https://raw.githubusercontent.com/esp-rs/esp-hal/main/esp-storage/README.md)). It
does not give a Rust NVS binding. Rather than depend on an ESP-IDF Python tool to generate NVS
blobs, the new firmware owns a **dedicated, versioned, CRC-protected binary config region**
inside a `data`-subtype partition. Rationale:

- No NVS dependency, so the format is defined by this project and versioned with it.
- One `write-bin` of a fixed blob provisions a device with no firmware running.
- The USB console path can rewrite the same region from inside the firmware.
- A CRC over the payload means a torn write is detected at boot and the device falls back to
  defaults rather than running with a corrupt setpoint.

The C++ NVS layout (FNV-1a hashed keys, a `config` namespace, a `maintenance` namespace) is not
carried over. This is a clean break by design: see [architecture.md](architecture.md#storage).

---

## Decision: OTA via `esp-bootloader-esp-idf`

`esp-bootloader-esp-idf` 0.6.0 provides `read_partition_table` and `OtaUpdater`, and the
upstream example
([examples/ota/update](https://raw.githubusercontent.com/esp-rs/esp-hal/main/examples/ota/update/src/main.rs))
shows switching the boot partition. Decision: two `ota_0` / `ota_1` app partitions, a 32 KB
`otadata` partition, and an `app` update written by the firmware from an HTTP upload, a URL
download, an espota push, or a blob sent over the provisioning channel, followed by a partition
switch and reboot.

All four paths are kept; see [Decision: OTA is kept](#decision-ota-is-kept-in-all-four-paths).

---

## Decision: the Wi-Fi captive portal is dropped

**Confirmed by the user on 2026-09-29.** `WiFiManager` (a git dependency in the C++ build) runs a
60-second blocking captive portal at boot when no SSID is configured. The new provisioning channel
over USB removes the need for it, and the C++ portal blocks the main loop for up to 60 seconds.

## Decision: both temperature sensors are kept

**Confirmed by the user on 2026-09-29.** The plan had proposed dropping the TSIC 306 in favour of
the DS18B20, because the bench has a DS18B20 and `ZACwire` has no Rust equivalent. Both are kept
instead: `hardware.sensors.temperature.type` selects one at boot, and the TSIC driver is a second
command layer over the same `TemperatureSensor` trait. The bench device has a DS18B20, so the TSIC
path stays build-verified until a TSIC-equipped machine is available.

## Decision: OTA is kept, in all four paths

**Confirmed by the user on 2026-09-29: remove OTA only if space becomes a problem, otherwise
keep it.** The plan had proposed deleting the HTTP OTA routes on the grounds that they are
unauthenticated (D17) and that the URL variant is an SSRF vector (D16). Both remain, corrected
rather than removed:

- every OTA path requires the configured password;
- every OTA path refuses to start unless the machine is idle, which is also the fix for D01;
- the URL variant gains a scheme and host allow-list, and the firmware variant gains the
  extension check the filesystem variant already had;
- a USB OTA path is added alongside, through the provisioning channel.

Space is not a constraint. Two 1.5 MB app slots against a 99 KB image, with the web assets in
their own region. If a future feature does push the image past the slot, the lever is the asset
region or, on the 8 MB S3 and C6 boards, larger slots.

## Decision: the C6 pin map does not fit, and needs a user decision

**Found on 2026-09-29 while researching dev board pinouts, resolved the same day.** The project
needs 17 pins: 3 relays, 3 LEDs, 4 panel switches, a water-tank input, a 1-Wire data pin, 3 HX711
pins and 2 I2C pins. The ESP32-C6-DevKitC-1 exposes 23 GPIOs on its header, of which **14 are
usable** after removing the module's SDIO flash bus, the native USB pins and the on-board RGB
LED. The ESP32 and ESP32-S3 both have room.

Resolution, by the user on 2026-09-29: **features are disabled on the C6**. The essential 11
signals plus a single-cell HX711 fit in 14 pins with one spare, so the three indicator LEDs and
the second load cell are not compiled for the C6 board feature. The alternative, an IO expander,
was rejected; it would also have left the heater PWM timing over I2B unresolved. Full evidence and
the map are in [board-pinouts.md](board-pinouts.md#c6-map).
