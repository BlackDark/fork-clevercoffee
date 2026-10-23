//! Sensor tasks. Each publishes fresh readings to [`shared::publish`]; the control
//! loop takes them on its next tick.

use core::cell::RefCell;

use critical_section::Mutex;
use embassy_time::{Delay, Duration, Instant, Ticker, Timer};
use esp_hal::gpio::{Event, Flex, Input};
use esp_hal::handler;
use log::warn;

use crate::shared;

/// A reading older than this counts as missing (the machine then reports a sensor
/// error after its own timeout).
const FAULT_AFTER_ERRORS: u32 = 10;

// --- TSIC 306 (ZACwire) -------------------------------------------------------------
//
// The sensor sends a 20-bit frame every 100 ms; bits are 125 µs long. Edges are
// timestamped in the GPIO interrupt and decoded in a task.

const EDGE_RING: usize = 128;

struct Edges {
    buf: [(bool, u32); EDGE_RING],
    head: usize,
    len: usize,
}

static TSIC_PIN: Mutex<RefCell<Option<Input<'static>>>> = Mutex::new(RefCell::new(None));
static EDGES: Mutex<RefCell<Edges>> = Mutex::new(RefCell::new(Edges {
    buf: [(false, 0); EDGE_RING],
    head: 0,
    len: 0,
}));

#[handler]
fn gpio_interrupt() {
    let t = Instant::now().as_micros() as u32;
    critical_section::with(|cs| {
        let mut pin = TSIC_PIN.borrow_ref_mut(cs);
        let Some(pin) = pin.as_mut() else { return };
        if !pin.is_interrupt_set() {
            return;
        }
        let rising = pin.is_high();
        pin.clear_interrupt();
        let mut e = EDGES.borrow_ref_mut(cs);
        let at = (e.head + e.len) % EDGE_RING;
        e.buf[at] = (rising, t);
        if e.len < EDGE_RING {
            e.len += 1;
        } else {
            e.head = (e.head + 1) % EDGE_RING;
        }
    });
}

pub fn install_tsic(io: &mut esp_hal::gpio::Io<'_>, mut pin: Input<'static>) {
    io.set_interrupt_handler(gpio_interrupt);
    critical_section::with(|cs| {
        pin.listen(Event::AnyEdge);
        TSIC_PIN.borrow_ref_mut(cs).replace(pin);
    });
}

#[embassy_executor::task]
pub async fn tsic_task() -> ! {
    let mut decoder = cc_tsic::Decoder::new();
    let mut ticker = Ticker::every(Duration::from_millis(20));
    let mut last_ok = Instant::now();
    let mut errors = 0u32;
    loop {
        ticker.next().await;
        let mut batch = [(false, 0u32); EDGE_RING];
        let n = critical_section::with(|cs| {
            let mut e = EDGES.borrow_ref_mut(cs);
            let n = e.len;
            for (i, slot) in batch.iter_mut().take(n).enumerate() {
                *slot = e.buf[(e.head + i) % EDGE_RING];
            }
            e.head = (e.head + n) % EDGE_RING;
            e.len = 0;
            n
        });
        for &(rising, t) in &batch[..n] {
            match decoder.edge(rising, t) {
                Some(Ok(raw)) => {
                    let c = f64::from(cc_tsic::celsius(raw));
                    shared::publish(|p| {
                        p.temperature = Some(c);
                        p.temperature_fault = false;
                    });
                    last_ok = Instant::now();
                    errors = 0;
                }
                Some(Err(_)) => errors += 1,
                None => {}
            }
        }
        // No frames at all: the sensor is missing or its wire is broken.
        if last_ok.elapsed() > Duration::from_secs(1) || errors > FAULT_AFTER_ERRORS {
            shared::publish(|p| p.temperature_fault = true);
        }
    }
}

// --- DS18B20 (1-Wire) ---------------------------------------------------------------

/// Bit slots must not be interrupted; each lasts at most 70 µs.
struct Critical;

impl cc_onewire::Slot for Critical {
    fn with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        critical_section::with(|_| f())
    }
}

#[embassy_executor::task]
pub async fn ds18b20_task(pin: Flex<'static>) -> ! {
    let mut d = Delay;
    let mut s = Critical;
    let mut sensor = cc_onewire::OneWire::new(pin).map(cc_onewire::Ds18b20::new);
    if let Ok(sensor) = &mut sensor
        && let Err(e) = sensor.configure(&mut d, &mut s)
    {
        warn!("ds18b20: configure failed: {e:?}");
    }
    let mut errors = 0u32;
    loop {
        let reading = match &mut sensor {
            Ok(sensor) => match sensor.start(&mut d, &mut s) {
                Ok(()) => {
                    Timer::after(Duration::from_millis(u64::from(cc_onewire::CONVERSION_MS_11BIT))).await;
                    sensor.read_celsius(&mut d, &mut s)
                }
                Err(e) => Err(e),
            },
            Err(e) => Err(*e),
        };
        match reading {
            Ok(c) => {
                errors = 0;
                shared::publish(|p| {
                    p.temperature = Some(f64::from(c));
                    p.temperature_fault = false;
                });
            }
            Err(e) => {
                errors += 1;
                if errors == 1 || errors.is_multiple_of(100) {
                    warn!("ds18b20: {e:?}");
                }
                if errors > 3 {
                    shared::publish(|p| p.temperature_fault = true);
                }
                Timer::after(Duration::from_millis(200)).await;
            }
        }
    }
}

// --- HX711 scale --------------------------------------------------------------------

#[cfg(feature = "hx711-scale")]
mod hx711 {
    use embassy_time::{Delay, Duration, Timer};
    use esp_hal::gpio::{Input, Output};

    use super::{shared, warn};
    use cc_core::scale::ScaleReading;
    use log::info;

    /// A 24-pulse read must not be stretched past 60 µs per pulse (the HX711 powers
    /// down), so each pulse runs with interrupts off.
    struct Critical;

    impl cc_hx711::Pulse for Critical {
        fn with<R>(&mut self, f: impl FnOnce() -> R) -> R {
            critical_section::with(|_| f())
        }
    }

    async fn run<const N: usize>(clk: Output<'static>, data: [Input<'static>; N]) -> ! {
        let mut hx = match cc_hx711::Hx711::new(clk, data, cc_hx711::Gain::A128) {
            Ok(h) => h,
            Err(e) => {
                warn!("hx711: {e:?}");
                loop {
                    Timer::after(Duration::from_secs(3600)).await;
                }
            }
        };
        info!("hx711: {N} load cell(s)");
        loop {
            // 10 samples per second.
            Timer::after(Duration::from_millis(20)).await;
            if !hx.is_ready().unwrap_or(false) {
                continue;
            }
            if let Ok(v) = hx.read(&mut Delay, &mut Critical) {
                let reading = ScaleReading::Raw {
                    cell1: v[0],
                    cell2: if N > 1 { Some(v[N - 1]) } else { None },
                };
                shared::publish(|p| p.scale = Some(reading));
            }
        }
    }

    #[embassy_executor::task]
    pub async fn single(clk: Output<'static>, data: Input<'static>) -> ! {
        run(clk, [data]).await
    }

    #[embassy_executor::task]
    pub async fn dual(clk: Output<'static>, data: [Input<'static>; 2]) -> ! {
        run(clk, data).await
    }
}

#[cfg(feature = "hx711-scale")]
pub use hx711::{dual as hx711_dual_task, single as hx711_single_task};

// --- ABP2 pressure ------------------------------------------------------------------

#[cfg(feature = "pressure")]
#[embassy_executor::task]
pub async fn pressure_task(i2c: crate::I2cDev) -> ! {
    // ABP2LANT010BG2A3XX: 0..10 bar gauge.
    let mut sensor = cc_abp2::Abp2::new(i2c, cc_abp2::DEFAULT_ADDRESS, 0.0, 10.0);
    let mut ticker = Ticker::every(Duration::from_millis(100));
    let mut errors = 0u32;
    loop {
        ticker.next().await;
        if sensor.start().is_err() {
            errors += 1;
            if errors == 1 {
                warn!("pressure sensor not responding");
            }
            continue;
        }
        Timer::after(Duration::from_millis(u64::from(cc_abp2::CONVERSION_MS))).await;
        if let Ok(r) = sensor.read() {
            errors = 0;
            shared::publish(|p| p.pressure = Some(r.bar));
        }
    }
}
