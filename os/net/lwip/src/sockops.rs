//! `sockops` — the service half of the socket path: sdev request translation, socket creation, and continuation of suspended replies.
//!
//! Three C counterparts: `alloc_socket` (`lwip.c:151-190`, dispatching domain and
//! type to the individual socket modules, with RAW requiring root identity), the
//! sockdriver framework's "answer now or record a suspension" (the suspendable
//! table `sockdriver.c:8-26`, the Rust counterpart of `Continuation::new`), and the
//! sockevent wakeup receipt (continue with the requested reply shape once the
//! `SEV_*` event bits fire or become ready). For the mapping boundary see
//! `25-smoltcp-shim.md` §2/§3.
//!
//! Requests translated so far: `SDEV_SOCKET` (socket creation — answered
//! immediately; C does not suspend for it either). The remaining request families
//! (read/write/connect/data plane) cross the wall one by one with their own family
//! batches; for now they are answered with the generic reply shape as "not wired"
//! (ENOSYS) — every VFS-side continuation folds an unknown reply number to EIO, so
//! the generic shape is never misread as data.

use alloc::vec::Vec;

use minix_netdriver::sockid::{self, SockId};
use minix_netdriver::socktable::{Continuation, SockTable};
use minix_sockdriver::sdev::{SdevReply, SdevRequest};
use minix_sockdriver::sockevent::SocketEvent;
use minix_types::{Endpoint, Message};

use crate::lwip_port::{Stack, StackFamily, StackSocket};
use crate::startup::raw_allowed;

/// Domain constants (the `PF_*` values of Minix3 `sys/sys/socket.h:179-331`: the
/// socket path dispatches by domain; `alloc_socket` uses the same table at
/// `lwip.c:156-188`).
pub mod domain {
    /// `PF_INET` (2, `sys/sys/socket.h:179`).
    pub const INET: i32 = 2;
    /// `PF_LINK` (18, the `AF_LINK` of `sys/sys/socket.h:196`).
    pub const LINK: i32 = 18;
    /// `PF_INET6` (24, the `AF_INET6` of `sys/sys/socket.h:206`).
    pub const INET6: i32 = 24;
    /// `PF_ROUTE` (34, the `AF_ROUTE` of `sys/sys/socket.h:222`).
    pub const ROUTE: i32 = 34;
}

/// Type constants (`SOCK_*` of `sys/socket.h`: stream 1, datagram 2, raw 3).
pub mod sock_type {
    /// `SOCK_STREAM` (1).
    pub const STREAM: i32 = 1;
    /// `SOCK_DGRAM` (2).
    pub const DGRAM: i32 = 2;
    /// `SOCK_RAW` (3).
    pub const RAW: i32 = 3;
}

/// Decoded form of one `SDEV_SOCKET` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketRequest {
    /// Caller-supplied id (VFS puts the caller endpoint here; the reply carries it back verbatim).
    pub req_id: i32,
    /// Communication domain (a [`domain`] constant).
    pub domain: i32,
    /// Socket type (a [`sock_type`] constant).
    pub sock_type: i32,
    /// Protocol number (0 = default).
    pub protocol: i32,
    /// Endpoint of the user process that issued the socket call (used for RAW's root identity check).
    pub user_endpt: i32,
}

/// Decode the `SDEV_SOCKET` payload: `mess_vfs_lsockdriver_socket { req_id@0;
/// domain@4; type@8; protocol@12; user_endpt@16 }` (ipc.h:2329-2337).
pub fn decode_socket(msg: &Message) -> Option<SocketRequest> {
    // SAFETY: the payload of this request is written into the message payload area in the field order above (no dedicated union member).
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(SocketRequest {
        req_id: word(0),
        domain: word(4),
        sock_type: word(8),
        protocol: word(12),
        user_endpt: word(16),
    })
}

/// Assemble `SDEV_SOCKET_REPLY`: `mess_lsockdriver_vfs_socket_reply
/// { req_id@0; sock_id@4; sock_id2@8 }` — a negative `sock_id` is an error (VFS
/// reads it that way); `sock_id2` is always -1 for non-pair requests.
pub fn socket_reply(req_id: i32, sock_id: i32) -> Message {
    let mut m = Message {
        m_type: SdevReply::SocketReply as i32,
        ..Message::default()
    };
    // SAFETY: the reply payload is written into the message payload area in the field order above.
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
        raw[8..12].copy_from_slice(&(-1i32).to_le_bytes());
    }
    m
}

/// Wire normalization of error codes: the `minix_types::E*` constants are positive
/// (crate convention) while `util::ERR_*` and the errors returned by stack features
/// are negative wire shapes — when the two sources meet on the path they are folded
/// into "negative or zero", so the reply payload never flips a sign a second time.
fn wire(code: i32) -> i32 {
    if code > 0 {
        -code
    } else {
        code
    }
}

/// Assemble the generic reply (`mess_lsockdriver_vfs_reply { req_id; status }` —
/// **the status sits in the second word**): request families other than socket
/// creation answer honestly through this shape with ENOSYS; every VFS-side
/// continuation folds an unknown reply number to EIO and never misreads it as data.
pub fn simple_reply(req_id: i32, status: i32) -> Message {
    let mut m = Message {
        m_type: SdevReply::Reply as i32,
        ..Message::default()
    };
    // SAFETY: the reply payload is written into the message payload area in the field order above.
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&status.to_le_bytes());
    }
    m
}

/// Dispatch domain and type to a stack family (the internet half of `alloc_socket`,
/// `lwip.c:156-172`). Stream → TCP, datagram → UDP, raw → RAW (the root identity gate
/// lives in the caller); any other type is EPROTOTYPE (`lwip.c:170`).
pub fn internet_family(sock_type: i32, is_root: bool) -> Result<StackFamily, i32> {
    match sock_type {
        sock_type::STREAM => Ok(StackFamily::Tcp),
        sock_type::DGRAM => Ok(StackFamily::Udp),
        sock_type::RAW => {
            if raw_allowed(is_root) {
                Ok(StackFamily::Raw)
            } else {
                Err(minix_types::EACCES)
            }
        }
        _ => Err(minix_types::EPROTOTYPE),
    }
}

/// The caller-identity seam (C `util_is_root` — the service holds an fproc copy and
/// looks up the uid; this model's process surface is wired with the system-process
/// batch, so the production implementation fail-closes to non-root for now and RAW
/// creation returns EACCES; tests inject a canned implementation).
pub trait IdentitySource {
    /// Whether this endpoint has root identity (the gate on RAW creation).
    fn is_root(&self, user: Endpoint) -> bool;
}

/// Production implementation: fail-closed, always non-root (the process surface of
/// C `util_is_root` is wired with the system-process batch; until then RAW creation
/// always gets EACCES — no pretending to allow it).
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedIdentity;

impl IdentitySource for FailClosedIdentity {
    fn is_root(&self, _user: Endpoint) -> bool {
        false
    }
}

/// Canned stand-in: answers from its construction parameter (the service binary tests use it to allow or deny RAW).
#[derive(Debug, Clone, Copy)]
pub struct CannedIdentity(pub bool);

impl IdentitySource for CannedIdentity {
    fn is_root(&self, _user: Endpoint) -> bool {
        self.0
    }
}

/// Class base to family (used to look up a stack family from a wire socket id).
fn family_of_class(class: sockid::SockClass) -> Option<StackFamily> {
    match class {
        sockid::SockClass::Tcp => Some(StackFamily::Tcp),
        sockid::SockClass::Udp => Some(StackFamily::Udp),
        sockid::SockClass::Raw => Some(StackFamily::Raw),
        sockid::SockClass::Rt | sockid::SockClass::Lnk => None,
    }
}

/// Family to class base (at creation the in-stack index is used directly as the
/// namespace index — the two index spaces share origin and length (stack 16 bits,
/// namespace 20 bits) — so the wire-id reverse lookup is a closed-form mapping and no
/// second mapping table has to be maintained).
fn class_base_of(family: StackFamily) -> Option<i32> {
    match family {
        StackFamily::Tcp => Some(sockid::SOCKID_TCP),
        StackFamily::Udp => Some(sockid::SOCKID_UDP),
        StackFamily::Raw => Some(sockid::SOCKID_RAW),
        StackFamily::Icmp => None,
    }
}

/// Reverse-lookup a stack handle from a wire socket id (family follows the class; the index is the in-stack index).
pub fn stack_socket_of(raw_id: i32) -> Option<StackSocket> {
    let id = SockId::from_raw(raw_id)?;
    let family = family_of_class(id.class()?)?;
    Some(StackSocket::new(family, u16::try_from(id.index()).unwrap_or(u16::MAX)))
}

// ---------------------------------------------------------------------------
// Address translation: Minix `sockaddr_in` (16 bytes, `netinet/in.h:239-245` —
// sin_len u8@0, sin_family u8@1, sin_port network-order u16@2, sin_addr@4,
// sin_zero[8]@8) and the wall endpoint, in both directions.
// ---------------------------------------------------------------------------

/// Size of `sockaddr_in` in bytes (16).
pub const INET_ADDR_LEN: usize = 16;

/// Decode a 16-byte `sockaddr_in`; a family byte other than `AF_INET` (2) is rejected.
pub fn decode_inet_endpoint(bytes: &[u8]) -> Option<crate::lwip_port::StackEndpoint> {
    if bytes.len() < INET_ADDR_LEN || bytes[1] != domain::INET as u8 {
        return None;
    }
    let port = u16::from_be_bytes([bytes[2], bytes[3]]);
    Some(crate::lwip_port::StackEndpoint {
        addr: Some(crate::lwip_port::StackIpAddr::V4([
            bytes[4], bytes[5], bytes[6], bytes[7],
        ])),
        port,
    })
}

/// Encode a 16-byte `sockaddr_in` (sin_len = 16, port converted to network order).
pub fn encode_inet_endpoint(endpoint: crate::lwip_port::StackEndpoint) -> [u8; INET_ADDR_LEN] {
    let mut out = [0u8; INET_ADDR_LEN];
    out[0] = INET_ADDR_LEN as u8;
    out[1] = domain::INET as u8;
    out[2..4].copy_from_slice(&endpoint.port.to_be_bytes());
    if let Some(crate::lwip_port::StackIpAddr::V4(octets)) = endpoint.addr {
        out[4..8].copy_from_slice(&octets);
    }
    out
}

// ---------------------------------------------------------------------------
// Copy seam: every fetch across address spaces goes through a seam. The production
// implementation delegates to minix-sys's safecopy verbs (unreachable on the host,
// which reports the error honestly); tests inject a canned implementation.
// ---------------------------------------------------------------------------

/// The seam over the kernel copy verbs (C `sys_safecopyfrom`/`sys_safecopyto`).
pub trait CopyTransport {
    /// Copy the granter's memory into `buf` through a grant.
    fn safecopy_from(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32>;

    /// Copy `buf` to the granter through a grant.
    fn safecopy_to(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32>;
}

/// Canned stand-in: a grant-number-to-bytes map, plus a ledger of `safecopy_to`
/// writes for assertions (the service binary tests use it to feed addresses and data
/// without a real kernel).
#[derive(Debug, Default)]
pub struct CannedCopyTransport {
    /// Grant number → preset content (the source for `safecopy_from`).
    pub from: Vec<(i32, Vec<u8>)>,
    /// Ledger of `safecopy_to` writes (grant number and bytes).
    pub written: Vec<(i32, Vec<u8>)>,
}

impl CopyTransport for CannedCopyTransport {
    fn safecopy_from(
        &mut self,
        _granter: i32,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        let start = offset as usize;
        let Some((_, src)) = self.from.iter().find(|(id, _)| *id == grant) else {
            return Err(minix_types::EFAULT);
        };
        let end = (start + buf.len()).min(src.len());
        if start > end {
            return Err(minix_types::EINVAL);
        }
        buf.copy_from_slice(&src[start..end]);
        Ok(())
    }

    fn safecopy_to(
        &mut self,
        _granter: i32,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32> {
        self.written.push((grant, buf.to_vec()));
        let _ = offset;
        Ok(())
    }
}

/// Production implementation: minix-sys kernel calls (the trap is unreachable in a
/// host build; callers fold the error — consistent with the host behavior of the
/// other live surfaces).
#[derive(Debug, Default, Clone, Copy)]
pub struct SysCopyTransport;

impl CopyTransport for SysCopyTransport {
    fn safecopy_from(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyfrom(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter,
            grant,
            offset,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn safecopy_to(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyto(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter,
            grant,
            offset,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
    }
}

/// Socket creation (the internet half of C `alloc_socket` plus the module's
/// `*_socket` registration half): domain gate → type dispatch → open in the stack →
/// namespace allocation → service-table registration. Returns the wire socket id
/// (`SockId::raw`, carried verbatim in the reply payload).
///
/// The modules for the `PF_ROUTE`/`PF_LINK` domains (rtsock/lnksock) are wired in a
/// later batch and answer as not wired (ENOSYS) for now; an unknown domain returns
/// EAFNOSUPPORT as C does (`lwip.c:186`).
pub fn open_socket(
    stack: &mut dyn Stack,
    table: &mut SockTable,
    req: &SocketRequest,
    is_root: bool,
) -> Result<i32, i32> {
    // RT/LNK do not enter the stack: they are service-side table registrations
    // (C's rtsock/lnksocket are table objects too); their message plane
    // (chapters 20/11) is wired in a later batch.
    if req.domain == domain::ROUTE {
        return open_service_socket(table, sockid::SockClass::Rt);
    }
    if req.domain == domain::LINK {
        return open_service_socket(table, sockid::SockClass::Lnk);
    }
    let family = match req.domain {
        domain::INET | domain::INET6 => internet_family(req.sock_type, is_root)?,
        _ => return Err(minix_types::EAFNOSUPPORT),
    };
    // RAW creation validates the protocol number as C does (rawsock.c:312-313 —
    // reject negatives and values beyond u8), then passes the protocol value into
    // the stack (the filter surface). TCP/UDP do not consume it.
    let raw_protocol = match family {
        StackFamily::Raw => {
            if req.protocol < 0 || req.protocol > u8::MAX as i32 {
                return Err(minix_types::EPROTONOSUPPORT);
            }
            Some(req.protocol as u8)
        }
        _ => None,
    };
    let stack_socket = stack.open(family, raw_protocol)?;

    let id = class_base_of(family)
        .and_then(sockid::SockClass::from_base)
        .and_then(|c| SockId::from_class(c, stack_socket.index() as u32));
    let Some(id) = id else {
        //[SD19-gate] error path cleanup: a failed close leaks one orphan fd
        #[allow(clippy::let_underscore_must_use)]
        let _ = stack.close(stack_socket);
        return Err(minix_types::EINVAL);
    };
    if table.add(id).is_err() {
        //[SD19-gate] error path cleanup: a failed close leaks one orphan fd
        #[allow(clippy::let_underscore_must_use)]
        let _ = stack.close(stack_socket);
        return Err(minix_types::EAGAIN);
    }
    Ok(id.raw())
}

/// Service-side creation (rtsock/lnksock domains: they do not enter the stack, they
/// are pure service-table objects — C's `rtsock_socket`/`lnksock_socket` are table
/// registrations too). Scans the class linearly for a free index (these two domains
/// hold few sockets, so the scan cost is negligible).
pub fn open_service_socket(
    table: &mut SockTable,
    class: sockid::SockClass,
) -> Result<i32, i32> {
    for index in 0..=sockid::INDEX_MASK {
        let Some(id) = SockId::from_class(class, index) else {
            continue;
        };
        if !table.contains(id) && table.add(id).is_ok() {
            return Ok(id.raw());
        }
    }
    Err(minix_types::ENOSPC)
}

// ---------------------------------------------------------------------------
// UDP family operation translation (the sdev side of chapter 09): the five
// BIND/CONNECT/GETSOCKNAME/SEND/RECEIVE. The data plane moves bytes between the
// user grant and the stack buffers through the copy seam.
// ---------------------------------------------------------------------------

/// Payload of `SDEV_BIND`/`SDEV_CONNECT`/`SDEV_GETSOCKNAME`:
/// `mess_vfs_lsockdriver_addr { req_id@0; sock_id@4; grant@8; len@12;
/// user_endpt@16; sflags@20 }` (ipc.h:2258-2269).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddrRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub grant: i32,
    pub len: u32,
    pub user_endpt: i32,
    pub sflags: u32,
}

/// Decode the `mess_vfs_lsockdriver_addr` payload.
pub fn decode_addr(msg: &Message) -> Option<AddrRequest> {
    // SAFETY: the payload is written into the message payload area in the field order above.
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(AddrRequest {
        req_id: word(0),
        sock_id: word(4),
        grant: word(8),
        len: word(12) as u32,
        user_endpt: word(16),
        sflags: word(20) as u32,
    })
}

/// Payload of `SDEV_SEND`/`SDEV_RECV`: `mess_vfs_lsockdriver_sendrecv
/// { req_id@0; sock_id@4; data_grant@8; [pad@12]; data_len@16(8B);
/// ctl_grant@24; ctl_len@28; addr_grant@32; addr_len@36; user_endpt@40;
/// flags@44 }` (ipc.h:2305-2317; the LP64 total of 56 matches the assert macro;
/// `data_len` is a size_t and takes 8 bytes while `ctl_len`/`addr_len` are unsigned
/// int and take 4 — the field distances match the VFS-side encoding word for word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendRecvRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub data_grant: i32,
    pub data_len: usize,
    pub ctl_grant: i32,
    pub ctl_len: usize,
    pub addr_grant: i32,
    pub addr_len: usize,
    pub user_endpt: i32,
    pub flags: u32,
}

/// Decode the `mess_vfs_lsockdriver_sendrecv` payload.
pub fn decode_sendrecv(msg: &Message) -> Option<SendRecvRequest> {
    // SAFETY: the payload is written into the message payload area in the field order above; only data_len is 8 bytes.
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(SendRecvRequest {
        req_id: word(0),
        sock_id: word(4),
        data_grant: word(8),
        data_len: usize::from_le_bytes(raw[16..24].try_into().unwrap()),
        ctl_grant: word(24),
        ctl_len: word(28) as usize,
        addr_grant: word(32),
        addr_len: word(36) as usize,
        user_endpt: word(40),
        flags: word(44) as u32,
    })
}

/// Assemble `SDEV_RECV_REPLY`: `mess_lsockdriver_vfs_recv_reply { req_id@0;
/// status@4; ctl_len@8; addr_len@12; flags@16 }`.
pub fn recv_reply(req_id: i32, status: i32, ctl_len: u32, addr_len: u32) -> Message {
    let mut m = Message {
        m_type: SdevReply::ReceiveReply as i32,
        ..Message::default()
    };
    // SAFETY: the reply payload is written into the message payload area in the field order above.
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&status.to_le_bytes());
        raw[8..12].copy_from_slice(&ctl_len.to_le_bytes());
        raw[12..16].copy_from_slice(&addr_len.to_le_bytes());
    }
    m
}

/// Payload of `SDEV_LISTEN`/`SDEV_SHUTDOWN`: `mess_vfs_lsockdriver_simple
/// { req_id@0; sock_id@4; param@8 }` (ipc.h:2319-2327).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub param: i32,
}

/// Decode the `mess_vfs_lsockdriver_simple` payload.
pub fn decode_simple(msg: &Message) -> Option<SimpleRequest> {
    // SAFETY: the payload is written into the message payload area in the field order above.
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(SimpleRequest {
        req_id: word(0),
        sock_id: word(4),
        param: word(8),
    })
}

/// Assemble `SDEV_ACCEPT_REPLY`: `mess_lsockdriver_vfs_accept_reply
/// { req_id@0; sock_id@4; status@8; len@12 }` — the new socket id comes back in the reply.
pub fn accept_reply(req_id: i32, sock_id: i32, status: i32, len: u32) -> Message {
    let mut m = Message {
        m_type: SdevReply::AcceptReply as i32,
        ..Message::default()
    };
    // SAFETY: the reply payload is written into the message payload area in the field order above.
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
        raw[8..12].copy_from_slice(&status.to_le_bytes());
        raw[12..16].copy_from_slice(&len.to_le_bytes());
    }
    m
}

/// One sticky note: all the state needed to finish after a suspension (C's
/// `w_drv_sendrec` raw message is split into fields here — the continuation must
/// copy data, write the peer address, and answer the original request from it).
/// The sticky note for TCP connect: reply 0 once the connection is established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingConnect {
    pub sock_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
}

/// Sticky note for a TCP send: suspended when the buffer is full; once writable,
/// copy the data again and retry (the data lives in user memory, so every attempt
/// must re-copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTcpSend {
    pub sock_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
    pub data_grant: i32,
    pub data_len: usize,
    pub user_endpt: i32,
}

/// Sticky note for a TCP receive: suspended when there is no data; answered later
/// once readable (EOF replies 0, i.e. C's read returning 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTcpRecv {
    pub sock_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
    pub data_grant: i32,
    pub data_len: usize,
    pub user_endpt: i32,
}

/// Sticky note for TCP accept: reply the new socket id and the peer address once the accept completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingAccept {
    pub listener_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
    pub addr_grant: i32,
    pub addr_len: usize,
    pub user_endpt: i32,
}

/// The book of sticky notes (every bit of state for "not done yet, leave a note" on
/// the service side; the wakeup scan and the timeout removal both work on this book).
#[derive(Debug, Default)]
pub struct PendingTables {
    /// UDP receive suspension (with the peer-address outbound face).
    pub recvs: Vec<PendingRecv>,
    /// TCP connect suspension.
    pub connects: Vec<PendingConnect>,
    /// TCP accept suspension.
    pub accepts: Vec<PendingAccept>,
    /// TCP send suspension (buffer full).
    pub tcp_sends: Vec<PendingTcpSend>,
    /// TCP receive suspension (no data).
    pub tcp_recvs: Vec<PendingTcpRecv>,
    /// select waits (one-shot trigger: after the reply VFS re-arms by itself).
    pub selects: Vec<PendingSelect>,
}

/// One select wait: the socket id, the VFS endpoint (destination of the select1
/// reply) and the readiness bits of interest (`SDEV_OP_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingSelect {
    pub sock_id: i32,
    pub vfs: Endpoint,
    pub ops: u8,
}

impl PendingTables {
    /// Cancel every suspended state of one caller on one socket (the service half of
    /// C `sockevent_cancel`; SDEV_CANCEL has no reply, so after the cancel the
    /// original request just goes silent through the timeout/event path).
    pub fn cancel_for(&mut self, sock_id: i32, who: Endpoint) {
        self.recvs.retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.connects
            .retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.accepts
            .retain(|n| !(n.listener_id == sock_id && n.caller == who));
        self.tcp_sends
            .retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.tcp_recvs
            .retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.selects
            .retain(|n| !(n.sock_id == sock_id && n.vfs == who));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingRecv {
    /// Wire socket id.
    pub sock_id: i32,
    /// The suspended caller (the VFS process endpoint, destination of the continuation reply).
    pub caller: Endpoint,
    /// req_id of the original request (carried back verbatim in the reply).
    pub req_id: i32,
    /// Data grant (received data is copied to it).
    pub data_grant: i32,
    /// Length of the user data buffer.
    pub data_len: usize,
    /// Peer address grant (sockaddr_in is written back to it).
    pub addr_grant: i32,
    /// Length of the user address buffer.
    pub addr_len: usize,
    /// Granter endpoint (the granter argument of the copy verbs).
    pub user_endpt: i32,
}

/// How a close half-closes (`SHUT_*` values of `sys/socket.h`, the same table as the
/// shim's `shutdown_tcp`).
pub mod shut {
    /// `SHUT_RD` (0).
    pub const RD: i32 = 0;
    /// `SHUT_WR` (1).
    pub const WR: i32 = 1;
    /// `SHUT_RDWR` (2).
    pub const RDWR: i32 = 2;
}

/// Cancel (the service half of C `sdev_cancel`, the "no reply" face of sdev.c:25):
/// drop every suspended state of this caller on this socket — both the service-side
/// sticky notes and the in-table bookkeeping. **Sends no reply** (C: the cancel's
/// reply is the original request's reply; here the silence yields to it).
pub fn cancel_road(table: &mut SockTable, pending: &mut PendingTables, msg: &Message) {
    let Some(req) = decode_simple(msg) else {
        return;
    };
    let Some(id) = SockId::from_raw(req.sock_id) else {
        return;
    };
    let who = Endpoint(req.req_id);
    table.cancel(id, who);
    pending.cancel_for(req.sock_id, who);
}

/// The select path (C `sdev_select` has two reply layers): reply type one immediately
/// when ready; otherwise register the wait (one-shot trigger — after the reply VFS
/// decides by itself whether to re-arm), woken by ready-scan.
pub fn select_road(
    stack: &mut dyn Stack,
    table: &mut SockTable,
    pending: &mut PendingTables,
    vfs: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some((sock_id, ops)) = decode_select(msg) else {
        return Some(select1_reply(sock_id_from(0), 0));
    };
    let Some(stack_socket) = stack_socket_of(sock_id) else {
        return Some(select1_reply(sock_id, 0));
    };
    let readiness = stack.readiness(stack_socket);
    let ready = matched_ops(readiness, ops);
    if ready != 0 {
        return Some(select1_reply(sock_id, ready));
    }
    let Some(id) = SockId::from_raw(sock_id) else {
        return Some(select1_reply(sock_id, 0));
    };
    // Only one wait per socket (C `ss_endpt` is a single slot; a repeated select replaces it).
    if let Err(e) = table.register_select(id, vfs, ops) {
        let _ = e;
        return Some(select1_reply(sock_id, 0));
    }
    pending.selects.retain(|s| s.sock_id != sock_id);
    pending.selects.push(PendingSelect { sock_id, vfs, ops });
    None
}

fn sock_id_from(v: usize) -> i32 {
    v as i32
}

/// Close (C `sdev_close` → each module's close + table removal): stack classes close
/// the in-stack socket first, service-side classes (rtsock/lnksock) only remove the
/// table entry; both reply 0. C's suspendable close (a graceful finish while the
/// buffer drains) comes with the interface batch — this is an immediate close, a
/// registered difference.
pub fn close_socket(
    stack: &mut dyn Stack,
    table: &mut SockTable,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_simple(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let Some(id) = SockId::from_raw(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    if let Some(stack_socket) = stack_socket_of(req.sock_id) {
        //[SD19-gate] explicit close; C's close() return value is not consumed
        #[allow(clippy::let_underscore_must_use)]
        let _ = stack.close(stack_socket);
    }
    table.close(id);
    Some(simple_reply(req.req_id, 0))
}

/// Half-close (C `sdev_shutdown` → `sock_shutdown`): stack classes forward to
/// [`Stack::shutdown_tcp`]; service-side classes (rtsock/lnksock) have no
/// peer-half-close semantics and answer success with no operation (matching C's
/// datagram shutdown behavior).
pub fn shutdown_socket(
    stack: &mut dyn Stack,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_simple(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    match stack_socket_of(req.sock_id) {
        Some(stack_socket) => Some(match stack.shutdown_tcp(stack_socket, req.param) {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        }),
        None => Some(simple_reply(req.req_id, 0)),
    }
}

/// Decode the `SDEV_SELECT` payload: `mess_vfs_lsockdriver_select { sock_id@0; ops@4 }`
/// (ipc.h:2295-2302; ops holds `SDEV_OP_*` readiness bits).
pub fn decode_select(msg: &Message) -> Option<(i32, u8)> {
    // SAFETY: the payload is written into the message payload area in the field order above.
    let raw = unsafe { &msg.m_u.raw };
    Some((
        i32::from_le_bytes(raw[0..4].try_into().unwrap()),
        i32::from_le_bytes(raw[4..8].try_into().unwrap()) as u8,
    ))
}

/// Assemble the select type-one reply: `mess_lsockdriver_vfs_select_reply
/// { sock_id@0; status@4 }` — status carries the ready `SDEV_OP_*` bits.
pub fn select1_reply(sock_id: i32, ready: u8) -> Message {
    let mut m = Message {
        m_type: SdevReply::SelectReply1 as i32,
        ..Message::default()
    };
    // SAFETY: the reply payload is written into the message payload area in the field order above.
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&sock_id.to_le_bytes());
        raw[4..8].copy_from_slice(&(ready as i32).to_le_bytes());
    }
    m
}

/// Intersection of ready bits and bits of interest (the read/write faces; the error bit goes with the error field in its own batch).
fn matched_ops(readiness: crate::lwip_port::Readiness, ops: u8) -> u8 {
    use minix_sockdriver::sdev::{SDEV_OP_RD, SDEV_OP_WR};
    let mut matched = 0u8;
    if readiness.readable {
        matched |= SDEV_OP_RD;
    }
    if readiness.writable {
        matched |= SDEV_OP_WR;
    }
    matched & ops
}

/// Translation entry for the UDP path: `msg` is some request on a UDP-class socket.
/// Returns `Some(reply)` to answer immediately; `None` = suspended (a sticky note has
/// entered `pending`, the VFS-side calling process is asleep, do not reply; once
/// woken, [`ready_scan`] continues the answer).
pub fn translate_udp(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let m_type = msg.m_type;
    if m_type == SdevRequest::Bind as i32 || m_type == SdevRequest::Connect as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        // The address lives in user memory (grant); copy it in, then translate.
        let mut bytes = [0u8; INET_ADDR_LEN];
        let n = req.len.min(INET_ADDR_LEN as u32) as usize;
        if copy
            .safecopy_from(req.user_endpt, req.grant, 0, &mut bytes[..n])
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        let Some(endpoint) = decode_inet_endpoint(&bytes) else {
            return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
        };
        let result = if m_type == SdevRequest::Bind as i32 {
            stack.bind_udp(stack_socket, Some(endpoint))
        } else {
            stack.connect_udp(stack_socket, endpoint)
        };
        return Some(match result {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        });
    }
    if m_type == SdevRequest::GetSockName as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let Ok(endpoint) = stack.local_endpoint_udp(stack_socket) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let bytes = encode_inet_endpoint(endpoint);
        let len = req.len.min(INET_ADDR_LEN as u32) as usize;
        return match copy.safecopy_to(req.user_endpt, req.grant, 0, &bytes[..len]) {
            Ok(()) => Some(simple_reply(req.req_id, len as i32)),
            Err(code) => Some(simple_reply(req.req_id, wire(code))),
        };
    }
    if m_type == SdevRequest::Send as i32 || m_type == SdevRequest::Receive as i32 {
        return translate_udp_data(stack, copy, table, pending, caller, msg);
    }
    Some(simple_reply(0, -(minix_types::ENOSYS)))
}

fn translate_udp_data(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_sendrecv(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let Some(stack_socket) = stack_socket_of(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    let sending = msg.m_type == SdevRequest::Send as i32;
    if sending {
        // Copy the data into a stack buffer and send; the length is capped by the
        // user's data_len (the upper bound in this batch is the stack buffer size).
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        if copy
            .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        // Peer address: an addr_len present means sendto semantics, otherwise use
        // connect's default peer.
        let remote = if req.addr_len >= INET_ADDR_LEN {
            let mut addr_bytes = [0u8; INET_ADDR_LEN];
            if copy
                .safecopy_from(req.user_endpt, req.addr_grant, 0, &mut addr_bytes)
                .is_err()
            {
                return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
            }
            decode_inet_endpoint(&addr_bytes)
        } else {
            None
        };
        match stack.send_udp(stack_socket, &data, remote) {
            Ok(n) => Some(simple_reply(req.req_id, n as i32)),
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        }
    } else {
        // Receive: try once first (a packet may already be there); empty and
        // non-blocking replies EAGAIN at once; empty and blocking suspends with
        // bookkeeping (the caller is suspended on the VFS side, no reply here), and
        // ready-scan answers it later once woken.
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        match stack.recv_udp(stack_socket, &mut data) {
            Ok((n, peer)) => {
                // Both the data and the peer address must be copied to the user
                // (C recv's two safecopyto on the data plane); if the copy fails,
                // report an error — never drop data under a success code.
                let addr_bytes = encode_inet_endpoint(peer);
                let addr_len = req.addr_len.min(INET_ADDR_LEN);
                if copy
                    .safecopy_to(req.user_endpt, req.data_grant, 0, &data[..n])
                    .is_err()
                {
                    return Some(recv_reply(req.req_id, -(minix_types::EFAULT), 0, 0));
                }
                if copy
                    .safecopy_to(req.user_endpt, req.addr_grant, 0, &addr_bytes[..addr_len])
                    .is_err()
                {
                    return Some(recv_reply(req.req_id, -(minix_types::EFAULT), 0, 0));
                }
                Some(recv_reply(req.req_id, n as i32, 0, addr_len as u32))
            }
            Err(_) => {
                // Non-block decision: either the request flag (MSG_DONTWAIT) or the
                // socket's FIONBIO flag (ioctl batch) present means reply immediately.
                if req.flags & minix_sockdriver::sdev::MSG_DONTWAIT != 0
                    || stack.is_nonblock(stack_socket)
                {
                    return Some(recv_reply(req.req_id, -(minix_types::EAGAIN), 0, 0));
                }
                // Suspend: wakeup event = readable or peer closed; no timeout
                // (SO_RCVTIMEO comes with the options batch). The sticky note enters
                // `pending`; once woken, ready-scan completes the receive and continues
                // the answer.
                if stack_socket_of(req.sock_id).is_none() {
                    return Some(recv_reply(req.req_id, -(minix_types::EBADF), 0, 0));
                }
                let Some(id) = SockId::from_raw(req.sock_id) else {
                    return Some(recv_reply(req.req_id, -(minix_types::EBADF), 0, 0));
                };
                let wake = SocketEvent::Receive.bits() | SocketEvent::Close.bits();
                let Some(continuation) =
                    Continuation::new(SdevRequest::Receive, caller, wake, None)
                else {
                    return Some(recv_reply(req.req_id, -(minix_types::EINVAL), 0, 0));
                };
                if table.suspend(id, continuation).is_err() {
                    return Some(recv_reply(req.req_id, -(minix_types::EBADF), 0, 0));
                }
                pending.recvs.push(PendingRecv {
                    sock_id: req.sock_id,
                    caller,
                    req_id: req.req_id,
                    data_grant: req.data_grant,
                    data_len: req.data_len,
                    addr_grant: req.addr_grant,
                    addr_len: req.addr_len,
                    user_endpt: req.user_endpt,
                });
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Options path (SDEV_SETSOCKOPT/SDEV_GETSOCKOPT, the sdev-side closeout of chapters
// 08/09): C splits option handling into two layers — the libsockevent framework half
// handles SOL_SOCKET's switch-type and capacity-type options (sockevent.c:1823-1966),
// while each socket module's sop_setsockopt handles protocol-level options
// (tcpsock.c:2123/udpsock.c:580). This model serves both layers: the supported
// SOL_SOCKET names go through wall methods, and all other names plus protocol-level
// options are honestly refused per the C framework's "a driver must answer
// ENOPROTOOPT for options it does not recognize" contract (the dual note at
// sockdriver.c:843-844) — see FIXLOG and chapter 25 §3 for the registered differences.
// ---------------------------------------------------------------------------

/// Payload of `SDEV_SETSOCKOPT`/`SDEV_GETSOCKOPT`:
/// `mess_vfs_lsockdriver_getset { req_id@0; sock_id@4; level@8; name@12;
/// grant@16; len@20 }` (ipc.h:2270-2281).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetSetRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub level: i32,
    pub name: i32,
    pub grant: i32,
    pub len: u32,
}

/// Decode the `mess_vfs_lsockdriver_getset` payload.
pub fn decode_getset(msg: &Message) -> Option<GetSetRequest> {
    // SAFETY: the payload is written into the message payload area in the field order above.
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(GetSetRequest {
        req_id: word(0),
        sock_id: word(4),
        level: word(8),
        name: word(12),
        grant: word(16),
        len: word(20) as u32,
    })
}

/// Options path: the set direction copies the option value in and stores it on the
/// wall; the get direction reads the current value, copies it back to the user grant,
/// and replies the success code as the byte count (C `do_getsockopt`'s "the main
/// reply code is the length" contract, sockdriver.c:852-861; a successful set replies
/// 0, sockdriver.c:827-834).
pub fn sockopt_road(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_getset(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let setting = msg.m_type == SdevRequest::SetSockOpt as i32;
    // Only the SOL_SOCKET level goes through the wall: protocol-level options
    // (IPPROTO_TCP/IPPROTO_IP NODELAY, the multicast family, etc.) are handled by
    // each module's sop_setsockopt in C and are not wired in this model — honestly
    // refused as ENOPROTOOPT (a registered difference).
    if req.level != crate::lwip_port::SOL_SOCKET {
        return Some(simple_reply(req.req_id, wire(minix_types::ENOPROTOOPT)));
    }
    // RT/LNK are service-side classes with no option-surface semantics (C
    // rtsock_setsockopt's USELOOPBACK/RCVBUF are evaluated together with the
    // announcement-queue batch; registered).
    let service_class = SockId::from_raw(req.sock_id)
        .and_then(|id| id.class())
        .is_some_and(|class| matches!(class, sockid::SockClass::Rt | sockid::SockClass::Lnk));
    if service_class {
        return Some(simple_reply(req.req_id, wire(minix_types::ENOPROTOOPT)));
    }
    let Some(stack_socket) = stack_socket_of(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    // Copy in the value for the set direction: C `sockdriver_copyin_opt` requires
    // the length to be exactly sizeof(int) (sockdriver.c:184-194, anything else is EINVAL).
    let requested = if setting {
        if req.len != 4 {
            return Some(simple_reply(req.req_id, -(minix_types::EINVAL)));
        }
        let mut bytes = [0u8; 4];
        if copy
            .safecopy_from(caller.0, req.grant, 0, &mut bytes)
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        Some(i32::from_le_bytes(bytes))
    } else {
        None
    };
    let outcome = match stack_socket.family() {
        StackFamily::Tcp => stack.sockopt_tcp(stack_socket, req.name, requested),
        StackFamily::Udp => stack.sockopt_udp(stack_socket, req.name, requested),
        // RAW's option surface comes with the HDRINCL options batch (the cross-line ledger stays registered).
        _ => Err(crate::util::STACK_BAD_ARGUMENT),
    };
    Some(match outcome {
        // A name the wall does not know: an honest "option not supported".
        Err(e) if e == crate::util::STACK_BAD_ARGUMENT => {
            simple_reply(req.req_id, wire(minix_types::ENOPROTOOPT))
        }
        Err(e) => simple_reply(req.req_id, wire(e)),
        Ok(None) => simple_reply(req.req_id, 0),
        Ok(Some(value)) => {
            // Get direction: copy the value back to the user grant, truncated to the
            // user's optlen (C `sockdriver_copyout_opt`'s truncation semantics,
            // sockdriver.c:201-212); the success code is the copied length.
            let bytes = value.to_le_bytes();
            let out_len = (req.len as usize).min(bytes.len());
            match copy.safecopy_to(caller.0, req.grant, 0, &bytes[..out_len]) {
                Ok(()) => simple_reply(req.req_id, out_len as i32),
                Err(code) => simple_reply(req.req_id, wire(code)),
            }
        }
    })
}

// ---------------------------------------------------------------------------
// ioctl path (SDEV_IOCTL): FIONREAD is the framework-half behavior in C (the
// `sop_test_recv` exit of `sockevent_ioctl`, sockevent.c:1779-1793); FIONBIO
// is rewritten by libc into fcntl(O_NONBLOCK) in C (`libc/sys/ioctl.c:296`/
// `:330`), and this model adds a flag bit with the same semantics on the
// service side; any other request answers through C's service-side ENOTTY
// exit (sockevent.c:1765-1769 plus ifconf_ioctl's default branch).
// ---------------------------------------------------------------------------

/// `FIONREAD` (`_IOR('f', 127, int)` = 0x4004667F, `sys/filio.h:47`).
pub const FIONREAD: u64 = 0x4004_667F;
/// `FIONBIO` (`_IOW('f', 126, int)` = 0x8004667E, `sys/filio.h:48`; the
/// direction bit is `IOC_IN`).
pub const FIONBIO: u64 = 0x8004_667E;

/// Payload of `SDEV_IOCTL`: `mess_vfs_lsockdriver_ioctl { req_id@0;
/// sock_id@4; request@8(8B); grant@16; user_endpt@24; sflags@28 }`
/// (ipc.h:2283-2294; `request` is an unsigned long, 8 bytes under LP64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoctlRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub request: u64,
    pub grant: i32,
    pub user_endpt: i32,
    pub sflags: u32,
}

/// Decode the `mess_vfs_lsockdriver_ioctl` payload.
pub fn decode_ioctl(msg: &Message) -> Option<IoctlRequest> {
    // SAFETY: the payload is written into the message payload area in the field order above; only request is 8 bytes.
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(IoctlRequest {
        req_id: word(0),
        sock_id: word(4),
        request: u64::from_le_bytes(raw[8..16].try_into().unwrap()),
        grant: word(16),
        user_endpt: word(24),
        sflags: word(28) as u32,
    })
}

/// ioctl path: FIONREAD copies the pending-receive amount back to the user
/// grant (reply 0, the value travels in the buffer — C `sockevent_ioctl`'s
/// `copyout` plus `reply_generic(OK)` shape); FIONBIO sets/clears the socket's
/// non-blocking flag; any other command is ENOTTY.
pub fn ioctl_road(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_ioctl(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    // RT/LNK are service-side classes: no ioctl semantics (C rtsock/lnksock
    // have no sop_ioctl, and the framework folds it to ENOTTY).
    let service_class = SockId::from_raw(req.sock_id)
        .and_then(|id| id.class())
        .is_some_and(|class| matches!(class, sockid::SockClass::Rt | sockid::SockClass::Lnk));
    if service_class {
        return Some(simple_reply(req.req_id, -(minix_types::ENOTTY)));
    }
    let Some(stack_socket) = stack_socket_of(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    match req.request {
        FIONREAD => {
            let val: i32 = match stack_socket.family() {
                StackFamily::Tcp => stack.pending_recv_tcp(stack_socket) as i32,
                StackFamily::Udp => stack.pending_recv_udp(stack_socket).unwrap_or(0) as i32,
                StackFamily::Raw => stack.pending_recv_raw(stack_socket).unwrap_or(0) as i32,
                StackFamily::Icmp => 0,
            };
            Some(match copy.safecopy_to(caller.0, req.grant, 0, &val.to_le_bytes()) {
                Ok(()) => simple_reply(req.req_id, 0),
                Err(code) => simple_reply(req.req_id, wire(code)),
            })
        }
        FIONBIO => {
            let mut bytes = [0u8; 4];
            if copy
                .safecopy_from(caller.0, req.grant, 0, &mut bytes)
                .is_err()
            {
                return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
            }
            let nonblock = i32::from_le_bytes(bytes) != 0;
            Some(match stack.set_nonblock(stack_socket, nonblock) {
                Ok(()) => simple_reply(req.req_id, 0),
                Err(e) => simple_reply(req.req_id, wire(e)),
            })
        }
        _ => Some(simple_reply(req.req_id, -(minix_types::ENOTTY))),
    }
}

// ---------------------------------------------------------------------------
// rtsock/lnksock message path (the sdev-side closeout of chapters 20/11). RT
// read/write = add/delete/query messages for the routing table: write parses
// one RTM frame and applies it to the table (add/delete/change), read exports
// RTM_GET frames from the table. C's read pulls the "route change announcement
// queue" (table changes broadcast to all routing sockets), one at a time; this
// model's read exports a table snapshot, fitting as many entries as it can —
// isomorphic to C's sysctl export loop (rtsock.c:1473-1476), with the
// announcement-queue difference registered. The LNK domain has no peer
// half-close and no data plane: read/write answer EOPNOTSUPP per C's NULL hook
// semantics (registered; close is already closed out).
// ---------------------------------------------------------------------------

/// Wire sockaddr to a routing-table row's family and 16-byte destination
/// representation; a family other than version 4/6 returns `None` (the caller
/// folds it to EAFNOSUPPORT).
fn route_address(sa: crate::rtsock::RtSockaddr) -> Option<(crate::route::IpVersion, [u8; 16])> {
    match sa.family {
        2 => {
            let mut dest = [0u8; 16];
            dest[..4].copy_from_slice(&sa.addr[..4]);
            Some((crate::route::IpVersion::V4, dest))
        }
        24 => Some((crate::route::IpVersion::V6, sa.addr)),
        _ => None,
    }
}

/// Message path of the RT domain. Send = write one RTM frame into the table;
/// Receive = export a table snapshot. Type and identity gates follow C
/// rtsock_put: ADD/DELETE/CHANGE require root identity (rtsock.c:545-550,
/// `is_root` goes through the identity seam); GET is accepted successfully —
/// the query's answer is the read export surface (registered difference: C
/// piggybacks a single query result on the write receipt); LOCK and any other
/// type are EOPNOTSUPP (the metric-lock surface is not wired; registered).
pub fn rt_road(
    routes: Option<&mut crate::route::RouteTable>,
    copy: &mut dyn CopyTransport,
    identity: &dyn IdentitySource,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_sendrecv(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let receiving = msg.m_type == SdevRequest::Receive as i32;
    let Some(routes) = routes else {
        // Startup chain step 9 (route-table accounting) has not arrived yet: cannot serve.
        return Some(if receiving {
            recv_reply(req.req_id, -(minix_types::EAGAIN), 0, 0)
        } else {
            simple_reply(req.req_id, -(minix_types::EAGAIN))
        });
    };
    if receiving {
        // Empty table: the minimal model has no announcement queue to wait on, so answer honestly and immediately (registered).
        if routes.is_empty() {
            return Some(recv_reply(req.req_id, -(minix_types::EAGAIN), 0, 0));
        }
        let cap = req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER);
        let mut data = alloc::vec![0u8; cap];
        let mut off = 0usize;
        for entry in routes.entries() {
            let Some(n) =
                crate::rtsock::encode_entry_frame(entry, req.user_endpt, 0, &mut data[off..])
            else {
                break;
            };
            off += n;
        }
        if off == 0 {
            // Not even one frame fits (the user buffer is below the 168-byte
            // minimum frame): refuse to truncate — truncation would break frame
            // boundaries.
            return Some(recv_reply(req.req_id, -(minix_types::EMSGSIZE), 0, 0));
        }
        return match copy.safecopy_to(req.user_endpt, req.data_grant, 0, &data[..off]) {
            Ok(()) => Some(recv_reply(req.req_id, off as i32, 0, 0)),
            Err(code) => Some(recv_reply(req.req_id, wire(code), 0, 0)),
        };
    }
    // Write half: the message limit is 512 (`rtsock_pre_send`, rtsock.c:634-651).
    if req.data_len > crate::rtsock::SEND_BUFFER_MAX {
        return Some(simple_reply(req.req_id, -(minix_types::EMSGSIZE)));
    }
    let mut data = alloc::vec![0u8; req.data_len];
    if copy
        .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
        .is_err()
    {
        return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
    }
    let frame = match crate::rtsock::parse_frame(&data) {
        Ok(frame) => frame,
        Err(e) => return Some(simple_reply(req.req_id, wire(e))),
    };
    use crate::rtsock::{RTM_ADD, RTM_CHANGE, RTM_DELETE, RTM_GET};
    if matches!(frame.msg_type, t if t == RTM_ADD || t == RTM_DELETE || t == RTM_CHANGE)
        && !identity.is_root(caller)
    {
        return Some(simple_reply(req.req_id, -(minix_types::EPERM)));
    }
    let applied = match frame.msg_type {
        RTM_GET => Ok(()),
        t if t == RTM_ADD || t == RTM_CHANGE || t == RTM_DELETE => {
            let Some(dst) = frame.dst else {
                return Some(simple_reply(req.req_id, -(minix_types::EINVAL)));
            };
            let Some((version, dest)) = route_address(dst) else {
                return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
            };
            // No prefix bits present = a host entry (same semantics as C).
            let prefix = frame.prefix.unwrap_or(match version {
                crate::route::IpVersion::V4 => 32,
                crate::route::IpVersion::V6 => 128,
            });
            if t == RTM_DELETE {
                if routes.remove(version, dest, prefix) {
                    Ok(())
                } else {
                    Err(minix_types::ESRCH)
                }
            } else {
                // ADD and CHANGE both go through "insert or replace on the same key" (the table's change half).
                routes.add(crate::route::RouteEntry {
                    version,
                    dest,
                    prefix,
                    gateway: frame.gateway.map(|g| g.addr),
                    ifdev: frame.index,
                })
            }
        }
        _ => return Some(simple_reply(req.req_id, -(minix_types::EOPNOTSUPP))),
    };
    Some(match applied {
        Ok(()) => simple_reply(req.req_id, req.data_len as i32),
        Err(e) => simple_reply(req.req_id, wire(e)),
    })
}

/// Message path of the LNK domain: no data plane (chapter 11). C's lnksock
/// operation table has only `sop_ioctl`/`sop_free` (lnksock.c:75-76), and the
/// framework folds the NULL read/write hooks to EOPNOTSUPP
/// (sockdriver.c:745-746) — answer with the same error (registered; close was
/// closed out).
pub fn lnk_road(msg: &Message) -> Option<Message> {
    let Some(req) = decode_sendrecv(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    if msg.m_type == SdevRequest::Receive as i32 {
        Some(recv_reply(req.req_id, -(minix_types::EOPNOTSUPP), 0, 0))
    } else {
        Some(simple_reply(req.req_id, -(minix_types::EOPNOTSUPP)))
    }
}

/// Unified dispatch: hand the request to the UDP path or the TCP path according
/// to the wire socket's class. An unknown class (the message plane of the
/// route/link domains comes in a later batch) answers as not wired.
#[allow(clippy::too_many_arguments)]
pub fn translate(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    // sock_id sits at @4 in all three payload shapes; socket creation
    // (Socket/SocketPair) carries no sock_id and does not go through this
    // dispatch.
    let raw = unsafe { &msg.m_u.raw };
    let sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
    match SockId::from_raw(sock_id)
        .and_then(|id| id.class())
        .and_then(family_of_class)
    {
        Some(StackFamily::Udp) => translate_udp(stack, copy, table, pending, caller, msg),
        Some(StackFamily::Tcp) => translate_tcp(stack, copy, pending, caller, msg),
        Some(StackFamily::Raw) => translate_raw(stack, copy, msg),
        _ => Some(simple_reply(0, -(minix_types::ENOSYS))),
    }
}

/// Translation entry for the TCP path: BIND/LISTEN/CONNECT/ACCEPT/GETPEERNAME,
/// five operations. The suspendable semantics of connect and accept land here:
/// non-blocking replies EINPROGRESS/EWOULDBLOCK immediately, blocking suspends
/// with bookkeeping and is answered later by [`ready_scan`] once woken
/// (connect waits for establishment = writable; accept waits for a connection
/// = listener readable).
pub fn translate_tcp(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let m_type = msg.m_type;
    if m_type == SdevRequest::Bind as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let mut bytes = [0u8; INET_ADDR_LEN];
        let n = (req.len as usize).min(INET_ADDR_LEN);
        if copy
            .safecopy_from(req.user_endpt, req.grant, 0, &mut bytes[..n])
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        let Some(endpoint) = decode_inet_endpoint(&bytes) else {
            return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
        };
        return Some(match stack.bind_tcp(stack_socket, Some(endpoint)) {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        });
    }
    if m_type == SdevRequest::Listen as i32 {
        let Some(req) = decode_simple(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let backlog = req.param.clamp(0, i32::from(u16::MAX)) as u32;
        return Some(match stack.listen_tcp(stack_socket, backlog) {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        });
    }
    if m_type == SdevRequest::Connect as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let mut bytes = [0u8; INET_ADDR_LEN];
        let n = (req.len as usize).min(INET_ADDR_LEN);
        if copy
            .safecopy_from(req.user_endpt, req.grant, 0, &mut bytes[..n])
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        let Some(endpoint) = decode_inet_endpoint(&bytes) else {
            return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
        };
        return match stack.connect_tcp(stack_socket, endpoint) {
            Ok(()) => {
                if req.sflags & minix_sockdriver::sdev::SDEV_NONBLOCK != 0 {
                    // Non-blocking: reply "in progress" right away; the readiness bit reports completion.
                    Some(simple_reply(req.req_id, -(minix_types::EINPROGRESS)))
                } else {
                    // Blocking: suspend waiting for establishment (SEV_CONNECT
                    // wakes it; no timeout, options come in a later batch).
                    pending.connects.push(PendingConnect {
                        sock_id: req.sock_id,
                        caller,
                        req_id: req.req_id,
                    });
                    None
                }
            }
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        };
    }
    if m_type == SdevRequest::Accept as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(listener) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        return match stack.accept_tcp(listener) {
            Ok((accepted, peer)) => {
                let bytes = encode_inet_endpoint(peer);
                let len = (req.len as usize).min(INET_ADDR_LEN);
                if copy
                    .safecopy_to(req.user_endpt, req.grant, 0, &bytes[..len])
                    .is_err()
                {
                    return Some(accept_reply(
                        req.req_id,
                        0,
                        -(minix_types::EFAULT),
                        0,
                    ));
                }
                Some(accept_reply(req.req_id, accepted.index() as i32, 0, len as u32))
            }
            Err(_) => {
                if req.sflags & minix_sockdriver::sdev::SDEV_NONBLOCK != 0 {
                    return Some(accept_reply(
                        req.req_id,
                        0,
                        crate::util::ERR_WOULD_BLOCK,
                        0,
                    ));
                }
                pending.accepts.push(PendingAccept {
                    listener_id: req.sock_id,
                    caller,
                    req_id: req.req_id,
                    addr_grant: req.grant,
                    addr_len: req.len as usize,
                    user_endpt: req.user_endpt,
                });
                None
            }
        };
    }
    if m_type == SdevRequest::Send as i32 || m_type == SdevRequest::Receive as i32 {
        let Some(req) = decode_sendrecv(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        // Non-block decision: either the request flag or the FIONBIO flag present means reply immediately.
        let nonblock = req.flags & minix_sockdriver::sdev::MSG_DONTWAIT != 0
            || stack.is_nonblock(stack_socket);
        let _ = req.ctl_grant;
        let _ = req.ctl_len;
        let _ = req.addr_grant;
        let _ = req.addr_len;
        if m_type == SdevRequest::Send as i32 {
            // Stream send: reply as many bytes as were taken (partial sends are
            // legal); suspend when the buffer is full and blocking, reply EAGAIN
            // when non-blocking.
            let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
            if copy
                .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
                .is_err()
            {
                return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
            }
            return match stack.send_tcp(stack_socket, &data) {
                Ok(n) if n > 0 => Some(simple_reply(req.req_id, n as i32)),
                Ok(_) => {
                    if nonblock {
                        Some(simple_reply(req.req_id, -(minix_types::EAGAIN)))
                    } else {
                        pending.tcp_sends.push(PendingTcpSend {
                            sock_id: req.sock_id,
                            caller,
                            req_id: req.req_id,
                            data_grant: req.data_grant,
                            data_len: req.data_len,
                            user_endpt: req.user_endpt,
                        });
                        None
                    }
                }
                Err(e) => Some(simple_reply(req.req_id, wire(e))),
            };
        }
        // Stream receive: copy data to the user when present; reply 0 on EOF
        // (peer closed and drained); suspend when empty and blocking, reply
        // EAGAIN when non-blocking; an unopened connection answers with an error.
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        let outcome = stack.recv_tcp(stack_socket, &mut data);
        return match outcome {
            // EOF and real errors first (the stack folds "empty" into a blocking-class error, which belongs to the suspend half).
            Ok((0, true)) => Some(recv_reply(req.req_id, 0, 0, 0)),
            Err(e) if e != crate::util::ERR_WOULD_BLOCK => {
                Some(recv_reply(req.req_id, wire(e), 0, 0))
            }
            // Empty (blocking-class error or an empty read): non-blocking replies EAGAIN, blocking suspends with bookkeeping.
            _ => {
                if nonblock {
                    return Some(recv_reply(req.req_id, -(minix_types::EAGAIN), 0, 0));
                }
                pending.tcp_recvs.push(PendingTcpRecv {
                    sock_id: req.sock_id,
                    caller,
                    req_id: req.req_id,
                    data_grant: req.data_grant,
                    data_len: req.data_len,
                    user_endpt: req.user_endpt,
                });
                None
            }
        }
    }
    if m_type == SdevRequest::GetPeerName as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        return match stack.remote_endpoint_tcp(stack_socket) {
            Ok(endpoint) => {
                let bytes = encode_inet_endpoint(endpoint);
                let len = (req.len as usize).min(INET_ADDR_LEN);
                if copy
                    .safecopy_to(req.user_endpt, req.grant, 0, &bytes[..len])
                    .is_err()
                {
                    return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
                }
                Some(simple_reply(req.req_id, len as i32))
            }
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        };
    }
    Some(simple_reply(0, -(minix_types::ENOSYS)))
}

/// Translation entry for the RAW path: Send (write a full packet) and Receive
/// (read a full packet). The data plane moves bytes directly between the grant
/// and the stack buffers — full-packet semantics (including the IP header,
/// HDRINCL).
pub fn translate_raw(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_sendrecv(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let Some(stack_socket) = stack_socket_of(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    let sending = msg.m_type == SdevRequest::Send as i32;
    if sending {
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        if copy
            .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        return match stack.send_raw(stack_socket, &data) {
            Ok(n) => Some(simple_reply(req.req_id, n as i32)),
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        };
    }
    // Receive: copy to the user.
    let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
    match stack.recv_raw(stack_socket, &mut data) {
        Ok(n) => {
            if copy
                .safecopy_to(req.user_endpt, req.data_grant, 0, &data[..n])
                .is_err()
            {
                return Some(recv_reply(req.req_id, -(minix_types::EFAULT), 0, 0));
            }
            Some(recv_reply(req.req_id, n as i32, 0, 0))
        }
        Err(e) => Some(recv_reply(req.req_id, wire(e), 0, 0)),
    }
}

/// Readiness scan (the "event wakeup continuation" half of C
/// `sockevent_process`): after every advancing pass, scan the receive sticky
/// notes — one whose stack reports readable is completed (copy data and peer
/// address to the user, reply `SDEV_RECV_REPLY`), returning `(caller, reply)`
/// pairs for the loop tail to send; notes not ready yet stay in `pending`
/// unchanged. The timeout path (`poll_timers`' `TimedOut`) is handled by the
/// caller in the same place and removes the note from `pending`.
pub fn ready_scan(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
) -> Vec<(Endpoint, Message)> {
    let mut replies = Vec::new();
    let mut i = 0;
    while i < pending.recvs.len() {
        let note = pending.recvs[i];
        let readable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !readable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.recvs.remove(i);
            continue;
        };
        let mut data = alloc::vec![0u8; note.data_len];
        match stack.recv_udp(stack_socket, &mut data) {
            Ok((n, peer)) => {
                // Copy both the data and the peer address to the user; if
                // either copy fails, answer with an error (dropping data under
                // a success code would be a "fake success" — forbidden).
                let addr_bytes = encode_inet_endpoint(peer);
                let addr_len = note.addr_len.min(INET_ADDR_LEN);
                let data_ok =
                    copy.safecopy_to(note.user_endpt, note.data_grant, 0, &data[..n]).is_ok();
                let addr_ok = copy
                    .safecopy_to(note.user_endpt, note.addr_grant, 0, &addr_bytes[..addr_len])
                    .is_ok();
                let status = if data_ok && addr_ok {
                    n as i32
                } else {
                    -(minix_types::EFAULT)
                };
                replies.push((note.caller, recv_reply(note.req_id, status, 0, addr_len as u32)));
                pending.recvs.remove(i);
            }
            Err(_) => {
                // Readiness flipped but the receive fails (peer closed and the like): answer as a connection reset.
                replies.push((
                    note.caller,
                    recv_reply(note.req_id, -(minix_types::ECONNRESET), 0, 0),
                ));
                pending.recvs.remove(i);
            }
        }
    }
    // TCP connect: established (writable) replies 0.
    let mut i = 0;
    while i < pending.connects.len() {
        let note = pending.connects[i];
        let writable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.writable)
            .unwrap_or(false);
        if writable {
            replies.push((note.caller, simple_reply(note.req_id, 0)));
            pending.connects.remove(i);
        } else {
            i += 1;
        }
    }
    // TCP send: once writable, copy the data again and retry (a partial send replies the amount actually queued).
    let mut i = 0;
    while i < pending.tcp_sends.len() {
        let note = pending.tcp_sends[i];
        let writable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.writable)
            .unwrap_or(false);
        if !writable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.tcp_sends.remove(i);
            continue;
        };
        let mut data = alloc::vec![0u8; note.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        if copy
            .safecopy_from(note.user_endpt, note.data_grant, 0, &mut data)
            .is_err()
        {
            replies.push((note.caller, simple_reply(note.req_id, -(minix_types::EFAULT))));
            pending.tcp_sends.remove(i);
            continue;
        }
        match stack.send_tcp(stack_socket, &data) {
            Ok(n) if n > 0 => {
                replies.push((note.caller, simple_reply(note.req_id, n as i32)));
                pending.tcp_sends.remove(i);
            }
            Ok(_) => i += 1,
            Err(e) => {
                replies.push((note.caller, simple_reply(note.req_id, wire(e))));
                pending.tcp_sends.remove(i);
            }
        }
    }
    // TCP receive: answer once readable; EOF replies 0 (C read's end-of-file semantics).
    let mut i = 0;
    while i < pending.tcp_recvs.len() {
        let note = pending.tcp_recvs[i];
        let readable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !readable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.tcp_recvs.remove(i);
            continue;
        };
        let mut data = alloc::vec![0u8; note.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        match stack.recv_tcp(stack_socket, &mut data) {
            Ok((n, _eof)) if n > 0 => {
                if copy
                    .safecopy_to(note.user_endpt, note.data_grant, 0, &data[..n])
                    .is_err()
                {
                    replies.push((
                        note.caller,
                        recv_reply(note.req_id, -(minix_types::EFAULT), 0, 0),
                    ));
                } else {
                    replies.push((note.caller, recv_reply(note.req_id, n as i32, 0, 0)));
                }
                pending.tcp_recvs.remove(i);
            }
            Ok((0, true)) => {
                replies.push((note.caller, recv_reply(note.req_id, 0, 0, 0)));
                pending.tcp_recvs.remove(i);
            }
            _ => i += 1,
        }
    }
    // TCP accept: once the listener is readable, accept and reply the new socket id and peer address.
    let mut i = 0;
    while i < pending.accepts.len() {
        let note = pending.accepts[i];
        let pending_connection = stack_socket_of(note.listener_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !pending_connection {
            i += 1;
            continue;
        }
        let Some(listener) = stack_socket_of(note.listener_id) else {
            pending.accepts.remove(i);
            continue;
        };
        match stack.accept_tcp(listener) {
            Ok((accepted, peer)) => {
                let bytes = encode_inet_endpoint(peer);
                let len = note.addr_len.min(INET_ADDR_LEN);
                let copy_ok = copy
                    .safecopy_to(note.user_endpt, note.addr_grant, 0, &bytes[..len])
                    .is_ok();
                let status = if copy_ok { 0 } else { -(minix_types::EFAULT) };
                // Register the accepted socket in the service table (wire id =
                // TCP class base + in-stack index); a failed registration is
                // answered as "no free slot".
                let registered =
                    SockId::from_class(sockid::SockClass::Tcp, accepted.index() as u32);
                match registered {
                    Some(id) if table.add(id).is_ok() => {
                        replies.push((
                            note.caller,
                            accept_reply(note.req_id, id.raw(), status, len as u32),
                        ));
                        pending.accepts.remove(i);
                    }
                    _ => {
                        //[SD19-gate] accept orphan cleanup; a failed close has no face
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = stack.close(accepted);
                        replies.push((
                            note.caller,
                            accept_reply(
                                note.req_id,
                                0,
                                -(minix_types::ENOSPC),
                                0,
                            ),
                        ));
                        pending.accepts.remove(i);
                    }
                }
            }
            Err(_) => {
                // Readable but the accept is not complete (half-way handshake): keep the note and retry next round.
                i += 1;
            }
        }
    }
    // select waits: reply type one and drop the note as soon as the ready bits
    // intersect the bits of interest (one-shot trigger: after the reply VFS
    // decides by itself whether to re-arm; a peer close goes to the ERR face
    // through the error bit).
    let mut i = 0;
    while i < pending.selects.len() {
        let note = pending.selects[i];
        let ready = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| matched_ops(r, note.ops))
            .unwrap_or(0);
        if ready == 0 {
            i += 1;
            continue;
        }
        let Some(id) = SockId::from_raw(note.sock_id) else {
            pending.selects.remove(i);
            continue;
        };
        if table.take_select(id).is_some() {
            replies.push((note.vfs, select1_reply(note.sock_id, ready)));
        }
        pending.selects.remove(i);
    }
    replies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::SmoltcpStack;

    /// A ready-made stack plus an empty table: the minimal scene for socket creation.
    fn fixture() -> (SmoltcpStack, SockTable) {
        (SmoltcpStack::new(1, 0), SockTable::new())
    }

    #[test]
    fn test_open_stream_gets_tcp_class_id() {
        let (mut stack, mut table) = fixture();
        let req = SocketRequest {
            req_id: 7,
            domain: domain::INET,
            sock_type: sock_type::STREAM,
            protocol: 0,
            user_endpt: 100,
        };
        let id = open_socket(&mut stack, &mut table, &req, false).unwrap();
        assert_eq!(sockid::SockId::from_raw(id).unwrap().class_base(), sockid::SOCKID_TCP);
        assert!(table.contains(sockid::SockId::from_raw(id).unwrap()));
    }

    #[test]
    fn test_open_dgram_and_raw_distinct_classes() {
        let (mut stack, mut table) = fixture();
        let dgram = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        let raw_req = SocketRequest {
            req_id: 2, domain: domain::INET6, sock_type: sock_type::RAW,
            protocol: 0, user_endpt: 0, // root identity
        };
        let udp_id = open_socket(&mut stack, &mut table, &dgram, false).unwrap();
        let raw_id = open_socket(&mut stack, &mut table, &raw_req, true).unwrap();
        assert_eq!(
            sockid::SockId::from_raw(udp_id).unwrap().class_base(),
            sockid::SOCKID_UDP
        );
        assert_eq!(
            sockid::SockId::from_raw(raw_id).unwrap().class_base(),
            sockid::SOCKID_RAW
        );
    }

    #[test]
    fn test_raw_needs_root_and_bad_type_rejected() {
        let (mut stack, mut table) = fixture();
        let raw_req = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::RAW,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &raw_req, false).unwrap_err(),
            minix_types::EACCES,
            "非根开 RAW 被拒（lwip.c:166-167）"
        );
        let bogus = SocketRequest {
            req_id: 2, domain: domain::INET, sock_type: 7,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &bogus, true).unwrap_err(),
            minix_types::EPROTOTYPE,
            "未知类型按 EPROTOTYPE（lwip.c:170）"
        );
    }

    #[test]
    fn test_route_link_domains_answer_enosys_and_unknown_domain_eafnosupport() {
        let (mut stack, mut table) = fixture();
        let route = SocketRequest {
            req_id: 1, domain: domain::ROUTE, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        // RT/LNK service-side creation (real now): does not enter the stack, and each class keeps its own id.
        let route_id = open_socket(&mut stack, &mut table, &route, false).unwrap();
        assert_eq!(
            sockid::SockId::from_raw(route_id).unwrap().class_base(),
            sockid::SOCKID_RT,
            "路由套接字是服务侧对象，不进栈"
        );
        let alien = SocketRequest {
            req_id: 2, domain: 42, sock_type: sock_type::STREAM,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &alien, false).unwrap_err(),
            minix_types::EAFNOSUPPORT
        );
    }

    #[test]
    fn test_decode_and_reply_roundtrip() {
        let mut msg = Message::default();
        // SAFETY(test): fill in the field order of mess_vfs_lsockdriver_socket.
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&55i32.to_le_bytes());
            raw[4..8].copy_from_slice(&domain::INET.to_le_bytes());
            raw[8..12].copy_from_slice(&sock_type::STREAM.to_le_bytes());
            raw[12..16].copy_from_slice(&0i32.to_le_bytes());
            raw[16..20].copy_from_slice(&100i32.to_le_bytes());
        }
        let req = decode_socket(&msg).unwrap();
        assert_eq!(req.req_id, 55);
        assert_eq!(req.domain, domain::INET);
        assert_eq!(req.user_endpt, 100);
        // Reply shape: req_id comes back verbatim, sock_id2 is always -1.
        let reply = socket_reply(55, sockid::SOCKID_TCP + 3);
        // SAFETY(test): read in the field order of mess_lsockdriver_vfs_socket_reply.
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(reply.m_type, SdevReply::SocketReply as i32);
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 55);
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), sockid::SOCKID_TCP + 3);
        assert_eq!(i32::from_le_bytes(raw[8..12].try_into().unwrap()), -1);
    }

    #[test]
    fn test_simple_reply_carries_status_in_second_word() {
        let reply = simple_reply(9, -(minix_types::ENOSYS));
        assert_eq!(reply.m_type, SdevReply::Reply as i32);
        // SAFETY(test): { req_id@0; status@4 }.
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), -(minix_types::ENOSYS));
    }

    /// Builder helper for getset requests (ipc.h:2270-2281 field order).
    fn getset_msg(set: bool, req_id: i32, sock_id: i32, level: i32, name: i32, grant: i32, len: u32) -> Message {
        let mut msg = Message::default();
        msg.m_type = if set {
            SdevRequest::SetSockOpt as i32
        } else {
            SdevRequest::GetSockOpt as i32
        };
        // SAFETY(test): fill in the field order of mess_vfs_lsockdriver_getset.
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&req_id.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&level.to_le_bytes());
            raw[12..16].copy_from_slice(&name.to_le_bytes());
            raw[16..20].copy_from_slice(&grant.to_le_bytes());
            raw[20..24].copy_from_slice(&(len as i32).to_le_bytes());
        }
        msg
    }

    #[test]
    fn test_sockopt_road_tcp_roundtrip_and_honest_refusals() {
        use crate::lwip_port::{SO_KEEPALIVE, SO_SNDBUF, SOL_SOCKET};
        let (mut stack, mut _table) = fixture();
        let req = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::STREAM,
            protocol: 0, user_endpt: 100,
        };
        let id = open_socket(&mut stack, &mut _table, &req, false).unwrap();
        let mut copy = CannedCopyTransport::default();
        let caller = Endpoint(100);

        // Set: the value is copied in through the grant (exactly 4 bytes long), success replies 0.
        copy.from.push((7, 1i32.to_le_bytes().to_vec()));
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(true, 5, id, SOL_SOCKET, SO_KEEPALIVE, 7, 4),
        )
        .unwrap();
        // SAFETY(test): { req_id@0; status@4 }.
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(reply.m_type, SdevReply::Reply as i32);
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 5);
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), 0, "设置成功");

        // Get: copy the current value back to the grant (write ledger); the success code is the copied length, 4.
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(false, 6, id, SOL_SOCKET, SO_KEEPALIVE, 9, 4),
        )
        .unwrap();
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), 4, "回 4 字节");
        assert!(
            copy.written
                .iter()
                .any(|(g, b)| *g == 9 && b.as_slice() == [1, 0, 0, 0]),
            "使能值已拷进用户缓冲"
        );

        // Illegal name and illegal level: ENOPROTOOPT (an honest refusal).
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(true, 8, id, SOL_SOCKET, 0x2000, 7, 4),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOPROTOOPT),
            "SOL_SOCKET 未知名拒绝"
        );
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(true, 9, id, 6 /* IPPROTO_TCP */, 0x2000, 7, 4),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOPROTOOPT),
            "协议级选项未接（登记差异）"
        );

        // Set length other than 4: EINVAL (C copyin_opt's exact-length contract).
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(true, 10, id, SOL_SOCKET, SO_SNDBUF, 7, 8),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EINVAL)
        );
    }

    #[test]
    fn test_sockopt_road_service_classes_and_bad_fd() {
        use crate::lwip_port::{SO_KEEPALIVE, SOL_SOCKET};
        let (mut stack, mut table) = fixture();
        let mut copy = CannedCopyTransport::default();
        let caller = Endpoint(100);

        // RT class (a service-side object): no option surface → ENOPROTOOPT.
        let rt_id = open_service_socket(&mut table, sockid::SockClass::Rt).unwrap();
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(true, 1, rt_id, SOL_SOCKET, SO_KEEPALIVE, 1, 4),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOPROTOOPT),
            "RT 类无选项面"
        );

        // A negative id (the reserved area of the wire error channel): EBADF.
        // A well-formed non-negative id that lands on a closed slot takes the
        // wall's generic error (-204), consistent with the other paths.
        let ghost = -1i32;
        let reply = sockopt_road(
            &mut stack,
            &mut copy,
            caller,
            &getset_msg(false, 2, ghost, SOL_SOCKET, SO_KEEPALIVE, 1, 4),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EBADF)
        );
    }

    /// Builder helper for ioctl requests (ipc.h:2283-2294 field order; request is 8 bytes).
    fn ioctl_msg(req_id: i32, sock_id: i32, request: u64, grant: i32) -> Message {
        let mut msg = Message::default();
        msg.m_type = SdevRequest::Ioctl as i32;
        // SAFETY(test): fill in the field order of mess_vfs_lsockdriver_ioctl.
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&req_id.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..16].copy_from_slice(&request.to_le_bytes());
            raw[16..20].copy_from_slice(&grant.to_le_bytes());
        }
        msg
    }

    #[test]
    fn test_ioctl_road_fionbio_sets_flag_and_unknown_enotty() {
        use minix_sockdriver::sdev::MSG_DONTWAIT;
        let (mut stack, mut table) = fixture();
        let mut copy = CannedCopyTransport::default();
        let caller = Endpoint(100);
        let req = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        let id = open_socket(&mut stack, &mut table, &req, false).unwrap();

        // Set FIONBIO: the value 1 is copied in through the grant, success replies 0.
        copy.from.push((3, 1i32.to_le_bytes().to_vec()));
        let reply = ioctl_road(
            &mut stack,
            &mut copy,
            caller,
            &ioctl_msg(2, id, FIONBIO, 3),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "置非阻塞成功"
        );

        // Flag consumption: an empty receive on a non-blocking UDP socket replies EAGAIN at once (no suspension, no sticky note).
        let mut recv = Message::default();
        recv.m_type = SdevRequest::Receive as i32;
        // SAFETY(test): sendrecv field order (data@8/16, flags@44=0).
        unsafe {
            let raw = &mut recv.m_u.raw;
            raw[0..4].copy_from_slice(&4i32.to_le_bytes());
            raw[4..8].copy_from_slice(&id.to_le_bytes());
            raw[8..12].copy_from_slice(&5i32.to_le_bytes());
            raw[16..24].copy_from_slice(&64usize.to_le_bytes());
        }
        let reply = translate_udp(&mut stack, &mut copy, &mut table, &mut PendingTables::default(), caller, &recv).unwrap();
        // SAFETY(test): recv_reply { req_id@0; status@4 }.
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EAGAIN),
            "FIONBIO 置位后空收立即回（旗标被收发路消费）"
        );
        // Control: the equivalent MSG_DONTWAIT flag also replies immediately
        // (the two lanes merge correctly). Clear the flag first, then verify
        // the request-flag lane.
        // SAFETY(test): flags@44.
        unsafe {
            recv.m_u.raw[44..48].copy_from_slice(&(MSG_DONTWAIT as i32).to_le_bytes());
        }
        stack
            .set_nonblock(stack_socket_of(id).unwrap(), false)
            .unwrap();
        let reply = translate_udp(&mut stack, &mut copy, &mut table, &mut PendingTables::default(), caller, &recv).unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EAGAIN),
            "MSG_DONTWAIT 车道同样立即回"
        );

        // Unknown command: ENOTTY.
        let reply = ioctl_road(
            &mut stack,
            &mut copy,
            caller,
            &ioctl_msg(6, id, 0x2000_7466, 3),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOTTY),
            "未知命令按 C 服务侧 ENOTTY 出口"
        );
    }

    #[test]
    fn test_ioctl_road_fionread_udp_and_service_classes() {
        use crate::lwip_port::{StackEndpoint, StackIpAddr};
        let (mut stack, mut table) = fixture();
        let req = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        let id = open_socket(&mut stack, &mut table, &req, false).unwrap();
        let mut copy = CannedCopyTransport::default();
        let caller = Endpoint(100);

        // FIONREAD on an empty ring: 0. The value is copied out through the grant and the reply is 0 (the value travels in the buffer).
        let reply = ioctl_road(
            &mut stack,
            &mut copy,
            caller,
            &ioctl_msg(2, id, FIONREAD, 7),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "FIONREAD 回复码是成功，不是字节数"
        );
        let (_, written) = copy.written.last().expect("值已拷出");
        assert_eq!(i32::from_le_bytes(written[..4].try_into().unwrap()), 0);

        // With a packet queued: peek the head packet length (no consumption). Feed one packet on the loopback device.
        let mut loop_stack = crate::stack::SmoltcpStack::<crate::stack::LoopDevice>::with_device(
            0x5EED,
            crate::stack::LoopDevice::new(),
            0,
        );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        let lreq = SocketRequest {
            req_id: 3, domain: domain::INET, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        // A new stack gets a fresh table: slot indices restart from 0 and are independent of the previous half's table.
        let mut ltable = SockTable::new();
        let lid = open_socket(&mut loop_stack, &mut ltable, &lreq, false).unwrap();
        let sock = stack_socket_of(lid).unwrap();
        loop_stack
            .bind_udp(
                sock,
                Some(StackEndpoint { addr: Some(StackIpAddr::V4([127, 0, 0, 1])), port: 7777 }),
            )
            .unwrap();
        loop_stack
            .send_udp(
                sock,
                &[1, 2, 3, 4, 5],
                Some(StackEndpoint { addr: Some(StackIpAddr::V4([127, 0, 0, 1])), port: 7777 }),
            )
            .unwrap();
        for tick in 1..=8u64 {
            loop_stack.poll(tick * 100);
        }
        let mut lcopy = CannedCopyTransport::default();
        let reply = ioctl_road(
            &mut loop_stack,
            &mut lcopy,
            caller,
            &ioctl_msg(4, lid, FIONREAD, 7),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0
        );
        let (_, written) = lcopy.written.last().expect("值已拷出");
        assert_eq!(
            i32::from_le_bytes(written[..4].try_into().unwrap()),
            5,
            "队首包载荷长度（peek 不消费，C pktsock_test_recv 同语义）"
        );
        assert_eq!(loop_stack.pending_recv_udp(sock), Some(5), "包仍在环上");

        // RT class: ENOTTY (C rtsock has no sop_ioctl).
        let rt_id = open_service_socket(&mut table, sockid::SockClass::Rt).unwrap();
        let reply = ioctl_road(
            &mut stack,
            &mut copy,
            caller,
            &ioctl_msg(5, rt_id, FIONREAD, 7),
        )
        .unwrap();
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOTTY)
        );
    }
}
