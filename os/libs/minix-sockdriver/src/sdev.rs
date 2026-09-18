//! `sdev` — socket-driver wire vocabulary: numbers, classification, packing,
//! error mapping, reply routing.
//!
//! Corresponds to the numbering-and-shape half of Minix3's `sdev.c:1-1114`
//! (`sdev_sendrec`, `sdev_suspend`, `sdev_socket`, `sdev_bindconn`/
//! `sdev_bind`/`sdev_connect`, `sdev_simple`, `sdev_listen`, `sdev_accept`,
//! `sdev_readwrite`, `sdev_ioctl`, `sdev_setsockopt`, `sdev_get`/
//! `sdev_getsockopt`/`sdev_getsockname`/`sdev_getpeername`, `sdev_shutdown`,
//! `sdev_close`, `sdev_select`, `sdev_finish_accept`, `sdev_finish`,
//! `sdev_stop`, `sdev_cancel`, `sdev_reply`).
//!
//! Design decisions (see 22-sdev.md §3):
//! - `SdevRequest`/`SdevReply` carry the wire values as discriminants
//! - `may_suspend` types the long/short contract (`sdev.c:8-16`)
//! - `SockChannel` trait scripts the driver dialogue (test doubles)
//! - `grant_trio` counts the three data grants
//! - `sock_flags` combines NONBLOCK/NOSIGNAL bits as pure functions
//! - `route_reply` types who receives each driver reply
//!
//! Scope note: the dialogue pieces that read caller-owned tables —
//! suspend-record validation over `SdevAux`, revival groups keyed by the
//! caller's call enum, driver-death stop plans matched against the smap —
//! stay with the consumer (VFS) because their inputs are consumer state.
//! Transports (`asynsend3`), waiting, revival execution, and the upper
//! socket layer (24) stay outside; select replies stay with 23.

use minix_types::{EAGAIN, EINPROGRESS, EINTR, EIO, EINVAL};

/// Base of the socket-device request range (`SDEV_RQ_BASE`,
/// `minix3/minix/include/minix/com.h:1037` = 0x1900).
pub const SDEV_RQ_BASE: u32 = 0x1900;

/// Base of the socket-device reply range (`SDEV_RS_BASE`,
/// `com.h:1038` = 0x1980).
pub const SDEV_RS_BASE: u32 = 0x1980;

/// Request sent by the virtual file system to a socket driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SdevRequest {
    /// Open a socket (`SDEV_SOCKET`, base plus 0).
    Socket = SDEV_RQ_BASE,
    /// Open a connected pair (`SDEV_SOCKETPAIR`, base plus 1).
    SocketPair = SDEV_RQ_BASE + 1,
    /// Bind an address (`SDEV_BIND`, base plus 2).
    Bind = SDEV_RQ_BASE + 2,
    /// Connect to a peer (`SDEV_CONNECT`, base plus 3).
    Connect = SDEV_RQ_BASE + 3,
    /// Listen for peers (`SDEV_LISTEN`, base plus 4).
    Listen = SDEV_RQ_BASE + 4,
    /// Accept a peer (`SDEV_ACCEPT`, base plus 5).
    Accept = SDEV_RQ_BASE + 5,
    /// Send data (`SDEV_SEND`, base plus 6).
    Send = SDEV_RQ_BASE + 6,
    /// Receive data (`SDEV_RECV`, base plus 7).
    Receive = SDEV_RQ_BASE + 7,
    /// Device control (`SDEV_IOCTL`, base plus 8).
    Ioctl = SDEV_RQ_BASE + 8,
    /// Set an option (`SDEV_SETSOCKOPT`, base plus 9).
    SetSockOpt = SDEV_RQ_BASE + 9,
    /// Get an option (`SDEV_GETSOCKOPT`, base plus 10).
    GetSockOpt = SDEV_RQ_BASE + 10,
    /// Get the local address (`SDEV_GETSOCKNAME`, base plus 11).
    GetSockName = SDEV_RQ_BASE + 11,
    /// Get the peer address (`SDEV_GETPEERNAME`, base plus 12).
    GetPeerName = SDEV_RQ_BASE + 12,
    /// Shut one direction down (`SDEV_SHUTDOWN`, base plus 13).
    Shutdown = SDEV_RQ_BASE + 13,
    /// Close the socket (`SDEV_CLOSE`, base plus 14).
    Close = SDEV_RQ_BASE + 14,
    /// Cancel a waiting call (`SDEV_CANCEL`, base plus 15).
    Cancel = SDEV_RQ_BASE + 15,
    /// Wait for readiness (`SDEV_SELECT`, base plus 16).
    Select = SDEV_RQ_BASE + 16,
}

/// Answer flowing back from a socket driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SdevReply {
    /// Generic answer (`SDEV_REPLY`, reply base plus 0).
    Reply = SDEV_RS_BASE,
    /// Socket created (`SDEV_SOCKET_REPLY`, plus 1).
    SocketReply = SDEV_RS_BASE + 1,
    /// Peer accepted (`SDEV_ACCEPT_REPLY`, plus 2).
    AcceptReply = SDEV_RS_BASE + 2,
    /// Data received (`SDEV_RECV_REPLY`, plus 3).
    ReceiveReply = SDEV_RS_BASE + 3,
    /// Readiness, first half (`SDEV_SELECT1_REPLY`, plus 4).
    SelectReply1 = SDEV_RS_BASE + 4,
    /// Readiness, second half (`SDEV_SELECT2_REPLY`, plus 5).
    SelectReply2 = SDEV_RS_BASE + 5,
}

/// Whether a raw message number is a socket-device request
/// (`IS_SDEV_RQ`: all but the low seven bits equal the base).
pub fn is_sdev_request(raw: u32) -> bool {
    raw & !0x7f == SDEV_RQ_BASE
}

/// Whether a raw message number is a socket-device reply
/// (`IS_SDEV_RS`).
pub fn is_sdev_reply(raw: u32) -> bool {
    raw & !0x7f == SDEV_RS_BASE
}

/// Do not suspend the I/O request: answer at once instead of waiting
/// (`SDEV_NONBLOCK`, `com.h:1072`).
pub const SDEV_NONBLOCK: u32 = 0x01;

/// Selected for read operation (`SDEV_OP_RD`, `com.h:1075`).
pub const SDEV_OP_RD: u8 = 0x01;

/// Selected for write operation (`SDEV_OP_WR`, `com.h:1076`).
pub const SDEV_OP_WR: u8 = 0x02;

/// Selected for error operation (`SDEV_OP_ERR`, `com.h:1077`; the C
/// framework never tests this bit — see `sockevent.c:817` — and neither
/// does the machinery).
pub const SDEV_OP_ERR: u8 = 0x04;

/// Notification requested for a select (`SDEV_NOTIFY`, `com.h:1078`).
pub const SDEV_NOTIFY: u8 = 0x08;

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

/// Whether a request may wait for a later event
/// (`sockdriver.c:10-26`): waiting requests need suspend and resume;
/// the rest always answer at once. Cancel carries no answer at all.
pub const fn may_suspend(request: SdevRequest) -> bool {
    match request {
        SdevRequest::Bind
        | SdevRequest::Connect
        | SdevRequest::Accept
        | SdevRequest::Send
        | SdevRequest::Receive
        | SdevRequest::Ioctl
        | SdevRequest::Close
        | SdevRequest::Select => true,
        SdevRequest::Socket
        | SdevRequest::SocketPair
        | SdevRequest::Listen
        | SdevRequest::SetSockOpt
        | SdevRequest::GetSockOpt
        | SdevRequest::GetSockName
        | SdevRequest::GetPeerName
        | SdevRequest::Shutdown
        | SdevRequest::Cancel => false,
    }
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
#[cfg(test)]
pub struct ScriptedChannel {
    script: &'static [ChannelEvent],
    at: usize,
}

#[cfg(test)]
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

#[cfg(test)]
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
#[cfg(test)]
pub struct SilentChannel;

#[cfg(test)]
impl SockChannel for SilentChannel {
    fn roundtrip(&mut self, _expect: ReplyKind) -> Result<i32, SdevError> {
        Err(SdevError::Io)
    }

    fn dispatch(&mut self) -> Result<(), SdevError> {
        Err(SdevError::Io)
    }
}

/// The three data grants and their directions (`sdev_readwrite:355-384`).
///
/// Element `i` is `(needed, is_read)`: data/control/address, each skipped
/// when its buffer is zero; direction reuses the cdev cross (write-in
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

/// Close-status normalization (`sdev_finish:804-806`): a closed fd reads
/// as closed unless the driver is still working on it.
pub fn close_normalize(status: i32) -> i32 {
    if status != 0 && status != EINPROGRESS {
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

/// Errors of the socket-driver client, each mapping to one Minix3 errno.
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

impl minix_types::ToErrno for SdevError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
}

impl SdevError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Io => EIO,
            Self::Intr => EINTR,
            Self::Again => EAGAIN,
            Self::Inval => EINVAL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bases_match_com_header() {
        assert_eq!(SDEV_RQ_BASE, 0x1900);
        assert_eq!(SDEV_RS_BASE, 0x1980);
    }

    #[test]
    fn test_seventeen_requests_numbered_in_order() {
        assert_eq!(SdevRequest::Socket as u32, 0x1900);
        assert_eq!(SdevRequest::Select as u32, 0x1910);
        assert_eq!(SdevRequest::GetPeerName as u32, 0x190C);
        assert_eq!(SdevRequest::Shutdown as u32, 0x190D);
    }

    #[test]
    fn test_six_replies_follow_their_base() {
        assert_eq!(SdevReply::Reply as u32, 0x1980);
        assert_eq!(SdevReply::SelectReply2 as u32, 0x1985);
    }

    #[test]
    fn test_flag_bits_match_com_header() {
        assert_eq!(SDEV_NONBLOCK, 0x01);
        assert_eq!(SDEV_OP_RD, 0x01);
        assert_eq!(SDEV_OP_WR, 0x02);
        assert_eq!(SDEV_OP_ERR, 0x04);
        assert_eq!(SDEV_NOTIFY, 0x08);
    }

    #[test]
    fn test_guards_accept_own_range_only() {
        assert!(is_sdev_request(0x1900));
        assert!(is_sdev_request(0x1910));
        assert!(!is_sdev_request(0x1980));
        assert!(!is_sdev_reply(0x1910));
        assert!(is_sdev_reply(0x1985));
        assert!(!is_sdev_request(0x0400));
    }

    #[test]
    fn test_suspend_table_matches_driver_comment() {
        assert!(may_suspend(SdevRequest::Connect));
        assert!(may_suspend(SdevRequest::Receive));
        assert!(may_suspend(SdevRequest::Select));
        assert!(!may_suspend(SdevRequest::Socket));
        assert!(!may_suspend(SdevRequest::Listen));
        assert!(!may_suspend(SdevRequest::Shutdown));
        assert!(!may_suspend(SdevRequest::Cancel));
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
        assert_eq!(SdevError::Io.to_errno(), EIO);
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
    fn test_grant_and_flag_packing() {
        // Trio counts needs; direction rides the shared cross.
        assert_eq!(
            grant_trio(true, true, true, false),
            [(true, false), (true, false), (true, false)]
        );
        assert_eq!(
            grant_trio(true, false, false, true),
            [(true, true), (false, true), (false, true)]
        );
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
    fn test_reply_finish_helpers() {
        // Close normalizes everything but OK/EINPROGRESS (`:804-806`).
        assert_eq!(close_normalize(0), 0);
        assert_eq!(close_normalize(EINPROGRESS), EINPROGRESS);
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
            (SdevError::Io, EIO),
            (SdevError::Intr, EINTR),
            (SdevError::Again, EAGAIN),
            (SdevError::Inval, EINVAL),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
