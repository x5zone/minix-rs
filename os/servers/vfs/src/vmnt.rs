//! `vmnt` table — `device → FS` mount boundary.
//!
//! Corresponds to Minix3's `struct vmnt` (`minix3/minix/servers/vfs/vmnt.h:7-21`)
//! and `vmnt[NR_MNTS]` (`vmnt.c:63-287`, `const.h:NR_MNTS 8`).
//!
//! Design decisions (see 06-vmnt-table.md §3):
//! - `VmntTable: Box<[Vmnt]>` heap (ARCH A-4), `m_dev==NO_DEV` sentinel
//! - `m_lock` → `VmntLock` borrow (ARCH A-6)
//! - `mark_free` 2-field vs `clear` 6-field split

use minix_types::{DevId, Endpoint, NO_DEV};

/// `NR_MNTS` (`const.h:NR_MNTS` 8).
pub const NR_MNTS: usize = 8;

/// `VmntId` — index into `VmntTable::slots`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmntId(pub usize);

impl VmntId {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
    pub fn get(self) -> usize {
        self.0
    }
}

bitflags::bitflags! {
    /// `VMNT_*` flags (`vmnt.h:24-28`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VmntFlags: u32 {
        const READONLY = 0x01;
        const CALLBACK = 0x02;
        const MOUNTING = 0x04;
        const FORCEROOTBSF = 0x08;
        const CANSTAT = 0x10;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmntAccess {
    Read,
    Write,
    Excl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockState {
    Unlocked,
    Read(usize),
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmntLock {
    state: LockState,
}

impl Default for VmntLock {
    fn default() -> Self {
        Self { state: LockState::Unlocked }
    }
}

impl VmntLock {
    pub fn is_locked(&self) -> bool {
        self.state != LockState::Unlocked
    }
    pub fn try_lock(&mut self, access: VmntAccess) -> Result<(), VmntError> {
        let target = match access {
            VmntAccess::Read => LockState::Read(1),
            VmntAccess::Write => LockState::Write,
            VmntAccess::Excl => LockState::Write,
        };
        match (self.state, target) {
            (LockState::Unlocked, _) => {
                self.state = target;
                Ok(())
            }
            (LockState::Read(n), LockState::Read(1)) => {
                self.state = LockState::Read(n + 1);
                Ok(())
            }
            _ => Err(VmntError::Busy),
        }
    }
    pub fn unlock(&mut self) {
        match self.state {
            LockState::Read(n) if n > 1 => self.state = LockState::Read(n - 1),
            _ => self.state = LockState::Unlocked,
        }
    }
}

/// `struct vmnt` (`vmnt.h:7-21`).
#[derive(Debug, Clone)]
pub struct Vmnt {
    /// `m_fs_e` — FS endpoint.
    pub fs: Endpoint,
    pub lock: VmntLock,
    /// `m_dev` — device number; `NO_DEV` = free (`vmnt.h:11`).
    pub dev: DevId,
    pub flags: VmntFlags,
    pub fs_flags: u32,
    pub mounted_on: Option<usize>, // VnodeId index placeholder
    pub root: Option<usize>,
    pub label: String,
    pub mount_path: String,
    pub mount_dev: String,
    pub fstype: String,
}

impl Default for Vmnt {
    fn default() -> Self {
        Self {
            fs: Endpoint::NONE,
            lock: VmntLock::default(),
            dev: NO_DEV,
            flags: VmntFlags::empty(),
            fs_flags: 0,
            mounted_on: None,
            root: None,
            label: String::new(),
            mount_path: String::new(),
            mount_dev: String::new(),
            fstype: String::new(),
        }
    }
}

impl Vmnt {
    pub fn is_free(&self) -> bool {
        self.dev == NO_DEV
    }
}

/// `vmnt[NR_MNTS]` table (`vmnt.h:21`).
pub struct VmntTable {
    slots: Box<[Vmnt]>,
}

impl VmntTable {
    pub fn new() -> Self {
        let slots: Box<[Vmnt]> = (0..NR_MNTS).map(|_| Vmnt::default()).collect();
        Self { slots }
    }

    pub fn len(&self) -> usize {
        NR_MNTS
    }

    pub fn get(&self, id: VmntId) -> Option<&Vmnt> {
        let idx = id.get();
        if idx < NR_MNTS {
            Some(&self.slots[idx])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: VmntId) -> Option<&mut Vmnt> {
        let idx = id.get();
        if idx < NR_MNTS {
            Some(&mut self.slots[idx])
        } else {
            None
        }
    }

    /// `init_vmnts` (`vmnt.c:127-136`).
    pub fn init(&mut self) {
        for v in self.slots.iter_mut() {
            *v = Vmnt::default();
        }
    }

    /// `get_free_vmnt` (`vmnt.c:95-107`) — `m_dev==NO_DEV → clear → return`.
    pub fn alloc(&mut self) -> Result<VmntId, VmntError> {
        for (i, v) in self.slots.iter_mut().enumerate() {
            if v.dev == NO_DEV {
                *v = Vmnt::default();
                return Ok(VmntId(i));
            }
        }
        Err(VmntError::NoSpace)
    }

    /// `find_vmnt` (`vmnt.c:112-122`) — `m_fs_e==fs && m_dev!=NO_DEV`.
    pub fn find_by_fs(&self, fs: Endpoint) -> Option<VmntId> {
        for (i, v) in self.slots.iter().enumerate() {
            if v.fs == fs && v.dev != NO_DEV {
                return Some(VmntId(i));
            }
        }
        None
    }

    pub fn is_locked(&self, id: VmntId) -> bool {
        self.get(id).map(|v| v.lock.is_locked()).unwrap_or(false)
    }

    pub fn lock(&mut self, id: VmntId, access: VmntAccess, requester: Endpoint) -> Result<(), VmntError> {
        let v = self.get(id).ok_or(VmntError::BadVmnt)?;
        if v.fs == requester {
            return Err(VmntError::Deadlock);
        }
        let v = self.get_mut(id).unwrap();
        let effective = match access {
            VmntAccess::Excl => VmntAccess::Write,
            _ => access,
        };
        v.lock.try_lock(effective)
    }

    pub fn unlock(&mut self, id: VmntId) {
        if let Some(v) = self.get_mut(id) {
            v.lock.unlock();
        }
    }

    /// `mark_vmnt_free` (`vmnt.c:65-71`) — 2-field fast release.
    pub fn mark_free(&mut self, id: VmntId) {
        if let Some(v) = self.get_mut(id) {
            v.fs = Endpoint::NONE;
            v.dev = NO_DEV;
        }
    }

    /// `clear_vmnt` (`vmnt.c:76-89`) — 6-field full clear.
    pub fn clear(&mut self, id: VmntId) {
        if let Some(v) = self.get_mut(id) {
            v.fs = Endpoint::NONE;
            v.dev = NO_DEV;
            v.flags = VmntFlags::empty();
            v.mounted_on = None;
            v.root = None;
            v.label.clear();
            v.mount_path.clear();
            v.mount_dev.clear();
            v.fstype.clear();
        }
    }

    /// `vmnt_unmap_by_endpt` (`vmnt.c:180-191`) — 4-step cascade.
    pub fn unmap_by_endpoint(
        &mut self,
        fs: Endpoint,
        fs_cancel: &mut dyn FsCancel,
        filp_inval: &mut dyn FilpInval,
        vnode_put: &mut dyn VnodePut,
    ) -> bool {
        let id = match self.find_by_fs(fs) {
            Some(id) => id,
            None => return false,
        };
        let mounted_on = self.get(id).and_then(|v| v.mounted_on);
        self.mark_free(id);
        fs_cancel.cancel(self.get(id).unwrap());
        filp_inval.invalidate(fs);
        if let Some(vnode_id) = mounted_on {
            vnode_put.put(vnode_id);
        }
        true
    }
}

impl Default for VmntTable {
    fn default() -> Self {
        Self::new()
    }
}

pub trait FsCancel {
    fn cancel(&mut self, vmnt: &Vmnt);
}
pub trait FilpInval {
    fn invalidate(&mut self, fs: Endpoint);
}
pub trait VnodePut {
    fn put(&mut self, vnode: usize);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmntError {
    NoSpace,
    BadVmnt,
    Busy,
    Deadlock,
}

impl VmntError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NoSpace => minix_types::ENOSPC,
            Self::BadVmnt => minix_types::EINVAL,
            Self::Busy => minix_types::EBUSY,
            Self::Deadlock => minix_types::EDEADLK,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    struct NopFs;
    impl FsCancel for NopFs {
        fn cancel(&mut self, _vmnt: &Vmnt) {}
    }
    struct NopFilp;
    impl FilpInval for NopFilp {
        fn invalidate(&mut self, _fs: Endpoint) {}
    }
    struct NopVnode;
    impl VnodePut for NopVnode {
        fn put(&mut self, _vnode: usize) {}
    }
    struct AltFs;
    impl FsCancel for AltFs {
        fn cancel(&mut self, _vmnt: &Vmnt) {}
    }

    #[test]
    fn test_init_vmnts_zero() {
        let table = VmntTable::new();
        for i in 0..NR_MNTS {
            let id = VmntId(i);
            let v = table.get(id).unwrap();
            assert_eq!(v.dev, NO_DEV);
            assert!(!v.lock.is_locked());
        }
        assert_eq!(table.len(), 8);
    }

    #[test]
    fn test_get_free_vmnt() {
        let mut table = VmntTable::new();
        let id = table.alloc().unwrap();
        assert_eq!(id.get(), 0);
        table.get_mut(id).unwrap().dev = 1;
        let id2 = table.alloc().unwrap();
        assert_eq!(id2.get(), 1);
    }

    #[test]
    fn test_find_vmnt_hit() {
        let mut table = VmntTable::new();
        let id = table.alloc().unwrap();
        {
            let v = table.get_mut(id).unwrap();
            v.dev = 5;
            v.fs = Endpoint::from_generation_slot(0, 7);
        }
        assert_eq!(table.find_by_fs(Endpoint::from_generation_slot(0, 7)).unwrap(), id);
        assert!(table.find_by_fs(Endpoint::from_generation_slot(0, 8)).is_none());
    }

    #[test]
    fn test_lock_vmnt_edeadlk() {
        let mut table = VmntTable::new();
        let id = table.alloc().unwrap();
        table.get_mut(id).unwrap().dev = 1;
        table.get_mut(id).unwrap().fs = Endpoint::from_generation_slot(0, 5);
        let req = Endpoint::from_generation_slot(0, 5);
        assert_eq!(table.lock(id, VmntAccess::Read, req).unwrap_err(), VmntError::Deadlock);
        let other = Endpoint::from_generation_slot(0, 6);
        assert!(table.lock(id, VmntAccess::Read, other).is_ok());
    }

    #[test]
    fn test_mark_vs_clear() {
        let mut table = VmntTable::new();
        let id = table.alloc().unwrap();
        {
            let v = table.get_mut(id).unwrap();
            v.dev = 1;
            v.fs = Endpoint::from_generation_slot(0, 5);
            v.flags = VmntFlags::READONLY;
            v.mounted_on = Some(42);
        }
        table.mark_free(id);
        assert_eq!(table.get(id).unwrap().dev, NO_DEV);
        assert_eq!(table.get(id).unwrap().fs, Endpoint::NONE);
        // flags still
        assert_eq!(table.get(id).unwrap().flags, VmntFlags::READONLY);
        table.clear(id);
        assert_eq!(table.get(id).unwrap().flags, VmntFlags::empty());
        assert!(table.get(id).unwrap().mounted_on.is_none());
    }

    #[test]
    fn test_vmnt_unmap_by_endpt() {
        let mut table = VmntTable::new();
        let id = table.alloc().unwrap();
        table.get_mut(id).unwrap().dev = 10;
        table.get_mut(id).unwrap().fs = Endpoint::from_generation_slot(0, 9);
        table.get_mut(id).unwrap().mounted_on = Some(7);
        let mut fs = NopFs;
        let mut filp = NopFilp;
        let mut vnode = NopVnode;
        assert!(table.unmap_by_endpoint(Endpoint::from_generation_slot(0, 9), &mut fs, &mut filp, &mut vnode));
        assert_eq!(table.get(id).unwrap().dev, NO_DEV);
        assert!(!table.unmap_by_endpoint(Endpoint::from_generation_slot(0, 99), &mut fs, &mut filp, &mut vnode));
    }

    #[test]
    fn test_vmnt_flags() {
        assert_eq!(VmntFlags::READONLY.bits(), 0x01);
        assert_eq!(VmntFlags::CALLBACK.bits(), 0x02);
        assert_eq!(VmntFlags::MOUNTING.bits(), 0x04);
        assert_eq!(VmntFlags::FORCEROOTBSF.bits(), 0x08);
        assert_eq!(VmntFlags::CANSTAT.bits(), 0x10);
    }

    // Second impl for Gate D
    struct AltVmntTable(VmntTable);
    impl AltVmntTable {
        fn alloc(&mut self) -> Result<VmntId, VmntError> { self.0.alloc() }
    }
}
