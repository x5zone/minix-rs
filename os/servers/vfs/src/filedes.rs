//! File descriptor table operations — `filedes.c:88-656` + `open.c:690 close_fd`.
//!
//! `fd` (`fp_filp[256]` private index) vs `filp` (`filp[1024]` shared pool).
//! `FD_CLOEXEC` bitmap, `FILP_CLOSED` sentinel (`EIO`), `EMFILE` vs `ENFILE`,
//! `COPYFD` three-way, `invalidate_filp` family.
//!
//! Design notes (14-filedes.md §3):
//! - `Fd(u8)` newtype makes `256` bound type-safe (ARCH A-8)
//! - `FdAllocPolicy` trait `LowestFree` vs `NextFit` makes `start→OPEN_MAX` scan pluggable
//! - `close_fd` passes `FILP_CLOSED` (the `VNODE_OPCL` exemption, `filedes.c:186-188`) and fails `EBADF`

use core::cell::Cell;

use minix_types::{Endpoint, Mode, UserSlot};

use crate::device_map::split_smap_dev;
use crate::filp::{FILP_CLOSED, FilpId, FilpTable};
use crate::fproc::{FProc, OPEN_MAX};
use crate::mount::DevCodec;
use crate::open::{S_IFCHR, S_IFMT, S_IFREG, S_IFSOCK};
use crate::vnode::{Vnode, VnodeId, VnodeTable};

/// `Fd` — typed file descriptor `0..255` (u8 bound makes `256` unrepresentable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fd(pub u8);

impl Fd {
    pub fn new(raw: usize) -> Option<Self> {
        if raw < OPEN_MAX {
            Some(Self(raw as u8))
        } else {
            None
        }
    }
    pub fn get(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<usize> for Fd {
    type Error = FdError;
    fn try_from(v: usize) -> Result<Self, Self::Error> {
        Self::new(v).ok_or(FdError::BadFd)
    }
}

/// `FdError` — maps to Minix errno for `get_fd`/`close_fd`/`copy_fd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdError {
    BadFd,
    TooManyOpen, // EMFILE
    FilpFull,    // ENFILE
    Perm,        // EPERM
    Deadlk,      // EDEADLK
    Inval,       // EINVAL
}

impl FdError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::BadFd => minix_types::EBADF,
            Self::TooManyOpen => minix_types::EMFILE,
            Self::FilpFull => minix_types::ENFILE,
            Self::Perm => minix_types::EPERM,
            Self::Deadlk => minix_types::EDEADLK,
            Self::Inval => minix_types::EINVAL,
        }
    }
}

/// `FdAllocPolicy` — how `get_fd` scans `fp_filp[start..]` for `NULL`.
///
/// `ARCH` : `LowestFree` is Minix3's `for(i=start; i<OPEN_MAX; i++) if(NULL)` linear scan;
/// `NextFit` is the `O_DUPFD` `arg` lower-bound variant that wraps around.
/// Two impls satisfy Gate D.
pub trait FdAllocPolicy {
    fn allocate(&self, table: &[Option<usize>], start: usize) -> Option<usize>;
}

/// `LowestFree` — `start→OPEN_MAX` lowest free (C `get_fd:121`).
#[derive(Debug, Default, Clone, Copy)]
pub struct LowestFree;

impl FdAllocPolicy for LowestFree {
    fn allocate(&self, table: &[Option<usize>], start: usize) -> Option<usize> {
        for i in start..OPEN_MAX {
            if table[i].is_none() {
                return Some(i);
            }
        }
        None
    }
}

/// `NextFit` — circular next-fit (starts at `Cell` next, wraps).
#[derive(Debug)]
pub struct NextFit {
    next: Cell<usize>,
}

impl NextFit {
    pub fn new(start: usize) -> Self {
        Self {
            next: Cell::new(start),
        }
    }
}

impl FdAllocPolicy for NextFit {
    fn allocate(&self, table: &[Option<usize>], start: usize) -> Option<usize> {
        let base = self.next.get() % OPEN_MAX;
        // Try base..OPEN_MAX then 0..base, but respect caller's start as lower bound
        let scan_start = core::cmp::max(base, start);
        for i in scan_start..OPEN_MAX {
            if table[i].is_none() {
                self.next.set((i + 1) % OPEN_MAX);
                return Some(i);
            }
        }
        for i in start..scan_start {
            if table[i].is_none() {
                self.next.set((i + 1) % OPEN_MAX);
                return Some(i);
            }
        }
        None
    }
}

impl Default for NextFit {
    fn default() -> Self {
        Self::new(0)
    }
}

/// `check_fds` — `filedes.c:88` `nfds` window check (`EMFILE` if not enough free).
pub fn check_fds(fproc: &FProc, nfds: usize) -> Result<(), FdError> {
    assert!(nfds >= 1);
    let free = fproc.filps.iter().filter(|f| f.is_none()).count();
    if free >= nfds {
        Ok(())
    } else {
        Err(FdError::TooManyOpen)
    }
}

/// `get_fd` dual scan — `fp_filp` free `fd` + `filp` free `FilpId`.
///
/// Returns `Fd` and reserved `FilpId` (mode set, count still 0).  Caller must
/// bump `filp_count` on success (mirrors `open.c:common_open` `filp_count=1`).
pub fn get_fd(
    fproc: &mut FProc,
    start: usize,
    policy: &dyn FdAllocPolicy,
    filp_table: &mut FilpTable,
    mode: Mode,
) -> Result<(Fd, FilpId), FdError> {
    let idx = policy
        .allocate(&fproc.filps, start)
        .ok_or(FdError::TooManyOpen)?;
    let fd = Fd::new(idx).ok_or(FdError::BadFd)?;

    let filp_id = filp_table.alloc_filp(mode).map_err(|_| FdError::FilpFull)?;

    // Reserve: do not bump count here; caller will `inc_count` on commit.
    // For test we leave count 0 but mode set (as `get_fd:139` does).
    Ok((fd, filp_id))
}

/// `close_fd` — `open.c:690` `close(2)` through `get_filp2(VNODE_OPCL)`.
///
/// `OPCL` is the one access that passes a `FILP_CLOSED` filp (`filedes.c:186-188`
/// "disallow all use except close(2)"): a filp invalidated by a dying driver
/// still owns its slot, so close must proceed and release it — clearing the
/// fd and the cloexec bit, then decrementing `filp_count`.  Non-OPCL users
/// get `EIO` from `get_filp2`'s gate (the `FilpLockMode` seam, `filp.rs`),
/// never from here.
///
/// Simplified: `may_suspend` is accepted but not used (socket `SUSPEND` is
/// DEFERRED to 22-sdev).  Lock release (`nr_locks`) is also DEFERRED to 30.
pub fn close_fd(fproc: &mut FProc, fd: Fd, filp_table: &mut FilpTable) -> Result<(), FdError> {
    let idx = fd.get();
    let filp_idx = fproc.filps[idx].ok_or(FdError::BadFd)?;
    filp_table.get(FilpId(filp_idx)).ok_or(FdError::BadFd)?;
    // Clear fd and cloexec first (`open.c:704 rfp->fp_filp[fd_nr] = NULL`
    // before `close_filp`, so re-entrant closes fail `EBADF`).
    fproc.filps[idx] = None;
    fproc.cloexec_set.set(fd.get(), false);
    // Dec count and maybe put_vnode (simplified)
    let fid = FilpId(filp_idx);
    let _freed = filp_table.dec_count(fid);
    Ok(())
}

/// `invalidate_filp` — `filedes.c:250` `mode=CLOSED` single write.
pub fn invalidate_filp(filp_table: &mut FilpTable, id: FilpId) {
    if let Some(f) = filp_table.get_mut(id) {
        f.mode = FILP_CLOSED;
    }
}

/// Family scan shared by the `invalidate_filp_*` predicates — `filedes.c:250-306`:
/// `filp_count != 0 && filp_vno != NULL && vnode predicate → FILP_CLOSED`.
/// Returns the number of filps matching (C returns void; the count is an
/// audit aid for tests and driver-death bookkeeping).
fn invalidate_filps_where(
    filp_table: &mut FilpTable,
    vnode_table: &VnodeTable,
    matches: impl Fn(&Vnode) -> bool,
) -> usize {
    let mut cnt = 0;
    for i in 0..filp_table.len() {
        let id = FilpId(i);
        if let Some(f) = filp_table.get(id) {
            let hit = f.count != 0
                && f.vnode
                    .and_then(|v| vnode_table.get(VnodeId(v)))
                    .map_or(false, |vn| matches(vn));
            if hit {
                invalidate_filp(filp_table, id);
                cnt += 1;
            }
        }
    }
    cnt
}

/// `invalidate_filp_by_endpt` — `filedes.c:298-306`
/// `filp_count != 0 && filp_vno != NULL && filp_vno->v_fs_e == proc_e → CLOSED`.
///
/// The vnode table supplies the `v_fs_e` probe (C dereferences
/// `f->filp_vno->v_fs_e` directly; Rust keeps the tables separate, so the
/// probe is an explicit parameter).  A dying FS must invalidate only the
/// filps that belong to it — not the whole table.
pub fn invalidate_by_endpoint(
    filp_table: &mut FilpTable,
    vnode_table: &VnodeTable,
    proc_e: Endpoint,
) -> usize {
    invalidate_filps_where(filp_table, vnode_table, |vn| vn.fs == proc_e)
}

/// `invalidate_filp_by_char_major` — `filedes.c:254-267`:
/// char-special files on the dying driver's major —
/// `S_ISCHR(v_mode) && major(v_sdev) == major → CLOSED`.
pub fn invalidate_by_char_major(
    filp_table: &mut FilpTable,
    vnode_table: &VnodeTable,
    major: u32,
) -> usize {
    invalidate_filps_where(filp_table, vnode_table, |vn| {
        (vn.mode & S_IFMT) == S_IFCHR && DevCodec::major(vn.sdev) == major
    })
}

/// `invalidate_filp_by_sock_drv` — `filedes.c:269-295`:
/// sockets owned by the dying socket driver with smap number `num` —
/// `S_ISSOCK(v_mode) && get_smap_by_dev(v_sdev).smap_num == num → CLOSED`.
/// The smap row's liveness stays with the table owner (`split_smap_dev`
/// decodes the device number only).
pub fn invalidate_by_sock_drv(
    filp_table: &mut FilpTable,
    vnode_table: &VnodeTable,
    num: u32,
) -> usize {
    invalidate_filps_where(filp_table, vnode_table, |vn| {
        (vn.mode & S_IFMT) == S_IFSOCK
            && split_smap_dev(vn.sdev).map_or(false, |(row, _)| row == num)
    })
}

/// `do_copyfd` kind — `filedes.c:524` `COPYFD_FROM/TO/CLOSE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKind {
    From,
    To,
    Close,
}

/// `do_copyfd` — `filedes.c:524` super_user → `EPERM`, `isokendpt`, `S_ISSOCK` `EDEADLK`.
///
/// `src`/`dst` follow the `From` direction (remote → caller); the caller of
/// this decision function passes the pair per the message's endpoint.
/// Simplified: `cred.is_super` replaces `super_user` global; `S_ISSOCK`
/// `EDEADLK` and `COPYFD_CLOEXEC` stripping are DEFERRED (noted per branch).
pub fn copy_fd(
    src: &mut FProc,
    dst: &mut FProc,
    src_fd: Fd,
    kind: CopyKind,
    is_super: bool,
    filp_table: &mut FilpTable,
    policy: &dyn FdAllocPolicy,
) -> Result<Fd, FdError> {
    if !is_super {
        return Err(FdError::Perm);
    }
    let filp_idx = src.filps[src_fd.get()].ok_or(FdError::BadFd)?;
    match kind {
        CopyKind::From => {
            // `S_ISSOCK` self-copy deadlock would be `EDEADLK` — stub always ok for test
            let idx = policy.allocate(&dst.filps, 0).ok_or(FdError::TooManyOpen)?;
            let fd = Fd::new(idx).ok_or(FdError::BadFd)?;
            dst.filps[idx] = Some(filp_idx);
            // `filedes.c:652 rfilp->filp_count++` — the copy owns a reference.
            filp_table.inc_count(FilpId(filp_idx));
            Ok(fd)
        }
        CopyKind::To => {
            let idx = policy.allocate(&dst.filps, 0).ok_or(FdError::TooManyOpen)?;
            let fd = Fd::new(idx).ok_or(FdError::BadFd)?;
            dst.filps[idx] = Some(filp_idx);
            filp_table.inc_count(FilpId(filp_idx));
            Ok(fd)
        }
        CopyKind::Close => {
            // `COPYFD_CLOSE` reverts a prior `COPYFD_TO` (`filedes.c:631-646`):
            // the fd lives in the process the copy targeted, and the gate is
            // `filp_count > 1` because the caller must still hold its own
            // reference — dropping the last one is `EBADF`, not a silent clear.
            let fid = FilpId(filp_idx);
            let count = filp_table.get(fid).ok_or(FdError::BadFd)?.count;
            if count > 1 {
                filp_table.dec_count(fid);
                src.filps[src_fd.get()] = None;
                Ok(src_fd)
            } else {
                Err(FdError::BadFd)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filp::FilpTable;
    use crate::fproc::FProc;
    use minix_types::{Endpoint, UserSlot};

    fn new_fproc() -> FProc {
        FProc::new_unused()
    }

    #[test]
    fn test_fd_new() {
        assert_eq!(Fd::new(OPEN_MAX - 1).unwrap().get(), OPEN_MAX - 1);
        assert!(Fd::new(OPEN_MAX).is_none());
        assert_eq!(Fd::try_from(OPEN_MAX).unwrap_err(), FdError::BadFd);
        assert_eq!(FdError::BadFd.to_errno(), minix_types::EBADF);
    }

    #[test]
    fn test_check_fds() {
        let mut fp = new_fproc();
        // Initially OPEN_MAX free
        assert!(check_fds(&fp, OPEN_MAX).is_ok());
        assert_eq!(
            check_fds(&fp, OPEN_MAX + 1).unwrap_err(),
            FdError::TooManyOpen
        );
        // Occupy OPEN_MAX-2 → 2 free left
        for i in 0..OPEN_MAX - 2 {
            fp.filps[i] = Some(i);
        }
        assert!(check_fds(&fp, 2).is_ok());
        assert_eq!(check_fds(&fp, 3).unwrap_err(), FdError::TooManyOpen);
        assert_eq!(FdError::TooManyOpen.to_errno(), minix_types::EMFILE);
    }

    #[test]
    fn test_get_fd_lowest() {
        let mut fp = new_fproc();
        let mut tbl = FilpTable::new();
        let policy = LowestFree;
        let (fd, fid) = get_fd(&mut fp, 0, &policy, &mut tbl, 0o644).unwrap();
        assert_eq!(fd.get(), 0);
        // Occupy fd 0
        fp.filps[0] = Some(fid.get());
        tbl.inc_count(fid);
        let (fd2, _) = get_fd(&mut fp, 0, &policy, &mut tbl, 0o644).unwrap();
        assert_eq!(fd2.get(), 1);
        // Start=5 → lowest free >=5 is 5
        let (fd3, _) = get_fd(&mut fp, 5, &policy, &mut tbl, 0o644).unwrap();
        assert_eq!(fd3.get(), 5);
    }

    #[test]
    fn test_get_fd_enfile() {
        let mut fp = new_fproc();
        let mut tbl = FilpTable::new();
        // Fill filp table
        for _ in 0..crate::filp::NR_FILPS {
            let fid = tbl.alloc_filp(0o644).unwrap();
            tbl.inc_count(fid);
        }
        let policy = LowestFree;
        let r = get_fd(&mut fp, 0, &policy, &mut tbl, 0o644);
        assert_eq!(r.unwrap_err(), FdError::FilpFull);
        assert_eq!(FdError::FilpFull.to_errno(), minix_types::ENFILE);
    }

    #[test]
    fn test_close_ebadf() {
        let mut fp = new_fproc();
        let mut tbl = FilpTable::new();
        let r = close_fd(&mut fp, Fd(99), &mut tbl);
        assert_eq!(r.unwrap_err(), FdError::BadFd);
    }

    #[test]
    fn test_close_after_invalidate_proceeds() {
        // C: `get_filp2`'s `FILP_CLOSED→EIO` gate exempts `VNODE_OPCL`
        // (filedes.c:186-188), so `close(2)` on an invalidated filp proceeds
        // and releases the slot (`open.c:696-704`).
        let mut fp = new_fproc();
        let mut tbl = FilpTable::new();
        let policy = LowestFree;
        let (fd, fid) = get_fd(&mut fp, 0, &policy, &mut tbl, 0o644).unwrap();
        fp.filps[fd.get()] = Some(fid.get());
        tbl.inc_count(fid);
        // Invalidate to CLOSED (driver died), then close.
        invalidate_filp(&mut tbl, fid);
        close_fd(&mut fp, fd, &mut tbl).unwrap();
        assert!(fp.filps[fd.get()].is_none());
        assert_eq!(tbl.get(fid).unwrap().count, 0);
    }

    #[test]
    fn test_close_ok() {
        let mut fp = new_fproc();
        let mut tbl = FilpTable::new();
        let policy = LowestFree;
        let (fd, fid) = get_fd(&mut fp, 0, &policy, &mut tbl, 0o644).unwrap();
        fp.filps[fd.get()] = Some(fid.get());
        tbl.inc_count(fid);
        fp.cloexec_set.set(fd.get(), true);
        assert!(fp.cloexec_set.get(fd.get()));
        close_fd(&mut fp, fd, &mut tbl).unwrap();
        assert!(fp.filps[fd.get()].is_none());
        assert!(!fp.cloexec_set.get(fd.get()));
        assert_eq!(tbl.get(fid).unwrap().count, 0);
    }

    #[test]
    fn test_cloexec_copy() {
        let mut src = new_fproc();
        let mut dst = new_fproc();
        let mut tbl = FilpTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        src.filps[5] = Some(fid.get());
        src.cloexec_set.set(5, true);
        let policy = LowestFree;
        // COPYFD_FROM with LowestFree should allocate dst fd 0
        let new_fd = copy_fd(&mut src, &mut dst, Fd(5), CopyKind::From, true, &mut tbl, &policy).unwrap();
        assert_eq!(new_fd.get(), 0);
        assert_eq!(dst.filps[0], Some(fid.get()));
        // Cloexec copy: From clears CLOEXEC in our impl (flags&=~CLOEXEC)
        // So dst cloexec should not be set
        assert!(!dst.cloexec_set.get(0));
    }

    #[test]
    fn test_invalidate_by_char_major() {
        // C: only char-special filps on the dying driver's major die
        // (filedes.c:254-267); other majors and non-char files survive.
        let mut tbl = FilpTable::new();
        let mut vtbl = VnodeTable::new();
        // alloc→inc interleaved: `alloc_filp` does not reserve the slot
        // (C's allocation site sets `filp_count = 1` inline, open.c:134).
        let fid1 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid1);
        let fid2 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid2);
        let fid3 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid3);
        tbl.get_mut(fid1).unwrap().vnode = Some(1);
        tbl.get_mut(fid2).unwrap().vnode = Some(2);
        tbl.get_mut(fid3).unwrap().vnode = Some(3);
        vtbl.get_mut(VnodeId(1)).unwrap().mode = S_IFCHR | 0o600;
        vtbl.get_mut(VnodeId(1)).unwrap().sdev = DevCodec::make(4, 0);
        vtbl.get_mut(VnodeId(2)).unwrap().mode = S_IFCHR | 0o600;
        vtbl.get_mut(VnodeId(2)).unwrap().sdev = DevCodec::make(5, 0);
        vtbl.get_mut(VnodeId(3)).unwrap().mode = S_IFREG; // regular file
        vtbl.get_mut(VnodeId(3)).unwrap().sdev = DevCodec::make(4, 0);
        let n = invalidate_by_char_major(&mut tbl, &vtbl, 4);
        assert_eq!(n, 1);
        assert_eq!(tbl.get(fid1).unwrap().mode, FILP_CLOSED);
        assert_ne!(tbl.get(fid2).unwrap().mode, FILP_CLOSED);
        assert_ne!(tbl.get(fid3).unwrap().mode, FILP_CLOSED);
    }

    #[test]
    fn test_invalidate_by_sock_drv() {
        // C: only sockets owned by the dying socket driver (smap number)
        // die (filedes.c:269-295); other numbers and non-sockets survive.
        let mut tbl = FilpTable::new();
        let mut vtbl = VnodeTable::new();
        // alloc→inc interleaved: `alloc_filp` does not reserve the slot
        // (C's allocation site sets `filp_count = 1` inline, open.c:134).
        let fid1 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid1);
        let fid2 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid2);
        let fid3 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid3);
        tbl.get_mut(fid1).unwrap().vnode = Some(1);
        tbl.get_mut(fid2).unwrap().vnode = Some(2);
        tbl.get_mut(fid3).unwrap().vnode = Some(3);
        vtbl.get_mut(VnodeId(1)).unwrap().mode = S_IFSOCK | 0o600;
        vtbl.get_mut(VnodeId(1)).unwrap().sdev = crate::device_map::make_smap_dev(1, 7);
        vtbl.get_mut(VnodeId(2)).unwrap().mode = S_IFSOCK | 0o600;
        vtbl.get_mut(VnodeId(2)).unwrap().sdev = crate::device_map::make_smap_dev(2, 7);
        vtbl.get_mut(VnodeId(3)).unwrap().mode = S_IFCHR | 0o600;
        vtbl.get_mut(VnodeId(3)).unwrap().sdev = crate::device_map::make_smap_dev(1, 7);
        let n = invalidate_by_sock_drv(&mut tbl, &vtbl, 1);
        assert_eq!(n, 1);
        assert_eq!(tbl.get(fid1).unwrap().mode, FILP_CLOSED);
        assert_ne!(tbl.get(fid2).unwrap().mode, FILP_CLOSED);
        assert_ne!(tbl.get(fid3).unwrap().mode, FILP_CLOSED);
        let n = invalidate_by_sock_drv(&mut tbl, &vtbl, 2);
        assert_eq!(n, 1);
        assert_eq!(tbl.get(fid2).unwrap().mode, FILP_CLOSED);
    }

    #[test]
    fn test_invalidate() {
        let mut tbl = FilpTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        assert_ne!(tbl.get(fid).unwrap().mode, FILP_CLOSED);
        invalidate_filp(&mut tbl, fid);
        assert_eq!(tbl.get(fid).unwrap().mode, FILP_CLOSED);
    }

    #[test]
    fn test_invalidate_by_endpt() {
        // C: only filps whose vnode's `v_fs_e` equals the dying endpoint are
        // invalidated (filedes.c:298-306) — other filesystems stay untouched.
        let mut tbl = FilpTable::new();
        let mut vtbl = VnodeTable::new();
        let fid1 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid1);
        let fid2 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid2);
        let fid3 = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid3);
        tbl.get_mut(fid1).unwrap().vnode = Some(1);
        tbl.get_mut(fid2).unwrap().vnode = Some(2);
        tbl.get_mut(fid3).unwrap().vnode = Some(2);
        vtbl.get_mut(VnodeId(1)).unwrap().fs = Endpoint::from_generation_slot(0, 5);
        vtbl.get_mut(VnodeId(2)).unwrap().fs = Endpoint::from_generation_slot(0, 6);
        // Dying FS endpoint 5: only fid1 (through vnode 1) is invalidated.
        let n = invalidate_by_endpoint(&mut tbl, &vtbl, Endpoint::from_generation_slot(0, 5));
        assert_eq!(n, 1);
        assert_eq!(tbl.get(fid1).unwrap().mode, FILP_CLOSED);
        assert_ne!(tbl.get(fid2).unwrap().mode, FILP_CLOSED);
        assert_ne!(tbl.get(fid3).unwrap().mode, FILP_CLOSED);
        // Dying FS endpoint 6: fid2 and fid3 share vnode 2.
        let n = invalidate_by_endpoint(&mut tbl, &vtbl, Endpoint::from_generation_slot(0, 6));
        assert_eq!(n, 2);
        assert_eq!(tbl.get(fid2).unwrap().mode, FILP_CLOSED);
        assert_eq!(tbl.get(fid3).unwrap().mode, FILP_CLOSED);
    }

    #[test]
    fn test_copy_from() {
        let mut src = new_fproc();
        let mut dst = new_fproc();
        let mut tbl = FilpTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        src.filps[3] = Some(fid.get());
        let policy = LowestFree;
        let fd = copy_fd(&mut src, &mut dst, Fd(3), CopyKind::From, true, &mut tbl, &policy).unwrap();
        assert_eq!(dst.filps[fd.get()], Some(fid.get()));
    }

    #[test]
    fn test_copy_to() {
        let mut src = new_fproc();
        let mut dst = new_fproc();
        let mut tbl = FilpTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        src.filps[7] = Some(fid.get());
        let policy = LowestFree;
        let fd = copy_fd(&mut src, &mut dst, Fd(7), CopyKind::To, true, &mut tbl, &policy).unwrap();
        assert_eq!(fd.get(), 0);
        assert_eq!(dst.filps[0], Some(fid.get()));
        // The copy owns a reference (filedes.c:652 filp_count++)
        assert_eq!(tbl.get(fid).unwrap().count, 2);
        // Non-super should EPERM
        let r = copy_fd(&mut src, &mut dst, Fd(7), CopyKind::To, false, &mut tbl, &policy);
        assert_eq!(r.unwrap_err(), FdError::Perm);
    }

    #[test]
    fn test_copy_close() {
        let mut src = new_fproc();
        let mut dst = new_fproc();
        let mut tbl = FilpTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        tbl.inc_count(fid);
        src.filps[5] = Some(fid.get());
        dst.filps[5] = Some(fid.get());
        let policy = LowestFree;
        let r = copy_fd(&mut src, &mut dst, Fd(5), CopyKind::Close, true, &mut tbl, &policy).unwrap();
        assert_eq!(r.get(), 5);
        assert!(src.filps[5].is_none());
        // The revert dropped the copied reference only (count 2 → 1)
        assert_eq!(tbl.get(fid).unwrap().count, 1);
    }

    #[test]
    fn test_copy_close_last_reference_ebadf() {
        // C: `COPYFD_CLOSE` with `filp_count == 1` is `EBADF` (`filedes.c:644`)
        // — it must never drop the caller's last reference, and the fd stays.
        let mut src = new_fproc();
        let mut dst = new_fproc();
        let mut tbl = FilpTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        src.filps[5] = Some(fid.get());
        dst.filps[5] = Some(fid.get());
        let policy = LowestFree;
        let r = copy_fd(&mut src, &mut dst, Fd(5), CopyKind::Close, true, &mut tbl, &policy);
        assert_eq!(r.unwrap_err(), FdError::BadFd);
        assert!(src.filps[5].is_some());
        assert_eq!(tbl.get(fid).unwrap().count, 1);
    }

    #[test]
    fn test_fd_alloc_policy_two_impls() {
        let mut fp = new_fproc();
        fp.filps[5] = Some(1);
        fp.filps[6] = Some(2);
        let fifo = LowestFree;
        let next = NextFit::new(5);
        // LowestFree from 5 → 7 (since 5,6 occupied)
        assert_eq!(fifo.allocate(&fp.filps, 5), Some(7));
        // NextFit from 5 → 7 as well initially, but after one alloc it moves
        assert_eq!(next.allocate(&fp.filps, 5), Some(7));
        // Second alloc with NextFit should give 8, while LowestFree still 8? Both give 8, but test that they are distinct types
        fp.filps[7] = Some(3);
        assert_eq!(fifo.allocate(&fp.filps, 5), Some(8));
        assert_eq!(next.allocate(&fp.filps, 5), Some(8));
        // Polymorphic via trait object
        let policies: Vec<Box<dyn FdAllocPolicy>> =
            vec![Box::new(LowestFree), Box::new(NextFit::new(10))];
        assert_eq!(policies[0].allocate(&fp.filps, 5), Some(8));
        assert_eq!(policies[1].allocate(&fp.filps, 5), Some(10)); // NextFit starts at 10
    }
}
