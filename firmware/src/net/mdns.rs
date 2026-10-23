//! mDNS responder: the machine answers as `<hostname>.local` on the home network.

use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};

use edge_mdns::HostAnswersMdnsHandler;
use edge_mdns::buf::VecBufAccess;
use edge_mdns::domain::base::Ttl;
use edge_mdns::host::Host;
use edge_mdns::io::{Mdns, bind};
use edge_nal::UdpSplit;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use heapless::String;
use log::{info, warn};

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, hostname: String<32>) -> ! {
    // Buffers on the heap, see `web::task`.
    let buffers = alloc::boxed::Box::new(UdpBuffers::<1, 1500, 1500, 2>::new());
    let udp = Udp::new(stack, &*buffers);
    let recv_buf = alloc::boxed::Box::new(VecBufAccess::<NoopRawMutex, 1500>::new());
    let send_buf = alloc::boxed::Box::new(VecBufAccess::<NoopRawMutex, 1500>::new());
    let signal = Signal::<NoopRawMutex, ()>::new();
    loop {
        stack.wait_config_up().await;
        let Some(ip) = stack.config_v4().map(|c| c.address.address()) else {
            continue;
        };
        info!("mdns: {hostname}.local -> {ip}");
        let host = Host {
            hostname: &hostname,
            ipv4: ip,
            ipv6: Ipv6Addr::UNSPECIFIED,
            ttl: Ttl::from_secs(60),
        };
        match bind(
            &udp,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5353)),
            Some(Ipv4Addr::UNSPECIFIED),
            None,
        )
        .await
        {
            Ok(mut socket) => {
                let (recv, send) = socket.split();
                let mdns = Mdns::new(
                    Some(ip),
                    None,
                    recv,
                    send,
                    &*recv_buf,
                    &*send_buf,
                    esp_hal::rng::Rng::new(),
                    &signal,
                );
                let r = embassy_futures::select::select(
                    mdns.run(HostAnswersMdnsHandler::new(&host)),
                    stack.wait_config_down(),
                )
                .await;
                if let embassy_futures::select::Either::First(Err(e)) = r {
                    warn!("mdns stopped: {e:?}");
                }
            }
            Err(e) => warn!("mdns: bind failed: {e:?}"),
        }
        Timer::after(Duration::from_secs(1)).await;
    }
}
