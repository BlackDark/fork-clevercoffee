//! The control loop: every 10 ms read the switches, feed the machine, drive the
//! relays and LEDs, feed the watchdog. Runs on the high-priority executor so network
//! traffic cannot delay it.

use cc_core::config::{Config, RelayTrigger};
use cc_core::machine::{Event, Inputs, TICK_MS};
use cc_core::scale::ScaleAction;
use embassy_time::{Duration, Instant, Ticker};
use esp_hal::gpio::{Input, Level, Output};
use esp_hal::peripherals::TIMG1;
use esp_hal::timer::timg::Wdt;
use log::{info, warn};

use crate::shared::{self, RestartKind};

pub struct Io {
    pub brew: Input<'static>,
    pub steam: Input<'static>,
    pub water: Input<'static>,
    pub power: Input<'static>,
    pub tank: Input<'static>,
    pub heater: Output<'static>,
    pub pump: Output<'static>,
    pub valve: Output<'static>,
    pub status_led: Option<Output<'static>>,
    pub brew_led: Option<Output<'static>>,
    pub steam_led: Option<Output<'static>>,
}

/// Relay polarity, read once at boot like the GPIO setup.
#[derive(Clone, Copy)]
pub struct Polarity {
    pub heater: bool,
    pub pump: bool,
    pub valve: bool,
    pub status_led: bool,
    pub brew_led: bool,
    pub steam_led: bool,
}

impl Polarity {
    pub fn from_config(c: &Config) -> Self {
        let active_high = |t| t == RelayTrigger::High;
        Self {
            heater: active_high(c.hardware_relays_heater_trigger_type),
            pump: active_high(c.hardware_relays_pump_trigger_type),
            valve: active_high(c.hardware_relays_valve_trigger_type),
            status_led: !c.hardware_leds_status_inverted,
            brew_led: !c.hardware_leds_brew_inverted,
            steam_led: !c.hardware_leds_steam_inverted,
        }
    }

    /// The level that switches a relay off.
    pub fn off_level(active_high: bool) -> Level {
        if active_high { Level::Low } else { Level::High }
    }
}

fn drive(pin: &mut Output<'static>, on: bool, active_high: bool) {
    pin.set_level(if on == active_high {
        Level::High
    } else {
        Level::Low
    });
}

/// Every relay off. Used at boot, on panic and before restarting.
pub fn all_off(io: &mut Io, p: Polarity) {
    drive(&mut io.heater, false, p.heater);
    drive(&mut io.pump, false, p.pump);
    drive(&mut io.valve, false, p.valve);
}

#[embassy_executor::task]
pub async fn run(mut io: Io, polarity: Polarity, mut wdt: Wdt<TIMG1<'static>>) -> ! {
    let mut ticker = Ticker::every(Duration::from_millis(TICK_MS));
    let start = Instant::now();
    let mut last_history_ms = 0u64;
    let mut slowest_us = 0u64;
    loop {
        ticker.next().await;
        let t0 = Instant::now();
        let now_ms = t0.duration_since(start).as_millis();
        let pending = shared::take_pending();
        let inputs = Inputs {
            now_ms,
            temperature: pending.temperature,
            temperature_fault: pending.temperature_fault,
            brew_switch: io.brew.is_high(),
            steam_switch: io.steam.is_high(),
            hot_water_switch: io.water.is_high(),
            power_switch: io.power.is_high(),
            water_tank: io.tank.is_high(),
            scale: pending.scale,
            pressure: pending.pressure,
            ota_active: false,
        };

        let (outputs, events, timing_debug) = shared::with(|s| {
            let inputs = Inputs {
                ota_active: s.ota_active,
                ..inputs
            };
            let events = s.machine.tick(&mut s.cfg, &inputs);
            if now_ms.saturating_sub(last_history_ms) >= 1000 {
                last_history_ms = now_ms;
                let st = s.machine.status(&s.cfg);
                s.history.offer(cc_core::history::Point {
                    temperature: st.temperature as f32,
                    setpoint: st.setpoint as f32,
                    heater_power: st.heater_power as f32,
                });
                s.live.uptime_ms = now_ms;
            }
            for e in &events {
                match e {
                    Event::ConfigChanged => s.config_dirty = true,
                    Event::ShotCounterChanged => s.shots_dirty = true,
                    _ => {}
                }
            }
            (s.machine.outputs(), events, s.cfg.system_timing_debug_enabled)
        });

        drive(&mut io.heater, outputs.heater, polarity.heater);
        drive(&mut io.pump, outputs.pump, polarity.pump);
        drive(&mut io.valve, outputs.valve, polarity.valve);
        if let Some(l) = &mut io.status_led {
            drive(l, outputs.status_led, polarity.status_led);
        }
        if let Some(l) = &mut io.brew_led {
            drive(l, outputs.brew_led, polarity.brew_led);
        }
        if let Some(l) = &mut io.steam_led {
            drive(l, outputs.steam_led, polarity.steam_led);
        }

        for e in events {
            match e {
                Event::StateChanged { from, to } => info!("state {} -> {}", from.name(), to.name()),
                Event::ConfigChanged | Event::ShotCounterChanged => shared::PERSIST.signal(()),
                Event::Scale(ScaleAction::BluetoothTare) => shared::SCALE_TARE.signal(()),
                Event::Reboot => {
                    warn!("power switch long press: restarting");
                    all_off(&mut io, polarity);
                    shared::RESTART.signal(RestartKind::Plain);
                }
            }
        }

        #[cfg(feature = "bench")]
        if HANG.signaled() {
            warn!("bench: control loop hangs, the watchdog must reset the chip");
            all_off(&mut io, polarity);
            loop {
                core::hint::spin_loop();
            }
        }

        wdt.feed();
        let took = t0.elapsed().as_micros();
        if took > slowest_us {
            slowest_us = took;
        }
        if timing_debug && now_ms % 10_000 < TICK_MS {
            info!("control loop: slowest tick {slowest_us} µs");
            slowest_us = 0;
        }
    }
}

static PANIC_POLARITY: critical_section::Mutex<core::cell::Cell<Option<Polarity>>> =
    critical_section::Mutex::new(core::cell::Cell::new(None));

/// Remembers the relay polarity for [`custom_pre_backtrace`].
pub fn arm_panic_shutdown(p: Polarity) {
    critical_section::with(|cs| PANIC_POLARITY.borrow(cs).set(Some(p)));
}

/// Called by `esp-backtrace` before printing a panic: heater, pump and valve off,
/// whatever task panicked and whoever owns the pins.
#[unsafe(no_mangle)]
pub fn custom_pre_backtrace() {
    let Some(p) = critical_section::with(|cs| PANIC_POLARITY.borrow(cs).get()) else {
        return;
    };
    let cfg = esp_hal::gpio::OutputConfig::default();
    // SAFETY: the system is going down; nothing else drives these pins any more and
    // the reset follows right after the backtrace. `Output` has no `Drop`: the
    // level stays after the driver goes out of scope.
    let (heater, pump, valve) = unsafe {
        (
            esp_hal::peripherals::GPIO2::steal(),
            esp_hal::peripherals::GPIO27::steal(),
            esp_hal::peripherals::GPIO17::steal(),
        )
    };
    let _ = Output::new(heater, Polarity::off_level(p.heater), cfg);
    let _ = Output::new(pump, Polarity::off_level(p.pump), cfg);
    let _ = Output::new(valve, Polarity::off_level(p.valve), cfg);
}

/// Bench builds only: blocks the control executor so the watchdog must reset the chip.
#[cfg(feature = "bench")]
pub static HANG: embassy_sync::signal::Signal<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    (),
> = embassy_sync::signal::Signal::new();
