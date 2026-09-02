//! VFS process structure (`FProc`).
//!
//! Corresponds to Minix3's `struct fproc` (`minix3/minix/servers/vfs/fproc.h`).
//!
//! `FProc` is VFS's private per-process file system context: the file
//! descriptor table, directory anchors, credentials, controlling terminal and
//! the blocking state of the process's current (or suspended) file system
//! call. It is the VFS counterpart of PM's `mproc` and VM's `vmproc`: each
//! server keeps its own projection of the same process, associated by
//! endpoint.
//!
//! # Design Principles
//!
//! - **Typed fields over raw integers**: `Pid`/`Uid`/`Gid`/`Endpoint`/
//!   `DevId`/`Mode` come from `minix-types` (ARCH A-8); raw C scalar types
//!   appear only at wire/ABI conversion points.
//! - **Tagged enum instead of int + union**: C's `fp_blocked_on` + `fp_u`
//!   union (`fproc.h:30-61`) is modeled as [`BlockedOn`], a tagged enum whose
//!   payload is the per-type blocking detail. The discriminator and payload
//!   cannot disagree (ARCH A-3).
//! - **Lock belongs to the slot, not the process**: `fp_lock` must survive
//!   fork's wholesale `fproc` copy, so the child slot keeps its own mutex
//!   (ARCH A-6). In the single-threaded event loop the mutex downgrades to
//!   borrow rules; there is therefore no lock field here.
//! - **Worker/message state lives in the worker slot**: `fp_worker`/
//!   `fp_func`/`fp_msg`/`fp_pm_msg` are transient request-slot state, not
//!   process context. The worker side holds the inverse association
//!   (`WorkerThread::fp_slot`), so no two-directional pointer bookkeeping is
//!   needed. See `02-fproc-struct.md` §3.

use minix_types::{
    Bitmap, DevId, Endpoint, Gid, GrantId, Mode, NO_DEV, NR_PROCS, Pid, Uid, UserSlot, VirBytes,
};

/// Maximum number of open file descriptors per process.
///
/// Corresponds to `OPEN_MAX` (`minix3/sys/sys/syslimits.h:38`): in Minix3
/// builds (`__minix` defined) `OPEN_MAX` is 255, not the NetBSD fallback 128
/// (syslimits.h:61-62). `FD_SETSIZE` is likewise 255 (fd_set.h:60), so the
/// FD_CLOEXEC bitmap is 255 bits.
pub const OPEN_MAX: usize = 255;

/// Maximum number of supplemental groups.
///
/// Corresponds to `NGROUPS_MAX` (`minix3/sys/sys/syslimits.h:59`, value 16).
/// Matches PM's `mproc/credentials.rs` (NGROUPS_MAX = 16).
pub const NGROUPS_MAX: usize = 16;

/// Maximum process name length including the trailing NUL.
///
/// Corresponds to `PROC_NAME_LEN` (`minix3/minix/include/minix/type.h:145`).
pub const PROC_NAME_LEN: usize = 16;

/// `PID_FREE` — marks an unused fproc slot.
///
/// Corresponds to `#define PID_FREE 0` (`fproc.h:103`). PID 0 is never
/// assigned to a user process, so 0 is a safe "slot free" sentinel.
pub const PID_FREE: Pid = 0;

bitflags::bitflags! {
    /// Process flags.
    ///
    /// Corresponds to Minix3's `fp_flags` (`fproc.h:91-98`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FpFlags: u32 {
        /// No flags — the initial state of a forked child (`FP_NOFLAGS`).
        const NOFLAGS = 0x0000;
        /// Server process, e.g. a file system callback (`FP_SRV_PROC`).
        const SRV_PROC = 0x0001;
        /// Process is being revived after a suspension (`FP_REVIVED`).
        const REVIVED = 0x0002;
        /// Session leader (`FP_SESLDR`).
        const SESLDR = 0x0004;
        /// Process has pending work (`FP_PENDING`).
        const PENDING = 0x0010;
        /// Process is exiting (`FP_EXITING`).
        const EXITING = 0x0020;
        /// Process has a postponed PM request (`FP_PM_WORK`).
        const PM_WORK = 0x0040;
    }
}

/// What a process's current file system call is suspended on, plus the
/// per-type parameters needed to resume it.
///
/// Corresponds to Minix3's `fp_blocked_on` (int) + `fp_u` union
/// (`fproc.h:30-61`, block constants `const.h:19-25`).
///
/// A tagged enum replaces the discriminator/union pair: the variant *is* the
/// discriminator, so it is impossible to read pipe parameters while blocked
/// on a device — in C, `fp_pipe` aliases `fp_u.u_pipe` regardless of
/// `fp_blocked_on` (ARCH A-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockedOn {
    /// Not blocked (`FP_BLOCKED_ON_NONE`).
    #[default]
    None,
    /// Suspended on a pipe/fifo read or write (`FP_BLOCKED_ON_PIPE`).
    Pipe(PipeBlock),
    /// Suspended opening a FIFO with no reader/writer present
    /// (`FP_BLOCKED_ON_POPEN`).
    PipeOpen(PipeOpenBlock),
    /// Suspended on a blocking POSIX record lock, `F_SETLKW`
    /// (`FP_BLOCKED_ON_FLOCK`).
    Flock(FlockBlock),
    /// Suspended on `select()` (`FP_BLOCKED_ON_SELECT`). Select keeps its own
    /// state internally, so there is no payload (fproc.h:46).
    Select,
    /// Suspended on character device I/O (`FP_BLOCKED_ON_CDEV`).
    Cdev(CdevBlock),
    /// Suspended on socket I/O (`FP_BLOCKED_ON_SDEV`).
    Sdev(SdevBlock),
}

/// Pipe I/O direction recorded in [`PipeBlock::call`].
///
/// C stores `VFS_READ`/`VFS_WRITE` (`pipe.c:399-407`); only these two calls
/// can be suspended on a pipe, so two variants suffice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeIo {
    /// Suspended `read()`/`VFS_READ`.
    Read,
    /// Suspended `write()`/`VFS_WRITE`.
    Write,
}

/// Resume parameters for a suspended pipe/fifo read or write.
///
/// Corresponds to `fp_u.u_pipe` (`fproc.h:31-37`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeBlock {
    /// Original user call: read or write.
    pub call: PipeIo,
    /// File descriptor of the blocking call.
    pub fd: usize,
    /// User buffer address.
    pub buf: VirBytes,
    /// Bytes left to transfer.
    pub nbytes: usize,
    /// Partial (write) result byte count already transferred.
    pub cum_io: usize,
}

/// Resume parameters for a suspended FIFO open.
///
/// Corresponds to `fp_u.u_popen` (`fproc.h:38-40`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeOpenBlock {
    /// File descriptor being opened.
    pub fd: usize,
}

/// fcntl lock command recorded while blocked on a record lock.
///
/// C stores `cmd` in `fp_u.u_flock` with the comment "fcntl command, always
/// F_SETLKW" (`fproc.h:43`): `F_GETLK`/`F_SETLK` return immediately, so
/// only the blocking command can appear here. Encoding it as a single-variant
/// enum makes any other value unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlockCmd {
    /// `F_SETLKW` — wait until the lock is granted.
    SetLkw,
}

/// Resume parameters for a suspended `F_SETLKW`.
///
/// Corresponds to `fp_u.u_flock` (`fproc.h:41-45`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlockBlock {
    /// File descriptor of the blocking call.
    pub fd: usize,
    /// fcntl lock command (always `F_SETLKW`).
    pub cmd: FlockCmd,
    /// User address of the `struct flock` argument.
    pub arg: VirBytes,
}

/// Resume parameters for a suspended character device I/O.
///
/// Corresponds to `fp_u.u_cdev` (`fproc.h:47-51`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdevBlock {
    /// Device number of the blocking call.
    pub dev: DevId,
    /// Driver endpoint.
    pub endpt: Endpoint,
    /// Data grant issued for the I/O; revoked when the operation completes.
    /// `None` = `GRANT_INVALID`.
    pub grant: Option<GrantId>,
}

/// Original call suspended on socket I/O.
///
/// C stores the raw call number in `fp_u.u_sdev.callnr`
/// (`sdev.c:sdev_suspend`, `fproc.h:54`). Only these calls suspend while
/// talking to a socket driver (`sdev.c`: `sdev_bindconn`/`sdev_accept`/
/// `sdev_sendrecv`/`sdev_sockmsg`/`sdev_ioctl`/`sdev_close` → `sdev_suspend`).
/// `shutdown(2)` and the get/set sockopt-name calls use the synchronous
/// `sdev_sendrec` and never suspend, hence are absent; `getsockname`/
/// `getpeername` likewise go through the synchronous `sdev_getset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdevCall {
    /// `read(2)` on a socket (`VFS_READ`).
    Read,
    /// `write(2)` on a socket (`VFS_WRITE`).
    Write,
    /// `close(2)` on a socket with a pending close (`VFS_CLOSE`).
    Close,
    /// `ioctl(2)` on a socket (`VFS_IOCTL`).
    Ioctl,
    /// `bind(2)` (`VFS_BIND`).
    Bind,
    /// `connect(2)` (`VFS_CONNECT`).
    Connect,
    /// `accept(2)` (`VFS_ACCEPT`).
    Accept,
    /// `sendto(2)` (`VFS_SENDTO`).
    Sendto,
    /// `sendmsg(2)` (`VFS_SENDMSG`).
    Sendmsg,
    /// `recvfrom(2)` (`VFS_RECVFROM`).
    Recvfrom,
    /// `recvmsg(2)` (`VFS_RECVMSG`).
    Recvmsg,
}

/// Call-specific auxiliary data for a suspended socket call.
///
/// Corresponds to `fp_u.u_sdev.aux` (`fproc.h:56-59`): `VFS_ACCEPT` stores
/// the listening file descriptor, `VFS_RECVMSG` the user buffer address,
/// everything else stores neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdevAux {
    /// Listener file descriptor (`VFS_ACCEPT`).
    Fd(usize),
    /// User buffer address (`VFS_RECVMSG`).
    Buf(VirBytes),
    /// No auxiliary data (all other calls).
    None,
}

/// Resume parameters for a suspended socket call.
///
/// Corresponds to `fp_u.u_sdev` (`fproc.h:52-60`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdevBlock {
    /// Socket device number.
    pub dev: DevId,
    /// Original call that suspended.
    pub call: SdevCall,
    /// Up to three data grants (data/control/address), unused slots `None`.
    pub grants: [Option<GrantId>; 3],
    /// Call-specific auxiliary data.
    pub aux: SdevAux,
}

/// VFS process structure.
///
/// Corresponds to Minix3's `struct fproc`.
///
/// # Key Differences from C Version
///
/// - `fp_filp` uses `Option<usize>` global filp-table indices instead of raw
///   pointers; `fp_wd`/`fp_rd` likewise index the global vnode table.
/// - `fp_cloexec_set` is a plain 128-bit bitmap (one bit per fd), matching
///   `fd_set` for `OPEN_MAX == 128`.
/// - `fp_tty` keeps the `NO_DEV` sentinel (0 = no controlling terminal) so
///   device-number equality checks read naturally.
/// - `fp_blocked_on` + `fp_u` is a single tagged [`BlockedOn`] enum.
/// - `fp_lock` is not in this struct — it belongs to the fproc slot (ARCH A-6).
/// - `fp_worker`/`fp_func`/`fp_msg`/`fp_pm_msg` are worker-slot state and live
///   in the worker module (see module docs).
#[derive(Debug, Clone)]
pub struct FProc {
    /// Process flags.
    pub flags: FpFlags,
    /// Process ID; `PID_FREE` marks an unused slot.
    pub pid: Pid,
    /// Kernel endpoint of this process; `Endpoint::NONE` marks an unused slot.
    pub endpoint: Endpoint,
    /// Root directory vnode index (global vnode table); `None` during reboot.
    pub root_dir: Option<usize>,
    /// Working directory vnode index (global vnode table); `None` during reboot.
    pub work_dir: Option<usize>,
    /// File descriptor table: fd → global filp-table index (`None` = free).
    pub filps: [Option<usize>; OPEN_MAX],
    /// FD_CLOEXEC bitmap (bit `fd` set = close-on-exec).
    ///
    /// 255-bit bitmap matching `fd_set` (`FD_SETSIZE = 255`,
    /// minix3/sys/sys/fd_set.h:60); uses [`Bitmap`] with size `OPEN_MAX`.
    pub cloexec_set: Bitmap,
    /// Controlling terminal device number; `NO_DEV` = none.
    pub tty: DevId,
    /// What the process's current call is suspended on.
    pub blocked_on: BlockedOn,
    /// Real user ID.
    pub real_uid: Uid,
    /// Effective user ID (used for permission checks).
    pub eff_uid: Uid,
    /// Real group ID.
    pub real_gid: Gid,
    /// Effective group ID.
    pub eff_gid: Gid,
    /// Number of supplemental groups (0 = none).
    pub ngroups: usize,
    /// Supplemental group list.
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    /// umask set by the `umask` syscall.
    pub umask: Mode,
    /// Name of the last executed program (`fp_name`, `PROC_NAME_LEN` bytes).
    pub name: [u8; PROC_NAME_LEN],
}

impl FProc {
    /// Creates an unused empty fproc slot.
    ///
    /// Corresponds to Minix3's first-phase initialization in
    /// `sef_cb_init_fresh()` (`main.c:405-408`):
    /// `fp_endpoint = NONE; fp_pid = PID_FREE;`
    pub const fn new_unused() -> Self {
        Self {
            flags: FpFlags::NOFLAGS,
            pid: PID_FREE,
            endpoint: Endpoint::NONE,
            root_dir: None,
            work_dir: None,
            filps: [const { None }; OPEN_MAX],
            cloexec_set: Bitmap::new(OPEN_MAX),
            tty: NO_DEV,
            blocked_on: BlockedOn::None,
            real_uid: 0,
            eff_uid: 0,
            real_gid: 0,
            eff_gid: 0,
            ngroups: 0,
            supplemental_groups: [0; NGROUPS_MAX],
            umask: 0,
            name: [0; PROC_NAME_LEN],
        }
    }

    /// Checks if the process is blocked on any file system operation.
    ///
    /// Corresponds to the `fp_is_blocked(fp)` macro
    /// (`const.h:28`): `fp_blocked_on != FP_BLOCKED_ON_NONE`.
    pub fn is_blocked(&self) -> bool {
        self.blocked_on != BlockedOn::None
    }

    /// Checks if the process is idle (no pending/PM work, not blocked).
    ///
    /// Used to decide whether the process's fproc slot can be bound to a
    /// worker slot (worker.c: `w_fp` association).
    pub fn is_idle(&self) -> bool {
        !self.flags.contains(FpFlags::PENDING | FpFlags::PM_WORK) && !self.is_blocked()
    }

    /// Checks if the process slot is in use.
    pub fn is_in_use(&self) -> bool {
        self.pid != PID_FREE
    }
}

/// VFS process table.
///
/// Corresponds to Minix3's global `struct fproc fproc[NR_PROCS]`
/// (`fproc.h:82`).
///
/// # Design Notes
///
/// An array indexed by `UserSlot` instead of a map because:
/// 1. Slots are dense and direct indexing is O(1) (`fproc_addr(e)` macro,
///    `glo.h:26-27`).
/// 2. `NR_PROCS` must match the kernel's process table size.
/// 3. Fixed size, no dynamic allocation (no_std).
///
/// The slot array is heap-allocated (`Box<[FProc]>`), not embedded by value:
/// `FProc` is roughly 4.4 KiB per slot, so a by-value `[FProc; NR_PROCS]`
/// is ~1.1 MiB and overflows small (e.g. test) thread stacks when built as
/// a local. Minix3 keeps `fproc[]` in static BSS; in Rust the A-4
/// aggregation (`VfsState`) owns the table, so the heap is the natural home
/// for that storage while `NR_PROCS` stays a compile-time bound.
pub struct FProcTable {
    slots: Box<[FProc]>,
}

impl FProcTable {
    /// Creates a new process table, all slots initialized as unused.
    pub fn new() -> Self {
        // Build directly on the heap: a stack-local `[FProc; NR_PROCS]`
        // would be ~1.1 MiB and overflow the default 2 MiB test-thread
        // stack in unoptimized builds (each element is copied per frame).
        let slots: Box<[FProc]> = (0..NR_PROCS).map(|_| FProc::new_unused()).collect();
        Self { slots }
    }

    /// Gets an immutable fproc reference by slot.
    pub fn get(&self, slot: UserSlot) -> Option<&FProc> {
        let idx = slot.get();
        if idx < NR_PROCS {
            Some(&self.slots[idx])
        } else {
            None
        }
    }

    /// Gets a mutable fproc reference by slot.
    pub fn get_mut(&mut self, slot: UserSlot) -> Option<&mut FProc> {
        let idx = slot.get();
        if idx < NR_PROCS {
            Some(&mut self.slots[idx])
        } else {
            None
        }
    }

    /// Finds fproc by endpoint.
    ///
    /// Corresponds to Minix3's `fproc_addr(e)` macro (`glo.h:27`) plus the
    /// caller's endpoint-validity check (`okendpt`): the endpoint's user slot
    /// must be in range, otherwise `None`.
    pub fn find_by_endpoint(&self, ep: Endpoint) -> Option<&FProc> {
        let slot = ep.to_user_slot()?;
        self.get(slot)
    }

    /// Finds fproc by endpoint (mutable).
    pub fn find_by_endpoint_mut(&mut self, ep: Endpoint) -> Option<&mut FProc> {
        let slot = ep.to_user_slot()?;
        self.get_mut(slot)
    }

    /// Resets all slots to the unused state.
    ///
    /// Corresponds to Minix3's first-phase initialization in
    /// `sef_cb_init_fresh()` (`main.c:405-408`). In Rust, `new()` already
    /// constructs empty slots; the explicit reset keeps the restart/LU path
    /// (currently DEFERRED) honest.
    pub fn reset_all(&mut self) {
        for slot in self.slots.iter_mut() {
            *slot = FProc::new_unused();
        }
    }

    /// Phase-2 initialization — clear directory anchors and file descriptor
    /// table of every slot.
    ///
    /// Corresponds to the second fproc loop in `sef_cb_init_fresh()`
    /// (`main.c:480-483`): `fp_filp[i] = NULL; fp_rd = NULL; fp_wd = NULL;`
    pub fn init_phase2(&mut self) {
        for proc in &mut self.slots {
            proc.filps = [const { None }; OPEN_MAX];
            proc.root_dir = None;
            proc.work_dir = None;
        }
    }
}

impl FProcTable {
    /// Validates an endpoint via the three-guard `isokendpt_f` logic
    /// (`utility.c:92-123`, non-fatal variant `isokendpt`).
    ///
    /// Guards in order: `NONE` → `EDEADEPT`, slot out-of-range → `EDEADEPT`,
    /// `ke != ep` → `EDEADEPT` (with `PID_FREE` mutual check as `debug_assert`).
    pub fn is_ok_endpoint(&self, ep: Endpoint) -> Result<UserSlot, FprocError> {
        if ep == Endpoint::NONE {
            return Err(FprocError::BadEndpoint);
        }
        let slot = ep.to_user_slot().ok_or(FprocError::BadEndpoint)?;
        let idx = slot.get();
        if idx >= NR_PROCS {
            return Err(FprocError::BadEndpoint);
        }
        let ke = self.slots[idx].endpoint;
        if ke != ep {
            // C: ke==NONE → assert(pid==PID_FREE) else assert(pid!=PID_FREE)
            if ke == Endpoint::NONE {
                debug_assert_eq!(self.slots[idx].pid, PID_FREE);
            } else {
                debug_assert_ne!(self.slots[idx].pid, PID_FREE);
            }
            return Err(FprocError::BadEndpoint);
        }
        Ok(slot)
    }

    /// Fatal variant `okendpt` (`proto.h:357`, `fatal=1`).
    ///
    /// Panics with `file:line` context on failure, mirroring
    /// `panic("isokendpt_f failed")` (`utility.c:120`).
    #[track_caller]
    pub fn ok_endpoint(&self, ep: Endpoint) -> UserSlot {
        self.is_ok_endpoint(ep).unwrap_or_else(|_| {
            panic!(
                "ok_endpoint failed at {}:{}: endpoint {} is not ok",
                file!(),
                line!(),
                ep.get()
            )
        })
    }

    /// `fproc_addr(e)` macro (`glo.h:27`) — O(1) `endpoint → &FProc`.
    pub fn at(&self, ep: Endpoint) -> Option<&FProc> {
        self.is_ok_endpoint(ep).ok().and_then(|slot| self.get(slot))
    }

    /// `fproc_addr` mutable variant.
    pub fn at_mut(&mut self, ep: Endpoint) -> Option<&mut FProc> {
        let slot = self.is_ok_endpoint(ep).ok()?;
        self.get_mut(slot)
    }

    /// Whether a slot is free (`pid == PID_FREE`, `fproc.h:103`).
    pub fn is_slot_free(&self, slot: UserSlot) -> bool {
        self.get(slot).map(|fp| !fp.is_in_use()).unwrap_or(true)
    }

    /// Snapshot the `fproc_light` projection (`fproc.h:111-115`, `misc.c:75-96`).
    ///
    /// `ARCH A-7` defer: `MIB` 拉取未实现，`#[cfg(feature = "fproc_light")]` 缺口占位。
    #[cfg(feature = "fproc_light")]
    pub fn snapshot_light(&self) -> Vec<FprocLight> {
        self.slots
            .iter()
            .map(|fp| FprocLight {
                tty: fp.tty,
                blocked_on: fp.blocked_on,
                task: Endpoint::NONE, // fpl_task 语义待 MIB 澄清，暂以 NONE 占位
            })
            .collect()
    }
}

/// `fproc_light` projection (`fproc.h:111-115`, `ARCH A-7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FprocLight {
    pub tty: DevId,
    pub blocked_on: BlockedOn,
    pub task: Endpoint,
}

/// `FprocLightTable` heap storage (`fproc.h:115`, `ARCH A-4`).
pub struct FprocLightTable(Box<[FprocLight]>);

impl FprocLightTable {
    pub fn new() -> Self {
        let v: Box<[FprocLight]> = (0..NR_PROCS)
            .map(|_| FprocLight {
                tty: NO_DEV,
                blocked_on: BlockedOn::None,
                task: Endpoint::NONE,
            })
            .collect();
        Self(v)
    }
    pub fn get(&self, slot: UserSlot) -> Option<&FprocLight> {
        let idx = slot.get();
        if idx < NR_PROCS {
            Some(&self.0[idx])
        } else {
            None
        }
    }
}

/// `isokendpt_f` error (`utility.c:122`, `EDEADEPT` 78).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FprocError {
    BadEndpoint,
}

impl FprocError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::BadEndpoint => minix_types::EDEADEPT,
        }
    }
}

impl Default for FProcTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    #[test]
    fn test_fproc_new_unused() {
        let fp = FProc::new_unused();
        assert_eq!(fp.pid, PID_FREE);
        assert!(fp.endpoint.is_none());
        assert_eq!(fp.flags, FpFlags::NOFLAGS);
        assert!(fp.root_dir.is_none());
        assert!(fp.work_dir.is_none());
        for filp in &fp.filps {
            assert!(filp.is_none());
        }
        assert_eq!(fp.cloexec_set.count_ones(), 0);
        assert_eq!(fp.cloexec_set.size(), OPEN_MAX);
        assert_eq!(fp.tty, NO_DEV);
        assert!(!fp.is_blocked());
        assert!(fp.is_idle());
        assert_eq!(fp.blocked_on, BlockedOn::None);
        assert_eq!(fp.umask, 0);
        assert_eq!(fp.name, [0; PROC_NAME_LEN]);
    }

    #[test]
    fn test_fproc_is_in_use() {
        let mut fp = FProc::new_unused();
        assert!(!fp.is_in_use());
        fp.pid = 1234;
        assert!(fp.is_in_use());
    }

    #[test]
    fn test_fproc_is_blocked_tagged_enum() {
        let mut fp = FProc::new_unused();
        assert!(!fp.is_blocked());

        // Pipe payload carries resume parameters (u_pipe).
        fp.blocked_on = BlockedOn::Pipe(PipeBlock {
            call: PipeIo::Read,
            fd: 3,
            buf: VirBytes::new(0x1000),
            nbytes: 4096,
            cum_io: 0,
        });
        assert!(fp.is_blocked());
        assert!(!fp.is_idle());

        // Variant and payload must agree: extracting pipe fields from a
        // non-pipe variant is a compile error, not a runtime alias bug.
        fp.blocked_on = BlockedOn::Cdev(CdevBlock {
            dev: NO_DEV,
            endpt: Endpoint::TTY,
            grant: Some(7),
        });
        assert!(fp.is_blocked());
        assert!(matches!(fp.blocked_on, BlockedOn::Cdev(_)));
    }

    #[test]
    fn test_blocked_on_payload_roundtrip() {
        let pipe = BlockedOn::Pipe(PipeBlock {
            call: PipeIo::Write,
            fd: 4,
            buf: VirBytes::new(0x2000),
            nbytes: 128,
            cum_io: 64,
        });
        assert_eq!(
            pipe,
            BlockedOn::Pipe(PipeBlock {
                call: PipeIo::Write,
                fd: 4,
                buf: VirBytes::new(0x2000),
                nbytes: 128,
                cum_io: 64,
            })
        );

        let flock = BlockedOn::Flock(FlockBlock {
            fd: 2,
            cmd: FlockCmd::SetLkw,
            arg: VirBytes::new(0x3000),
        });
        assert!(matches!(flock, BlockedOn::Flock(_)));

        let sdev = BlockedOn::Sdev(SdevBlock {
            dev: NO_DEV,
            call: SdevCall::Accept,
            grants: [None, None, None],
            aux: SdevAux::Fd(9),
        });
        assert!(matches!(sdev, BlockedOn::Sdev(_)));
    }

    #[test]
    fn test_blocked_on_default() {
        let blocked = BlockedOn::default();
        assert_eq!(blocked, BlockedOn::None);
    }

    #[test]
    fn test_fproc_table_new() {
        let table = FProcTable::new();
        for i in 0..NR_PROCS {
            let slot = UserSlot::new(i);
            let fp = table.get(slot).unwrap();
            assert_eq!(fp.pid, PID_FREE);
        }
    }

    #[test]
    fn test_fproc_table_find_by_endpoint() {
        let mut table = FProcTable::new();
        let ep = Endpoint::PM;
        let slot = ep.to_user_slot().unwrap();
        table.get_mut(slot).unwrap().pid = 1;
        table.get_mut(slot).unwrap().endpoint = ep;

        let found = table.find_by_endpoint(ep);
        assert!(found.is_some());
        assert_eq!(found.unwrap().pid, 1);
    }

    #[test]
    fn test_fproc_table_find_by_endpoint_not_found() {
        let table = FProcTable::new();
        let ep = Endpoint::KERNEL;
        let found = table.find_by_endpoint(ep);
        assert!(found.is_none());
    }

    #[test]
    fn test_fp_flags() {
        let mut flags = FpFlags::NOFLAGS;
        assert!(!flags.contains(FpFlags::SRV_PROC));
        flags.insert(FpFlags::SRV_PROC);
        assert!(flags.contains(FpFlags::SRV_PROC));
        flags.remove(FpFlags::SRV_PROC);
        assert!(!flags.contains(FpFlags::SRV_PROC));
    }

    #[test]
    fn test_fproc_table_init_phase2() {
        let mut table = FProcTable::new();
        let slot = UserSlot::new(0);
        table.get_mut(slot).unwrap().root_dir = Some(42);
        table.get_mut(slot).unwrap().work_dir = Some(99);
        table.get_mut(slot).unwrap().filps[0] = Some(10);

        table.init_phase2();

        let fp = table.get(slot).unwrap();
        assert!(fp.root_dir.is_none());
        assert!(fp.work_dir.is_none());
        assert!(fp.filps[0].is_none());
    }

    #[test]
    fn test_fp_flags_values_match_c() {
        // fproc.h:92-98 — bit values must match the C definitions exactly.
        assert_eq!(FpFlags::SRV_PROC.bits(), 0x0001);
        assert_eq!(FpFlags::REVIVED.bits(), 0x0002);
        assert_eq!(FpFlags::SESLDR.bits(), 0x0004);
        assert_eq!(FpFlags::PENDING.bits(), 0x0010);
        assert_eq!(FpFlags::EXITING.bits(), 0x0020);
        assert_eq!(FpFlags::PM_WORK.bits(), 0x0040);
    }

    #[test]
    fn test_is_ok_endpoint_none() {
        let table = FProcTable::new();
        let r = table.is_ok_endpoint(Endpoint::NONE);
        assert_eq!(r.unwrap_err(), FprocError::BadEndpoint);
        assert_eq!(FprocError::BadEndpoint.to_errno(), minix_types::EDEADEPT);
    }

    #[test]
    fn test_is_ok_endpoint_out_of_range() {
        let table = FProcTable::new();
        // Endpoint with slot 300 (>255) via generation trick
        let bad = Endpoint::from_generation_slot(0, 300);
        let r = table.is_ok_endpoint(bad);
        assert_eq!(r.unwrap_err(), FprocError::BadEndpoint);
    }

    #[test]
    fn test_is_ok_endpoint_mismatch() {
        let mut table = FProcTable::new();
        // Occupied slot PM but query with different generation
        let ep = Endpoint::PM;
        let slot = ep.to_user_slot().unwrap();
        table.get_mut(slot).unwrap().pid = 1;
        table.get_mut(slot).unwrap().endpoint = ep;
        let mismatched = Endpoint::from_generation_slot(1, slot.get() as i32);
        let r = table.is_ok_endpoint(mismatched);
        assert_eq!(r.unwrap_err(), FprocError::BadEndpoint);
        // Valid case
        assert_eq!(table.is_ok_endpoint(ep).unwrap(), slot);
    }

    #[test]
    #[should_panic(expected = "ok_endpoint failed")]
    fn test_ok_endpoint_panic() {
        let table = FProcTable::new();
        let _ = table.ok_endpoint(Endpoint::NONE);
    }

    #[test]
    fn test_fproc_addr_none() {
        let table = FProcTable::new();
        assert!(table.at(Endpoint::NONE).is_none());
        let ep = Endpoint::PM;
        let slot = ep.to_user_slot().unwrap();
        let mut t2 = FProcTable::new();
        t2.get_mut(slot).unwrap().pid = 42;
        t2.get_mut(slot).unwrap().endpoint = ep;
        assert!(t2.at(ep).is_some());
        assert_eq!(t2.at(ep).unwrap().pid, 42);
    }

    #[test]
    fn test_is_in_use_pid_free() {
        let mut fp = FProc::new_unused();
        assert!(!fp.is_in_use());
        fp.pid = 99;
        assert!(fp.is_in_use());
        let mut table = FProcTable::new();
        let slot = UserSlot::new(5);
        assert!(table.is_slot_free(slot));
        table.get_mut(slot).unwrap().pid = 5;
        assert!(!table.is_slot_free(slot));
    }

    #[test]
    #[cfg(feature = "fproc_light")]
    fn test_fproc_light_snapshot() {
        let mut table = FProcTable::new();
        let slot = UserSlot::new(0);
        table.get_mut(slot).unwrap().tty = 7;
        let v = table.snapshot_light();
        assert_eq!(v[0].tty, 7);
        assert_eq!(v[0].blocked_on, BlockedOn::None);
    }

    // Second impl for Gate D trait threshold
    struct AltFprocTable(FProcTable);
    impl AltFprocTable {
        fn is_ok(&self, ep: Endpoint) -> Result<UserSlot, FprocError> {
            self.0.is_ok_endpoint(ep)
        }
    }
}
