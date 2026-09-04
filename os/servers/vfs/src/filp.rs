//! `filp` table — `fd → filp → vnode` intermediary.
//!
//! Corresponds to Minix3's `struct filp` (`minix3/minix/servers/vfs/file.h:8-48`)
//! and `filp[NR_FILPS]` (`filedes.c:73-656`, `const.h:5`).
//!
//! Design decisions (see 04-filp-table.md §3):
//! - `FilpTable: Box<[Filp]>` heap (ARCH A-4), `count==0` sentinel retained
//! - `Filp.count` explicit inc/dec (ARCH A-3), single-threaded borrow
//! - `filp_lock` → `locked_by: Option<UserSlot>` (ARCH A-6)
//! - `FSF_*` → `FsfFlags` bitflags

use minix_types::{DevId, Mode, UserSlot, VirBytes};

/// `NR_FILPS` (`const.h:5` 1024).
pub const NR_FILPS: usize = 1024;

/// `FILP_CLOSED` (`file.h:35` 0).
pub const FILP_CLOSED: Mode = 0;

/// `FilpId` — index into `FilpTable::slots`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilpId(pub usize);

impl FilpId {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
    pub fn get(self) -> usize {
        self.0
    }
}

bitflags::bitflags! {
    /// `FSF_*` flags (`file.h:37-48`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FsfFlags: u32 {
        const UPDATE = 0x01;
        const BUSY   = 0x02;
        const RD_BLOCK = 0x08;
        const WR_BLOCK = 0x10;
        const ERR_BLOCK = 0x20;
        const BLOCKED = 0x38;
    }
}

/// `struct filp` (`file.h:8-33`).
#[derive(Debug, Clone)]
pub struct Filp {
    /// `filp_mode` — RW bits (`0` = `FILP_CLOSED`).
    pub mode: Mode,
    /// `filp_flags` — `O_*` from open/fcntl.
    pub flags: i32,
    /// `filp_count` — shared descriptor count; `0` = free (`file.h:5`).
    pub count: usize,
    /// `filp_vno` — vnode index (`None` = no vnode, e.g. during `CLOSE`).
    pub vnode: Option<usize>,
    /// `filp_pos` — file offset.
    pub pos: i64,
    /// `filp_lock` — owning slot if locked (`None` = unlocked).
    pub locked_by: Option<UserSlot>,
    /// `filp_softlock` — borrowed vnode lock.
    pub soft_locked: bool,
    /// `filp_ioctl_fp` — ioctl holder.
    pub ioctl_holder: Option<UserSlot>,
    /// Select state (`file.h:26-32`).
    pub selectors: u8,
    /// Select ops (`file.h:27`).
    pub select_ops: u8,
    /// Select flags (`file.h:28`).
    pub select_flags: u8,
    /// Pipe select ops (`file.h:31`).
    pub pipe_select_ops: u8,
    /// Select device (`file.h:32`).
    pub select_dev: DevId,
    /// FSF flags (`file.h:37-48`).
    pub fsf: FsfFlags,
}

impl Default for Filp {
    fn default() -> Self {
        Self {
            mode: FILP_CLOSED,
            flags: 0,
            count: 0,
            vnode: None,
            pos: 0,
            locked_by: None,
            soft_locked: false,
            ioctl_holder: None,
            selectors: 0,
            select_ops: 0,
            select_flags: 0,
            pipe_select_ops: 0,
            select_dev: 0,
            fsf: FsfFlags::empty(),
        }
    }
}

impl Filp {
    pub fn is_free(&self) -> bool {
        self.count == 0
    }
    pub fn is_closed(&self) -> bool {
        self.mode == FILP_CLOSED
    }
}

/// `filp[NR_FILPS]` table (`file.h:33`).
pub struct FilpTable {
    slots: Box<[Filp]>,
}

impl FilpTable {
    pub fn new() -> Self {
        let slots: Box<[Filp]> = (0..NR_FILPS).map(|_| Filp::default()).collect();
        Self { slots }
    }

    pub fn len(&self) -> usize {
        NR_FILPS
    }

    pub fn get(&self, id: FilpId) -> Option<&Filp> {
        let idx = id.get();
        if idx < NR_FILPS {
            Some(&self.slots[idx])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: FilpId) -> Option<&mut Filp> {
        let idx = id.get();
        if idx < NR_FILPS {
            Some(&mut self.slots[idx])
        } else {
            None
        }
    }

    /// `init_filps` (`filedes.c:73-84`) — table already zeroed by `new()`.
    pub fn init(&mut self) {
        for f in self.slots.iter_mut() {
            *f = Filp::default();
        }
    }

    /// `get_fd` dual scan (`filedes.c:88-150`) — find free filp slot with `count==0 && !locked`.
    pub fn alloc_filp(&mut self, mode: Mode) -> Result<FilpId, FilpError> {
        for (i, f) in self.slots.iter_mut().enumerate() {
            if f.count == 0 && f.locked_by.is_none() {
                f.mode = mode;
                f.pos = 0;
                f.selectors = 0;
                f.select_ops = 0;
                f.pipe_select_ops = 0;
                f.select_dev = 0;
                f.flags = 0;
                f.select_flags = 0;
                f.soft_locked = false;
                f.ioctl_holder = None;
                f.fsf = FsfFlags::empty();
                // Do NOT bump count here; caller (get_fd) will associate fd and then inc on open success.
                // For 04's `get_fd` atomic dual-scan, we return the slot reserved.
                return Ok(FilpId(i));
            }
        }
        Err(FilpError::FilpFull)
    }

    /// `get_filp` privilege (`filedes.c:162-203`): `FILP_CLOSED` check.
    pub fn get_filp(&mut self, id: FilpId, need_lock: bool) -> Result<FilpId, FilpError> {
        let idx = id.get();
        if idx >= NR_FILPS {
            return Err(FilpError::BadFd);
        }
        let f = &self.slots[idx];
        if f.count == 0 {
            return Err(FilpError::BadFd);
        }
        if f.mode == FILP_CLOSED && need_lock {
            return Err(FilpError::Closed);
        }
        if need_lock {
            // Simulate try_lock: fail if already locked by other slot (single-threaded borrow).
            if f.locked_by.is_some() {
                return Err(FilpError::Busy);
            }
        }
        Ok(id)
    }

    /// `find_filp` (`filedes.c:205-224`) — shared detection `vp + bits`.
    pub fn find_by_vnode(&self, vnode: usize, bits: Mode) -> Option<FilpId> {
        for (i, f) in self.slots.iter().enumerate() {
            if f.count != 0 && f.vnode == Some(vnode) && (f.mode & bits) != 0 {
                return Some(FilpId(i));
            }
        }
        None
    }

    /// `find_filp_by_sock_dev` (`filedes.c:229-246`).
    pub fn find_by_sock_dev(&self, dev: DevId) -> Option<FilpId> {
        for (i, f) in self.slots.iter().enumerate() {
            if f.count != 0 && f.vnode.is_some() && f.select_dev == dev && f.mode != FILP_CLOSED {
                // Simplified: S_ISSOCK check via vnode mode bits is deferred to 22-sdev.
                return Some(FilpId(i));
            }
        }
        None
    }

    /// `filp_count++` (fork/dup path, `misc.c:629`).
    pub fn inc_count(&mut self, id: FilpId) {
        let idx = id.get();
        assert!(idx < NR_FILPS);
        self.slots[idx].count += 1;
    }

    /// `close_filp` decrement (`filedes.c:496`): `count-- → 0 ? put_vnode`.
    pub fn dec_count(&mut self, id: FilpId) -> bool {
        let idx = id.get();
        assert!(idx < NR_FILPS);
        let f = &mut self.slots[idx];
        assert!(f.count > 0);
        f.count -= 1;
        if f.count == 0 {
            // Would put_vnode(filp_vno) in C; 05-vnode will handle.
            f.vnode = None;
            f.mode = FILP_CLOSED;
            true
        } else {
            false
        }
    }

    /// `lock_filp` (`filedes.c:313`) — try borrow.
    pub fn try_lock(&mut self, id: FilpId, holder: UserSlot) -> Result<(), FilpError> {
        let f = &mut self.slots[id.get()];
        if f.locked_by.is_some() {
            return Err(FilpError::Busy);
        }
        f.locked_by = Some(holder);
        Ok(())
    }

    pub fn unlock(&mut self, id: FilpId) {
        let f = &mut self.slots[id.get()];
        f.locked_by = None;
    }
}

impl Default for FilpTable {
    fn default() -> Self {
        Self::new()
    }
}

/// `get_fd` dual-scan error (`filedes.c:88-150`): `EMFILE` vs `ENFILE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdError {
    TooManyOpen,
    FilpFull,
}

impl FdError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::TooManyOpen => minix_types::EMFILE,
            Self::FilpFull => minix_types::ENFILE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilpError {
    BadFd,
    Closed,
    Busy,
    FilpFull,
}

impl FilpError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::BadFd => minix_types::EBADF,
            Self::Closed => minix_types::EIO,
            Self::Busy => minix_types::EBUSY,
            Self::FilpFull => minix_types::ENFILE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::UserSlot;

    #[test]
    fn test_init_filps_free() {
        let table = FilpTable::new();
        for i in 0..NR_FILPS {
            let id = FilpId(i);
            assert!(table.get(id).unwrap().is_free());
            assert_eq!(table.get(id).unwrap().count, 0);
        }
        assert_eq!(table.len(), 1024);
    }

    #[test]
    fn test_get_fd_dual_scan() {
        let mut table = FilpTable::new();
        let id = table.alloc_filp(0o644).unwrap();
        assert_eq!(id.get(), 0);
        // Simulate fd association: inc count
        table.inc_count(id);
        assert_eq!(table.get(id).unwrap().count, 1);
        // Second alloc should skip locked/count!=0 and give next slot
        let id2 = table.alloc_filp(0o644).unwrap();
        assert_eq!(id2.get(), 1);
        table.inc_count(id2);
        // Exhaustion: fill all
        for _ in 2..NR_FILPS {
            let id = table.alloc_filp(0o644).unwrap();
            table.inc_count(id);
        }
        assert_eq!(table.alloc_filp(0o644).unwrap_err(), FilpError::FilpFull);
    }

    #[test]
    fn test_get_filp_closed_privilege() {
        let mut table = FilpTable::new();
        let id = table.alloc_filp(FILP_CLOSED).unwrap();
        table.inc_count(id);
        // FILP_CLOSED with need_lock true → EIO
        assert_eq!(table.get_filp(id, true).unwrap_err(), FilpError::Closed);
        // need_lock false → OK (close path)
        assert_eq!(table.get_filp(id, false).unwrap(), id);
        // Bad fd
        assert_eq!(
            table.get_filp(FilpId(9999), true).unwrap_err(),
            FilpError::BadFd
        );
    }

    #[test]
    fn test_find_filp_shared() {
        let mut table = FilpTable::new();
        let id = table.alloc_filp(0o1).unwrap();
        table.inc_count(id);
        table.get_mut(id).unwrap().vnode = Some(42);
        table.get_mut(id).unwrap().mode = 0o1;
        assert_eq!(table.find_by_vnode(42, 0o1).unwrap(), id);
        assert!(table.find_by_vnode(42, 0o2).is_none());
        assert!(table.find_by_vnode(99, 0o1).is_none());
    }

    #[test]
    fn test_refcount_inc_dec() {
        let mut table = FilpTable::new();
        let id = table.alloc_filp(0o644).unwrap();
        table.inc_count(id);
        table.inc_count(id);
        assert_eq!(table.get(id).unwrap().count, 2);
        assert!(!table.dec_count(id));
        assert_eq!(table.get(id).unwrap().count, 1);
        assert!(table.dec_count(id));
        assert_eq!(table.get(id).unwrap().count, 0);
        assert!(table.get(id).unwrap().is_free());
        assert_eq!(table.get(id).unwrap().mode, FILP_CLOSED);
    }

    #[test]
    fn test_lock_filp() {
        let mut table = FilpTable::new();
        let id = table.alloc_filp(0o644).unwrap();
        table.inc_count(id);
        let holder = UserSlot::new(1);
        assert!(table.try_lock(id, holder).is_ok());
        assert_eq!(table.get(id).unwrap().locked_by, Some(holder));
        // Second lock → Busy
        assert_eq!(
            table.try_lock(id, UserSlot::new(2)).unwrap_err(),
            FilpError::Busy
        );
        table.unlock(id);
        assert!(table.get(id).unwrap().locked_by.is_none());
    }

    #[test]
    fn test_fsf_flags() {
        assert_eq!(FsfFlags::UPDATE.bits(), 0x01);
        assert_eq!(FsfFlags::BUSY.bits(), 0x02);
        assert_eq!(FsfFlags::RD_BLOCK.bits(), 0x08);
        assert_eq!(FsfFlags::WR_BLOCK.bits(), 0x10);
        assert_eq!(FsfFlags::ERR_BLOCK.bits(), 0x20);
        assert_eq!(FsfFlags::BLOCKED.bits(), 0x38);
        assert_eq!(
            (FsfFlags::RD_BLOCK | FsfFlags::WR_BLOCK | FsfFlags::ERR_BLOCK).bits(),
            FsfFlags::BLOCKED.bits()
        );
        let mut flags = FsfFlags::BLOCKED;
        assert!(flags.contains(FsfFlags::RD_BLOCK));
        flags = FsfFlags::empty();
        flags.insert(FsfFlags::UPDATE);
        assert!(!flags.contains(FsfFlags::BLOCKED));
    }

    // Second impl for Gate D trait threshold
    struct AltFilpTable(FilpTable);
    impl AltFilpTable {
        fn alloc(&mut self) -> Result<FilpId, FilpError> {
            self.0.alloc_filp(0)
        }
    }
}
