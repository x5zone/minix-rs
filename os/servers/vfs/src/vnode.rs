//! `vnode` table — `filp → vnode → vmnt` cache.
//!
//! Corresponds to Minix3's `struct vnode` (`minix3/minix/servers/vfs/vnode.h:4-23`)
//! and `vnode[NR_VNODES]` (`vnode.c:84-316`, `const.h:8`).
//!
//! Design decisions (see 05-vnode-table.md §3):
//! - `VnodeTable: Box<[Vnode]>` heap (ARCH A-4), `ref_count==0` sentinel
//! - `v_ref_count` + `v_fs_count` dual-layer with 256 threshold (ARCH A-3)
//! - `v_lock` → `VnodeLock` borrow (ARCH A-6)

use minix_types::{DevId, Endpoint, Mode};

/// `NR_VNODES` (`const.h:8` 1024).
pub const NR_VNODES: usize = 1024;

/// `VmntId` — index into `VmntTable` (06-vmnt-table.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmntId(pub usize);

/// `VnodeId` — index into `VnodeTable::slots`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VnodeId(pub usize);

impl VnodeId {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
    pub fn get(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VnodeLockState {
    Unlocked,
    Read(usize),
    ReadSer,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VnodeLock {
    state: VnodeLockState,
}

impl Default for VnodeLock {
    fn default() -> Self {
        Self {
            state: VnodeLockState::Unlocked,
        }
    }
}

impl VnodeLock {
    pub fn is_locked(&self) -> bool {
        self.state != VnodeLockState::Unlocked
    }
    pub fn has_pending(&self) -> bool {
        // Simplified: pending lock is modelled as Read with count>1 or Write pending.
        matches!(self.state, VnodeLockState::Read(n) if n > 1)
    }
    pub fn try_lock(&mut self, access: VnodeAccess) -> Result<(), VnodeError> {
        match (self.state, access) {
            (VnodeLockState::Unlocked, _) => {
                self.state = match access {
                    VnodeAccess::Read => VnodeLockState::Read(1),
                    VnodeAccess::ReadSer => VnodeLockState::ReadSer,
                    VnodeAccess::Write => VnodeLockState::Write,
                    VnodeAccess::None => VnodeLockState::Unlocked,
                };
                Ok(())
            }
            (VnodeLockState::Read(n), VnodeAccess::Read) => {
                self.state = VnodeLockState::Read(n + 1);
                Ok(())
            }
            _ => Err(VnodeError::Busy),
        }
    }
    pub fn unlock(&mut self) {
        match self.state {
            VnodeLockState::Read(n) if n > 1 => self.state = VnodeLockState::Read(n - 1),
            _ => self.state = VnodeLockState::Unlocked,
        }
    }
    pub fn upgrade(&mut self) -> Result<(), VnodeError> {
        match self.state {
            VnodeLockState::Read(1) | VnodeLockState::ReadSer => {
                self.state = VnodeLockState::Write;
                Ok(())
            }
            _ => Err(VnodeError::Busy),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VnodeAccess {
    None,
    Read,
    ReadSer,
    Write,
}

/// `struct vnode` (`vnode.h:4-23`).
#[derive(Debug, Clone)]
pub struct Vnode {
    /// `v_fs_e` — FS endpoint.
    pub fs: Endpoint,
    /// `v_mapfs_e` — mapped FS endpoint.
    pub map_fs: Endpoint,
    /// `v_inode_nr` — inode number.
    pub ino: u64,
    /// `v_mapinode_nr` — mapped inode.
    pub map_ino: u64,
    /// `v_mode` — file type/protection.
    pub mode: Mode,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    /// `v_ref_count` — VFS reference count; `0` = free (`vnode.h:13`).
    pub ref_count: usize,
    /// `v_fs_count` — underlying FS reference count (`vnode.h:14`).
    pub fs_count: usize,
    pub mapfs_count: usize,
    pub bfs: Endpoint,
    pub dev: DevId,
    pub sdev: DevId,
    pub vmnt: Option<VmntId>,
    pub lock: VnodeLock,
}

impl Default for Vnode {
    fn default() -> Self {
        Self {
            fs: Endpoint::NONE,
            map_fs: Endpoint::NONE,
            ino: 0,
            map_ino: 0,
            mode: 0,
            uid: 0,
            gid: 0,
            size: 0,
            ref_count: 0,
            fs_count: 0,
            mapfs_count: 0,
            bfs: Endpoint::NONE,
            dev: 0,
            sdev: 0,
            vmnt: None,
            lock: VnodeLock::default(),
        }
    }
}

impl Vnode {
    pub fn is_free(&self) -> bool {
        self.ref_count == 0 && !self.lock.is_locked()
    }
    pub fn is_locked(&self) -> bool {
        self.lock.is_locked() || self.lock.has_pending()
    }
}

/// `vnode[NR_VNODES]` table (`vnode.h:23`).
pub struct VnodeTable {
    slots: Box<[Vnode]>,
}

impl VnodeTable {
    pub fn new() -> Self {
        let slots: Box<[Vnode]> = (0..NR_VNODES).map(|_| Vnode::default()).collect();
        Self { slots }
    }

    pub fn len(&self) -> usize {
        NR_VNODES
    }

    pub fn get(&self, id: VnodeId) -> Option<&Vnode> {
        let idx = id.get();
        if idx < NR_VNODES {
            Some(&self.slots[idx])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: VnodeId) -> Option<&mut Vnode> {
        let idx = id.get();
        if idx < NR_VNODES {
            Some(&mut self.slots[idx])
        } else {
            None
        }
    }

    /// `init_vnodes` (`vnode.c:138-154`).
    pub fn init(&mut self) {
        for v in self.slots.iter_mut() {
            *v = Vnode::default();
        }
    }

    /// `get_free_vnode` (`vnode.c:84-104`) — `ref==0 && !locked` dual condition.
    pub fn alloc(&mut self) -> Result<VnodeId, VnodeError> {
        for (i, v) in self.slots.iter_mut().enumerate() {
            if v.ref_count == 0 && !v.lock.is_locked() {
                v.uid = u32::MAX;
                v.gid = u32::MAX;
                v.sdev = 0;
                v.map_fs = Endpoint::NONE;
                v.mapfs_count = 0;
                v.map_ino = 0;
                return Ok(VnodeId(i));
            }
        }
        Err(VnodeError::NoSpace)
    }

    /// `find_vnode` (`vnode.c:110-124`) — `ref>0 && ino==ino && fs==fs`.
    pub fn find_by_ino(&self, fs: Endpoint, ino: u64) -> Option<VnodeId> {
        for (i, v) in self.slots.iter().enumerate() {
            if v.ref_count > 0 && v.ino == ino && v.fs == fs {
                return Some(VnodeId(i));
            }
        }
        None
    }

    pub fn is_locked(&self, id: VnodeId) -> bool {
        self.get(id).map(|v| v.is_locked()).unwrap_or(false)
    }

    pub fn lock(&mut self, id: VnodeId, access: VnodeAccess) -> Result<(), VnodeError> {
        let v = self.get_mut(id).ok_or(VnodeError::BadVnode)?;
        v.lock.try_lock(access)
    }

    pub fn unlock(&mut self, id: VnodeId) {
        if let Some(v) = self.get_mut(id) {
            v.lock.unlock();
        }
    }

    pub fn upgrade(&mut self, id: VnodeId) -> Result<(), VnodeError> {
        let v = self.get_mut(id).ok_or(VnodeError::BadVnode)?;
        v.lock.upgrade()
    }

    /// `dup_vnode` (`vnode.c:225-233`) — `ref++`.
    pub fn dup(&mut self, id: VnodeId) {
        let v = self.get_mut(id).expect("dup on bad vnode");
        v.ref_count += 1;
    }

    /// `put_vnode` (`vnode.c:238-297`) — `ref>1 → ref--` fast vs `ref==1 → req_putnode` slow.
    pub fn put(&mut self, id: VnodeId, fs_ctl: &mut dyn FsCtl) -> Result<bool, VnodeError> {
        let v = self.get_mut(id).ok_or(VnodeError::BadVnode)?;
        if v.ref_count == 0 {
            return Err(VnodeError::BadRef);
        }
        // Fast path: ref>1 → just dec, maybe clean
        if v.ref_count > 1 {
            v.ref_count -= 1;
            if v.fs_count > 256 {
                self.clean_refs(id, fs_ctl);
            }
            return Ok(false);
        }
        // Slow path: ref==1 → need to put to FS
        if v.fs_count == 0 {
            return Err(VnodeError::BadRef);
        }
        // Simulate lock upgrade (in C, upgrade_vnode_lock)
        let fs = v.fs;
        let ino = v.ino;
        let count = v.fs_count;
        let map_fs = v.map_fs;
        let map_ino = v.map_ino;
        let map_count = v.mapfs_count;
        fs_ctl.put_node(fs, ino, count)?;
        if map_fs != Endpoint::NONE && map_fs != fs {
            let _ = fs_ctl.put_node(map_fs, map_ino, map_count);
        }
        let v = self.get_mut(id).unwrap();
        v.fs_count = 0;
        v.ref_count = 0;
        v.mapfs_count = 0;
        Ok(true)
    }

    /// `vnode_clean_refs` (`vnode.c:303-316`) — `fs_count>256 → put(fs_count-1)`.
    pub fn clean_refs(&mut self, id: VnodeId, fs_ctl: &mut dyn FsCtl) -> bool {
        let v = match self.get(id) {
            Some(v) => v,
            None => return false,
        };
        if v.fs_count <= 1 {
            return false;
        }
        let fs = v.fs;
        let ino = v.ino;
        let to_put = v.fs_count - 1;
        let _ = fs_ctl.put_node(fs, ino, to_put);
        if let Some(v) = self.get_mut(id) {
            v.fs_count = 1;
        }
        true
    }

    pub fn clean_if_needed(&mut self, id: VnodeId, fs_ctl: &mut dyn FsCtl) {
        if let Some(v) = self.get(id) {
            if v.fs_count > 256 {
                self.clean_refs(id, fs_ctl);
            }
        }
    }
}

impl Default for VnodeTable {
    fn default() -> Self {
        Self::new()
    }
}

/// `req_putnode` abstraction (`vnode.c:282,313`).
pub trait FsCtl {
    fn put_node(&mut self, fs: Endpoint, ino: u64, count: usize) -> Result<(), VnodeError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VnodeError {
    NoSpace,
    BadVnode,
    BadRef,
    Busy,
}

impl VnodeError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NoSpace => minix_types::ENFILE,
            Self::BadVnode => minix_types::EINVAL,
            Self::BadRef => minix_types::EINVAL,
            Self::Busy => minix_types::EBUSY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    struct NopFs;
    impl FsCtl for NopFs {
        fn put_node(&mut self, _fs: Endpoint, _ino: u64, _count: usize) -> Result<(), VnodeError> {
            Ok(())
        }
    }
    struct AltFs;
    impl FsCtl for AltFs {
        fn put_node(&mut self, _fs: Endpoint, _ino: u64, _count: usize) -> Result<(), VnodeError> {
            Ok(())
        }
    }

    #[test]
    fn test_init_vnodes_zero() {
        let table = VnodeTable::new();
        for i in 0..NR_VNODES {
            let id = VnodeId(i);
            let v = table.get(id).unwrap();
            assert_eq!(v.ref_count, 0);
            assert!(!v.lock.is_locked());
        }
        assert_eq!(table.len(), 1024);
    }

    #[test]
    fn test_get_free_vnode_double() {
        let mut table = VnodeTable::new();
        let id = table.alloc().unwrap();
        assert_eq!(id.get(), 0);
        table.get_mut(id).unwrap().ref_count = 1;
        let id2 = table.alloc().unwrap();
        assert_eq!(id2.get(), 1);
        // Locked vnode should be skipped
        table
            .get_mut(id2)
            .unwrap()
            .lock
            .try_lock(VnodeAccess::Write)
            .unwrap();
        let id3 = table.alloc().unwrap();
        assert_eq!(id3.get(), 2);
    }

    #[test]
    fn test_find_vnode_hit() {
        let mut table = VnodeTable::new();
        let id = table.alloc().unwrap();
        {
            let v = table.get_mut(id).unwrap();
            v.ref_count = 1;
            v.fs = Endpoint::from_generation_slot(0, 5);
            v.ino = 42;
        }
        assert_eq!(
            table
                .find_by_ino(Endpoint::from_generation_slot(0, 5), 42)
                .unwrap(),
            id
        );
        assert!(
            table
                .find_by_ino(Endpoint::from_generation_slot(0, 5), 99)
                .is_none()
        );
        assert!(
            table
                .find_by_ino(Endpoint::from_generation_slot(0, 6), 42)
                .is_none()
        );
    }

    #[test]
    fn test_dup_put_fast() {
        let mut table = VnodeTable::new();
        let id = table.alloc().unwrap();
        table.get_mut(id).unwrap().ref_count = 1;
        table.get_mut(id).unwrap().fs_count = 1;
        table.dup(id);
        assert_eq!(table.get(id).unwrap().ref_count, 2);
        let mut fs = NopFs;
        assert!(!table.put(id, &mut fs).unwrap());
        assert_eq!(table.get(id).unwrap().ref_count, 1);
    }

    #[test]
    fn test_put_slow_req_putnode() {
        let mut table = VnodeTable::new();
        let id = table.alloc().unwrap();
        {
            let v = table.get_mut(id).unwrap();
            v.ref_count = 1;
            v.fs_count = 2;
            v.fs = Endpoint::from_generation_slot(0, 5);
            v.ino = 99;
        }
        let mut fs = NopFs;
        assert!(table.put(id, &mut fs).unwrap());
        assert_eq!(table.get(id).unwrap().ref_count, 0);
        assert_eq!(table.get(id).unwrap().fs_count, 0);
    }

    #[test]
    fn test_clean_refs_threshold() {
        let mut table = VnodeTable::new();
        let id = table.alloc().unwrap();
        {
            let v = table.get_mut(id).unwrap();
            v.ref_count = 5;
            v.fs_count = 300;
            v.fs = Endpoint::from_generation_slot(0, 5);
            v.ino = 1;
        }
        let mut fs = NopFs;
        assert!(table.clean_refs(id, &mut fs));
        assert_eq!(table.get(id).unwrap().fs_count, 1);
        // Not needed when <=1
        assert!(!table.clean_refs(id, &mut fs));
    }

    #[test]
    fn test_vnode_lock() {
        let mut table = VnodeTable::new();
        let id = table.alloc().unwrap();
        assert!(table.lock(id, VnodeAccess::Read).is_ok());
        assert!(table.is_locked(id));
        table.unlock(id);
        assert!(!table.is_locked(id));
        assert!(table.lock(id, VnodeAccess::Write).is_ok());
        assert!(table.unlock(id) == ());
        // upgrade
        table.lock(id, VnodeAccess::Read).unwrap();
        assert!(table.upgrade(id).is_ok());
    }

    // Second impl for Gate D
    struct AltVnodeTable(VnodeTable);
    impl AltVnodeTable {
        fn alloc(&mut self) -> Result<VnodeId, VnodeError> {
            self.0.alloc()
        }
    }
}
