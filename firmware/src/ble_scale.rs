//! Bluetooth scales (feature `ble-scale`): scan for a supported name, connect, find
//! the protocol by its GATT service, then stream weight notifications. Protocol
//! details live in `cc-blescale`.

use core::cell::Cell;

use cc_blescale::{Kind, Uuid as ScaleUuid};
use cc_core::scale::ScaleReading;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, Either3, select, select3};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use esp_radio::ble::controller::BleConnector;
use log::{info, warn};
use static_cell::StaticCell;
use trouble_host::prelude::*;

use crate::shared;

type Ble = ExternalController<BleConnector<'static>, 1>;
type Host = trouble_host::Stack<'static, Ble, DefaultPacketPool>;

fn uuid(u: ScaleUuid) -> Uuid {
    match u {
        ScaleUuid::Short(v) => Uuid::new_short(v),
        ScaleUuid::Long(b) => Uuid::new_long(b),
    }
}

/// Remembers the first supported scale seen in advertisements.
struct Finder {
    found: Signal<NoopRawMutex, Address>,
    scanning: Cell<bool>,
}

impl EventHandler for Finder {
    fn on_adv_reports(&self, reports: bt_hci::param::LeAdvReportsIter) {
        if !self.scanning.get() {
            return;
        }
        for r in reports.flatten() {
            if cc_blescale::adv_name(r.data).is_some_and(cc_blescale::is_supported_name) {
                info!("ble scale: found {:?}", cc_blescale::adv_name(r.data));
                self.scanning.set(false);
                self.found.signal(Address {
                    kind: r.addr_kind,
                    addr: r.addr,
                });
                return;
            }
        }
    }
}

/// A static random address derived from the chip MAC (top two bits set, as BLE
/// requires for static random addresses).
fn ble_address() -> [u8; 6] {
    let mut a = [0u8; 6];
    a.copy_from_slice(esp_hal::efuse::base_mac_address().as_bytes());
    a.reverse();
    a[5] |= 0xC0;
    a
}

pub fn start(spawner: &Spawner, bt: esp_hal::peripherals::BT<'static>) {
    match BleConnector::new(bt, Default::default()) {
        Ok(connector) => spawner.spawn(task(ExternalController::new(connector)).unwrap()),
        Err(e) => warn!("ble: {e:?}"),
    }
}

#[embassy_executor::task]
async fn task(controller: Ble) {
    static RESOURCES: StaticCell<HostResources<Ble, DefaultPacketPool, 1, 1>> = StaticCell::new();
    static STACK: StaticCell<Host> = StaticCell::new();
    let resources = RESOURCES.init(HostResources::new());
    let stack: &'static Host = STACK.init(
        trouble_host::new(controller, resources)
            .set_random_address(Address::random(ble_address()))
            .build(),
    );
    let mut runner = stack.runner();
    let mut central = stack.central();
    let finder = Finder {
        found: Signal::new(),
        scanning: Cell::new(false),
    };
    let _ = select(runner.run_with_handler(&finder), async {
        loop {
            finder.scanning.set(true);
            let mut scanner = Scanner::new(central);
            let config = ScanConfig {
                active: true,
                ..Default::default()
            };
            let found = match scanner.scan(&config).await {
                Ok(session) => {
                    let a = finder.found.wait().await;
                    drop(session);
                    Some(a)
                }
                Err(e) => {
                    warn!("ble scale: scan failed: {e:?}");
                    None
                }
            };
            central = scanner.into_inner();
            let Some(addr) = found else {
                Timer::after(Duration::from_secs(10)).await;
                continue;
            };
            let config = ConnectConfig {
                scan_config: ScanConfig {
                    filter_accept_list: &[addr],
                    ..Default::default()
                },
                connect_params: Default::default(),
            };
            match central.connect(&config).await {
                Ok(conn) => {
                    if let Err(e) = session(stack, &conn).await {
                        warn!("ble scale: {e}");
                    }
                }
                Err(e) => warn!("ble scale: connect failed: {e:?}"),
            }
            shared::with(|s| s.net.ble_scale_connected = false);
            Timer::after(Duration::from_secs(2)).await;
        }
    })
    .await;
}

async fn session(
    stack: &'static Host,
    conn: &Connection<'static, DefaultPacketPool>,
) -> Result<(), &'static str> {
    let client = GattClient::<Ble, DefaultPacketPool, 8>::new(stack, conn)
        .await
        .map_err(|_| "gatt setup failed")?;
    let r = select(client.task(), async {
        let (kind, write, notify) = probe(&client).await.ok_or("no known scale protocol")?;
        info!("ble scale: {kind:?} protocol");
        let mut listener = client
            .subscribe(&notify, false)
            .await
            .map_err(|_| "subscribe failed")?;
        for msg in kind.handshake() {
            client
                .write_characteristic(&write, msg)
                .await
                .map_err(|_| "handshake failed")?;
        }
        shared::with(|s| s.net.ble_scale_connected = true);
        let mut decent_counter = 0u8;
        let mut last_packet = Instant::now();
        let mut next_heartbeat = Instant::now() + Duration::from_millis(cc_blescale::HEARTBEAT_PERIOD_MS);
        loop {
            let deadline = Timer::at(
                next_heartbeat.min(last_packet + Duration::from_millis(cc_blescale::MAX_PACKET_PERIOD_MS)),
            );
            match select3(listener.next(), shared::SCALE_TARE.wait(), deadline).await {
                Either3::First(n) => {
                    if let Some(g) = cc_blescale::parse_weight(kind, n.as_ref()) {
                        last_packet = Instant::now();
                        shared::publish(|p| p.scale = Some(ScaleReading::Grams(g)));
                    }
                }
                Either3::Second(()) => {
                    let mut buf = [0u8; 7];
                    let n = cc_blescale::tare(kind, &mut decent_counter, &mut buf);
                    let _ = client.write_characteristic(&write, &buf[..n]).await;
                }
                Either3::Third(()) => {
                    if last_packet.elapsed() > Duration::from_millis(cc_blescale::MAX_PACKET_PERIOD_MS) {
                        return Err("scale went silent");
                    }
                    if kind.needs_heartbeat() {
                        let _ = client.write_characteristic(&write, cc_blescale::HEARTBEAT).await;
                    }
                    next_heartbeat = Instant::now() + Duration::from_millis(cc_blescale::HEARTBEAT_PERIOD_MS);
                }
            }
        }
    })
    .await;
    match r {
        Either::First(_) => Err("disconnected"),
        Either::Second(r) => r,
    }
}

type Char = Characteristic<[u8]>;

async fn probe(client: &GattClient<'_, Ble, DefaultPacketPool, 8>) -> Option<(Kind, Char, Char)> {
    for kind in Kind::PROBE_ORDER {
        let g = kind.gatt();
        let (w, n) = (
            uuid(ScaleUuid::parse(g.write)?),
            uuid(ScaleUuid::parse(g.notify)?),
        );
        let services = match g.service {
            Some(s) => client.services_by_uuid(&uuid(ScaleUuid::parse(s)?)).await.ok()?,
            None => client.services().await.ok()?,
        };
        for svc in &services {
            if let (Ok(wc), Ok(nc)) = (
                client.characteristic_by_uuid(svc, &w).await,
                client.characteristic_by_uuid(svc, &n).await,
            ) {
                return Some((kind, wc, nc));
            }
        }
    }
    None
}
