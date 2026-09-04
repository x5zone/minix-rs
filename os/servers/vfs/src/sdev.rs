//! `sdev` — socket-driver dialogue: short/long asks, suspend records, revival.
//!
//! Corresponds to Minix3's `sdev.c:1-1114` (`sdev_sendrec`, `sdev_suspend`,
//! `sdev_socket`, `sdev_bindconn`/`sdev_bind`/`sdev_connect`, `sdev_simple`,
//! `sdev_listen`, `sdev_accept`, `sdev_readwrite`, `sdev_ioctl`,
//! `sdev_setsockopt`, `sdev_get`/`sdev_getsockopt`/`sdev_getsockname`/
//! `sdev_getpeername`, `sdev_shutdown`, `sdev_close`, `sdev_select`,
//! `sdev_finish_accept`, `sdev_finish`, `sdev_stop`, `sdev_cancel`,
//! `sdev_reply`).
//!
//! Design decisions (see 22-sdev.md §3):
//! - `AskKind` types short/long/fire-and-forget waits per operation
//! - `SockChannel` trait scripts the driver dialogue (test doubles)
//! - `suspend_aux` validates suspend records, reusing 02's `SdevAux`
//! - `grant_trio` counts the three data grants; direction reuses cdev's cross
//! - `sock_flags` combines NONBLOCK/NOSIGNAL bits as pure functions
//! - `finish_kind` routes revival into simple/recv/accept groups
//! - `route_reply` types who receives each driver reply
//!
//! Scope note: `SdevCall`/`SdevAux` are reused from 02-fproc-struct.md
//! (single source); `grant_dir` cross semantics mirror 21-cdev.md.
//! Transports (`asynsend3`), waiting, revival execution, and the upper
//! socket layer (24) stay outside; select replies stay with 23.

use minix_types::VirBytes;

use crate::cdev::grant_dir;
use crate::fproc::{SdevAux, SdevCall};

/// `SDEV_RQ_BASE` (`minix3/minix/include/minix/com.h:1037` = 0x1900).
pub const SDEV_RQ_BASE: u64 = 0x1900;

/// `SDEV_RS_BASE` (`com.h:1038` = 0x1980).
pub const SDEV_RS_BASE: u64 = 0x1980;

/// Request offsets (`com.h:1044-1060`): socket/pair/bind/connect/listen/
/// accept/send/recv/ioctl/set/get/getname/peername/shutdown/close/cancel/
/// select in declaration order.
pub const SDEV_SOCKET_OFF: u64 = 0;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_SOCKETPAIR_OFF: u64 = 1;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_BIND_OFF: u64 = 2;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_CONNECT_OFF: u64 = 3;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_LISTEN_OFF: u64 = 4;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_ACCEPT_OFF: u64 = 5;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_SEND_OFF: u64 = 6;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_RECV_OFF: u64 = 7;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_IOCTL_OFF: u64 = 8;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_SETSOCKOPT_OFF: u64 = 9;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_GETSOCKOPT_OFF: u64 = 10;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_GETSOCKNAME_OFF: u64 = 11;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_GETPEERNAME_OFF: u64 = 12;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_SHUTDOWN_OFF: u64 = 13;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_CLOSE_OFF: u64 = 14;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_CANCEL_OFF: u64 = 15;
/// See the [`SDEV_SOCKET_OFF`] group comment.
pub const SDEV_SELECT_OFF: u64 = 16;

/// Reply offsets (`com.h:1063-1068`): reply/socket/accept/recv/sel1/sel2.
pub const SDEV_REPLY_OFF: u64 = 0;
/// See the reply group comment.
pub const SDEV_SOCKET_REPLY_OFF: u64 = 1;
/// See the reply group comment.
pub const SDEV_ACCEPT_REPLY_OFF: u64 = 2;
/// See the reply group comment.
pub const SDEV_RECV_REPLY_OFF: u64 = 3;

/// `SDEV_NONBLOCK` (`com.h:1072`): do not suspend the I/O request.
pub const SDEV_NONBLOCK: u32 = 0x01;

/// `MSG_DONTWAIT` (`minix3/sys/sys/socket.h:515`): nonblocking message.
pub const MSG_DONTWAIT: u32 = 0x0080;
/// `MSG_NOSIGNAL` (`socket.h:518`): no SIGPIPE on EOF.
pub const MSG_NOSIGNAL: u32 = 0x0400;

/// `SHUT_RD/WR/RDWR` (`socket.h:604-606`): shutdown directions 0/1/2.
pub const SHUT_RD: i32 = 0;
/// See the [`SHUT_RD`] group comment.
pub const SHUT_WR: i32 = 1;
/// See the [`SHUT_RD`] group comment.
pub const SHUT_RDWR: i32 = 2;

/// Socket operation kinds (one per `sdev_*` entry family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdevOp {
    /// `sdev_socket`.
    Socket,
    /// `sdev_socket` pair form.
    SocketPair,
    /// `sdev_bind`.
    Bind,
    /// `sdev_connect`.
    Connect,
    /// `sdev_listen`.
    Listen,
    /// `sdev_accept`.
    Accept,
    /// `sdev_readwrite` write side.
    Send,
    /// `sdev_readwrite` read side.
    Recv,
    /// `sdev_ioctl`.
    Ioctl,
    /// `sdev_setsockopt`.
    SetSockOpt,
    /// `sdev_getsockopt/getsockname/getpeername` (shared `sdev_get`).
    GetSockOpt,
    /// See [`SdevOp::GetSockOpt`].
    GetSockName,
    /// See [`SdevOp::GetSockOpt`].
    GetPeerName,
    /// `sdev_shutdown`.
    Shutdown,
    /// `sdev_close`.
    Close,
    /// `sdev_select`.
    Select,
    /// `sdev_cancel`.
    Cancel,
}

impl SdevOp {
    /// Wire request type for this operation.
    pub fn msg_type(self) -> u64 {
        SDEV_RQ_BASE
            + match self {
                Self::Socket => SDEV_SOCKET_OFF,
                Self::SocketPair => SDEV_SOCKETPAIR_OFF,
                Self::Bind => SDEV_BIND_OFF,
                Self::Connect => SDEV_CONNECT_OFF,
                Self::Listen => SDEV_LISTEN_OFF,
                Self::Accept => SDEV_ACCEPT_OFF,
                Self::Send => SDEV_SEND_OFF,
                Self::Recv => SDEV_RECV_OFF,
                Self::Ioctl => SDEV_IOCTL_OFF,
                Self::SetSockOpt => SDEV_SETSOCKOPT_OFF,
                Self::GetSockOpt => SDEV_GETSOCKOPT_OFF,
                Self::GetSockName => SDEV_GETSOCKNAME_OFF,
                Self::GetPeerName => SDEV_GETPEERNAME_OFF,
                Self::Shutdown => SDEV_SHUTDOWN_OFF,
                Self::Close => SDEV_CLOSE_OFF,
                Self::Select => SDEV_SELECT_OFF,
                Self::Cancel => SDEV_CANCEL_OFF,
            }
    }

    /// How this operation waits (`sdev.c:8-16` long/short contract).
    pub fn ask_kind(self) -> AskKind {
        match self {
            Self::Socket
            | Self::SocketPair
            | Self::Listen
            | Self::Shutdown
            | Self::SetSockOpt
            | Self::GetSockOpt
            | Self::GetSockName
            | Self::GetPeerName => AskKind::RoundTrip,
            Self::Bind
            | Self::Connect
            | Self::Accept
            | Self::Send
            | Self::Recv
            | Self::Ioctl
            | Self::Close
            | Self::Cancel => AskKind::Suspend,
            Self::Select => AskKind::FireAndForget,
        }
    }
}

/// Waiting semantics per operation (`sdev.c:8-16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskKind {
    /// Short-lived: pin the worker thread until the reply lands.
    RoundTrip,
    /// Long-lived: suspend the process until the reply (or a signal).
    Suspend,
    /// Send-and-return: the reply arrives elsewhere (select layer).
    FireAndForget,
}

/// Expected reply shape for a round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    /// `SDEV_REPLY`: generic status.
    Reply,
    /// `SDEV_SOCKET_REPLY`: socket ids.
    SocketReply,
    /// `SDEV_ACCEPT_REPLY`: accepted socket.
    AcceptReply,
    /// `SDEV_RECV_REPLY`: data lengths.
    RecvReply,
}

/// One scripted channel event (test double language).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEvent {
    /// Round trip answered with this status.
    Answer(i32),
    /// Fire-and-forget dispatch acknowledged.
    Sent,
    /// Driver died mid-dialogue.
    Dead,
}

/// Driver dialogue channel (`asynsend3`/`sdev_sendrec` seam).
pub trait SockChannel {
    /// Short-lived round trip expecting `ReplyKind`; driver status or death.
    fn roundtrip(&mut self, expect: ReplyKind) -> Result<i32, SdevError>;
    /// Long-lived/select dispatch: sent (caller suspends or returns).
    fn dispatch(&mut self) -> Result<(), SdevError>;
}

/// Scripted channel: replays events in order (test double).
#[derive(Debug, Clone)]
pub struct ScriptedChannel {
    script: &'static [ChannelEvent],
    at: usize,
}

impl ScriptedChannel {
    /// New channel replaying `script`.
    pub fn new(script: &'static [ChannelEvent]) -> Self {
        Self { script, at: 0 }
    }

    /// Events consumed so far.
    pub fn rounds(self) -> usize {
        self.at
    }

    fn next(&mut self) -> ChannelEvent {
        let last = self.script.len().saturating_sub(1);
        let out = self.script[self.at.min(last)];
        self.at += 1;
        out
    }
}

impl SockChannel for ScriptedChannel {
    fn roundtrip(&mut self, _expect: ReplyKind) -> Result<i32, SdevError> {
        match self.next() {
            ChannelEvent::Answer(status) => Ok(status),
            ChannelEvent::Sent | ChannelEvent::Dead => Err(SdevError::Io),
        }
    }

    fn dispatch(&mut self) -> Result<(), SdevError> {
        match self.next() {
            ChannelEvent::Sent | ChannelEvent::Answer(_) => Ok(()),
            ChannelEvent::Dead => Err(SdevError::Io),
        }
    }
}

/// Silent channel: every dialogue dies (test double).
///
/// Behaves differently from [`ScriptedChannel`] (fixed fate vs
/// programmable script), satisfying the "two behaviorally different
/// impls" rule for traits.
#[derive(Debug, Default, Clone, Copy)]
pub struct SilentChannel;

impl SockChannel for SilentChannel {
    fn roundtrip(&mut self, _expect: ReplyKind) -> Result<i32, SdevError> {
        Err(SdevError::Io)
    }

    fn dispatch(&mut self) -> Result<(), SdevError> {
        Err(SdevError::Io)
    }
}

/// Validate a suspend record (`sdev_suspend:93-107`), reusing 02's
/// [`SdevAux`] instead of redefining the three shapes.
///
/// `fd == -1` means "no fd"; `buf == 0` means "no buffer".
pub fn suspend_aux(call: SdevCall, fd: i32, buf: VirBytes) -> Result<SdevAux, SdevError> {
    match call {
        SdevCall::Accept => {
            if fd != -1 && buf == VirBytes::new(0) {
                Ok(SdevAux::Fd(fd as usize))
            } else {
                Err(SdevError::Inval)
            }
        }
        SdevCall::Recvmsg => {
            if fd == -1 {
                Ok(SdevAux::Buf(buf))
            } else {
                Err(SdevError::Inval)
            }
        }
        _ => {
            if fd == -1 && buf == VirBytes::new(0) {
                Ok(SdevAux::None)
            } else {
                Err(SdevError::Inval)
            }
        }
    }
}

/// The three data grants and their directions (`sdev_readwrite:355-384`).
///
/// Element `i` is `(needed, is_read)`: data/control/address, each skipped
/// when its buffer is zero; direction reuses cdev's cross (write-in
/// grants read, read-out grants write).
pub fn grant_trio(
    need_data: bool,
    need_ctl: bool,
    need_addr: bool,
    is_read: bool,
) -> [(bool, bool); 3] {
    [
        (need_data, is_read),
        (need_ctl, is_read),
        (need_addr, is_read),
    ]
}

/// Grant access bits for one direction (delegates to the shared cross).
pub fn trio_access(is_read: bool) -> u32 {
    grant_dir(is_read)
}

/// `sdev_bindconn` sflags (`sdev.c:205-206`): nonblocking or nothing.
pub fn sdev_sflags(nonblock: bool) -> u32 {
    if nonblock { SDEV_NONBLOCK } else { 0 }
}

/// `sdev_readwrite` message flags (`sdev.c:398-402`).
pub fn sock_msg_flags(nonblock: bool, write_nosigpipe: bool) -> u32 {
    let mut flags = 0;
    if nonblock {
        flags |= MSG_DONTWAIT;
    }
    if write_nosigpipe {
        flags |= MSG_NOSIGNAL;
    }
    flags
}

/// Shutdown direction (`sdev_shutdown:595` asserts the trio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutHow {
    /// `SHUT_RD`: no more receives.
    Rd,
    /// `SHUT_WR`: no more sends.
    Wr,
    /// `SHUT_RDWR`: neither.
    RdWr,
}

impl TryFrom<i32> for ShutHow {
    type Error = SdevError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            SHUT_RD => Ok(Self::Rd),
            SHUT_WR => Ok(Self::Wr),
            SHUT_RDWR => Ok(Self::RdWr),
            _ => Err(SdevError::Inval),
        }
    }
}

/// Whether `close(2)` suspends or goes synchronous (`sdev_close:619-639`).
///
/// Only the user process calling `close(2)` suspends; exit/exec/dup2
/// closes stay thread-synchronous (SO_LINGER comment, `sdev.c:611-618`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseMode {
    /// Suspend until the driver answers.
    Suspend,
    /// Thread-synchronous close (`SDEV_NONBLOCK` flavor).
    Sync,
}

/// Pure close-mode decision.
pub fn close_mode(may_suspend: bool) -> CloseMode {
    if may_suspend {
        CloseMode::Suspend
    } else {
        CloseMode::Sync
    }
}

/// Revival group for `sdev_finish` (`sdev.c:785-892`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishGroup {
    /// `SDEV_REPLY` status-only calls (bind/connect/write/send*/ioctl/close).
    Simple,
    /// `SDEV_RECV_REPLY` calls (read/recvfrom/recvmsg).
    Recv,
    /// `SDEV_ACCEPT_REPLY` calls (accept, failed path only here).
    Accept,
    /// Anything else (C panics; hardens to `EIO`).
    Unknown,
}

/// Route one suspended call to its revival group.
pub fn finish_kind(call: SdevCall) -> FinishGroup {
    match call {
        SdevCall::Bind
        | SdevCall::Connect
        | SdevCall::Write
        | SdevCall::Sendto
        | SdevCall::Sendmsg
        | SdevCall::Ioctl
        | SdevCall::Close => FinishGroup::Simple,
        SdevCall::Read | SdevCall::Recvfrom | SdevCall::Recvmsg => FinishGroup::Recv,
        SdevCall::Accept => FinishGroup::Accept,
    }
}

/// Close-status normalization (`sdev_finish:804-806`): a closed fd reads
/// as closed unless the driver is still working on it.
pub fn close_normalize(status: i32) -> i32 {
    if status != 0 && status != minix_types::EINPROGRESS {
        0
    } else {
        status
    }
}

/// Failed-accept status gate (`sdev_finish:885-891`): negative socket id
/// with negative status is a plain failure; anything else is a protocol
/// error (`EIO`).
pub fn accept_fail_status(sock_id: i32, status: i32) -> Result<i32, SdevError> {
    if sock_id >= 0 || status >= 0 {
        return Err(SdevError::Io);
    }
    Ok(status)
}

/// Cancel follow-up (`sdev_cancel:975-979`): a successful accept reply
/// needs accept finishing even on the cancel path; everything else goes
/// through the normal finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelFollow {
    /// Hand to accept finishing.
    FinishAccept,
    /// Hand to normal finishing.
    Finish,
}

/// Pure cancel branch.
pub fn cancel_follow(is_accept_reply: bool, sock_ok: bool) -> CancelFollow {
    if is_accept_reply && sock_ok {
        CancelFollow::FinishAccept
    } else {
        CancelFollow::Finish
    }
}

/// Incoming driver-reply kinds that need a requester (`sdev_reply:1004+`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyNeed {
    /// Generic/socket/accept/recv replies carry a requester id.
    Requester,
    /// Select replies route straight to the select layer (23).
    Select,
    /// Unknown reply types are dropped with a log line.
    Unknown,
}

/// Classify one reply type by whether it needs requester lookup.
pub fn reply_need(is_select: bool, known: bool) -> ReplyNeed {
    if is_select {
        ReplyNeed::Select
    } else if known {
        ReplyNeed::Requester
    } else {
        ReplyNeed::Unknown
    }
}

/// Where one driver reply goes (`sdev_reply:1035-1113`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyRoute {
    /// Select replies: hand to 23-select.md, stop here.
    Select,
    /// A worker thread waits: hand it the message.
    WorkerDeliver,
    /// Suspended process, same driver: clear the block, run `sdev_finish`.
    BlockedFinish,
    /// Successful accept on the main thread: spawn a worker for it.
    AcceptSpawn,
    /// Successful accept already on a worker (cancel path): finish inline.
    AcceptDirect,
    /// Drop with a log line, for this reason.
    Ignore(DropReason),
}

/// Drop reasons for driver replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// No smap row names the replier (`sdev_reply:998-1002`).
    UnknownDriver,
    /// Requester endpoint unknown (`sdev_reply:1035-1039`).
    BadEndpoint,
    /// Neither worker nor matching suspension (`sdev_reply:1053-1057`).
    NotBlocked,
    /// Unknown reply type (`sdev_reply:1029-1032`).
    UnknownReply,
}

/// Pure reply routing.
///
/// - `known_driver`: a smap row names the replier.
/// - `select`: select-1/2 reply (routes to 23, needs nothing else).
/// - `reply_known`: a request-carrying reply type we handle.
/// - `endpoint_ok`: requester endpoint resolves.
/// - `worker_waiting`: a worker thread holds this driver's slot.
/// - `blocked` + `same_driver`: suspended here for this driver.
/// - `accept_ok`: accept reply with a fresh socket (`sock_id >= 0`).
/// - `at_worker`: already on a worker thread (cancel path).
#[allow(clippy::too_many_arguments)]
pub fn route_reply(
    known_driver: bool,
    select: bool,
    reply_known: bool,
    endpoint_ok: bool,
    worker_waiting: bool,
    blocked: bool,
    same_driver: bool,
    accept_ok: bool,
    at_worker: bool,
) -> ReplyRoute {
    if !known_driver {
        return ReplyRoute::Ignore(DropReason::UnknownDriver);
    }
    if select {
        return ReplyRoute::Select;
    }
    if !reply_known {
        return ReplyRoute::Ignore(DropReason::UnknownReply);
    }
    if !endpoint_ok {
        return ReplyRoute::Ignore(DropReason::BadEndpoint);
    }
    if worker_waiting {
        return ReplyRoute::WorkerDeliver;
    }
    if !blocked || !same_driver {
        return ReplyRoute::Ignore(DropReason::NotBlocked);
    }
    if accept_ok && !at_worker {
        return ReplyRoute::AcceptSpawn;
    }
    if accept_ok {
        return ReplyRoute::AcceptDirect;
    }
    ReplyRoute::BlockedFinish
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// `SUSPEND` is a verdict (08/09 own suspension), not an error; driver
/// death everywhere reports `EIO` by the single-death convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdevError {
    /// `EIO`: driver died, bad reply type, protocol violations.
    Io,
    /// `EINTR`: signal-interrupted long calls.
    Intr,
    /// `EAGAIN`: non-blocking fast failures (consumed by 24-socket.md).
    Again,
    /// `EINVAL`: bad suspend records, bad shutdown/unknown calls.
    Inval,
}

impl SdevError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Io => minix_types::EIO,
            Self::Intr => minix_types::EINTR,
            Self::Again => minix_types::EAGAIN,
            Self::Inval => minix_types::EINVAL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_kinds_cover_all_ops() {
        // Short asks pin the worker (`sdev_sendrec` users).
        for op in [
            SdevOp::Socket,
            SdevOp::SocketPair,
            SdevOp::Listen,
            SdevOp::Shutdown,
            SdevOp::SetSockOpt,
            SdevOp::GetSockOpt,
            SdevOp::GetSockName,
            SdevOp::GetPeerName,
        ] {
            assert_eq!(op.ask_kind(), AskKind::RoundTrip, "{op:?}");
        }
        // Long asks suspend the process.
        for op in [
            SdevOp::Bind,
            SdevOp::Connect,
            SdevOp::Accept,
            SdevOp::Send,
            SdevOp::Recv,
            SdevOp::Ioctl,
            SdevOp::Close,
            SdevOp::Cancel,
        ] {
            assert_eq!(op.ask_kind(), AskKind::Suspend, "{op:?}");
        }
        // Select neither pins nor suspends here.
        assert_eq!(SdevOp::Select.ask_kind(), AskKind::FireAndForget);
        // Seventeen operations, all classified (exhaustive match above).
        assert_eq!(SdevOp::Cancel.msg_type(), SDEV_RQ_BASE + 15);
        assert_eq!(SdevOp::Socket.msg_type(), SDEV_RQ_BASE);
        assert_eq!(SdevOp::Select.msg_type(), SDEV_RQ_BASE + 16);
    }

    #[test]
    fn test_channel_scripts() {
        // Scripted answers drive round trips; death converts to EIO.
        let mut ch = ScriptedChannel::new(&[ChannelEvent::Answer(0)]);
        assert_eq!(ch.roundtrip(ReplyKind::Reply).unwrap(), 0);
        assert_eq!(ch.rounds(), 1);
        let mut ch = ScriptedChannel::new(&[ChannelEvent::Dead]);
        assert_eq!(
            ch.roundtrip(ReplyKind::SocketReply).unwrap_err(),
            SdevError::Io
        );
        assert_eq!(SdevError::Io.to_errno(), minix_types::EIO);
        // Dispatches acknowledge sends.
        let mut ch = ScriptedChannel::new(&[ChannelEvent::Sent]);
        assert!(ch.dispatch().is_ok());
        let mut ch = ScriptedChannel::new(&[ChannelEvent::Dead]);
        assert!(ch.dispatch().is_err());
        // Gate D: the silent double behaves differently via one bound.
        fn via<C: SockChannel>(c: &mut C) -> bool {
            c.roundtrip(ReplyKind::Reply).is_ok()
        }
        let mut live = ScriptedChannel::new(&[ChannelEvent::Answer(0)]);
        let mut dead = SilentChannel;
        assert!(via(&mut live));
        assert!(!via(&mut dead));
        assert!(SilentChannel.dispatch().is_err());
    }

    #[test]
    fn test_suspend_aux_shapes() {
        // Accept wants an fd, recvmsg a buffer, the rest neither.
        assert_eq!(
            suspend_aux(SdevCall::Accept, 3, VirBytes::new(0)).unwrap(),
            SdevAux::Fd(3)
        );
        assert_eq!(
            suspend_aux(SdevCall::Recvmsg, -1, VirBytes::new(0x800)).unwrap(),
            SdevAux::Buf(VirBytes::new(0x800))
        );
        assert_eq!(
            suspend_aux(SdevCall::Bind, -1, VirBytes::new(0)).unwrap(),
            SdevAux::None
        );
        // Shape violations refuse (`sdev_suspend:93-107` asserts).
        assert_eq!(
            suspend_aux(SdevCall::Accept, -1, VirBytes::new(0)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(
            suspend_aux(SdevCall::Recvmsg, 3, VirBytes::new(0)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(
            suspend_aux(SdevCall::Bind, 3, VirBytes::new(0)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(
            suspend_aux(SdevCall::Bind, -1, VirBytes::new(8)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(SdevError::Inval.to_errno(), minix_types::EINVAL);
    }

    #[test]
    fn test_grant_and_flag_packing() {
        // Trio counts needs; direction reuses the cdev cross.
        assert_eq!(
            grant_trio(true, true, true, false),
            [(true, false), (true, false), (true, false)]
        );
        assert_eq!(
            grant_trio(true, false, false, true),
            [(true, true), (false, true), (false, true)]
        );
        assert_eq!(trio_access(false), crate::cdev::grant_dir(false));
        assert_eq!(trio_access(true), crate::cdev::grant_dir(true));
        // Bind sflags: nonblocking or nothing (`sdev.c:205-206`).
        assert_eq!(sdev_sflags(true), SDEV_NONBLOCK);
        assert_eq!(sdev_sflags(false), 0);
        // Message flags combine both bits (`sdev.c:398-402`).
        assert_eq!(sock_msg_flags(true, true), MSG_DONTWAIT | MSG_NOSIGNAL);
        assert_eq!(sock_msg_flags(true, false), MSG_DONTWAIT);
        assert_eq!(sock_msg_flags(false, true), MSG_NOSIGNAL);
        assert_eq!(sock_msg_flags(false, false), 0);
        // Shutdown trio decodes 0/1/2 (`socket.h:604-606`).
        assert_eq!(ShutHow::try_from(0).unwrap(), ShutHow::Rd);
        assert_eq!(ShutHow::try_from(1).unwrap(), ShutHow::Wr);
        assert_eq!(ShutHow::try_from(2).unwrap(), ShutHow::RdWr);
        assert_eq!(ShutHow::try_from(3).unwrap_err(), SdevError::Inval);
        // Close mode follows may_suspend (`sdev_close:619-639`).
        assert_eq!(close_mode(true), CloseMode::Suspend);
        assert_eq!(close_mode(false), CloseMode::Sync);
    }

    #[test]
    fn test_finish_groups() {
        // Simple group: seven calls share SDEV_REPLY handling.
        for call in [
            SdevCall::Bind,
            SdevCall::Connect,
            SdevCall::Write,
            SdevCall::Sendto,
            SdevCall::Sendmsg,
            SdevCall::Ioctl,
            SdevCall::Close,
        ] {
            assert_eq!(finish_kind(call), FinishGroup::Simple, "{call:?}");
        }
        for call in [SdevCall::Read, SdevCall::Recvfrom, SdevCall::Recvmsg] {
            assert_eq!(finish_kind(call), FinishGroup::Recv, "{call:?}");
        }
        assert_eq!(finish_kind(SdevCall::Accept), FinishGroup::Accept);
        // Close normalizes everything but OK/EINPROGRESS (`:804-806`).
        assert_eq!(close_normalize(0), 0);
        assert_eq!(
            close_normalize(minix_types::EINPROGRESS),
            minix_types::EINPROGRESS
        );
        assert_eq!(close_normalize(-5), 0);
        // Failed accepts need negative socket and status (`:885-891`).
        assert_eq!(accept_fail_status(-1, -5).unwrap(), -5);
        assert_eq!(accept_fail_status(3, 0).unwrap_err(), SdevError::Io);
        assert_eq!(accept_fail_status(-1, 0).unwrap_err(), SdevError::Io);
        // Cancel follows accept-success, else normal finish (`:975-979`).
        assert_eq!(cancel_follow(true, true), CancelFollow::FinishAccept);
        assert_eq!(cancel_follow(true, false), CancelFollow::Finish);
        assert_eq!(cancel_follow(false, true), CancelFollow::Finish);
    }

    #[test]
    fn test_route_reply_matrix() {
        use DropReason as D;
        use ReplyRoute as R;
        // Unknown drivers and replies drop first.
        assert_eq!(
            route_reply(false, false, true, true, false, true, true, false, false),
            R::Ignore(D::UnknownDriver)
        );
        // Select replies route straight to 23.
        assert_eq!(
            route_reply(true, true, true, true, false, true, true, false, false),
            R::Select
        );
        // Unknown reply types drop.
        assert_eq!(
            route_reply(true, false, false, true, false, true, true, false, false),
            R::Ignore(D::UnknownReply)
        );
        // Bad requester endpoints drop.
        assert_eq!(
            route_reply(true, false, true, false, false, true, true, false, false),
            R::Ignore(D::BadEndpoint)
        );
        // Waiting workers take the message.
        assert_eq!(
            route_reply(true, false, true, true, true, false, false, false, false),
            R::WorkerDeliver
        );
        // Unmatched suspensions drop.
        assert_eq!(
            route_reply(true, false, true, true, false, false, true, false, false),
            R::Ignore(D::NotBlocked)
        );
        assert_eq!(
            route_reply(true, false, true, true, false, true, false, false, false),
            R::Ignore(D::NotBlocked)
        );
        // Accept success spawns, unless already on a worker.
        assert_eq!(
            route_reply(true, false, true, true, false, true, true, true, false),
            R::AcceptSpawn
        );
        assert_eq!(
            route_reply(true, false, true, true, false, true, true, true, true),
            R::AcceptDirect
        );
        // Everything else finishes inline.
        assert_eq!(
            route_reply(true, false, true, true, false, true, true, false, false),
            R::BlockedFinish
        );
        // Reply-need classification for the select split.
        assert_eq!(reply_need(true, true), ReplyNeed::Select);
        assert_eq!(reply_need(false, true), ReplyNeed::Requester);
        assert_eq!(reply_need(false, false), ReplyNeed::Unknown);
    }

    #[test]
    fn test_errno_map_covers_sdev_c() {
        let cases = [
            (SdevError::Io, minix_types::EIO),
            (SdevError::Intr, minix_types::EINTR),
            (SdevError::Again, minix_types::EAGAIN),
            (SdevError::Inval, minix_types::EINVAL),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
