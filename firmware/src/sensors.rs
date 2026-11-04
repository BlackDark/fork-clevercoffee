//! Sensor tasks. Each publishes fresh readings to [`shared::publish`]; the control
//! loop takes them on its next tick.

use embassy_time::{Delay, Duration, Instant, Timer, with_timeout};
use esp_hal::gpio::Flex;
use log::{info, warn};

use crate::shared;

/// A reading older than this counts as missing (the machine then reports a sensor
/// error after its own timeout).
const FAULT_AFTER_ERRORS: u32 = 10;

// --- TSIC 306 (ZACwire) -------------------------------------------------------------
//
// The sensor sends a 20-bit frame every 100 ms. The RMT peripheral measures the pulse
// lengths in hardware at 1 µs, so interrupt latency cannot disturb the bit timing (a
// GPIO interrupt did, and lost a quarter of the frames); `cc_tsic` decodes the pulses.

/// How long the RMT waits for the line to stay high before it ends a reception: longer
/// than the gap between the two packets of a frame (about one bit, 250 µs at the
/// slowest 4 kHz strobe), far shorter than the pause between frames (about 80 ms).
const FRAME_IDLE_US: u16 = 2000;

/// Receiver without glitch filter: the filter would eat into the 26 µs bit margins.
pub fn tsic_rx_config() -> esp_hal::rmt::RxChannelConfig {
    esp_hal::rmt::RxChannelConfig::default()
        .with_clk_divider(80)
        .with_idle_threshold(FRAME_IDLE_US)
        .with_memsize(1)
}

#[embassy_executor::task]
pub async fn tsic_task(mut channel: esp_hal::rmt::Channel<'static, esp_hal::Async, esp_hal::rmt::Rx>) -> ! {
    use esp_hal::gpio::Level;
    use esp_hal::rmt::PulseCode;

    let mut spike_filter = cc_tsic::SpikeFilter::new();
    let mut codes = [PulseCode::default(); 64];
    let mut last_ok = Instant::now();
    let mut errors = 0u32;
    // What the log says about the sensor: a summary when frames went wrong, and the
    // moments it is declared faulty and fine again.
    let (mut ok, mut parity, mut framing, mut rx_errors, mut spikes) = (0u32, 0u32, 0u32, 0u32, 0u32);
    let mut last_rx_error = None;
    let mut faulty = false;
    let mut last_report = Instant::now();
    loop {
        codes.fill(PulseCode::default());
        // A timeout only lets the fault check below run while the line is silent.
        match with_timeout(Duration::from_millis(300), channel.receive(&mut codes)).await {
            Ok(Ok(n)) => {
                // A pulse code holds two pulses; a length of 0 ends the capture.
                let pulses = codes[..n.min(codes.len())]
                    .iter()
                    .flat_map(|c| [(c.level1(), c.length1()), (c.level2(), c.length2())])
                    .take_while(|&(_, len)| len != 0)
                    .map(|(level, len)| (level == Level::High, len));
                match cc_tsic::decode_pulses(pulses) {
                    Ok(raw) => {
                        // Parity cannot catch two flipped bits; the median of three drops
                        // a single wrong value.
                        if let Some((raw, spike)) = spike_filter.push(raw) {
                            let c = f64::from(cc_tsic::celsius(raw));
                            shared::publish(|p| {
                                p.temperature = Some(c);
                                p.temperature_fault = false;
                            });
                            spikes += u32::from(spike);
                        }
                        last_ok = Instant::now();
                        errors = 0;
                        ok += 1;
                    }
                    Err(e) => {
                        errors += 1;
                        match e {
                            cc_tsic::Error::Parity => parity += 1,
                            cc_tsic::Error::Framing => framing += 1,
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                errors += 1;
                rx_errors += 1;
                last_rx_error = Some(e);
                Timer::after(Duration::from_millis(5)).await;
            }
            // Silence is judged below by the time since the last good frame.
            Err(_) => {}
        }
        // No frames at all: the sensor is missing or its wire is broken.
        let silent = last_ok.elapsed() > Duration::from_secs(1);
        if silent || errors > FAULT_AFTER_ERRORS {
            shared::publish(|p| p.temperature_fault = true);
            if !faulty {
                faulty = true;
                // After an outage the old readings must not take part in the median.
                spike_filter = cc_tsic::SpikeFilter::new();
                warn!(
                    "tsic: fault, {} ms since the last good frame, {errors} bad frames in a row",
                    last_ok.elapsed().as_millis()
                );
            }
        } else if faulty && errors == 0 {
            faulty = false;
            info!("tsic: frames are good again");
        }
        if last_report.elapsed() >= Duration::from_secs(10) {
            last_report = Instant::now();
            if parity + framing + rx_errors + spikes > 0 {
                warn!(
                    "tsic: in 10 s {ok} good frames, {spikes} dropped as spikes, {parity} parity, {framing} framing and {rx_errors} receiver errors ({last_rx_error:?})"
                );
            }
            (ok, parity, framing, rx_errors, spikes) = (0, 0, 0, 0, 0);
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
    let mut ticker = embassy_time::Ticker::every(Duration::from_millis(100));
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
