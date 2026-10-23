//! The setup portal on the access point: a DHCP server hands out addresses and
//! a DNS server answers every name with the device, so phones open the web UI.

use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use edge_dhcp::server::{Server, ServerOptions};
use edge_nal::UdpBind;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_net::Stack;
use embassy_time::{Duration, Instant, Timer};
use log::warn;

use super::AP_IP;

#[embassy_executor::task]
pub async fn dns_task(stack: Stack<'static>) -> ! {
    let buffers = alloc::boxed::Box::new(UdpBuffers::<1, 512, 512, 1>::new());
    let udp = Udp::new(stack, &*buffers);
    let mut tx = alloc::vec![0; 512];
    let mut rx = alloc::vec![0; 512];
    loop {
        let r = edge_captive::io::run(
            &udp,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 53)),
            &mut tx,
            &mut rx,
            AP_IP,
            core::time::Duration::from_secs(60),
        )
        .await;
        warn!("captive dns stopped: {r:?}");
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
pub async fn dhcp_task(stack: Stack<'static>) -> ! {
    let buffers = alloc::boxed::Box::new(UdpBuffers::<1, 1024, 1024, 1>::new());
    let udp = Udp::new(stack, &*buffers);
    let mut buf = alloc::vec![0u8; 1024];
    let start = Instant::now();
    let mut server = Server::<_, 4>::new(|| start.elapsed().as_secs(), AP_IP);
    let mut gw = [AP_IP];
    let dns = [AP_IP];
    let mut options = ServerOptions::new(AP_IP, Some(&mut gw));
    options.dns = &dns;
    loop {
        match udp
            .bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 67)))
            .await
        {
            Ok(mut socket) => {
                let r = edge_dhcp::io::server::run(&mut server, &options, &mut socket, &mut buf).await;
                warn!("dhcp server stopped: {r:?}");
            }
            Err(e) => warn!("dhcp server: bind failed: {e:?}"),
        }
        Timer::after(Duration::from_secs(1)).await;
    }
}
