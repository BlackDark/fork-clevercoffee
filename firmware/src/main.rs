//! CleverCoffee firmware for the ESP32 DevKitC v4.
//!
//! The machine logic lives in `cc-core` (host-tested); this crate wires it to the
//! hardware: a 10 ms control loop on a high-priority executor, sensor, display,
//! storage and network tasks on the thread-mode executor.

#![no_std]
#![no_main]

extern crate alloc;

mod board;
mod control;
#[cfg(feature = "display")]
mod display;
mod flash;
mod net;
mod ota;
mod sensors;
mod shared;
mod storage;

#[cfg(feature = "ble-scale")]
mod ble_scale;

mod ui_assets {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

use cc_core::config::{Config, LogLevel, SwitchMode, TempSensorType};
use cc_core::{Machine, history::History};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveMode, Flex, Input, InputConfig, Io, Output, OutputConfig, Pull};
use esp_hal::interrupt::Priority;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::{MwdtStage, TimerGroup};
use esp_hal::{Blocking, ram};
use esp_rtos::embassy::InterruptExecutor;
use log::{LevelFilter, info, warn};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

/// The shared I²C bus (OLED, pressure sensor). Users run on the thread-mode
/// executor, so a no-op mutex suffices; transfers never hold a critical section.
pub type I2cDev = embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice<
    'static,
    embassy_sync::blocking_mutex::raw::NoopRawMutex,
    esp_hal::i2c::master::I2c<'static, Blocking>,
>;

/// The control loop must feed the watchdog within this time.
const WATCHDOG: Duration = Duration::from_secs(2);

fn level_filter(l: LogLevel) -> LevelFilter {
    match l {
        LogLevel::Trace => LevelFilter::Trace,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Warning => LevelFilter::Warn,
        LogLevel::Error | LogLevel::Fatal => LevelFilter::Error,
        LogLevel::Silent => LevelFilter::Off,
    }
}

/// Switch inputs get the pull the C++ firmware used: pull-down for normally-open,
/// pull-up for normally-closed contacts (GPIO34–39 have none and need resistors).
fn switch_input(pin: impl esp_hal::gpio::InputPin + 'static, mode: SwitchMode) -> Input<'static> {
    let pull = match mode {
        SwitchMode::NormallyOpen => Pull::Down,
        SwitchMode::NormallyClosed => Pull::Up,
    };
    Input::new(pin, InputConfig::default().with_pull(pull))
}

fn relay(pin: impl esp_hal::gpio::OutputPin + 'static, active_high: bool) -> Output<'static> {
    Output::new(
        pin,
        control::Polarity::off_level(active_high),
        OutputConfig::default(),
    )
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    // Info until the settings are loaded; then `system.log_level` applies.
    esp_println::logger::init_logger(LevelFilter::Info);
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // ADR 0001: the reclaimed bootloader RAM plus a small main-RAM region.
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 94 * 1024);
    // Bluetooth claims 64 KB of main RAM for its controller; then there is no room
    // for a second heap region.
    #[cfg(not(feature = "ble-scale"))]
    esp_alloc::heap_allocator!(size: 48 * 1024);

    let timg0 = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timg0.timer0, p.FROM_CPU_INTR0);

    info!("CleverCoffee {} starting", env!("CARGO_PKG_VERSION"));

    // Settings.
    flash::init(esp_storage::FlashStorage::new(p.FLASH)).await;
    let mut store = storage::Store::open().await;
    let (mut cfg, shots) = match &mut store {
        Some(s) => s.load().await,
        None => {
            warn!("settings partition missing: running on defaults");
            (Config::default(), 0)
        }
    };
    if storage::apply_provisioning(&mut cfg).await
        && let Some(s) = &mut store
    {
        s.save(&cfg, shots).await;
    }
    log::set_max_level(level_filter(cfg.system_log_level));
    ota::init().await;

    // Relays off before anything else can switch them.
    let polarity = control::Polarity::from_config(&cfg);
    let pins = take_pins!(p);
    let mut io = control::Io {
        heater: relay(pins.heater, polarity.heater),
        pump: relay(pins.pump, polarity.pump),
        valve: relay(pins.valve, polarity.valve),
        brew: switch_input(pins.brew_switch, cfg.hardware_switches_brew_mode),
        steam: switch_input(pins.steam_switch, cfg.hardware_switches_steam_mode),
        water: switch_input(pins.water_switch, cfg.hardware_switches_hot_water_mode),
        power: switch_input(pins.power_switch, cfg.hardware_switches_power_mode),
        tank: switch_input(pins.water_tank, cfg.hardware_sensors_watertank_mode),
        status_led: cfg
            .hardware_leds_status_enabled
            .then(|| relay(pins.status_led, polarity.status_led)),
        brew_led: cfg
            .hardware_leds_brew_enabled
            .then(|| relay(pins.brew_led, polarity.brew_led)),
        // GPIO1 is the serial TX line: only claimed when the LED is wanted.
        steam_led: cfg
            .hardware_leds_steam_enabled
            .then(|| relay(pins.steam_led, polarity.steam_led)),
    };
    control::all_off(&mut io, polarity);
    control::arm_panic_shutdown(polarity);

    let machine = Machine::new(&cfg, shots, 0);
    let wifi = net::WifiSettings {
        ssid: cfg.system_wifi_ssid.clone(),
        password: cfg.system_wifi_password.clone(),
        hostname: cfg.system_hostname.clone(),
        offline: cfg.system_offline_mode,
    };
    let temp_type = cfg.hardware_sensors_temperature_type;
    #[cfg(feature = "display")]
    let oled_enabled = cfg.hardware_oled_enabled;
    #[cfg(feature = "pressure")]
    let pressure_enabled = cfg.hardware_sensors_pressure_enabled;
    #[cfg(any(feature = "hx711-scale", feature = "ble-scale"))]
    let (scale_enabled, scale_type) = (
        cfg.hardware_sensors_scale_enabled,
        cfg.hardware_sensors_scale_type,
    );
    #[cfg(feature = "mqtt")]
    let mqtt_enabled = cfg.mqtt_enabled && !cfg.system_offline_mode;
    shared::init(shared::Shared {
        cfg,
        machine,
        history: alloc::boxed::Box::new(History::new()),
        net: shared::NetState::default(),
        live: cc_api::Live::default(),
        config_dirty: false,
        shots_dirty: false,
        ota_active: false,
    });

    // Control loop on its own executor, above everything but the radio.
    let mut wdt = TimerGroup::new(p.TIMG1).wdt;
    wdt.set_timeout(
        MwdtStage::Stage0,
        esp_hal::time::Duration::from_millis(WATCHDOG.as_millis()),
    );
    wdt.enable();
    static CONTROL: StaticCell<InterruptExecutor<2>> = StaticCell::new();
    let control = CONTROL.init(InterruptExecutor::new(p.FROM_CPU_INTR2));
    control
        .start(Priority::Priority3)
        .spawn(control::run(io, polarity, wdt).unwrap());

    // Sensors.
    match temp_type {
        TempSensorType::Tsic306 => {
            let mut gpio = Io::new(p.IO_MUX);
            sensors::install_tsic(&mut gpio, Input::new(pins.temp_sensor, InputConfig::default()));
            spawner.spawn(sensors::tsic_task().unwrap());
        }
        TempSensorType::Ds18b20 => {
            let mut pin = Flex::new(pins.temp_sensor);
            pin.apply_output_config(
                &OutputConfig::default()
                    .with_drive_mode(DriveMode::OpenDrain)
                    .with_pull(Pull::Up),
            );
            pin.set_high();
            pin.set_output_enable(true);
            pin.set_input_enable(true);
            spawner.spawn(sensors::ds18b20_task(pin).unwrap());
        }
    }
    #[cfg(feature = "hx711-scale")]
    if scale_enabled {
        let clk = Output::new(pins.hx_clock, esp_hal::gpio::Level::Low, OutputConfig::default());
        let data =
            |pin: esp_hal::gpio::AnyPin<'static>| Input::new(pin, InputConfig::default().with_pull(Pull::Up));
        match scale_type {
            cc_core::config::ScaleType::Hx711Single => {
                spawner.spawn(sensors::hx711_single_task(clk, data(pins.hx_data.into())).unwrap());
            }
            cc_core::config::ScaleType::Hx711Dual => spawner.spawn(
                sensors::hx711_dual_task(clk, [data(pins.hx_data.into()), data(pins.hx_data2.into())])
                    .unwrap(),
            ),
            cc_core::config::ScaleType::Bluetooth => {}
        }
    }

    // I²C peripherals.
    #[cfg(any(feature = "display", feature = "pressure"))]
    {
        use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
        use embassy_sync::blocking_mutex::Mutex;
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        use esp_hal::i2c::master::{Config as I2cConfig, I2c};

        type Bus = Mutex<NoopRawMutex, core::cell::RefCell<I2c<'static, Blocking>>>;
        static BUS: StaticCell<Bus> = StaticCell::new();
        match I2c::new(
            p.I2C0,
            I2cConfig::default().with_frequency(esp_hal::time::Rate::from_khz(400)),
        ) {
            Ok(i2c) => {
                let bus: &'static Bus = BUS.init(Mutex::new(core::cell::RefCell::new(
                    i2c.with_sda(pins.sda).with_scl(pins.scl),
                )));
                #[cfg(feature = "display")]
                if oled_enabled {
                    spawner.spawn(display::task(I2cDevice::new(bus)).unwrap());
                }
                #[cfg(feature = "pressure")]
                if pressure_enabled {
                    spawner.spawn(sensors::pressure_task(I2cDevice::new(bus)).unwrap());
                }
                let _ = bus;
            }
            Err(e) => warn!("i2c: {e:?}"),
        }
    }

    // Network.
    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    // The station interface is a singleton: borrow it for the MAC and drop it before the
    // stack takes it, or `net::stacks` panics ("station interface already taken").
    let ap_name = net::ap_ssid(esp_radio::wifi::Interface::station().mac_address());
    let (stacks, sta_runner, ap_runner) = net::stacks(&wifi.hostname, seed);
    let controller = esp_radio::wifi::WifiController::new(
        p.WIFI,
        esp_radio::wifi::ControllerConfig::default()
            .with_initial_config(net::initial_config(&wifi, &ap_name)),
    );
    match controller {
        Ok(controller) => {
            spawner.spawn(net::net_task(sta_runner).unwrap());
            spawner.spawn(net::net_task(ap_runner).unwrap());
            for id in 0..net::web::STA_WORKERS {
                spawner.spawn(net::web::task(id, stacks.sta).unwrap());
            }
            spawner.spawn(net::web::task(net::web::STA_WORKERS, stacks.ap).unwrap());
            spawner.spawn(net::portal::dns_task(stacks.ap).unwrap());
            spawner.spawn(net::portal::dhcp_task(stacks.ap).unwrap());
            spawner.spawn(net::mdns::task(stacks.sta, wifi.hostname.clone()).unwrap());
            #[cfg(feature = "mqtt")]
            if mqtt_enabled {
                spawner.spawn(net::mqtt::task(stacks.sta).unwrap());
            }
            spawner.spawn(net::wifi_task(controller, wifi, ap_name, stacks.sta).unwrap());
        }
        Err(e) => warn!("wifi: {e:?}"),
    }

    #[cfg(feature = "ble-scale")]
    if scale_enabled && scale_type == cc_core::config::ScaleType::Bluetooth {
        ble_scale::start(&spawner, p.BT);
    }

    if let Some(store) = store {
        spawner.spawn(storage::run(store).unwrap());
    }

    loop {
        let stats = esp_alloc::HEAP.stats();
        let free = (stats.size - stats.current_usage) as u32;
        if free < shared::MIN_FREE_HEAP.fetch_min(free, core::sync::atomic::Ordering::Relaxed) {
            info!("heap: {free} bytes free (lowest so far)");
        }
        Timer::after(Duration::from_secs(10)).await;
    }
}
