//! File descriptor table operations — `filedes.c:88-656` + `open.c:690 close_fd`.
//!
//! `fd` (`fp_filp[256]` private index) vs `filp` (`filp[1024]` shared pool).
//! `FD_CLOEXEC` bitmap, `FILP_CLOSED` sentinel (`EIO`), `EMFILE` vs `ENFILE`,
//! `COPYFD` three-way, `invalidate_filp` family.
//!
//! Design notes (14-filedes.md §3):
//! - `Fd(u8)` newtype makes `256` bound type-safe (ARCH A-8)
//! - `FdAllocPolicy` parametrizes the `start→OPEN_MAX` scan (`O_DUPFD`'s `arg`); `NextFitDemo` (cfg(test)) is a test-only contrast, not a C policy
//! - `close_fd` passes `FILP_CLOSED` (the `VNODE_OPCL` exemption, `filedes.c:186-188`) and fails `EBADF`

#[cfg(test)]
use core::cell::Cell;

use minix_types::{Endpoint, Mode, UserSlot};

use crate::device_map::{smap_endpt_by_dev, split_smap_dev, SmapTable};
use crate::filp::{FILP_CLOSED, FilpId, FilpTable};
use crate::fproc::{FProc, OPEN_MAX};
use crate::mount::DevCodec;
use crate::open::{S_IFCHR, S_IFMT, S_IFSOCK};
#[cfg(test)]
use crate::open::S_IFREG;
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

impl minix_types::ToErrno for FdError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
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
/// `LowestFree` is the only real policy in either OS: Minix3's
/// `for(i=start; i<OPEN_MAX; i++) if(NULL)` (`filedes.c:121`) and Linux's
/// `alloc_fd(start, end)` are both lowest-free-from-`start`; `O_DUPFD`
/// reuses the same policy with `start = arg`.  The trait exists to make
/// that `start` parametrization injectable; `NextFitDemo` (cfg(test)) is a
/// polymorphism contrast, not a second C policy.
pub trait FdAllocPolicy {
    fn allocate(&self, table: &[Option<usize>], start: usize) -> Option<usize>;
}

/// `LowestFree` — `start→OPEN_MAX` lowest free (C `get_fd:121`).
#[derive(Debug, Default, Clone, Copy)]
pub struct LowestFree;

impl FdAllocPolicy for LowestFree {
    fn allocate(&self, table: &[Option<usize>], start: usize) -> Option<usize> {
        table[start..OPEN_MAX]
            .iter()
            .position(|slot| slot.is_none())
            .map(|offset| start + offset)
    }
}

/// Circular next-fit — `cfg(test)` contrast for [`FdAllocPolicy`].
///
/// No C origin: neither Minix3 nor Linux allocates fds by wrapped
/// next-fit.  Kept (test-only, renamed from `NextFit`) so the trait's
/// polymorphism has a behaviourally different second impl to test against.
#[cfg(test)]
#[derive(Debug)]
pub struct NextFitDemo {
    next: Cell<usize>,
}

#[cfg(test)]
impl NextFitDemo {
    pub fn new(start: usize) -> Self {
        Self {
            next: Cell::new(start),
        }
    }
}

#[cfg(test)]
impl FdAllocPolicy for NextFitDemo {
    fn allocate(&self, table: &[Option<usize>], start: usize) -> Option<usize> {
        let base = self.next.get() % OPEN_MAX;
        // Try base..OPEN_MAX then 0..base, but respect caller's start as lower bound
        let scan_start = core::cmp::max(base, start);
        let find = |range: core::ops::Range<usize>| -> Option<usize> {
            let offset = table[range.clone()]
                .iter()
                .position(|slot| slot.is_none())?;
            Some(offset + range.start)
        };
        match find(scan_start..OPEN_MAX).or_else(|| find(start..scan_start)) {
            Some(i) => {
                self.next.set((i + 1) % OPEN_MAX);
                Some(i)
            }
            None => None,
        }
    }
}

#[cfg(test)]
impl Default for NextFitDemo {
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
/// The C `may_suspend` flag (`filedes.c:414`) only ever matters on the socket
/// last close (`sdev_close`, `filedes.c:475`: only close(2) may SUSPEND).  The
/// Rust close paths model `may_suspend=FALSE` (`socket.rs` `CloseTarget`), and
/// the suspending seam belongs to `22-sdev`.  POSIX record locks (`nr_locks`)
/// are likewise out of this seam until `30-fcntl-lock`.
pub fn close_fd(fproc: &mut FProc, fd: Fd, filp_table: &mut FilpTable) -> Result<(), FdError> {
    let idx = fd.get();
    let filp_idx = fproc.filps[idx].ok_or(FdError::BadFd)?;
    filp_table.get(FilpId(filp_idx)).ok_or(FdError::BadFd)?;
    // Clear fd and cloexec first (`open.c:704 rfp->fp_filp[fd_nr] = NULL`
    // before `close_filp`, so re-entrant closes fail `EBADF`).
    fproc.filps[idx] = None;
    fproc.cloexec_set.set(fd.get(), false);
    // Last close (`--filp_count == 0`, `filedes.c:496`) clears the filp slot
    // (`dec_count`: vnode drop + `FILP_CLOSED`).  C fans out further at last
    // close — device close (`filedes.c:451/:453/:475`), pipe waiter release
    // (`:493`), FIFO size report + `put_vnode` (`:496-505`) — each wired at
    // its own consumer (`20-bdev`/`21-cdev`/`22-sdev`, `17-pipe`,
    // `05-vnode-table`), not in this fd-table seam.
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
                    .is_some_and(&matches);
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
            && split_smap_dev(vn.sdev).is_some_and(|(row, _)| row == num)
    })
}

/// `do_copyfd` kind — `filedes.c:524` `COPYFD_FROM/TO/CLOSE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKind {
    From,
    To,
    Close,
}

/// Context for [`copy_fd`] — C's implicit environment made explicit
/// (the `super_user`/`who_e` globals and the shared tables; `FreeCtx` 同型).
pub struct CopyFdCtx<'a> {
    pub filp_table: &'a mut FilpTable,
    pub vnode_table: &'a VnodeTable,
    pub smap_table: &'a SmapTable,
    pub policy: &'a dyn FdAllocPolicy,
    /// `who_e` — the back-calling driver (UDS/VND), for the `S_ISSOCK`
    /// self-copy deadlock check.
    pub caller_endpoint: Endpoint,
    /// The remote process's slot — `filp_ioctl_fp`'s identity compare.
    pub remote_slot: UserSlot,
    /// `super_user` — `COPYFD` is a driver back-call privilege.
    pub is_super: bool,
    /// `COPYFD_CLOEXEC` from `what` (`filedes.c:546`).
    pub cloexec: bool,
}

/// `do_copyfd` — `filedes.c:524-650`.  Direction is decided by `kind`:
///
/// - `From`: filp read from the **remote** table, installed in the
///   **caller's** (`filedes.c:600-602` redirects `rfp = fp`; `COPYFD_CLOEXEC`
///   is stripped — a copied-in fd never starts life close-on-exec).
/// - `To`: filp read from the **caller's** table, installed in the
///   **remote's** with `COPYFD_CLOEXEC` honored (`filedes.c:568`).
/// - `Close`: revert a prior `To` — gate `filp_count > 1`, then clear the
///   **remote's** fd (`filedes.c:631-646`).
///
/// Guards shared by all three: `super_user → EPERM`, the `filp_ioctl_fp`
/// VND self-IOCTL probe (`:582-585`), and — for `From` on a socket — the
/// owning-driver self-copy `EDEADLK` check (`:606-613`).
pub fn copy_fd(
    caller: &mut FProc,
    remote: &mut FProc,
    fd: Fd,
    kind: CopyKind,
    ctx: CopyFdCtx<'_>,
) -> Result<Fd, FdError> {
    if !ctx.is_super {
        return Err(FdError::Perm);
    }
    // `rfilp = get_filp2((what == COPYFD_TO) ? fp : rfp, fd, VNODE_NONE)` —
    // the filp comes from the caller for `To`, from the remote otherwise.
    let filp_idx = match kind {
        CopyKind::To => caller.filps[fd.get()],
        CopyKind::From | CopyKind::Close => remote.filps[fd.get()],
    }
    .ok_or(FdError::BadFd)?;
    let fid = FilpId(filp_idx);
    {
        let f = ctx.filp_table.get(fid).ok_or(FdError::BadFd)?;
        // VND deadlock guard: the remote process is blocked in an IOCTL on
        // this very filp (`filedes.c:582-585`).
        if f.ioctl_holder == Some(ctx.remote_slot) {
            return Err(FdError::BadFd);
        }
    }
    match kind {
        CopyKind::From => {
            // Owning-driver self-copy: a socket whose driver is the caller
            // itself must not be copied back into it (`filedes.c:606-613`).
            let self_copy = ctx
                .filp_table
                .get(fid)
                .and_then(|f| f.vnode)
                .and_then(|v| ctx.vnode_table.get(VnodeId(v)))
                .is_some_and(|vn| {
                    (vn.mode & S_IFMT) == S_IFSOCK
                        && smap_endpt_by_dev(ctx.smap_table, vn.sdev)
                            == Some(ctx.caller_endpoint)
                });
            if self_copy {
                return Err(FdError::Deadlk);
            }
            // `rfp = fp; flags &= ~COPYFD_CLOEXEC` — install in the caller,
            // never close-on-exec (`filedes.c:600-602`).
            let idx = ctx.policy.allocate(&caller.filps, 0).ok_or(FdError::TooManyOpen)?;
            let new_fd = Fd::new(idx).ok_or(FdError::BadFd)?;
            caller.filps[idx] = Some(filp_idx);
            ctx.filp_table.inc_count(fid);
            Ok(new_fd)
        }
        CopyKind::To => {
            // Install in the remote with `COPYFD_CLOEXEC` honored
            // (`filedes.c:617-624`).
            let idx = ctx.policy.allocate(&remote.filps, 0).ok_or(FdError::TooManyOpen)?;
            let new_fd = Fd::new(idx).ok_or(FdError::BadFd)?;
            remote.filps[idx] = Some(filp_idx);
            if ctx.cloexec {
                remote.cloexec_set.set(new_fd.get(), true);
            }
            ctx.filp_table.inc_count(fid);
            Ok(new_fd)
        }
        CopyKind::Close => {
            // `COPYFD_CLOSE` reverts a prior `To`: gate `filp_count > 1`
            // (the caller still holds its own reference), then clear the
            // remote's fd (`filedes.c:631-646`).
            let count = ctx.filp_table.get(fid).ok_or(FdError::BadFd)?.count;
            if count > 1 {
                ctx.filp_table.dec_count(fid);
                remote.filps[fd.get()] = None;
                Ok(fd)
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

    /// `CopyFdCtx` with a plain caller/remote pair and empty smap — helpers.
    fn copy_ctx<'a>(
        tbl: &'a mut FilpTable,
        vtbl: &'a VnodeTable,
        stbl: &'a SmapTable,
        policy: &'a dyn FdAllocPolicy,
        remote_slot: UserSlot,
    ) -> CopyFdCtx<'a> {
        CopyFdCtx {
            filp_table: tbl,
            vnode_table: vtbl,
            smap_table: stbl,
            policy,
            caller_endpoint: Endpoint::from_generation_slot(0, 9),
            remote_slot,
            is_super: true,
            cloexec: false,
        }
    }

    #[test]
    fn test_cloexec_copy() {
        // COPYFD_FROM: filp comes from the REMOTE, lands in the CALLER, and
        // never starts life close-on-exec (`filedes.c:600-602` strips the
        // flag).  The source fd's own cloexec bit is irrelevant.
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let vtbl = VnodeTable::new();
        let stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        remote.filps[5] = Some(fid.get());
        remote.cloexec_set.set(5, true);
        let policy = LowestFree;
        let ctx = copy_ctx(&mut tbl, &vtbl, &stbl, &policy, UserSlot::new(1));
        let new_fd = copy_fd(&mut caller, &mut remote, Fd(5), CopyKind::From, ctx).unwrap();
        assert_eq!(new_fd.get(), 0);
        assert_eq!(caller.filps[0], Some(fid.get()));
        assert!(!caller.cloexec_set.get(0));
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
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let vtbl = VnodeTable::new();
        let stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        remote.filps[3] = Some(fid.get());
        let policy = LowestFree;
        let ctx = copy_ctx(&mut tbl, &vtbl, &stbl, &policy, UserSlot::new(1));
        let fd = copy_fd(&mut caller, &mut remote, Fd(3), CopyKind::From, ctx).unwrap();
        assert_eq!(caller.filps[fd.get()], Some(fid.get()));
    }

    #[test]
    fn test_copy_to() {
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let vtbl = VnodeTable::new();
        let stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        caller.filps[7] = Some(fid.get());
        let policy = LowestFree;
        // COPYFD_TO: filp comes from the CALLER, lands in the REMOTE with
        // COPYFD_CLOEXEC honored (`filedes.c:617-624`).
        let ctx = CopyFdCtx {
            filp_table: &mut tbl,
            vnode_table: &vtbl,
            smap_table: &stbl,
            policy: &policy,
            caller_endpoint: Endpoint::from_generation_slot(0, 9),
            remote_slot: UserSlot::new(1),
            is_super: true,
            cloexec: true,
        };
        let fd = copy_fd(&mut caller, &mut remote, Fd(7), CopyKind::To, ctx).unwrap();
        assert_eq!(fd.get(), 0);
        assert_eq!(remote.filps[0], Some(fid.get()));
        assert!(remote.cloexec_set.get(0));
        // The copy owns a reference (filedes.c:652 filp_count++)
        assert_eq!(tbl.get(fid).unwrap().count, 2);
        // Non-super should EPERM
        let ctx = CopyFdCtx {
            filp_table: &mut tbl,
            vnode_table: &vtbl,
            smap_table: &stbl,
            policy: &policy,
            caller_endpoint: Endpoint::from_generation_slot(0, 9),
            remote_slot: UserSlot::new(1),
            is_super: false,
            cloexec: false,
        };
        let r = copy_fd(&mut caller, &mut remote, Fd(7), CopyKind::To, ctx);
        assert_eq!(r.unwrap_err(), FdError::Perm);
    }

    #[test]
    fn test_copy_from_self_socket_edeadlk() {
        // A socket filp owned by the calling driver itself must not be
        // copied back into it (`filedes.c:606-613` → EDEADLK).
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let mut vtbl = VnodeTable::new();
        let mut stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        remote.filps[3] = Some(fid.get());
        tbl.get_mut(fid).unwrap().vnode = Some(1);
        vtbl.get_mut(VnodeId(1)).unwrap().mode = S_IFSOCK | 0o600;
        vtbl.get_mut(VnodeId(1)).unwrap().sdev = crate::device_map::make_smap_dev(1, 7);
        stbl.entries[0].endpt = Some(Endpoint::from_generation_slot(0, 9));
        let policy = LowestFree;
        let ctx = CopyFdCtx {
            filp_table: &mut tbl,
            vnode_table: &vtbl,
            smap_table: &stbl,
            policy: &policy,
            caller_endpoint: Endpoint::from_generation_slot(0, 9),
            remote_slot: UserSlot::new(1),
            is_super: true,
            cloexec: false,
        };
        let r = copy_fd(&mut caller, &mut remote, Fd(3), CopyKind::From, ctx);
        assert_eq!(r.unwrap_err(), FdError::Deadlk);
    }

    #[test]
    fn test_copy_to_ioctl_holder_ebadf() {
        // The remote process is blocked in an IOCTL on this very filp — VND
        // deadlock guard rejects the copy (`filedes.c:582-585` → EBADF).
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let vtbl = VnodeTable::new();
        let stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        caller.filps[7] = Some(fid.get());
        tbl.get_mut(fid).unwrap().ioctl_holder = Some(UserSlot::new(1));
        let policy = LowestFree;
        let ctx = CopyFdCtx {
            filp_table: &mut tbl,
            vnode_table: &vtbl,
            smap_table: &stbl,
            policy: &policy,
            caller_endpoint: Endpoint::from_generation_slot(0, 9),
            remote_slot: UserSlot::new(1),
            is_super: true,
            cloexec: false,
        };
        let r = copy_fd(&mut caller, &mut remote, Fd(7), CopyKind::To, ctx);
        assert_eq!(r.unwrap_err(), FdError::BadFd);
    }

    #[test]
    fn test_copy_close() {
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let vtbl = VnodeTable::new();
        let stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        tbl.inc_count(fid);
        // The reverted fd lives in the REMOTE (`filedes.c:631-646`).
        remote.filps[5] = Some(fid.get());
        caller.filps[5] = Some(fid.get());
        let policy = LowestFree;
        let ctx = copy_ctx(&mut tbl, &vtbl, &stbl, &policy, UserSlot::new(1));
        let r = copy_fd(&mut caller, &mut remote, Fd(5), CopyKind::Close, ctx).unwrap();
        assert_eq!(r.get(), 5);
        assert!(remote.filps[5].is_none());
        assert!(caller.filps[5].is_some());
        // The revert dropped the copied reference only (count 2 → 1)
        assert_eq!(tbl.get(fid).unwrap().count, 1);
    }

    #[test]
    fn test_copy_close_last_reference_ebadf() {
        // C: `COPYFD_CLOSE` with `filp_count == 1` is `EBADF` (`filedes.c:644`)
        // — it must never drop the caller's last reference, and the fd stays.
        let mut caller = new_fproc();
        let mut remote = new_fproc();
        let mut tbl = FilpTable::new();
        let vtbl = VnodeTable::new();
        let stbl = SmapTable::new();
        let fid = tbl.alloc_filp(0o644).unwrap();
        tbl.inc_count(fid);
        remote.filps[5] = Some(fid.get());
        caller.filps[5] = Some(fid.get());
        let policy = LowestFree;
        let ctx = copy_ctx(&mut tbl, &vtbl, &stbl, &policy, UserSlot::new(1));
        let r = copy_fd(&mut caller, &mut remote, Fd(5), CopyKind::Close, ctx);
        assert_eq!(r.unwrap_err(), FdError::BadFd);
        assert!(remote.filps[5].is_some());
        assert_eq!(tbl.get(fid).unwrap().count, 1);
    }

    #[test]
    fn test_fd_alloc_policy_two_impls() {
        let mut fp = new_fproc();
        fp.filps[5] = Some(1);
        fp.filps[6] = Some(2);
        let fifo = LowestFree;
        let next = NextFitDemo::new(5);
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
            vec![Box::new(LowestFree), Box::new(NextFitDemo::new(10))];
        assert_eq!(policies[0].allocate(&fp.filps, 5), Some(8));
        assert_eq!(policies[1].allocate(&fp.filps, 5), Some(10)); // NextFit starts at 10
    }
}
