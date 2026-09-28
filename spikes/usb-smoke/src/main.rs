//! Proves the USB Serial/JTAG CDC channel can be used for provisioning on S3 and C6.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::usb::usb_serial_jtag::UsbSerialJtag;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn tick() {
    loop {
        Timer::after(Duration::from_millis(1_000)).await;
    }
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size: 32 * 1024);
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let (rx, tx) = UsbSerialJtag::new(peripherals.USB_DEVICE)
        .into_async()
        .split();

    static SIGNAL: StaticCell<Signal<NoopRawMutex, heapless::String<256>>> = StaticCell::new();
    let signal = &*SIGNAL.init(Signal::new());

    spawner.spawn(reader(rx, &signal).unwrap());
    spawner.spawn(writer(tx, &signal).unwrap());
    spawner.spawn(tick().unwrap());

    loop {
        Timer::after(Duration::from_millis(5_000)).await;
    }
}

#[embassy_executor::task]
async fn reader(
    mut rx: esp_hal::usb::usb_serial_jtag::UsbSerialJtagRx<'static, esp_hal::Async>,
    signal: &'static Signal<NoopRawMutex, heapless::String<256>>,
) {
    let mut buf = [0u8; 256];
    loop {
        match embedded_io_async::Read::read(&mut rx, &mut buf).await {
            Ok(n) => {
                let mut v = heapless::Vec::<_, 256>::new();
                v.extend_from_slice(&buf[..n]).unwrap();
                signal.signal(heapless::String::from_utf8(v).unwrap());
            }
            Err(_) => {}
        }
    }
}

#[embassy_executor::task]
async fn writer(
    mut tx: esp_hal::usb::usb_serial_jtag::UsbSerialJtagTx<'static, esp_hal::Async>,
    signal: &'static Signal<NoopRawMutex, heapless::String<256>>,
) {
    use core::fmt::Write;
    embedded_io_async::Write::write_all(&mut tx, b"provisioning-console-ready\r\n")
        .await
        .unwrap();
    loop {
        let msg = signal.wait().await;
        signal.reset();
        write!(&mut tx, "-- received '{}' --\r\n", msg).unwrap();
        embedded_io_async::Write::flush(&mut tx).await.unwrap();
    }
}
