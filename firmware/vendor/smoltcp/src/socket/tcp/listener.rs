// CleverCoffee patch (firmware/Cargo.toml, ADR 0008): a listen backlog, in the
// direction a maintainer proposed on smoltcp-rs/smoltcp#1133. The listener only
// queues connection requests; `Socket::accept` hands one to an idle socket, which
// answers it with its own state machine (SYN-ACK, handshake, first data).

use managed::ManagedSlice;

#[cfg(feature = "async")]
use core::task::Waker;

#[cfg(feature = "async")]
use crate::socket::WakerRegistration;
use crate::socket::{Context, PollAt};
use crate::time::{Duration, Instant};
use crate::wire::{
    IpEndpoint, IpListenEndpoint, IpProtocol, IpRepr, TcpControl, TcpRepr, TcpSeqNumber,
    TcpTimestampRepr,
};

use super::ListenError;

/// A request is skipped by [`Listener::accept`] once its client has not retransmitted
/// its SYN for this long, doubled for every retransmission seen. Clients retransmit an
/// unanswered SYN with a doubling backoff (Linux, macOS: after 1, 2, 4, 8 s; Windows
/// starts at 3 s). One that stopped has given up (browsers drop connections they opened
/// ahead); answering it would hold a socket until its handshake times out. A fixed 3 s
/// skipped live macOS clients during their 4 s pause and cost them 7 s on the bench.
pub const MAX_WAIT: Duration = Duration::from_secs(3);

/// A connection request (a SYN) waiting in a [`Listener`]'s backlog.
///
/// It holds what the SYN said, not a buffer: [`Socket::accept`](super::Socket::accept)
/// replays it into a socket as if it had just arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PendingConnection {
    pub(super) local: IpEndpoint,
    pub(super) remote: IpEndpoint,
    seq_number: TcpSeqNumber,
    window_len: u16,
    window_scale: Option<u8>,
    max_seg_size: Option<u16>,
    sack_permitted: bool,
    timestamp: Option<TcpTimestampRepr>,
    /// When its latest SYN arrived.
    seen: Instant,
    /// Retransmissions of the SYN seen so far.
    retries: u8,
}

impl PendingConnection {
    fn from_syn(ip_repr: &IpRepr, repr: &TcpRepr, seen: Instant) -> Self {
        Self {
            local: IpEndpoint::new(ip_repr.dst_addr(), repr.dst_port),
            remote: IpEndpoint::new(ip_repr.src_addr(), repr.src_port),
            seq_number: repr.seq_number,
            window_len: repr.window_len,
            window_scale: repr.window_scale,
            max_seg_size: repr.max_seg_size,
            sack_permitted: repr.sack_permitted,
            timestamp: repr.timestamp,
            seen,
            retries: 0,
        }
    }

    /// Until when the client is expected to retransmit, at the latest.
    fn expires(&self) -> Instant {
        self.seen + MAX_WAIT * (1u32 << self.retries.min(5))
    }

    /// The client's address and port.
    pub fn remote_endpoint(&self) -> IpEndpoint {
        self.remote
    }

    /// The address and port the client connected to.
    pub fn local_endpoint(&self) -> IpEndpoint {
        self.local
    }

    /// The SYN, rebuilt for [`Socket::process`](super::Socket::process).
    pub(super) fn syn(&self) -> (IpRepr, TcpRepr<'static>) {
        let ip_repr = IpRepr::new(self.remote.addr, self.local.addr, IpProtocol::Tcp, 0, 64);
        let tcp_repr = TcpRepr {
            src_port: self.remote.port,
            dst_port: self.local.port,
            control: TcpControl::Syn,
            seq_number: self.seq_number,
            ack_number: None,
            window_len: self.window_len,
            window_scale: self.window_scale,
            max_seg_size: self.max_seg_size,
            sack_permitted: self.sack_permitted,
            sack_ranges: [None; 3],
            timestamp: self.timestamp,
            payload: &[],
        };
        (ip_repr, tcp_repr)
    }
}

/// A TCP listener: queues connection requests on its endpoint until a
/// [`Socket`](super::Socket) is free to take one.
///
/// Without one, a SYN that finds no socket in the LISTEN state is answered with a RST,
/// so a server needs a socket (with its buffers) per client that may connect at the
/// same time. With a listener, a request is held in the backlog and answered once a
/// socket calls [`Socket::accept`](super::Socket::accept). A request that finds the
/// backlog full is dropped, not refused: the client retransmits its SYN.
///
/// Sockets in the LISTEN state on the same endpoint still take requests first.
#[derive(Debug)]
pub struct Listener<'a> {
    endpoint: Option<IpListenEndpoint>,
    backlog: ManagedSlice<'a, Option<PendingConnection>>,
    #[cfg(feature = "async")]
    waker: WakerRegistration,
}

impl<'a> Listener<'a> {
    /// A closed listener with room for `backlog.len()` waiting requests.
    pub fn new<T>(backlog: T) -> Listener<'a>
    where
        T: Into<ManagedSlice<'a, Option<PendingConnection>>>,
    {
        let mut backlog = backlog.into();
        for entry in backlog.iter_mut() {
            *entry = None;
        }
        Listener {
            endpoint: None,
            backlog,
            #[cfg(feature = "async")]
            waker: WakerRegistration::new(),
        }
    }

    /// Start queueing requests for `endpoint`.
    pub fn listen<T>(&mut self, endpoint: T) -> Result<(), ListenError>
    where
        T: Into<IpListenEndpoint>,
    {
        let endpoint = endpoint.into();
        if endpoint.port == 0 {
            return Err(ListenError::Unaddressable);
        }
        match self.endpoint {
            Some(current) if current == endpoint => Ok(()),
            Some(_) => Err(ListenError::InvalidState),
            None => {
                self.endpoint = Some(endpoint);
                Ok(())
            }
        }
    }

    /// Stop listening and forget the waiting requests (their clients time out).
    pub fn close(&mut self) {
        self.endpoint = None;
        for entry in self.backlog.iter_mut() {
            *entry = None;
        }
    }

    pub fn is_listening(&self) -> bool {
        self.endpoint.is_some()
    }

    /// Number of waiting requests.
    pub fn pending(&self) -> usize {
        self.backlog.iter().filter(|e| e.is_some()).count()
    }

    /// The oldest waiting request, removed from the backlog. Hand it to an idle socket
    /// with [`Socket::accept`](super::Socket::accept). Requests whose client has not
    /// retransmitted for [`MAX_WAIT`] are dropped on the way.
    pub fn accept(&mut self, now: Instant) -> Option<PendingConnection> {
        loop {
            let first = self.backlog.first_mut()?.take()?;
            // Keep arrival order: shift the rest up.
            let len = self.backlog.len();
            for i in 1..len {
                self.backlog[i - 1] = self.backlog[i].take();
            }
            if now < first.expires() {
                return Some(first);
            }
            net_trace!("tcp listener: {} gave up, skipped", first.remote);
        }
    }

    /// Wake `waker` when a request arrives (one waker; a later call replaces it).
    #[cfg(feature = "async")]
    pub fn register_waker(&mut self, waker: &Waker) {
        self.waker.register(waker)
    }

    pub(crate) fn accepts(&self, ip_repr: &IpRepr, repr: &TcpRepr) -> bool {
        let Some(endpoint) = self.endpoint else {
            return false;
        };
        repr.control == TcpControl::Syn
            && repr.ack_number.is_none()
            && repr.dst_port == endpoint.port
            && endpoint.addr.is_none_or(|addr| addr == ip_repr.dst_addr())
    }

    /// Queues a SYN that [`Self::accepts`]. A retransmitted SYN replaces its queued
    /// copy; when the backlog is full the SYN is dropped.
    pub(crate) fn process(&mut self, cx: &mut Context, ip_repr: &IpRepr, repr: &TcpRepr) {
        let mut request = PendingConnection::from_syn(ip_repr, repr, cx.now());
        let slot = match self
            .backlog
            .iter()
            .position(|e| e.is_some_and(|e| e.remote == request.remote && e.local == request.local))
        {
            Some(i) => Some(i),
            None => self.backlog.iter().position(|e| e.is_none()),
        };
        match slot {
            Some(i) => {
                net_trace!("tcp listener: queued SYN from {}", request.remote);
                if let Some(queued) = self.backlog[i] {
                    request.retries = queued.retries.saturating_add(1);
                }
                self.backlog[i] = Some(request);
                #[cfg(feature = "async")]
                self.waker.wake();
            }
            None => net_trace!("tcp listener: backlog full, SYN from {} dropped", request.remote),
        }
    }

    pub(crate) fn poll_at(&self, _cx: &mut Context) -> PollAt {
        // Nothing to send: sockets answer the requests.
        PollAt::Ingress
    }
}
