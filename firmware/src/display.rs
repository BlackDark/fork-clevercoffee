//! The OLED: renders [`cc_display`] screens every 100 ms and flushes a frame only
//! when it changed.

use alloc::boxed::Box;
use cc_core::config::OledType;
use cc_display::canvas::Frame;
use cc_display::{Banner, Net, Ota, View};
use cc_oled::{Controller, Oled};
use embassy_time::{Duration, Instant, Ticker};
use log::warn;

use crate::{I2cDev, ota, shared};

/// The boot logo stays this long.
const SPLASH: Duration = Duration::from_millis(1500);

#[embassy_executor::task]
pub async fn task(i2c: I2cDev) -> ! {
    let (address, controller) = shared::with(|s| {
        let c = match s.cfg.hardware_oled_type {
            OledType::Ssd1306 => Controller::Ssd1306,
            OledType::Sh1106 => Controller::Sh1106,
        };
        (s.cfg.hardware_oled_address.i2c_address(), c)
    });
    let mut oled = Oled::new(i2c, address, controller);
    if let Err(e) = oled.init() {
        warn!("display: init failed ({e:?}); continuing without it");
    }
    let mut frame = Box::new(Frame::default());
    let mut shown = Box::new(Frame::default());
    let mut panel_on = true;
    let start = Instant::now();
    let mut ticker = Ticker::every(Duration::from_millis(100));
    let mut errors = 0u32;
    loop {
        ticker.next().await;
        let ota = ota::status();
        // Copy under the lock, render outside it: the lock is a critical section.
        let (st, cfg, net) = shared::with(|s| {
            let net = Net {
                offline: s.cfg.system_offline_mode,
                wifi_connected: s.net.wifi_connected,
                signal: s.net.signal(),
                wifi_reconnects: s.net.reconnects,
                mqtt_connected: s.net.mqtt_connected,
                ble_scale_connected: s.net.ble_scale_connected,
            };
            (s.machine.status(&s.cfg), Box::new(s.cfg.clone()), net)
        });
        let ota_view = if ota.in_progress {
            Some(Ota::Running {
                percent: ota.progress,
            })
        } else if ota.status == "complete" {
            Some(Ota::Done)
        } else {
            None
        };
        let banner = (start.elapsed() < SPLASH).then_some(Banner {
            line1: "CleverCoffee",
            line2: env!("CARGO_PKG_VERSION"),
        });
        let view = View {
            status: &st,
            cfg: &cfg,
            net,
            uptime_ms: start.elapsed().as_millis(),
            ota: ota_view,
            banner,
        };
        frame.bytes.fill(0);
        let rendered = cc_display::render(&mut frame, &view);
        let result = if rendered.panel_off != !panel_on {
            panel_on = !rendered.panel_off;
            oled.set_on(panel_on)
        } else {
            Ok(())
        };
        let result = result.and_then(|()| {
            if panel_on && frame.bytes != shown.bytes {
                oled.flush(&frame.bytes)?;
                shown.bytes = frame.bytes;
            }
            Ok(())
        });
        if let Err(e) = result {
            errors += 1;
            if errors == 1 || errors.is_multiple_of(600) {
                warn!("display: {e:?}");
            }
            // Re-initialise after a glitch (loose cable, brown-out of the panel).
            let _ = oled.init();
            shown.bytes.fill(0xAA);
        }
    }
}
