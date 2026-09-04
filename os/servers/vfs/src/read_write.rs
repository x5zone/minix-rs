//! `read_write` — the shared read/write/peek pipeline and its guards.
//!
//! Corresponds to Minix3's `read.c:1-393` (`do_read`, `lock_bsf/unlock_bsf`,
//! `check_bsf_lock`, `actual_read_write_peek`, `do_read_write_peek`,
//! `read_write`, `do_getdents`, `rw_pipe`) and `write.c:1-25` (`do_write`),
//! with `bsf_lock` declared in `glo.h:36` and the direction constants in
//! `minix3/minix/include/minix/const.h:77-79`.
//!
//! Design decisions (see 16-read-write.md §3):
//! - `RwDir` enum replaces the `READING/WRITING/PEEKING` integers
//! - `BsfLock` trait isolates the global block-device serialization
//! - `validate_head` converges the lock/mode/zero gate trio
//! - `select_route` is a pure five-way dispatch verdict
//! - `apply_append/grow_size` make position advance explicit
//! - `should_signal_pipe/finish` tabulate the tail
//! - pipe sizing math (`pipe_chunk/pipe_apply/after_partial`) stays here while
//!   execution (`pipe_check/pipe_suspend`) belongs to 17-pipe.md

use core::cell::Cell;

use crate::open::{FileType, R_BIT, W_BIT};

/// `READING` (`const.h:77`): copy data to user.
pub const READING: i32 = 0;
/// `WRITING` (`const.h:78`): copy data from user.
pub const WRITING: i32 = 1;
/// `PEEKING` (`const.h:79`): retrieve FS data without copying.
pub const PEEKING: i32 = 2;

/// `NO_DEV`: no device attached (`v_sdev` sentinel for character, socket,
/// and block vnodes; `read.c:168,208,214` panic sites).
pub const NO_DEV: u64 = u64::MAX;

/// `SSIZE_MAX` on 64-bit (`minix3/sys/sys/common_limits.h:55`
/// `LONG_MAX`): largest single transfer.
pub const SSIZE_MAX: u64 = i64::MAX as u64;

/// Transfer direction decoded from `rw_flag`.
///
/// Replaces the bare `READING/WRITING/PEEKING` integers (`const.h:77-79`):
/// any other value is unrepresentable here, so the `assert` at
/// `read.c:150` becomes a conversion failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RwDir {
    /// `READING`: copy data to user.
    Read,
    /// `WRITING`: copy data from user.
    Write,
    /// `PEEKING`: retrieve FS data without copying or advancing.
    Peek,
}

impl TryFrom<i32> for RwDir {
    type Error = IoError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            READING => Ok(Self::Read),
            WRITING => Ok(Self::Write),
            PEEKING => Ok(Self::Peek),
            _ => Err(IoError::Inval),
        }
    }
}

impl RwDir {
    /// Whether this direction mutates stored bytes.
    pub fn wants_write(self) -> bool {
        matches!(self, Self::Write)
    }

    /// Vnode lock flavour `actual_read_write_peek` takes (`read.c:103`):
    /// writers lock exclusively, everyone else shares.
    pub fn lock_kind(self) -> LockKind {
        match self {
            Self::Write => LockKind::Write,
            Self::Read | Self::Peek => LockKind::Read,
        }
    }
}

/// Vnode lock flavour for the transfer head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockKind {
    /// `VNODE_READ`: shared.
    Read,
    /// `VNODE_WRITE`: exclusive.
    Write,
}

/// How the `bsf_lock` was acquired (`lock_bsf`, `read.c:49-62`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsfPath {
    /// `mutex_trylock` succeeded immediately (`read.c:53-54`).
    Fast,
    /// Try failed: suspend, blocking lock, resume (`read.c:56-61`).
    Slow,
}

/// Global block-special-file serialization (`glo.h:36`).
///
/// Under the single-threaded event loop (ARCH A-1) the mutex degrades to
/// a held bit plus a suspend hook; the trait keeps pairing and the slow
/// path observable without a real mutex.
pub trait BsfLock {
    /// Non-blocking attempt (`mutex_trylock`).
    fn try_acquire(&self) -> bool;
    /// Blocking acquisition after suspension (`mutex_lock` past `worker_suspend`).
    fn resume_acquire(&self);
    /// Release (`mutex_unlock`).
    fn release(&self);
    /// Whether currently held (for `check_bsf_lock` parity).
    fn is_held(&self) -> bool;
}

/// Test lock whose try always succeeds (fast-path behaviour).
#[derive(Debug, Default)]
pub struct ImmediateBsf {
    held: Cell<bool>,
}

impl ImmediateBsf {
    /// New unheld lock.
    pub fn new() -> Self {
        Self {
            held: Cell::new(false),
        }
    }
}

impl BsfLock for ImmediateBsf {
    fn try_acquire(&self) -> bool {
        self.held.set(true);
        true
    }
    fn resume_acquire(&self) {
        self.held.set(true);
    }
    fn release(&self) {
        self.held.set(false);
    }
    fn is_held(&self) -> bool {
        self.held.get()
    }
}

/// Test lock whose first try fails, forcing the slow path.
///
/// Behaves differently from [`ImmediateBsf`] (fast vs slow acquisition),
/// satisfying the "two behaviorally different impls" rule for traits.
#[derive(Debug, Default)]
pub struct ContendedBsf {
    held: Cell<bool>,
    tries: Cell<u32>,
    slow_entries: Cell<u32>,
}

impl ContendedBsf {
    /// New unheld lock.
    pub fn new() -> Self {
        Self {
            held: Cell::new(false),
            tries: Cell::new(0),
            slow_entries: Cell::new(0),
        }
    }
    /// How many slow-path entries happened.
    pub fn slow_entries(&self) -> u32 {
        self.slow_entries.get()
    }
}

impl BsfLock for ContendedBsf {
    fn try_acquire(&self) -> bool {
        self.tries.set(self.tries.get() + 1);
        false
    }
    fn resume_acquire(&self) {
        self.slow_entries.set(self.slow_entries.get() + 1);
        self.held.set(true);
    }
    fn release(&self) {
        self.held.set(false);
    }
    fn is_held(&self) -> bool {
        self.held.get()
    }
}

/// Acquire the `bsf_lock` over an abstract lock (`lock_bsf` core).
pub fn bsf_acquire<L: BsfLock>(lock: &L) -> BsfPath {
    if lock.try_acquire() {
        BsfPath::Fast
    } else {
        // `worker_suspend()` … `mutex_lock()` … `worker_resume()`:
        // suspension itself is executed by 08-worker-thread.md.
        lock.resume_acquire();
        BsfPath::Slow
    }
}

/// RAII guard pairing `bsf` acquire with release.
///
/// Guarantees the `unlock_bsf` at `read.c:230` runs on every path out of
/// the block branch, including early error returns.
pub struct BsfGuard<'a, L: BsfLock> {
    lock: &'a L,
}

impl<'a, L: BsfLock> BsfGuard<'a, L> {
    /// Acquire (fast or slow) and guard the critical section.
    pub fn acquire(lock: &'a L) -> (Self, BsfPath) {
        let path = bsf_acquire(lock);
        (Self { lock }, path)
    }
}

impl<L: BsfLock> Drop for BsfGuard<'_, L> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

/// Verdict of `check_bsf_lock` (`read.c:76-87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsfCheck {
    /// Try succeeded so the lock is free; `unlock_bsf` already conceptually ran.
    Free,
}

/// Assert the `bsf_lock` is free (`check_bsf_lock`, `read.c:76-87`).
///
/// `r == -EBUSY → "bsf_lock locked"` panic and `r != 0 → "weird state"`
/// panic become typed rejections; the success path releases the trial hold.
pub fn check_bsf_free<L: BsfLock>(lock: &L) -> Result<BsfCheck, IoError> {
    if lock.is_held() {
        return Err(IoError::Inval);
    }
    if lock.try_acquire() {
        lock.release();
        Ok(BsfCheck::Free)
    } else {
        Err(IoError::Inval)
    }
}

/// Head verdict of `actual_read_write_peek` (`read.c:92-122`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadVerdict {
    /// Proceed with this vnode lock flavour.
    Proceed(LockKind),
    /// `nbytes == 0`: return 0 without touching drivers (`read.c:113-116`).
    Zero,
}

/// Converged head gate: lock flavour + mode bit + zero short-circuit.
///
/// - `mode_bits`: `filp_mode` (`R_BIT`/`W_BIT` from open).
/// - `EBADF` when the direction's bit is absent (`read.c:109-112`).
/// - Zero-length transfers short-circuit before dispatch (`read.c:113`).
pub fn validate_head(mode_bits: u32, dir: RwDir, nbytes: u64) -> Result<HeadVerdict, IoError> {
    let need = if dir.wants_write() { W_BIT } else { R_BIT };
    if mode_bits & need == 0 {
        return Err(IoError::BadF);
    }
    if nbytes == 0 {
        return Ok(HeadVerdict::Zero);
    }
    Ok(HeadVerdict::Proceed(dir.lock_kind()))
}

/// Guard shared by `do_read/do_write/do_getdents` (`read.c:38,292` +
/// `write.c:20`): the `cum_io` message field must be zero on entry.
pub fn check_cum_io_zero(cum_io: u64) -> Result<(), IoError> {
    if cum_io != 0 {
        return Err(IoError::Inval);
    }
    Ok(())
}

/// Largest single transfer (`read.c:152`): `size > SSIZE_MAX → EINVAL`.
pub fn check_size(size: u64) -> Result<(), IoError> {
    if size > SSIZE_MAX {
        return Err(IoError::Inval);
    }
    Ok(())
}

/// Pure five-way dispatch verdict (`read_write`, `read.c:154-251`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoRoute {
    /// `S_ISFIFO`: `rw_pipe` (17-pipe.md executes).
    Pipe,
    /// `S_ISCHR`: `cdev_io` (21-cdev.md executes).
    Char,
    /// `S_ISSOCK`: `sdev_readwrite` (22-sdev.md executes).
    Sock,
    /// `S_ISBLK`: `req_bpeek/req_breadwrite` under `bsf` (12 executes).
    Block,
    /// Regular file or directory: `req_peek/req_readwrite` (12 executes).
    Regular,
    /// Peek on pipe/char/sock makes no sense (`read.c:157,165,205`).
    Reject(IoError),
}

/// Pure dispatch over file type and direction (`read.c:154-251`).
///
/// Driver calls stay with 12/17/20-22; this only decides *where* a
/// transfer goes, including the three peek rejections.
pub fn select_route(ft: FileType, dir: RwDir) -> IoRoute {
    let peek = dir == RwDir::Peek;
    match ft {
        FileType::Fifo => {
            if peek {
                IoRoute::Reject(IoError::Inval)
            } else {
                IoRoute::Pipe
            }
        }
        FileType::Char => {
            if peek {
                IoRoute::Reject(IoError::Inval)
            } else {
                IoRoute::Char
            }
        }
        FileType::Socket => {
            if peek {
                IoRoute::Reject(IoError::Inval)
            } else {
                IoRoute::Sock
            }
        }
        FileType::Block => IoRoute::Block,
        FileType::Regular | FileType::Directory => IoRoute::Regular,
        FileType::Unknown(_) => IoRoute::Reject(IoError::Io),
    }
}

/// Device presence check (`read.c:168-169,208-209,214-215`).
///
/// C panics on `NO_DEV` ("tries to access … NO_DEV"); here that
/// impossible path becomes a defensive `ENXIO` (ARCH hardening, D4).
pub fn check_dev(dev: u64) -> Result<u64, IoError> {
    if dev == NO_DEV {
        return Err(IoError::NoDev);
    }
    Ok(dev)
}

/// `O_APPEND` repositioning (`read.c:232-235`): writes start at end of
/// file; reads and non-append transfers keep the current position.
pub fn apply_append(pos: u64, size: u64, appending: bool, writing: bool) -> u64 {
    if writing && appending { size } else { pos }
}

/// Post-write size growth (`read.c:254-260`): only regular files and
/// directories grow, and only on writes past the end.
pub fn grow_size(size: u64, pos: u64, growable: bool, writing: bool) -> u64 {
    if writing && growable && pos > size {
        pos
    } else {
        size
    }
}

/// Whether a broken pipe must raise `SIGPIPE` (`read.c:264-271`).
///
/// Three-way matrix: error is `EPIPE` × direction is write × caller did
/// not set `O_NOSIGPIPE`. The `sys_kill` itself stays with the caller.
pub fn should_signal_pipe(err: IoError, writing: bool, nosigpipe: bool) -> bool {
    err == IoError::Pipe && writing && !nosigpipe
}

/// Tail mapping (`read.c:273-276`): `OK → cum_io`, anything else passes through.
pub fn finish(r: Result<(), IoError>, cum_io: u64) -> Result<u64, IoError> {
    r.map(|()| cum_io)
}

/// Clamp a pipe transfer to what is available (`rw_pipe`, `read.c:342-360`).
///
/// - `avail`: `pipe_check` allowance for this round.
/// - `requested`: remaining `nbytes`.
/// - `buffered`: `v_size` bytes currently in the pipe (reads only).
pub fn pipe_chunk(avail: u64, requested: u64, buffered: u64, reading: bool) -> u64 {
    let mut size = avail.min(requested);
    if reading {
        size = size.min(buffered);
    }
    size
}

/// Apply a completed pipe chunk to the buffered count (`read.c:377-380`).
pub fn pipe_apply(buffered: u64, moved: u64, reading: bool) -> u64 {
    if reading {
        buffered.saturating_sub(moved)
    } else {
        buffered.saturating_add(moved)
    }
}

/// Verdict after a partial pipe round (`read.c:382-390`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialVerdict {
    /// Non-blocking or complete: hand back the count so far.
    ReturnCount,
    /// Blocking partial of a large write: suspend for the rest.
    Suspend,
}

/// Partial-round verdict: a short round suspends only when more remains
/// *and* the fd is blocking (`read.c:385`: `!(oflags & O_NONBLOCK)`).
pub fn after_partial(partial: bool, nonblock: bool) -> PartialVerdict {
    if partial && !nonblock {
        PartialVerdict::Suspend
    } else {
        PartialVerdict::ReturnCount
    }
}

/// `do_getdents` gate (`read.c:300-306`): valid fd + read bit + directory.
///
/// Both failures report `EBADF` (a non-directory is "the wrong kind of
/// readable", not `ENOTDIR` — the lookup already succeeded).
pub fn validate_getdents(mode_bits: u32, is_dir: bool) -> Result<(), IoError> {
    if mode_bits & R_BIT == 0 {
        return Err(IoError::BadF);
    }
    if !is_dir {
        return Err(IoError::BadF);
    }
    Ok(())
}

/// Advance a directory position after entries were returned
/// (`read.c:312`): only positive counts move the offset.
pub fn advance_getdents(pos: u64, moved: i64, new_pos: u64) -> u64 {
    if moved > 0 { new_pos } else { pos }
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// No invented codes: every variant names the errno it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    /// `EINVAL`: nonzero `cum_io`, oversize transfer, bad direction, peek misuse.
    Inval,
    /// `EBADF`: direction bit absent, getdents on unreadable/non-directory.
    BadF,
    /// `EPIPE`: write with no reader (also arms `SIGPIPE`).
    Pipe,
    /// `ENXIO`: deviceless vnode (C panics; ARCH hardening, D4).
    NoDev,
    /// `EOPNOTSUPP`: reserved rejection slot for dispatch.
    NotSup,
    /// `EIO`: unknown file type / driver error归一.
    Io,
}

impl IoError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Inval => minix_types::EINVAL,
            Self::BadF => minix_types::EBADF,
            Self::Pipe => minix_types::EPIPE,
            Self::NoDev => minix_types::ENXIO,
            Self::NotSup => minix_types::EOPNOTSUPP,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::FileType;

    #[test]
    fn test_direction_decode_fourth_rejected() {
        // `const.h:77-79` parity: 0→Read 1→Write 2→Peek, else EINVAL.
        assert_eq!(RwDir::try_from(READING).unwrap(), RwDir::Read);
        assert_eq!(RwDir::try_from(WRITING).unwrap(), RwDir::Write);
        assert_eq!(RwDir::try_from(PEEKING).unwrap(), RwDir::Peek);
        assert_eq!(RwDir::try_from(3).unwrap_err(), IoError::Inval);
        assert_eq!(RwDir::try_from(-1).unwrap_err(), IoError::Inval);
        // Lock flavour split (`read.c:103`): writers exclusive.
        assert_eq!(RwDir::Write.lock_kind(), LockKind::Write);
        assert_eq!(RwDir::Read.lock_kind(), LockKind::Read);
        assert_eq!(RwDir::Peek.lock_kind(), LockKind::Read);
        assert!(RwDir::Write.wants_write());
        assert!(!RwDir::Peek.wants_write());
    }

    #[test]
    fn test_entry_zero_guard() {
        // `do_read/do_write/do_getdents` reject nonzero `cum_io`.
        assert!(check_cum_io_zero(0).is_ok());
        assert_eq!(check_cum_io_zero(7).unwrap_err(), IoError::Inval);
        assert_eq!(IoError::Inval.to_errno(), minix_types::EINVAL);
        // `size > SSIZE_MAX → EINVAL` (`read.c:152`).
        assert!(check_size(0).is_ok());
        assert!(check_size(SSIZE_MAX).is_ok());
        assert_eq!(check_size(SSIZE_MAX + 1).unwrap_err(), IoError::Inval);
    }

    #[test]
    fn test_head_gate_matrix() {
        // Readable fd, nonzero → proceed shared.
        assert_eq!(
            validate_head(R_BIT, RwDir::Read, 10).unwrap(),
            HeadVerdict::Proceed(LockKind::Read)
        );
        // Writable fd, write → proceed exclusive.
        assert_eq!(
            validate_head(W_BIT, RwDir::Write, 10).unwrap(),
            HeadVerdict::Proceed(LockKind::Write)
        );
        // Missing direction bit → EBADF (`read.c:109-112`).
        assert_eq!(
            validate_head(R_BIT, RwDir::Write, 10).unwrap_err(),
            IoError::BadF
        );
        assert_eq!(
            validate_head(W_BIT, RwDir::Read, 10).unwrap_err(),
            IoError::BadF
        );
        assert_eq!(IoError::BadF.to_errno(), minix_types::EBADF);
        // Zero bytes short-circuit before dispatch (`read.c:113-116`).
        assert_eq!(
            validate_head(R_BIT, RwDir::Read, 0).unwrap(),
            HeadVerdict::Zero
        );
        // …but only after the mode check: unreadable + zero still EBADF.
        assert_eq!(
            validate_head(W_BIT, RwDir::Read, 0).unwrap_err(),
            IoError::BadF
        );
    }

    #[test]
    fn test_route_five_way_and_peek_rejections() {
        // Five destinations (`read.c:154,162,202,213,231`).
        assert_eq!(select_route(FileType::Fifo, RwDir::Read), IoRoute::Pipe);
        assert_eq!(select_route(FileType::Char, RwDir::Write), IoRoute::Char);
        assert_eq!(select_route(FileType::Socket, RwDir::Read), IoRoute::Sock);
        assert_eq!(select_route(FileType::Block, RwDir::Write), IoRoute::Block);
        assert_eq!(
            select_route(FileType::Regular, RwDir::Read),
            IoRoute::Regular
        );
        assert_eq!(
            select_route(FileType::Directory, RwDir::Read),
            IoRoute::Regular
        );
        // Peek rejected on pipe/char/sock (`read.c:155-157,163-166,203-206`).
        assert_eq!(
            select_route(FileType::Fifo, RwDir::Peek),
            IoRoute::Reject(IoError::Inval)
        );
        assert_eq!(
            select_route(FileType::Char, RwDir::Peek),
            IoRoute::Reject(IoError::Inval)
        );
        assert_eq!(
            select_route(FileType::Socket, RwDir::Peek),
            IoRoute::Reject(IoError::Inval)
        );
        // …but allowed on block and regular (`read.c:219,238`).
        assert_eq!(select_route(FileType::Block, RwDir::Peek), IoRoute::Block);
        assert_eq!(
            select_route(FileType::Regular, RwDir::Peek),
            IoRoute::Regular
        );
        // Unknown type → EIO (`open.c:273` family behaviour).
        assert_eq!(
            select_route(FileType::Unknown(0), RwDir::Read),
            IoRoute::Reject(IoError::Io)
        );
    }

    #[test]
    fn test_bsf_fast_and_slow_paths() {
        // Immediate lock: fast path, guard releases on drop.
        let fast = ImmediateBsf::new();
        assert_eq!(bsf_acquire(&fast), BsfPath::Fast);
        fast.release();
        {
            let (_g, path) = BsfGuard::acquire(&fast);
            assert_eq!(path, BsfPath::Fast);
            assert!(fast.is_held());
        }
        assert!(!fast.is_held());
        // Contended lock: slow path recorded, still paired.
        let slow = ContendedBsf::new();
        assert_eq!(bsf_acquire(&slow), BsfPath::Slow);
        assert_eq!(slow.slow_entries(), 1);
        slow.release();
        {
            let (_g, path) = BsfGuard::acquire(&slow);
            assert_eq!(path, BsfPath::Slow);
        }
        assert!(!slow.is_held());
        assert_eq!(slow.slow_entries(), 2);
        // Polymorphic use through the trait bound (Gate D).
        fn via<L: BsfLock>(l: &L) -> BsfPath {
            bsf_acquire(l)
        }
        assert_eq!(via(&ImmediateBsf::new()), BsfPath::Fast);
        assert_eq!(via(&ContendedBsf::new()), BsfPath::Slow);
    }

    #[test]
    fn test_bsf_check_free() {
        // Free lock checks out (`check_bsf_lock` success path).
        let free = ImmediateBsf::new();
        assert_eq!(check_bsf_free(&free).unwrap(), BsfCheck::Free);
        assert!(!free.is_held());
        // Held lock is rejected (C would panic "bsf_lock locked").
        let held = ImmediateBsf::new();
        held.try_acquire();
        assert_eq!(check_bsf_free(&held).unwrap_err(), IoError::Inval);
    }

    #[test]
    fn test_position_advance_trio() {
        // Append repositions writes only (`read.c:232-235`).
        assert_eq!(apply_append(100, 1000, true, true), 1000);
        assert_eq!(apply_append(100, 1000, true, false), 100);
        assert_eq!(apply_append(100, 1000, false, true), 100);
        // Growth needs write + growable + past-end (`read.c:254-260`).
        assert_eq!(grow_size(1000, 1200, true, true), 1200);
        assert_eq!(grow_size(1000, 800, true, true), 1000);
        assert_eq!(grow_size(1000, 1200, true, false), 1000);
        assert_eq!(grow_size(1000, 1200, false, true), 1000);
        // Tail: OK carries the count, errors pass through (`read.c:273-276`).
        assert_eq!(finish(Ok(()), 41).unwrap(), 41);
        assert_eq!(finish(Err(IoError::Pipe), 41).unwrap_err(), IoError::Pipe);
    }

    #[test]
    fn test_sigpipe_matrix() {
        // EPIPE × write × no NOSIGPIPE → signal (`read.c:264-271`).
        assert!(should_signal_pipe(IoError::Pipe, true, false));
        assert!(!should_signal_pipe(IoError::Pipe, true, true));
        assert!(!should_signal_pipe(IoError::Pipe, false, false));
        assert!(!should_signal_pipe(IoError::Inval, true, false));
        assert_eq!(IoError::Pipe.to_errno(), minix_types::EPIPE);
    }

    #[test]
    fn test_no_dev_hardening() {
        // Real devices pass; NO_DEV becomes ENXIO (C panics).
        assert_eq!(check_dev(0x0401).unwrap(), 0x0401);
        assert_eq!(check_dev(NO_DEV).unwrap_err(), IoError::NoDev);
        assert_eq!(IoError::NoDev.to_errno(), minix_types::ENXIO);
    }

    #[test]
    fn test_pipe_math() {
        // Clamp to allowance, request, and buffer (`read.c:354-360`).
        assert_eq!(pipe_chunk(100, 1000, 500, true), 100);
        assert_eq!(pipe_chunk(1000, 100, 500, true), 100);
        assert_eq!(pipe_chunk(1000, 1000, 40, true), 40);
        // Writes ignore the buffer level.
        assert_eq!(pipe_chunk(100, 1000, 0, false), 100);
        // Buffer accounting follows direction (`read.c:377-380`).
        assert_eq!(pipe_apply(500, 100, true), 400);
        assert_eq!(pipe_apply(500, 100, false), 600);
        assert_eq!(pipe_apply(50, 100, true), 0);
        // Partial blocking rounds suspend; nonblocking returns (`read.c:382-390`).
        assert_eq!(after_partial(true, false), PartialVerdict::Suspend);
        assert_eq!(after_partial(true, true), PartialVerdict::ReturnCount);
        assert_eq!(after_partial(false, false), PartialVerdict::ReturnCount);
    }

    #[test]
    fn test_getdents_gate() {
        // Readable directory proceeds (`read.c:303-306`).
        assert!(validate_getdents(R_BIT, true).is_ok());
        // Unreadable or non-directory → EBADF (both, not ENOTDIR).
        assert_eq!(validate_getdents(W_BIT, true).unwrap_err(), IoError::BadF);
        assert_eq!(validate_getdents(R_BIT, false).unwrap_err(), IoError::BadF);
        // Positive counts advance, the rest hold (`read.c:312`).
        assert_eq!(advance_getdents(10, 5, 30), 30);
        assert_eq!(advance_getdents(10, 0, 30), 10);
        assert_eq!(advance_getdents(10, -5, 30), 10);
    }

    #[test]
    fn test_errno_map_covers_read_c() {
        let cases = [
            (IoError::Inval, minix_types::EINVAL),
            (IoError::BadF, minix_types::EBADF),
            (IoError::Pipe, minix_types::EPIPE),
            (IoError::NoDev, minix_types::ENXIO),
            (IoError::NotSup, minix_types::EOPNOTSUPP),
            (IoError::Io, minix_types::EIO),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
