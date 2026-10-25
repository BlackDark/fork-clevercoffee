//! Networking: WiFi station (home network) and access point (setup portal), each with
//! its own IP stack. The web server listens on both.
//!
//! Without credentials, or when the station cannot connect for
//! [`AP_FALLBACK_AFTER`], the access point `CleverCoffee-XXXX` opens (password
//! [`AP_PASSWORD`]) and the same web UI is reachable at `http://192.168.4.1`.

pub mod portal;
pub mod web;

#[cfg(feature = "mqtt")]
pub mod mqtt;

pub mod mdns;

use core::net::Ipv4Addr;

use embassy_futures::select::{Either, select};
use embassy_net::tcp::{PendingConnection, TcpListener};
use embassy_net::{Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Instant, Timer};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, Interface, WifiController, ap, ap::AccessPointConfig,
    sta::StationConfig,
};
use heapless::String;
use log::{info, warn};

use crate::shared;

pub const AP_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);
pub const AP_PASSWORD: &str = "CleverCoffee";
pub const AP_FALLBACK_AFTER: Duration = Duration::from_secs(120);
/// Station retries while the access point is open, and none while a client is on it.
/// Each attempt scans every channel and takes the radio off the access point's: at the
/// normal 5 s the portal barely answered (macOS showed a blank portal window).
pub const STATION_RETRY_WITH_AP: Duration = Duration::from_secs(60);

pub struct Stacks {
    pub sta: Stack<'static>,
    pub ap: Stack<'static>,
    /// Port 80 on each network: connection requests wait here for an HTTP worker.
    pub sta_http: TcpListener<'static>,
    pub ap_http: TcpListener<'static>,
}

/// Connection requests a network holds for the HTTP workers. A page load opens about
/// eight connections, a second tab as many again.
const HTTP_BACKLOG: usize = 8;

/// What the WiFi task needs from the config, read once at boot.
pub struct WifiSettings {
    pub ssid: String<32>,
    pub password: String<64>,
    pub hostname: String<32>,
    pub offline: bool,
}

pub fn ap_ssid(mac: [u8; 6]) -> String<32> {
    let mut s = String::new();
    let _ = core::fmt::write(&mut s, format_args!("CleverCoffee-{:02X}{:02X}", mac[4], mac[5]));
    s
}

fn station(w: &WifiSettings) -> StationConfig {
    let auth = if w.password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(w.password.as_str().try_into().unwrap_or_default())
    };
    StationConfig::default()
        .with_ssid(w.ssid.as_str().try_into().unwrap_or_default())
        .with_authentication(auth)
}

fn access_point(ssid: &str) -> AccessPointConfig {
    AccessPointConfig::default()
        .with_ssid(ssid.try_into().unwrap_or_default())
        .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
            AP_PASSWORD.try_into().unwrap_or_default(),
        ))
}

pub fn initial_config(w: &WifiSettings, ap_ssid: &str) -> WifiConfig {
    if w.ssid.is_empty() {
        WifiConfig::AccessPoint(access_point(ap_ssid))
    } else {
        WifiConfig::Station(station(w))
    }
}

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($val)
    }};
}

/// Builds both IP stacks; returns them with their runners.
pub fn stacks(
    hostname: &str,
    seed: u64,
) -> (
    &'static Stacks,
    Runner<'static, Interface>,
    Runner<'static, Interface>,
) {
    let mut dhcp = embassy_net::DhcpConfig::default();
    dhcp.hostname = hostname.try_into().ok();
    let (sta, sta_runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(dhcp),
        mk_static!(StackResources<10>, StackResources::new()),
        seed,
    );
    let (ap, ap_runner) = embassy_net::new(
        Interface::access_point(),
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(AP_IP, 24),
            gateway: Some(AP_IP),
            dns_servers: heapless::Vec::new(),
        }),
        mk_static!(StackResources<8>, StackResources::new()),
        seed.rotate_left(17),
    );
    // One `mk_static!` per listener: each expands to its own static cell.
    let sta_backlog = mk_static!([Option<PendingConnection>; HTTP_BACKLOG], [None; HTTP_BACKLOG]);
    let ap_backlog = mk_static!([Option<PendingConnection>; HTTP_BACKLOG], [None; HTTP_BACKLOG]);
    let stacks = Stacks {
        sta,
        ap,
        sta_http: TcpListener::new(sta, sta_backlog, 80).expect("port 80 is valid"),
        ap_http: TcpListener::new(ap, ap_backlog, 80).expect("port 80 is valid"),
    };
    (mk_static!(Stacks, stacks), sta_runner, ap_runner)
}

#[embassy_executor::task(pool_size = 2)]
pub async fn net_task(mut runner: Runner<'static, Interface>) -> ! {
    runner.run().await
}

/// Keeps the station connected; opens the access point when needed.
#[embassy_executor::task]
pub async fn wifi_task(
    mut controller: WifiController<'static>,
    w: WifiSettings,
    ap_name: String<32>,
    sta: Stack<'static>,
) {
    if w.ssid.is_empty() || w.offline {
        info!("wifi: no network configured, access point {ap_name} open");
        shared::with(|s| s.net.ap_active = true);
        return;
    }
    let mut ap_open = false;
    let mut ap_clients = 0u8;
    let mut failing_since = embassy_time::Instant::now();
    loop {
        match controller.connect_async().await {
            Ok(_) => {
                info!("wifi: connected to {}", w.ssid);
                sta.wait_config_up().await;
                if let Some(c) = sta.config_v4() {
                    info!("wifi: address {}", c.address.address());
                    shared::with(|s| {
                        s.net.wifi_connected = true;
                        s.net.ip = c.address.address().octets();
                    });
                }
                if ap_open {
                    restart_when_idle().await;
                }
                // Refresh the RSSI until the link drops.
                loop {
                    let rssi = controller.rssi().unwrap_or(-100);
                    shared::with(|s| {
                        s.net.rssi = rssi.clamp(-128, 0) as i8;
                        s.live.wifi_rssi = s.net.rssi;
                        s.live.wifi_connected = true;
                    });
                    match select(
                        controller.wait_for_disconnect_async(),
                        Timer::after(Duration::from_secs(5)),
                    )
                    .await
                    {
                        Either::First(_) => break,
                        Either::Second(()) => {}
                    }
                }
                warn!("wifi: disconnected");
                shared::with(|s| {
                    s.net.wifi_connected = false;
                    s.net.reconnects += 1;
                    s.live.wifi_connected = false;
                });
                failing_since = embassy_time::Instant::now();
            }
            Err(e) => {
                warn!("wifi: connect failed: {e:?}");
                if !ap_open && failing_since.elapsed() > AP_FALLBACK_AFTER {
                    info!("wifi: opening access point {ap_name}");
                    let cfg = WifiConfig::AccessPointStation(station(&w), access_point(&ap_name));
                    if controller.set_config(&cfg).is_ok() {
                        ap_open = true;
                        shared::with(|s| s.net.ap_active = true);
                    }
                }
                if ap_open {
                    wait_for_station_retry(&controller, &mut ap_clients).await;
                } else {
                    Timer::after(Duration::from_secs(5)).await;
                }
            }
        }
    }
}

/// The home network is back while the fallback access point is open. A restart
/// closes the access point (its password is printed in the docs) and brings the
/// device back to one network, which the HTTP workers rely on (`web::task`).
/// It waits until no shot, steam or hot water is running.
async fn restart_when_idle() -> ! {
    info!("wifi: home network back, restarting once the machine is idle to close the access point");
    loop {
        if shared::with(|s| s.machine.status(&s.cfg).idle_for_restart()) {
            shared::RESTART.signal(shared::RestartKind::Plain);
            core::future::pending::<()>().await;
        }
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Waits [`STATION_RETRY_WITH_AP`], and on while a client is on the access point.
/// Clients that join during a connect attempt are missed; they get the 60 s retries.
async fn wait_for_station_retry(controller: &WifiController<'static>, clients: &mut u8) {
    let mut until = Instant::now() + STATION_RETRY_WITH_AP;
    loop {
        match select(
            Timer::at(until),
            controller.wait_for_access_point_connected_event_async(),
        )
        .await
        {
            Either::First(()) if *clients == 0 => return,
            Either::First(()) => until = Instant::now() + STATION_RETRY_WITH_AP,
            Either::Second(Ok(ap::EventInfo::Connected(_))) => *clients = clients.saturating_add(1),
            Either::Second(Ok(ap::EventInfo::Disconnected(_))) => *clients = clients.saturating_sub(1),
            Either::Second(Err(_)) => {}
        }
    }
}
