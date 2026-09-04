//! `socket` — upper socket layer: BSD syscall surface over the socket driver.
//!
//! Corresponds to Minix3's `socket.c:1-762` (`get_sock_flags`,
//! `check_sock_fds`, `make_sock_fd`, `do_socket`, `do_socketpair`,
//! `get_sock`, `do_bind`, `do_connect`, `do_listen`, `do_accept`,
//! `resume_accept`, `do_sendto`, `do_recvfrom`, `resume_recvfrom`,
//! `do_sockmsg`, `resume_recvmsg`, `do_setsockopt`, `do_getsockopt`,
//! `do_getsockname`, `do_getpeername`, `do_shutdown`).
//!
//! Design decisions (see 24-socket.md §3):
//! - `SockCall` types the sixteen calls; numbers stay with `call_table`
//! - `sock_flags`/`strip_sock_type` convert `SOCK_*` open bits purely
//! - `SockLookup` trait gates "is this a socket" (test doubles)
//! - `FdAllocator` trait models the nine-step landing (test doubles)
//! - `compensate` closes what each build step opened, nothing else
//! - `classify_accept` types the three resume cases plus listener loss
//! - `iov_gate` and friends type the message-envelope doors
//!
//! Scope note: driver dialogue (`sdev_*`) stays with 22-sdev.md; fd-table
//! execution (`check_fds`/`get_fd`/`close_fd`) stays with 14-filedes.md;
//! PFS landing (`req_newnode`) stays with 12-request-wrappers.md; waiting
//! (`suspend`), revival (`revive`/`reply`), and timers stay with 08/09;
//! `do_socketpath` executes in 13-path-lookup.md (`path.c:803`).
//! This module only decides: classify, convert, gate, land, compensate,
//! resume, and reply.
//!
//! Linux models the same split as `net/socket.c` (syscall surface:
//! `__sock_create`, `socket_file`, per-call `sock_sendmsg` dispatch) over
//! per-family `proto_ops` (the driver dialogue); Redox models it as a
//! `Socket` scheme with `open`/`read`/`write` handles plus option ioctls.
//! Here `SockCall` is the surface and [`SockLookup`]/[`FdAllocator`] are
//! the per-table answers.

use crate::sdev::{SHUT_RD, SHUT_RDWR, SHUT_WR};

/// `SOCK_CLOEXEC` (`minix3/sys/sys/socket.h:113`): close-on-exec at birth.
pub const SOCK_CLOEXEC: u32 = 0x1000_0000;
/// `SOCK_NONBLOCK` (`socket.h:114`): non-blocking I/O at birth.
pub const SOCK_NONBLOCK: u32 = 0x2000_0000;
/// `SOCK_NOSIGPIPE` (`socket.h:115`): no SIGPIPE at birth.
pub const SOCK_NOSIGPIPE: u32 = 0x4000_0000;
/// `SOCK_FLAGS_MASK` (`socket.h:116`): open-flag bits inside `type`.
pub const SOCK_FLAGS_MASK: u32 = 0xf000_0000;

/// `O_NONBLOCK` (`minix3/sys/sys/fcntl.h:81`).
pub const O_NONBLOCK: u32 = 0x0000_0004;
/// `O_CLOEXEC` (`fcntl.h:118`).
pub const O_CLOEXEC: u32 = 0x0040_0000;
/// `O_NOSIGPIPE` (`fcntl.h:124`).
pub const O_NOSIGPIPE: u32 = 0x0100_0000;
/// `O_RDWR` (`fcntl.h:66`): sockets always open read-write (`socket.c:158`).
pub const O_RDWR: u32 = 0x0000_0002;

/// Flags `make_sock_fd` accepts (`socket.c:94` assertion mask).
pub const SOCK_FD_FLAG_MASK: u32 = O_CLOEXEC | O_NONBLOCK | O_NOSIGPIPE;

/// Flags an accepted socket inherits (`socket.c:459`).
pub const ACCEPT_INHERIT_MASK: u32 = O_CLOEXEC | O_NONBLOCK | O_NOSIGPIPE;

/// `S_IFSOCK` (`minix3/sys/sys/stat.h:162`, octal 0140000).
pub const S_IFSOCK: u32 = 0o140000;
/// `ACCESSPERMS` (`stat.h:189`, 0777): fresh sockets are fully open.
pub const ACCESSPERMS: u32 = 0o777;
/// Fresh-socket mode (`socket.c:139`: `S_IFSOCK | ACCESSPERMS`).
pub const SOCK_NEW_MODE: u32 = S_IFSOCK | ACCESSPERMS;

/// `SSIZE_MAX` (`minix3/sys/sys/common_limits.h:55` = `LONG_MAX`, 64-bit).
pub const SSIZE_MAX_U64: u64 = i64::MAX as u64;

/// The sixteen upper-layer calls (header table, `socket.c:9-32`).
///
/// Call numbers are *not* redefined here: they stay with
/// `call_table.rs:VfsCallNum` (single source). `SocketPathRef` marks the
/// path-side call (`path.c:803`, executes in 13) so the surface is complete
/// without claiming its execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockCall {
    /// `VFS_SOCKET`: create one socket.
    Socket,
    /// `VFS_SOCKETPAIR`: create a connected pair.
    SocketPair,
    /// `VFS_BIND`: bind a local address.
    Bind,
    /// `VFS_CONNECT`: connect a remote address.
    Connect,
    /// `VFS_LISTEN`: listen with a backlog.
    Listen,
    /// `VFS_ACCEPT`: accept, creating a new socket.
    Accept,
    /// `VFS_SENDTO`: send with an address.
    SendTo,
    /// `VFS_RECVFROM`: receive with an address.
    RecvFrom,
    /// `VFS_SENDMSG`: send with a `msghdr`.
    SendMsg,
    /// `VFS_RECVMSG`: receive with a `msghdr`.
    RecvMsg,
    /// `VFS_SETSOCKOPT`: set an option.
    SetSockOpt,
    /// `VFS_GETSOCKOPT`: get an option (length written back).
    GetSockOpt,
    /// `VFS_GETSOCKNAME`: local address (length written back).
    GetSockName,
    /// `VFS_GETPEERNAME`: remote address (length written back).
    GetPeerName,
    /// `VFS_SHUTDOWN`: shut read/write directions.
    Shutdown,
    /// `VFS_SOCKETPATH` (`path.c:803`): device lookup by path; executes
    /// in 13-path-lookup.md, referenced here only for surface completeness.
    SocketPathRef,
}

/// Reply shape of a call (header table right column, `socket.c:15-31`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyLayout {
    /// Status only (`m_type`).
    Status,
    /// `m_vfs_lc_fdpair` (`do_socketpair:264-265`).
    FdPair,
    /// `m_vfs_lc_socklen.len` (accept/recvfrom/get-variants).
    SockLen,
}

impl SockCall {
    /// Reply shape per call (`socket.c:15-31`).
    pub fn reply_layout(self) -> ReplyLayout {
        match self {
            Self::SocketPair => ReplyLayout::FdPair,
            Self::Accept
            | Self::RecvFrom
            | Self::GetSockOpt
            | Self::GetSockName
            | Self::GetPeerName => ReplyLayout::SockLen,
            _ => ReplyLayout::Status,
        }
    }

    /// Calls needing a cheap fd-budget pre-check before building
    /// (`do_socket:204`, `do_socketpair:242`, `do_accept:375`).
    pub fn needs_fd_precheck(self) -> bool {
        matches!(self, Self::Socket | Self::SocketPair | Self::Accept)
    }
}

/// `get_sock_flags` (`socket.c:44-57`): `SOCK_*` birth flags → `O_*` flags.
///
/// Only the three known bits convert; anything else in `type` is the
/// socket type proper (stripped separately by [`strip_sock_type`]).
pub fn sock_flags(sock_type: u32) -> u32 {
    let mut flags = 0;
    if sock_type & SOCK_CLOEXEC != 0 {
        flags |= O_CLOEXEC;
    }
    if sock_type & SOCK_NONBLOCK != 0 {
        flags |= O_NONBLOCK;
    }
    if sock_type & SOCK_NOSIGPIPE != 0 {
        flags |= O_NOSIGPIPE;
    }
    flags
}

/// Split raw `type` into base type + open flags
/// (`socket.c:207-208,245-246`: `type & ~SOCK_FLAGS_MASK`).
pub fn strip_sock_type(raw_type: u32) -> (u32, u32) {
    (raw_type & !SOCK_FLAGS_MASK, sock_flags(raw_type))
}

/// A verified socket endpoint (`get_sock` success, `socket.c:276-302`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SockTarget {
    /// Socket device number (`filp_vno->v_sdev`).
    pub dev: u64,
    /// File-pointer flags (`filp_flags`), if the caller wants them
    /// (`flags != NULL`; e.g. `do_listen` passes `NULL`).
    pub flags: Option<u32>,
}

/// The fd→socket gate behind a trait.
///
/// `get_sock` (`socket.c:276-302`) needs a live filp table; the table is
/// the only untestable point, so only the lookup is abstracted. The
/// unlocked-during-call note (`293-299`) becomes a documentation note:
/// under the single-threaded event loop the borrow rules hold by
/// construction (ARCH A-1).
pub trait SockLookup {
    /// Look up `fd`: live filp + `S_ISSOCK` mode, else an error.
    fn lookup(&self, fd: i32) -> Result<SockTarget, SockError>;
}

/// Scripted table (test double with programmed answers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLookup {
    /// Programmed `(fd, dev, flags, is_sock)` rows.
    pub rows: [(i32, u64, u32, bool); 8],
    /// Number of live rows.
    pub nrows: usize,
}

impl TableLookup {
    /// One live socket row.
    pub fn single(fd: i32, dev: u64, flags: u32) -> Self {
        let mut rows = [(0, 0, 0, false); 8];
        rows[0] = (fd, dev, flags, true);
        Self { rows, nrows: 1 }
    }
}

impl SockLookup for TableLookup {
    fn lookup(&self, fd: i32) -> Result<SockTarget, SockError> {
        for (rfd, dev, flags, is_sock) in self.rows.iter().take(self.nrows) {
            if *rfd == fd {
                if !is_sock {
                    return Err(SockError::NotSock);
                }
                return Ok(SockTarget {
                    dev: *dev,
                    flags: Some(*flags),
                });
            }
        }
        Err(SockError::BadFd)
    }
}

/// Empty table (test double: every fd is bad).
///
/// Behaves differently from [`TableLookup`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EmptyTable;

impl SockLookup for EmptyTable {
    fn lookup(&self, _fd: i32) -> Result<SockTarget, SockError> {
        Err(SockError::BadFd)
    }
}

/// Landing specification for one fresh socket (`make_sock_fd` inputs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SockFdSpec {
    /// Socket device number from the driver.
    pub dev: u64,
    /// `O_*` open flags (must sit inside [`SOCK_FD_FLAG_MASK`]).
    pub flags: u32,
}

impl SockFdSpec {
    /// Checked constructor: the C `assert` (`socket.c:94`) becomes a
    /// verdict (`EINVAL`), which — unlike `assert` — also holds in
    /// release builds (ARCH hardening).
    pub fn new(dev: u64, flags: u32) -> Result<Self, SockError> {
        if flags & !SOCK_FD_FLAG_MASK != 0 {
            return Err(SockError::Inval);
        }
        Ok(Self { dev, flags })
    }

    /// Fresh-socket mode (`socket.c:139`).
    pub fn mode(self) -> u32 {
        let _ = self;
        SOCK_NEW_MODE
    }

    /// Stored filp flags (`socket.c:158`: `O_RDWR | flags`).
    pub fn filp_flags(self) -> u32 {
        O_RDWR | self.flags
    }
}

/// The nine-step landing behind a trait.
///
/// `make_sock_fd` (`socket.c:86-170`) needs PFS, vnode, and fd tables;
/// only the outcome is modeled: fresh fd number, or a negative errno
/// with the socket left open (`socket.c:83`). Debug duplicate-device
/// refusal (`104-108`) surfaces as `Err(SockError::Io)`.
pub trait FdAllocator {
    /// Land `spec` in the caller: fresh fd, or a negative-errno error.
    fn alloc(&mut self, spec: SockFdSpec) -> Result<i32, SockError>;
}

/// Scripted allocator (test double with programmed answers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedAlloc {
    /// Programmed results, consumed in order.
    pub script: [Result<i32, SockError>; 8],
    /// Next script index.
    pub pos: usize,
    /// Specs landed (observable dialogue).
    pub landed: [u64; 8],
    /// Number of landings attempted.
    pub nlanded: usize,
}

impl ScriptedAlloc {
    /// Landings hand out `fd` values from `first` upward.
    pub fn fds_from(first: i32) -> Self {
        Self {
            script: [
                Ok(first),
                Ok(first + 1),
                Ok(first + 2),
                Ok(first + 3),
                Ok(first + 4),
                Ok(first + 5),
                Ok(first + 6),
                Ok(first + 7),
            ],
            pos: 0,
            landed: [0; 8],
            nlanded: 0,
        }
    }

    /// Script with failures at programmed positions.
    pub fn scripted(script: [Result<i32, SockError>; 8]) -> Self {
        Self {
            script,
            pos: 0,
            landed: [0; 8],
            nlanded: 0,
        }
    }
}

impl FdAllocator for ScriptedAlloc {
    fn alloc(&mut self, spec: SockFdSpec) -> Result<i32, SockError> {
        if self.nlanded < self.landed.len() {
            self.landed[self.nlanded] = spec.dev;
        }
        self.nlanded += 1;
        let out = self.script[self.pos % self.script.len()];
        self.pos += 1;
        out
    }
}

/// Failing allocator (test double: every landing fails with `EMFILE`).
///
/// Behaves differently from [`ScriptedAlloc`] (blanket failure vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailingAlloc(pub SockError);

impl FdAllocator for FailingAlloc {
    fn alloc(&mut self, _spec: SockFdSpec) -> Result<i32, SockError> {
        Err(self.0)
    }
}

/// What to close: a driver-side socket, or a process-side fd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseTarget {
    /// `sdev_close(dev, may_suspend=FALSE)`.
    DriverSock(u64),
    /// `close_fd(fp, fd, may_suspend=FALSE)`.
    ProcFd(i32),
}

/// Ordered close list (at most two closes: the pair path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseList {
    /// Closes in order.
    pub targets: [Option<CloseTarget>; 2],
}

impl CloseList {
    /// No closes.
    pub fn none() -> Self {
        Self {
            targets: [None, None],
        }
    }

    /// One close.
    pub fn one(t: CloseTarget) -> Self {
        Self {
            targets: [Some(t), None],
        }
    }

    /// Two closes, in order.
    pub fn two(a: CloseTarget, b: CloseTarget) -> Self {
        Self {
            targets: [Some(a), Some(b)],
        }
    }

    /// Number of closes.
    pub fn len(self) -> usize {
        self.targets.iter().filter(|t| t.is_some()).count()
    }

    /// True when nothing needs closing.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// Build progress needing compensation on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStep {
    /// `do_socket`: one driver socket built, landing failed.
    SocketBuilt(u64),
    /// `do_socketpair`: first landing failed; both driver sockets live.
    PairFirstFailed(u64, u64),
    /// `do_socketpair`: second landing failed; fd0 + dev1 live.
    PairSecondFailed(i32, u64),
    /// `resume_accept` case #1: landing failed; new driver socket lives.
    AcceptBuilt(u64),
}

/// Failure compensation: who buries what (`socket.c:214-215,252-261,461-466`).
///
/// Rule ("who builds, buries"): every built-but-unlanded socket is closed
/// driver-side without suspending; a landed-then-failed pair also closes
/// the landed fd. Nothing else is touched — in particular a failed lookup
/// closes nothing (nothing was built).
pub fn compensate(step: BuildStep) -> CloseList {
    match step {
        BuildStep::SocketBuilt(dev) | BuildStep::AcceptBuilt(dev) => {
            CloseList::one(CloseTarget::DriverSock(dev))
        }
        BuildStep::PairFirstFailed(dev0, dev1) => {
            CloseList::two(CloseTarget::DriverSock(dev0), CloseTarget::DriverSock(dev1))
        }
        BuildStep::PairSecondFailed(fd0, dev1) => {
            CloseList::two(CloseTarget::ProcFd(fd0), CloseTarget::DriverSock(dev1))
        }
    }
}

/// `resume_accept` outcome (`socket.c:399-477`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptResume {
    /// Case #3 (`411-415`): failed with no new socket — reply the error,
    /// never block.
    FailNoSock,
    /// Listener gone (`430-434`): reply `EIO`, keep the new socket
    /// (no burial: the driver death, not us, orphaned it).
    ListenGone,
    /// Case #2 (`444-450`): error with a new socket — bury it, reply error.
    CloseNewSock,
    /// Case #1 (`452-477`): land the new socket with masked flags.
    MakeFd {
        /// Flags masked to [`ACCEPT_INHERIT_MASK`] (`socket.c:459`).
        flags: u32,
    },
}

/// Classify an accept wakeup (`socket.c:411,430,444,452`).
///
/// - `status_ok`: driver reports success.
/// - `has_new_sock`: `dev != NO_DEV`.
/// - `listen_alive`: the listening socket still verifies.
pub fn classify_accept(status_ok: bool, has_new_sock: bool, listen_alive: bool) -> AcceptResume {
    if !status_ok && !has_new_sock {
        return AcceptResume::FailNoSock;
    }
    if !listen_alive {
        return AcceptResume::ListenGone;
    }
    if !status_ok {
        return AcceptResume::CloseNewSock;
    }
    AcceptResume::MakeFd { flags: 0 }
}

/// Mask listen flags for the accepted socket (`socket.c:459`).
pub fn accept_inherit_flags(listen_flags: u32) -> u32 {
    listen_flags & ACCEPT_INHERIT_MASK
}

/// `resume_recvfrom` reply (`socket.c:526-537`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvReply {
    /// `status >= 0`: reply `socklen.len = addr_len`, result `status`.
    Data {
        /// Address length to report.
        addr_len: u32,
        /// Byte count (= `status`).
        status: i32,
    },
    /// `status < 0`: reply the error code.
    Error(i32),
}

/// Classify a recvfrom wakeup (`socket.c:530-536`).
pub fn classify_recvfrom(status: i32, addr_len: u32) -> RecvReply {
    if status >= 0 {
        RecvReply::Data { addr_len, status }
    } else {
        RecvReply::Error(status)
    }
}

/// `msghdr` fields `resume_recvmsg` rewrites (`socket.c:641-644`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvmsgUpdate {
    /// `msg_controllen = ctl_len`.
    pub ctl_len: u32,
    /// `msg_flags = flags`.
    pub flags: u32,
    /// `msg_namelen = addr_len`, only when `addr_len > 0` (`643-644`).
    pub addr_len: Option<u32>,
}

/// Compute the `msghdr` rewrite (`socket.c:641-644`).
pub fn recvmsg_update(ctl_len: u32, flags: u32, addr_len: u32) -> RecvmsgUpdate {
    RecvmsgUpdate {
        ctl_len,
        flags,
        addr_len: if addr_len > 0 { Some(addr_len) } else { None },
    }
}

/// `iovec` plan (`do_sockmsg:566-587`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IovPlan {
    /// `msg_iovlen == 0`: no data.
    Empty,
    /// Exactly one element: take its base/len.
    One,
}

/// `iovec` gate: Minix3 takes at most one element; libc consolidates
/// (`socket.c:567-574`). More than one is `EMSGSIZE`, not silent
/// truncation.
pub fn iov_gate(iovlen: usize) -> Result<IovPlan, SockError> {
    match iovlen {
        0 => Ok(IovPlan::Empty),
        1 => Ok(IovPlan::One),
        _ => Err(SockError::MsgSize),
    }
}

/// `iov_len` bound (`socket.c:580-581`): beyond `SSIZE_MAX` is `EINVAL`.
pub fn check_iov_len(len: u64) -> Result<(), SockError> {
    if len > SSIZE_MAX_U64 {
        return Err(SockError::Inval);
    }
    Ok(())
}

/// Reply `msg_buf` rule (`socket.c:593-594`): receives echo the user
/// `msghdr` address back for the resume rewrite; sends pass zero.
pub fn reply_msgbuf(is_recv: bool, msg_buf: u64) -> u64 {
    if is_recv { msg_buf } else { 0 }
}

/// Shutdown direction (`socket.c:758-759`; values reuse 22-sdev.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownHow {
    /// `SHUT_RD`: disallow receives.
    Rd,
    /// `SHUT_WR`: disallow sends.
    Wr,
    /// `SHUT_RDWR`: disallow both.
    RdWr,
}

/// `how` gate (`socket.c:758-759`): anything else is `EINVAL`.
pub fn check_shutdown_how(how: i32) -> Result<ShutdownHow, SockError> {
    if how == SHUT_RD {
        Ok(ShutdownHow::Rd)
    } else if how == SHUT_WR {
        Ok(ShutdownHow::Wr)
    } else if how == SHUT_RDWR {
        Ok(ShutdownHow::RdWr)
    } else {
        Err(SockError::Inval)
    }
}

/// `backlog` clamp (`do_listen:355-356`): negative becomes zero.
pub fn clamp_backlog(backlog: i32) -> u32 {
    backlog.max(0) as u32
}

/// What `do_*` tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockVerdict {
    /// Reply now (fd, count, or status carried separately).
    Done,
    /// The driver holds the call; a resume will finish it.
    Suspend,
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// `SUSPEND` is a verdict (08/09 own suspension), not an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockError {
    /// `EAFNOSUPPORT`: no socket driver for this domain (`187,235`).
    NoSupport,
    /// `ENOTSOCK`: fd is live but not a socket (`283-286`).
    NotSock,
    /// `EMSGSIZE`: more than one `iovec` element (`573-574`).
    MsgSize,
    /// `EINVAL`: bad `how`, bad landing flags, oversize `iov_len`.
    Inval,
    /// `EIO`: driver misbehavior (in-use socket ID) or listener lost.
    Io,
    /// `EBADF`: dead fd on lookup.
    BadFd,
    /// `EMFILE`: no fd room (cheap pre-check or landing failure).
    NoSpace,
}

impl SockError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NoSupport => minix_types::EAFNOSUPPORT,
            Self::NotSock => minix_types::ENOTSOCK,
            Self::MsgSize => minix_types::EMSGSIZE,
            Self::Inval => minix_types::EINVAL,
            Self::Io => minix_types::EIO,
            Self::BadFd => minix_types::EBADF,
            Self::NoSpace => minix_types::EMFILE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calls_cover_sixteen() {
        // Sixteen surface calls (`socket.c:9-32`); numbers stay with 09.
        let all = [
            SockCall::Socket,
            SockCall::SocketPair,
            SockCall::Bind,
            SockCall::Connect,
            SockCall::Listen,
            SockCall::Accept,
            SockCall::SendTo,
            SockCall::RecvFrom,
            SockCall::SendMsg,
            SockCall::RecvMsg,
            SockCall::SetSockOpt,
            SockCall::GetSockOpt,
            SockCall::GetSockName,
            SockCall::GetPeerName,
            SockCall::Shutdown,
            SockCall::SocketPathRef,
        ];
        assert_eq!(all.len(), 16);
        // Reply shapes follow the header table right column.
        assert_eq!(SockCall::SocketPair.reply_layout(), ReplyLayout::FdPair);
        assert_eq!(SockCall::Accept.reply_layout(), ReplyLayout::SockLen);
        assert_eq!(SockCall::RecvFrom.reply_layout(), ReplyLayout::SockLen);
        assert_eq!(SockCall::Socket.reply_layout(), ReplyLayout::Status);
        assert_eq!(SockCall::Shutdown.reply_layout(), ReplyLayout::Status);
        // Only builders need the cheap pre-check.
        assert!(SockCall::Socket.needs_fd_precheck());
        assert!(SockCall::SocketPair.needs_fd_precheck());
        assert!(SockCall::Accept.needs_fd_precheck());
        assert!(!SockCall::Bind.needs_fd_precheck());
        assert!(!SockCall::Shutdown.needs_fd_precheck());
    }

    #[test]
    fn test_flag_conversion() {
        // `get_sock_flags:49-54` converts exactly three bits.
        assert_eq!(sock_flags(SOCK_CLOEXEC), O_CLOEXEC);
        assert_eq!(
            sock_flags(SOCK_NONBLOCK | SOCK_NOSIGPIPE),
            O_NONBLOCK | O_NOSIGPIPE
        );
        assert_eq!(sock_flags(0), 0);
        // Unknown SOCK_ bits are the socket type, silently ignored here.
        assert_eq!(sock_flags(0x0800_0000), 0);
        // `strip_sock_type:207` splits base type from birth flags.
        let (base, flags) = strip_sock_type(1 | SOCK_NONBLOCK);
        assert_eq!(base, 1);
        assert_eq!(flags, O_NONBLOCK);
    }

    #[test]
    fn test_lookup_gate() {
        // Live socket row verifies (`get_sock:283-290`).
        let table = TableLookup::single(3, 0x2201, O_NONBLOCK);
        let hit = table.lookup(3).unwrap();
        assert_eq!(hit.dev, 0x2201);
        assert_eq!(hit.flags, Some(O_NONBLOCK));
        // Live non-socket fd is ENOTSOCK, not EBADF (`283-286`).
        let mut rows = [(0, 0, 0, false); 8];
        rows[0] = (4, 0x100, 0, false);
        let mixed = TableLookup { rows, nrows: 1 };
        assert_eq!(mixed.lookup(4), Err(SockError::NotSock));
        // Dead fd is EBADF (`280-281` err_code path).
        assert_eq!(table.lookup(9), Err(SockError::BadFd));
        // Empty table refuses everything (second impl).
        assert_eq!(EmptyTable.lookup(3), Err(SockError::BadFd));
        assert_eq!(SockError::NotSock.to_errno(), minix_types::ENOTSOCK);
    }

    #[test]
    fn test_landing_spec_and_allocator() {
        // Mask assertion is a verdict, not a crash (`socket.c:94`).
        let spec = SockFdSpec::new(0x2201, O_CLOEXEC | O_NONBLOCK).unwrap();
        assert_eq!(spec.mode(), SOCK_NEW_MODE);
        assert_eq!(spec.filp_flags(), O_RDWR | O_CLOEXEC | O_NONBLOCK);
        assert_eq!(SockFdSpec::new(0x2201, 0x0000_0008), Err(SockError::Inval));
        // Fresh-socket mode is IFsock + full perms (`socket.c:139`).
        assert_eq!(SOCK_NEW_MODE, S_IFSOCK | ACCESSPERMS);
        // Scripted landings hand out fds in order.
        let mut alloc = ScriptedAlloc::fds_from(3);
        assert_eq!(alloc.alloc(spec), Ok(3));
        assert_eq!(alloc.alloc(spec), Ok(4));
        assert_eq!(alloc.nlanded, 2);
        // Blanket failure models table exhaustion.
        let mut failing = FailingAlloc(SockError::NoSpace);
        assert_eq!(failing.alloc(spec), Err(SockError::NoSpace));
        // Accept inherits exactly three flags (`socket.c:459`).
        assert_eq!(accept_inherit_flags(0xFFFF_FFFF), ACCEPT_INHERIT_MASK);
        assert_eq!(accept_inherit_flags(O_RDWR), 0);
    }

    #[test]
    fn test_compensation_table() {
        // Single build: bury the driver socket (`214-215,461-466`).
        let c = compensate(BuildStep::SocketBuilt(7));
        assert_eq!(c.len(), 1);
        assert_eq!(c.targets[0], Some(CloseTarget::DriverSock(7)));
        assert_eq!(compensate(BuildStep::AcceptBuilt(9)).len(), 1);
        // Pair first-landing failure: bury both driver sockets (`252-255`).
        let c = compensate(BuildStep::PairFirstFailed(7, 8));
        assert_eq!(c.len(), 2);
        assert_eq!(
            c.targets,
            [
                Some(CloseTarget::DriverSock(7)),
                Some(CloseTarget::DriverSock(8))
            ]
        );
        // Pair second-landing failure: close fd0, bury dev1 (`258-261`).
        let c = compensate(BuildStep::PairSecondFailed(3, 8));
        assert_eq!(
            c.targets,
            [
                Some(CloseTarget::ProcFd(3)),
                Some(CloseTarget::DriverSock(8))
            ]
        );
        assert_eq!(CloseList::none().len(), 0);
    }

    #[test]
    fn test_resume_classification() {
        // Case #3: failed with no socket — error out, never block (`411-415`).
        assert_eq!(
            classify_accept(false, false, true),
            AcceptResume::FailNoSock
        );
        // Listener gone: EIO upstream, keep the new socket (`430-434`).
        assert_eq!(classify_accept(true, true, false), AcceptResume::ListenGone);
        assert_eq!(
            classify_accept(false, true, false),
            AcceptResume::ListenGone
        );
        // Case #2: error with a socket — bury then reply (`444-450`).
        assert_eq!(
            classify_accept(false, true, true),
            AcceptResume::CloseNewSock
        );
        // Case #1: land with masked flags (`452-466`).
        assert_eq!(
            classify_accept(true, true, true),
            AcceptResume::MakeFd { flags: 0 }
        );
        // recvfrom splits on the sign (`530-536`).
        assert_eq!(
            classify_recvfrom(12, 16),
            RecvReply::Data {
                addr_len: 16,
                status: 12
            }
        );
        assert_eq!(classify_recvfrom(-5, 16), RecvReply::Error(-5));
        // recvmsg rewrites three fields, namelen only when set (`641-644`).
        let u = recvmsg_update(8, 0, 16);
        assert_eq!(u.addr_len, Some(16));
        let u = recvmsg_update(8, 0, 0);
        assert_eq!(u.addr_len, None);
        assert_eq!(u.ctl_len, 8);
    }

    #[test]
    fn test_envelope_doors() {
        // At most one iovec element (`573-574`); libc consolidates.
        assert_eq!(iov_gate(0), Ok(IovPlan::Empty));
        assert_eq!(iov_gate(1), Ok(IovPlan::One));
        assert_eq!(iov_gate(2), Err(SockError::MsgSize));
        assert_eq!(iov_gate(8), Err(SockError::MsgSize));
        // Oversize iov length rejected (`580-581`).
        assert!(check_iov_len(0).is_ok());
        assert!(check_iov_len(SSIZE_MAX_U64).is_ok());
        assert_eq!(check_iov_len(SSIZE_MAX_U64 + 1), Err(SockError::Inval));
        // Reply msg_buf: receives echo, sends zero (`593-594`).
        assert_eq!(reply_msgbuf(true, 0x1000), 0x1000);
        assert_eq!(reply_msgbuf(false, 0x1000), 0);
        // Shutdown takes exactly three directions (`758-759`).
        assert_eq!(check_shutdown_how(SHUT_RD), Ok(ShutdownHow::Rd));
        assert_eq!(check_shutdown_how(SHUT_WR), Ok(ShutdownHow::Wr));
        assert_eq!(check_shutdown_how(SHUT_RDWR), Ok(ShutdownHow::RdWr));
        assert_eq!(check_shutdown_how(3), Err(SockError::Inval));
        // Negative backlog clamps to zero (`355-356`).
        assert_eq!(clamp_backlog(-1), 0);
        assert_eq!(clamp_backlog(0), 0);
        assert_eq!(clamp_backlog(128), 128);
    }

    #[test]
    fn test_errno_map_covers_socket_c() {
        for (err, errno) in [
            (SockError::NoSupport, minix_types::EAFNOSUPPORT),
            (SockError::NotSock, minix_types::ENOTSOCK),
            (SockError::MsgSize, minix_types::EMSGSIZE),
            (SockError::Inval, minix_types::EINVAL),
            (SockError::Io, minix_types::EIO),
            (SockError::BadFd, minix_types::EBADF),
            (SockError::NoSpace, minix_types::EMFILE),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
