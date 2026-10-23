//! Network test firmware (ADR 0001): WiFi AP+STA, BLE scan, HTTP, MQTT, mDNS and
//! captive DNS all at once, to measure the RAM budget on a classic ESP32.

#![no_std]
#![no_main]

use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use embassy_executor::Spawner;
use embassy_net::{Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, ram, rng::Rng, timer::timg::TimerGroup};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
    ap::AccessPointConfig, sta::StationConfig,
};
use log::info;
use picoserve::routing::get;
use static_cell::StaticCell;
use trouble_host::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: StaticCell<$t> = StaticCell::new();
        CELL.uninit().write($val)
    }};
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 94 * 1024);
    esp_alloc::heap_allocator!(size: 24 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let config = Config::AccessPointStation(
        StationConfig::default()
            .with_ssid(option_env!("CC_SSID").unwrap_or("test").try_into().unwrap())
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                option_env!("CC_PASS").unwrap_or("testtest").try_into().unwrap(),
            )),
        AccessPointConfig::default().with_ssid("CleverCoffee-test".try_into().unwrap()),
    );
    let controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(config),
    )
    .unwrap();

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    let ap_ip = Ipv4Addr::new(192, 168, 4, 1);
    let (ap_stack, ap_runner) = embassy_net::new(
        Interface::access_point(),
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(ap_ip, 24),
            gateway: Some(ap_ip),
            dns_servers: Default::default(),
        }),
        mk_static!(StackResources<6>, StackResources::new()),
        seed,
    );
    let (sta_stack, sta_runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<8>, StackResources::new()),
        seed,
    );

    spawner.spawn(connection(controller).unwrap());
    spawner.spawn(net_task(ap_runner).unwrap());
    spawner.spawn(net_task(sta_runner).unwrap());
    for id in 0..2 {
        spawner.spawn(web_task(id, sta_stack).unwrap());
    }
    spawner.spawn(web_task(2, ap_stack).unwrap());
    spawner.spawn(captive_dns(ap_stack, ap_ip).unwrap());
    spawner.spawn(mdns_task(sta_stack).unwrap());
    spawner.spawn(mqtt_task(sta_stack).unwrap());

    let connector = esp_radio::ble::controller::BleConnector::new(peripherals.BT, Default::default()).unwrap();
    let controller: ExternalController<_, 1> = ExternalController::new(connector);
    spawner.spawn(ble_task(controller).unwrap());

    loop {
        let stats = esp_alloc::HEAP.stats();
        info!("heap: {stats}");
        Timer::after(Duration::from_secs(10)).await;
    }
}

type Ble = ExternalController<esp_radio::ble::controller::BleConnector<'static>, 1>;

#[embassy_executor::task]
async fn ble_task(controller: Ble) {
    let mut resources: HostResources<Ble, DefaultPacketPool, 1, 1> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(Address::random([0xff, 0x8f, 0x1b, 0x05, 0xe4, 0xff]))
        .build();
    let mut runner = stack.runner();
    let mut scanner = Scanner::new(stack.central());
    let _ = embassy_futures::join::join(runner.run(), async {
        let config = ScanConfig { active: true, ..Default::default() };
        let _session = scanner.scan(&config).await.unwrap();
        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    })
    .await;
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    loop {
        if let Ok(info) = controller.connect_async().await {
            info!("connected {info:?}");
            let _ = controller.wait_for_disconnect_async().await;
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

static WEB_CONFIG: picoserve::Config = picoserve::Config::const_default().keep_connection_alive();

#[embassy_executor::task(pool_size = 3)]
async fn web_task(id: usize, stack: Stack<'static>) -> ! {
    let app = picoserve::Router::new()
        .route("/", get(|| async { "hello" }))
        .route("/api/status", get(|| async { picoserve::response::Json(("ok", 1)) }));
    let mut rx = [0; 1024];
    let mut tx = [0; 1536];
    let mut http = [0; 2048];
    picoserve::Server::new(&app, &WEB_CONFIG, &mut http)
        .listen_and_serve(id, stack, 80, &mut rx, &mut tx)
        .await
        .into_never()
}

#[embassy_executor::task]
async fn captive_dns(stack: Stack<'static>, ip: Ipv4Addr) {
    let buffers = edge_nal_embassy::UdpBuffers::<1, 512, 512, 1>::new();
    let udp = edge_nal_embassy::Udp::new(stack, &buffers);
    let mut tx = [0; 512];
    let mut rx = [0; 512];
    let _ = edge_captive::io::run(
        &udp,
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 53)),
        &mut tx,
        &mut rx,
        ip,
        core::time::Duration::from_secs(60),
    )
    .await;
}

#[embassy_executor::task]
async fn mdns_task(stack: Stack<'static>) {
    stack.wait_config_up().await;
    let _ = stack;
    // edge-mdns responder is wired in the real firmware; the socket cost is counted here.
    let buffers = edge_nal_embassy::UdpBuffers::<1, 1500, 1500, 2>::new();
    let _udp = edge_nal_embassy::Udp::new(stack, &buffers);
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

#[embassy_executor::task]
async fn mqtt_task(stack: Stack<'static>) {
    use rust_mqtt::{buffer::AllocBuffer, client::Client, client::options::ConnectOptions};
    stack.wait_config_up().await;
    let mut rx = [0; 1024];
    let mut tx = [0; 1024];
    loop {
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut rx, &mut tx);
        if socket.connect((Ipv4Addr::new(192, 168, 1, 2), 1883)).await.is_ok() {
            let mut buffer = AllocBuffer;
            let mut client: Client<'_, '_, _, _, 1, 1, 1, 0, 0, 0, 0> = Client::new(&mut buffer);
            let _ = client.connect(socket, &ConnectOptions::new(), None).await;
        }
        Timer::after(Duration::from_secs(30)).await;
    }
}
