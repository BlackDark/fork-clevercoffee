//! mDNS responder: the machine answers as `<hostname>.local` on the home network.

use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};

use edge_mdns::buf::VecBufAccess;
use edge_mdns::domain::base::Record;
use edge_mdns::domain::base::Ttl;
use edge_mdns::domain::base::iana::Class;
use edge_mdns::domain::rdata::dnssec::RtypeBitmap;
use edge_mdns::domain::rdata::{AllRecordData, Nsec};
use edge_mdns::host::Host;
use edge_mdns::io::{Mdns, bind};
use edge_mdns::{HostAnswer, HostAnswers, HostAnswersMdnsHandler, MdnsError, NameSlice, RecordDataChain};
use edge_nal::UdpSplit;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use heapless::String;
use log::{info, warn};

/// Type bitmap of an NSEC record listing only A (RFC 4034 §4.1.2: window 0, one
/// byte, bit 1).
const ONLY_A: &[u8] = &[0x00, 0x01, 0x40];

/// The host's A record plus an NSEC record saying it has no other address type.
/// Without it a resolver asking for AAAA gets no answer and waits for its timeout:
/// macOS took 5 s per connection (RFC 6762 §6.1, negative responses).
struct V4Only<'a>(Host<'a>);

impl HostAnswers for V4Only<'_> {
    fn visit<F, E>(&self, mut f: F) -> Result<(), E>
    where
        F: FnMut(HostAnswer) -> Result<(), E>,
        E: From<MdnsError>,
    {
        self.0.visit(&mut f)?;
        let owner = &[self.0.hostname, "local"];
        let types = RtypeBitmap::from_octets(ONLY_A).map_err(|_| MdnsError::InvalidMessage)?;
        f(Record::new(
            NameSlice::new(owner),
            Class::IN,
            self.0.ttl,
            RecordDataChain::Next(AllRecordData::Nsec(Nsec::new(NameSlice::new(owner), types))),
        ))
    }
}

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, hostname: String<32>) -> ! {
    // Buffers on the heap, see `web::task`. Answers (one A record) are small; queries
    // larger than 1 KB are dropped, which costs an answer only if ours was among them.
    let buffers = alloc::boxed::Box::new(UdpBuffers::<1, 512, 1024, 2>::new());
    let udp = Udp::new(stack, &*buffers);
    let recv_buf = alloc::boxed::Box::new(VecBufAccess::<NoopRawMutex, 1024>::new());
    let send_buf = alloc::boxed::Box::new(VecBufAccess::<NoopRawMutex, 512>::new());
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
                    mdns.run(HostAnswersMdnsHandler::new(V4Only(host))),
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
