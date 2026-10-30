//! Third-party stack port surface: build subset, key options, hooks, patches.
//!
//! C correspondence: `minix3/minix/lib/liblwip/` (build subset under
//! `dist/src`, glue under `lib/`, four patches under `patches/`). No packet
//! code lives here. This module owns the portion that can be stated as
//! numbers and names: which build subset is compiled, which option values
//! the service relies on, which four hooks the service provides, and which
//! four patches are applied.
//!
//! [ARCH N-1]: ruled 2026-09-17 (decision record: 24-liblwip-port.md §1.5).
//! The replacement is a Rust stack of the smoltcp family behind a
//! service-owned wall, with a semantic shim carrying the MINIX3 socket
//! behavior; the FFI path stays reachable because every service module
//! depends only on the [`Stack`] and [`StackHooks`] traits below, never on
//! a concrete stack. The glue constants below remain the behavior contract
//! the shim must honor: single-threaded, no pool allocator, receive window
//! 16384, send buffer 11 × 1460, multicast ceilings.

/// Compiled build subset (68 `.c` files / 58232 lines across the core,
/// Internet Protocol version 4, version 6, and network interface groups).
pub const BUILD_C_FILES: usize = 68;

/// Single-threaded stack, no operating-system layer (`NO_SYS`, 1,
/// `lwipopts.h:14`).
pub const OPTION_NO_SYS: u8 = 1;

/// Custom pool replaces the pool allocator (`PBUF_POOL_SIZE`, 0,
/// `lwipopts.h:80`).
pub const OPTION_POOL_SIZE: usize = 0;

/// Largest segment size (`TCP_MSS`, 1460, `lwipopts.h:259`).
pub const TCP_MAX_SEGMENT: usize = 1460;

/// Receive window (`TCP_WND`, 16384, `lwipopts.h:267`).
pub const TCP_WINDOW: usize = 16384;

/// Send buffer (`TCP_SND_BUF`, 11 times the segment size, `lwipopts.h:282`).
pub const TCP_SEND_BUFFER: usize = 11 * TCP_MAX_SEGMENT;

// Capacity contract for the SO_SNDBUF/SO_RCVBUF options (the three-tier constants
// at the top of each C socket module; the shim accepts and records the same tiers
// while the physical buffers stay sized by the two lwipopts constants above —
// see `25-smoltcp-shim.md` §3 for the registered difference).
/// TCP send-buffer floor (`TCP_SNDBUF_MIN`, 1, `tcpsock.c:86`).
pub const TCP_SNDBUF_MIN: usize = 1;
/// TCP send-buffer default (`TCP_SNDBUF_DEF`, 32768, `tcpsock.c:87`).
pub const TCP_SNDBUF_DEF: usize = 32768;
/// TCP send-buffer ceiling (`TCP_SNDBUF_MAX`, 131072, `tcpsock.c:88`).
pub const TCP_SNDBUF_MAX: usize = 131072;
/// TCP receive-buffer floor (`TCP_RCVBUF_MIN`, i.e. `TCP_WND`, `tcpsock.c:89`).
pub const TCP_RCVBUF_MIN: usize = TCP_WINDOW;
/// TCP receive-buffer default (`TCP_RCVBUF_DEF`, `MAX(TCP_WND, 32768)`; the receive
/// window of 16384 is below 32768, so the value is 32768, `tcpsock.c:90`).
pub const TCP_RCVBUF_DEF: usize = 32768;
/// TCP receive-buffer ceiling (`TCP_RCVBUF_MAX`, `MAX(TCP_WND, 131072)`; likewise
/// the value is 131072, `tcpsock.c:91`).
pub const TCP_RCVBUF_MAX: usize = 131072;
/// UDP send-buffer floor (`UDP_SNDBUF_MIN`, 1, `udpsock.c:29`).
pub const UDP_SNDBUF_MIN: usize = 1;
/// UDP send-buffer default (`UDP_SNDBUF_DEF`, 8192, `udpsock.c:30`).
pub const UDP_SNDBUF_DEF: usize = 8192;
/// UDP send-buffer ceiling (`UDP_SNDBUF_MAX`, `UDP_MAX_PAYLOAD`, `udpsock.c:28`/`:31`).
pub const UDP_SNDBUF_MAX: usize = u16::MAX as usize;
/// UDP receive-buffer floor (`UDP_RCVBUF_MIN`, i.e. `MEMPOOL_BUFSIZE`,
/// `lwipopts.h:49` + `udpsock.c:32`).
pub const UDP_RCVBUF_MIN: usize = 512;
/// UDP receive-buffer default (`UDP_RCVBUF_DEF`, 32768, `udpsock.c:33`).
pub const UDP_RCVBUF_DEF: usize = 32768;
/// UDP receive-buffer ceiling (`UDP_RCVBUF_MAX`, 65536, `udpsock.c:34`).
pub const UDP_RCVBUF_MAX: usize = 65536;

/// Idle interval while TCP keepalive is enabled (the pcb's default idle value
/// `TCP_KEEPIDLE_DEFAULT` once C enables `SOF_KEEPALIVE`, 7200000 milliseconds,
/// `lwip/priv/tcp_priv.h:138-139`).
pub const TCP_KEEPALIVE_IDLE_MS: u64 = 7_200_000;

// Option names (the wire values of `sys/sys/socket.h`; the service path dispatches
// by name and the wall applies the effect by name — the name list is consumed once
// on each side).
/// `SOL_SOCKET` (0xffff, `sys/socket.h:171`).
pub const SOL_SOCKET: i32 = 0xffff;
/// `SO_KEEPALIVE` (0x0008, `sys/socket.h:124`).
pub const SO_KEEPALIVE: i32 = 0x0008;
/// `SO_BROADCAST` (0x0020, `sys/socket.h:126`).
pub const SO_BROADCAST: i32 = 0x0020;
/// `SO_SNDBUF` (0x1001, `sys/socket.h:139`).
pub const SO_SNDBUF: i32 = 0x1001;
/// `SO_RCVBUF` (0x1002, `sys/socket.h:140`).
pub const SO_RCVBUF: i32 = 0x1002;

/// Hooks the service provides to the stack (`lwiphooks.h`: sequence-number
/// generation, two route overrides, two gateway lookups).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackHook {
    /// Initial sequence number generation.
    TcpSequenceNumber,
    /// Version 4 route override.
    RouteVersion4,
    /// Version 6 route override.
    RouteVersion6,
    /// Gateway lookup (address resolution and neighbor discovery share one
    /// policy entry here; the service fans out to both).
    Gateway,
}

/// All four hooks.
pub const ALL_HOOKS: [StackHook; 4] = [
    StackHook::TcpSequenceNumber,
    StackHook::RouteVersion4,
    StackHook::RouteVersion6,
    StackHook::Gateway,
];

/// Applied patches, in directory order.
pub const PATCHES: [&str; 4] = [
    "0001-MINIX-3-only-mark-various-functions-as-weak",
    "0002-MINIX-3-only-control-IP-forwarding-at-run-time",
    "0003-MINIX-3-only-ignore-IPv6-Router-Advertisements",
    "0004-MINIX-3-only-avoid-large-contiguous-allocations",
];

// ---------------------------------------------------------------------------
// Stack wall: the third-party stack boundary the service depends on exclusively
// ([ARCH N-1], decision record in chapter 24 §1.5).
//
// The ownership direction of the boundary is the whole point of this wall:
// - Routing and gateway policy belong to the service (in C, calling
//   lwip_hook_ip4_route panics immediately, lwiphooks.h:15 — meaning the stack has
//   no routing authority at all and the table in route.c is the single truth). The
//   wall therefore asks the stack only for resolution services such as "next
//   hop/gateway" and never hands over route selection.
// - The time source belongs to the service (C sys_now = getticks × 1000 / sys_hz,
//   milliseconds); every instant on the wall is a millisecond count.
// - The device belongs to the service: frames arrive from NDEV and are handed to
//   the stack, and the stack hands frames back to the service when it wants to
//   send; the buffer model, once refined by the buffer round, replaces the
//   borrowed-slice signature here.
// ---------------------------------------------------------------------------

/// An IP address in wire shape. Its final home is settled by the wire-type decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackIpAddr {
    /// Version 4 address, network-order bytes.
    V4([u8; 4]),
    /// Version 6 address, network-order bytes.
    V6([u8; 16]),
}

/// Socket families the stack supports, one-to-one with the service-side tcpsock/udpsock/rawsock/icmp faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackFamily {
    /// Transmission control protocol (chapter 08).
    Tcp,
    /// User datagram protocol (chapter 09).
    Udp,
    /// Raw IP packets (chapter 10).
    Raw,
    /// Internet control message protocol (the icmp face of chapter 07).
    Icmp,
}

/// Handle for one socket inside the stack: family plus in-stack index. The service
/// never holds stack-internal memory; the object a handle points at disappears on
/// close, and later calls that use it are answered with an error by the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackSocket {
    family: StackFamily,
    index: u16,
}

impl StackSocket {
    /// Build a handle from a family and an in-stack index.
    pub const fn new(family: StackFamily, index: u16) -> Self {
        StackSocket { family, index }
    }

    /// The handle's family.
    pub const fn family(&self) -> StackFamily {
        self.family
    }

    /// The handle's index within the stack.
    pub const fn index(&self) -> u16 {
        self.index
    }
}

/// The input quad for a TCP initial sequence number (the Rust restatement of
/// `lwip_hook_tcp_isn`, lwiphooks.h:8-11). There is exactly one contract:
/// unpredictability (RFC 6528).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionQuad {
    /// Local address.
    pub local_address: StackIpAddr,
    /// Local port.
    pub local_port: u16,
    /// Peer address.
    pub remote_address: StackIpAddr,
    /// Peer port.
    pub remote_port: u16,
}

/// Policy hooks the service provides to the stack.
///
/// The C counterparts are lwiphooks.h's sequence-number hook and two gateway hooks
/// (lwiphooks.h:24-27 and :40-43, where the service answers resolution from its own
/// table). The two route-override hooks are **not inside the wall**: their C contract
/// is "panic when called" (lwiphooks.h:15 and :31), whose real meaning is that route
/// decisions belong to the service and are not a policy point the stack may call back
/// into, so the wall has no opening for them.
pub trait StackHooks {
    /// Initial sequence number for a new TCP connection. smoltcp-family stacks
    /// generate sequence numbers internally and do not consume this hook; it stays on
    /// the wall for service implementations that hold a random source (recorded in the
    /// deviation table of chapter 24 §1.5).
    fn initial_sequence(&mut self, quad: ConnectionQuad) -> u32;

    /// Version 4 gateway/next-hop resolution (the last step before address resolution);
    /// `None` means the destination is on the local link. `netif_index` is a row number
    /// in the service-side interface table (chapter 14).
    fn gateway_v4(&mut self, netif_index: u16, destination: [u8; 4]) -> Option<[u8; 4]>;

    /// Version 6 gateway/next-hop resolution (consumed by neighbor discovery).
    fn gateway_v6(&mut self, netif_index: u16, destination: [u8; 16]) -> Option<[u8; 16]>;
}

/// One socket readiness bit as the stack reports it to the service. The service
/// translates it into SEV_SEND/SEV_RECV event bits (chapter 02); error readiness does
/// not travel here and is expressed by the error an operation returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Readiness {
    /// Readable (data or a connection event is pending).
    pub readable: bool,
    /// Writable (the send buffer has room).
    pub writable: bool,
}

/// One UDP endpoint: address plus port. `addr` being `None` means wildcard (an address
/// not yet fixed at bind time; on send, a bare-port face outside the socket's
/// default-peer semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackEndpoint {
    /// Address; `None` = wildcard.
    pub addr: Option<StackIpAddr>,
    /// Port (host order).
    pub port: u16,
}

impl StackEndpoint {
    /// An endpoint with the given address and port.
    pub const fn new(addr: Option<StackIpAddr>, port: u16) -> Self {
        StackEndpoint { addr, port }
    }
}

/// The main loop asks the stack when the next delivery is due, the semantic position
/// of C's `sys_check_timeouts`. The millisecond instant carried by `At` uses the same
/// time base as C `sys_now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollWhen {
    /// No pump needed before this millisecond instant.
    At(u64),
    /// No timers, no retransmission pending: sleep until the next message arrives.
    Never,
}

/// The stack wall's main trait. Service code (startup's dispatch path, the sock
/// modules) depends only on the surface declared here and on no concrete stack; the
/// skeleton stage first establishes pump, readiness and the minimal lifecycle, while
/// per-family operations and data-plane details land with the buffer round. Errors
/// always return the `STACK_*` wire values of `util.rs` — they are the stack-side half
/// of the error bijection table and are not redefined here.
pub trait Stack {
    /// Advance the stack: run due timers and queued work. Readiness bits are queried
    /// per handle through [`Stack::readiness`] after the pump (poll-then-scan, the
    /// dispatch shape of Redox smolnetd).
    fn poll(&mut self, now_millis: u64) -> PollWhen;

    /// Query the current readiness bits of one socket; a closed handle returns the default (no events).
    fn readiness(&self, socket: StackSocket) -> Readiness;

    /// Open a socket of the given family and return the stack-side handle. `protocol`
    /// is consumed only by the RAW family (Minix creation's third argument, the filter
    /// protocol of `raw_new_ip_type`, rawsock.c:314 — a u8 domain, e.g. ICMP=1).
    fn open(&mut self, family: StackFamily, protocol: Option<u8>) -> Result<StackSocket, i32>;

    /// RAW send/receive (the stack face of chapter 10): full-packet semantics (including the IP header, HDRINCL).
    fn send_raw(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32>;

    fn recv_raw(&mut self, socket: StackSocket, data: &mut [u8]) -> Result<usize, i32>;

    /// Close a socket; its handle is invalid afterwards.
    fn close(&mut self, socket: StackSocket) -> Result<(), i32>;

    /// Deliver one inbound frame (the endpoint of the NDEV receive path). The frame model is refined with the buffer round.
    fn receive_frame(&mut self, frame: &[u8]) -> Result<(), i32>;

    /// Fetch one outbound frame from the stack into a service-provided buffer and
    /// return the written length; `None` means no frame is pending.
    fn transmit_frame(&mut self, frame: &mut [u8]) -> Option<usize>;

    // -- UDP half (the stack face of chapter 09; family operations go on the wall batch by batch) --

    /// Bind a local endpoint; `None` = wildcard address with the ephemeral-port
    /// semantics chosen by the implementation (allocated on the stack side). A non-UDP
    /// handle or a repeated bind answers with a stack error.
    fn bind_udp(&mut self, socket: StackSocket, local: Option<StackEndpoint>) -> Result<(), i32>;

    /// Set the default peer (the UDP semantics of connect): from then on
    /// [`Self::send_udp`] without an address sends to the peer.
    fn connect_udp(&mut self, socket: StackSocket, remote: StackEndpoint) -> Result<(), i32>;

    /// Send one chunk of data; with `remote` this is sendto semantics, `None` uses the
    /// default peer set by connect. Returns the number of bytes actually queued (a full
    /// stack buffer answers with an EWOULDBLOCK-class error, never a partial send).
    fn send_udp(
        &mut self,
        socket: StackSocket,
        data: &[u8],
        remote: Option<StackEndpoint>,
    ) -> Result<usize, i32>;

    /// Receive one chunk of data, returning `(byte count, sender endpoint)`; no incoming
    /// packet answers with an EWOULDBLOCK-class error (the caller decides whether to
    /// suspend or return immediately).
    fn recv_udp(&mut self, socket: StackSocket, data: &mut [u8])
        -> Result<(usize, StackEndpoint), i32>;

    /// Query the local endpoint (the stack half of getsockname); an unbound socket returns the wildcard endpoint.
    fn local_endpoint_udp(&self, socket: StackSocket) -> Result<StackEndpoint, i32>;

    // -- TCP half (the stack face of chapter 08) --

    /// Pre-bind the local endpoint (the TCP semantics of bind: the in-stack listener lands with listen).
    fn bind_tcp(&mut self, socket: StackSocket, local: Option<StackEndpoint>) -> Result<(), i32>;

    /// Switch to listening; `backlog` is accepted with C's semantics (the stack's model
    /// of "one connection attached to a single listener at a time" only records it and
    /// commits to no queue depth, see chapter 08).
    fn listen_tcp(&mut self, socket: StackSocket, backlog: u32) -> Result<(), i32>;

    /// Initiate a connection (the stack half of connect). The non-blocking caller
    /// semantics (EINPROGRESS) are folded by the service layer; here only "can it be
    /// started" is reported.
    fn connect_tcp(&mut self, socket: StackSocket, remote: StackEndpoint) -> Result<(), i32>;

    /// Accept a connection (the stack half of accept): if the listener already has an
    /// established connection, turn it wholesale into an accepted socket and swap a new
    /// listener in place, returning `(accepted handle, peer endpoint)`; no connection
    /// answers with a blocking-class error (the service layer decides whether to suspend
    /// or return immediately).
    fn accept_tcp(&mut self, socket: StackSocket) -> Result<(StackSocket, StackEndpoint), i32>;

    /// Query the peer endpoint (the stack half of getpeername).
    fn remote_endpoint_tcp(&self, socket: StackSocket) -> Result<StackEndpoint, i32>;

    /// Stream send (the stack half of write): queue the data in the stack buffer and
    /// return the number of bytes **actually queued** — stream semantics allow partial
    /// sends, and a full buffer returns 0 (the caller decides whether to suspend or
    /// answer EAGAIN); an unestablished connection answers with an error.
    fn send_tcp(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32>;

    /// Stream receive (the stack half of read): returns `(byte count, peer closed)`.
    /// Empty with the connection still open answers with a blocking-class error; peer
    /// closed with the data drained returns `(0, true)` — C's read returning 0 is EOF.
    fn recv_tcp(&mut self, socket: StackSocket, data: &mut [u8])
        -> Result<(usize, bool), i32>;

    /// Half-close/full-close (the stack half of shutdown): closing the write direction
    /// sends a graceful FIN (emitted by later progress); closing the read direction
    /// alone has no counterpart verb in the stack and is answered by the implementation
    /// according to its capabilities.
    fn shutdown_tcp(&mut self, socket: StackSocket, how: i32) -> Result<(), i32>;

    // -- Options half (the stack face of SDEV_SETSOCKOPT/SDEV_GETSOCKOPT) --

    /// Read or write one TCP socket option (collapsed form: a `value` sets it, `None`
    /// queries it and returns the current value. The design ruling is recorded in
    /// `25-smoltcp-shim.md` §3 — one wall method per option would make the wall grow with
    /// the option list, so the collapsed form leaves a single opening).
    /// `name` is the wire `SO_*` value; a name the wall does not know answers with a
    /// stack argument error.
    fn sockopt_tcp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32>;

    /// Read or write one UDP socket option, same shape as [`Stack::sockopt_tcp`].
    fn sockopt_udp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32>;

    // -- ioctl half (the stack face of SDEV_IOCTL) --

    /// Query the payload length of the first pending packet (the UDP half of FIONREAD:
    /// C `pktsock_test_recv` takes the head packet's tot_len, `pktsock.c:886-899`). An
    /// empty ring returns `None`.
    fn pending_recv_udp(&mut self, socket: StackSocket) -> Option<usize>;

    /// Query the queued byte count of the receive buffer (the TCP half of FIONREAD: the
    /// `tr_len` semantics of C `tcpsock_test_recv`, `tcpsock.c:1969`).
    fn pending_recv_tcp(&self, socket: StackSocket) -> usize;

    /// Query the queued byte count of the RAW receive ring (the RAW half of FIONREAD;
    /// smoltcp's raw packet ring reports only the total, and the difference from C's
    /// first-packet semantics is registered).
    fn pending_recv_raw(&self, socket: StackSocket) -> Option<usize>;

    /// Set or clear the non-blocking flag (the stack half of FIONBIO; the flag lives in
    /// a service-side slot and the send/receive paths fold it into the MSG_DONTWAIT
    /// decision — in C FIONBIO is rewritten by libc into fcntl(O_NONBLOCK),
    /// `libc/sys/ioctl.c:296`/`:330`, and this model adds the bit on the service side as
    /// a complement of the same semantics).
    fn set_nonblock(&mut self, socket: StackSocket, nonblock: bool) -> Result<(), i32>;

    /// Query the non-blocking flag.
    fn is_nonblock(&self, socket: StackSocket) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_options_match_glue_header() {
        assert_eq!(OPTION_NO_SYS, 1);
        assert_eq!(OPTION_POOL_SIZE, 0);
        assert_eq!(TCP_MAX_SEGMENT, 1460);
        assert_eq!(TCP_WINDOW, 16384);
        assert_eq!(TCP_SEND_BUFFER, 11 * 1460);
        // The three tiers of the option capacity contract (tcpsock.c:86-91, udpsock.c:29-34).
        assert_eq!(TCP_SNDBUF_MIN, 1);
        assert_eq!(TCP_SNDBUF_DEF, 32768);
        assert_eq!(TCP_SNDBUF_MAX, 131072);
        assert_eq!(TCP_RCVBUF_MIN, 16384);
        assert_eq!(TCP_RCVBUF_DEF, 32768);
        assert_eq!(TCP_RCVBUF_MAX, 131072);
        assert_eq!(UDP_SNDBUF_MIN, 1);
        assert_eq!(UDP_SNDBUF_DEF, 8192);
        assert_eq!(UDP_SNDBUF_MAX, 65535);
        assert_eq!(UDP_RCVBUF_MIN, 512);
        assert_eq!(UDP_RCVBUF_DEF, 32768);
        assert_eq!(UDP_RCVBUF_MAX, 65536);
        // The default idle interval while keepalive is enabled (lwip/priv/tcp_priv.h:138-139).
        assert_eq!(TCP_KEEPALIVE_IDLE_MS, 7_200_000);
    }

}

// The hook list mirrors `lwiphooks.h`, the patch list mirrors the patch
// directory, and the build-file count mirrors the compiled `.c` subset — all
// three are hand-maintained numbers the compiler can check statically, so they
// freeze in a const block instead of burning a runtime test on a fact the
// compiler already knows. The glue values above stay runtime
// assertions because a maintainer can edit them to a *wrong* number that is
// still a compile-time-constant self-comparison; the runtime test pins each
// against the documented `lwipopts.h` value.
#[cfg(test)]
const _: () = {
    assert!(ALL_HOOKS.len() == 4);
    assert!(PATCHES.len() == 4);
    assert!(BUILD_C_FILES == 68);
};

#[cfg(test)]
mod wall_tests {
    use super::*;
    use alloc::vec::Vec;

    /// A fixed sequence-number source held by the service: the minimal real hook implementation.
    struct FixedSource(u32);

    impl StackHooks for FixedSource {
        fn initial_sequence(&mut self, _quad: ConnectionQuad) -> u32 {
            self.0
        }

        fn gateway_v4(&mut self, _netif_index: u16, destination: [u8; 4]) -> Option<[u8; 4]> {
            (!destination.iter().all(|octet| *octet == 0)).then_some([192, 0, 2, 1])
        }

        fn gateway_v6(
            &mut self,
            _netif_index: u16,
            _destination: [u8; 16],
        ) -> Option<[u8; 16]> {
            None
        }
    }

    /// Shape stand-in: proves the wall's surface is enough for the service to walk open, pump, check readiness, close.
    struct ShapeStack {
        opened: Vec<StackSocket>,
        next_index: u16,
        readable: Option<u16>,
    }

    impl ShapeStack {
        fn new() -> Self {
            ShapeStack { opened: Vec::new(), next_index: 0, readable: None }
        }

        fn mark_readable(&mut self, index: u16) {
            self.readable = Some(index);
        }
    }

    impl Stack for ShapeStack {
        fn bind_udp(
            &mut self,
            _socket: StackSocket,
            _local: Option<StackEndpoint>,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn connect_udp(
            &mut self,
            _socket: StackSocket,
            _remote: StackEndpoint,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn send_udp(
            &mut self,
            _socket: StackSocket,
            data: &[u8],
            _remote: Option<StackEndpoint>,
        ) -> Result<usize, i32> {
            Ok(data.len())
        }

        fn recv_udp(
            &mut self,
            _socket: StackSocket,
            data: &mut [u8],
        ) -> Result<(usize, StackEndpoint), i32> {
            let n = data.len().min(3);
            data[..n].copy_from_slice(&[1, 2, 3][..n]);
            Ok((
                n,
                StackEndpoint {
                    addr: Some(StackIpAddr::V4([127, 0, 0, 1])),
                    port: 7,
                },
            ))
        }

        fn local_endpoint_udp(&self, _socket: StackSocket) -> Result<StackEndpoint, i32> {
            Ok(StackEndpoint { addr: None, port: 0 })
        }

        fn bind_tcp(
            &mut self,
            _socket: StackSocket,
            _local: Option<StackEndpoint>,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn listen_tcp(&mut self, _socket: StackSocket, _backlog: u32) -> Result<(), i32> {
            Ok(())
        }

        fn connect_tcp(
            &mut self,
            _socket: StackSocket,
            _remote: StackEndpoint,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn accept_tcp(&mut self, socket: StackSocket) -> Result<(StackSocket, StackEndpoint), i32> {
            let accepted = StackSocket::new(socket.family(), self.next_index);
            self.next_index += 1;
            Ok((
                accepted,
                StackEndpoint {
                    addr: Some(StackIpAddr::V4([127, 0, 0, 1])),
                    port: 5,
                },
            ))
        }

        fn remote_endpoint_tcp(&self, _socket: StackSocket) -> Result<StackEndpoint, i32> {
            Ok(StackEndpoint {
                addr: Some(StackIpAddr::V4([127, 0, 0, 1])),
                port: 5,
            })
        }

        fn send_tcp(&mut self, _socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
            Ok(data.len())
        }

        fn recv_tcp(
            &mut self,
            _socket: StackSocket,
            data: &mut [u8],
        ) -> Result<(usize, bool), i32> {
            let n = data.len().min(2);
            data[..n].copy_from_slice(&[7, 7][..n]);
            Ok((n, false))
        }

        fn shutdown_tcp(&mut self, _socket: StackSocket, _how: i32) -> Result<(), i32> {
            Ok(())
        }

        fn sockopt_tcp(
            &mut self,
            _socket: StackSocket,
            _name: i32,
            _value: Option<i32>,
        ) -> Result<Option<i32>, i32> {
            Err(crate::util::STACK_BAD_ARGUMENT)
        }

        fn sockopt_udp(
            &mut self,
            _socket: StackSocket,
            _name: i32,
            _value: Option<i32>,
        ) -> Result<Option<i32>, i32> {
            Err(crate::util::STACK_BAD_ARGUMENT)
        }

        fn pending_recv_udp(&mut self, _socket: StackSocket) -> Option<usize> {
            None
        }

        fn pending_recv_tcp(&self, _socket: StackSocket) -> usize {
            0
        }

        fn pending_recv_raw(&self, _socket: StackSocket) -> Option<usize> {
            None
        }

        fn set_nonblock(&mut self, _socket: StackSocket, _nonblock: bool) -> Result<(), i32> {
            Err(crate::util::STACK_BAD_VALUE)
        }

        fn is_nonblock(&self, _socket: StackSocket) -> bool {
            false
        }

        fn poll(&mut self, now_millis: u64) -> PollWhen {
            let _ = now_millis;
            PollWhen::Never
        }

        fn readiness(&self, socket: StackSocket) -> Readiness {
            let live =
                self.opened.iter().any(|open| *open == socket);
            let readable = live && self.readable == Some(socket.index());
            Readiness { readable, writable: live }
        }

        fn open(&mut self, family: StackFamily, _protocol: Option<u8>) -> Result<StackSocket, i32> {
            let socket = StackSocket::new(family, self.next_index);
            self.next_index += 1;
            self.opened.push(socket);
            Ok(socket)
        }

        fn send_raw(&mut self, _socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
            Ok(data.len())
        }

        fn recv_raw(&mut self, _socket: StackSocket, data: &mut [u8]) -> Result<usize, i32> {
            let n = data.len().min(2);
            data[..n].copy_from_slice(&[7, 7][..n]);
            Ok(n)
        }

        fn close(&mut self, socket: StackSocket) -> Result<(), i32> {
            let before = self.opened.len();
            self.opened.retain(|open| *open != socket);
            if self.opened.len() == before {
                return Err(crate::util::STACK_BAD_VALUE);
            }
            Ok(())
        }

        fn receive_frame(&mut self, frame: &[u8]) -> Result<(), i32> {
            if frame.is_empty() {
                return Err(crate::util::STACK_BAD_ARGUMENT);
            }
            Ok(())
        }

        fn transmit_frame(&mut self, frame: &mut [u8]) -> Option<usize> {
            frame.get_mut(..4)?.copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
            Some(4)
        }
    }

    fn sample_quad() -> ConnectionQuad {
        ConnectionQuad {
            local_address: StackIpAddr::V4([10, 0, 0, 1]),
            local_port: 7,
            remote_address: StackIpAddr::V4([10, 0, 0, 2]),
            remote_port: 40_000,
        }
    }

    #[test]
    fn test_wall_composes_hooks_lifecycle_and_poll() {
        let mut hooks = FixedSource(0x1234_5678);
        let mut stack = ShapeStack::new();

        assert_eq!(hooks.initial_sequence(sample_quad()), 0x1234_5678);
        assert_eq!(
            hooks.gateway_v4(0, [0, 0, 0, 0]),
            None,
            "零目的地即本地链路，无网关"
        );
        assert_eq!(hooks.gateway_v4(0, [10, 0, 0, 2]), Some([192, 0, 2, 1]));

        let socket = stack.open(StackFamily::Tcp, None).expect("open");
        assert_eq!(socket.family(), StackFamily::Tcp);
        assert!(stack.readiness(socket).writable, "新开即写就绪");
        assert_eq!(stack.poll(1_000), PollWhen::Never);

        stack.mark_readable(socket.index());
        assert!(stack.readiness(socket).readable);

        stack.close(socket).expect("close");
        assert_eq!(
            stack.readiness(socket),
            Readiness::default(),
            "关闭后的句柄不再报任何事件"
        );
        assert!(stack.close(socket).is_err(), "重复关闭报栈错误");
    }

    #[test]
    fn test_wall_frame_seam_moves_bytes_both_ways() {
        let mut stack = ShapeStack::new();

        assert!(stack.receive_frame(&[]).is_err(), "空帧被拒");
        assert!(stack.receive_frame(&[0x45, 0, 0, 20]).is_ok());

        let mut buffer = [0u8; 64];
        assert_eq!(stack.transmit_frame(&mut buffer), Some(4));
        assert_eq!(&buffer[..4], &[0xde, 0xad, 0xbe, 0xef]);
    }
}
