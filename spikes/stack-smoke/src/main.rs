//! Compile-only smoke test: touches every capability the migration depends on.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::i2c::master::I2c;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::Uart;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, Interface};
use esp_storage::FlashStorage;

esp_bootloader_esp_idf::esp_app_desc!();

const SSID: &str = "smoke";
const PASSWORD: &str = "smoke";

#[embassy_executor::task]
async fn tick() {
    loop {
        Timer::after(Duration::from_millis(1_000)).await;
    }
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size: 96 * 1024);
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Flash access and the partition table: the storage and OTA substrate.
    let mut flash = FlashStorage::new(peripherals.FLASH);
    let mut pt_buf = [0u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN];
    let pt =
        esp_bootloader_esp_idf::partitions::read_partition_table(&mut flash, &mut pt_buf).unwrap();
    esp_println::println!("partitions: {}", pt.iter().count());

    // I2C master for the OLED.
    // The C++ pin map (SDA 21 / SCL 22) only exists on the original ESP32.
    // ESP32-S3 has no GPIO22-25; ESP32-C6 uses GPIO8/9 for I2C0.
    #[cfg(feature = "esp32")]
    let mut i2c = I2c::new(peripherals.I2C0, esp_hal::i2c::master::Config::default())
        .unwrap()
        .with_sda(peripherals.GPIO21)
        .with_scl(peripherals.GPIO22);
    #[cfg(feature = "esp32s3")]
    let mut i2c = I2c::new(peripherals.I2C0, esp_hal::i2c::master::Config::default())
        .unwrap()
        .with_sda(peripherals.GPIO8)
        .with_scl(peripherals.GPIO9);
    #[cfg(feature = "esp32c6")]
    let mut i2c = I2c::new(peripherals.I2C0, esp_hal::i2c::master::Config::default())
        .unwrap()
        .with_sda(peripherals.GPIO8)
        .with_scl(peripherals.GPIO9);

    // UART0 console, the provisioning channel on the original ESP32.
    let _uart = Uart::new(
        peripherals.UART0,
        esp_hal::uart::Config::default().with_baudrate(115_200),
    )
    .unwrap();

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    spawner.spawn(tick().unwrap());

    // Wi-Fi station.
    let iface = Interface::station();
    let mut controller = esp_radio::wifi::WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(WifiConfig::Station(
            esp_radio::wifi::sta::StationConfig::default()
                .with_ssid(SSID.try_into().unwrap())
                .with_authentication(esp_radio::wifi::AuthenticationMethodConfig::Wpa2Personal(
                    PASSWORD.try_into().unwrap(),
                )),
        )),
    )
    .unwrap();
    controller
        .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
        .unwrap();

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    static RES: static_cell::StaticCell<StackResources<3>> = static_cell::StaticCell::new();
    let res = RES.init(StackResources::<3>::new());
    let (_stack, _runner) = embassy_net::new(
        iface,
        embassy_net::Config::dhcpv4(Default::default()),
        res,
        seed,
    );

    let _ = i2c;
    loop {
        Timer::after(Duration::from_millis(5_000)).await;
    }
}
