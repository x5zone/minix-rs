//! `pipe` — pipe sizing, suspension bookkeeping, and wakeup verdicts.
//!
//! Corresponds to Minix3's `pipe.c:1-561` (`do_pipe2/create_pipe`,
//! `map_vnode`, `pipe_check`, `suspend/pipe_suspend`, `unsuspend_by_endpt`,
//! `release`, `revive`, `unpause`) with the suspension counters declared in
//! `glo.h:14-16` (`susp_count/reviving`) and the blocked-on states in
//! `const.h:19-25`.
//!
//! Design decisions (see 17-pipe.md §3):
//! - `pipe_check_decision` is a pure sizing function over the read/write matrix
//! - `PipeNodeFactory` trait isolates PipeFS node creation (test doubles)
//! - `SuspLedger/ReviveLedger` make the suspension counters observable
//! - `release_match` is a pure wakeup predicate over the proc-table rule
//! - `revive_decision/unpause_decision` are pure verdicts; execution stays
//!   with the main loop (09), drivers (21/22), and select (23)
//! - unreachable C panics become defensive errors (ARCH hardening)

use minix_types::VirBytes;

use crate::fproc::{PipeBlock, PipeIo};
use crate::read_write::RwDir;

/// `PIPE_BUF` under `__minix` (`minix3/sys/sys/syslimits.h:66`): atomic
/// pipe-write threshold used by the sizing matrix.
pub const PIPE_BUF: u64 = 32768;

/// Merge `pipe2` flag halves (`do_pipe2`, `pipe.c:45-46`): the `oflags`
/// field rides along for backward compatibility.
pub fn merge_pipe2_flags(flags: u32, oflags: u32) -> u32 {
    flags | oflags
}

/// Which end of a pipe a decision concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeEnd {
    /// Read end (`O_RDONLY`).
    Read,
    /// Write end (`O_WRONLY`).
    Write,
}

/// Who to wake when a pipe state change unblocks peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WakePlan {
    /// Release suspended readers (`release(vp, VFS_READ, …)`).
    pub wake_readers: bool,
    /// Release suspended writers (`release(vp, VFS_WRITE, …)`).
    pub wake_writers: bool,
}

/// Verdict of the pipe sizing check (`pipe_check`, `pipe.c:187-288`).
///
/// Every verdict carries its full post-condition: `Allow` and `Suspend`
/// embed the peers to wake, so the caller never consults `susp_count`
/// twice. The single exception is an empty-read `EAGAIN`, whose writer
/// wake rides the companion [`read_reject_wake`] (`pipe.c:229` runs
/// before the fail-fast return).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeCheckVerdict {
    /// Transfer may proceed with this many bytes; wake the plan first.
    Allow {
        /// Bytes approved for this round.
        bytes: u64,
        /// Peers to release before transferring.
        wake: WakePlan,
    },
    /// Caller must suspend; wake the planned peers first.
    Suspend(WakePlan),
    /// Transfer must fail with this error.
    Reject(PipeError),
}

/// Pure pipe sizing decision (`pipe_check` core).
///
/// - `dir`: read or write (peek is rejected by 16 before reaching here).
/// - `buffered`: `v_size` bytes currently in the pipe.
/// - `capacity`: `PIPE_BUF` atomic threshold.
/// - `reader`/`writer`: whether the opposite end is still open
///   (`find_filp` results).
/// - `nonblock`: `O_NONBLOCK` present.
/// - `touch`: `!notouch` — POSIX "check only" mode skips wakeups, except
///   the empty-read wake which fires regardless (`pipe.c:229-230`).
/// - `requested`: remaining bytes for this round.
/// - `waiters`: `susp_count` sleepers that could be released.
#[allow(clippy::too_many_arguments)]
pub fn pipe_check_decision(
    dir: RwDir,
    buffered: u64,
    capacity: u64,
    reader: bool,
    writer: bool,
    nonblock: bool,
    touch: bool,
    requested: u64,
    waiters: u32,
) -> PipeCheckVerdict {
    if dir == RwDir::Read {
        return read_decision(buffered, writer, nonblock, touch, requested, waiters);
    }
    if dir == RwDir::Write {
        return write_decision(buffered, capacity, reader, nonblock, touch, requested);
    }
    // `rw_pipe:340` asserts read-or-write; peek never reaches the sizer.
    PipeCheckVerdict::Reject(PipeError::Inval)
}

/// Read half of the sizing matrix (`pipe.c:217-235`).
///
/// The empty-read fail-fast (`EAGAIN`) still owes sleeping writers a
/// wakeup; see [`read_reject_wake`].
fn read_decision(
    buffered: u64,
    writer: bool,
    nonblock: bool,
    _touch: bool,
    requested: u64,
    waiters: u32,
) -> PipeCheckVerdict {
    if buffered > 0 {
        return PipeCheckVerdict::Allow {
            bytes: requested,
            wake: WakePlan::default(),
        };
    }
    // Empty pipe: a living writer means "wait or fail fast".
    if writer {
        // Fires even in check-only mode (`pipe.c:229-230`).
        let wake = WakePlan {
            wake_readers: false,
            wake_writers: waiters > 0,
        };
        if nonblock {
            return PipeCheckVerdict::Reject(PipeError::Again);
        }
        return PipeCheckVerdict::Suspend(wake);
    }
    // Empty with no writer: end of stream, zero bytes (`pipe.c:232`).
    PipeCheckVerdict::Allow {
        bytes: 0,
        wake: WakePlan::default(),
    }
}

/// Writers owed a wakeup after an empty-read `EAGAIN`.
///
/// `pipe_check` releases sleeping writers even when failing fast
/// (`pipe.c:229` runs before the `return r` at :232), so the caller
/// applies this plan alongside a `Reject(Again)` on an empty read.
pub fn read_reject_wake(waiters: u32) -> WakePlan {
    WakePlan {
        wake_readers: false,
        wake_writers: waiters > 0,
    }
}

/// Write half of the sizing matrix (`pipe.c:237-287`).
fn write_decision(
    buffered: u64,
    capacity: u64,
    reader: bool,
    nonblock: bool,
    touch: bool,
    requested: u64,
) -> PipeCheckVerdict {
    // No reader at all: broken pipe (`pipe.c:238-240`).
    if !reader {
        return PipeCheckVerdict::Reject(PipeError::Pipe);
    }
    let wake_readers = WakePlan {
        wake_readers: touch,
        wake_writers: false,
    };
    // Over capacity: atomicity decides between fail-fast and partial.
    if buffered.saturating_add(requested) > capacity {
        if nonblock {
            if requested <= capacity {
                return PipeCheckVerdict::Reject(PipeError::Again);
            }
            let room = capacity.saturating_sub(buffered);
            if room > 0 {
                return PipeCheckVerdict::Allow {
                    bytes: room,
                    wake: wake_readers,
                };
            }
            return PipeCheckVerdict::Reject(PipeError::Again);
        }
        let room = capacity.saturating_sub(buffered);
        if room > 0 {
            // Partial round; the caller suspends for the rest while the
            // woken readers drain (`pipe.c:268-274`).
            return PipeCheckVerdict::Suspend(wake_readers);
        }
        return PipeCheckVerdict::Suspend(WakePlan::default());
    }
    // Fits. A write into an empty pipe still pokes sleeping readers
    // (`pipe.c:282-284`); otherwise there is nobody new to wake.
    let wake = if buffered == 0 {
        wake_readers
    } else {
        WakePlan::default()
    };
    PipeCheckVerdict::Allow {
        bytes: requested,
        wake,
    }
}

/// Which creation stage `create_pipe` reached (`pipe.c:60-144`).
///
/// Later stages roll back more: each failure unwinds everything the
/// earlier stages reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CreateStage {
    /// PFS vmnt located and vnode reserved.
    Vnode,
    /// Read fd + filp reserved.
    FdRead,
    /// Write fd + filp reserved.
    FdWrite,
    /// PipeFS node created and filled in.
    Node,
}

/// What unwinding a failed stage must release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RollbackPlan {
    /// Clear the read fd and free its filp (`pipe.c:90-92`).
    pub free_read: bool,
    /// Clear the write fd and free its filp (`pipe.c:105-110`).
    pub free_write: bool,
    /// Release the reserved vnode (`unlock_vnode` on every path).
    pub free_vnode: bool,
}

/// Rollback table for `create_pipe` failures.
///
/// - `FdRead` fails → only the vnode is held (`pipe.c:82-86`).
/// - `FdWrite` fails → additionally unwind the read end (`pipe.c:89-96`).
/// - `Node` fails → unwind both ends (`pipe.c:104-113`).
pub fn rollback_for(failed_at: CreateStage) -> RollbackPlan {
    match failed_at {
        CreateStage::Vnode => RollbackPlan {
            free_read: false,
            free_write: false,
            free_vnode: true,
        },
        CreateStage::FdRead => RollbackPlan {
            free_read: false,
            free_write: false,
            free_vnode: true,
        },
        CreateStage::FdWrite => RollbackPlan {
            free_read: true,
            free_write: false,
            free_vnode: true,
        },
        CreateStage::Node => RollbackPlan {
            free_read: true,
            free_write: true,
            free_vnode: true,
        },
    }
}

/// A fresh PipeFS node (`node_details` for `I_NAMED_PIPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeNode {
    /// File-server endpoint owning the node.
    pub fs_e: i32,
    /// Inode number assigned by PipeFS.
    pub inode_nr: u64,
}

/// PipeFS hook for node creation (`req_newnode`, `pipe.c:101`).
///
/// Isolates the PipeFS round-trip so the creation machine is unit-testable
/// without a live PipeFS.
pub trait PipeNodeFactory {
    /// Create an `I_NAMED_PIPE` node; map FS errors to [`PipeError`].
    fn newnode(&self) -> Result<PipeNode, PipeError>;
}

/// Factory whose creation always succeeds (test double).
#[derive(Debug, Default, Clone, Copy)]
pub struct MemPipeFs;

impl PipeNodeFactory for MemPipeFs {
    fn newnode(&self) -> Result<PipeNode, PipeError> {
        Ok(PipeNode {
            fs_e: 9,
            inode_nr: 1,
        })
    }
}

/// Factory whose creation always fails with `EIO` (test double).
///
/// Behaves differently from [`MemPipeFs`] (success vs refusal), satisfying
/// the "two behaviorally different impls" rule for traits.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailPipeFs;

impl PipeNodeFactory for FailPipeFs {
    fn newnode(&self) -> Result<PipeNode, PipeError> {
        Err(PipeError::Io)
    }
}

/// Split `pipe2` flags into per-end open flags (`pipe.c:133-134`).
///
/// The read end is `O_RDONLY`, the write end `O_WRONLY`; the remaining
/// flag bits (minus the access mode) apply to both ends.
pub fn end_flags(flags: u32, accmode_mask: u32) -> (u32, u32) {
    let shared = flags & !accmode_mask;
    (shared, shared | 0x1)
}

/// Suspension counter (`susp_count`, `glo.h:14`).
///
/// Counts processes suspended on pipes; `release` decrements per wakeup
/// and C panics if it goes negative (`pipe.c:424-425`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SuspLedger(pub u32);

impl SuspLedger {
    /// New empty ledger.
    pub fn new() -> Self {
        Self(0)
    }

    /// Current waiter count.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Increment per `suspend()` pipe rule (`pipe.c:304-306`).
    pub fn inc(&mut self, why_pipe: bool) {
        if why_pipe {
            self.0 += 1;
        }
    }

    /// Decrement per wakeup; `false` on underflow (C panics here —
    /// a double wakeup is always a bug, reported as a testable value
    /// instead of a crash).
    pub fn dec_checked(&mut self) -> bool {
        if self.0 == 0 {
            return false;
        }
        self.0 -= 1;
        true
    }
}

/// Whether `suspend()` counts this suspension (`pipe.c:304-306`).
///
/// Only pipe-open and pipe I/O suspensions feed `susp_count`.
pub fn susp_delta(block: SuspendKind) -> u32 {
    match block {
        SuspendKind::Pipe | SuspendKind::PipeOpen => 1,
        SuspendKind::Other => 0,
    }
}

/// Suspension reason relevant to the pipe ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendKind {
    /// `FP_BLOCKED_ON_PIPE`.
    Pipe,
    /// `FP_BLOCKED_ON_POPEN`.
    PipeOpen,
    /// Any other blocked-on state (select/cdev/sdev/flock).
    Other,
}

/// Revival counter (`reviving`, `glo.h:16`).
///
/// Counts pipe/lock waiters marked for deferred wakeup in the main loop.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReviveLedger(pub u32);

impl ReviveLedger {
    /// New empty ledger.
    pub fn new() -> Self {
        Self(0)
    }

    /// Current count.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Mark one more waiter (`revive`, `pipe.c:459`).
    pub fn inc(&mut self) {
        self.0 += 1;
    }

    /// Consume one mark (`unpause`, `pipe.c:518`); `false` on underflow.
    pub fn dec_checked(&mut self) -> bool {
        if self.0 == 0 {
            return false;
        }
        self.0 -= 1;
        true
    }
}

/// Build the resume record `pipe_suspend` stores (`pipe.c:315-328`).
///
/// Reuses the authoritative [`PipeBlock`] (02-fproc-struct.md) instead of
/// redefining the five fields.
pub fn suspend_record(
    call: PipeIo,
    fd: usize,
    buf: VirBytes,
    nbytes: usize,
    cum_io: usize,
) -> PipeBlock {
    PipeBlock {
        call,
        fd,
        buf,
        nbytes,
        cum_io,
    }
}

/// Which operation a waiter was performing (for `release` matching).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeOp {
    /// `VFS_OPEN`: match pipe-open waiters.
    Open,
    /// `VFS_READ` / `VFS_WRITE`: match pipe-I/O waiters by call.
    Io(PipeIo),
}

/// The blocked-on flavour of a candidate waiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// `FP_BLOCKED_ON_POPEN`.
    PipeOpen,
    /// `FP_BLOCKED_ON_PIPE` with its recorded call.
    Pipe(PipeIo),
    /// Any other state (never matches a pipe release).
    Other,
}

/// Pure `release` match predicate (`pipe.c:403-419`).
///
/// Six-way conjunction: live process, blocked in a matching way, not
/// already revived, valid open filp, same vnode. Select-callback sweep
/// (379-394) and table scan (397+) share this rule.
#[allow(clippy::too_many_arguments)]
pub fn release_match(
    live: bool,
    blocked: BlockKind,
    op: WakeOp,
    revived: bool,
    filp_ok: bool,
    same_vnode: bool,
) -> bool {
    if !live || revived || !filp_ok || !same_vnode {
        return false;
    }
    match (op, blocked) {
        (WakeOp::Open, BlockKind::PipeOpen) => true,
        (WakeOp::Io(call), BlockKind::Pipe(call2)) => call == call2,
        _ => false,
    }
}

/// Clear one select direction after its callback (`pipe.c:392`).
pub fn select_ack(ops: u8, selop: u8) -> u8 {
    ops & !selop
}

/// Driver-waiter classification for `unsuspend_by_endpt` (`pipe.c:335-357`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverWake {
    /// Revive a vanished char-driver waiter with `EIO` (`pipe.c:344-346`).
    ReviveEio,
    /// Stop a vanished socket-driver waiter (`pipe.c:347-350`).
    StopSdev,
    /// Select waiters are scattered separately (`pipe.c:354`); anything
    /// else is none of this function's business.
    Ignore,
}

/// Classify one waiter for driver-vanish scattering.
pub fn classify_driver_waiter(cdev_match: bool, sdev_match: bool) -> DriverWake {
    if cdev_match {
        DriverWake::ReviveEio
    } else if sdev_match {
        DriverWake::StopSdev
    } else {
        DriverWake::Ignore
    }
}

/// How `revive` answers (`pipe.c:435-492`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviveVerdict {
    /// Bad endpoint, unblocked, or already revived: do nothing.
    Noop,
    /// Pipe/lock waiter: mark for main-loop revival (`pipe.c:456-459`).
    MarkReviving,
    /// Anyone else: reply immediately with this code.
    Reply(i32),
    /// Socket/unknown states C panics on (`pipe.c:487,489`): defensive `EIO`.
    Invalid,
}

/// Pure revive decision.
///
/// - `endpoint_ok`: `proc_e != NONE && isokendpt` (`pipe.c:445`).
/// - `blocked`: current blocked-on state, if any.
/// - `revived`: `FP_REVIVED` already set (`pipe.c:448`).
/// - `popen_fd`: fd to report for pipe-open waiters (`pipe.c:465`).
/// - `code`: driver/select return code to report.
pub fn revive_decision(
    endpoint_ok: bool,
    blocked: Option<BlockKindEx>,
    revived: bool,
    popen_fd: i32,
    code: i32,
) -> ReviveVerdict {
    if !endpoint_ok || revived {
        return ReviveVerdict::Noop;
    }
    match blocked {
        None => ReviveVerdict::Noop,
        Some(BlockKindEx::Pipe) | Some(BlockKindEx::Flock) => ReviveVerdict::MarkReviving,
        Some(BlockKindEx::PipeOpen) => ReviveVerdict::Reply(popen_fd),
        Some(BlockKindEx::Select) | Some(BlockKindEx::Cdev) => ReviveVerdict::Reply(code),
        Some(BlockKindEx::Sdev) | Some(BlockKindEx::Unknown) => ReviveVerdict::Invalid,
    }
}

/// Blocked-on flavour for revive/unpause decisions (all six + unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKindEx {
    /// `FP_BLOCKED_ON_PIPE`.
    Pipe,
    /// `FP_BLOCKED_ON_FLOCK`.
    Flock,
    /// `FP_BLOCKED_ON_POPEN`.
    PipeOpen,
    /// `FP_BLOCKED_ON_SELECT`.
    Select,
    /// `FP_BLOCKED_ON_CDEV`.
    Cdev,
    /// `FP_BLOCKED_ON_SDEV`.
    Sdev,
    /// Anything else (`revive:489` default panic arm).
    Unknown,
}

/// How `unpause` answers (`pipe.c:498-561`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnpauseReply {
    /// Partial pipe progress wins over `EINTR` (`pipe.c:527-528`).
    Bytes(u64),
    /// Interrupted with no progress (`pipe.c:503` default).
    Intr,
}

/// Cancellation side-effect of `unpause` per branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOp {
    /// Pipe-open/flock/pipe-bytes: nothing to cancel.
    None,
    /// Select: `select_forget()` (`pipe.c:535`).
    ForgetSelect,
    /// Char device: `cdev_cancel()` (`pipe.c:542`).
    CancelCdev,
    /// Socket: `sdev_cancel()` sends its own reply (`pipe.c:548-549`).
    CancelSdev,
}

/// Full `unpause` plan: reply + cancel action + counter adjustments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnpausePlan {
    /// What to report for the interrupted call.
    pub reply: UnpauseReply,
    /// Cancellation to execute.
    pub cancel: CancelOp,
    /// Decrement `susp_count` (pipe/pipe-open, not already reviving).
    pub dec_susp: bool,
    /// Decrement `reviving` (was marked, `pipe.c:516-520`).
    pub dec_reviving: bool,
}

/// Pure unpause decision.
///
/// - `blocked`: what the process was blocked on (already cleared, `pipe.c:514`).
/// - `cum_io`: partial pipe progress so far.
/// - `was_reviving`: `FP_REVIVED` was set.
pub fn unpause_decision(blocked: BlockKindEx, cum_io: u64, was_reviving: bool) -> UnpausePlan {
    let (reply, cancel) = match blocked {
        BlockKindEx::Pipe => {
            if cum_io > 0 {
                (UnpauseReply::Bytes(cum_io), CancelOp::None)
            } else {
                (UnpauseReply::Intr, CancelOp::None)
            }
        }
        BlockKindEx::Flock | BlockKindEx::PipeOpen => (UnpauseReply::Intr, CancelOp::None),
        BlockKindEx::Select => (UnpauseReply::Intr, CancelOp::ForgetSelect),
        BlockKindEx::Cdev => (UnpauseReply::Intr, CancelOp::CancelCdev),
        BlockKindEx::Sdev => (UnpauseReply::Intr, CancelOp::CancelSdev),
        BlockKindEx::Unknown => (UnpauseReply::Intr, CancelOp::None),
    };
    let pipe_like = matches!(blocked, BlockKindEx::Pipe | BlockKindEx::PipeOpen);
    UnpausePlan {
        reply,
        cancel,
        dec_susp: pipe_like && !was_reviving,
        dec_reviving: was_reviving,
    }
}

/// Mapping verdict for `map_vnode` (`pipe.c:151-182`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapVerdict {
    /// Already mapped: nothing to do (`pipe.c:157`).
    AlreadyMapped,
    /// Proceed with creation; note whether to unlock afterwards.
    Proceed(UnlockNote),
    /// No such endpoint (`pipe.c:160` panic becomes `EIO`).
    Absent,
}

/// Post-creation unlock obligation (`pipe.c:162-167,179`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockNote {
    /// Unlock the vmnt afterwards.
    Unlock,
    /// `EBUSY` meant it was already locked: do not unlock.
    SkipUnlock,
}

/// Pure map decision.
pub fn map_decision(mapped: bool, vmnt_found: bool, busy: bool) -> MapVerdict {
    if mapped {
        return MapVerdict::AlreadyMapped;
    }
    if !vmnt_found {
        return MapVerdict::Absent;
    }
    if busy {
        MapVerdict::Proceed(UnlockNote::SkipUnlock)
    } else {
        MapVerdict::Proceed(UnlockNote::Unlock)
    }
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// `SUSPEND` (`com.h:1151`) is a verdict, not an error, and lives in
/// [`PipeCheckVerdict::Suspend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeError {
    /// `EAGAIN`: non-blocking empty read / full write.
    Again,
    /// `EPIPE`: write with no reader (also arms `SIGPIPE` in 16).
    Pipe,
    /// `EIO`: creation failure, driver-vanish revival, defensive cases.
    Io,
    /// `EINTR`: signal-interrupted suspension.
    Intr,
    /// `EINVAL`: reserved (e.g. peek reaching the sizer).
    Inval,
}

impl PipeError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Again => minix_types::EAGAIN,
            Self::Pipe => minix_types::EPIPE,
            Self::Io => minix_types::EIO,
            Self::Intr => minix_types::EINTR,
            Self::Inval => minix_types::EINVAL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe2_flag_merge() {
        // `flags |= oflags` backward compat (`pipe.c:46`).
        assert_eq!(merge_pipe2_flags(0x80000, 0x4), 0x80004);
        assert_eq!(merge_pipe2_flags(0, 0), 0);
        // Per-end split (`pipe.c:133-134`): read drops to_RDONLY, write to
        // WRONLY, shared bits survive on both.
        let (rd, wr) = end_flags(0x80004, 0x3);
        assert_eq!(rd & 0x3, 0);
        assert_eq!(wr & 0x3, 0x1);
        assert_eq!(rd & 0x80000, 0x80000);
        assert_eq!(wr & 0x80004, 0x80004);
    }

    #[test]
    fn test_create_rollback_table() {
        // Later failures unwind more (`pipe.c:82-96,104-113`).
        assert_eq!(
            rollback_for(CreateStage::FdRead),
            RollbackPlan {
                free_read: false,
                free_write: false,
                free_vnode: true
            }
        );
        assert_eq!(
            rollback_for(CreateStage::FdWrite),
            RollbackPlan {
                free_read: true,
                free_write: false,
                free_vnode: true
            }
        );
        assert_eq!(
            rollback_for(CreateStage::Node),
            RollbackPlan {
                free_read: true,
                free_write: true,
                free_vnode: true
            }
        );
        // Monotonic: each stage frees a superset of the previous.
        let stages = [
            CreateStage::Vnode,
            CreateStage::FdRead,
            CreateStage::FdWrite,
            CreateStage::Node,
        ];
        let mut prev = 0u8;
        for s in stages {
            let r = rollback_for(s);
            let bits =
                (r.free_read as u8) | ((r.free_write as u8) << 1) | ((r.free_vnode as u8) << 2);
            assert!(bits >= prev);
            prev = bits;
        }
    }

    #[test]
    fn test_factories_differ() {
        // Gate D: the two `PipeNodeFactory` impls behave differently.
        assert!(MemPipeFs.newnode().is_ok());
        assert_eq!(FailPipeFs.newnode().unwrap_err(), PipeError::Io);
        fn via<F: PipeNodeFactory>(f: &F) -> bool {
            f.newnode().is_ok()
        }
        assert!(via(&MemPipeFs));
        assert!(!via(&FailPipeFs));
    }

    #[test]
    fn test_read_sizing_matrix() {
        let none = WakePlan::default();
        // Non-empty pipe serves the full request, nobody new to wake.
        assert_eq!(
            pipe_check_decision(RwDir::Read, 100, PIPE_BUF, true, true, false, true, 1000, 0),
            PipeCheckVerdict::Allow {
                bytes: 1000,
                wake: none
            }
        );
        // Empty + writer: block (`pipe.c:224-225`) or fail fast (`222-223`).
        assert_eq!(
            pipe_check_decision(RwDir::Read, 0, PIPE_BUF, false, true, false, true, 10, 0),
            PipeCheckVerdict::Suspend(none)
        );
        assert_eq!(
            pipe_check_decision(RwDir::Read, 0, PIPE_BUF, false, true, true, true, 10, 0),
            PipeCheckVerdict::Reject(PipeError::Again)
        );
        // Empty + no writer: end of stream, zero bytes (`pipe.c:232`).
        assert_eq!(
            pipe_check_decision(RwDir::Read, 0, PIPE_BUF, false, false, false, true, 10, 0),
            PipeCheckVerdict::Allow {
                bytes: 0,
                wake: none
            }
        );
        // Sleeping writers get poked even on the suspend path (`229-230`).
        let writers = WakePlan {
            wake_readers: false,
            wake_writers: true,
        };
        assert_eq!(
            pipe_check_decision(RwDir::Read, 0, PIPE_BUF, false, true, false, true, 10, 2),
            PipeCheckVerdict::Suspend(writers)
        );
        // …and the fail-fast rejection owes the same wakeup.
        assert_eq!(read_reject_wake(2), writers);
        assert_eq!(read_reject_wake(0), none);
        // Check-only mode still wakes (`notouch` ignored here).
        assert_eq!(
            pipe_check_decision(RwDir::Read, 0, PIPE_BUF, false, true, false, false, 10, 2),
            PipeCheckVerdict::Suspend(writers)
        );
    }

    #[test]
    fn test_write_sizing_matrix() {
        let none = WakePlan::default();
        let readers = WakePlan {
            wake_readers: true,
            wake_writers: false,
        };
        // No reader: broken pipe (`pipe.c:238-240`).
        assert_eq!(
            pipe_check_decision(RwDir::Write, 0, PIPE_BUF, false, true, false, true, 10, 0),
            PipeCheckVerdict::Reject(PipeError::Pipe)
        );
        assert_eq!(PipeError::Pipe.to_errno(), minix_types::EPIPE);
        // Fits in empty pipe: full allowance plus a reader poke (`282-284`).
        assert_eq!(
            pipe_check_decision(RwDir::Write, 0, PIPE_BUF, true, false, false, true, 10, 3),
            PipeCheckVerdict::Allow {
                bytes: 10,
                wake: readers
            }
        );
        // Fits in non-empty pipe: allowance alone, nobody new to wake.
        assert_eq!(
            pipe_check_decision(RwDir::Write, 5, PIPE_BUF, true, false, false, true, 10, 3),
            PipeCheckVerdict::Allow {
                bytes: 10,
                wake: none
            }
        );
        // Check-only mode suppresses the poke.
        assert_eq!(
            pipe_check_decision(RwDir::Write, 0, PIPE_BUF, true, false, false, false, 10, 3),
            PipeCheckVerdict::Allow {
                bytes: 10,
                wake: none
            }
        );
        // Over capacity, nonblocking, atomic-sized: fail fast (`245-248`).
        assert_eq!(
            pipe_check_decision(
                RwDir::Write,
                PIPE_BUF,
                PIPE_BUF,
                true,
                false,
                true,
                true,
                100,
                0
            ),
            PipeCheckVerdict::Reject(PipeError::Again)
        );
        // Over capacity, nonblocking, non-atomic-sized: partial (`251-257`).
        // (requested must exceed PIPE_BUF; atomic-sized requests fail fast.)
        assert_eq!(
            pipe_check_decision(
                RwDir::Write,
                PIPE_BUF - 10,
                PIPE_BUF,
                true,
                false,
                true,
                true,
                PIPE_BUF + 100,
                0
            ),
            PipeCheckVerdict::Allow {
                bytes: 10,
                wake: readers
            }
        );
        // Full pipe, nonblocking: EAGAIN (`258-261`).
        assert_eq!(
            pipe_check_decision(
                RwDir::Write,
                PIPE_BUF,
                PIPE_BUF,
                true,
                false,
                true,
                true,
                PIPE_BUF + 1,
                0
            ),
            PipeCheckVerdict::Reject(PipeError::Again)
        );
        assert_eq!(PipeError::Again.to_errno(), minix_types::EAGAIN);
        // Blocking partial/full: suspend (with reader wake when partial).
        assert_eq!(
            pipe_check_decision(
                RwDir::Write,
                PIPE_BUF - 10,
                PIPE_BUF,
                true,
                false,
                false,
                true,
                PIPE_BUF,
                0
            ),
            PipeCheckVerdict::Suspend(readers)
        );
        assert_eq!(
            pipe_check_decision(
                RwDir::Write,
                PIPE_BUF,
                PIPE_BUF,
                true,
                false,
                false,
                true,
                1,
                0
            ),
            PipeCheckVerdict::Suspend(WakePlan::default())
        );
        // Peek never reaches the sizer (`rw_pipe:340` assert arm).
        assert_eq!(
            pipe_check_decision(RwDir::Peek, 0, PIPE_BUF, true, true, false, true, 10, 0),
            PipeCheckVerdict::Reject(PipeError::Inval)
        );
    }

    #[test]
    fn test_susp_ledgers() {
        // Only pipe suspensions feed the counter (`pipe.c:304-306`).
        assert_eq!(susp_delta(SuspendKind::Pipe), 1);
        assert_eq!(susp_delta(SuspendKind::PipeOpen), 1);
        assert_eq!(susp_delta(SuspendKind::Other), 0);
        let mut susp = SuspLedger::new();
        susp.inc(true);
        susp.inc(false);
        assert_eq!(susp.get(), 1);
        assert!(susp.dec_checked());
        assert!(!susp.dec_checked());
        let mut rev = ReviveLedger::new();
        rev.inc();
        assert_eq!(rev.get(), 1);
        assert!(rev.dec_checked());
        assert!(!rev.dec_checked());
    }

    #[test]
    fn test_suspend_record_reuses_pipe_block() {
        // `pipe_suspend` five fields land in the authoritative struct.
        let rec = suspend_record(PipeIo::Write, 3, VirBytes(0x1000), 512, 128);
        assert_eq!(rec.call, PipeIo::Write);
        assert_eq!(rec.fd, 3);
        assert_eq!(rec.nbytes, 512);
        assert_eq!(rec.cum_io, 128);
    }

    #[test]
    fn test_release_match_conjunction() {
        let io_r = WakeOp::Io(PipeIo::Read);
        // Full match: live, pipe-read waiter, unrevived, valid filp, same vnode.
        assert!(release_match(
            true,
            BlockKind::Pipe(PipeIo::Read),
            io_r,
            false,
            true,
            true
        ));
        // Call mismatch, revived, dead, bad filp, foreign vnode: no match.
        assert!(!release_match(
            true,
            BlockKind::Pipe(PipeIo::Write),
            io_r,
            false,
            true,
            true
        ));
        assert!(!release_match(
            true,
            BlockKind::Pipe(PipeIo::Read),
            io_r,
            true,
            true,
            true
        ));
        assert!(!release_match(
            false,
            BlockKind::Pipe(PipeIo::Read),
            io_r,
            false,
            true,
            true
        ));
        assert!(!release_match(
            true,
            BlockKind::Pipe(PipeIo::Read),
            io_r,
            false,
            false,
            true
        ));
        assert!(!release_match(
            true,
            BlockKind::Pipe(PipeIo::Read),
            io_r,
            false,
            true,
            false
        ));
        // Open matches only pipe-open waiters (`pipe.c:405`).
        assert!(release_match(
            true,
            BlockKind::PipeOpen,
            WakeOp::Open,
            false,
            true,
            true
        ));
        assert!(!release_match(
            true,
            BlockKind::Pipe(PipeIo::Read),
            WakeOp::Open,
            false,
            true,
            true
        ));
        assert!(!release_match(
            true,
            BlockKind::Other,
            io_r,
            false,
            true,
            true
        ));
        // Select bit clearing (`pipe.c:392`).
        assert_eq!(select_ack(0b11, 0b01), 0b10);
        assert_eq!(select_ack(0b10, 0b01), 0b10);
    }

    #[test]
    fn test_revive_verdicts() {
        // Gates shut: bad endpoint, no state, already revived.
        assert_eq!(
            revive_decision(false, Some(BlockKindEx::Pipe), false, 0, 0),
            ReviveVerdict::Noop
        );
        assert_eq!(
            revive_decision(true, None, false, 0, 0),
            ReviveVerdict::Noop
        );
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Pipe), true, 0, 0),
            ReviveVerdict::Noop
        );
        // Pipe/lock: deferred mark (`pipe.c:456-459`).
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Pipe), false, 0, 0),
            ReviveVerdict::MarkReviving
        );
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Flock), false, 0, 0),
            ReviveVerdict::MarkReviving
        );
        // Pipe-open replies the fd; select/cdev reply the code.
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::PipeOpen), false, 7, 0),
            ReviveVerdict::Reply(7)
        );
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Select), false, 0, 3),
            ReviveVerdict::Reply(3)
        );
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Cdev), false, 0, 0),
            ReviveVerdict::Reply(0)
        );
        // Socket/unknown: defensive invalid (C panics).
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Sdev), false, 0, 0),
            ReviveVerdict::Invalid
        );
        assert_eq!(
            revive_decision(true, Some(BlockKindEx::Unknown), false, 0, 0),
            ReviveVerdict::Invalid
        );
    }

    #[test]
    fn test_unpause_plans() {
        // Partial pipe progress beats EINTR (`pipe.c:527-528`).
        let p = unpause_decision(BlockKindEx::Pipe, 64, false);
        assert_eq!(p.reply, UnpauseReply::Bytes(64));
        assert_eq!(p.cancel, CancelOp::None);
        assert!(p.dec_susp);
        assert!(!p.dec_reviving);
        // No progress: EINTR, still counts down.
        let p = unpause_decision(BlockKindEx::Pipe, 0, false);
        assert_eq!(p.reply, UnpauseReply::Intr);
        assert!(p.dec_susp);
        // Already reviving: shift the decrement to the other ledger.
        let p = unpause_decision(BlockKindEx::PipeOpen, 0, true);
        assert!(!p.dec_susp);
        assert!(p.dec_reviving);
        assert_eq!(PipeError::Intr.to_errno(), minix_types::EINTR);
        // Select forgets, cdev cancels, sdev self-replies.
        assert_eq!(
            unpause_decision(BlockKindEx::Select, 0, false).cancel,
            CancelOp::ForgetSelect
        );
        assert_eq!(
            unpause_decision(BlockKindEx::Cdev, 0, false).cancel,
            CancelOp::CancelCdev
        );
        assert_eq!(
            unpause_decision(BlockKindEx::Sdev, 0, false).cancel,
            CancelOp::CancelSdev
        );
        assert_eq!(
            unpause_decision(BlockKindEx::Flock, 0, false).reply,
            UnpauseReply::Intr
        );
    }

    #[test]
    fn test_map_verdicts() {
        // Mapped short-circuits (`pipe.c:157`).
        assert_eq!(map_decision(true, true, false), MapVerdict::AlreadyMapped);
        // Missing endpoint is absent (C panics; ARCH hardening).
        assert_eq!(map_decision(false, false, false), MapVerdict::Absent);
        // Busy skips the unlock (`pipe.c:162-163`).
        assert_eq!(
            map_decision(false, true, true),
            MapVerdict::Proceed(UnlockNote::SkipUnlock)
        );
        assert_eq!(
            map_decision(false, true, false),
            MapVerdict::Proceed(UnlockNote::Unlock)
        );
    }

    #[test]
    fn test_driver_waiter_classification() {
        assert_eq!(classify_driver_waiter(true, false), DriverWake::ReviveEio);
        assert_eq!(classify_driver_waiter(false, true), DriverWake::StopSdev);
        assert_eq!(classify_driver_waiter(false, false), DriverWake::Ignore);
        // Char match wins ties (checked first, `pipe.c:344`).
        assert_eq!(classify_driver_waiter(true, true), DriverWake::ReviveEio);
    }

    #[test]
    fn test_errno_map_covers_pipe_c() {
        let cases = [
            (PipeError::Again, minix_types::EAGAIN),
            (PipeError::Pipe, minix_types::EPIPE),
            (PipeError::Io, minix_types::EIO),
            (PipeError::Intr, minix_types::EINTR),
            (PipeError::Inval, minix_types::EINVAL),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
