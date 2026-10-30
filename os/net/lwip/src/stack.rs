//! `stack` — the stack half of the semantics shim: the smoltcp family's
//! implementation of the [`crate::lwip_port::Stack`] wall.
//!
//! Selection decision (recorded in the network stage notes,
//! `[ARCH: protocol stack behind the Stack trait]`): the protocol stack body is
//! smoltcp, while Minix's socket semantics (suspendable requests, the data plane
//! over three grants, the sdev reply shape) are carried by a purpose-built shim;
//! service code only knows the [`crate::lwip_port::Stack`] trait, not any concrete
//! stack. This module is that shim on the smoltcp side; the full argument for the
//! mapping boundary is in `25-smoltcp-shim.md`.
//!
//! There are two C counterparts: `lwip_init()` (`lwip.c:208-210`, the stack-library
//! initialization — here it is constructing the interface and the socket set, with
//! the seed handed to the stack through `Config::random_seed` for its random face,
//! the same position as C feeding the seed to `srand48`) and the main timer
//! (`init_timer(&lwip_timer)` plus `recheck_timer = TRUE`, `lwip.c:255-259`; the
//! main loop's `check_lwip_timer`/`expire_timers` are at `lwip.c:315`/`:328`).
//! smoltcp folds "due timers are serviced along with one pass of progress" into a
//! single `Interface::poll`, so [`Stack::poll`]'s return value (the next delivery
//! instant) on the wall is the semantic slot of C's `sys_check_timeouts`; after
//! arming, "check once on entering the loop" is honoured by the caller calling
//! [`Stack::poll`] on its first pass.

use alloc::vec;
use alloc::vec::Vec;

use managed::ManagedSlice;
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{
    ChecksumCapabilities, Device, DeviceCapabilities, Medium, RxToken, TxToken,
};
use smoltcp::socket::raw;
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{
    HardwareAddress, IpAddress, IpCidr, IpEndpoint, IpListenEndpoint, IpVersion,
};

use crate::lwip_port::{PollWhen, Readiness, Stack, StackFamily, StackSocket};
use crate::lwip_port::{TCP_SEND_BUFFER, TCP_WINDOW};
use crate::util;

/// The shim itself: interface, trunk device, socket set and slot table.
///
/// The slot table translates the `index` half of a service-side handle into an
/// in-stack handle: indices are stable (closing only clears a slot and never
/// shifts the slots after it, so a handle the service holds always names the same
/// socket), and the family is recorded per slot — a handle whose family does not
/// match is answered as "already closed" (the wall's contract: default readiness
/// bits, no panic).
pub struct SmoltcpStack<D: Device = TrunkDevice> {
    iface: Interface,
    device: D,
    sockets: SocketSet<'static>,
    slots: Vec<Option<Slot>>,
    /// Ephemeral port allocator (the local port when connect has no pre-bound
    /// one, starting at 49152).
    ephemeral: u16,
}

/// One live slot: the in-stack handle, the opening family, the UDP default peer
/// (connect semantics), the TCP pre-bound endpoint and listener flag, plus the
/// bookkeeping half of the option face (send/receive buffer capacities start at
/// the lwipopts contract defaults and the SO_SNDBUF/SO_RCVBUF options may change
/// them within bounds; SO_BROADCAST and the non-blocking flag get one bit each.
/// Physical buffer sizes do not follow the options — smoltcp buffers are
/// fixed-capacity after construction; the difference is recorded in
/// `25-smoltcp-shim.md` §3).
#[derive(Debug, Clone, Copy)]
struct Slot {
    handle: smoltcp::iface::SocketHandle,
    family: StackFamily,
    udp_peer: Option<crate::lwip_port::StackEndpoint>,
    tcp_local: Option<crate::lwip_port::StackEndpoint>,
    tcp_listener: bool,
    sndbuf: usize,
    rcvbuf: usize,
    broadcast: bool,
    nonblock: bool,
}

impl<D: Device + 'static> SmoltcpStack<D> {
    /// Stack construction (C `lwip_init`, `lwip.c:208-210`): the interface uses
    /// the layer-3 medium (`HardwareAddress::Ip` — the Ethernet-frame medium waits
    /// for the 16-stage NIC seam), and `seed` goes to the stack for its random face
    /// (TCP initial sequence numbers etc., the same position as C's `srand48`).
    /// The device is the shim's wall: [`TrunkDevice`] by default, [`LoopDevice`]
    /// in tests.
    pub fn new(seed: u64, now_millis: i64) -> Self
    where
        D: Default,
    {
        Self::with_device(seed, D::default(), now_millis)
    }

    /// Construct with an explicit device.
    pub fn with_device(seed: u64, device: D, now_millis: i64) -> Self {
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = seed;
        let mut device = device;
        let iface = Interface::new(config, &mut device, Instant::from_millis(now_millis));
        SmoltcpStack {
            iface,
            device,
            sockets: SocketSet::new(vec![]),
            slots: Vec::new(),
            ephemeral: 49152,
        }
    }

    /// Look up a slot by service-side handle: an out-of-range index, an empty slot
    /// and a family mismatch all count as "closed".
    fn slot(&self, socket: StackSocket) -> Option<Slot> {
        let idx = socket.index() as usize;
        let slot = self.slots.get(idx).copied().flatten()?;
        (slot.family == socket.family()).then_some(slot)
    }

    /// Mutable slot reference; out of range, empty slot or family mismatch gives
    /// `None`.
    fn slot_mut(&mut self, socket: StackSocket) -> Option<&mut Slot> {
        let idx = socket.index() as usize;
        match self.slots.get_mut(idx) {
            Some(Some(slot)) if slot.family == socket.family() => Some(slot),
            _ => None,
        }
    }

    /// Mutable in-stack reference to a UDP socket; a stale handle or a family
    /// mismatch gives `None`.
    fn udp_mut(
        &mut self,
        socket: StackSocket,
    ) -> Option<&mut smoltcp::socket::udp::Socket<'static>> {
        self.slot(socket)
            .filter(|slot| slot.family == StackFamily::Udp)
            .map(|slot| self.sockets.get_mut(slot.handle))
    }

    /// Mutable in-stack reference to a TCP socket; a stale handle or a family
    /// mismatch gives `None`.
    fn tcp_mut(
        &mut self,
        socket: StackSocket,
    ) -> Option<&mut smoltcp::socket::tcp::Socket<'static>> {
        self.slot(socket)
            .filter(|slot| slot.family == StackFamily::Tcp)
            .map(|slot| self.sockets.get_mut(slot.handle))
    }

    /// Replace the in-stack socket on a slot (TCP accepted connection: the
    /// listener's handle is transferred to the accepted socket, and the listener
    /// gets a fresh object).
    fn replace_handle(&mut self, socket: StackSocket, handle: smoltcp::iface::SocketHandle) {
        let idx = socket.index() as usize;
        if let Some(Some(slot)) = self.slots.get_mut(idx) {
            slot.handle = handle;
        }
    }

    /// Open a new TCP slot pointing at an existing handle (registering an accepted
    /// socket). The option bookkeeping half is inherited from the listener's slot
    /// (C `ipsock_clone` copies sndbuf/rcvbuf to the clone, `ipsock.c:157-158`;
    /// the flag bits come from the same source, `sockevent_clone`).
    fn push_slot_for(
        &mut self,
        family: StackFamily,
        handle: smoltcp::iface::SocketHandle,
        inherit: Option<Slot>,
    ) -> Option<StackSocket> {
        let mut slot = Slot {
            handle,
            family,
            udp_peer: None,
            tcp_local: None,
            tcp_listener: false,
            sndbuf: crate::lwip_port::TCP_SNDBUF_DEF,
            rcvbuf: crate::lwip_port::TCP_RCVBUF_DEF,
            broadcast: false,
            nonblock: false,
        };
        if let Some(from) = inherit {
            slot.sndbuf = from.sndbuf;
            slot.rcvbuf = from.rcvbuf;
            slot.broadcast = from.broadcast;
            slot.nonblock = from.nonblock;
        }
        let slot = Some(slot);
        let index = match self.slots.iter().position(|s| s.is_none()) {
            Some(free) => {
                self.slots[free] = slot;
                free
            }
            None => {
                self.slots.push(slot);
                self.slots.len() - 1
            }
        };
        Some(StackSocket::new(family, index as u16))
    }

    /// Take an ephemeral local port.
    fn next_ephemeral(&mut self) -> u16 {
        let port = self.ephemeral;
        self.ephemeral = self.ephemeral.wrapping_add(1).max(49152);
        port
    }
}

impl<D: Device + 'static> Stack for SmoltcpStack<D> {
    /// Advance the stack: receive/send packets, service due timers, then ask the
    /// stack when it next needs advancing. One smoltcp `poll` is the same action as
    /// C's `expire_timers`; the returned instant shares C's `sys_now` millisecond
    /// time base.
    fn poll(&mut self, now_millis: u64) -> PollWhen {
        let now = Instant::from_millis(now_millis as i64);
        self.iface.poll(now, &mut self.device, &mut self.sockets);
        match self.iface.poll_delay(now, &self.sockets) {
            // `Duration::total_millis` already returns `u64`, so add directly.
            Some(d) => PollWhen::At(now_millis + d.total_millis()),
            None => PollWhen::Never,
        }
    }

    /// The current readiness bits of one socket. TCP follows send/receive buffer
    /// headroom, UDP and RAW follow packet-ring headroom; a stale handle returns
    /// the default value per the wall's contract.
    fn readiness(&self, socket: StackSocket) -> Readiness {
        let Some(slot) = self.slot(socket) else {
            return Readiness::default();
        };
        match slot.family {
            StackFamily::Tcp => {
                let s = self.sockets.get::<tcp::Socket>(slot.handle);
                if slot.tcp_listener {
                    // Listener: any state other than Listen means a connection is
                    // pending (acceptable).
                    Readiness {
                        readable: s.state() != smoltcp::socket::tcp::State::Listen,
                        writable: false,
                    }
                } else {
                    Readiness { readable: s.can_recv(), writable: s.can_send() }
                }
            }
            StackFamily::Udp => {
                let s = self.sockets.get::<smoltcp::socket::udp::Socket>(slot.handle);
                Readiness { readable: s.can_recv(), writable: s.can_send() }
            }
            StackFamily::Raw => {
                let s = self.sockets.get::<raw::Socket>(slot.handle);
                Readiness { readable: s.can_recv(), writable: s.can_send() }
            }
            StackFamily::Icmp => Readiness::default(),
        }
    }

    /// Open a socket of the given family. TCP send/receive buffers are sized per
    /// the lwipopts contract (receive window [`TCP_WINDOW`], send buffer
    /// [`TCP_SEND_BUFFER`], `lwipopts.h:267`/`:282`); the UDP and RAW packet rings
    /// take a whole block of the same contract's magnitude; the ICMP face is wired
    /// with the doc-07 batch (on the C side ICMP goes through the RAW protocol
    /// slot, and this feature set does not compile smoltcp's dedicated ICMP socket
    /// in), so for now it answers with a generic error.
    fn open(&mut self, family: StackFamily, protocol: Option<u8>) -> Result<StackSocket, i32> {
        let handle = match family {
            StackFamily::Tcp => {
                // SocketBuffer is a managed slice (the ring buffer's constructor
                // argument), and the owned buffer is handed to the stack through
                // `ManagedSlice::from`.
                let rx = ManagedSlice::from(vec![0u8; TCP_WINDOW]);
                let tx = ManagedSlice::from(vec![0u8; TCP_SEND_BUFFER]);
                self.sockets.add(tcp::Socket::new(rx, tx))
            }
            StackFamily::Udp => {
                let rx = smoltcp::socket::udp::PacketBuffer::new(
                    vec![smoltcp::socket::udp::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_WINDOW],
                );
                let tx = smoltcp::socket::udp::PacketBuffer::new(
                    vec![smoltcp::socket::udp::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_SEND_BUFFER],
                );
                self.sockets.add(smoltcp::socket::udp::Socket::new(rx, tx))
            }
            StackFamily::Raw => {
                // The protocol is taken at creation (Minix's third argument, the
                // same position as `raw_new_ip_type` at rawsock.c:314); None =
                // wildcard (accept everything on the receive filter).
                let rx = raw::PacketBuffer::new(
                    vec![raw::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_WINDOW],
                );
                let tx = raw::PacketBuffer::new(
                    vec![raw::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_SEND_BUFFER],
                );
                let ip_protocol = protocol.map(smoltcp::wire::IpProtocol::from);
                self.sockets
                    .add(raw::Socket::new(Some(IpVersion::Ipv4), ip_protocol, rx, tx))
            }
            StackFamily::Icmp => return Err(util::ERR_GENERIC),
        };
        // Capacity defaults follow the family (tcpsock.c:87/:90,
        // udpsock.c:30/:33).
        let (sndbuf, rcvbuf) = match family {
            StackFamily::Tcp => (
                crate::lwip_port::TCP_SNDBUF_DEF,
                crate::lwip_port::TCP_RCVBUF_DEF,
            ),
            _ => (
                crate::lwip_port::UDP_SNDBUF_DEF,
                crate::lwip_port::UDP_RCVBUF_DEF,
            ),
        };
        let slot = Some(Slot {
            handle,
            family,
            udp_peer: None,
            tcp_local: None,
            tcp_listener: false,
            sndbuf,
            rcvbuf,
            broadcast: false,
            nonblock: false,
        });
        let index = match self.slots.iter().position(|s| s.is_none()) {
            Some(free) => {
                self.slots[free] = slot;
                free
            }
            None => {
                self.slots.push(slot);
                self.slots.len() - 1
            }
        };
        Ok(StackSocket::new(family, index as u16))
    }

    /// Close a socket: clear the slot and remove the in-stack socket. Its handle is
    /// stale afterwards (the slot is empty, and later lookups answer as already
    /// closed).
    fn close(&mut self, socket: StackSocket) -> Result<(), i32> {
        let idx = socket.index() as usize;
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        self.slots[idx] = None;
        self.sockets.remove(slot.handle);
        Ok(())
    }

    // -- UDP half (the stack face of doc 09) --

    fn bind_udp(
        &mut self,
        socket: StackSocket,
        local: Option<crate::lwip_port::StackEndpoint>,
    ) -> Result<(), i32> {
        let listen = listen_endpoint(local);
        self.udp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .bind(listen)
            .map_err(|_| util::ERR_ADDRESS_IN_USE)
    }

    fn connect_udp(
        &mut self,
        socket: StackSocket,
        remote: crate::lwip_port::StackEndpoint,
    ) -> Result<(), i32> {
        // smoltcp's UDP has no connection state: the default peer is recorded on
        // the service-side slot, and send without an address takes it (the
        // per-packet metadata model, `send_slice`'s meta parameter).
        if remote.addr.is_none() {
            return Err(util::ERR_INVALID);
        }
        let idx = socket.index() as usize;
        match self.slots.get_mut(idx).and_then(|s| s.as_mut()) {
            Some(slot) if slot.family == StackFamily::Udp => {
                slot.udp_peer = Some(remote);
                Ok(())
            }
            _ => Err(util::ERR_GENERIC),
        }
    }

    fn send_udp(
        &mut self,
        socket: StackSocket,
        data: &[u8],
        remote: Option<crate::lwip_port::StackEndpoint>,
    ) -> Result<usize, i32> {
        let endpoint = match remote {
            Some(remote) => {
                let Some(addr) = remote.addr else {
                    return Err(util::ERR_INVALID);
                };
                IpEndpoint { addr: to_ip_address(addr), port: remote.port }
            }
            None => {
                // No address means send semantics: take the default peer set by
                // connect.
                let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
                let peer = slot.udp_peer.ok_or(util::ERR_INVALID)?;
                let peer_addr = peer.addr.expect("connect 时已校验地址在场");
                IpEndpoint { addr: to_ip_address(peer_addr), port: peer.port }
            }
        };
        let bound = self
            .udp_mut(socket)
            .map(|s| s.endpoint())
            .ok_or(util::ERR_GENERIC)?;
        if bound.port == 0 {
            return Err(util::ERR_INVALID);
        }
        self.udp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .send_slice(data, endpoint)
            .map(|_| data.len())
            .map_err(|_| util::ERR_NO_BUFFERS)
    }

    fn recv_udp(
        &mut self,
        socket: StackSocket,
        data: &mut [u8],
    ) -> Result<(usize, crate::lwip_port::StackEndpoint), i32> {
        self.udp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .recv_slice(data)
            .map(|(n, meta)| {
                (
                    n,
                    crate::lwip_port::StackEndpoint {
                        addr: Some(from_ip_address(meta.endpoint.addr)),
                        port: meta.endpoint.port,
                    },
                )
            })
            .map_err(|_| util::ERR_WOULD_BLOCK)
    }

    fn local_endpoint_udp(
        &self,
        socket: StackSocket,
    ) -> Result<crate::lwip_port::StackEndpoint, i32> {
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        if slot.family != StackFamily::Udp {
            return Err(util::ERR_GENERIC);
        }
        let listen = self
            .sockets
            .get::<smoltcp::socket::udp::Socket>(slot.handle)
            .endpoint();
        Ok(crate::lwip_port::StackEndpoint {
            addr: listen.addr.map(from_ip_address),
            port: listen.port,
        })
    }

    // -- TCP half (the stack face of doc 08) --

    fn bind_tcp(
        &mut self,
        socket: StackSocket,
        local: Option<crate::lwip_port::StackEndpoint>,
    ) -> Result<(), i32> {
        let idx = socket.index() as usize;
        match self.slots.get_mut(idx).and_then(|s| s.as_mut()) {
            Some(slot) if slot.family == StackFamily::Tcp => {
                slot.tcp_local = local;
                Ok(())
            }
            _ => Err(util::ERR_GENERIC),
        }
    }

    fn listen_tcp(&mut self, socket: StackSocket, backlog: u32) -> Result<(), i32> {
        // The backlog is accepted for bookkeeping (smoltcp's single listener hangs
        // one connection at a time; concurrent listening is evaluated with the
        // interface batch).
        let _ = backlog;
        let local = self.slot(socket).and_then(|s| s.tcp_local);
        let listen = listen_endpoint(local);
        self.tcp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .listen(listen)
            .map_err(|e| match e {
                smoltcp::socket::tcp::ListenError::Unaddressable => util::ERR_INVALID,
                _ => util::ERR_ADDRESS_IN_USE,
            })?;
        let idx = socket.index() as usize;
        if let Some(Some(slot)) = self.slots.get_mut(idx) {
            slot.tcp_listener = true;
        }
        Ok(())
    }

    fn connect_tcp(
        &mut self,
        socket: StackSocket,
        remote: crate::lwip_port::StackEndpoint,
    ) -> Result<(), i32> {
        let Some(addr) = remote.addr else {
            return Err(util::ERR_INVALID);
        };
        let remote = IpEndpoint { addr: to_ip_address(addr), port: remote.port };
        let local_port = match self.slot(socket).and_then(|s| s.tcp_local) {
            Some(local) => local.port,
            None => self.next_ephemeral(),
        };
        // Field-level split borrow: read the slot first (copying the handle out),
        // then mutably borrow the iface context and the socket set at the same
        // time.
        let handle = self
            .slot(socket)
            .filter(|slot| slot.family == StackFamily::Tcp)
            .map(|slot| slot.handle)
            .ok_or(util::ERR_GENERIC)?;
        let listen = IpListenEndpoint { addr: None, port: local_port };
        let cx = self.iface.context();
        self.sockets
            .get_mut::<smoltcp::socket::tcp::Socket>(handle)
            .connect(cx, remote, listen)
            .map_err(|_| util::ERR_NO_BUFFERS)
    }

    fn accept_tcp(
        &mut self,
        socket: StackSocket,
    ) -> Result<(StackSocket, crate::lwip_port::StackEndpoint), i32> {
        use smoltcp::socket::tcp::State;
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        if slot.family != StackFamily::Tcp || !slot.tcp_listener {
            return Err(util::ERR_GENERIC);
        }
        let state = self.sockets.get::<smoltcp::socket::tcp::Socket>(slot.handle).state();
        if state != State::Established {
            // Only connections that completed the handshake count; incomplete ones
            // are left for the service layer to park and wait on.
            return Err(util::ERR_WOULD_BLOCK);
        }
        let peer = self
            .sockets
            .get::<smoltcp::socket::tcp::Socket>(slot.handle)
            .remote_endpoint()
            .ok_or(util::ERR_GENERIC)?;
        let listen_endpoint = self
            .sockets
            .get::<smoltcp::socket::tcp::Socket>(slot.handle)
            .listen_endpoint();
        // Install the fresh listener first (taking a new handle), then remove the
        // established old object and register it again — the reverse order would
        // let the set reuse the just-freed slot number, so the two handles would
        // collide.
        let fresh = {
            let rx = ManagedSlice::from(vec![0u8; TCP_WINDOW]);
            let tx = ManagedSlice::from(vec![0u8; TCP_SEND_BUFFER]);
            smoltcp::socket::tcp::Socket::new(rx, tx)
        };
        let fresh_handle = self.sockets.add(fresh);
        self.replace_handle(socket, fresh_handle);
        let fresh_stack = StackSocket::new(StackFamily::Tcp, socket.index());
        self.tcp_mut(fresh_stack)
            .expect("刚换上的监听者")
            .listen(listen_endpoint)
            .map_err(|_| util::ERR_ADDRESS_IN_USE)?;
        let established = self.sockets.remove(slot.handle);
        let accepted_handle = self.sockets.add(established);
        // The accepted slot inherits the listener's option bookkeeping half (the
        // same position as C's ipsock_clone).
        let accepted = self
            .push_slot_for(StackFamily::Tcp, accepted_handle, Some(slot))
            .ok_or(util::ERR_GENERIC)?;
        Ok((
            accepted,
            crate::lwip_port::StackEndpoint {
                addr: Some(from_ip_address(peer.addr)),
                port: peer.port,
            },
        ))
    }

    fn send_tcp(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
        self.tcp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .send_slice(data)
            .map_err(|_| util::ERR_NOT_CONNECTED)
    }

    fn recv_tcp(
        &mut self,
        socket: StackSocket,
        data: &mut [u8],
    ) -> Result<(usize, bool), i32> {
        use smoltcp::socket::tcp::RecvError;
        match self.tcp_mut(socket).ok_or(util::ERR_GENERIC)?.recv_slice(data) {
            // Three-way semantics: data; EOF (peer closed and everything
            // drained); connection not open.
            Ok(0) => Err(util::ERR_WOULD_BLOCK),
            Ok(n) => Ok((n, false)),
            Err(RecvError::Finished) => Ok((0, true)),
            Err(_) => Err(util::ERR_NOT_CONNECTED),
        }
    }

    fn shutdown_tcp(&mut self, socket: StackSocket, how: i32) -> Result<(), i32> {
        // Write direction and both directions: a graceful FIN (sent with the next
        // pass; the socket stays until it finishes). Closing the read direction
        // alone has no counterpart verb in smoltcp — answer honestly per
        // capability.
        const SHUT_RD: i32 = 0;
        const SHUT_WR: i32 = 1;
        const SHUT_RDWR: i32 = 2;
        match how {
            SHUT_WR | SHUT_RDWR => {
                self.tcp_mut(socket)
                    .ok_or(util::ERR_GENERIC)?
                    .close();
                Ok(())
            }
            SHUT_RD => Err(util::ERR_INVALID),
            _ => Err(util::ERR_INVALID),
        }
    }

    // -- Option half (the stack face of SDEV_SETSOCKOPT/SDEV_GETSOCKOPT) --

    fn sockopt_tcp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32> {
        match name {
            // Keepalive: enabling follows C's SOF_KEEPALIVE semantics
            // (setsockmask, tcpsock.c:2096-2100) — smoltcp's interval-style API
            // takes the C pcb's default idle value (TCP_KEEPIDLE_DEFAULT,
            // tcp_priv.h:139).
            crate::lwip_port::SO_KEEPALIVE => {
                let s = self.tcp_mut(socket).ok_or(util::ERR_GENERIC)?;
                match value {
                    Some(v) => {
                        s.set_keep_alive(
                            (v != 0).then(|| {
                                smoltcp::time::Duration::from_millis(
                                    crate::lwip_port::TCP_KEEPALIVE_IDLE_MS,
                                )
                            }),
                        );
                        Ok(None)
                    }
                    None => Ok(Some(i32::from(s.keep_alive().is_some()))),
                }
            }
            // Send/receive capacity: record within bounds (the bounds check plus
            // the bookkeeping half of C's ipsock_setsockopt:482-510); the physical
            // buffer keeps the lwipopts contract size, a recorded difference.
            crate::lwip_port::SO_SNDBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::TCP_SNDBUF_MIN
                        || (v as usize) > crate::lwip_port::TCP_SNDBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.sndbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let sndbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.sndbuf;
                    Ok(Some(sndbuf as i32))
                }
            },
            crate::lwip_port::SO_RCVBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::TCP_RCVBUF_MIN
                        || (v as usize) > crate::lwip_port::TCP_RCVBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.rcvbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let rcvbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.rcvbuf;
                    Ok(Some(rcvbuf as i32))
                }
            },
            _ => Err(util::STACK_BAD_ARGUMENT),
        }
    }

    fn sockopt_udp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32> {
        match name {
            // Broadcast permission: C sets SOF_BROADCAST in setsockmask
            // (udpsock.c:553-557); smoltcp's broadcast send/receive is in-stack
            // behaviour (no counterpart API), so the flag lives on the service-side
            // slot — the query semantics are preserved and the send-face difference
            // is recorded.
            crate::lwip_port::SO_BROADCAST => match value {
                Some(v) => {
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.broadcast = v != 0;
                    Ok(None)
                }
                None => {
                    let broadcast = self.slot(socket).ok_or(util::ERR_GENERIC)?.broadcast;
                    Ok(Some(i32::from(broadcast)))
                }
            },
            // The capacity face has the same shape as TCP, with the UDP three
            // bounds.
            crate::lwip_port::SO_SNDBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::UDP_SNDBUF_MIN
                        || (v as usize) > crate::lwip_port::UDP_SNDBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.sndbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let sndbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.sndbuf;
                    Ok(Some(sndbuf as i32))
                }
            },
            crate::lwip_port::SO_RCVBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::UDP_RCVBUF_MIN
                        || (v as usize) > crate::lwip_port::UDP_RCVBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.rcvbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let rcvbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.rcvbuf;
                    Ok(Some(rcvbuf as i32))
                }
            },
            _ => Err(util::STACK_BAD_ARGUMENT),
        }
    }

    // -- ioctl half (the stack face of SDEV_IOCTL) --

    fn pending_recv_udp(&mut self, socket: StackSocket) -> Option<usize> {
        // peek does not consume: the payload length of the first packet (the
        // first-packet semantics of C pktsock_test_recv, pktsock.c:886-899).
        self.udp_mut(socket)?.peek().ok().map(|(payload, _)| payload.len())
    }

    fn pending_recv_tcp(&self, socket: StackSocket) -> usize {
        match self.slot(socket) {
            Some(slot) if slot.family == StackFamily::Tcp => {
                self.sockets.get::<tcp::Socket>(slot.handle).recv_queue()
            }
            _ => 0,
        }
    }

    fn pending_recv_raw(&self, socket: StackSocket) -> Option<usize> {
        let slot = self.slot(socket)?;
        if slot.family != StackFamily::Raw {
            return None;
        }
        let queue = self.sockets.get::<raw::Socket>(slot.handle).recv_queue();
        (queue > 0).then_some(queue)
    }

    fn set_nonblock(&mut self, socket: StackSocket, nonblock: bool) -> Result<(), i32> {
        self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.nonblock = nonblock;
        Ok(())
    }

    fn is_nonblock(&self, socket: StackSocket) -> bool {
        self.slot(socket).map(|slot| slot.nonblock).unwrap_or(false)
    }

    // -- RAW half (the stack face of doc 10; full-packet semantics = HDRINCL) --

    fn send_raw(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
        let slot = self
            .slot(socket)
            .filter(|slot| slot.family == StackFamily::Raw)
            .ok_or(util::ERR_GENERIC)?;
        self.sockets
            .get_mut::<raw::Socket>(slot.handle)
            .send_slice(data)
            .map(|_| data.len())
            .map_err(|_| util::ERR_NO_BUFFERS)
    }

    fn recv_raw(&mut self, socket: StackSocket, data: &mut [u8]) -> Result<usize, i32> {
        let slot = self
            .slot(socket)
            .filter(|slot| slot.family == StackFamily::Raw)
            .ok_or(util::ERR_GENERIC)?;
        self.sockets
            .get_mut::<raw::Socket>(slot.handle)
            .recv_slice(data)
            .map_err(|_| util::ERR_WOULD_BLOCK)
    }

    fn remote_endpoint_tcp(
        &self,
        socket: StackSocket,
    ) -> Result<crate::lwip_port::StackEndpoint, i32> {
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        if slot.family != StackFamily::Tcp {
            return Err(util::ERR_GENERIC);
        }
        let remote = self
            .sockets
            .get::<smoltcp::socket::tcp::Socket>(slot.handle)
            .remote_endpoint()
            .ok_or(util::ERR_GENERIC)?;
        Ok(crate::lwip_port::StackEndpoint {
            addr: Some(from_ip_address(remote.addr)),
            port: remote.port,
        })
    }

    /// Deliver an inbound frame: the trunk device has no real NIC to hand it to,
    /// so answer "network down". The real data path lands with the 16-stage NIC
    /// seam (the frame model is refined along with it).
    fn receive_frame(&mut self, _frame: &[u8]) -> Result<(), i32> {
        Err(util::ERR_NETWORK_DOWN)
    }

    /// Take one outbound frame from the stack: the trunk device's transmit half is
    /// always empty, always `None` (no frame pending — without a NIC the stack
    /// cannot send anything out).
    fn transmit_frame(&mut self, _frame: &mut [u8]) -> Option<usize> {
        None
    }
}

/// Translation from a wall endpoint to a stack listen endpoint (`None` address =
/// wildcard).
fn listen_endpoint(
    endpoint: Option<crate::lwip_port::StackEndpoint>,
) -> IpListenEndpoint {
    let (addr, port) = match endpoint {
        Some(e) => (e.addr.map(to_ip_address), e.port),
        None => (None, 0),
    };
    IpListenEndpoint { addr, port }
}

/// Wall address to stack address.
fn to_ip_address(addr: crate::lwip_port::StackIpAddr) -> IpAddress {
    match addr {
        crate::lwip_port::StackIpAddr::V4(octets) => {
            IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(
                octets[0], octets[1], octets[2], octets[3],
            ))
        }
        crate::lwip_port::StackIpAddr::V6(bytes) => {
            IpAddress::Ipv6(core::net::Ipv6Addr::from(bytes))
        }
    }
}

/// Stack address to wall address.
fn from_ip_address(addr: IpAddress) -> crate::lwip_port::StackIpAddr {
    match addr {
        IpAddress::Ipv4(v4) => crate::lwip_port::StackIpAddr::V4(v4.octets()),
        IpAddress::Ipv6(v6) => crate::lwip_port::StackIpAddr::V6(v6.octets()),
    }
}

impl<D: Device + 'static> SmoltcpStack<D> {
    /// Attach a version 4 address to the interface (the stack half of ifconf; the
    /// interface batch will wrap it in a layer outside the wall, and for now it
    /// serves tests and the default configuration step).
    pub fn add_address_v4(&mut self, octets: [u8; 4], prefix: u8, now_millis: i64) {
        let cidr = IpCidr::new(
            IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(
                octets[0], octets[1], octets[2], octets[3],
            )),
            prefix,
        );
        let _ = now_millis;
        self.iface.update_ip_addrs(|addrs| {
            if addrs.push(cidr).is_err() {
                // Address table full: the first version carries one address, so
                // replace the first entry.
                if let Some(first) = addrs.iter_mut().next() {
                    *first = cidr;
                }
            }
        });
    }
}

/// The seam to the 16-stage NIC data path (the shim's wall): a trunk device whose
/// receive and transmit are always empty.
///
/// The interface is fully ready on it (construction, addresses, routing table,
/// socket set, timer advance), only no packet comes in or goes out — every pass
/// spins idle. When the real driver lands, the receive and transmit halves of this
/// type are replaced; if the decision falls back to FFI (behind the same wall),
/// that is the same single point that changes.
#[derive(Debug, Default, Clone, Copy)]
pub struct TrunkDevice;

/// [`TrunkDevice`]'s receive token: `receive` is always `None`, so the token is
/// never handed out.
#[derive(Debug)]
pub struct TrunkRxToken;

/// [`TrunkDevice`]'s transmit token: `transmit` is always `None`, so the token is
/// never handed out.
#[derive(Debug)]
pub struct TrunkTxToken;

impl RxToken for TrunkRxToken {
    fn consume<R, F>(self, _f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        unreachable!("TrunkDevice 的 receive 恒 None，接收令牌不会发出去")
    }
}

impl TxToken for TrunkTxToken {
    fn consume<R, F>(self, _len: usize, _f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        unreachable!("TrunkDevice 的 transmit 恒 None，发送令牌不会发出去")
    }
}

impl Device for TrunkDevice {
    type RxToken<'a> = TrunkRxToken;
    type TxToken<'a> = TrunkTxToken;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        None
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        None
    }

    fn capabilities(&self) -> DeviceCapabilities {
        // A non-exhaustive struct: outside the crate, take the default and then
        // set fields (the same form as smoltcp's own `phy/mod.rs:54`).
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = 1500;
        caps.max_burst_size = None;
        caps.checksum = ChecksumCapabilities::default();
        caps
    }
}

/// Loopback device (tests only): a sent frame loops back to the receive half, so
/// together with an interface address the whole data plane (bind, send, advance,
/// receive) runs in a host test without depending on any NIC.
#[derive(Debug, Default)]
pub struct LoopDevice {
    looped: alloc::vec::Vec<alloc::vec::Vec<u8>>,
}

impl LoopDevice {
    /// An empty loopback device.
    pub fn new() -> Self {
        LoopDevice { looped: alloc::vec::Vec::new() }
    }
}

impl Device for LoopDevice {
    type RxToken<'a> = LoopRxToken;
    type TxToken<'a> = LoopTxToken<'a>;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.looped.is_empty() {
            return None;
        }
        let frame = self.looped.remove(0);
        Some((LoopRxToken { frame }, LoopTxToken { queue: &mut self.looped }))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(LoopTxToken { queue: &mut self.looped })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = 1500;
        caps
    }
}

/// [`LoopDevice`]'s receive token: hands the looped frame to the stack.
#[derive(Debug)]
pub struct LoopRxToken {
    frame: alloc::vec::Vec<u8>,
}

impl RxToken for LoopRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.frame)
    }
}

/// [`LoopDevice`]'s transmit token: the frame enters the loopback queue.
#[derive(Debug)]
pub struct LoopTxToken<'a> {
    queue: &'a mut alloc::vec::Vec<alloc::vec::Vec<u8>>,
}

impl<'a> TxToken for LoopTxToken<'a> {
    fn consume<R, F>(self, _len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut frame = alloc::vec![0u8; _len];
        let result = f(&mut frame);
        self.queue.push(frame);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_close_roundtrip_reuses_slot() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let first = stack.open(StackFamily::Tcp, None).expect("TCP 建户");
        assert_eq!(first.index(), 0, "首户占 0 号槽");
        stack.close(first).expect("关闭");
        let second = stack.open(StackFamily::Tcp, None).expect("再开");
        assert_eq!(second.index(), first.index(), "槽位回收复用");
    }

    #[test]
    fn test_open_families_get_distinct_slots() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let t = stack.open(StackFamily::Tcp, None).unwrap();
        let u = stack.open(StackFamily::Udp, None).unwrap();
        let r = stack.open(StackFamily::Raw, None).unwrap();
        assert_ne!(t.index(), u.index());
        assert_ne!(u.index(), r.index());
        // The ICMP face is unwired: answer honestly with a generic error, no fake
        // handle.
        assert_eq!(
            stack.open(StackFamily::Icmp, None).unwrap_err(),
            util::ERR_GENERIC,
            "ICMP 面随第 07 篇批次接线"
        );
    }

    #[test]
    fn test_readiness_after_close_is_default() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let ghost = StackSocket::new(StackFamily::Tcp, 9);
        assert_eq!(
            stack.readiness(ghost),
            Readiness::default(),
            "失效句柄回缺省就绪位（墙的契约）"
        );
        // Use UDP to check the live bits: there is no connection state, and
        // headroom in the packet ring already allows sending (TCP's `can_send`
        // depends on the MSS negotiation after connection setup, so a fresh socket
        // has both bits false, indistinguishable from the defaults, and cannot pin
        // this assertion).
        let socket = stack.open(StackFamily::Udp, None).unwrap();
        assert!(!stack.readiness(socket).readable);
        assert!(stack.readiness(socket).writable);
        stack.close(socket).unwrap();
        assert_eq!(stack.readiness(socket), Readiness::default());
    }

    #[test]
    fn test_close_guards_family_and_double_close() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let tcp = stack.open(StackFamily::Tcp, None).unwrap();
        // A handle with a mismatched family is answered as "already closed" (the
        // in-stack socket is not touched).
        let wrong_family = StackSocket::new(StackFamily::Udp, tcp.index());
        assert!(stack.close(wrong_family).is_err());
        stack.close(tcp).expect("首关成功");
        assert!(stack.close(tcp).is_err(), "双关按栈错误回答");
    }

    #[test]
    fn test_udp_datagram_loopback_roundtrip() {
        use crate::lwip_port::StackEndpoint;
        // The full data plane on the loopback device: bind → send to itself →
        // advance (the frame loops back and the stack puts the packet into the
        // socket buffer) → receive the verbatim data with the sender endpoint.
        let mut stack: SmoltcpStack<LoopDevice> =
            SmoltcpStack::with_device(0x5EED, LoopDevice::new(), 0);
        stack.add_address_v4([127, 0, 0, 1], 8, 0);
        let socket = stack.open(StackFamily::Udp, None).expect("UDP 建户");
        stack
            .bind_udp(
                socket,
                Some(StackEndpoint {
                    addr: Some(crate::lwip_port::StackIpAddr::V4([127, 0, 0, 1])),
                    port: 7777,
                }),
            )
            .expect("绑定 7777");
        let sent = stack
            .send_udp(
                socket,
                &[9, 9, 9, 9],
                Some(StackEndpoint {
                    addr: Some(crate::lwip_port::StackIpAddr::V4([127, 0, 0, 1])),
                    port: 7777,
                }),
            )
            .expect("发送入栈");
        assert_eq!(sent, 4);
        // One pass is not enough for the frame to loop back and be consumed by the
        // stack (sending and receiving each need a pass), so advance until time
        // moves on — receive again once there is data.
        let mut received = None;
        for tick in 1..=8u64 {
            stack.poll(tick * 100);
            let mut buf = [0u8; 64];
            match stack.recv_udp(socket, &mut buf) {
                Ok((n, peer)) => {
                    received = Some((n, peer, buf));
                    break;
                }
                Err(_) => continue,
            }
        }
        let (n, peer, buf) = received.expect("回环报文在数趟推进内到达");
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], &[9, 9, 9, 9]);
        assert_eq!(
            peer,
            StackEndpoint {
                addr: Some(crate::lwip_port::StackIpAddr::V4([127, 0, 0, 1])),
                port: 7777,
            },
            "发送方端点随报文带回"
        );
    }

    #[test]
    fn test_tcp_control_plane_loopback_handshake() {
        use crate::lwip_port::StackEndpoint;
        let v4 = |octets: [u8; 4]| Some(crate::lwip_port::StackIpAddr::V4(octets));
        let mut stack: SmoltcpStack<LoopDevice> =
            SmoltcpStack::with_device(0x5EED, LoopDevice::new(), 0);
        stack.add_address_v4([127, 0, 0, 1], 8, 0);

        // Server: bind 7777 → listen.
        let server = stack.open(StackFamily::Tcp, None).expect("服务端建户");
        stack
            .bind_tcp(
                server,
                Some(StackEndpoint { addr: v4([127, 0, 0, 1]), port: 7777 }),
            )
            .expect("绑定");
        stack.listen_tcp(server, 1).expect("监听");

        // Client: connect to 127.0.0.1:7777 (the local port is ephemeral).
        let client = stack.open(StackFamily::Tcp, None).expect("客户端建户");
        stack
            .connect_tcp(
                client,
                StackEndpoint { addr: v4([127, 0, 0, 1]), port: 7777 },
            )
            .expect("connect 开张");

        // Poll forward: the handshake completes within a few passes (SYN →
        // SYN-ACK → ACK, each looping back through the loopback device once). The
        // readability bit lights as soon as the SYN arrives (the state leaves
        // Listen), and accept still returns a would-block error then — the real
        // service shape is to retry after being woken, until the connection is
        // established.
        let mut outcome = None;
        for tick in 1..=16u64 {
            stack.poll(tick * 100);
            if !stack.readiness(server).readable {
                continue;
            }
            if let Ok(pair) = stack.accept_tcp(server) {
                outcome = Some(pair);
                break;
            }
        }
        let (accepted, peer) = outcome.expect("数趟推进内完成受纳");
        assert_eq!(
            peer,
            StackEndpoint { addr: v4([127, 0, 0, 1]), port: 49152 },
            "对端端点=客户端的临时端口"
        );
        assert_ne!(accepted.index(), server.index(), "受纳套接字是独立槽位");
        // The fresh listener stays on duty: opening another client connects
        // again.
        assert!(
            stack.readiness(server).writable == false && !stack.readiness(server).readable,
            "换上的新监听者回到等待态"
        );
        let client2 = stack.open(StackFamily::Tcp, None).expect("第二客户端建户");
        stack
            .connect_tcp(
                client2,
                StackEndpoint { addr: v4([127, 0, 0, 1]), port: 7777 },
            )
            .expect("第二连接开张");
        let mut accepted2 = None;
        for tick in 20..=40u64 {
            stack.poll(tick * 100);
            if let Ok(pair) = stack.accept_tcp(server) {
                accepted2 = Some(pair);
                break;
            }
        }
        let (accepted2, _) = accepted2.expect("新监听者继续受理");
        assert_ne!(accepted2.index(), accepted.index());
        // Both ends of an established connection are writable (can_send of
        // established).
        assert!(stack.readiness(accepted).writable, "受纳套接字可写");
        assert!(stack.readiness(client).writable, "客户端套接字可写");
    }

    #[test]
    fn test_fresh_stack_polls_never() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(0x1234_5678, 0);
        // An empty stack has no timers and nothing pending: after advancing it
        // sleeps until the next message arrives.
        assert_eq!(stack.poll(0), PollWhen::Never);
        assert_eq!(stack.poll(10_000), PollWhen::Never);
    }

    #[test]
    fn test_tcp_sockopt_keepalive_and_buffer_bounds() {
        use crate::lwip_port::{SO_KEEPALIVE, SO_RCVBUF, SO_SNDBUF};
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let socket = stack.open(StackFamily::Tcp, None).unwrap();

        // Keepalive: off by default; once enabled the query says on (the interval
        // takes C's default idle value).
        assert_eq!(
            stack.sockopt_tcp(socket, SO_KEEPALIVE, None).unwrap(),
            Some(0),
            "新套接字保活缺省关"
        );
        assert_eq!(stack.sockopt_tcp(socket, SO_KEEPALIVE, Some(1)), Ok(None));
        assert_eq!(
            stack.sockopt_tcp(socket, SO_KEEPALIVE, None).unwrap(),
            Some(1),
            "使能后查询为开"
        );
        assert_eq!(stack.sockopt_tcp(socket, SO_KEEPALIVE, Some(0)), Ok(None));
        assert_eq!(
            stack.sockopt_tcp(socket, SO_KEEPALIVE, None).unwrap(),
            Some(0),
            "清位后查询为关"
        );

        // Capacity: default tier (tcpsock.c:87/:90), recorded within bounds,
        // EINVAL outside them.
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, None).unwrap(),
            Some(crate::lwip_port::TCP_SNDBUF_DEF as i32)
        );
        assert_eq!(stack.sockopt_tcp(socket, SO_SNDBUF, Some(65536)), Ok(None));
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, None).unwrap(),
            Some(65536)
        );
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, Some(0)).unwrap_err(),
            util::ERR_INVALID,
            "零容量拒（C val <= 0 同判）"
        );
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, Some(131073)).unwrap_err(),
            util::ERR_INVALID,
            "超上界拒（tcpsock.c:88）"
        );
        assert_eq!(
            stack.sockopt_tcp(socket, SO_RCVBUF, Some(16383)).unwrap_err(),
            util::ERR_INVALID,
            "低于接收窗拒（tcpsock.c:89：接收缓冲必须不小于窗口）"
        );
        // An unsupported name: the wall answers with a stack argument error (folded
        // to ENOPROTOOPT on the way out).
        assert_eq!(
            stack.sockopt_tcp(socket, 0x2000, Some(1)).unwrap_err(),
            util::STACK_BAD_ARGUMENT
        );
    }

    #[test]
    fn test_udp_sockopt_broadcast_and_capacity() {
        use crate::lwip_port::{SO_BROADCAST, SO_RCVBUF};
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let socket = stack.open(StackFamily::Udp, None).unwrap();

        // Broadcast permission: off by default; after setting the bit the query
        // says on.
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, None).unwrap(), Some(0));
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, Some(1)), Ok(None));
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, None).unwrap(), Some(1));

        // Receive capacity: default 32768 (udpsock.c:33), lower bound 512
        // (MEMPOOL_BUFSIZE).
        assert_eq!(
            stack.sockopt_udp(socket, SO_RCVBUF, None).unwrap(),
            Some(crate::lwip_port::UDP_RCVBUF_DEF as i32)
        );
        assert_eq!(
            stack.sockopt_udp(socket, SO_RCVBUF, Some(511)).unwrap_err(),
            util::ERR_INVALID
        );
        assert_eq!(stack.sockopt_udp(socket, SO_RCVBUF, Some(512)), Ok(None));
        // Options do not cross slots: the broadcast bit is still set.
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, None).unwrap(), Some(1));
    }
}
