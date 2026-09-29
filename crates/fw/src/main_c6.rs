//! Firmware entry point for the ESP32-C6.
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
    let relays = clevercoffee_bsp_esp32c6::relays!(peripherals);
    let inputs = clevercoffee_bsp_esp32c6::switches!(peripherals);

    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // The config region, read and validated. A region that is absent, unparseable or rejected
    // leaves the machine on its compiled defaults, which is a machine that brews rather than one
    // that will not start. The read is a flash operation, so it happens here, once, before the
    // loop: nothing on the control path touches flash.
    let loaded = clevercoffee_fw::config::read(peripherals.FLASH);
    esp_println::logger::init_logger_from_env();
    esp_println::println!("{}", loaded.log_line());

    let runtime = clevercoffee_app::boot::Runtime::new(relays, loaded.config);
    run(runtime, BoardSensors::new(inputs)).await
}

/// The board's sensors, polled once per control tick on each driver's own period.
struct BoardSensors<'a> {
    inputs: clevercoffee_bsp_esp32c6::SwitchInputs<'a>,
    aggregator: clevercoffee_app::sensors::Aggregator,
    pressure: clevercoffee_fw::sensors::NoPressure,
}

impl<'a> BoardSensors<'a> {
    fn new(inputs: clevercoffee_bsp_esp32c6::SwitchInputs<'a>) -> Self {
        Self {
            inputs,
            aggregator: clevercoffee_app::sensors::Aggregator::new(),
            pressure: clevercoffee_fw::sensors::NoPressure,
        }
    }
}

/// The board's input block, as the aggregator's two sources.
struct BoardTank<'a>(&'a clevercoffee_bsp_esp32c6::SwitchInputs<'a>);

impl clevercoffee_app::sensors::TankSource for BoardTank<'_> {
    fn is_full(&self) -> bool {
        self.0.water_tank_full()
    }
}

impl clevercoffee_app::sensors::SwitchSource for BoardTank<'_> {
    fn sample(&self) -> clevercoffee_app::machine::Switches {
        self.0.sample()
    }
}

impl<'a> clevercoffee_fw::SensorSource for BoardSensors<'a> {
    type Actuators = clevercoffee_bsp_esp32c6::Relays<'a>;

    fn snapshot(&mut self) -> clevercoffee_app::Sensors {
        let now = self.aggregator.now_ms();
        let tank = BoardTank(&self.inputs);
        clevercoffee_fw::sensors::poll(&mut self.aggregator, now, &mut self.pressure, &tank, &tank);
        // The aggregator's clock is the firmware's, advanced by the tick the loop just did.
        self.aggregator
            .advance(clevercoffee_fw::CONTROL_PERIOD.as_millis() as u32);
        self.aggregator.snapshot()
    }
}
