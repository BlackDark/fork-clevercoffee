//! The web server: [`cc_web::Api`] over the device's shared state.

use cc_api::Live;
use cc_core::history::History;
use cc_core::{Command, Config, Event, Reply, Status};
use cc_web::{Asset, Backend, OtaError, OtaStatus};
use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer};

use crate::shared::{self, RestartKind};
use log::{debug, warn};

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
                min_free_stack: crate::stackmon::last(),
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
                log::set_max_level(crate::level_filter(s.cfg.system_log_level));
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

    fn heap_free(&self) -> usize {
        esp_alloc::HEAP.free()
    }

    fn deadline(&self, ms: u64) -> core::pin::Pin<alloc::boxed::Box<dyn core::future::Future<Output = ()>>> {
        alloc::boxed::Box::pin(Timer::after(Duration::from_millis(ms)))
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

/// picoserve's defaults, but 5 s instead of 1 s to write a piece of a response: one
/// lost packet costs about a second of TCP retransmission, and a write timeout ends
/// the connection with a RST (seen under parallel downloads on the bench).
static CONFIG: picoserve::Config = picoserve::Config::new(picoserve::Timeouts {
    start_read_request: Duration::from_secs(5),
    persistent_start_read_request: Duration::from_secs(1),
    read_request: Duration::from_secs(3),
    write: Duration::from_secs(5),
})
.keep_connection_alive();

/// HTTP workers, shared by the home network and the access point: a boot runs one of
/// them at a time (the device restarts when the home network comes back after the
/// fallback, `net::wifi_task`). Each UI tab holds one for its event stream; a
/// connection that finds every worker busy waits for its SYN retry (ADR 0008).
pub const WORKERS: usize = 4;
/// With a Bluetooth scale the controller keeps its 64 KB and the heap is that much
/// smaller: two workers (ADR 0008).
const WORKERS_WITH_BLE: usize = 2;
/// Event streams (one per open UI tab) may hold at most half the workers; the rest
/// stay free for page loads and API calls.
static STREAMS: cc_web::StreamSlots = cc_web::StreamSlots::new(WORKERS / 2);
static STREAMS_WITH_BLE: cc_web::StreamSlots = cc_web::StreamSlots::new(WORKERS_WITH_BLE / 2);

/// Number of workers and the stream cap they share.
pub fn pool(ble_scale: bool) -> (usize, &'static cc_web::StreamSlots) {
    if ble_scale {
        (WORKERS_WITH_BLE, &STREAMS_WITH_BLE)
    } else {
        (WORKERS, &STREAMS)
    }
}

/// The TCP receive buffer is the window a client may send without waiting: 1 KB held a
/// firmware upload to 35–45 KB/s (one window per round trip).
const RX_BUF: usize = 2048;
const TX_BUF: usize = 1536;
const HTTP_BUF: usize = 2048;

/// The network to serve next: the home network if its link is up, else the access
/// point, else whichever comes up first.
async fn pick(stacks: &super::Stacks) -> Stack<'static> {
    loop {
        if stacks.sta.is_link_up() {
            return stacks.sta;
        }
        if stacks.ap.is_link_up() {
            return stacks.ap;
        }
        let _ = select(stacks.sta.wait_link_up(), stacks.ap.wait_link_up()).await;
    }
}

#[embassy_executor::task(pool_size = 4)]
pub async fn task(id: usize, stacks: &'static super::Stacks, streams: &'static cc_web::StreamSlots) -> ! {
    // Buffers only once a network is up.
    let _ = pick(stacks).await;
    let app = cc_web::Api::new(&DEVICE, streams).router();
    // Socket and request buffers on the heap (reclaimed RAM), not in the static task
    // pool: main RAM is short once Bluetooth reserves its 64 KB.
    let mut rx = alloc::vec![0; RX_BUF];
    let mut tx = alloc::vec![0; TX_BUF];
    let mut http = alloc::vec![0; HTTP_BUF];
    loop {
        let stack = pick(stacks).await;
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        // Waiting on a network that goes away (the home network drops, the fallback
        // opens the access point) would hold this worker forever.
        match select(socket.accept(80), stack.wait_link_down()).await {
            Either::First(Ok(())) => {}
            Either::First(Err(e)) => {
                warn!("http {id}: accept: {e:?}");
                continue;
            }
            Either::Second(()) => continue,
        }
        socket.set_keep_alive(Some(Duration::from_secs(30)));
        socket.set_timeout(Some(Duration::from_secs(45)));
        if let Err(e) = picoserve::Server::new(&app, &CONFIG, &mut http)
            .serve(socket)
            .await
        {
            debug!("http {id}: {e:?}");
        }
    }
}
