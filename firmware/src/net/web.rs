//! The web server: [`cc_web::Api`] over the device's shared state.

use cc_api::Live;
use cc_core::history::History;
use cc_core::{Command, Config, Event, Reply, Status};
use cc_web::{Asset, Backend, OtaError, OtaStatus};
use embassy_net::Stack;
use embassy_time::{Duration, Timer};

use crate::shared::{self, RestartKind};
use crate::{ota, ui_assets};

pub struct Device;

impl Backend for Device {
    fn read<R>(&self, f: impl FnOnce(&Config, &Status, &Live) -> R) -> R {
        let heap = esp_alloc::HEAP.stats();
        shared::with(|s| {
            let st = s.machine.status(&s.cfg);
            let live = Live {
                mqtt_connected: s.net.mqtt_connected,
                free_heap: (heap.size - heap.current_usage) as u32,
                min_free_heap: shared::MIN_FREE_HEAP.load(core::sync::atomic::Ordering::Relaxed),
                ..s.live
            };
            f(&s.cfg, &st, &live)
        })
    }

    fn history<R>(&self, f: impl FnOnce(&History) -> R) -> R {
        shared::with(|s| f(&s.history))
    }

    fn command(&self, cmd: Command) -> Reply {
        let (reply, events) = shared::with(|s| {
            let mut ev = cc_core::machine::Events::new();
            let r = s.machine.command(cmd, &mut s.cfg, &mut ev);
            (r, ev)
        });
        dispatch(&events);
        reply
    }

    fn update_config(&self, f: &mut dyn FnMut(&mut Config) -> bool) {
        let changed = shared::with(|s| {
            let changed = f(&mut s.cfg);
            if changed {
                let mut ev = cc_core::machine::Events::new();
                s.machine.command(Command::ConfigUpdated, &mut s.cfg, &mut ev);
                s.config_dirty = true;
            }
            changed
        });
        if changed {
            shared::PERSIST.signal(());
        }
    }

    fn restart(&self) {
        shared::RESTART.signal(RestartKind::Plain);
    }

    fn factory_reset(&self) {
        shared::RESTART.signal(RestartKind::FactoryReset);
    }

    fn wifi_reset(&self) {
        shared::RESTART.signal(RestartKind::WifiReset);
    }

    fn asset(&self, path: &str) -> Option<Asset> {
        ui_assets::ASSETS
            .iter()
            .find(|(p, ..)| *p == path)
            .map(|&(p, bytes, content_type, gzip)| Asset {
                bytes,
                content_type,
                gzip,
                // Vite puts a content hash into every file name under assets/.
                immutable: p.starts_with("assets/"),
            })
    }

    async fn sleep_ms(&self, ms: u64) {
        Timer::after(Duration::from_millis(ms)).await;
    }

    fn ota_status(&self) -> OtaStatus {
        ota::status()
    }

    fn ota_begin(&self) -> Result<(), OtaError> {
        ota::begin()
    }

    async fn ota_write(&self, data: &[u8]) -> Result<(), OtaError> {
        ota::write(data).await
    }

    async fn ota_finish(&self) -> Result<(), OtaError> {
        ota::finish().await
    }

    fn ota_abort(&self, err: OtaError) {
        ota::abort(err);
    }

    #[cfg(feature = "bench")]
    fn debug_hang(&self) {
        crate::control::HANG.signal(());
    }
}

/// Side effects of a command (persisting, scale tare).
fn dispatch(events: &[Event]) {
    for e in events {
        match e {
            Event::ConfigChanged | Event::ShotCounterChanged => shared::PERSIST.signal(()),
            Event::Scale(cc_core::scale::ScaleAction::BluetoothTare) => shared::SCALE_TARE.signal(()),
            Event::StateChanged { .. } | Event::Reboot => {}
        }
    }
}

static DEVICE: Device = Device;

static CONFIG: picoserve::Config = picoserve::Config::const_default().keep_connection_alive();

/// HTTP workers: two on the home network (one may hold the event stream), one on
/// the access point.
pub const STA_WORKERS: usize = 2;

#[embassy_executor::task(pool_size = 3)]
pub async fn task(id: usize, stack: Stack<'static>) -> ! {
    let app = cc_web::Api::new(&DEVICE).router();
    // Socket and request buffers on the heap (reclaimed RAM), not in the static
    // task pool: main RAM is short once Bluetooth reserves its 64 KB.
    let mut rx = alloc::vec![0; 1024];
    let mut tx = alloc::vec![0; 1536];
    let mut http = alloc::vec![0; 2048];
    picoserve::Server::new(&app, &CONFIG, &mut http)
        .listen_and_serve(id, stack, 80, &mut rx, &mut tx)
        .await
        .into_never()
}
