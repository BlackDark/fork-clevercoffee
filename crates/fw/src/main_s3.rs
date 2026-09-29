//! Firmware entry point for the ESP32-S3.
//!
//! The board and the provisioning transport are selected at compile time, and selecting more
//! than one of either is a build error rather than a runtime surprise: see `checks.rs`.

#![no_std]
#![no_main]

use esp_backtrace as _;

use clevercoffee_fw::run;
use embassy_executor::Spawner;

esp_bootloader_esp_idf::esp_app_desc!();

#[path = "checks.rs"]
mod checks;

#[esp_hal::main]
async fn main(_spawner: Spawner) -> ! {
    // Clocks and the timer group come first; `esp_rtos::start` hands the timer's interrupt to the
    // scheduler, and nothing else can run before it.
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // The relays come up inactive, before anything that could block and before the
    // scheduler takes its share of the peripherals. See `clevercoffee-fw`'s module docs for the
    // order and why it is the order it is.
    let relays = clevercoffee_bsp_esp32s3::relays!(peripherals);
    let inputs = clevercoffee_bsp_esp32s3::switches!(peripherals);

    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let runtime = clevercoffee_app::boot::Runtime::new(relays, Default::default());
    run(runtime, Sensors { inputs }).await
}

/// The board's sensors, read once per control tick.
///
/// The driver periods are not implemented here: the temperature sensor is bit-banged and the
/// pressure sensor is I2C, and both belong in their own crates with their own timing. This
/// compiles with the switch and tank inputs only, and the sensor half of this struct is the next
/// thing to fill in. A machine built this way reads the panel and the tank and nothing else, which
/// is why the compatibility matrix marks the sensor rows unverified rather than claiming them.
struct Sensors<'a> {
    inputs: clevercoffee_bsp_esp32s3::SwitchInputs<'a>,
}

impl<'a> clevercoffee_fw::SensorSource for Sensors<'a> {
    type Actuators = clevercoffee_bsp_esp32s3::Relays<'a>;

    fn snapshot(&mut self) -> clevercoffee_app::Sensors {
        clevercoffee_app::Sensors {
            water_tank_full: self.inputs.water_tank_full(),
            switches: self.inputs.sample(),
            ..clevercoffee_app::Sensors::empty()
        }
    }
}
