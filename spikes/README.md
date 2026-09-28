# Capability spikes

Throwaway builds that prove a capability compiles for a target. They are not part of the
firmware and are not in the cargo workspace. `just spike` rebuilds all of them.

Each one exists because the corresponding claim in
[docs/rust-migration/compatibility-matrix.md](../docs/rust-migration/compatibility-matrix.md)
would otherwise be unverified.

| Spike | Targets | Proves |
| --- | --- | --- |
| `hal-smoke` | esp32, esp32s3, esp32c6 | `esp-hal` 1.2.2 init, the `embassy` executor on `esp-rtos` 0.4, a hardware timer source, a periodic task, `esp-println` logging |
| `stack-smoke` | esp32, esp32s3, esp32c6 | `esp-storage` flash access and `esp-bootloader-esp-idf` partition table reads, `esp-hal` I2C master, UART0, `esp-radio` 1.0.0-beta.1 Wi-Fi station, a linked `embassy-net` 0.9 TCP/IP stack |
| `usb-smoke` | esp32s3, esp32c6 | the USB Serial/JTAG CDC peripheral as an async read and write channel, which is the S3 and C6 provisioning transport |
| `proto-host` | host | `serialport` 4.10 builds with no system libraries, which is what lets the provisioning host tool stay in Rust under mise |

## Building

```
just spike
```

Or individually:

```
cd spikes/hal-smoke
cargo build -Zbuild-std=core,alloc --release --target xtensa-esp32-none-elf --features esp32
```

`-Zbuild-std=core,alloc` is required for all three targets in this environment: the Xtensa
target has no prebuilt `rust-std`, and building the RISC-V target from source keeps all three
builds on one code path.

## API findings worth keeping

These cost a build cycle each and are recorded so nobody repeats them.

| Finding | Detail |
| --- | --- |
| `esp-wifi` is retired | the Wi-Fi driver is now `esp-radio`, inside the `esp-hal` monorepo, and is version `1.0.0-beta.1` |
| `esp-hal-embassy` is superseded | the embassy integration moved into `esp-rtos` with the `embassy` feature |
| `esp-rtos::start` takes two arguments | `start(timer, peripherals.FROM_CPU_INTR0)`; the `main`-branch example shows one, the published 0.4.0 crate takes two |
| I2C pin setters are on `I2c`, not on `Config` | `I2c::new(i2c, config)?.with_sda(pin).with_scl(pin)` |
| `embassy_net::new` argument order | `new(driver, config, &mut StackResources, seed)` |
| USB Serial/JTAG lives under `usb` | `esp_hal::usb::usb_serial_jtag`, and the peripheral is `USB_DEVICE`, not `USBTMC` |
| `esp_hal` has no 1-Wire module | the module list has no 1-Wire entry, and the C6 datasheet lists no 1-Wire peripheral |
| The C++ pin map does not exist on S3 or C6 | S3 has no GPIO22-25, so the SDA 21 / SCL 22 assignment cannot be reused |
| `serialport`'s `Box<dyn SerialPort>` is not itself a `SerialPort` | use the concrete `TTYPort` |

## Cleanup

These spikes are evidence for the Part A decision record. Delete them in the final migration
phase, or keep them as a toolchain smoke test. Keeping them costs nothing at build time because
they are outside the workspace.
