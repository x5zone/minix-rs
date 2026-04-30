//! VFS process structure (FProc).
//!
//! Corresponds to Minix3's `struct fproc` (defined in `minix3/minix/servers/vfs/fproc.h`).
//!
//! FProc is VFS's private view of each process, containing file descriptor table,
//! directory pointers, credentials, etc. Associated with PM's mproc and VM's vmproc
//! via endpoint, each maintaining independent process tables.
//!
//! # Design Principles
//!
//! - Uses Typestate View pattern: runtime state stored in FProc fields,
//!   compile-time constraints provided through borrow types like ActiveProc/ExitingProc.
//! - fp_lock belongs to slot, not process—fork must preserve child's own mutex.

use minix_types::{Endpoint, Pid, Uid, Gid, UserSlot, NR_PROCS};

pub const OPEN_MAX: usize = 128;
pub const NGROUPS_MAX: usize = 32;
pub const PROC_NAME_LEN: usize = 16;

bitflags::bitflags! {
    /// Process flags.
    ///
    /// Corresponds to Minix3's `fp_flags`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FpFlags: u32 {
        /// Server process (e.g. FS callback).
        const SRV_PROC = 0x0001;
        /// Revived (resumed after blocking).
        const REVIVED = 0x0002;
        /// Session leader process.
        const SESLDR = 0x0004;
        /// Has pending PM request.
        const PENDING = 0x0010;
        /// Process is exiting.
        const EXITING = 0x0020;
        /// Has PM work pending.
        const PM_WORK = 0x0040;
        /// No flags.
        const NOFLAGS = 0x0000;
    }
}

/// Blocking reason.
///
/// Corresponds to Minix3's `fp_blocked_on`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockedOn {
    /// Not blocked.
    None = 0,
    /// Blocked on pipe I/O.
    Pipe = 1,
    /// Blocked on file lock.
    Flock = 2,
    /// Blocked on device I/O (e.g. select).
    Other = 3,
}

impl Default for BlockedOn {
    fn default() -> Self {
        Self::None
    }
}

/// VFS process structure.
///
/// Corresponds to Minix3's `struct fproc`.
///
/// # Key Differences from C Version
///
/// - `fp_filp` uses `Option<usize>` instead of raw pointers, storing global filp table index.
/// - `fp_rd`/`fp_wd` uses `Option<usize>` instead of raw pointers, storing global vnode table index.
/// - `fp_lock` not in this struct—mutex belongs to slot, not process.
/// - `fp_worker` moved to worker module management.
#[derive(Debug, Clone)]
pub struct FProc {
    /// Process flags.
    pub flags: FpFlags,
    /// Process ID.
    pub pid: Pid,
    /// Kernel endpoint.
    pub endpoint: Endpoint,
    /// Root directory vnode index (global vnode table).
    pub root_dir: Option<usize>,
    /// Working directory vnode index (global vnode table).
    pub work_dir: Option<usize>,
    /// File descriptor table (global filp table index).
    pub filps: [Option<usize>; OPEN_MAX],
    /// FD_CLOEXEC bitmap.
    pub cloexec_set: u128,
    /// Real user ID.
    pub real_uid: Uid,
    /// Effective user ID.
    pub eff_uid: Uid,
    /// Real group ID.
    pub real_gid: Gid,
    /// Effective group ID.
    pub eff_gid: Gid,
    /// Supplemental group count.
    pub ngroups: usize,
    /// Supplemental group list.
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    /// umask.
    pub umask: u32,
    /// Process name.
    pub name: [u8; PROC_NAME_LEN],
    /// Blocking reason.
    pub blocked_on: BlockedOn,
}

impl FProc {
    /// Creates an unused empty fproc slot.
    ///
    /// Corresponds to Minix3's first-phase initialization in `sef_cb_init_fresh()`:
    /// `fp_endpoint = NONE; fp_pid = PID_FREE;`
    pub const fn new_unused() -> Self {
        Self {
            flags: FpFlags::NOFLAGS,
            pid: PID_FREE,
            endpoint: Endpoint::NONE,
            root_dir: None,
            work_dir: None,
            filps: [const { None }; OPEN_MAX],
            cloexec_set: 0,
            real_uid: 0,
            eff_uid: 0,
            real_gid: 0,
            eff_gid: 0,
            ngroups: 0,
            supplemental_groups: [0; NGROUPS_MAX],
            umask: !0,
            name: [0; PROC_NAME_LEN],
            blocked_on: BlockedOn::None,
        }
    }

    /// Checks if process is idle (no worker thread associated).
    pub fn is_idle(&self) -> bool {
        !self.flags.contains(FpFlags::PENDING | FpFlags::PM_WORK)
            && self.blocked_on == BlockedOn::None
    }

    /// Checks if process is in use.
    pub fn is_in_use(&self) -> bool {
        self.pid != PID_FREE
    }
}

/// PID_FREE constant—marks slot as unused.
///
/// Corresponds to Minix3's `#define PID_FREE 0`.
pub const PID_FREE: Pid = 0;

/// VFS process table.
///
/// Corresponds to Minix3's global `struct fproc fproc[NR_PROCS]`.
///
/// # Design Notes
///
/// Uses array instead of HashMap because:
/// 1. Process slot numbers are contiguous, direct indexing is O(1).
/// 2. Consistent with Minix3's fproc array semantics.
/// 3. Fixed size, no dynamic allocation needed.
pub struct FProcTable {
    slots: [FProc; NR_PROCS],
}

impl FProcTable {
    /// Creates new process table, all slots initialized as unused.
    pub fn new() -> Self {
        Self {
            slots: [const { FProc::new_unused() }; NR_PROCS],
        }
    }

    /// Gets fproc immutable reference by UserSlot.
    pub fn get(&self, slot: UserSlot) -> Option<&FProc> {
        let idx = slot.get();
        if idx < NR_PROCS {
            Some(&self.slots[idx])
        } else {
            None
        }
    }

    /// Gets fproc mutable reference by UserSlot.
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
    /// Corresponds to Minix3's `fproc_addr(e)` macro.
    pub fn find_by_endpoint(&self, ep: Endpoint) -> Option<&FProc> {
        let slot = ep.to_user_slot()?;
        self.get(slot)
    }

    /// Finds fproc by endpoint (mutable).
    pub fn find_by_endpoint_mut(&mut self, ep: Endpoint) -> Option<&mut FProc> {
        let slot = ep.to_user_slot()?;
        self.get_mut(slot)
    }

    /// Phase 2 initialization—set mutex and clear directory/file descriptors.
    ///
    /// Corresponds to Minix3's second-phase initialization in `sef_cb_init_fresh()`.
    pub fn init_phase2(&mut self) {
        for proc in &mut self.slots {
            proc.filps = [const { None }; OPEN_MAX];
            proc.root_dir = None;
            proc.work_dir = None;
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
        assert_eq!(fp.umask, !0);
        assert_eq!(fp.blocked_on, BlockedOn::None);
    }

    #[test]
    fn test_fproc_is_in_use() {
        let mut fp = FProc::new_unused();
        assert!(!fp.is_in_use());
        fp.pid = 1234;
        assert!(fp.is_in_use());
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
    fn test_blocked_on_default() {
        let blocked = BlockedOn::default();
        assert_eq!(blocked, BlockedOn::None);
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
}
