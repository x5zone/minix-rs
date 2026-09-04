//! `fcntl` — the remote control: dup, flag doors, and advisory record locks.
//!
//! Corresponds to Minix3's `do_fcntl` (`misc.c:117-271`), `lock_op` and
//! `lock_revive` (`lock.c:21-192`), the lock table (`lock.h:7-13`), the
//! close-time release (`open.c:690-724`), and the FLOCK resume/unblock
//! paths (`main.c:946-954`, `pipe.c:454-456,531`).
//!
//! Design decisions (see 30-fcntl-lock.md §3):
//! - `FcntlCmd` types the thirteen commands (unknown decodes to `None → EINVAL`)
//! - `dupfd_arg_check`/`cloexec_*`/`status_*`/`nosigpipe_*` type the narrow doors
//! - `LockType`/`Whence` + `lock_gate` type the five-question lock door
//! - `compute_region` types the first/last arithmetic (`checked_add`)
//! - `LockTable` + `LockOutcome` type the eight slots (grant/hit/miss/wait/unlock)
//! - `release_for` + `revive_all` type close-release and broadcast wake
//! - `FcntlFs` types the FS dialogue (`ScriptedFcntl` vs `RefusingFcntl`)
//! - `FcntlVerdict::{Done, Suspend}` types the reply intent (ARCH A-5)
//!
//! Scope note: fd allocation (`get_fd`) stays with 14-filedes.md; filp
//! fields stay with 04-filp-table.md; credential fields stay with
//! 02-fproc-struct.md; `BlockedOn::Flock` payload stays with fproc
//! (`FlockCmd::SetLkw` proves cmd is constant); FS requests (`req_ftrunc`/
//! `req_flush`) execute FS-side (12-request-wrappers.md describes the
//! envelopes). This module only decides: command, doors, region, table,
//! release, and dialogue.
//!
//! Linux models the same core as `fcntl_setlk` (conflict table over
//! `file_lock` list, `F_SETLK` → `EAGAIN`, `F_SETLKW` → wait) with `flock`
//! aside; Redox models it as handle-flag mutation over `FdTable` plus
//! advisory locks behind scheme r/w handles. Here [`LockTable`] is the
//! core and [`FcntlFs`] is the per-filesystem answer.

extern crate alloc;

use alloc::vec::Vec;

use minix_types::Endpoint;

use crate::fproc::OPEN_MAX;
use crate::open::FileType;
use crate::protect::{R_BIT, W_BIT};

/// `F_DUPFD` (`minix3/sys/sys/fcntl.h:178`): duplicate fd above a floor.
pub const F_DUPFD: u32 = 0;
/// `F_GETFD` (`fcntl.h:179`): read the cloexec flag.
pub const F_GETFD: u32 = 1;
/// `F_SETFD` (`fcntl.h:180`): write the cloexec flag.
pub const F_SETFD: u32 = 2;
/// `F_GETFL` (`fcntl.h:181`): read status flags.
pub const F_GETFL: u32 = 3;
/// `F_SETFL` (`fcntl.h:182`): write status flags (narrow door).
pub const F_SETFL: u32 = 4;
/// `F_GETLK` (`fcntl.h:188`): query a record lock.
pub const F_GETLK: u32 = 7;
/// `F_SETLK` (`fcntl.h:189`): set a record lock, refuse if blocked.
pub const F_SETLK: u32 = 8;
/// `F_SETLKW` (`fcntl.h:190`): set a record lock, wait if blocked.
pub const F_SETLKW: u32 = 9;
/// `F_DUPFD_CLOEXEC` (`fcntl.h:194`): duplicate fd, cloexec set.
pub const F_DUPFD_CLOEXEC: u32 = 12;
/// `F_GETNOSIGPIPE` (`fcntl.h:195`): read the nosigpipe sentinel.
pub const F_GETNOSIGPIPE: u32 = 13;
/// `F_SETNOSIGPIPE` (`fcntl.h:196`): write the nosigpipe sentinel.
pub const F_SETNOSIGPIPE: u32 = 14;
/// `F_FREESP` (`fcntl.h:328`): punch a hole (truncate a span).
pub const F_FREESP: u32 = 100;
/// `F_FLUSH_FS_CACHE` (`fcntl.h:329`): flush the hosting FS cache.
pub const F_FLUSH_FS_CACHE: u32 = 101;

/// `F_RDLCK` (`fcntl.h:203`): shared (read) record lock.
pub const F_RDLCK: u32 = 1;
/// `F_UNLCK` (`fcntl.h:204`): unlock a region.
pub const F_UNLCK: u32 = 2;
/// `F_WRLCK` (`fcntl.h:205`): exclusive (write) record lock.
pub const F_WRLCK: u32 = 3;

/// `FD_CLOEXEC` (`fcntl.h:199`): the close-on-exec flag bit.
pub const FD_CLOEXEC: u32 = 1;

/// `O_ACCMODE` (`fcntl.h:67`): access-mode mask (read back by `F_GETFL`).
pub const O_ACCMODE: u32 = 0x0000_0003;
/// `O_NONBLOCK` (`fcntl.h:81`): non-blocking flag.
pub const O_NONBLOCK: u32 = 0x0000_0004;
/// `O_APPEND` (`fcntl.h:82`): append flag.
pub const O_APPEND: u32 = 0x0000_0008;
/// `O_NOSIGPIPE` (`fcntl.h:124`): suppress-SIGPIPE flag.
pub const O_NOSIGPIPE: u32 = 0x0100_0000;

/// `NR_LOCKS` (`minix3/minix/servers/vfs/const.h:6`): eight lock slots.
pub const NR_LOCKS: usize = 8;
/// `MAX_FILE_POS` (`minix3/minix/include/minix/const.h:124`): zero length
/// locks to the horizon.
pub const MAX_FILE_POS: i64 = 0x7FFF_FFFF;

/// `SEEK_SET` (`minix3/sys/sys/unistd.h:174`): region base is zero.
pub const SEEK_SET: u32 = 0;
/// `SEEK_CUR` (`unistd.h:175`): region base is the file position.
pub const SEEK_CUR: u32 = 1;
/// `SEEK_END` (`unistd.h:176`): region base is the file size.
pub const SEEK_END: u32 = 2;

/// The thirteen fcntl commands (`do_fcntl:135-267`).
///
/// Commands with no branch (`F_GETOWN/F_SETOWN/F_CLOSEM/F_MAXFD`, `5/6/10/11`)
/// decode to `None` — the caller answers `EINVAL` (`default:266`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FcntlCmd {
    /// `F_DUPFD`: duplicate above a floor.
    DupFd,
    /// `F_DUPFD_CLOEXEC`: duplicate above a floor, cloexec set.
    DupFdCloexec,
    /// `F_GETFD`: read cloexec.
    GetFd,
    /// `F_SETFD`: write cloexec.
    SetFd,
    /// `F_GETFL`: read status flags.
    GetFl,
    /// `F_SETFL`: write status flags (narrow door).
    SetFl,
    /// `F_GETLK`: query a lock.
    GetLk,
    /// `F_SETLK`: set a lock, refuse if blocked.
    SetLk,
    /// `F_SETLKW`: set a lock, wait if blocked.
    SetLkw,
    /// `F_FREESP`: punch a hole.
    FreeSp,
    /// `F_GETNOSIGPIPE`: read the nosigpipe sentinel.
    GetNoSigPipe,
    /// `F_SETNOSIGPIPE`: write the nosigpipe sentinel.
    SetNoSigPipe,
    /// `F_FLUSH_FS_CACHE`: flush the hosting FS cache.
    FlushFsCache,
}

impl FcntlCmd {
    /// Decode a wire command; unknown values refuse (`default:266`).
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            F_DUPFD => Some(Self::DupFd),
            F_GETFD => Some(Self::GetFd),
            F_SETFD => Some(Self::SetFd),
            F_GETFL => Some(Self::GetFl),
            F_SETFL => Some(Self::SetFl),
            F_GETLK => Some(Self::GetLk),
            F_SETLK => Some(Self::SetLk),
            F_SETLKW => Some(Self::SetLkw),
            F_DUPFD_CLOEXEC => Some(Self::DupFdCloexec),
            F_GETNOSIGPIPE => Some(Self::GetNoSigPipe),
            F_SETNOSIGPIPE => Some(Self::SetNoSigPipe),
            F_FREESP => Some(Self::FreeSp),
            F_FLUSH_FS_CACHE => Some(Self::FlushFsCache),
            _ => None,
        }
    }

    /// Only `F_FREESP` takes the vnode write lock (`do_fcntl:131`).
    pub fn wants_write_lock(self) -> bool {
        matches!(self, Self::FreeSp)
    }

    /// Commands that reach `lock_op` (`do_fcntl:177-182`).
    pub fn is_lock_cmd(self) -> bool {
        matches!(self, Self::GetLk | Self::SetLk | Self::SetLkw)
    }
}

/// `F_DUPFD` floor door (`do_fcntl:139`).
///
/// The floor must sit inside the table: `[0, OPEN_MAX)`.
pub fn dupfd_arg_check(arg: i32) -> Result<(), FcntlError> {
    if arg < 0 || arg as usize >= OPEN_MAX {
        return Err(FcntlError::Inval);
    }
    Ok(())
}

/// `F_GETFD` (`do_fcntl:150-155`): cloexec set reads `FD_CLOEXEC`, else 0.
pub fn cloexec_get(set: bool) -> u32 {
    if set { FD_CLOEXEC } else { 0 }
}

/// `F_SETFD` (`do_fcntl:157-163`): only the `FD_CLOEXEC` bit steers.
pub fn cloexec_apply(arg: u32) -> bool {
    arg & FD_CLOEXEC != 0
}

/// `F_GETFL` (`do_fcntl:165-169`): read three bits
/// (`O_NONBLOCK | O_APPEND | O_ACCMODE`).
pub fn status_get(flags: u32) -> u32 {
    flags & (O_NONBLOCK | O_APPEND | O_ACCMODE)
}

/// `F_SETFL` (`do_fcntl:171-175`): write two bits only
/// (`O_NONBLOCK | O_APPEND`); the rest of the flags survive.
pub fn status_set(flags: u32, arg: u32) -> u32 {
    (flags & !(O_NONBLOCK | O_APPEND)) | (arg & (O_NONBLOCK | O_APPEND))
}

/// `F_GETNOSIGPIPE` (`do_fcntl:238-240`): the sentinel reads 0/1.
pub fn nosigpipe_get(flags: u32) -> u32 {
    u32::from(flags & O_NOSIGPIPE != 0)
}

/// `F_SETNOSIGPIPE` (`do_fcntl:241-246`): nonzero arg sets, zero clears.
pub fn nosigpipe_set(flags: u32, arg: u32) -> u32 {
    if arg != 0 {
        flags | O_NOSIGPIPE
    } else {
        flags & !O_NOSIGPIPE
    }
}

/// Record lock type (`lock_op:42-44`, `fcntl.h:203-205`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// `F_RDLCK`: shared lock.
    Read,
    /// `F_UNLCK`: unlock.
    Unlock,
    /// `F_WRLCK`: exclusive lock.
    Write,
}

impl LockType {
    /// Decode a wire lock type; anything else refuses (`lock_op:44`).
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            F_RDLCK => Some(Self::Read),
            F_UNLCK => Some(Self::Unlock),
            F_WRLCK => Some(Self::Write),
            _ => None,
        }
    }
}

/// Region base (`lock_op:52-57`, `unistd.h:174-176`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whence {
    /// `SEEK_SET`: base is zero.
    Set,
    /// `SEEK_CUR`: base is the file position.
    Cur,
    /// `SEEK_END`: base is the file size.
    End,
}

impl Whence {
    /// Decode a wire whence; anything else refuses (`default:56`).
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            SEEK_SET => Some(Self::Set),
            SEEK_CUR => Some(Self::Cur),
            SEEK_END => Some(Self::End),
            _ => None,
        }
    }

    /// Resolve the base (`lock_op:53-55`).
    pub fn base(self, pos: i64, size: i64) -> i64 {
        match self {
            Self::Set => 0,
            Self::Cur => pos,
            Self::End => size,
        }
    }
}

/// The five-question lock door (`lock_op:42-49`).
///
/// In order: known type (`44`), `GETLK` never unlocks (`45`), regular or
/// block only (`46-47`), read locks need read permission (`48`), write
/// locks need write permission (`49`). `R_BIT`/`W_BIT` ride from 29
/// (`protect.rs`), `FileType` rides from 15 (`open.rs`).
pub fn lock_gate(
    ltype: Option<LockType>,
    is_getlk: bool,
    file_type: FileType,
    mode: u8,
) -> Result<LockType, FcntlError> {
    let ltype = ltype.ok_or(FcntlError::Inval)?;
    if is_getlk && ltype == LockType::Unlock {
        return Err(FcntlError::Inval);
    }
    if !matches!(file_type, FileType::Regular | FileType::Block) {
        return Err(FcntlError::Inval);
    }
    if !is_getlk && ltype == LockType::Read && mode & R_BIT == 0 {
        return Err(FcntlError::BadF);
    }
    if !is_getlk && ltype == LockType::Write && mode & W_BIT == 0 {
        return Err(FcntlError::BadF);
    }
    Ok(ltype)
}

/// A locked byte span, both ends inclusive (`lock_op:64-67`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockRegion {
    /// First byte locked.
    pub first: i64,
    /// Last byte locked.
    pub last: i64,
}

/// First/last arithmetic (`lock_op:59-67`).
///
/// Overflow in either direction refuses (`60-63`, spelled `checked_add`
/// here — the C probe-then-add is the means, refusing overflow is the end);
/// zero length locks to the horizon (`66`); an inverted span refuses (`67`).
pub fn compute_region(base: i64, start: i64, len: i64) -> Result<LockRegion, FcntlError> {
    let first = base.checked_add(start).ok_or(FcntlError::Inval)?;
    if len == 0 {
        return Ok(LockRegion {
            first,
            last: MAX_FILE_POS,
        });
    }
    let last = first.checked_add(len).ok_or(FcntlError::Inval)? - 1;
    if last < first {
        return Err(FcntlError::Inval);
    }
    Ok(LockRegion { first, last })
}

/// Vnode identity (`lock.h:10`).
///
/// C compares `lock_vnode` pointers (`lock_op:76`); pointers cannot cross
/// calls here, so identity is the `(v_fs_e, v_inode_nr)` value pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VnodeKey {
    /// Hosting FS endpoint (`v_fs_e`).
    pub fs: Endpoint,
    /// Inode number (`v_inode_nr`).
    pub ino: u64,
}

/// One table entry (`lock.h:7-13`).
///
/// The C `lock_type == 0` free slot reads as `None` one level up: "empty"
/// is a type, not a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileLock {
    /// Lock type (never `Unlock`: stored locks always hold).
    pub lock_type: LockType,
    /// Holder pid (`lock_pid`).
    pub pid: u32,
    /// Locked file (`lock_vnode`).
    pub vnode: VnodeKey,
    /// First byte (`lock_first`).
    pub first: i64,
    /// Last byte (`lock_last`).
    pub last: i64,
}

/// A lock operation after the door (`lock_op:31,177-182`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOp {
    /// `F_GETLK` (type is `Read` or `Write`; the door rejects `Unlock`).
    Query {
        /// Queried type.
        ltype: LockType,
    },
    /// `F_SETLK` (`wait == false`) / `F_SETLKW` (`wait == true`).
    Set {
        /// Requested type (`Read`/`Write`; `Unlock` rides [`LockOp::Unlock`]).
        ltype: LockType,
        /// Wait when blocked (`F_SETLKW`).
        wait: bool,
    },
    /// `F_SETLK`/`F_SETLKW` with `F_UNLCK`: clear an overlapping span.
    Unlock,
}

impl LockOp {
    /// The type this operation carries (`Unlock` for [`LockOp::Unlock`]).
    pub fn lock_type(self) -> LockType {
        match self {
            Self::Query { ltype } | Self::Set { ltype, .. } => ltype,
            Self::Unlock => LockType::Unlock,
        }
    }

    /// Build from a gated command and type.
    ///
    /// Non-lock commands refuse (`None`): only `GETLK`/`SETLK`/`SETLKW`
    /// reach `lock_op` (`lock_op:31`). `GETLK` with `Unlock` is
    /// unrepresentable — the door (`lock_gate`) already refused it — so it
    /// also reads `None`.
    pub fn from_req(cmd: FcntlCmd, ltype: LockType) -> Option<Self> {
        match (cmd, ltype) {
            (FcntlCmd::GetLk, LockType::Read | LockType::Write) => Some(Self::Query { ltype }),
            (FcntlCmd::SetLk, LockType::Read | LockType::Write) => {
                Some(Self::Set { ltype, wait: false })
            }
            (FcntlCmd::SetLkw, LockType::Read | LockType::Write) => {
                Some(Self::Set { ltype, wait: true })
            }
            (FcntlCmd::SetLk | FcntlCmd::SetLkw, LockType::Unlock) => Some(Self::Unlock),
            _ => None,
        }
    }
}

/// What `F_GETLK` reports on a hit (`lock_op:136-143`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockAnswer {
    /// Conflicting lock type.
    pub lock_type: LockType,
    /// Reported start (rebased to `SEEK_SET`, `139-140`).
    pub first: i64,
    /// Reported length (`last - first + 1`, `141`).
    pub len: i64,
    /// Holder pid (`142`).
    pub pid: u32,
}

/// Suspend record for a waiting `F_SETLKW` (`lock_op:93-97`).
///
/// C stores `{fd, cmd, arg}` in `fp_flock`; `cmd` is constant (`F_SETLKW`,
/// proven by `FlockCmd::SetLkw` in fproc) and `arg` rides the caller's
/// resume record (`FlockBlock`), so the decision layer keeps only `fd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlockWait {
    /// File descriptor of the blocking call.
    pub fd: usize,
}

/// What a lock operation concludes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOutcome {
    /// Lock stored (`lock_op:157-165`).
    Granted,
    /// `GETLK` hit: the conflicting lock, reported (`136-143`).
    QueryHit(LockAnswer),
    /// `GETLK` miss: report `F_UNLCK` (`144-146`).
    QueryMiss,
    /// `F_SETLKW` blocked: caller suspends (`93-97` → [`FcntlVerdict::Suspend`]).
    Wait(FlockWait),
    /// Region cleared (`155`); `revive` tells whether anyone may proceed.
    Unlocked {
        /// A lock left the table: the caller runs [`revive_all`]
        /// (C calls `lock_revive()` itself, `133`).
        revive: bool,
    },
}

/// The eight-slot table (`lock.h:13`, `glo.h:15`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockTable {
    /// Slots (`file_lock[NR_LOCKS]`); `None` is the free slot.
    pub slots: [Option<FileLock>; NR_LOCKS],
    /// Locks in place (`nr_locks`).
    pub nr: usize,
}

impl LockTable {
    /// Empty table.
    pub fn new() -> Self {
        Self {
            slots: [None; NR_LOCKS],
            nr: 0,
        }
    }

    /// Conflict scan and update (`lock_op:69-165`).
    ///
    /// Three skips: other files (`76`), disjoint spans (`77-78`),
    /// read-read compatibility (`79`); same-process locks never block
    /// their owner's set or query (`80`) — but anyone's unlock clears
    /// (`ltype != F_UNLCK` guards the skip, so `Unlock` skips nothing).
    /// First real conflict decides: query breaks to report (`84`), a
    /// non-waiting set refuses (`88-90`), a waiting set suspends
    /// (`91-98`), an unlock reshapes (`101-132`: full clear / trim head /
    /// trim tail / middle split, `ENOLCK` when a split finds no slot,
    /// `121`). After the scan an unlock that freed anything wakes the
    /// waiters (`133`); a query reports hit or miss (`135-153`); a set
    /// stores or reports a full table (`157-165`).
    pub fn lock_op_decision(
        &mut self,
        op: LockOp,
        pid: u32,
        vnode: VnodeKey,
        region: LockRegion,
        wait_fd: usize,
    ) -> Result<LockOutcome, FcntlError> {
        let first = region.first;
        let last = region.last;
        let ltype = op.lock_type();
        let mut empty: Option<usize> = None;
        let mut hit: Option<FileLock> = None;
        let mut unlocking = false;

        let mut i = 0;
        while i < NR_LOCKS {
            let cur = self.slots[i];
            let Some(fl) = cur else {
                if empty.is_none() {
                    empty = Some(i);
                }
                i += 1;
                continue;
            };
            if fl.vnode != vnode {
                i += 1;
                continue;
            }
            if last < fl.first || first > fl.last {
                i += 1;
                continue;
            }
            if ltype == LockType::Read && fl.lock_type == LockType::Read {
                i += 1;
                continue;
            }
            if ltype != LockType::Unlock && fl.pid == pid {
                i += 1;
                continue;
            }
            // A live conflict. Query breaks to report; set refuses or
            // waits; unlock reshapes and keeps scanning.
            match op {
                LockOp::Query { .. } => {
                    // Copy the holder out: no re-read, no panic path.
                    hit = Some(fl);
                    break;
                }
                LockOp::Set { wait, .. } if ltype != LockType::Unlock => {
                    if wait {
                        return Ok(LockOutcome::Wait(FlockWait { fd: wait_fd }));
                    }
                    return Err(FcntlError::Again);
                }
                _ => {
                    // Unlock path (C reaches it for any `F_UNLCK`,
                    // whatever the request number).
                    unlocking = true;
                    if first <= fl.first && last >= fl.last {
                        self.slots[i] = None;
                        self.nr -= 1;
                        i += 1;
                        continue;
                    }
                    if first <= fl.first {
                        if let Some(slot) = self.slots[i].as_mut() {
                            slot.first = last + 1;
                        }
                        i += 1;
                        continue;
                    }
                    if last >= fl.last {
                        if let Some(slot) = self.slots[i].as_mut() {
                            slot.last = first - 1;
                        }
                        i += 1;
                        continue;
                    }
                    // Middle split: two locks where one stood. No free
                    // slot refuses (`121`); the scan below must find one
                    // while `nr < NR_LOCKS`, and fail-closed otherwise.
                    if self.nr == NR_LOCKS {
                        return Err(FcntlError::NoLock);
                    }
                    let free = (0..NR_LOCKS).find(|&j| self.slots[j].is_none());
                    match free {
                        Some(j) => {
                            let old_last = fl.last;
                            if let Some(slot) = self.slots[i].as_mut() {
                                slot.last = first - 1;
                            }
                            self.slots[j] = Some(FileLock {
                                lock_type: fl.lock_type,
                                pid: fl.pid,
                                vnode: fl.vnode,
                                first: last + 1,
                                last: old_last,
                            });
                            self.nr += 1;
                        }
                        None => return Err(FcntlError::NoLock),
                    }
                    i += 1;
                }
            }
        }

        match op {
            LockOp::Query { .. } => match hit {
                Some(fl) => Ok(LockOutcome::QueryHit(LockAnswer {
                    lock_type: fl.lock_type,
                    first: fl.first,
                    len: fl.last - fl.first + 1,
                    pid: fl.pid,
                })),
                None => Ok(LockOutcome::QueryMiss),
            },
            LockOp::Unlock => Ok(LockOutcome::Unlocked { revive: unlocking }),
            LockOp::Set { .. } => {
                if ltype == LockType::Unlock {
                    // `Set` carrying `Unlock` clears like `Unlock`
                    // (C keys the unlock path off `ltype`, not `req`).
                    return Ok(LockOutcome::Unlocked { revive: unlocking });
                }
                match empty {
                    None => Err(FcntlError::NoLock),
                    Some(idx) => {
                        self.slots[idx] = Some(FileLock {
                            lock_type: ltype,
                            pid,
                            vnode,
                            first,
                            last,
                        });
                        self.nr += 1;
                        Ok(LockOutcome::Granted)
                    }
                }
            }
        }
    }

    /// Close-time release (`close_fd:713-724`).
    ///
    /// Every lock of this `(vnode, pid)` pair leaves; the caller wakes the
    /// waiters when anything left (`722-723`).
    pub fn release_for(&mut self, vnode: VnodeKey, pid: u32) -> bool {
        let mut released = false;
        for slot in self.slots.iter_mut() {
            if let Some(fl) = slot
                && fl.vnode == vnode
                && fl.pid == pid
            {
                *slot = None;
                self.nr -= 1;
                released = true;
            }
        }
        released
    }
}

impl Default for LockTable {
    fn default() -> Self {
        Self::new()
    }
}

/// One process's suspend state as seen by the revive scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendedProc {
    /// Process id (`fp_pid`; `PID_FREE` reads as not alive).
    pub pid: u32,
    /// Slot in use (`fp_pid != PID_FREE`, `187`).
    pub alive: bool,
    /// Blocked on a record lock (`fp_blocked_on == FP_BLOCKED_ON_FLOCK`, `188`).
    pub blocked_on_flock: bool,
}

/// Broadcast wake (`lock_revive:172-192`).
///
/// Every live process blocked on a record lock revives — no roll call.
/// The C comment signs the tradeoff (`175-182`): finding exactly who may
/// proceed costs code and wins only "in extremely rare circumstances".
/// Woken processes re-run `lock_op` on resume (`unblock` rebuilds the
/// `VFS_FCNTL` message, `main.c:946-954`); `FP_BLOCKED_ON_FLOCK` needs no
/// cancel (`unpause:531` breaks clean), so a spurious wake is harmless.
pub fn revive_all(procs: &[SuspendedProc]) -> Vec<u32> {
    procs
        .iter()
        .filter(|p| p.alive && p.blocked_on_flock)
        .map(|p| p.pid)
        .collect()
}

/// What `F_FREESP` truncates (`do_fcntl:222-234`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreespSpan {
    /// Cut `[start, end)`: nonzero length, end clamped to the file size.
    TruncateTo {
        /// Hole start.
        start: i64,
        /// Hole end (exclusive, already clamped).
        end: i64,
    },
    /// Cut the tail: zero length truncates the size to `start` (`233-234`).
    TruncateSize(i64),
}

/// Hole arithmetic (`do_fcntl:201-229`).
///
/// Overflow in either direction refuses (`214-215`); a negative start
/// refuses (`218`); nonzero length must start inside the file (`223`),
/// must not wrap (`224`), and clamps to the file size (`225`).
pub fn freesp_span(v_size: i64, base: i64, start: i64, len: i64) -> Result<FreespSpan, FcntlError> {
    let start_pos = base.checked_add(start).ok_or(FcntlError::Inval)?;
    if start_pos < 0 {
        return Err(FcntlError::Inval);
    }
    if len != 0 {
        if start_pos >= v_size {
            return Err(FcntlError::Inval);
        }
        let end = start_pos.checked_add(len).ok_or(FcntlError::Inval)?;
        if end <= start_pos {
            return Err(FcntlError::Inval);
        }
        return Ok(FreespSpan::TruncateTo {
            start: start_pos,
            end: end.min(v_size),
        });
    }
    Ok(FreespSpan::TruncateSize(start_pos))
}

/// What `F_FLUSH_FS_CACHE` flushes (`do_fcntl:247-264`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushTarget {
    /// Block device: flush the device blocks (`253-255`).
    BlockDev,
    /// Regular file or directory: flush the hosting FS (`256-258`).
    HostingFs,
}

/// Flush door (`do_fcntl:251-262`).
///
/// Strangers refuse (`!super_user → EPERM`, `251`); block devices flush
/// elsewhere than files; the rest refuse `ENODEV` (`260-261`, "meaning
/// unclear" in C — kept verbatim, not invented).
pub fn flush_target(is_root: bool, file_type: FileType) -> Result<FlushTarget, FcntlError> {
    if !is_root {
        return Err(FcntlError::Perm);
    }
    match file_type {
        FileType::Block => Ok(FlushTarget::BlockDev),
        FileType::Regular | FileType::Directory => Ok(FlushTarget::HostingFs),
        _ => Err(FcntlError::NoDev),
    }
}

/// The FS dialogue behind a trait.
///
/// `req_ftrunc`/`req_flush` execute FS-side; the FS is the only
/// untestable point, so only the dialogue is abstracted (same-source convention as 29-D7).
pub trait FcntlFs {
    /// `req_ftrunc`: punch `[start, end)` on `(fs, ino)`.
    fn ftrunc(&mut self, fs: Endpoint, ino: u64, start: i64, end: i64) -> Result<(), FcntlError>;
    /// `req_flush`: flush `(fs, dev)`.
    fn flush(&mut self, fs: Endpoint, dev: u64) -> Result<(), FcntlError>;
}

/// Scripted FS (test double with programmed answers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedFcntl {
    /// Programmed `ftrunc` answer.
    pub ftrunc_out: Result<(), FcntlError>,
    /// Programmed `flush` answer.
    pub flush_out: Result<(), FcntlError>,
    /// Downcalls made (observable dialogue).
    pub ncalls: u32,
}

impl Default for ScriptedFcntl {
    fn default() -> Self {
        Self {
            ftrunc_out: Ok(()),
            flush_out: Ok(()),
            ncalls: 0,
        }
    }
}

impl FcntlFs for ScriptedFcntl {
    fn ftrunc(
        &mut self,
        _fs: Endpoint,
        _ino: u64,
        _start: i64,
        _end: i64,
    ) -> Result<(), FcntlError> {
        self.ncalls += 1;
        self.ftrunc_out
    }
    fn flush(&mut self, _fs: Endpoint, _dev: u64) -> Result<(), FcntlError> {
        self.ncalls += 1;
        self.flush_out
    }
}

/// Refusing FS (test double: every downcall fails with `EIO`).
///
/// Behaves differently from [`ScriptedFcntl`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusingFcntl;

impl FcntlFs for RefusingFcntl {
    fn ftrunc(
        &mut self,
        _fs: Endpoint,
        _ino: u64,
        _start: i64,
        _end: i64,
    ) -> Result<(), FcntlError> {
        Err(FcntlError::Io)
    }
    fn flush(&mut self, _fs: Endpoint, _dev: u64) -> Result<(), FcntlError> {
        Err(FcntlError::Io)
    }
}

/// What the call tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FcntlVerdict {
    /// Reply now (status carried separately).
    Done,
    /// C `SUSPEND`: do not reply now; a later revive path replies
    /// (09 `ReplyIntent::ReplyLater`; `F_SETLKW` wait, `lock_op:97`).
    Suspend,
}

impl From<LockOutcome> for FcntlVerdict {
    /// Only waiting suspends; grants, answers, and clears reply now.
    fn from(outcome: LockOutcome) -> Self {
        match outcome {
            LockOutcome::Wait(_) => Self::Suspend,
            _ => Self::Done,
        }
    }
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// Lookup failures ride `err_code` upstream (caller-side inputs: bad fd
/// from `get_filp`), so they are not variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FcntlError {
    /// `EINVAL`: wild commands/types/whences/spans, non-regular targets.
    Inval,
    /// `EBADF`: lock mode exceeds the open mode (`48-49`).
    BadF,
    /// `EAGAIN`: `F_SETLK` blocked (`88-90`).
    Again,
    /// `ENOLCK`: table full, or a middle split finds no slot (`121/158`).
    NoLock,
    /// `EPERM`: strangers flush caches (`251`).
    Perm,
    /// `ENODEV`: flush target is neither block nor file (`260-261`).
    NoDev,
    /// `EIO`: FS-side refusal.
    Io,
}

impl FcntlError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Inval => minix_types::EINVAL,
            Self::BadF => minix_types::EBADF,
            Self::Again => minix_types::EAGAIN,
            Self::NoLock => minix_types::ENOLCK,
            Self::Perm => minix_types::EPERM,
            Self::NoDev => minix_types::ENODEV,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(fs: i32, ino: u64) -> VnodeKey {
        VnodeKey {
            fs: Endpoint(fs),
            ino,
        }
    }

    fn region(first: i64, last: i64) -> LockRegion {
        LockRegion { first, last }
    }

    fn grant(table: &mut LockTable, pid: u32, fs: i32, first: i64, last: i64, ltype: LockType) {
        let op = LockOp::Set { ltype, wait: false };
        assert_eq!(
            table.lock_op_decision(op, pid, key(fs, 7), region(first, last), 3),
            Ok(LockOutcome::Granted)
        );
    }

    #[test]
    fn test_cmd_decode_and_dupfd_door() {
        // Thirteen commands decode (`do_fcntl:135-267` surface).
        let all = [
            (F_DUPFD, FcntlCmd::DupFd),
            (F_GETFD, FcntlCmd::GetFd),
            (F_SETFD, FcntlCmd::SetFd),
            (F_GETFL, FcntlCmd::GetFl),
            (F_SETFL, FcntlCmd::SetFl),
            (F_GETLK, FcntlCmd::GetLk),
            (F_SETLK, FcntlCmd::SetLk),
            (F_SETLKW, FcntlCmd::SetLkw),
            (F_DUPFD_CLOEXEC, FcntlCmd::DupFdCloexec),
            (F_GETNOSIGPIPE, FcntlCmd::GetNoSigPipe),
            (F_SETNOSIGPIPE, FcntlCmd::SetNoSigPipe),
            (F_FREESP, FcntlCmd::FreeSp),
            (F_FLUSH_FS_CACHE, FcntlCmd::FlushFsCache),
        ];
        assert_eq!(all.len(), 13);
        for (raw, cmd) in all {
            assert_eq!(FcntlCmd::from_raw(raw), Some(cmd), "raw {raw}");
        }
        // Branchless commands refuse (`default:266`): GETOWN/SETOWN(5/6),
        // CLOSEM/MAXFD(10/11), and wild values.
        for raw in [5, 6, 10, 11, 99, 1000] {
            assert_eq!(FcntlCmd::from_raw(raw), None, "raw {raw}");
        }
        // Only FREESP takes the write lock (`131`).
        assert!(FcntlCmd::FreeSp.wants_write_lock());
        assert!(!FcntlCmd::GetLk.wants_write_lock());
        assert!(!FcntlCmd::DupFd.wants_write_lock());
        // Only the three lock commands reach lock_op (`177-182`).
        assert!(FcntlCmd::GetLk.is_lock_cmd());
        assert!(FcntlCmd::SetLk.is_lock_cmd());
        assert!(FcntlCmd::SetLkw.is_lock_cmd());
        assert!(!FcntlCmd::FreeSp.is_lock_cmd());
        // Floor door (`139`): [0, OPEN_MAX).
        assert!(dupfd_arg_check(0).is_ok());
        assert!(dupfd_arg_check(254).is_ok());
        assert_eq!(dupfd_arg_check(-1), Err(FcntlError::Inval));
        assert_eq!(dupfd_arg_check(255), Err(FcntlError::Inval));
        assert_eq!(dupfd_arg_check(1000), Err(FcntlError::Inval));
    }

    #[test]
    fn test_narrow_flag_doors() {
        // GETFD reads the bit (`152-154`).
        assert_eq!(cloexec_get(true), FD_CLOEXEC);
        assert_eq!(cloexec_get(false), 0);
        // SETFD steers on the bit only (`159-162`).
        assert!(cloexec_apply(FD_CLOEXEC));
        assert!(!cloexec_apply(0));
        assert!(cloexec_apply(0xFFFE_0001));
        // GETFL reads three bits (`167`).
        assert_eq!(
            status_get(O_NONBLOCK | O_APPEND | 0x3),
            O_NONBLOCK | O_APPEND | 0x3
        );
        assert_eq!(status_get(0xFFFF_FFFF), O_NONBLOCK | O_APPEND | O_ACCMODE);
        assert_eq!(status_get(0), 0);
        // SETFL writes two bits only (`173-174`): the rest survive.
        assert_eq!(status_set(0xFFFF_FFFF, O_NONBLOCK | O_APPEND), 0xFFFF_FFFF);
        assert_eq!(
            status_set(O_ACCMODE, 0xFFFF_FFFF),
            O_ACCMODE | O_NONBLOCK | O_APPEND
        );
        assert_eq!(status_set(O_NONBLOCK, 0), 0);
        // NOSIGPIPE sentinel both ways (`239-246`).
        assert_eq!(nosigpipe_get(O_NOSIGPIPE), 1);
        assert_eq!(nosigpipe_get(0), 0);
        assert_eq!(nosigpipe_set(0, 1), O_NOSIGPIPE);
        assert_eq!(nosigpipe_set(O_NOSIGPIPE, 0), 0);
        assert_eq!(nosigpipe_set(0, 0), 0);
    }

    #[test]
    fn test_lock_door_five_questions() {
        // Known types only (`44`).
        assert_eq!(LockType::from_raw(1), Some(LockType::Read));
        assert_eq!(LockType::from_raw(2), Some(LockType::Unlock));
        assert_eq!(LockType::from_raw(3), Some(LockType::Write));
        assert_eq!(LockType::from_raw(0), None);
        assert_eq!(LockType::from_raw(4), None);
        // Known whences only (`56`).
        assert_eq!(Whence::from_raw(0), Some(Whence::Set));
        assert_eq!(Whence::from_raw(1), Some(Whence::Cur));
        assert_eq!(Whence::from_raw(2), Some(Whence::End));
        assert_eq!(Whence::from_raw(3), None);
        // Whence bases (`53-55`).
        assert_eq!(Whence::Set.base(50, 1000), 0);
        assert_eq!(Whence::Cur.base(50, 1000), 50);
        assert_eq!(Whence::End.base(50, 1000), 1000);
        // GETLK never unlocks (`45`).
        assert_eq!(
            lock_gate(Some(LockType::Unlock), true, FileType::Regular, 0o6),
            Err(FcntlError::Inval)
        );
        // Regular or block only (`46-47`).
        assert!(lock_gate(Some(LockType::Read), false, FileType::Regular, 0o4).is_ok());
        assert!(lock_gate(Some(LockType::Write), false, FileType::Block, 0o2).is_ok());
        assert_eq!(
            lock_gate(Some(LockType::Read), false, FileType::Directory, 0o4),
            Err(FcntlError::Inval)
        );
        assert_eq!(
            lock_gate(Some(LockType::Read), false, FileType::Fifo, 0o4),
            Err(FcntlError::Inval)
        );
        // Unknown type refuses even for good files (`44`).
        assert_eq!(
            lock_gate(None, false, FileType::Regular, 0o6),
            Err(FcntlError::Inval)
        );
        // Mode doors (`48-49`): queries skip them.
        assert_eq!(
            lock_gate(Some(LockType::Read), false, FileType::Regular, 0o2),
            Err(FcntlError::BadF)
        );
        assert_eq!(
            lock_gate(Some(LockType::Write), false, FileType::Regular, 0o4),
            Err(FcntlError::BadF)
        );
        assert!(lock_gate(Some(LockType::Read), true, FileType::Regular, 0).is_ok());
        assert!(lock_gate(Some(LockType::Write), true, FileType::Regular, 0).is_ok());
        // Request shapes (`from_req`): GETLK+Unlock unrepresentable.
        assert_eq!(
            LockOp::from_req(FcntlCmd::GetLk, LockType::Read),
            Some(LockOp::Query {
                ltype: LockType::Read
            })
        );
        assert_eq!(
            LockOp::from_req(FcntlCmd::SetLk, LockType::Write),
            Some(LockOp::Set {
                ltype: LockType::Write,
                wait: false
            })
        );
        assert_eq!(
            LockOp::from_req(FcntlCmd::SetLkw, LockType::Write),
            Some(LockOp::Set {
                ltype: LockType::Write,
                wait: true
            })
        );
        assert_eq!(
            LockOp::from_req(FcntlCmd::SetLk, LockType::Unlock),
            Some(LockOp::Unlock)
        );
        assert_eq!(LockOp::from_req(FcntlCmd::GetLk, LockType::Unlock), None);
        assert_eq!(LockOp::from_req(FcntlCmd::DupFd, LockType::Read), None);
    }

    #[test]
    fn test_region_arithmetic() {
        // Plain span (`64-65`).
        assert_eq!(
            compute_region(0, 10, 5),
            Ok(LockRegion {
                first: 10,
                last: 14
            })
        );
        // Zero length locks to the horizon (`66`).
        assert_eq!(
            compute_region(100, 0, 0),
            Ok(LockRegion {
                first: 100,
                last: MAX_FILE_POS
            })
        );
        assert_eq!(MAX_FILE_POS, 0x7FFF_FFFF);
        // Overflow either way refuses (`60-63`).
        assert_eq!(compute_region(i64::MAX - 1, 5, 10), Err(FcntlError::Inval));
        assert_eq!(compute_region(i64::MIN + 1, -5, 10), Err(FcntlError::Inval));
        assert_eq!(
            compute_region(i64::MAX, 0, 2).map(|_| ()),
            Err(FcntlError::Inval)
        );
        // Inverted span refuses (`67`): negative length overshoots.
        assert_eq!(compute_region(10, 0, -5), Err(FcntlError::Inval));
        // Length one locks a single byte.
        assert_eq!(
            compute_region(7, 0, 1),
            Ok(LockRegion { first: 7, last: 7 })
        );
    }

    #[test]
    fn test_conflict_matrix_and_unlock_shapes() {
        let mut table = LockTable::new();
        // Owner holds a write lock on [10, 19].
        grant(&mut table, 100, 1, 10, 19, LockType::Write);
        assert_eq!(table.nr, 1);
        // Other files never conflict (`76`).
        let op = LockOp::Set {
            ltype: LockType::Write,
            wait: false,
        };
        assert_eq!(
            table.lock_op_decision(op, 200, key(2, 7), region(10, 19), 3),
            Ok(LockOutcome::Granted)
        );
        // Disjoint spans never conflict (`77-78`).
        assert_eq!(
            table.lock_op_decision(op, 200, key(1, 7), region(20, 29), 3),
            Ok(LockOutcome::Granted)
        );
        assert_eq!(table.nr, 3);
        // Same process never blocks itself (`80`): overwrite own span.
        assert_eq!(
            table.lock_op_decision(op, 100, key(1, 7), region(10, 19), 3),
            Ok(LockOutcome::Granted)
        );
        // Stranger's write on overlap: SETLK refuses (`88-90`).
        assert_eq!(
            table.lock_op_decision(op, 300, key(1, 7), region(15, 25), 3),
            Err(FcntlError::Again)
        );
        // Stranger's write on overlap: SETLKW waits (`91-98`).
        let opw = LockOp::Set {
            ltype: LockType::Write,
            wait: true,
        };
        assert_eq!(
            table.lock_op_decision(opw, 300, key(1, 7), region(15, 25), 5),
            Ok(LockOutcome::Wait(FlockWait { fd: 5 }))
        );
        // Read-read stays compatible (`79`): fresh table, two readers share.
        let mut readers = LockTable::new();
        grant(&mut readers, 100, 1, 0, 99, LockType::Read);
        let opr = LockOp::Set {
            ltype: LockType::Read,
            wait: false,
        };
        assert_eq!(
            readers.lock_op_decision(opr, 200, key(1, 7), region(0, 99), 3),
            Ok(LockOutcome::Granted)
        );
        // Writer against readers refuses (`87-90`): write meets read.
        let opw2 = LockOp::Set {
            ltype: LockType::Write,
            wait: false,
        };
        assert_eq!(
            readers.lock_op_decision(opw2, 300, key(1, 7), region(0, 99), 3),
            Err(FcntlError::Again)
        );
        // GETLK hit reports the first holder in table order (`84` breaks
        // at the first conflict): slot 0's read lock, not a write.
        let q = LockOp::Query {
            ltype: LockType::Write,
        };
        assert_eq!(
            readers.lock_op_decision(q, 400, key(1, 7), region(10, 20), 3),
            Ok(LockOutcome::QueryHit(LockAnswer {
                lock_type: LockType::Read,
                first: 0,
                len: 100,
                pid: 100,
            }))
        );
        // GETLK never reports own locks (`80` applies to queries too):
        // a table holding only the querier's lock answers miss.
        let mut own = LockTable::new();
        grant(&mut own, 100, 1, 0, 99, LockType::Write);
        assert_eq!(
            own.lock_op_decision(q, 100, key(1, 7), region(10, 20), 3),
            Ok(LockOutcome::QueryMiss)
        );
        // GETLK miss reports unlock (`144-146`).
        assert_eq!(
            readers.lock_op_decision(q, 400, key(9, 9), region(0, 10), 3),
            Ok(LockOutcome::QueryMiss)
        );
        // Unlock shapes. Full cover clears (`103-107`).
        let mut shapes = LockTable::new();
        grant(&mut shapes, 100, 1, 10, 19, LockType::Write);
        assert_eq!(
            shapes.lock_op_decision(LockOp::Unlock, 100, key(1, 7), region(10, 19), 3),
            Ok(LockOutcome::Unlocked { revive: true })
        );
        assert_eq!(shapes.nr, 0);
        // Unlock clears whatever overlaps, not just own (`ltype != F_UNLCK`
        // guards the same-pid skip, so Unlock skips nothing).
        grant(&mut shapes, 100, 1, 10, 19, LockType::Write);
        assert_eq!(
            shapes.lock_op_decision(LockOp::Unlock, 999, key(1, 7), region(10, 19), 3),
            Ok(LockOutcome::Unlocked { revive: true })
        );
        assert_eq!(shapes.nr, 0);
        // Trim head (`110-113`): unlock [10, 14] of [10, 19] leaves [15, 19].
        grant(&mut shapes, 100, 1, 10, 19, LockType::Write);
        assert_eq!(
            shapes.lock_op_decision(LockOp::Unlock, 100, key(1, 7), region(10, 14), 3),
            Ok(LockOutcome::Unlocked { revive: true })
        );
        assert_eq!(
            shapes.slots[0].map(|fl| (fl.first, fl.last)),
            Some((15, 19))
        );
        // Trim tail (`115-118`): unlock [17, 30] of [15, 19] leaves [15, 16].
        assert_eq!(
            shapes.lock_op_decision(LockOp::Unlock, 100, key(1, 7), region(17, 30), 3),
            Ok(LockOutcome::Unlocked { revive: true })
        );
        assert_eq!(
            shapes.slots[0].map(|fl| (fl.first, fl.last)),
            Some((15, 16))
        );
        // Middle split (`120-131`): unlock [5, 6] of [0, 9] leaves [0, 4] + [7, 9].
        let mut split = LockTable::new();
        grant(&mut split, 100, 1, 0, 9, LockType::Read);
        assert_eq!(
            split.lock_op_decision(LockOp::Unlock, 100, key(1, 7), region(5, 6), 3),
            Ok(LockOutcome::Unlocked { revive: true })
        );
        assert_eq!(split.nr, 2);
        assert_eq!(split.slots[0].map(|fl| (fl.first, fl.last)), Some((0, 4)));
        assert_eq!(
            split.slots[1].map(|fl| (fl.first, fl.last, fl.pid)),
            Some((7, 9, 100))
        );
        // Unlock with no locks anywhere: nothing happens, no wake (`155`).
        let mut empty = LockTable::new();
        assert_eq!(
            empty.lock_op_decision(LockOp::Unlock, 100, key(1, 7), region(0, 9), 3),
            Ok(LockOutcome::Unlocked { revive: false })
        );
        // Full table refuses new locks (`158`).
        let mut full = LockTable::new();
        for s in 0..NR_LOCKS {
            full.slots[s] = Some(FileLock {
                lock_type: LockType::Read,
                pid: 1,
                vnode: key(1, s as u64),
                first: 0,
                last: 9,
            });
        }
        full.nr = NR_LOCKS;
        assert_eq!(
            full.lock_op_decision(
                LockOp::Set {
                    ltype: LockType::Read,
                    wait: false
                },
                2,
                key(1, 99),
                region(0, 9),
                3
            ),
            Err(FcntlError::NoLock)
        );
        // Full table refuses a middle split too (`121`).
        assert_eq!(
            full.lock_op_decision(LockOp::Unlock, 1, key(1, 3), region(4, 5), 3),
            Err(FcntlError::NoLock)
        );
        // Verdict mapping: only waiting suspends (ARCH A-5).
        assert_eq!(
            FcntlVerdict::from(LockOutcome::Wait(FlockWait { fd: 5 })),
            FcntlVerdict::Suspend
        );
        assert_eq!(FcntlVerdict::from(LockOutcome::Granted), FcntlVerdict::Done);
        assert_eq!(
            FcntlVerdict::from(LockOutcome::QueryMiss),
            FcntlVerdict::Done
        );
        assert_eq!(
            FcntlVerdict::from(LockOutcome::Unlocked { revive: true }),
            FcntlVerdict::Done
        );
    }

    #[test]
    fn test_close_release_and_revive() {
        // Two locks of pid 100 on one file, one of pid 200: close by 100
        // releases exactly its own (`close_fd:715-721`).
        let mut table = LockTable::new();
        grant(&mut table, 100, 1, 0, 9, LockType::Write);
        grant(&mut table, 100, 1, 20, 29, LockType::Read);
        grant(&mut table, 200, 1, 40, 49, LockType::Read);
        assert_eq!(table.nr, 3);
        assert!(table.release_for(key(1, 7), 100));
        assert_eq!(table.nr, 1);
        // Other files of the same pid survive.
        grant(&mut table, 100, 2, 0, 9, LockType::Write);
        assert!(!table.release_for(key(9, 9), 100));
        assert!(table.release_for(key(2, 7), 100));
        // Broadcast wake (`lock_revive:186-191`): free slots and strangers
        // stay asleep; only live FLOCK-blocked revive.
        let procs = [
            SuspendedProc {
                pid: 10,
                alive: true,
                blocked_on_flock: true,
            },
            SuspendedProc {
                pid: 11,
                alive: true,
                blocked_on_flock: false,
            },
            SuspendedProc {
                pid: 12,
                alive: false,
                blocked_on_flock: true,
            },
            SuspendedProc {
                pid: 13,
                alive: true,
                blocked_on_flock: true,
            },
        ];
        assert_eq!(revive_all(&procs), alloc::vec![10, 13]);
        assert!(revive_all(&[]).is_empty());
    }

    #[test]
    fn test_freesp_and_flush_dialogue() {
        // Hole inside the file clamps nothing (`222-225`).
        assert_eq!(
            freesp_span(1000, 0, 100, 50),
            Ok(FreespSpan::TruncateTo {
                start: 100,
                end: 150
            })
        );
        // Hole past the end clamps (`225`).
        assert_eq!(
            freesp_span(1000, 0, 900, 500),
            Ok(FreespSpan::TruncateTo {
                start: 900,
                end: 1000
            })
        );
        // Zero length cuts the tail (`233-234`).
        assert_eq!(
            freesp_span(1000, 0, 400, 0),
            Ok(FreespSpan::TruncateSize(400))
        );
        // Starting past the end refuses (`223`).
        assert_eq!(freesp_span(1000, 0, 1000, 10), Err(FcntlError::Inval));
        assert_eq!(freesp_span(1000, 0, 2000, 10), Err(FcntlError::Inval));
        // Wrapped or negative spans refuse (`214-218, 224`).
        assert_eq!(freesp_span(1000, 0, -5, 10), Err(FcntlError::Inval));
        assert_eq!(freesp_span(1000, 100, -200, 10), Err(FcntlError::Inval));
        assert_eq!(freesp_span(1000, 0, 900, -100), Err(FcntlError::Inval));
        assert_eq!(freesp_span(1000, i64::MAX, 1, 10), Err(FcntlError::Inval));
        // SEEK_END base rides whence (`208`): base = size.
        assert_eq!(
            freesp_span(1000, 1000, -100, 50),
            Ok(FreespSpan::TruncateTo {
                start: 900,
                end: 950
            })
        );
        // Flush door (`251-262`): strangers out, block apart, rest out.
        assert_eq!(
            flush_target(false, FileType::Regular),
            Err(FcntlError::Perm)
        );
        assert_eq!(
            flush_target(true, FileType::Block),
            Ok(FlushTarget::BlockDev)
        );
        assert_eq!(
            flush_target(true, FileType::Regular),
            Ok(FlushTarget::HostingFs)
        );
        assert_eq!(
            flush_target(true, FileType::Directory),
            Ok(FlushTarget::HostingFs)
        );
        assert_eq!(flush_target(true, FileType::Fifo), Err(FcntlError::NoDev));
        assert_eq!(flush_target(true, FileType::Socket), Err(FcntlError::NoDev));
        // Scripted dialogue answers; refusal propagates (second impl).
        let mut scripted = ScriptedFcntl::default();
        assert!(scripted.ftrunc(Endpoint(1), 7, 100, 150).is_ok());
        assert!(scripted.flush(Endpoint(1), 7).is_ok());
        assert_eq!(scripted.ncalls, 2);
        let mut failing = ScriptedFcntl {
            ftrunc_out: Err(FcntlError::Io),
            ..Default::default()
        };
        assert_eq!(
            failing.ftrunc(Endpoint(1), 7, 100, 150),
            Err(FcntlError::Io)
        );
        let mut refusing = RefusingFcntl;
        assert_eq!(
            refusing.ftrunc(Endpoint(1), 7, 100, 150),
            Err(FcntlError::Io)
        );
        assert_eq!(refusing.flush(Endpoint(1), 7), Err(FcntlError::Io));
    }

    #[test]
    fn test_errno_map_covers_fcntl_c() {
        for (err, errno) in [
            (FcntlError::Inval, minix_types::EINVAL),
            (FcntlError::BadF, minix_types::EBADF),
            (FcntlError::Again, minix_types::EAGAIN),
            (FcntlError::NoLock, minix_types::ENOLCK),
            (FcntlError::Perm, minix_types::EPERM),
            (FcntlError::NoDev, minix_types::ENODEV),
            (FcntlError::Io, minix_types::EIO),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
        // Wire numbers hold: commands, types, flags, table size.
        assert_eq!(
            (
                F_DUPFD, F_GETFD, F_SETFD, F_GETFL, F_SETFL, F_GETLK, F_SETLK, F_SETLKW
            ),
            (0, 1, 2, 3, 4, 7, 8, 9)
        );
        assert_eq!((F_RDLCK, F_UNLCK, F_WRLCK), (1, 2, 3));
        assert_eq!((O_NONBLOCK, O_APPEND, O_ACCMODE), (0x4, 0x8, 0x3));
        assert_eq!(FD_CLOEXEC, 1);
        assert_eq!(NR_LOCKS, 8);
    }
}
